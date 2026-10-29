//! The registry checks that back content findings (docs/composition-documents.md
//! §3.4). A check is `{claim, steps: [{command, args, expect: [{path, op,
//! value}]}]}`: each step runs a read-only `bundle.*` or `plan.*` command on
//! a scratch session holding a clone of the model, and each `expect` row
//! compares a JSON-Pointer into the result. The scratch session keeps the
//! caller's session, artifacts and undo history untouched and makes a replay
//! independent of any session state.

use serde_json::{json, Value};

use super::content::{Entry, OPS};
use super::document::canonical_json;
use super::Composition;
use crate::search_plan::OpenPlan;
use crate::session::Session;

/// Most steps a check may have: a check is a loop of registry queries, and a
/// document from outside must not be able to ask for unbounded work.
pub const MAX_STEPS: usize = 16;

/// The queries a check may run: reads of the model, and nothing that signs, writes a document or
/// edits. `bundle.receipt` signs with the local receipt key and `bundle.export` writes a document,
/// so a check from outside cannot run them; the test `every_bundle_and_plan_query_is_either_a_read_or_named_here`
/// makes a new query a decision rather than a default.
pub const READS: [&str; 20] = [
    "bundle.instances",
    "bundle.scenarios",
    "bundle.ontology",
    "bundle.state",
    "bundle.validate",
    "bundle.candidates",
    "bundle.verdicts",
    "bundle.flatten",
    "bundle.incidence",
    "bundle.run",
    "bundle.boundaries",
    "bundle.game",
    "bundle.lens",
    "plan.net",
    "plan.opening",
    "plan.pieces",
    "plan.state",
    "plan.check",
    "plan.slot_candidates",
    "plan.game",
];
/// Queries in the same namespaces that a check may not run.
pub const NOT_READS: [&str; 2] = ["bundle.receipt", "bundle.export"];

/// Whether the check runner may run `id`: a read of the model (a registered query, `journal: false`,
/// in [`READS`]).
pub fn command_allowed(id: &str) -> std::result::Result<(), String> {
    if !(id.starts_with("bundle.") || id.starts_with("plan.")) {
        return Err(format!(
            "command {id:?} is not allowed: a check runs only bundle.* and plan.* queries"
        ));
    }
    let spec = crate::commands::ALL
        .iter()
        .flat_map(|set| set.iter())
        .find(|c| c.id == id)
        .ok_or_else(|| format!("command {id:?} is not a registry command"))?;
    if spec.journal {
        return Err(format!(
            "command {id:?} is not allowed: it edits (journal: true), and a check only reads"
        ));
    }
    if !READS.contains(&id) {
        return Err(format!(
            "command {id:?} is not allowed: it is a query, but not a read of the model (it signs or writes a document); a check may run {READS:?}"
        ));
    }
    Ok(())
}

/// What is wrong with a check's shape, as `(code, message)` each: `CHECK_COMMAND_FORBIDDEN` for a
/// step whose command may not run, `CHECK_INVALID` for the rest (no registry command is run).
pub fn check_issues(check: &Value) -> Vec<(&'static str, String)> {
    const INVALID: &str = "CHECK_INVALID";
    let mut out: Vec<(&'static str, String)> = vec![];
    let Some(steps) = check.get("steps").and_then(Value::as_array) else {
        return vec![(
            INVALID,
            "a check needs steps: [{command, args, expect}]".into(),
        )];
    };
    if steps.is_empty() {
        out.push((INVALID, "a check needs at least one step".into()));
    }
    if steps.len() > MAX_STEPS {
        out.push((
            INVALID,
            format!(
                "a check has {} steps; at most {MAX_STEPS} are run",
                steps.len()
            ),
        ));
    }
    for (i, st) in steps.iter().enumerate() {
        match st.get("command").and_then(Value::as_str) {
            None => out.push((INVALID, format!("step {i}: command must be a string"))),
            Some(c) => {
                if let Err(e) = command_allowed(c) {
                    out.push(("CHECK_COMMAND_FORBIDDEN", format!("step {i}: {e}")));
                }
            }
        }
        if st
            .get("args")
            .is_some_and(|a| !a.is_object() && !a.is_null())
        {
            out.push((INVALID, format!("step {i}: args must be an object")));
        }
        let rows = st.get("expect").and_then(Value::as_array);
        if rows.is_none_or(|r| r.is_empty()) {
            out.push((INVALID, format!("step {i}: expect needs at least one row")));
        }
        for (j, row) in rows.into_iter().flatten().enumerate() {
            let path = row.get("path").and_then(Value::as_str);
            if !path.is_some_and(|p| p.is_empty() || p.starts_with('/')) {
                out.push((
                    INVALID,
                    format!("step {i} expect {j}: path must be a JSON Pointer ('' or '/…')"),
                ));
            }
            let op = row.get("op").and_then(Value::as_str).unwrap_or("");
            if !OPS.contains(&op) {
                out.push((
                    INVALID,
                    format!("step {i} expect {j}: op {op:?}; there are {OPS:?}"),
                ));
            } else if !["exists", "absent"].contains(&op) && row.get("value").is_none() {
                out.push((
                    INVALID,
                    format!("step {i} expect {j}: op {op:?} needs a value"),
                ));
            }
        }
    }
    out
}

/// [`check_issues`] without the codes: one message each.
pub fn check_problems(check: &Value) -> Vec<String> {
    check_issues(check).into_iter().map(|(_, m)| m).collect()
}

/// RFC 6901 lookup.
pub fn pointer<'a>(doc: &'a Value, path: &str) -> Option<&'a Value> {
    doc.pointer(path)
}

/// Whether `actual` (None: the pointer found nothing) satisfies `op value`.
pub fn holds(op: &str, actual: Option<&Value>, value: Option<&Value>) -> bool {
    match op {
        "exists" => actual.is_some(),
        "absent" => actual.is_none(),
        _ => {
            let (Some(a), Some(v)) = (actual, value) else {
                return false;
            };
            match op {
                "==" => a == v,
                "!=" => a != v,
                "contains" => match a {
                    Value::String(s) => v.as_str().is_some_and(|x| s.contains(x)),
                    Value::Array(xs) => xs.contains(v),
                    _ => false,
                },
                "<" | "<=" | ">" | ">=" => match (a.as_f64(), v.as_f64()) {
                    (Some(x), Some(y)) => match op {
                        "<" => x < y,
                        "<=" => x <= y,
                        ">" => x > y,
                        _ => x >= y,
                    },
                    _ => false,
                },
                _ => false,
            }
        }
    }
}

/// What a check runs against.
pub enum Base<'a> {
    Composition(&'a Composition),
    /// The plan open in the caller's session, if any; the scratch session
    /// opens the library plan when there is none.
    Plan(Option<&'a OpenPlan>),
}

fn scratch(base: &Base) -> Result<Session, String> {
    let mut s = Session::default();
    match base {
        Base::Composition(c) => s.composition = Some((*c).clone()),
        Base::Plan(Some(p)) => s.search_plan = Some((*p).clone()),
        Base::Plan(None) => {
            s.execute("plan.open", json!({}))
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(s)
}

/// Runs the check of `entry` and reports every step and row.
pub fn run_check(base: &Base, entry: &Entry) -> Value {
    let Some(check) = &entry.check else {
        return json!({"id": entry.id, "ok": false, "claim": "", "error": "the entry has no check", "steps": []});
    };
    let claim = check.get("claim").and_then(Value::as_str).unwrap_or("");
    let problems = check_problems(check);
    if !problems.is_empty() {
        return json!({"id": entry.id, "ok": false, "claim": claim, "error": problems.join("; "), "steps": []});
    }
    let mut sess = match scratch(base) {
        Ok(s) => s,
        Err(e) => {
            return json!({"id": entry.id, "ok": false, "claim": claim, "error": e, "steps": []})
        }
    };
    let mut all_ok = true;
    let mut steps = vec![];
    for st in check["steps"].as_array().into_iter().flatten() {
        let command = st["command"].as_str().unwrap_or_default();
        let args = st
            .get("args")
            .cloned()
            .filter(|a| !a.is_null())
            .unwrap_or(json!({}));
        let mut step = json!({"command": command, "args": args});
        match sess.execute(command, args) {
            Err(e) => {
                step["ok"] = json!(false);
                step["error"] = json!(e.to_string());
                step["expect"] = json!([]);
                all_ok = false;
            }
            Ok(result) => {
                let mut rows = vec![];
                let mut step_ok = true;
                for row in st["expect"].as_array().into_iter().flatten() {
                    let path = row["path"].as_str().unwrap_or_default();
                    let op = row["op"].as_str().unwrap_or_default();
                    let actual = pointer(&result, path);
                    let ok = holds(op, actual, row.get("value"));
                    step_ok &= ok;
                    rows.push(json!({
                        "path": path, "op": op, "value": row.get("value").cloned().unwrap_or(Value::Null),
                        "actual": actual.cloned().unwrap_or(Value::Null),
                        "found": actual.is_some(), "ok": ok,
                    }));
                }
                step["ok"] = json!(step_ok);
                step["expect"] = json!(rows);
                all_ok &= step_ok;
            }
        }
        steps.push(step);
    }
    json!({"id": entry.id, "ok": all_ok, "claim": claim, "steps": steps})
}

/// `sha256:` over the canonical text of a check result: what a check
/// receipt signs. Artifact ids and nothing else volatile are absent from it.
pub fn result_hash(result: &Value) -> String {
    crate::provenance::hash_result(
        &serde_json::from_str(&canonical_json(result)).unwrap_or(Value::Null),
    )
}

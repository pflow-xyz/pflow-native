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

/// Whether the check runner may run `id`: a registered query (`journal:
/// false`) in the `bundle.` or `plan.` namespace.
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
    Ok(())
}

/// What is wrong with a check's shape, one message each (no registry
/// command is run).
pub fn check_problems(check: &Value) -> Vec<String> {
    let mut out = vec![];
    let Some(steps) = check.get("steps").and_then(Value::as_array) else {
        return vec!["a check needs steps: [{command, args, expect}]".into()];
    };
    if steps.is_empty() {
        out.push("a check needs at least one step".into());
    }
    if steps.len() > MAX_STEPS {
        out.push(format!(
            "a check has {} steps; at most {MAX_STEPS} are run",
            steps.len()
        ));
    }
    for (i, st) in steps.iter().enumerate() {
        match st.get("command").and_then(Value::as_str) {
            None => out.push(format!("step {i}: command must be a string")),
            Some(c) => {
                if let Err(e) = command_allowed(c) {
                    out.push(format!("step {i}: {e}"));
                }
            }
        }
        if st
            .get("args")
            .is_some_and(|a| !a.is_object() && !a.is_null())
        {
            out.push(format!("step {i}: args must be an object"));
        }
        let rows = st.get("expect").and_then(Value::as_array);
        if rows.is_none_or(|r| r.is_empty()) {
            out.push(format!("step {i}: expect needs at least one row"));
        }
        for (j, row) in rows.into_iter().flatten().enumerate() {
            let path = row.get("path").and_then(Value::as_str);
            if !path.is_some_and(|p| p.is_empty() || p.starts_with('/')) {
                out.push(format!(
                    "step {i} expect {j}: path must be a JSON Pointer ('' or '/…')"
                ));
            }
            let op = row.get("op").and_then(Value::as_str).unwrap_or("");
            if !OPS.contains(&op) {
                out.push(format!("step {i} expect {j}: op {op:?}; there are {OPS:?}"));
            } else if !["exists", "absent"].contains(&op) && row.get("value").is_none() {
                out.push(format!("step {i} expect {j}: op {op:?} needs a value"));
            }
        }
    }
    out
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

//! Model identity: which CID names a model. The sealing contract is
//! [`crate::cid`], unchanged; this module decides only what is sealed.
//!
//! * A **net** (Shape A editor JSON-LD, coordinates included) is a model. Its
//!   model CID is the CID it already has: nothing is re-minted.
//! * A **composition** is its own document, `@type` `PetriNetBundle` (the
//!   go-pflow `metamodel.Bundle` shape, pflow-compose's [`Bundle`]). Each
//!   subnet is inline (`model`, for parts with no net CID) or a stored net by
//!   CID (`cid`, a plain string: [`crate::cid`] refuses a non-top-level
//!   `@id`). It is sealed as written, so changing a link changes only the
//!   composition's CID; each part keeps its own.
//! * A composition that is a **bare wrapper** (one by-reference part and
//!   nothing else) *is* that net: its model CID is the part's CID.
//! * A **plan** is a separate document, `@type` `SearchPlan`, that names its
//!   model by CID; the model never names its plans.
//!
//! Arrays under a term that is not an `@list` container hash as unordered
//! sets (see [`crate::cid`]), so anything ordered that rides in a sealed
//! document, a subnet's recipe `params` or a plan's `plan`, is embedded as
//! canonical JSON *text* ([`canonical_json`]) and hashed exactly as given.
//!
//! The same rule is implemented by pflow.xyz's `internal/model` (Go) and
//! `public/model-cid.mjs` (JS); `tests/fixtures/model/` holds their shared
//! vectors (pflow-xyz `parity/model/`), checked by `tests/model.rs`. This is
//! not a classifier: what a store calls a document's kind is the store's.
//!
//! [`Bundle`]: pflow_compose::Bundle

use std::collections::HashSet;

use serde_json::{json, Map, Value};

use crate::{cid, Error, Result};

/// `@type` of a composition.
pub const TYPE_COMPOSITION: &str = "PetriNetBundle";
/// `@type` of a search plan.
pub const TYPE_PLAN: &str = "SearchPlan";

/// What the rule says about one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelId {
    /// The model CID.
    pub cid: String,
    /// The CID of the document exactly as given ([`cid::seal`]). Equal to
    /// `cid` unless the document is a bare wrapper.
    pub sealed: String,
    /// True when `cid` is a part's CID: the document is a bare wrapper.
    pub reduced: bool,
    /// The by-reference part CIDs of a composition, in document order.
    pub parts: Vec<String>,
}

fn refuse(code: &str, msg: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("[{code}] {msg}"))
}

/// Has the shape of a pflow CID (`z` and 20 to 80 base58 characters).
pub fn is_cid(s: &str) -> bool {
    const B58: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    let Some(rest) = s.strip_prefix('z') else {
        return false;
    };
    (20..=80).contains(&rest.chars().count()) && rest.chars().all(|c| B58.contains(c))
}

fn type_of(doc: &Value) -> &str {
    doc.get("@type").and_then(Value::as_str).unwrap_or("")
}

/// The structural check of a composition or plan; a net passes unexamined.
/// Refusals read `[CODE] message` with the codes pflow.xyz uses:
/// `E_NO_SUBNETS`, `E_SUBNET`, `E_PART`, `E_PATTERN`, `E_LINK`,
/// `E_BUNDLE_SHAPE` (go-pflow's other `PetriNetBundle`, which joins ports by
/// alias) and `E_PLAN`.
pub fn check(doc: &Value) -> Result<()> {
    match type_of(doc) {
        TYPE_COMPOSITION => check_composition(doc),
        TYPE_PLAN => check_plan(doc),
        _ => Ok(()),
    }
}

fn check_composition(doc: &Value) -> Result<()> {
    let subs = doc
        .get("subnets")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    if subs.is_empty() {
        return Err(refuse(
            "E_NO_SUBNETS",
            "a composition needs at least one subnet",
        ));
    }
    let mut ids = HashSet::new();
    for (i, s) in subs.iter().enumerate() {
        let Some(m) = s.as_object() else {
            return Err(refuse("E_SUBNET", format!("subnets[{i}] is not an object")));
        };
        let id = m.get("id").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            return Err(refuse("E_SUBNET", format!("subnets[{i}] has no id")));
        }
        if !ids.insert(id.to_string()) {
            return Err(refuse(
                "E_SUBNET",
                format!("subnet id {id:?} appears twice"),
            ));
        }
        match (m.get("cid"), m.get("model")) {
            (Some(_), Some(_)) => {
                return Err(refuse(
                    "E_PART",
                    format!("subnet {id:?} has both cid and model: a part is referenced or inline, not both"),
                ))
            }
            (None, None) => {
                return Err(refuse(
                    "E_PART",
                    format!("subnet {id:?} has neither cid nor model: a pattern recipe alone does not pin a net"),
                ))
            }
            (Some(c), None) => {
                if !c.as_str().is_some_and(is_cid) {
                    return Err(refuse(
                        "E_PART",
                        format!("subnet {id:?}: cid {c} is not a CID"),
                    ));
                }
            }
            (None, Some(model)) => {
                if !model.is_object() {
                    return Err(refuse(
                        "E_PART",
                        format!("subnet {id:?}: model is not an object"),
                    ));
                }
            }
        }
        if let Some(p) = m.get("pattern") {
            if !p.as_str().is_some_and(|s| !s.is_empty()) {
                return Err(refuse(
                    "E_PATTERN",
                    format!("subnet {id:?}: pattern must be a non-empty string"),
                ));
            }
        }
        if let Some(p) = m.get("params") {
            let Some(text) = p.as_str() else {
                return Err(refuse(
                    "E_PATTERN",
                    format!("subnet {id:?}: params must be a canonical-JSON string (arrays inside a sealed document hash as sets, so ordered parameters are embedded as text)"),
                ));
            };
            if !serde_json::from_str::<Value>(text).is_ok_and(|v| v.is_object()) {
                return Err(refuse(
                    "E_PATTERN",
                    format!("subnet {id:?}: params is not a JSON object"),
                ));
            }
        }
    }
    if let Some(links) = doc.get("links").and_then(Value::as_array) {
        for (i, l) in links.iter().enumerate() {
            let Some(m) = l.as_object() else {
                return Err(refuse("E_LINK", format!("links[{i}] is not an object")));
            };
            for k in ["from_subnet", "from_port", "to_subnet", "to_port"] {
                if m.contains_key(k) {
                    return Err(refuse(
                        "E_BUNDLE_SHAPE",
                        format!("links[{i}] uses {k:?}: that is go-pflow's tokenmodel/subnet bundle, not metamodel.Bundle (links are {{kind, from:{{subnet,…}}, to:{{subnet,…}}}})"),
                    ));
                }
            }
            if m.get("kind")
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
            {
                return Err(refuse("E_LINK", format!("links[{i}] has no kind")));
            }
            for side in ["from", "to"] {
                let Some(e) = m.get(side).and_then(Value::as_object) else {
                    return Err(refuse(
                        "E_LINK",
                        format!("links[{i}].{side} is not an endpoint object"),
                    ));
                };
                let sn = e.get("subnet").and_then(Value::as_str).unwrap_or("");
                if !ids.contains(sn) {
                    return Err(refuse(
                        "E_LINK",
                        format!("links[{i}].{side} names subnet {sn:?}, which is not a part"),
                    ));
                }
            }
        }
    }
    Ok(())
}

fn check_plan(doc: &Value) -> Result<()> {
    if !doc
        .get("model_cid")
        .and_then(Value::as_str)
        .is_some_and(is_cid)
    {
        return Err(refuse(
            "E_PLAN",
            "model_cid must be the CID of the model the plan is for",
        ));
    }
    let Some(text) = doc.get("plan").and_then(Value::as_str) else {
        return Err(refuse(
            "E_PLAN",
            "plan must be the plan's canonical JSON as a string (its lists are ordered; a nested array would hash as a set)",
        ));
    };
    if !serde_json::from_str::<Value>(text).is_ok_and(|v| v.is_object()) {
        return Err(refuse("E_PLAN", "plan is not a JSON object"));
    }
    Ok(())
}

/// The by-reference part CIDs of a composition, in document order.
pub fn parts(doc: &Value) -> Vec<String> {
    doc.get("subnets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.get("cid").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

const BARE_TOP: &[&str] = &[
    "@context",
    "@id",
    "@type",
    "subnets",
    "namespace",
    "arc_merge",
    "links",
    "constraints",
];
const BARE_SUBNET: &[&str] = &["@type", "id", "net_type", "cid"];

/// The CID of the net a composition is nothing but, if it is. Precisely: one
/// subnet whose only keys are `@type`, `id`, `net_type` and `cid`; no links
/// and no constraints (absent or empty); no name, version, description,
/// parents or any other key. Flatten returns such a bundle's lone model
/// unchanged, so the wrapper adds nothing to the net's identity. A name, a
/// constraint or a recipe says more, so those stay compositions.
pub fn bare_wrapper(doc: &Value) -> Option<String> {
    if type_of(doc) != TYPE_COMPOSITION {
        return None;
    }
    let top = doc.as_object()?;
    for (k, v) in top {
        if !BARE_TOP.contains(&k.as_str()) {
            return None;
        }
        if (k == "links" || k == "constraints") && !v.as_array().is_some_and(Vec::is_empty) {
            return None;
        }
    }
    let subs = top.get("subnets")?.as_array()?;
    if subs.len() != 1 {
        return None;
    }
    let part = subs[0].as_object()?;
    if part.keys().any(|k| !BARE_SUBNET.contains(&k.as_str())) {
        return None;
    }
    part.get("cid")?
        .as_str()
        .filter(|c| is_cid(c))
        .map(str::to_string)
}

/// Apply the rule to a parsed document. A net is never validated or changed
/// (its model CID is [`cid::seal`]); a malformed composition or plan is
/// refused with [`check`]'s `[CODE] message`.
pub fn model_id(doc: &Value) -> Result<ModelId> {
    check(doc)?;
    let sealed = cid::seal(doc)?;
    let mut id = ModelId {
        cid: sealed.clone(),
        sealed,
        reduced: false,
        parts: vec![],
    };
    if type_of(doc) == TYPE_COMPOSITION {
        id.parts = parts(doc);
        if let Some(part) = bare_wrapper(doc) {
            id.cid = part;
            id.reduced = true;
        }
    }
    Ok(id)
}

/// The model CID alone.
pub fn model_cid(doc: &Value) -> Result<String> {
    Ok(model_id(doc)?.cid)
}

/// Canonical JSON text, the form embedded as a subnet's `params` or a plan's
/// `plan`: object keys sorted by code point, lists in order, no whitespace,
/// numbers as doubles in `JSON.stringify`'s shortest form, strings escaped
/// only for `"`, `\` and controls below 0x20. A sealed string is hashed as
/// given and never re-canonicalised. Numbers beyond ±2^53, and ones that need
/// an exponent, are not spelled alike by every implementation: avoid them.
pub fn canonical_json(text: &str) -> Result<String> {
    let v: Value = serde_json::from_str(text)
        .map_err(|e| Error::Invalid(format!("canonical json: not JSON: {e}")))?;
    let mut out = String::new();
    write_canonical(&mut out, &v);
    Ok(out)
}

/// [`canonical_json`] of a value that is already parsed: what a writer
/// emits when it builds the text a sealed document carries (a subnet's
/// `params`, a check). A *received* string is never passed through this: it
/// is hashed as given.
pub fn canonical_value(v: &Value) -> String {
    let mut out = String::new();
    write_canonical(&mut out, v);
    out
}

fn write_canonical(out: &mut String, v: &Value) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&es6_number(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => write_string(out, s),
        Value::Array(a) => {
            out.push('[');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(out, e);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort(); // byte order of UTF-8 is code-point order
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(out, k);
                out.push(':');
                write_canonical(out, &m[k]);
            }
            out.push('}');
        }
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `JSON.stringify` for a finite double: the shortest round-trip digits,
/// without an exponent between 1e-6 and 1e21, with one outside it.
fn es6_number(f: f64) -> String {
    if f == 0.0 {
        return "0".into();
    }
    let a = f.abs();
    if (1e-6..1e21).contains(&a) {
        return format!("{f}");
    }
    let s = format!("{f:e}");
    match s.split_once('e') {
        Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
        _ => s,
    }
}

/// The sealed document for a search plan on a model: `{"@context", "@type":
/// "SearchPlan", "model_cid", "plan"}`, where `plan` is the plan's canonical
/// JSON text (`plan_json` is the `SearchPlan` as pflow-rs accepts it). A plan
/// never changes its model's CID; the same plan on another model is another
/// document.
pub fn new_plan(model_cid: &str, plan_json: &str) -> Result<Value> {
    if !is_cid(model_cid) {
        return Err(refuse(
            "E_PLAN",
            format!("model_cid {model_cid:?} is not a CID"),
        ));
    }
    let text = canonical_json(plan_json).map_err(|e| refuse("E_PLAN", e))?;
    let mut doc = Map::new();
    doc.insert("@context".into(), json!(cid::PFLOW_CONTEXT_URL));
    doc.insert("@type".into(), json!(TYPE_PLAN));
    doc.insert("model_cid".into(), json!(model_cid));
    doc.insert("plan".into(), json!(text));
    let doc = Value::Object(doc);
    check_plan(&doc)?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    const COFFEE: &str = "z4EBG9jDsuUdvVN4bUXRHxoDG3p274FfhcJd8BiruRcE1jLCHi8";

    #[test]
    fn numbers_are_spelled_like_json_stringify() {
        for (f, want) in [
            (1.0, "1"),
            (0.5, "0.5"),
            (-3.0, "-3"),
            (100.0, "100"),
            (0.0, "0"),
            (-0.0, "0"),
            (1e21, "1e+21"),
            (1.5e-7, "1.5e-7"),
            (123456789012.0, "123456789012"),
        ] {
            assert_eq!(es6_number(f), want, "{f}");
        }
    }

    #[test]
    fn cid_shape() {
        assert!(is_cid(COFFEE));
        assert!(!is_cid("z"));
        assert!(!is_cid("not-a-cid"));
        assert!(!is_cid(&format!("{COFFEE}0"))); // 0 is not base58
    }

    #[test]
    fn a_bare_wrapper_is_its_part() {
        let wrap = |extra: Value| {
            let mut d = json!({
                "@context": cid::PFLOW_CONTEXT_URL, "@type": TYPE_COMPOSITION,
                "subnets": [{"@type": "PetriNet", "id": "a", "cid": COFFEE}],
            });
            d.as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            d
        };
        assert_eq!(bare_wrapper(&wrap(json!({}))).as_deref(), Some(COFFEE));
        assert_eq!(
            bare_wrapper(&wrap(
                json!({"links": [], "constraints": [], "namespace": false})
            ))
            .as_deref(),
            Some(COFFEE)
        );
        for more in [
            json!({"name": "x"}),
            json!({"description": "x"}),
            json!({"parents": [COFFEE]}),
            json!({"constraints": [{"id": "c"}]}),
        ] {
            assert_eq!(bare_wrapper(&wrap(more.clone())), None, "{more}");
        }
    }
}

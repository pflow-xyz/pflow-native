//! The model library: named documents kept in the session beside the open
//! one, for systems (bindings between models), diffs and composition.
//! "current" (or "") always means the open document. Phase 7 makes the
//! library a persistent, content-addressed store.

use serde_json::{json, Value};

use crate::doc::Document;
use crate::registry::{always, arg, b, opt, s, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "model.save",
        label: "Save to Library…",
        help: "Keep a copy of the open net in the model library under `name`.",
        menu: "Models",
        shortcut: None,
        args: &[arg("name", Str, "library name"), opt("overwrite", Bool, "replace an existing entry")],
        enabled: always,
        run: save,
        journal: true,
    },
    CommandSpec {
        id: "model.list",
        label: "List Models",
        help: "Names in the model library with their sizes.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: list,
        journal: false,
    },
    CommandSpec {
        id: "model.get",
        label: "Get Model",
        help: "A library model (or \"current\") as pflow.xyz JSON-LD.",
        menu: "",
        shortcut: None,
        args: &[arg("name", Str, "library name or \"current\"")],
        enabled: always,
        run: get,
        journal: false,
    },
    CommandSpec {
        id: "model.open",
        label: "Open from Library…",
        help: "Make a library model the open net (undoable; the previous net is not saved to the library).",
        menu: "Models",
        shortcut: None,
        args: &[arg("name", Str, "library name")],
        enabled: always,
        run: open,
        journal: true,
    },
    CommandSpec {
        id: "model.import",
        label: "Import into Library…",
        help: "Add a model to the library from a file, inline JSON, or a bundled example, without opening it.",
        menu: "Models",
        shortcut: None,
        args: &[
            arg("name", Str, "library name"),
            opt("path", Str, "pflow JSON file"),
            opt("json", Json, "pflow JSON document (object or string)"),
            opt("example", Str, "bundled example name"),
            opt("overwrite", Bool, "replace an existing entry"),
        ],
        enabled: always,
        run: import,
        journal: true,
    },
    CommandSpec {
        id: "model.delete",
        label: "Delete from Library",
        help: "Remove a model from the library.",
        menu: "",
        shortcut: None,
        args: &[arg("name", Str, "library name")],
        enabled: always,
        run: delete,
        journal: true,
    },
];

/// The document `name` refers to: "current"/"" is the open net, anything
/// else a library entry.
pub fn resolve(sess: &Session, name: &str) -> Result<Document> {
    match name {
        "" | "current" => Ok(sess.doc.clone()),
        n => {
            sess.models.get(n).cloned().ok_or_else(|| {
                Error::NotFound(format!("no model {n:?} in the library (model.list)"))
            })
        }
    }
}

fn check_name(sess: &Session, name: &str, overwrite: bool) -> Result<()> {
    if name.is_empty() || name == "current" || name.chars().any(char::is_control) {
        return Err(Error::BadArgs(format!("invalid model name {name:?}")));
    }
    if !overwrite && sess.models.contains_key(name) {
        return Err(Error::Invalid(format!(
            "model {name:?} exists (pass overwrite: true)"
        )));
    }
    Ok(())
}

fn summary(name: &str, d: &Document) -> Value {
    json!({
        "name": name,
        "places": d.places.len(),
        "transitions": d.transitions.len(),
        "arcs": d.arcs.len(),
        "title": d.extra.get("name"),
    })
}

fn save(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    check_name(sess, name, b(a, "overwrite").unwrap_or(false))?;
    sess.models.insert(name.to_string(), sess.doc.clone());
    Ok(summary(name, &sess.doc))
}

fn list(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(sess
        .models
        .iter()
        .map(|(n, d)| summary(n, d))
        .collect::<Vec<_>>()))
}

fn get(sess: &mut Session, a: &Value) -> Result<Value> {
    Ok(resolve(sess, s(a, "name").unwrap())?.to_value())
}

fn open(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    let doc = resolve(sess, name)?;
    let out = summary(name, &doc);
    super::file::replace(sess, doc)?;
    sess.path = None;
    Ok(out)
}

fn import(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    check_name(sess, name, b(a, "overwrite").unwrap_or(false))?;
    let doc = match (
        s(a, "path"),
        a.get("json").filter(|v| !v.is_null()),
        s(a, "example"),
    ) {
        (Some(p), None, None) => {
            let data = std::fs::read(p).map_err(|e| Error::Io(format!("{p}: {e}")))?;
            Document::from_json(&data)?
        }
        (None, Some(Value::String(t)), None) => Document::from_json(t.as_bytes())?,
        (None, Some(v), None) => Document::from_value(v)?,
        (None, None, Some(ex)) => {
            let (_, src) = super::file::EXAMPLES
                .iter()
                .find(|(n, _)| *n == ex)
                .ok_or_else(|| Error::NotFound(format!("no example {ex:?}")))?;
            Document::from_json(src.as_bytes())?
        }
        _ => {
            return Err(Error::BadArgs(
                "give exactly one of path, json or example".into(),
            ))
        }
    };
    let out = summary(name, &doc);
    sess.models.insert(name.to_string(), doc);
    Ok(out)
}

fn delete(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    sess.models
        .remove(name)
        .map(|_| json!({ "deleted": name }))
        .ok_or_else(|| Error::NotFound(format!("no model {name:?}")))
}

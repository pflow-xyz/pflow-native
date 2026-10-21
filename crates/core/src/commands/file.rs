use serde_json::{json, Value};

use crate::doc::Document;
use crate::registry::{always, arg, opt, s, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

/// The pflow.xyz landing-page gallery (fetched by CID from pflow.xyz/o/),
/// plus the SIR model used by the tests.
pub const EXAMPLES: &[(&str, &str)] = &[
    (
        "coffee-shop",
        include_str!("../../../../examples/coffee-shop.json"),
    ),
    (
        "tic-tac-toe",
        include_str!("../../../../examples/tic-tac-toe.json"),
    ),
    (
        "enzyme-kinetics",
        include_str!("../../../../examples/enzyme-kinetics.json"),
    ),
    (
        "predator-prey",
        include_str!("../../../../examples/predator-prey.json"),
    ),
    (
        "producer-consumer",
        include_str!("../../../../examples/producer-consumer.json"),
    ),
    (
        "usdc-bridge",
        include_str!("../../../../examples/usdc-bridge.json"),
    ),
    ("sir", include_str!("../../../../examples/sir.json")),
];

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "file.new",
        label: "New",
        help: "Replace the document with an empty net.",
        menu: "File",
        shortcut: Some("Cmd+N"),
        args: &[],
        enabled: always,
        run: new,
        journal: true,
    },
    CommandSpec {
        id: "file.open",
        label: "Open…",
        help: "Load a pflow.xyz JSON(-LD) file from disk.",
        menu: "File",
        shortcut: Some("Cmd+O"),
        args: &[arg("path", Str, "file to read")],
        enabled: always,
        run: open,
        journal: true,
    },
    CommandSpec {
        id: "file.open_example",
        label: "Open Example",
        help: "Load a bundled example net.",
        menu: "",
        shortcut: None,
        args: &[arg("name", Str, "example name (see file.examples)")],
        enabled: always,
        run: open_example,
        journal: true,
    },
    CommandSpec {
        id: "file.examples",
        label: "List Examples",
        help: "Names of the bundled example nets.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: examples,
        journal: false,
    },
    CommandSpec {
        id: "file.save",
        label: "Save",
        help: "Write the document as pflow.xyz JSON-LD to `path`, or to the file it came from.",
        menu: "File",
        shortcut: Some("Cmd+S"),
        args: &[opt(
            "path",
            Str,
            "file to write; defaults to the current path",
        )],
        enabled: always,
        run: save,
        journal: false,
    },
    CommandSpec {
        id: "file.load_json",
        label: "Load JSON",
        help: "Replace the document with a pflow.xyz JSON document (object or string). Undoable.",
        menu: "",
        shortcut: None,
        args: &[
            arg("json", Json, "the document, as a JSON object or a string"),
            opt(
                "live",
                Bool,
                "an edit of the current document (JSON pane): keep view, results and the marking of surviving places",
            ),
        ],
        enabled: always,
        run: load_json,
        journal: true,
    },
    CommandSpec {
        id: "file.export_json",
        label: "Export JSON",
        help: "The document as pflow.xyz JSON-LD.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: export_json,
        journal: false,
    },
];

/// Replaces the whole document, as one undoable edit.
pub(crate) fn replace(s: &mut Session, doc: Document) -> Result<()> {
    s.edit("replace document", |s| {
        let mut doc = doc;
        doc.normalize_colors();
        s.doc = doc;
        s.selection.clear();
        s.rates.retain(|t, _| s.doc.transition(t).is_some());
        Ok(())
    })?;
    s.reset_marking();
    s.artifacts.clear();
    super::view::fit_view(s);
    Ok(())
}

fn summary(s: &Session) -> Value {
    json!({
        "path": s.path,
        "places": s.doc.places.len(),
        "transitions": s.doc.transitions.len(),
        "arcs": s.doc.arcs.len(),
        "colors": s.doc.token,
    })
}

fn new(s: &mut Session, _: &Value) -> Result<Value> {
    replace(s, Document::default())?;
    s.path = None;
    s.saved_doc = s.doc.clone();
    Ok(summary(s))
}

fn open(sess: &mut Session, a: &Value) -> Result<Value> {
    let path = s(a, "path").expect("required");
    let data = std::fs::read(path).map_err(|e| Error::Io(format!("{path}: {e}")))?;
    let doc = Document::from_json(&data)?;
    replace(sess, doc)?;
    sess.path = Some(path.to_string());
    sess.saved_doc = sess.doc.clone();
    Ok(summary(sess))
}

fn open_example(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").expect("required");
    let (_, src) = EXAMPLES
        .iter()
        .find(|(n, _)| *n == name)
        .ok_or_else(|| Error::NotFound(format!("no example {name:?}")))?;
    replace(sess, Document::from_json(src.as_bytes())?)?;
    sess.path = None;
    sess.saved_doc = sess.doc.clone();
    Ok(summary(sess))
}

fn examples(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(EXAMPLES.iter().map(|(n, _)| *n).collect::<Vec<_>>()))
}

fn save(sess: &mut Session, a: &Value) -> Result<Value> {
    let path = match s(a, "path") {
        Some(p) => p.to_string(),
        None => sess
            .path
            .clone()
            .ok_or_else(|| Error::BadArgs("no path: the document was never saved".into()))?,
    };
    std::fs::write(&path, sess.doc.to_json_pretty() + "\n")
        .map_err(|e| Error::Io(format!("{path}: {e}")))?;
    sess.path = Some(path.clone());
    sess.saved_doc = sess.doc.clone();
    Ok(json!({ "path": path }))
}

fn load_json(sess: &mut Session, a: &Value) -> Result<Value> {
    let doc = match &a["json"] {
        Value::String(t) => Document::from_json(t.as_bytes())?,
        v => Document::from_value(v)?,
    };
    if a.get("live").and_then(Value::as_bool).unwrap_or(false) {
        // A source edit: same document, so keep the view, the results and
        // the token game where the places still exist.
        let marking = sess.marking.clone();
        sess.edit("edit JSON", |s| {
            let mut doc = doc;
            doc.normalize_colors();
            s.doc = doc;
            s.rates.retain(|t, _| s.doc.transition(t).is_some());
            Ok(())
        })?;
        sess.reset_marking();
        let n = sess.doc.colors();
        for (id, v) in marking {
            if let Some(slot) = sess.marking.get_mut(&id) {
                if v.len() == n {
                    *slot = v;
                }
            }
        }
        return Ok(summary(sess));
    }
    replace(sess, doc)?;
    Ok(summary(sess))
}

fn export_json(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(sess.doc.to_value())
}

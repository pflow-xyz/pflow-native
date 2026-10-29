use serde_json::{json, Value};

use crate::doc::Document;
use crate::registry::{always, arg, b, opt, s, ArgType::*, CommandSpec};
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
        help: "Write the document as pflow.xyz JSON-LD to `path`, or to the file it came from. \
               Session rates (set, fitted or calibrated) go into each transition's `rate` field \
               unless rates: false; opening the file reads them back.",
        menu: "File",
        shortcut: Some("Cmd+S"),
        args: &[
            opt("path", Str, "file to write; defaults to the current path"),
            opt("rates", Bool, "write session rates as transition `rate` fields (default true)"),
        ],
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
        help: "The document as pflow.xyz JSON-LD (with rates: true, session rates as transition `rate` fields).",
        menu: "",
        shortcut: None,
        args: &[opt("rates", Bool, "include session rates (default false)")],
        enabled: always,
        run: export_json,
        journal: false,
    },
];

/// Replaces the whole document, as one undoable edit.
/// Moves numeric transition `rate` fields out of the document into a rate
/// map (they are session rates: the editor keeps them beside the net, and
/// `file.save` writes them back).
pub(crate) fn take_rates(doc: &mut Document) -> std::collections::BTreeMap<String, f64> {
    let mut out = std::collections::BTreeMap::new();
    for t in &mut doc.transitions {
        if let Some(r) = t.extra.get("rate").and_then(Value::as_f64) {
            if r.is_finite() && r >= 0.0 {
                t.extra.shift_remove("rate");
                out.insert(t.id.clone(), r);
            }
        }
    }
    out
}

/// The document with the session's rates written into its transitions'
/// `rate` fields (what `file.save` writes; pflow.xyz ignores the field, and
/// `take_rates` reads it back on open).
pub fn with_rates(doc: &Document, rates: &std::collections::BTreeMap<String, f64>) -> Document {
    let mut d = doc.clone();
    for t in &mut d.transitions {
        if let Some(r) = rates.get(&t.id) {
            t.extra.insert("rate".into(), serde_json::json!(r));
        }
    }
    d
}

pub(crate) fn replace(s: &mut Session, doc: Document) -> Result<()> {
    let mut doc = doc;
    let file_rates = take_rates(&mut doc);
    // Rates that arrive with the document undo with it.
    s.edit_with_rates("replace document", |s| {
        doc.normalize_colors();
        s.doc = doc;
        s.selection.clear();
        s.rates.retain(|t, _| s.doc.transition(t).is_some());
        s.rates.extend(file_rates);
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
    // A composition document is not a net: opened as one it would be an empty net with no hint of
    // why. Say where it opens.
    if let Ok(v) = serde_json::from_slice::<Value>(&data) {
        let ty = v.get("@type").and_then(Value::as_str).unwrap_or_default();
        if ["PetriNetBundle", "PatternComposition", "ModelContent"].contains(&ty) {
            let how = if ty == "ModelContent" {
                "content.load {document} attaches it to the open composition"
            } else {
                "bundle.load {document} opens it as a composition (Compose → Load Composition…)"
            };
            return Err(Error::Invalid(format!(
                "[DOCUMENT_TYPE] {path} is a {ty}, not a net; {how}"
            )));
        }
    }
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
    let doc = if b(a, "rates").unwrap_or(true) {
        with_rates(&sess.doc, &sess.rates)
    } else {
        sess.doc.clone()
    };
    std::fs::write(&path, doc.to_json_pretty() + "\n")
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
        let mut doc = doc;
        let file_rates = take_rates(&mut doc);
        sess.edit_with_rates("edit JSON", |s| {
            doc.normalize_colors();
            s.doc = doc;
            s.rates.retain(|t, _| s.doc.transition(t).is_some());
            s.rates.extend(file_rates);
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

fn export_json(sess: &mut Session, a: &Value) -> Result<Value> {
    if b(a, "rates").unwrap_or(false) {
        Ok(with_rates(&sess.doc, &sess.rates).to_value())
    } else {
        Ok(sess.doc.to_value())
    }
}

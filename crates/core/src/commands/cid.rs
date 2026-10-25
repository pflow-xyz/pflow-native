//! Content identifiers: pflow.xyz's CID sealing (see [`crate::cid`]).

use serde_json::{json, Value};

use crate::registry::{always, b, opt, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::Result;

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "file.cid",
        label: "Content ID",
        help: "The document's pflow.xyz CID (URDNA2015 → sha2-256 → CIDv1 base58btc), ignoring any stored @id. \
               `sealed` says whether the stored @id equals it.",
        menu: "",
        shortcut: None,
        args: &[opt(
            "nquads",
            Bool,
            "also return the canonical N-Quads that are hashed",
        )],
        enabled: always,
        run: cid,
        journal: false,
    },
    CommandSpec {
        id: "file.seal",
        label: "Seal (Set @id to CID)",
        help: "Compute the CID and store it as the document's top-level @id, as pflow.xyz does. \
               Idempotent; undoable.",
        menu: "File",
        shortcut: None,
        args: &[],
        enabled: always,
        run: seal,
        journal: true,
    },
];

fn stored_id(sess: &Session) -> Option<&str> {
    sess.doc.extra.get("@id").and_then(Value::as_str)
}

fn cid(sess: &mut Session, a: &Value) -> Result<Value> {
    let doc = sess.export_doc().to_value();
    let cid = crate::cid::seal(&doc)?;
    let mut out = json!({ "cid": cid, "sealed": stored_id(sess) == Some(cid.as_str()) });
    if b(a, "nquads").unwrap_or(false) {
        out["nquads"] = json!(crate::cid::nquads(&doc)?);
    }
    Ok(out)
}

fn seal(sess: &mut Session, _: &Value) -> Result<Value> {
    let cid = crate::cid::seal(&sess.export_doc().to_value())?;
    if stored_id(sess) == Some(cid.as_str()) {
        return Ok(json!({ "cid": cid, "changed": false }));
    }
    sess.edit("seal", |s| {
        s.doc.extra.insert("@id".into(), json!(cid));
        Ok(json!({ "cid": cid, "changed": true }))
    })
}

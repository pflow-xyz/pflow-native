//! Document-level edits: rename a node, name/description, token colors.

use serde_json::{json, Value};

use crate::registry::{always, arg, opt, req, s, strs, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "net.rename",
        label: "Rename Node",
        help: "Change a node's id, rewriting its arcs, marking and rate.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "current id"), arg("new_id", Str, "new id")],
        enabled: always,
        run: rename,
        journal: true,
    },
    CommandSpec {
        id: "net.set_details",
        label: "Edit Details",
        help: "Set the document's name and/or description (pflow.xyz \"Edit Details\").",
        menu: "",
        shortcut: None,
        args: &[opt("name", Str, "diagram name"), opt("description", Str, "description")],
        enabled: always,
        run: set_details,
        journal: true,
    },
    CommandSpec {
        id: "net.set_colors",
        label: "Set Token Colors",
        help: "Replace the token color list (`token[]`): names like \"red\", pflow.xyz token URLs or #rrggbb. \
               Per-color vectors are kept by position, padded with 0 (weights with 1 for the first color) or truncated.",
        menu: "",
        shortcut: None,
        args: &[arg("token", StrList, "color names in order; [] = single uncolored token")],
        enabled: always,
        run: set_colors,
        journal: true,
    },
];

fn rename(sess: &mut Session, a: &Value) -> Result<Value> {
    let (id, new_id) = (req(a, "id").to_string(), req(a, "new_id").to_string());
    if sess.doc.kind_of(&id).is_none() {
        return Err(Error::NotFound(format!("no node {id:?}")));
    }
    if new_id.is_empty() || new_id.chars().any(char::is_whitespace) {
        return Err(Error::BadArgs(format!("invalid id {new_id:?}")));
    }
    if new_id == id {
        return Ok(json!({ "id": id }));
    }
    if sess.doc.kind_of(&new_id).is_some() {
        return Err(Error::Invalid(format!("id {new_id:?} already exists")));
    }
    sess.edit("rename", |s| {
        if let Some(p) = s.doc.place_mut(&id) {
            p.id = new_id.clone();
        } else if let Some(t) = s.doc.transition_mut(&id) {
            t.id = new_id.clone();
        }
        for arc in &mut s.doc.arcs {
            if arc.source == id {
                arc.source = new_id.clone();
            }
            if arc.target == id {
                arc.target = new_id.clone();
            }
        }
        if let Some(m) = s.marking.remove(&id) {
            s.marking.insert(new_id.clone(), m);
        }
        if let Some(r) = s.rates.remove(&id) {
            s.rates.insert(new_id.clone(), r);
        }
        for sel in &mut s.selection {
            if *sel == id {
                *sel = new_id.clone();
            }
        }
        Ok(json!({ "id": new_id }))
    })
}

fn set_details(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").map(str::to_string);
    let desc = s(a, "description").map(str::to_string);
    sess.edit("details", |s| {
        for (k, v) in [("name", name), ("description", desc)] {
            match v {
                Some(t) if t.is_empty() => {
                    s.doc.extra.shift_remove(k);
                }
                Some(t) => {
                    s.doc.extra.insert(k.into(), json!(t));
                }
                None => {}
            }
        }
        Ok(json!({ "name": s.doc.extra.get("name"), "description": s.doc.extra.get("description") }))
    })
}

/// pflow.xyz writes token colors as `https://pflow.xyz/tokens/<name>`.
fn token_url(c: &str) -> String {
    if c.contains("://") || c.starts_with('#') {
        c.to_string()
    } else {
        format!("https://pflow.xyz/tokens/{c}")
    }
}

fn set_colors(sess: &mut Session, a: &Value) -> Result<Value> {
    let token: Vec<String> = strs(a, "token")
        .unwrap()
        .iter()
        .map(|c| token_url(c))
        .collect();
    let n = token.len().max(1);
    sess.edit("colors", |s| {
        s.doc.token = token.clone();
        for p in &mut s.doc.places {
            p.initial.resize(n, 0.0);
            p.capacity.resize(n, 0.0);
        }
        for arc in &mut s.doc.arcs {
            arc.weight.resize(n, 0.0);
            if arc.weight.iter().all(|w| *w == 0.0) {
                arc.weight[0] = 1.0;
            }
        }
        s.reset_marking();
        Ok(json!({ "token": token }))
    })
}

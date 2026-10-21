use serde_json::{json, Value};

use crate::registry::{always, b, opt, strs, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "edit.undo",
        label: "Undo",
        help: "Undo the last edit or firing.",
        menu: "Edit",
        shortcut: Some("Cmd+Z"),
        args: &[],
        enabled: can_undo,
        run: undo,
        journal: true,
    },
    CommandSpec {
        id: "edit.redo",
        label: "Redo",
        help: "Redo the last undone step.",
        menu: "Edit",
        shortcut: Some("Cmd+Shift+Z"),
        args: &[],
        enabled: can_redo,
        run: redo,
        journal: true,
    },
    CommandSpec {
        id: "edit.select",
        label: "Select",
        help: "Select nodes by id (replacing the selection unless add=true).",
        menu: "",
        shortcut: None,
        args: &[
            opt("ids", StrList, "node ids"),
            opt("add", Bool, "toggle these into the current selection"),
        ],
        enabled: always,
        run: select,
        journal: true,
    },
    CommandSpec {
        id: "edit.select_all",
        label: "Select All",
        help: "Select every place and transition.",
        menu: "Edit",
        shortcut: Some("Cmd+A"),
        args: &[],
        enabled: always,
        run: select_all,
        journal: true,
    },
    CommandSpec {
        id: "edit.select_none",
        label: "Select None",
        help: "Clear the selection.",
        menu: "Edit",
        shortcut: Some("Escape"),
        args: &[],
        enabled: always,
        run: select_none,
        journal: true,
    },
    CommandSpec {
        id: "edit.delete",
        label: "Delete",
        help: "Delete nodes (default: the selection) and every arc touching them.",
        menu: "Edit",
        shortcut: Some("Delete"),
        args: &[opt("ids", StrList, "node ids; defaults to the selection")],
        enabled: always,
        run: delete,
        journal: true,
    },
];

fn can_undo(s: &Session) -> std::result::Result<(), String> {
    s.can_undo()
        .then_some(())
        .ok_or_else(|| "nothing to undo".into())
}

fn can_redo(s: &Session) -> std::result::Result<(), String> {
    s.can_redo()
        .then_some(())
        .ok_or_else(|| "nothing to redo".into())
}

fn undo(s: &mut Session, _: &Value) -> Result<Value> {
    let snap = s
        .undo
        .pop()
        .ok_or_else(|| Error::Invalid("nothing to undo".into()))?;
    let label = snap.label.clone();
    let cur = s.snapshot(&label);
    s.redo.push(cur);
    s.restore(snap);
    Ok(json!({ "undone": label }))
}

fn redo(s: &mut Session, _: &Value) -> Result<Value> {
    let snap = s
        .redo
        .pop()
        .ok_or_else(|| Error::Invalid("nothing to redo".into()))?;
    let label = snap.label.clone();
    let cur = s.snapshot(&label);
    s.undo.push(cur);
    s.restore(snap);
    Ok(json!({ "redone": label }))
}

fn select(s: &mut Session, a: &Value) -> Result<Value> {
    let ids = strs(a, "ids").unwrap_or_default();
    for id in &ids {
        if s.doc.kind_of(id).is_none() {
            return Err(Error::NotFound(format!("no node {id:?}")));
        }
    }
    if b(a, "add").unwrap_or(false) {
        for id in ids {
            if let Some(i) = s.selection.iter().position(|x| *x == id) {
                s.selection.remove(i);
            } else {
                s.selection.push(id);
            }
        }
    } else {
        s.selection = ids;
    }
    Ok(json!(s.selection))
}

fn select_all(s: &mut Session, _: &Value) -> Result<Value> {
    s.selection = s
        .doc
        .places
        .iter()
        .map(|p| p.id.clone())
        .chain(s.doc.transitions.iter().map(|t| t.id.clone()))
        .collect();
    Ok(json!(s.selection))
}

fn select_none(s: &mut Session, _: &Value) -> Result<Value> {
    s.selection.clear();
    Ok(json!([]))
}

pub(crate) fn delete(s: &mut Session, a: &Value) -> Result<Value> {
    let ids = strs(a, "ids").unwrap_or_else(|| s.selection.clone());
    if ids.is_empty() {
        return Err(Error::BadArgs("nothing selected to delete".into()));
    }
    for id in &ids {
        if s.doc.kind_of(id).is_none() {
            return Err(Error::NotFound(format!("no node {id:?}")));
        }
    }
    s.edit("delete", |s| {
        let before = s.doc.arcs.len();
        s.doc.places.retain(|p| !ids.contains(&p.id));
        s.doc.transitions.retain(|t| !ids.contains(&t.id));
        s.doc
            .arcs
            .retain(|a| !ids.contains(&a.source) && !ids.contains(&a.target));
        for id in &ids {
            s.marking.remove(id);
            s.rates.remove(id);
        }
        Ok(json!({ "deleted": ids, "arcs_removed": before - s.doc.arcs.len() }))
    })
}

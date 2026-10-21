//! Copy / cut / paste / duplicate. pflow.xyz has none of these; the
//! clipboard is a sub-document (the copied nodes and the arcs among them)
//! held in the session, so agents get the same behavior.

use serde_json::{json, Value};

use crate::doc::Document;
use crate::registry::{n, opt, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "edit.copy",
        label: "Copy",
        help: "Copy the selected nodes and the arcs between them.",
        menu: "Edit",
        shortcut: Some("Cmd+C"),
        args: &[],
        enabled: has_selection,
        run: copy,
        journal: true,
    },
    CommandSpec {
        id: "edit.cut",
        label: "Cut",
        help: "Copy the selection, then delete it.",
        menu: "Edit",
        shortcut: Some("Cmd+X"),
        args: &[],
        enabled: has_selection,
        run: cut,
        journal: true,
    },
    CommandSpec {
        id: "edit.paste",
        label: "Paste",
        help: "Paste the clipboard offset by (dx, dy) with fresh ids; the pasted nodes become the selection.",
        menu: "Edit",
        shortcut: Some("Cmd+V"),
        args: &[opt("dx", Num, "x offset (default 40)"), opt("dy", Num, "y offset (default 40)")],
        enabled: has_clipboard,
        run: paste,
        journal: true,
    },
    CommandSpec {
        id: "edit.duplicate",
        label: "Duplicate",
        help: "Copy and paste the selection in one step.",
        menu: "Edit",
        shortcut: Some("Cmd+D"),
        args: &[],
        enabled: has_selection,
        run: duplicate,
        journal: true,
    },
];

fn has_selection(s: &Session) -> std::result::Result<(), String> {
    (!s.selection.is_empty())
        .then_some(())
        .ok_or_else(|| "nothing selected".into())
}

fn has_clipboard(s: &Session) -> std::result::Result<(), String> {
    s.clipboard
        .is_some()
        .then_some(())
        .ok_or_else(|| "clipboard is empty".into())
}

fn copy(sess: &mut Session, _: &Value) -> Result<Value> {
    let sel = &sess.selection;
    let doc = &sess.doc;
    let clip = Document {
        token: doc.token.clone(),
        places: doc
            .places
            .iter()
            .filter(|p| sel.contains(&p.id))
            .cloned()
            .collect(),
        transitions: doc
            .transitions
            .iter()
            .filter(|t| sel.contains(&t.id))
            .cloned()
            .collect(),
        arcs: doc
            .arcs
            .iter()
            .filter(|a| sel.contains(&a.source) && sel.contains(&a.target))
            .cloned()
            .collect(),
        extra: Default::default(),
    };
    let out = json!({ "places": clip.places.len(), "transitions": clip.transitions.len(), "arcs": clip.arcs.len() });
    sess.clipboard = Some(clip);
    Ok(out)
}

fn cut(sess: &mut Session, a: &Value) -> Result<Value> {
    let out = copy(sess, a)?;
    // Directly, not via execute: a nested command would not get its own
    // undo point.
    super::edit::delete(sess, &json!({}))?;
    Ok(out)
}

fn paste(sess: &mut Session, a: &Value) -> Result<Value> {
    let clip = sess
        .clipboard
        .clone()
        .ok_or_else(|| Error::Invalid("clipboard is empty".into()))?;
    let (dx, dy) = (n(a, "dx").unwrap_or(40.0), n(a, "dy").unwrap_or(40.0));
    let ncol = sess.doc.colors();
    sess.edit("paste", |s| {
        let mut map = std::collections::HashMap::new();
        let mut new_ids = vec![];
        for p in &clip.places {
            let id = s.doc.fresh_id("p");
            map.insert(p.id.clone(), id.clone());
            let mut q = p.clone();
            q.id = id.clone();
            q.x += dx;
            q.y += dy;
            q.initial.resize(ncol, 0.0);
            q.capacity.resize(ncol, 0.0);
            s.marking.insert(
                id.clone(),
                q.initial.iter().map(|f| f.round() as i64).collect(),
            );
            s.doc.places.push(q);
            new_ids.push(id);
        }
        for t in &clip.transitions {
            let id = s.doc.fresh_id("t");
            map.insert(t.id.clone(), id.clone());
            let mut u = t.clone();
            u.id = id.clone();
            u.x += dx;
            u.y += dy;
            s.doc.transitions.push(u);
            new_ids.push(id);
        }
        for arc in &clip.arcs {
            let mut b = arc.clone();
            b.source = map[&arc.source].clone();
            b.target = map[&arc.target].clone();
            b.weight.resize(ncol, 0.0);
            s.doc.arcs.push(b);
        }
        s.selection = new_ids.clone();
        Ok(json!({ "pasted": new_ids }))
    })
}

fn duplicate(sess: &mut Session, _: &Value) -> Result<Value> {
    let saved = sess.clipboard.take();
    copy(sess, &Value::Null)?;
    let out = paste(sess, &json!({}));
    sess.clipboard = saved;
    out
}

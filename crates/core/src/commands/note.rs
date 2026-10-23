//! Shared context between the person at the window and an agent:
//! `view.focus` says what the person is looking at, and `note.*` lets the
//! agent (or the person) pin short notes to a node, a result or a point on
//! the canvas, which the app draws where they belong. Notes live in the
//! session, not the document, so they never reach a saved file.

use serde_json::{json, Value};

use crate::doc::NodeKind;
use crate::registry::{always, arg, b, n, opt, req, s, ArgType::*, CommandSpec};
use crate::session::{Focus, Note, NoteTarget, Session};
use crate::{Error, Result};

/// Longest note, in characters, and most notes at once.
pub const MAX_TEXT: usize = 500;
pub const MAX_NOTES: usize = 100;

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "view.focus",
        label: "What the User Sees",
        help: "What the person at the window is looking at, so an agent can talk about the same thing: the selected nodes \
               (with tokens, or enablement and rate), the result open in the Results tab, the bottom tab, the nodes inside \
               the visible canvas, the canvas highlight, whether the window is on screen, the notes, and the last commands. \
               Headless, only the session's own parts (selection, viewport, highlight, notes, recent commands).",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: focus,
        journal: false,
    },
    CommandSpec {
        id: "view.report_focus",
        label: "Report Focus",
        help: "Set by the app when what it shows changes: the open result, the bottom tab, whether the window is visible. \
               Agents read it with view.focus.",
        menu: "",
        shortcut: None,
        args: &[
            opt("artifact", Str, "the artifact open in the Results tab"),
            opt("tab", Str, "the bottom tab: results, workbench, run, models or console"),
            opt("bottom_panel", Bool, "whether the bottom panel is open"),
            opt("window_visible", Bool, "whether the window is on screen"),
        ],
        enabled: always,
        run: report_focus,
        journal: false,
    },
    CommandSpec {
        id: "note.add",
        label: "Add Note",
        help: "Pin a short note where the person will see it: on a node (`node`), on a result (`artifact`, shown above it \
               in the Results tab), at a canvas point (`x`, `y`), or on the net as a whole (none of these). The app draws \
               it as a sticky note; the Inspector lists every note with a delete button. Session-only: notes are never \
               saved into the document.",
        menu: "",
        shortcut: None,
        args: &[
            arg("text", Str, "the note (≤ 500 characters)"),
            opt("node", Str, "place or transition id"),
            opt("artifact", Str, "artifact id"),
            opt("x", Num, "canvas x (with y)"),
            opt("y", Num, "canvas y (with x)"),
            opt("author", Str, "who wrote it (default \"agent\")"),
        ],
        enabled: always,
        run: add,
        journal: false,
    },
    CommandSpec {
        id: "note.list",
        label: "Notes",
        help: "Every note, oldest first, with its target; `orphaned` when its node or result is gone.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: list,
        journal: false,
    },
    CommandSpec {
        id: "note.delete",
        label: "Delete Note",
        help: "Remove one note.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "note id")],
        enabled: always,
        run: delete,
        journal: false,
    },
    CommandSpec {
        id: "note.clear",
        label: "Clear Notes",
        help: "Remove every note (or, with `author`, every note by that author).",
        menu: "",
        shortcut: None,
        args: &[opt("author", Str, "only this author's notes")],
        enabled: always,
        run: clear,
        journal: false,
    },
];

fn report_focus(sess: &mut Session, a: &Value) -> Result<Value> {
    sess.focus = Focus {
        reported: true,
        artifact: s(a, "artifact").map(String::from),
        tab: s(a, "tab").map(String::from),
        bottom_panel: b(a, "bottom_panel").unwrap_or(true),
        window_visible: b(a, "window_visible").unwrap_or(true),
    };
    Ok(json!({ "ok": true }))
}

fn node_json(sess: &Session, id: &str, enabled: &[String]) -> Value {
    match sess.doc.kind_of(id) {
        Some(NodeKind::Place) => {
            let p = sess.doc.place(id).expect("a place");
            json!({
                "id": id, "kind": "place", "label": p.label,
                "tokens": sess.marking.get(id), "initial": p.initial,
            })
        }
        Some(NodeKind::Transition) => {
            let t = sess.doc.transition(id).expect("a transition");
            let enabled = enabled.iter().any(|t| t == id);
            json!({
                "id": id, "kind": "transition", "label": t.label,
                "enabled": enabled, "rate": sess.rates.get(id).copied().unwrap_or(1.0),
            })
        }
        None => json!({ "id": id, "kind": "missing" }),
    }
}

fn note_json(sess: &Session, nt: &Note) -> Value {
    let (target, orphaned) = match &nt.target {
        NoteTarget::Net => (json!({ "net": true }), false),
        NoteTarget::Node(id) => (json!({ "node": id }), sess.doc.kind_of(id).is_none()),
        NoteTarget::Artifact(id) => (json!({ "artifact": id }), sess.artifact(id).is_none()),
        NoteTarget::Point { x, y } => (json!({ "x": x, "y": y }), false),
    };
    let mut v = json!({ "id": nt.id, "text": nt.text, "author": nt.author, "target": target });
    if orphaned {
        v["orphaned"] = json!(true);
    }
    v
}

fn focus(sess: &mut Session, _: &Value) -> Result<Value> {
    let enabled = sess.enabled_transitions();
    let selection: Vec<Value> = sess
        .selection
        .iter()
        .map(|id| node_json(sess, id, &enabled))
        .collect();
    // Nodes inside the viewport (world coordinates of the canvas).
    let v = sess.view;
    let (hw, hh) = (v.width / v.zoom / 2.0, v.height / v.zoom / 2.0);
    let inside = |x: f64, y: f64| (x - v.cx).abs() <= hw && (y - v.cy).abs() <= hh;
    let mut visible: Vec<&str> = sess
        .doc
        .places
        .iter()
        .filter(|p| inside(p.x, p.y))
        .map(|p| p.id.as_str())
        .collect();
    visible.extend(
        sess.doc
            .transitions
            .iter()
            .filter(|t| inside(t.x, t.y))
            .map(|t| t.id.as_str()),
    );
    let total = sess.doc.places.len() + sess.doc.transitions.len();
    let open = sess
        .focus
        .artifact
        .as_deref()
        .and_then(|id| sess.artifact(id))
        .map(|a| {
            json!({
                "id": a.id, "title": a.title, "kind": a.data.kind(), "source": a.source.command,
                "notes": sess.notes.iter().filter(|n| n.target == NoteTarget::Artifact(a.id.clone())).count(),
            })
        });
    let recent: Vec<&str> = sess
        .journal
        .iter()
        .rev()
        .take(5)
        .map(|st| st.command.as_str())
        .collect();
    let mut out = json!({
        "front_end": if sess.focus.reported { "app" } else { "headless" },
        "net": sess.doc.extra.get("name").cloned().unwrap_or(Value::Null),
        "selection": selection,
        "viewport": {
            "center": [v.cx, v.cy], "zoom": v.zoom,
            "visible_nodes": if visible.len() <= 60 { json!(visible) } else { json!({"count": visible.len()}) },
            "all_visible": visible.len() == total,
        },
        "highlight": sess.highlight.as_ref().map(|h| json!({ "label": h.label, "nodes": h.nodes })),
        "notes": sess.notes.iter().map(|nt| note_json(sess, nt)).collect::<Vec<_>>(),
        "recent_commands": recent,
    });
    if sess.focus.reported {
        out["window_visible"] = json!(sess.focus.window_visible);
        out["bottom_panel"] = json!(sess.focus.bottom_panel);
        out["tab"] = json!(sess.focus.tab);
        out["open_result"] = open.unwrap_or(Value::Null);
    }
    Ok(out)
}

fn add(sess: &mut Session, a: &Value) -> Result<Value> {
    let text = req(a, "text").trim().to_string();
    if text.is_empty() {
        return Err(Error::BadArgs("text is empty".into()));
    }
    if text.chars().count() > MAX_TEXT {
        return Err(Error::BadArgs(format!(
            "a note is at most {MAX_TEXT} characters"
        )));
    }
    if sess.notes.len() >= MAX_NOTES {
        return Err(Error::Invalid(format!(
            "{MAX_NOTES} notes already; delete some (note.delete / note.clear)"
        )));
    }
    let (x, y) = (n(a, "x"), n(a, "y"));
    let target = match (s(a, "node"), s(a, "artifact"), x, y) {
        (None, None, None, None) => NoteTarget::Net,
        (Some(id), None, None, None) => {
            if sess.doc.kind_of(id).is_none() {
                return Err(Error::NotFound(format!("no node {id:?}")));
            }
            NoteTarget::Node(id.to_string())
        }
        (None, Some(id), None, None) => {
            if sess.artifact(id).is_none() {
                return Err(Error::NotFound(format!("no artifact {id:?}")));
            }
            NoteTarget::Artifact(id.to_string())
        }
        (None, None, Some(x), Some(y)) => NoteTarget::Point { x, y },
        _ => {
            return Err(Error::BadArgs(
                "give one target: node, artifact, or x and y together (or none, for the net)"
                    .into(),
            ))
        }
    };
    let id = sess.next_note_id();
    let note = Note {
        id: id.clone(),
        text,
        author: s(a, "author").unwrap_or("agent").to_string(),
        target,
    };
    let out = note_json(sess, &note);
    sess.notes.push(note);
    Ok(out)
}

fn list(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(sess
        .notes
        .iter()
        .map(|nt| note_json(sess, nt))
        .collect::<Vec<_>>()))
}

fn delete(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    let n = sess.notes.len();
    sess.notes.retain(|nt| nt.id != id);
    if sess.notes.len() == n {
        return Err(Error::NotFound(format!("no note {id:?}")));
    }
    Ok(json!({ "deleted": id }))
}

fn clear(sess: &mut Session, a: &Value) -> Result<Value> {
    let n = sess.notes.len();
    match s(a, "author") {
        Some(who) => sess.notes.retain(|nt| nt.author != who),
        None => sess.notes.clear(),
    }
    Ok(json!({ "deleted": n - sess.notes.len() }))
}

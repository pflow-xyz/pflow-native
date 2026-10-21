//! Macros are recorded command sequences cut from the journal. They live in
//! the session (not the UI), so the CLI and MCP see the same list.

use serde_json::{json, Value};

use crate::registry::{always, arg, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "macro.record",
        label: "Start Recording",
        help: "Start recording journaled commands into a macro.",
        menu: "Macro",
        shortcut: None,
        args: &[],
        enabled: not_recording,
        run: record,
        journal: false,
    },
    CommandSpec {
        id: "macro.stop",
        label: "Stop Recording",
        help: "Stop recording and save the steps as macro `name` (default \"last\").",
        menu: "Macro",
        shortcut: None,
        args: &[opt("name", Str, "macro name")],
        enabled: recording,
        run: stop,
        journal: false,
    },
    CommandSpec {
        id: "macro.play",
        label: "Play Macro",
        help: "Replay a macro as one undo step (rolled back if any step fails).",
        menu: "Macro",
        shortcut: None,
        args: &[opt("name", Str, "macro name (default \"last\")")],
        enabled: always,
        run: play,
        journal: true,
    },
    CommandSpec {
        id: "macro.define",
        label: "Define Macro",
        help: "Store a macro from a step list [{command, args}].",
        menu: "",
        shortcut: None,
        args: &[
            arg("name", Str, "macro name"),
            arg("steps", Json, "[{command, args}]"),
        ],
        enabled: always,
        run: define,
        journal: false,
    },
    CommandSpec {
        id: "macro.list",
        label: "List Macros",
        help: "Macro names and step counts.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: list,
        journal: false,
    },
    CommandSpec {
        id: "macro.get",
        label: "Get Macro",
        help: "A macro's steps.",
        menu: "",
        shortcut: None,
        args: &[arg("name", Str, "macro name")],
        enabled: always,
        run: get,
        journal: false,
    },
    CommandSpec {
        id: "macro.delete",
        label: "Delete Macro",
        help: "Forget a macro.",
        menu: "",
        shortcut: None,
        args: &[arg("name", Str, "macro name")],
        enabled: always,
        run: delete,
        journal: false,
    },
];

fn not_recording(s: &Session) -> std::result::Result<(), String> {
    s.recording
        .is_none()
        .then_some(())
        .ok_or_else(|| "already recording".into())
}

fn recording(s: &Session) -> std::result::Result<(), String> {
    s.recording
        .is_some()
        .then_some(())
        .ok_or_else(|| "not recording".into())
}

fn record(sess: &mut Session, _: &Value) -> Result<Value> {
    sess.recording = Some(sess.journal.len());
    Ok(json!({ "recording": true }))
}

fn stop(sess: &mut Session, a: &Value) -> Result<Value> {
    let start = sess.recording.take().unwrap_or(sess.journal.len());
    let steps: Vec<Step> = sess.journal[start.min(sess.journal.len())..].to_vec();
    let name = s(a, "name").unwrap_or("last").to_string();
    let n = steps.len();
    sess.macros.insert(name.clone(), steps);
    Ok(json!({ "name": name, "steps": n }))
}

fn play(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap_or("last");
    let steps = sess
        .macros
        .get(name)
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("no macro {name:?}")))?;
    sess.batch(&steps)
}

fn define(sess: &mut Session, a: &Value) -> Result<Value> {
    let steps: Vec<Step> = serde_json::from_value(a["steps"].clone())
        .map_err(|e| Error::BadArgs(format!("steps: {e}")))?;
    for st in &steps {
        if sess.registry().find(&st.command).is_none() {
            return Err(Error::UnknownCommand(st.command.clone()));
        }
    }
    let name = s(a, "name").unwrap().to_string();
    let n = steps.len();
    sess.macros.insert(name.clone(), steps);
    Ok(json!({ "name": name, "steps": n }))
}

fn list(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(sess
        .macros
        .iter()
        .map(|(k, v)| json!({ "name": k, "steps": v.len() }))
        .collect::<Vec<_>>()))
}

fn get(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    sess.macros
        .get(name)
        .map(|v| json!(v))
        .ok_or_else(|| Error::NotFound(format!("no macro {name:?}")))
}

fn delete(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    sess.macros
        .remove(name)
        .map(|_| json!({ "deleted": name }))
        .ok_or_else(|| Error::NotFound(format!("no macro {name:?}")))
}

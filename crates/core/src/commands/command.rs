use serde_json::{json, Value};

use crate::registry::{always, arg, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "command.batch",
        label: "Batch",
        help: "Run [{command, args}] as one undo step; everything rolls back if a step fails.",
        menu: "",
        shortcut: None,
        args: &[arg("steps", Json, "[{command, args}]")],
        enabled: always,
        run: batch,
        journal: true,
    },
    CommandSpec {
        id: "command.list",
        label: "List Commands",
        help: "Every command with its args schema, menu, shortcut and current enablement.",
        menu: "Help",
        shortcut: None,
        args: &[opt(
            "prefix",
            Str,
            "only ids starting with this, e.g. \"net.\"",
        )],
        enabled: always,
        run: list,
        journal: false,
    },
    CommandSpec {
        id: "command.journal",
        label: "Journal",
        help: "The last `limit` journaled commands (what a macro recording captures).",
        menu: "",
        shortcut: None,
        args: &[opt("limit", Int, "how many (default 50)")],
        enabled: always,
        run: journal,
        journal: false,
    },
];

pub fn parse_steps(v: &Value) -> Result<Vec<Step>> {
    serde_json::from_value(v.clone()).map_err(|e| Error::BadArgs(format!("steps: {e}")))
}

fn batch(sess: &mut Session, a: &Value) -> Result<Value> {
    let steps = parse_steps(&a["steps"])?;
    sess.batch(&steps)
}

fn list(sess: &mut Session, a: &Value) -> Result<Value> {
    let prefix = s(a, "prefix").unwrap_or("");
    let reg = sess.registry();
    Ok(json!(reg
        .commands()
        .iter()
        .filter(|c| c.id.starts_with(prefix))
        .map(|c| c.describe(sess))
        .collect::<Vec<_>>()))
}

fn journal(sess: &mut Session, a: &Value) -> Result<Value> {
    let limit = a.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
    let j = &sess.journal;
    Ok(json!(j[j.len().saturating_sub(limit)..]))
}

//! `pflow-native-cli mcp`: a Model Context Protocol server on stdio
//! (newline-delimited JSON-RPC 2.0, no async runtime). It holds one
//! long-lived [`Session`], and its tools are thin doors onto the registry,
//! so an agent can do exactly what the app's menus can, headless.

use std::io::{self, BufRead, Write};
use std::process::ExitCode;

use pflow_native_core::Session;
use serde_json::{json, Value};

const PROTOCOL: &str = "2025-06-18";

const INSTRUCTIONS: &str = "pflow-native: a Petri-net editor and simulator built on pflow-rs. \
Everything is a registry command `<namespace>.<verb>` (file, edit, net, sim, view, export, artifact, macro, command). \
Call list_commands first (optionally with a prefix like \"net.\"), then execute one command or batch several \
(a batch is one undo step and rolls back on the first failure). \
The document is the pflow.xyz editor JSON; places/transitions are referenced by id. \
Typical flow: net.add_place / net.add_transition / net.add_arc, then sim.enabled, sim.fire, sim.ode \
(stores a time-series artifact; export.artifact {format:\"csv\"|\"svg\"}), file.save {path}.";

fn tools() -> Value {
    json!([
        {
            "name": "list_commands",
            "description": "List registry commands with their argument JSON Schemas, help, and whether each is enabled right now.",
            "inputSchema": {
                "type": "object",
                "properties": { "prefix": { "type": "string", "description": "only ids starting with this, e.g. \"sim.\"" } },
                "additionalProperties": false
            }
        },
        {
            "name": "execute",
            "description": "Run one registry command, e.g. {\"command\":\"net.add_place\",\"args\":{\"x\":100,\"y\":100,\"initial\":1}}. Returns the command's JSON result.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "command id from list_commands" },
                    "args": { "type": "object", "description": "arguments; see the command's schema" }
                },
                "required": ["command"],
                "additionalProperties": false
            }
        },
        {
            "name": "batch",
            "description": "Run several commands as one undo step: [{\"command\":..., \"args\":...}]. Rolled back entirely if any step fails; the error names the step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "steps": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": { "command": { "type": "string" }, "args": { "type": "object" } },
                            "required": ["command"]
                        }
                    }
                },
                "required": ["steps"],
                "additionalProperties": false
            }
        },
        {
            "name": "get_state",
            "description": "The whole editing state in one call: net (places with tokens, transitions with enablement and rates, arcs), selection, view, artifacts and macros.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }
    ])
}

fn call_tool(sess: &mut Session, name: &str, args: &Value) -> Result<Value, String> {
    let allowed: &[&str] = match name {
        "list_commands" => &["prefix"],
        "execute" => &["command", "args"],
        "batch" => &["steps"],
        "get_state" => &[],
        _ => return Err(format!("unknown tool {name:?}")),
    };
    if let Some(obj) = args.as_object() {
        if let Some(k) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!("{name}: unknown argument {k:?}; takes {allowed:?}"));
        }
    }
    let r = match name {
        "list_commands" => sess.execute(
            "command.list",
            args.get("prefix")
                .map_or(Value::Null, |p| json!({ "prefix": p })),
        ),
        "execute" => {
            let cmd = args
                .get("command")
                .and_then(Value::as_str)
                .ok_or("execute: missing \"command\"")?;
            sess.execute(cmd, args.get("args").cloned().unwrap_or(Value::Null))
        }
        "batch" => {
            let steps = args.get("steps").ok_or("batch: missing \"steps\"")?;
            sess.execute("command.batch", json!({ "steps": steps }))
        }
        "get_state" => sess.execute("net.info", Value::Null).and_then(|net| {
            let artifacts = sess.execute("artifact.list", Value::Null)?;
            let macros = sess.execute("macro.list", Value::Null)?;
            Ok(json!({
                "net": net,
                "view": sess.view,
                "tool": sess.tool,
                "path": sess.path,
                "artifacts": artifacts,
                "macros": macros,
                "can_undo": sess.can_undo(),
                "can_redo": sess.can_redo(),
            }))
        }),
        _ => unreachable!(),
    };
    r.map_err(|e| e.to_string())
}

fn handle(sess: &mut Session, req: &Value) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(Value::Null);
    // Notifications (no id) get no reply.
    let id = id?;
    let result = match method {
        "initialize" => {
            let version = params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or(PROTOCOL);
            Ok(json!({
                "protocolVersion": version,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "pflow-native", "version": env!("CARGO_PKG_VERSION") },
                "instructions": INSTRUCTIONS,
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            let (text, is_error) = match call_tool(sess, name, &args) {
                Ok(Value::String(s)) => (s, false),
                Ok(v) => (serde_json::to_string_pretty(&v).unwrap(), false),
                Err(e) => (e, true),
            };
            Ok(json!({ "content": [{ "type": "text", "text": text }], "isError": is_error }))
        }
        _ => Err(json!({ "code": -32601, "message": format!("method not found: {method}") })),
    };
    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(e) => json!({ "jsonrpc": "2.0", "id": id, "error": e }),
    })
}

pub fn serve(sess: &mut Session) -> ExitCode {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(sess, &req),
            Err(e) => Some(json!({
                "jsonrpc": "2.0", "id": null,
                "error": { "code": -32700, "message": format!("parse error: {e}") }
            })),
        };
        if let Some(r) = reply {
            if writeln!(out, "{r}").and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
    ExitCode::SUCCESS
}

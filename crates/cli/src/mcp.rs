//! `pflow-native-cli mcp`: a Model Context Protocol server on stdio
//! (newline-delimited JSON-RPC 2.0, no async runtime). It holds one
//! long-lived [`Session`], and its tools are thin doors onto the registry,
//! so an agent can do exactly what the app's menus can, headless.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::{Duration, Instant};

use pflow_native_core::Session;
use serde_json::{json, Value};

/// Where commands run: a private headless session, or a running app's
/// control port (so the agent and the human share one live window).
pub enum Backend {
    Headless(Box<Session>),
    Remote {
        addr: String,
        /// The app's per-launch control token (see `core::control_auth`).
        token: String,
        reader: BufReader<TcpStream>,
        writer: TcpStream,
        next_id: u64,
    },
}

impl Backend {
    /// Attaches to the app at `addr` with its control token (from
    /// `$PFLOW_NATIVE_CONTROL_TOKEN` or the app's `<data_dir>/control.json`).
    pub fn connect(addr: &str) -> io::Result<Self> {
        let token = pflow_native_core::control_auth::read_token(addr).ok_or_else(|| {
            io::Error::other(format!(
                "no control token for {addr}: is the app running? (looked in {} and ${})",
                pflow_native_core::control_auth::control_file()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "<no data dir>".into()),
                pflow_native_core::control_auth::TOKEN_ENV
            ))
        })?;
        let sock = addr
            .to_socket_addrs()?
            .next()
            .ok_or_else(|| io::Error::other(format!("cannot resolve {addr}")))?;
        let stream = TcpStream::connect_timeout(&sock, Duration::from_millis(300))?;
        Ok(Backend::Remote {
            addr: addr.to_string(),
            token,
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
            next_id: 0,
        })
    }

    pub fn attached(&self) -> Option<&str> {
        match self {
            Backend::Remote { addr, .. } => Some(addr),
            Backend::Headless(_) => None,
        }
    }

    pub fn exec(&mut self, cmd: &str, args: Value) -> Result<Value, String> {
        match self {
            Backend::Headless(sess) => sess.execute(cmd, args).map_err(|e| e.to_string()),
            Backend::Remote {
                reader,
                writer,
                next_id,
                addr,
                token,
            } => {
                *next_id += 1;
                let req = json!({ "token": token, "id": *next_id, "command": cmd, "args": args });
                writeln!(writer, "{req}")
                    .and_then(|_| writer.flush())
                    .map_err(|e| format!("app at {addr} went away: {e}"))?;
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .map_err(|e| format!("app at {addr}: {e}"))?;
                if line.is_empty() {
                    return Err(format!("app at {addr} closed the connection"));
                }
                let reply: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
                if reply["ok"] == json!(true) {
                    Ok(reply["result"].clone())
                } else {
                    Err(reply["error"].as_str().unwrap_or("error").to_string())
                }
            }
        }
    }
}

const PROTOCOL: &str = "2025-06-18";

/// What an agent should know first. The namespace list comes from the
/// registry, so it cannot go stale as command sets are added.
fn instructions() -> String {
    let reg = pflow_native_core::registry::registry();
    let mut ns: Vec<&str> = reg
        .commands()
        .iter()
        .filter_map(|c| c.id.split('.').next())
        .collect();
    ns.dedup();
    let mut seen = std::collections::BTreeSet::new();
    ns.retain(|n| seen.insert(*n));
    format!(
        "pflow-native: a Petri-net editor and simulator built on pflow-rs. \
Everything is a registry command `<namespace>.<verb>`; namespaces: {}. \
Call list_commands first (optionally with a prefix like \"net.\" or \"analysis.\"), then execute one command or batch several \
(a batch is one undo step and rolls back on the first failure). \
The document is the pflow.xyz editor JSON; places/transitions are referenced by id. \
Typical flow: net.add_place / net.add_transition / net.add_arc, then sim.enabled, sim.fire, sim.ode \
(stores a time-series artifact; export.artifact {{format:\"csv\"|\"svg\"}}), file.save {{path}}.{}",
        ns.join(", "),
        if cfg!(feature = "services") {
            " Remote writes (remote.*) return a preview unless called with send: true; \
when attached to the app, a send waits for the user's confirmation there."
        } else {
            ""
        }
    )
}

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
            "name": "screenshot",
            "description": "A PNG of the running app window (only when attached to the app's control port; headless, use execute export.svg).",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        },
        {
            "name": "get_state",
            "description": "The whole editing state in one call: net (places with tokens, transitions with enablement and rates, arcs), selection, view, artifacts and macros.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }
    ])
}

fn call_tool(be: &mut Backend, name: &str, args: &Value) -> Result<Value, String> {
    let allowed: &[&str] = match name {
        "list_commands" => &["prefix"],
        "execute" => &["command", "args"],
        "batch" => &["steps"],
        "get_state" | "screenshot" => &[],
        _ => return Err(format!("unknown tool {name:?}")),
    };
    if let Some(obj) = args.as_object() {
        if let Some(k) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!("{name}: unknown argument {k:?}; takes {allowed:?}"));
        }
    }
    match name {
        "list_commands" => be.exec(
            "command.list",
            args.get("prefix")
                .map_or(Value::Null, |p| json!({ "prefix": p })),
        ),
        "execute" => {
            let cmd = args
                .get("command")
                .and_then(Value::as_str)
                .ok_or("execute: missing \"command\"")?;
            be.exec(cmd, args.get("args").cloned().unwrap_or(Value::Null))
        }
        "batch" => {
            let steps = args.get("steps").ok_or("batch: missing \"steps\"")?;
            be.exec("command.batch", json!({ "steps": steps }))
        }
        "get_state" => Ok(json!({
            "attached": be.attached(),
            "net": be.exec("net.info", Value::Null)?,
            "view": be.exec("view.get", Value::Null)?,
            "artifacts": be.exec("artifact.list", Value::Null)?,
            "macros": be.exec("macro.list", Value::Null)?,
        })),
        _ => unreachable!(),
    }
}

/// Asks the app for a screenshot and waits for the file to settle.
fn screenshot(be: &mut Backend) -> Result<Vec<u8>, String> {
    if be.attached().is_none() {
        return Err("screenshot needs a running app (start `pflow-native`; mcp attaches to its control port). Headless, use execute export.svg.".into());
    }
    let path = std::env::temp_dir().join(format!(
        "pflow-native-mcp-{}-{}.png",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let _ = std::fs::remove_file(&path);
    be.exec("view.screenshot", json!({ "path": path.to_string_lossy() }))?;
    let t0 = Instant::now();
    let mut last = 0;
    while t0.elapsed() < Duration::from_secs(10) {
        std::thread::sleep(Duration::from_millis(100));
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if size > 0 && size == last {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            let _ = std::fs::remove_file(&path);
            return Ok(bytes);
        }
        last = size;
    }
    Err("the app did not produce a screenshot within 10 s (is its window minimized, hidden or covered?)".into())
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16
            | (*c.get(1).unwrap_or(&0) as u32) << 8
            | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= c.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn handle(be: &mut Backend, req: &Value) -> Option<Value> {
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
                "instructions": instructions(),
            }))
        }
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tools() })),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            if name == "screenshot" {
                return Some(
                    json!({ "jsonrpc": "2.0", "id": id, "result": match screenshot(be) {
                        Ok(png) => json!({ "content": [{ "type": "image", "data": base64(&png), "mimeType": "image/png" }], "isError": false }),
                        Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
                    }}),
                );
            }
            let (text, is_error) = match call_tool(be, name, &args) {
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

/// Serves MCP on stdio until stdin closes, then hands the backend back (so
/// the caller can save with `-o`).
pub fn serve(mut be: Backend) -> Backend {
    match be.attached() {
        Some(addr) => eprintln!("pflow-native mcp: attached to the app at {addr}"),
        None => eprintln!("pflow-native mcp: headless session"),
    }
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Ok(req) => handle(&mut be, &req),
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
    be
}

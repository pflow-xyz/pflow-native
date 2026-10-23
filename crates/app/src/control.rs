//! The app's control port: the same JSON-lines protocol as
//! `pflow-native-cli control`, on a local TCP socket, so agents (and
//! `pflow-native-cli mcp`, which attaches when the port is up) drive the
//! live window instead of a headless copy. Requests are handed to the UI
//! thread and run in `App::logic`, which eframe calls even while the window
//! is hidden or covered, through the registry like everything else.
//!
//! Request:  `{"token": "…", "id": 1, "command": "net.info", "args": {}}` or `{"token": "…", "id": 2, "batch": [...]}`
//! Reply:    `{"id": 1, "ok": true, "result": ...}` / `{"id": 1, "ok": false, "error": "..."}`
//!
//! Every request must carry the per-launch token the app writes to
//! `<data_dir>/control.json` (owner-only; see `core::control_auth`). A
//! request without it, or anything that looks like HTTP (a browser), gets
//! the connection closed. Remote sends that arrive here still need the
//! user's confirmation in the app (see `App::serve_control`).

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::{channel, Receiver, Sender};

use eframe::egui;
use serde_json::{json, Value};

pub const DEFAULT_ADDR: &str = "127.0.0.1:7979";

pub struct Request {
    pub body: Value,
    pub reply: Sender<Value>,
}

pub struct Control {
    pub addr: String,
    rx: Receiver<Request>,
}

impl Control {
    /// Binds `addr` (loopback only) and starts the accept thread. Fails if
    /// the address is not loopback or the port is taken (e.g. a second app).
    pub fn start(addr: &str, ctx: egui::Context, token: String) -> Result<Self, String> {
        let listener = TcpListener::bind(addr).map_err(|e| format!("control port {addr}: {e}"))?;
        let local = listener.local_addr().map_err(|e| e.to_string())?;
        if !local.ip().is_loopback() {
            return Err(format!("control port must be loopback, got {local}"));
        }
        let (tx, rx) = channel::<Request>();
        std::thread::Builder::new()
            .name("pflow-native-control".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let tx = tx.clone();
                    let ctx = ctx.clone();
                    let token = token.clone();
                    std::thread::spawn(move || serve(stream, tx, ctx, &token));
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            addr: local.to_string(),
            rx,
        })
    }

    /// Requests waiting for the UI thread (at most `max` per frame).
    pub fn drain(&self, max: usize) -> Vec<Request> {
        self.rx.try_iter().take(max).collect()
    }
}

/// Whether a line is the start of an HTTP request (a browser or anything
/// else speaking HTTP at the port): those are dropped without a reply.
fn looks_like_http(line: &str) -> bool {
    let m = line.split(' ').next().unwrap_or("");
    matches!(
        m,
        "GET" | "POST" | "PUT" | "DELETE" | "HEAD" | "OPTIONS" | "PATCH" | "CONNECT" | "TRACE"
    ) || line.contains(" HTTP/")
}

fn serve(stream: std::net::TcpStream, tx: Sender<Request>, ctx: egui::Context, token: &str) {
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if looks_like_http(&line) {
            break;
        }
        let body = match serde_json::from_str::<Value>(&line) {
            Err(e) => {
                let _ = writeln!(
                    out,
                    "{}",
                    json!({ "id": null, "ok": false, "error": format!("bad JSON: {e}") })
                );
                break;
            }
            Ok(b) => b,
        };
        let given = body.get("token").and_then(Value::as_str).unwrap_or("");
        if !pflow_native_core::control_auth::tokens_equal(given, token) {
            let id = body.get("id").cloned().unwrap_or(Value::Null);
            let _ = writeln!(
                out,
                "{}",
                json!({ "id": id, "ok": false, "error": "unauthorized: send the token from <data_dir>/control.json (pflow-native-cli mcp does this)" })
            );
            break;
        }
        let (rtx, rrx) = channel();
        if tx.send(Request { body, reply: rtx }).is_err() {
            break;
        }
        ctx.request_repaint();
        let reply = match rrx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(v) => v,
            Err(_) => {
                json!({ "id": null, "ok": false, "error": "app did not answer within 60 s: for a long command use job.start (or the MCP run_job tool)" })
            }
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpStream;

    /// A control server whose "app" answers every request with its command.
    fn start() -> (String, String) {
        let token = pflow_native_core::control_auth::new_token().unwrap();
        let c = Control::start("127.0.0.1:0", egui::Context::default(), token.clone()).unwrap();
        let addr = c.addr.clone();
        std::thread::spawn(move || loop {
            for r in c.drain(64) {
                let _ = r
                    .reply
                    .send(json!({ "ok": true, "result": r.body["command"] }));
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        });
        (addr, token)
    }

    fn exchange(addr: &str, line: &str) -> Option<Value> {
        let mut s = TcpStream::connect(addr).unwrap();
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        writeln!(s, "{line}").unwrap();
        let mut reply = String::new();
        BufReader::new(s).read_line(&mut reply).ok()?;
        (!reply.is_empty()).then(|| serde_json::from_str(&reply).unwrap())
    }

    #[test]
    fn requests_need_the_launch_token() {
        let (addr, token) = start();
        let none = exchange(
            &addr,
            r#"{"id":1,"command":"remote.pflow.list","args":{"server":"http://attacker"}}"#,
        )
        .unwrap();
        assert_eq!(none["ok"], false);
        assert!(none["error"].as_str().unwrap().starts_with("unauthorized"));
        let wrong = exchange(
            &addr,
            &json!({"token": "0".repeat(64), "id": 2, "command": "net.info"}).to_string(),
        )
        .unwrap();
        assert_eq!(wrong["ok"], false);
        let ok = exchange(
            &addr,
            &json!({"token": token, "id": 3, "command": "net.info"}).to_string(),
        )
        .unwrap();
        assert_eq!(ok, json!({"ok": true, "result": "net.info"}));
    }

    #[test]
    fn http_requests_get_no_answer() {
        let (addr, _) = start();
        assert!(exchange(&addr, "POST / HTTP/1.1").is_none());
        assert!(exchange(&addr, "GET /?x HTTP/1.1").is_none());
    }
}

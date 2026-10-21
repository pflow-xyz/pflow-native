//! The app's control port: the same JSON-lines protocol as
//! `pflow-native-cli control`, on a local TCP socket, so agents (and
//! `pflow-native-cli mcp`, which attaches when the port is up) drive the
//! live window instead of a headless copy. Requests are handed to the UI
//! thread and run between frames, through the registry like everything else.
//!
//! Request:  `{"id": 1, "command": "net.info", "args": {}}` or `{"id": 2, "batch": [...]}`
//! Reply:    `{"id": 1, "ok": true, "result": ...}` / `{"id": 1, "ok": false, "error": "..."}`

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
    pub fn start(addr: &str, ctx: egui::Context) -> Result<Self, String> {
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
                    std::thread::spawn(move || serve(stream, tx, ctx));
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

fn serve(stream: std::net::TcpStream, tx: Sender<Request>, ctx: egui::Context) {
    let Ok(mut out) = stream.try_clone() else {
        return;
    };
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(e) => json!({ "id": null, "ok": false, "error": format!("bad JSON: {e}") }),
            Ok(body) => {
                let (rtx, rrx) = channel();
                if tx.send(Request { body, reply: rtx }).is_err() {
                    break;
                }
                ctx.request_repaint();
                match rrx.recv_timeout(std::time::Duration::from_secs(60)) {
                    Ok(v) => v,
                    Err(_) => json!({ "id": null, "ok": false, "error": "app did not answer" }),
                }
            }
        };
        if writeln!(out, "{reply}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

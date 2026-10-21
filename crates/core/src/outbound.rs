//! Talking to the outside world, safely by default.
//!
//! Every command that would publish or write to a remote service
//! (pflow.xyz save, a Gist, sim.pflow.xyz) builds an [`Outbound`] request
//! and, unless the caller passed `send: true`, returns its preview instead of
//! sending it. Credentials come from the environment or a local credentials
//! file, never from documents, args, the journal or artifacts, and previews
//! redact them.

use std::path::PathBuf;

use serde::Serialize;
use serde_json::{json, Value};

use crate::{Error, Result};

/// A request about to leave the machine.
#[derive(Debug, Clone, Serialize)]
pub struct Outbound {
    pub method: String,
    pub url: String,
    /// Header names and values; credentials appear as "<redacted>".
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
    #[serde(skip)]
    secret_headers: Vec<(String, String)>,
}

impl Outbound {
    pub fn new(method: &str, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            headers: vec![],
            body: None,
            secret_headers: vec![],
        }
    }

    pub fn json(mut self, body: Value) -> Self {
        self.headers
            .push(("Content-Type".into(), "application/json".into()));
        self.body = Some(body);
        self
    }

    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }

    /// A credential header (shown redacted in previews).
    pub fn secret(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), "<redacted>".into()));
        self.secret_headers.push((k.into(), v.into()));
        self
    }

    /// The preview a command returns when it was not asked to send.
    pub fn preview(&self, why: &str) -> Value {
        json!({
            "sent": false,
            "preview": self,
            "note": format!("{why} Nothing was sent; repeat with \"send\": true to send it."),
        })
    }

    /// Sends the request and returns the response body as JSON (a non-JSON
    /// body comes back as a string).
    #[cfg(feature = "http")]
    pub fn send(&self) -> Result<Value> {
        let mut headers: Vec<(String, String)> = self
            .headers
            .iter()
            .filter(|(k, _)| !self.secret_headers.iter().any(|(s, _)| s == k))
            .cloned()
            .collect();
        headers.extend(self.secret_headers.iter().cloned());
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .build()
            .new_agent();
        let mut resp = match self.method.as_str() {
            "GET" | "DELETE" => {
                let mut r = if self.method == "GET" {
                    agent.get(&self.url)
                } else {
                    agent.delete(&self.url)
                };
                for (k, v) in &headers {
                    r = r.header(k, v);
                }
                r.call()
            }
            "POST" | "PUT" | "PATCH" => {
                let mut r = match self.method.as_str() {
                    "POST" => agent.post(&self.url),
                    "PUT" => agent.put(&self.url),
                    _ => agent.patch(&self.url),
                };
                for (k, v) in &headers {
                    r = r.header(k, v);
                }
                let body = self
                    .body
                    .as_ref()
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                r.send(body.as_bytes())
            }
            m => return Err(Error::BadArgs(format!("unsupported method {m}"))),
        }
        .map_err(|e| Error::Io(format!("{} {}: {e}", self.method, self.url)))?;
        let status = resp.status().as_u16();
        let text = resp
            .body_mut()
            .with_config()
            .limit(32 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| Error::Io(format!("{} {}: {e}", self.method, self.url)))?;
        let body: Value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        if !(200..300).contains(&status) {
            return Err(Error::Io(format!(
                "{} {} → HTTP {status}: {body}",
                self.method, self.url
            )));
        }
        Ok(body)
    }

    #[cfg(not(feature = "http"))]
    pub fn send(&self) -> Result<Value> {
        Err(Error::Io(format!(
            "cannot send {} {}: this build has no HTTP client (feature \"http\")",
            self.method, self.url
        )))
    }
}

/// Sends `req` if `send` is true, else returns its preview.
pub fn send_or_preview(req: &Outbound, send: bool, why: &str) -> Result<Value> {
    if send {
        let response = req.send()?;
        Ok(json!({ "sent": true, "request": req, "response": response }))
    } else {
        Ok(req.preview(why))
    }
}

/// The per-user data directory: `$PFLOW_NATIVE_HOME`, else the platform's
/// application-data directory. `None` on the web (no file system).
pub fn data_dir() -> Option<PathBuf> {
    if cfg!(target_arch = "wasm32") {
        return None;
    }
    if let Ok(d) = std::env::var("PFLOW_NATIVE_HOME") {
        return Some(PathBuf::from(d));
    }
    let home = std::env::var("HOME").ok().map(PathBuf::from)?;
    Some(if cfg!(target_os = "macos") {
        home.join("Library/Application Support/pflow-native")
    } else if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        PathBuf::from(x).join("pflow-native")
    } else {
        home.join(".local/share/pflow-native")
    })
}

/// A credential by name: the environment variable `name`, else the key
/// `name` in `<data_dir>/credentials.json`. Never logged or stored.
pub fn credential(name: &str) -> Option<String> {
    if let Ok(v) = std::env::var(name) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let path = data_dir()?.join("credentials.json");
    let text = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get(name).and_then(Value::as_str).map(str::to_string)
}

/// Which credentials are configured (names and source only).
pub fn credential_status(names: &[&str]) -> Value {
    json!(names
        .iter()
        .map(|n| json!({
            "name": n,
            "configured": credential(n).is_some(),
            "from": if std::env::var(n).is_ok_and(|v| !v.is_empty()) { "env" } else if credential(n).is_some() { "credentials.json" } else { "" },
        }))
        .collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_redact_secrets_and_send_nothing() {
        let req = Outbound::new("POST", "https://example.invalid/api")
            .json(json!({"a": 1}))
            .secret("Authorization", "Bearer s3cret");
        let p = send_or_preview(&req, false, "Publishing is outbound.").unwrap();
        assert_eq!(p["sent"], false);
        let text = p.to_string();
        assert!(!text.contains("s3cret"), "{text}");
        assert!(text.contains("<redacted>"));
    }
}

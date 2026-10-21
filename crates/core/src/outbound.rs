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

/// The hosts each credential may be sent to (HTTPS only). A credential is
/// never attached to a request for any other host, whatever URL a caller
/// supplies: pflow.xyz verifies its bearer against GitHub, so a GitHub token
/// may go to both.
pub fn credential_hosts(name: &str) -> &'static [&'static str] {
    match name {
        "PFLOW_XYZ_TOKEN" => &["pflow.xyz"],
        "GITHUB_TOKEN" => &["api.github.com", "pflow.xyz"],
        "SIM_PFLOW_TOKEN" => &["sim.pflow.xyz"],
        _ => &[],
    }
}

/// Extra hosts a credential may go to, for tests against a local mock
/// server: `PFLOW_NATIVE_CREDENTIAL_HOSTS="127.0.0.1:4321,localhost:9000"`
/// (host[:port], http or https). Only the user's own environment can set it.
const EXTRA_HOSTS_ENV: &str = "PFLOW_NATIVE_CREDENTIAL_HOSTS";

/// (scheme, host, port) of a URL, or why it is not acceptable for a
/// credentialed request (no userinfo, a host present, lowercase compare).
fn authority(url: &str) -> std::result::Result<(String, String, Option<u16>), String> {
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| format!("{url:?} is not an absolute URL"))?;
    let scheme = scheme.to_ascii_lowercase();
    let auth = rest.split(['/', '?', '#']).next().unwrap_or("");
    if auth.contains('@') {
        return Err(format!("{url:?} embeds credentials in its address"));
    }
    if auth.is_empty() {
        return Err(format!("{url:?} has no host"));
    }
    let (host, port) = match auth.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => match p.parse::<u16>() {
            Ok(port) => (h.to_string(), Some(port)),
            Err(_) => return Err(format!("{url:?} has a malformed port")),
        },
        _ => (auth.to_string(), None),
    };
    Ok((scheme, host.to_ascii_lowercase(), port))
}

/// Whether `name`'s credential may be sent to `url`.
pub fn credential_allowed(name: &str, url: &str) -> std::result::Result<(), String> {
    let (scheme, host, port) =
        authority(url).map_err(|e| format!("refusing to send {name}: {e}"))?;
    let hostport = match port {
        Some(p) => format!("{host}:{p}"),
        None => host.clone(),
    };
    let extra: Vec<String> = std::env::var(EXTRA_HOSTS_ENV)
        .map(|v| {
            v.split(',')
                .map(|h| h.trim().to_ascii_lowercase())
                .filter(|h| !h.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if extra.iter().any(|h| *h == hostport || *h == host) && (scheme == "https" || scheme == "http")
    {
        return Ok(());
    }
    let allowed = credential_hosts(name);
    if scheme == "https" && port.is_none_or(|p| p == 443) && allowed.contains(&host.as_str()) {
        return Ok(());
    }
    Err(format!(
        "refusing to send {name} to {scheme}://{hostport}: it only goes over HTTPS to {allowed:?}"
    ))
}

/// `name`'s credential for a request to `url`: `Ok(None)` when it is not
/// configured, an error when it is configured but `url` is not one of its
/// hosts (so a caller-chosen server can never receive it).
pub fn credential_for(name: &str, url: &str) -> Result<Option<String>> {
    match credential(name) {
        None => Ok(None),
        Some(v) => {
            credential_allowed(name, url).map_err(Error::BadArgs)?;
            Ok(Some(v))
        }
    }
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
    fn credentials_go_only_to_their_hosts() {
        assert!(credential_allowed("GITHUB_TOKEN", "https://api.github.com/gists").is_ok());
        assert!(credential_allowed("GITHUB_TOKEN", "https://pflow.xyz/api/diagrams").is_ok());
        assert!(credential_allowed("PFLOW_XYZ_TOKEN", "https://pflow.xyz:443/api/save").is_ok());
        assert!(credential_allowed("SIM_PFLOW_TOKEN", "https://sim.pflow.xyz/mcp").is_ok());
        for bad in [
            "http://pflow.xyz/api/diagrams",          // not HTTPS
            "https://pflow.xyz.attacker.example/api", // suffix trick
            "https://attacker.example/api/diagrams",
            "https://pflow.xyz@attacker.example/", // userinfo trick
            "https://pflow.xyz:8443/api",          // other port
            "https://sim.pflow.xyz/mcp",           // wrong credential for host
        ] {
            assert!(credential_allowed("PFLOW_XYZ_TOKEN", bad).is_err(), "{bad}");
        }
        assert!(credential_allowed("SIM_PFLOW_TOKEN", "https://pflow.xyz/").is_err());
        assert!(credential_allowed("UNKNOWN", "https://pflow.xyz/").is_err());
    }

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

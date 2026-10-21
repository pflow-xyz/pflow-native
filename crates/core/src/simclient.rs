//! A minimal MCP client for sim.pflow.xyz's streamable-HTTP endpoint
//! (`POST https://sim.pflow.xyz/mcp`, served by mcp-go's
//! `NewStreamableHTTPServer` in sim-pflow-xyz `pkg/simmcp/simmcp.go`).
//!
//! The protocol, as that server speaks it:
//!
//! - Every message is a JSON-RPC 2.0 POST with
//!   `Accept: application/json, text/event-stream`. A reply is either one
//!   JSON message or an SSE stream whose `data:` lines carry the messages.
//! - `initialize` answers with an `Mcp-Session-Id` header that every later
//!   request carries, then the client sends `notifications/initialized`
//!   (answered 202, no body).
//! - Auth is an OAuth bearer (`Authorization: Bearer …`, issued by sim's
//!   own authorization server, `pkg/mcpauth`). Discovery and public tools
//!   need none; a private tool called without one answers a tool error
//!   whose `_meta["mcp/www_authenticate"]` names the scope it wants.
//!
//! The framing and decoding here are pure functions; the HTTP transport is a
//! trait, implemented with `ureq` under the `http` feature. Without it
//! (the wasm build) [`connect`] explains that nothing can be sent.
//!
//! The bearer token lives only in [`Client`] and the request headers; it is
//! never formatted into errors or results.

use serde_json::{json, Value};

use crate::{Error, Result};

pub const DEFAULT_ENDPOINT: &str = "https://sim.pflow.xyz/mcp";
pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const ACCEPT: &str = "application/json, text/event-stream";
pub const SESSION_HEADER: &str = "Mcp-Session-Id";

/// A JSON-RPC request.
pub fn request(id: u64, method: &str, params: Value) -> Value {
    let mut m = json!({ "jsonrpc": "2.0", "id": id, "method": method });
    if !params.is_null() {
        m["params"] = params;
    }
    m
}

/// A JSON-RPC notification (no id, no reply).
pub fn notification(method: &str, params: Value) -> Value {
    let mut m = json!({ "jsonrpc": "2.0", "method": method });
    if !params.is_null() {
        m["params"] = params;
    }
    m
}

/// The `tools/call` request for `tool`.
pub fn tool_call(id: u64, tool: &str, arguments: Value) -> Value {
    request(
        id,
        "tools/call",
        json!({ "name": tool, "arguments": arguments }),
    )
}

/// The JSON-RPC messages in a POST reply: a JSON message or batch, or an
/// SSE stream (`event:`/`data:` lines; multi-line data joined with `\n`).
pub fn messages(content_type: &str, body: &str) -> Result<Vec<Value>> {
    let trimmed = body.trim_start();
    let sse = content_type.contains("text/event-stream")
        || trimmed.starts_with("data:")
        || trimmed.starts_with("event:")
        || trimmed.starts_with("id:")
        || trimmed.starts_with(':');
    if !sse {
        if trimmed.is_empty() {
            return Ok(vec![]);
        }
        let v: Value = serde_json::from_str(body)
            .map_err(|e| Error::Parse(format!("MCP reply is not JSON: {e}")))?;
        return Ok(match v {
            Value::Array(a) => a,
            v => vec![v],
        });
    }
    let mut out = vec![];
    let mut data: Vec<&str> = vec![];
    let flush = |data: &mut Vec<&str>, out: &mut Vec<Value>| -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        let joined = data.join("\n");
        data.clear();
        if joined.trim().is_empty() {
            return Ok(());
        }
        let v: Value = serde_json::from_str(&joined)
            .map_err(|e| Error::Parse(format!("MCP SSE event is not JSON: {e}")))?;
        match v {
            Value::Array(a) => out.extend(a),
            v => out.push(v),
        }
        Ok(())
    };
    for line in body.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() {
            flush(&mut data, &mut out)?;
        } else if let Some(rest) = line.strip_prefix("data:") {
            data.push(rest.strip_prefix(' ').unwrap_or(rest));
        }
        // event:, id:, retry: and comments carry nothing we need.
    }
    flush(&mut data, &mut out)?;
    Ok(out)
}

/// The `result` of the reply to request `id`, or its JSON-RPC error.
pub fn response_for(msgs: &[Value], id: u64) -> Result<Value> {
    let m = msgs
        .iter()
        .find(|m| m.get("id").and_then(Value::as_u64) == Some(id))
        .ok_or_else(|| Error::Io(format!("MCP reply has no response to request {id}")))?;
    if let Some(e) = m.get("error") {
        let code = e.get("code").and_then(Value::as_i64).unwrap_or(0);
        let msg = e.get("message").and_then(Value::as_str).unwrap_or("");
        let data = e.get("data").map(|d| format!(" ({d})")).unwrap_or_default();
        return Err(Error::Io(format!("MCP error {code}: {msg}{data}")));
    }
    Ok(m.get("result").cloned().unwrap_or(Value::Null))
}

/// The text of a tool result's `content` (text parts, joined).
fn content_text(result: &Value) -> String {
    result
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|c| c.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|c| c.get("text").and_then(Value::as_str))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Decodes a `tools/call` result: an auth challenge or `isError` becomes an
/// error; otherwise `structuredContent`, else the text content parsed as
/// JSON when it is JSON, else the text.
pub fn tool_output(tool: &str, result: &Value) -> Result<Value> {
    if let Some(ch) = result.pointer("/_meta/mcp~1www_authenticate") {
        let challenge = match ch {
            Value::Array(a) => a
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let scope = challenge
            .split("scope=\"")
            .nth(1)
            .and_then(|r| r.split('"').next())
            .unwrap_or("");
        return Err(Error::Io(format!(
            "{tool} needs sign-in{}: set SIM_PFLOW_TOKEN to a sim.pflow.xyz OAuth access token{}",
            if scope.is_empty() {
                String::new()
            } else {
                format!(" (scope {scope})")
            },
            if challenge.is_empty() {
                String::new()
            } else {
                format!(" [challenge: {challenge}]")
            }
        )));
    }
    let text = content_text(result);
    if result.get("isError").and_then(Value::as_bool) == Some(true) {
        return Err(Error::Io(format!(
            "{tool}: {}",
            if text.is_empty() {
                "tool error".into()
            } else {
                text
            }
        )));
    }
    if let Some(s) = result.get("structuredContent").filter(|v| !v.is_null()) {
        return Ok(s.clone());
    }
    if text.is_empty() {
        return Ok(result.clone());
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::String(text)))
}

/// One HTTP reply, reduced to what MCP needs.
#[derive(Debug, Clone)]
pub struct HttpReply {
    pub status: u16,
    pub content_type: String,
    /// The `Mcp-Session-Id` response header, when present.
    pub session: Option<String>,
    pub body: String,
}

/// Sends one POST. Implementations must not log header values.
pub trait Transport {
    fn post(&mut self, url: &str, headers: &[(String, String)], body: &str) -> Result<HttpReply>;
}

/// The native transport.
#[cfg(feature = "http")]
pub struct UreqTransport {
    agent: ureq::Agent,
}

#[cfg(feature = "http")]
impl Default for UreqTransport {
    fn default() -> Self {
        let agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            // sim_prompt waits on an LLM.
            .timeout_global(Some(std::time::Duration::from_secs(300)))
            .build()
            .new_agent();
        Self { agent }
    }
}

#[cfg(feature = "http")]
impl Transport for UreqTransport {
    fn post(&mut self, url: &str, headers: &[(String, String)], body: &str) -> Result<HttpReply> {
        let mut r = self.agent.post(url);
        for (k, v) in headers {
            r = r.header(k, v);
        }
        let mut resp = r
            .send(body.as_bytes())
            .map_err(|e| Error::Io(format!("POST {url}: {e}")))?;
        let header = |name: &str| {
            resp.headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string)
        };
        let content_type = header("content-type").unwrap_or_default();
        let session = header(SESSION_HEADER);
        let status = resp.status().as_u16();
        let body = resp
            .body_mut()
            .with_config()
            .limit(32 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| Error::Io(format!("POST {url}: {e}")))?;
        Ok(HttpReply {
            status,
            content_type,
            session,
            body,
        })
    }
}

/// An MCP session against one endpoint.
pub struct Client<T: Transport> {
    pub endpoint: String,
    token: Option<String>,
    transport: T,
    session: Option<String>,
    protocol: Option<String>,
    next_id: u64,
    /// `serverInfo` from `initialize`.
    pub server_info: Value,
}

impl<T: Transport> Client<T> {
    /// A client; `token` is sent as `Authorization: Bearer …` when set.
    pub fn new(endpoint: &str, token: Option<String>, transport: T) -> Self {
        Self {
            endpoint: endpoint.to_string(),
            token,
            transport,
            session: None,
            protocol: None,
            next_id: 0,
            server_info: Value::Null,
        }
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session.as_deref()
    }

    pub fn has_token(&self) -> bool {
        self.token.is_some()
    }

    fn headers(&self) -> Vec<(String, String)> {
        let mut h = vec![
            ("Content-Type".to_string(), "application/json".to_string()),
            ("Accept".to_string(), ACCEPT.to_string()),
        ];
        if let Some(s) = &self.session {
            h.push((SESSION_HEADER.to_string(), s.clone()));
        }
        if let Some(p) = &self.protocol {
            h.push(("MCP-Protocol-Version".to_string(), p.clone()));
        }
        if let Some(t) = &self.token {
            h.push(("Authorization".to_string(), format!("Bearer {t}")));
        }
        h
    }

    fn post(&mut self, msg: &Value) -> Result<HttpReply> {
        let reply = self
            .transport
            .post(&self.endpoint, &self.headers(), &msg.to_string())?;
        if let Some(s) = &reply.session {
            self.session = Some(s.clone());
        }
        if !(200..300).contains(&reply.status) {
            let hint = match reply.status {
                401 | 403 => " (check SIM_PFLOW_TOKEN)",
                404 if self.session.is_some() => " (the MCP session may have expired)",
                _ => "",
            };
            let mut body = reply.body.trim().to_string();
            if body.len() > 2000 {
                let cut = (0..=2000).rev().find(|&i| body.is_char_boundary(i));
                body.truncate(cut.unwrap_or(0));
                body.push('…');
            }
            return Err(Error::Io(format!(
                "POST {} → HTTP {}{hint}: {body}",
                self.endpoint, reply.status
            )));
        }
        Ok(reply)
    }

    /// One request; returns its `result`.
    pub fn rpc(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        let reply = self.post(&request(id, method, params))?;
        let msgs = messages(&reply.content_type, &reply.body)?;
        response_for(&msgs, id)
    }

    /// One notification (the reply, normally 202, carries nothing).
    pub fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.post(&notification(method, params)).map(|_| ())
    }

    /// `initialize` + `notifications/initialized`.
    pub fn initialize(&mut self) -> Result<Value> {
        let result = self.rpc(
            "initialize",
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": { "name": "pflow-native", "version": env!("CARGO_PKG_VERSION") },
            }),
        )?;
        self.protocol = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| Some(PROTOCOL_VERSION.to_string()));
        self.server_info = result.get("serverInfo").cloned().unwrap_or(Value::Null);
        self.notify("notifications/initialized", Value::Null)?;
        Ok(result)
    }

    /// Every tool, following `nextCursor`.
    pub fn list_tools(&mut self) -> Result<Vec<Value>> {
        let mut tools = vec![];
        let mut cursor: Option<String> = None;
        for _ in 0..64 {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => Value::Null,
            };
            let r = self.rpc("tools/list", params)?;
            tools.extend(
                r.get("tools")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default(),
            );
            cursor = r
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|c| !c.is_empty())
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }
        Ok(tools)
    }

    /// `tools/call`, decoded with [`tool_output`].
    pub fn call_tool(&mut self, tool: &str, arguments: Value) -> Result<Value> {
        let r = self.rpc(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )?;
        tool_output(tool, &r)
    }
}

/// A native client for `endpoint`, not yet initialized.
#[cfg(feature = "http")]
pub fn connect(endpoint: &str, token: Option<String>) -> Result<Client<UreqTransport>> {
    Ok(Client::new(endpoint, token, UreqTransport::default()))
}

/// Without the `http` feature nothing can be sent.
#[cfg(not(feature = "http"))]
pub fn connect(endpoint: &str, _token: Option<String>) -> Result<Client<NoTransport>> {
    Err(Error::Io(format!(
        "cannot reach {endpoint}: this build has no HTTP client (feature \"http\"); previews still work"
    )))
}

/// The transport of a build without HTTP: refuses everything.
#[cfg(not(feature = "http"))]
pub struct NoTransport;

#[cfg(not(feature = "http"))]
impl Transport for NoTransport {
    fn post(&mut self, url: &str, _: &[(String, String)], _: &str) -> Result<HttpReply> {
        Err(Error::Io(format!(
            "cannot POST {url}: this build has no HTTP client (feature \"http\")"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_and_json_replies() {
        let sse = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":2,\n\
                   data: \"result\":{\"ok\":true}}\n\n: keepalive\n\n";
        let m = messages("text/event-stream", sse).unwrap();
        assert_eq!(response_for(&m, 2).unwrap(), json!({"ok": true}));
        let m = messages(
            "application/json",
            r#"{"jsonrpc":"2.0","id":3,"error":{"code":-32601,"message":"no such method"}}"#,
        )
        .unwrap();
        let e = response_for(&m, 3).unwrap_err().to_string();
        assert!(e.contains("-32601") && e.contains("no such method"), "{e}");
        assert!(messages("application/json", "").unwrap().is_empty());
    }

    #[test]
    fn tool_results_decode() {
        let ok = json!({"content": [{"type": "text", "text": "{\"id\":\"m1\"}"}]});
        assert_eq!(tool_output("t", &ok).unwrap(), json!({"id": "m1"}));
        let structured = json!({"content": [], "structuredContent": {"a": 1}});
        assert_eq!(tool_output("t", &structured).unwrap(), json!({"a": 1}));
        let err = json!({"isError": true, "content": [{"type": "text", "text": "bad model"}]});
        assert!(tool_output("t", &err)
            .unwrap_err()
            .to_string()
            .contains("bad model"));
        let auth = json!({
            "isError": true,
            "content": [{"type": "text", "text": "Sign in"}],
            "_meta": {"mcp/www_authenticate": ["Bearer resource_metadata=\"x\", scope=\"sim.publish\""]},
        });
        let e = tool_output("sim_publish", &auth).unwrap_err().to_string();
        assert!(
            e.contains("SIM_PFLOW_TOKEN") && e.contains("sim.publish"),
            "{e}"
        );
    }
}

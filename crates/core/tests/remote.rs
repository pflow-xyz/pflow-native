//! Remote bridges (pflow.xyz, GitHub Gist, sim.pflow.xyz MCP) without the
//! network: previews are checked as data, and sends go to a mock HTTP
//! server on 127.0.0.1. No test reaches a real host: every sending call
//! passes a `server` / `endpoint` / Gist `endpoint` pointing at the mock.

// Sending tests need the `http` feature (on in `cargo test --workspace`,
// which unifies the CLI's and app's features); previews run without it.
#![cfg_attr(not(feature = "http"), allow(dead_code, unused_imports))]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::Once;
use std::thread;

use pflow_native_core::Session;
use serde_json::{json, Value};

const PFLOW_TOKEN: &str = "fake-pflow-token-4f2a";
const SIM_TOKEN: &str = "fake-sim-token-9c1d";
const GH_TOKEN: &str = "fake-github-token-77e0";

/// Fake credentials, the same in every test (tests share the process env).
fn creds() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        std::env::set_var("PFLOW_XYZ_TOKEN", PFLOW_TOKEN);
        std::env::set_var("SIM_PFLOW_TOKEN", SIM_TOKEN);
        std::env::set_var("GITHUB_TOKEN", GH_TOKEN);
        // The mock server is on loopback; credentials otherwise only go to
        // their real HTTPS hosts (outbound::credential_hosts).
        std::env::set_var("PFLOW_NATIVE_CREDENTIAL_HOSTS", "127.0.0.1");
        std::env::set_var(
            "PFLOW_NATIVE_HOME",
            std::env::temp_dir().join("pflow-native-remote-test-none"),
        );
    });
}

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn sir() -> Session {
    creds();
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    s
}

fn assert_no_secrets(v: &Value) {
    let text = v.to_string();
    for t in [PFLOW_TOKEN, SIM_TOKEN, GH_TOKEN] {
        assert!(!text.contains(t), "credential leaked: {text}");
    }
}

// ---------------------------------------------------------------- mock HTTP

#[derive(Debug, Clone)]
struct Req {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl Req {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
    fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

struct Reply {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: String,
}

fn reply(status: u16, content_type: &str, body: impl Into<String>) -> Reply {
    Reply {
        status,
        headers: vec![("Content-Type", content_type.to_string())],
        body: body.into(),
    }
}

/// Serves `n` requests on 127.0.0.1 with `handler`, then stops. Returns the
/// base URL and a receiver of the recorded requests.
fn mock(
    n: usize,
    handler: impl Fn(&Req) -> Reply + Send + 'static,
) -> (String, mpsc::Receiver<Req>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for _ in 0..n {
            let (stream, _) = listener.accept().unwrap();
            let mut r = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            r.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let method = parts.next().unwrap_or("").to_string();
            let path = parts.next().unwrap_or("").to_string();
            let mut headers = vec![];
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                r.read_line(&mut h).unwrap();
                let h = h.trim_end();
                if h.is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    let (k, v) = (k.trim().to_string(), v.trim().to_string());
                    if k.eq_ignore_ascii_case("content-length") {
                        len = v.parse().unwrap();
                    }
                    headers.push((k, v));
                }
            }
            let mut body = vec![0u8; len];
            r.read_exact(&mut body).unwrap();
            let req = Req {
                method,
                path,
                headers,
                body: String::from_utf8(body).unwrap(),
            };
            let rep = handler(&req);
            tx.send(req).unwrap();
            let mut out = stream;
            let mut head = format!(
                "HTTP/1.1 {} X\r\nContent-Length: {}\r\nConnection: close\r\n",
                rep.status,
                rep.body.len()
            );
            for (k, v) in &rep.headers {
                head.push_str(&format!("{k}: {v}\r\n"));
            }
            head.push_str("\r\n");
            out.write_all(head.as_bytes()).unwrap();
            out.write_all(rep.body.as_bytes()).unwrap();
            out.flush().unwrap();
        }
    });
    (base, rx)
}

// ---------------------------------------------------------------- pflow.xyz

#[test]
fn save_previews_and_redacts() {
    let mut s = sir();
    let cid = run(&mut s, "file.cid", Value::Null)["cid"].clone();
    let p = run(&mut s, "remote.pflow.save", Value::Null);
    assert_eq!(p["sent"], false);
    assert_eq!(p["preview"]["method"], "POST");
    assert_eq!(p["preview"]["url"], "https://pflow.xyz/api/save");
    assert_eq!(p["preview"]["body"], s.doc.to_value());
    assert_eq!(p["expected_cid"], cid);
    assert_eq!(p["credential"], "PFLOW_XYZ_TOKEN");
    let auth = p["preview"]["headers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h[0] == "Authorization")
        .unwrap();
    assert_eq!(auth[1], "<redacted>");
    assert_no_secrets(&p);
    assert!(
        s.doc.extra.get("@id").is_none(),
        "a preview changes nothing"
    );
    assert!(
        s.journal.iter().any(|st| st.command == "remote.pflow.save"),
        "writes are journaled"
    );
    assert_no_secrets(&serde_json::to_value(&s.journal).unwrap());
}

#[cfg(feature = "http")]
#[test]
fn save_sends_to_mock_and_records_cid() {
    let mut s = sir();
    let cid = run(&mut s, "file.cid", Value::Null)["cid"]
        .as_str()
        .unwrap()
        .to_string();
    let answer = cid.clone();
    let (base, rx) = mock(1, move |_| {
        reply(200, "application/json", json!({"cid": answer}).to_string())
    });
    let r = run(
        &mut s,
        "remote.pflow.save",
        json!({"server": base, "send": true}),
    );
    let req = rx.recv().unwrap();
    assert_eq!(
        (req.method.as_str(), req.path.as_str()),
        ("POST", "/api/save")
    );
    assert_eq!(
        req.header("Authorization"),
        Some(&*format!("Bearer {PFLOW_TOKEN}"))
    );
    assert_eq!(req.json()["places"], s.doc.to_value()["places"]);
    assert_eq!(r["sent"], true);
    assert_eq!(r["cid"], cid);
    assert_eq!(r["matches"], true);
    assert_eq!(s.doc.extra["@id"], cid, "the returned CID is recorded");
    assert_eq!(r["links"]["page"], format!("{base}/?cid={cid}"));
    assert_no_secrets(&r);
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.doc.extra.get("@id").is_none(), "recording is undoable");
}

#[cfg(feature = "http")]
#[test]
fn list_reads_my_diagrams_into_a_table() {
    let mut s = sir();
    let (base, rx) = mock(1, |_| {
        reply(
            200,
            "application/json",
            json!({"diagrams": [
                {"cid": "z4EBa", "name": "SIR", "createdAt": "2026-10-01T00:00:00Z", "authorId": "7", "authorName": "M"},
                {"cid": "z4EBb", "description": "queue", "createdAt": "2026-09-01T00:00:00Z"},
            ]})
            .to_string(),
        )
    });
    // It sends a credential, so without `send` it is only a preview.
    let p = run(&mut s, "remote.pflow.list", json!({"server": base}));
    assert_eq!(p["sent"], false, "{p}");
    assert_no_secrets(&p);
    assert!(
        rx.try_recv().is_err(),
        "a preview must not reach the server"
    );
    let r = run(
        &mut s,
        "remote.pflow.list",
        json!({"server": base, "send": true}),
    );
    let req = rx.recv().unwrap();
    assert_eq!(
        (req.method.as_str(), req.path.as_str()),
        ("GET", "/api/diagrams")
    );
    assert_eq!(
        req.header("Authorization"),
        Some(&*format!("Bearer {PFLOW_TOKEN}"))
    );
    assert_eq!(r["count"], 2);
    let art = s.artifact(r["artifact"].as_str().unwrap()).unwrap();
    let data = serde_json::to_value(&art.data).unwrap();
    assert!(data.to_string().contains("z4EBb"), "{data}");
    assert_no_secrets(&r);
    assert_no_secrets(&data);
}

#[test]
fn delete_previews_then_sends() {
    let mut s = sir();
    let p = run(&mut s, "remote.pflow.delete", json!({"cid": "z4EBabc"}));
    assert_eq!(p["sent"], false);
    assert_eq!(p["preview"]["method"], "DELETE");
    assert_eq!(p["preview"]["url"], "https://pflow.xyz/o/z4EBabc");
    assert_no_secrets(&p);
    assert!(s
        .execute("remote.pflow.delete", json!({"cid": "../etc"}))
        .is_err());

    if !cfg!(feature = "http") {
        return;
    }
    let (base, rx) = mock(1, |_| reply(204, "text/plain", ""));
    let r = run(
        &mut s,
        "remote.pflow.delete",
        json!({"cid": "z4EBabc", "server": base, "send": true}),
    );
    let req = rx.recv().unwrap();
    assert_eq!(
        (req.method.as_str(), req.path.as_str()),
        ("DELETE", "/o/z4EBabc")
    );
    assert_eq!(
        req.header("Authorization"),
        Some(&*format!("Bearer {PFLOW_TOKEN}"))
    );
    assert_eq!(r["sent"], true);
}

#[test]
fn share_links_match_pflow_xyz() {
    let mut s = sir();
    // A real pflow.xyz example CID (tests/fixtures/cid).
    let cid = "z4EBG9j4KsBV5WWdHXkpBM6jKMEm13ZiW53oXY5BpYxi2SxWAJj";
    let r = run(&mut s, "remote.pflow.share", json!({"cid": cid}));
    let svg = format!("https://pflow.xyz/img/{cid}.svg");
    let page = format!("https://pflow.xyz/?cid={cid}");
    assert_eq!(r["svg"], svg);
    assert_eq!(r["page"], page);
    assert_eq!(r["markdown"], format!("[![pflow]({svg})]({page})"));
    assert_eq!(r["jsonld"], format!("https://pflow.xyz/o/{cid}"));
    // No cid: the open document's.
    let own = run(&mut s, "file.cid", Value::Null)["cid"].clone();
    let r = run(
        &mut s,
        "remote.pflow.share",
        json!({"server": "http://localhost:8083/"}),
    );
    assert_eq!(r["cid"], own);
    assert_eq!(
        r["svg"],
        format!("http://localhost:8083/img/{}.svg", own.as_str().unwrap())
    );
}

#[test]
fn gist_payload_and_send() {
    let mut s = sir();
    let cid = run(&mut s, "file.cid", Value::Null)["cid"]
        .as_str()
        .unwrap()
        .to_string();
    let p = run(&mut s, "remote.gist", Value::Null);
    assert_eq!(p["sent"], false);
    assert_eq!(p["preview"]["url"], "https://api.github.com/gists");
    let body = &p["preview"]["body"];
    assert_eq!(body["description"], "pflow Petri net diagram");
    assert_eq!(body["public"], true);
    let md = format!("[![pflow](https://pflow.xyz/img/{cid}.svg)](https://pflow.xyz/?cid={cid})");
    assert_eq!(body["files"][format!("{cid}.md")]["content"], md);
    let ld: Value = serde_json::from_str(
        body["files"][format!("{cid}.jsonld")]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(ld["@id"], cid);
    assert_eq!(ld["places"], s.doc.to_value()["places"]);
    assert_eq!(p["credential"], "GITHUB_TOKEN");
    assert_no_secrets(&p);

    let p = run(
        &mut s,
        "remote.gist",
        json!({"public": false, "description": "mine", "jsonld": false}),
    );
    assert_eq!(p["preview"]["body"]["public"], false);
    assert_eq!(p["preview"]["body"]["description"], "mine");
    assert_eq!(p["preview"]["body"]["files"].as_object().unwrap().len(), 1);
    if !cfg!(feature = "http") {
        return;
    }

    let (base, rx) = mock(1, |_| {
        reply(
            201,
            "application/json",
            json!({"html_url": "https://gist.github.com/x/1"}).to_string(),
        )
    });
    let r = run(
        &mut s,
        "remote.gist",
        json!({"endpoint": format!("{base}/gists"), "send": true}),
    );
    let req = rx.recv().unwrap();
    assert_eq!((req.method.as_str(), req.path.as_str()), ("POST", "/gists"));
    assert_eq!(
        req.header("Authorization"),
        Some(&*format!("Bearer {GH_TOKEN}"))
    );
    assert_eq!(req.header("Accept"), Some("application/vnd.github.v3+json"));
    assert_eq!(req.json()["files"][format!("{cid}.md")]["content"], md);
    assert_eq!(r["url"], "https://gist.github.com/x/1");
}

// ---------------------------------------------------------------- sim.pflow.xyz

const SESSION: &str = "sess-123";

fn rpc_ok(id: &Value, result: Value) -> String {
    json!({"jsonrpc": "2.0", "id": id, "result": result}).to_string()
}

fn text_result(v: Value) -> Value {
    json!({"content": [{"type": "text", "text": v.to_string()}]})
}

/// An MCP server: initialize (JSON, assigns the session), initialized (202),
/// tools/list (SSE), tools/call via `call`.
fn mcp_mock(
    n: usize,
    call: impl Fn(&str, &Value) -> Value + Send + 'static,
) -> (String, mpsc::Receiver<Req>) {
    let (base, rx) = mock(n, move |req| {
        let m = req.json();
        let id = m["id"].clone();
        let method = m["method"].as_str().unwrap_or("");
        if method != "initialize" && req.header("Mcp-Session-Id") != Some(SESSION) {
            return reply(400, "text/plain", "Invalid session ID");
        }
        match method {
            "initialize" => {
                let mut r = reply(
                    200,
                    "application/json",
                    rpc_ok(
                        &id,
                        json!({"protocolVersion": "2025-06-18", "capabilities": {"tools": {}},
                               "serverInfo": {"name": "sim-mock", "version": "0"}}),
                    ),
                );
                r.headers.push(("Mcp-Session-Id", SESSION.to_string()));
                r
            }
            "notifications/initialized" => reply(202, "text/plain", ""),
            "tools/list" => reply(
                200,
                "text/event-stream",
                format!(
                    "event: message\ndata: {}\n\n",
                    rpc_ok(
                        &id,
                        json!({"tools": [
                            {"name": "sim_create_model", "description": "Store a model. More.", "inputSchema": {"type": "object", "required": ["model"]}},
                            {"name": "sim_prove", "description": "Prove a relation.", "inputSchema": {"type": "object"}},
                        ]})
                    )
                ),
            ),
            "tools/call" => {
                let name = m["params"]["name"].as_str().unwrap_or("");
                let out = call(name, &m["params"]["arguments"]);
                if out.get("jsonrpc_error").is_some() {
                    return reply(
                        200,
                        "application/json",
                        json!({"jsonrpc": "2.0", "id": id, "error": out["jsonrpc_error"]})
                            .to_string(),
                    );
                }
                reply(200, "application/json", rpc_ok(&id, out))
            }
            _ => reply(404, "text/plain", "no"),
        }
    });
    (base, rx)
}

#[cfg(feature = "http")]
#[test]
fn sim_tools_round_trip_with_sse() {
    let mut s = sir();
    let (base, rx) = mcp_mock(3, |_, _| Value::Null);
    let r = run(
        &mut s,
        "remote.sim.tools",
        json!({"endpoint": format!("{base}/mcp")}),
    );
    assert_eq!(r["count"], 2);
    assert_eq!(r["server"]["name"], "sim-mock");
    let reqs: Vec<Req> = rx.iter().take(3).collect();
    assert_eq!(reqs[0].json()["method"], "initialize");
    assert_eq!(reqs[1].json()["method"], "notifications/initialized");
    assert_eq!(reqs[2].json()["method"], "tools/list");
    assert!(reqs.iter().all(|q| q.path == "/mcp"));
    assert_eq!(reqs[2].header("Mcp-Session-Id"), Some(SESSION));
    assert_eq!(reqs[2].header("MCP-Protocol-Version"), Some("2025-06-18"));
    assert!(
        reqs.iter().all(|q| q.header("Authorization").is_none()),
        "discovery sends no token"
    );
    assert!(reqs[0]
        .header("Accept")
        .unwrap()
        .contains("text/event-stream"));
}

#[test]
fn sim_call_previews_by_default() {
    let mut s = sir();
    let p = run(
        &mut s,
        "remote.sim.call",
        json!({"tool": "sim_code_to_flow", "args": {"code": "x"}}),
    );
    assert_eq!(p["sent"], false);
    assert_eq!(p["preview"]["url"], "https://sim.pflow.xyz/mcp");
    assert_eq!(p["preview"]["body"]["method"], "tools/call");
    assert_eq!(p["preview"]["body"]["params"]["name"], "sim_code_to_flow");
    assert_eq!(p["preview"]["body"]["params"]["arguments"]["code"], "x");
    assert_eq!(p["credential"], "SIM_PFLOW_TOKEN");
    assert_no_secrets(&p);

    let p = run(&mut s, "remote.sim.create_model", Value::Null);
    let model: Value =
        serde_json::from_str(p["steps"][0]["arguments"]["model"].as_str().unwrap()).unwrap();
    assert_eq!(model["places"].as_array().unwrap().len(), 3);
    assert!(model["arcs"][0].get("from").is_some(), "go-pflow arc shape");

    // A wrapper on "the open net" stores it first, in the same session.
    let p = run(
        &mut s,
        "remote.sim.publish",
        json!({"scenario": {"hours": 4}}),
    );
    let steps = p["steps"].as_array().unwrap();
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0]["tool"], "sim_create_model");
    assert_eq!(steps[1]["tool"], "sim_publish");
    assert_eq!(steps[1]["arguments"]["scenario"]["hours"], 4);
    let p = run(
        &mut s,
        "remote.sim.prove",
        json!({"subject": "m1", "predicate": "subnetOf", "object": "m2"}),
    );
    assert_eq!(p["steps"].as_array().unwrap().len(), 1);
    assert_eq!(p["preview"]["body"]["params"]["arguments"]["object"], "m2");
    assert!(s.execute("remote.sim.check_witness", json!({})).is_err());
    assert_no_secrets(&p);
}

#[cfg(feature = "http")]
#[test]
fn sim_publish_runs_two_steps_in_one_session() {
    let mut s = sir();
    s.rates.insert("infect".into(), 0.3);
    let (base, rx) = mcp_mock(4, |name, args| match name {
        "sim_create_model" => text_result(json!({"id": "m-sir", "url": "/api/models/m-sir"})),
        "sim_publish" => {
            assert_eq!(args["id"], "m-sir");
            text_result(json!({"url": "https://docs.google.com/spreadsheets/d/x"}))
        }
        _ => Value::Null,
    });
    let r = run(
        &mut s,
        "remote.sim.publish",
        json!({"endpoint": format!("{base}/mcp"), "send": true}),
    );
    assert_eq!(r["sent"], true);
    assert_eq!(r["stored_model"], "m-sir");
    assert_eq!(
        r["result"]["url"],
        "https://docs.google.com/spreadsheets/d/x"
    );
    let reqs: Vec<Req> = rx.iter().take(4).collect();
    let create = reqs[2].json();
    assert_eq!(create["params"]["name"], "sim_create_model");
    let model: Value =
        serde_json::from_str(create["params"]["arguments"]["model"].as_str().unwrap()).unwrap();
    let infect = model["transitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "infect")
        .unwrap();
    assert_eq!(infect["rate"], 0.3, "session rates ride along");
    for q in &reqs[2..] {
        assert_eq!(
            q.header("Authorization"),
            Some(&*format!("Bearer {SIM_TOKEN}"))
        );
        assert_eq!(q.header("Mcp-Session-Id"), Some(SESSION));
    }
    assert!(r["artifact"].is_string());
    assert_no_secrets(&r);
}

#[cfg(feature = "http")]
#[test]
fn sim_errors_surface() {
    let mut s = sir();
    // A JSON-RPC error.
    let (base, _rx) = mcp_mock(
        3,
        |_, _| json!({"jsonrpc_error": {"code": -32602, "message": "unknown tool sim_nope"}}),
    );
    let e = s
        .execute(
            "remote.sim.call",
            json!({"tool": "sim_nope", "endpoint": format!("{base}/mcp"), "send": true}),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("-32602") && e.contains("unknown tool"), "{e}");
    // A tool error and an auth challenge.
    let (base, _rx) = mcp_mock(3, |_, _| {
        json!({"isError": true, "content": [{"type": "text", "text": "Sign in to create and manage simulation models."}],
               "_meta": {"mcp/www_authenticate": ["Bearer resource_metadata=\"https://sim.pflow.xyz/.well-known/oauth-protected-resource/mcp\", error=\"insufficient_scope\", scope=\"sim.models.write\""]}})
    });
    let e = s
        .execute(
            "remote.sim.create_model",
            json!({"endpoint": format!("{base}/mcp"), "send": true}),
        )
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("SIM_PFLOW_TOKEN") && e.contains("sim.models.write"),
        "{e}"
    );
    assert!(!e.contains(SIM_TOKEN));
    // An HTTP error.
    let (base, _rx) = mock(1, |_| reply(500, "text/plain", "boom"));
    let e = s
        .execute(
            "remote.sim.tools",
            json!({"endpoint": format!("{base}/mcp")}),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("HTTP 500") && e.contains("boom"), "{e}");
}

#[cfg(feature = "http")]
#[test]
fn sim_prompt_opens_the_derived_model() {
    let mut s = sir();
    let (base, _rx) = mcp_mock(5, |name, args| match name {
        "sim_prompt" => {
            assert_eq!(args["parent"], "m-parent");
            assert_eq!(args["text"], "add a vaccinated compartment");
            text_result(
                json!({"promptId": "p1", "id": "m-child", "kind": "model", "url": "/api/models/m-child"}),
            )
        }
        "sim_get_model" => {
            assert_eq!(args["id"], "m-child");
            text_result(json!({
                "name": "SIRV",
                "places": [{"id": "S", "initial": 99}, {"id": "I", "initial": 1}, {"id": "R", "initial": 0}, {"id": "V", "initial": 0}],
                "transitions": [{"id": "infect", "rate": 0.3}, {"id": "recover", "rate": 0.1}, {"id": "vaccinate"}],
                "arcs": [
                    {"from": "S", "to": "infect"}, {"from": "I", "to": "infect"},
                    {"from": "infect", "to": "I", "weight": 2},
                    {"from": "I", "to": "recover"}, {"from": "recover", "to": "R"},
                    {"from": "S", "to": "vaccinate"}, {"from": "vaccinate", "to": "V"},
                ],
            }))
        }
        _ => Value::Null,
    });
    let r = run(
        &mut s,
        "remote.sim.prompt",
        json!({"text": "add a vaccinated compartment", "parent": "m-parent",
               "endpoint": format!("{base}/mcp"), "send": true, "open": true}),
    );
    assert_eq!(r["opened"]["places"], 4);
    assert_eq!(s.doc.places.len(), 4);
    assert_eq!(s.doc.transitions.len(), 3);
    assert_eq!(s.rates.get("infect"), Some(&0.3));
    assert!(
        s.doc.places.iter().any(|p| p.x != 0.0 || p.y != 0.0),
        "an unplaced model is laid out"
    );
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.doc.places.len(), 3, "opening is undoable");
}

#[test]
fn sim_call_refuses_uploading_local_data() {
    let mut s = sir();
    for send in [false, true] {
        let e = s
            .execute(
                "remote.sim.call",
                json!({"tool": "sim_map_put", "args": {"data": "{\"a\":1}"},
                       "endpoint": "http://127.0.0.1:9/mcp", "send": send}),
            )
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("sim_map_put")
                && e.contains("Policy")
                && e.contains("computed on sim.pflow.xyz"),
            "{e}"
        );
    }
    // A log as the input of a server computation is allowed (preview).
    let p = run(
        &mut s,
        "remote.sim.call",
        json!({"tool": "sim_conformance", "args": {"id": "m1", "log": "case_id,activity,timestamp\n"}}),
    );
    assert_eq!(p["sent"], false);
}

/// A caller-chosen server or endpoint never receives a credential: the
/// command refuses before any request is built or sent (preview or send).
#[test]
fn credentials_never_go_to_a_caller_chosen_host() {
    let mut s = sir();
    let evil = "https://attacker.example";
    for (cmd, args) in [
        ("remote.pflow.list", json!({"server": evil, "send": true})),
        ("remote.pflow.list", json!({"server": evil})),
        ("remote.pflow.save", json!({"server": evil, "send": true})),
        (
            "remote.pflow.delete",
            json!({"cid": "z4EBabc", "server": evil, "send": true}),
        ),
        (
            "remote.gist",
            json!({"endpoint": format!("{evil}/gists"), "send": true}),
        ),
        (
            "remote.sim.call",
            json!({"tool": "sim_list_models", "endpoint": format!("{evil}/mcp"), "send": true}),
        ),
        (
            "remote.sim.call",
            json!({"tool": "sim_list_models", "endpoint": "http://localhost:9/mcp"}),
        ),
        (
            "remote.pflow.list",
            json!({"server": "https://pflow.xyz@attacker.example", "send": true}),
        ),
    ] {
        let e = s
            .execute(cmd, args.clone())
            .expect_err(&format!("{cmd} {args}"))
            .to_string();
        assert!(e.contains("refusing to send"), "{cmd}: {e}");
        for t in [PFLOW_TOKEN, SIM_TOKEN, GH_TOKEN] {
            assert!(!e.contains(t), "credential in error: {e}");
        }
    }
}

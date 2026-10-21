//! Drives the real binary over stdio, as an MCP client would.

use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use serde_json::{json, Value};

#[test]
fn mcp_session_builds_fires_and_simulates() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_pflow-native-cli"))
        .args(["mcp", "--headless"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let mut call = |req: Value| -> Value {
        writeln!(stdin, "{req}").unwrap();
        let mut line = String::new();
        out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap()
    };
    let init = call(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}));
    assert_eq!(init["result"]["serverInfo"]["name"], "pflow-native");
    let tools = call(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "list_commands",
            "execute",
            "batch",
            "screenshot",
            "get_state"
        ]
    );

    let tool = |name: &str, args: Value, id: u64| json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":args}});
    let r = call(tool(
        "batch",
        json!({"steps": [
            {"command":"net.add_place","args":{"x":0,"y":0,"initial":1}},
            {"command":"net.add_transition","args":{"x":100,"y":0}},
            {"command":"net.add_place","args":{"x":200,"y":0}},
            {"command":"net.add_arc","args":{"source":"p0","target":"t0"}},
            {"command":"net.add_arc","args":{"source":"t0","target":"p1"}}
        ]}),
        3,
    ));
    assert_eq!(r["result"]["isError"], false, "{r}");
    let r = call(tool(
        "execute",
        json!({"command":"sim.fire","args":{"id":"t0"}}),
        4,
    ));
    assert_eq!(r["result"]["isError"], false, "{r}");
    let r = call(tool(
        "execute",
        json!({"command":"sim.fire","args":{"id":"t0"}}),
        5,
    ));
    assert_eq!(
        r["result"]["isError"], true,
        "second firing has no token: {r}"
    );
    let r = call(tool(
        "execute",
        json!({"command":"sim.ode","args":{"t_end":1}}),
        6,
    ));
    assert_eq!(r["result"]["isError"], false, "{r}");
    let r = call(tool("list_commands", json!({"prefix":"macro."}), 7));
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("macro.play"));
    let r = call(tool(
        "execute",
        json!({"command":"view.screenshot","args":{"path":"x.png"}}),
        8,
    ));
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("no app window"),
        "{r}"
    );
    drop(stdin);
    child.wait().unwrap();
}

/// A stand-in for the app's control port: answers like the app does, but
/// only for requests carrying `token`; records what it saw.
fn fake_app(token: &'static str) -> (String, std::sync::mpsc::Receiver<Value>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let tx = tx.clone();
            std::thread::spawn(move || {
                let mut w = stream.try_clone().unwrap();
                for line in BufReader::new(stream).lines() {
                    let Ok(line) = line else { break };
                    let req: Value = serde_json::from_str(&line).unwrap();
                    let _ = tx.send(req.clone());
                    let reply = if req["token"] == token {
                        json!({"id": req["id"], "ok": true, "result": {"echo": req["command"]}})
                    } else {
                        json!({"id": req["id"], "ok": false, "error": "unauthorized"})
                    };
                    writeln!(w, "{reply}").unwrap();
                }
            });
        }
    });
    (addr, rx)
}

#[test]
fn attach_sends_the_control_token() {
    let (addr, seen) = fake_app("tok-123");
    let empty_home = std::env::temp_dir().join(format!("pn-cli-attach-{}", std::process::id()));
    let mut child = Command::new(env!("CARGO_BIN_EXE_pflow-native-cli"))
        .args(["mcp", "--attach", &addr])
        .env("PFLOW_NATIVE_CONTROL_TOKEN", "tok-123")
        .env("PFLOW_NATIVE_HOME", &empty_home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut out = BufReader::new(child.stdout.take().unwrap());
    let call = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"execute","arguments":{"command":"net.info"}}});
    writeln!(stdin, "{call}").unwrap();
    let mut line = String::new();
    out.read_line(&mut line).unwrap();
    let r: Value = serde_json::from_str(&line).unwrap();
    assert_eq!(r["result"]["isError"], false, "{r}");
    let req = seen.recv().unwrap();
    assert_eq!(req["token"], "tok-123");
    assert_eq!(req["command"], "net.info");
    drop(stdin);
    child.wait().unwrap();
}

#[test]
fn attach_without_a_token_refuses() {
    let (addr, _) = fake_app("tok-123");
    let empty_home = std::env::temp_dir().join(format!("pn-cli-notoken-{}", std::process::id()));
    let out = Command::new(env!("CARGO_BIN_EXE_pflow-native-cli"))
        .args(["mcp", "--attach", &addr])
        .env_remove("PFLOW_NATIVE_CONTROL_TOKEN")
        .env("PFLOW_NATIVE_HOME", &empty_home)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no control token"));
}

#[test]
fn control_saves_with_o_when_stdin_closes() {
    let dir = std::env::temp_dir().join(format!("pn-cli-o-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.json");
    let mut child = Command::new(env!("CARGO_BIN_EXE_pflow-native-cli"))
        .args(["control", "-o", path.to_str().unwrap()])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    writeln!(
        child.stdin.take().unwrap(),
        r#"{{"id":1,"command":"net.add_place","args":{{"x":10,"y":10}}}}"#
    )
    .unwrap();
    assert!(child.wait().unwrap().success());
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("\"p0\""), "{saved}");
    std::fs::remove_dir_all(dir).ok();
}

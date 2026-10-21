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

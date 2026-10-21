use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn load_csv_and_jsonl_logs() {
    let mut s = Session::default();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/logs/coffee-orders.csv"
    );
    let r = run(&mut s, "data.load_log", json!({ "path": path }));
    assert_eq!(r["summary"]["cases"], 4);
    assert_eq!(r["summary"]["events"], 12);
    let id = r["artifact"].as_str().unwrap().to_string();
    let info = run(&mut s, "data.log_info", json!({ "log": id }));
    assert_eq!(info["variants"], 3);

    let jsonl = "{\"case_id\":\"a\",\"activity\":\"x\",\"timestamp\":\"2026-10-01T08:00:00Z\"}\n\
                 {\"case_id\":\"a\",\"activity\":\"y\",\"timestamp\":\"2026-10-01T08:01:00Z\"}\n";
    let r = run(
        &mut s,
        "data.load_log",
        json!({ "text": jsonl, "format": "jsonl" }),
    );
    assert_eq!(r["summary"]["events"], 2);
    assert!(
        s.execute("data.load_log", json!({ "text": "a,b\n1,2\n" }))
            .is_err(),
        "wrong columns"
    );

    // A table put by an agent works as a log too (time optional).
    let t = json!({"kind":"table","columns":["case","activity"],"rows":[["1","a"],["1","b"],["2","a"]]});
    let id = run(&mut s, "artifact.put", json!({"title":"log","data":t}))["artifact"].clone();
    assert_eq!(
        run(&mut s, "data.log_info", json!({ "log": id }))["cases"],
        2
    );
}

#[test]
fn exported_log_csv_reimports_and_conforms() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let g = run(&mut s, "data.generate", json!({"cases": 20, "seed": 3, "max_steps": 30}));
    let id = g["artifact"].as_str().unwrap().to_string();
    let csv = run(&mut s, "export.artifact", json!({"id": id, "format": "csv"}));
    let csv = csv.as_str().unwrap();
    assert!(csv.starts_with("case,activity,time"), "{}", &csv[..40]);
    let r = run(&mut s, "data.load_log", json!({"text": csv}));
    assert_eq!(r["summary"]["events"], g["summary"]["events"]);
    let a = run(&mut s, "data.conformance", json!({"log": r["artifact"]}));
    let b = run(&mut s, "data.conformance", json!({"log": id}));
    assert_eq!(a["fitness"], b["fitness"], "re-imported log replays identically");
}

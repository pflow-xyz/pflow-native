//! The library's model content (docs/composition-documents.md §3, §7): it is
//! sound against the model it is about, and every check that backs a finding
//! runs on the pinned pflow-rs and passes.

use pflow_native_core::store::{self, Store};
use pflow_native_core::Session;
use serde_json::{json, Value};
use std::sync::Arc;

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn refused(s: &mut Session, id: &str, args: Value) -> String {
    match s.execute(id, args) {
        Ok(v) => panic!("{id} should be refused, got {v}"),
        Err(e) => e.to_string(),
    }
}

fn session() -> Session {
    store::set_thread_store(Some(Arc::new(Store::memory())));
    Session::default()
}

fn library(s: &mut Session, id: &str) -> Value {
    run(s, "content.library", Value::Null)["library"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == id)
        .unwrap_or_else(|| panic!("no library content {id}"))
        .clone()
}

fn all_checks_pass(report: &Value, expected: u64) {
    // Each failure as its id, claim and the steps or expectations that
    // were not ok, so a red run names what broke.
    let failed: Vec<Value> = report["checks"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["ok"] != json!(true))
        .map(|c| {
            let bad: Vec<Value> = c["steps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|st| {
                    let expect: Vec<&Value> = st["expect"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|e| e["ok"] != json!(true))
                        .collect();
                    (st["ok"] != json!(true) || !expect.is_empty()).then(|| {
                        json!({"command": st["command"], "ok": st["ok"], "error": st["error"], "expect": expect})
                    })
                })
                .collect();
            json!({"id": c["id"], "claim": c["claim"], "bad_steps": bad})
        })
        .collect();
    assert!(failed.is_empty(), "checks that failed: {:#}", json!(failed));
    assert_eq!(report["failed"], 0, "{report}");
    assert_eq!(report["total"], expected, "every check in the entry is run");
    assert_eq!(report["passed"], expected, "{report}");
}

#[test]
fn the_orders_library_is_sound_against_its_composition_and_every_check_passes() {
    let mut s = session();
    let lib = library(&mut s, "orders");
    run(&mut s, "bundle.open", json!({}));

    let v = run(&mut s, "content.validate", Value::Null);
    assert_eq!(v["valid"], true, "{v}");
    assert_eq!(v["entries"], lib["entries"]);

    let report = run(&mut s, "content.check", Value::Null);
    all_checks_pass(&report, lib["checks"].as_u64().unwrap());
    assert!(report["total"].as_u64().unwrap() > 0);
}

#[test]
fn the_connect4_library_is_sound_against_the_plan_and_every_check_passes() {
    let mut s = session();
    let lib = library(&mut s, "connect4");
    run(&mut s, "plan.open", json!({}));

    let v = run(&mut s, "content.validate", json!({"subject": "plan"}));
    assert_eq!(v["valid"], true, "{v}");
    assert_eq!(v["entries"], lib["entries"]);

    let report = run(&mut s, "content.check", json!({"subject": "plan"}));
    all_checks_pass(&report, lib["checks"].as_u64().unwrap());
    assert!(report["total"].as_u64().unwrap() > 0);
}

#[test]
fn content_without_a_composition_is_refused() {
    let mut s = session();
    let e = refused(&mut s, "content.get", Value::Null);
    assert!(e.contains("no composition is open"), "{e}");
    let e = refused(&mut s, "content.load", json!({"library": "orders"}));
    assert!(e.contains("no composition is open"), "{e}");
}

#[test]
fn the_declared_scenarios_come_from_the_library_content() {
    let mut s = session();
    let out = run(&mut s, "bundle.scenarios", Value::Null);
    assert_eq!(out["source"], "library:orders", "{out}");
    let ids: Vec<&str> = out["scenarios"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["commit", "refund", "charge-fails", "second-order"]);
}

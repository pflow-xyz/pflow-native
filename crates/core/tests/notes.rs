//! view.focus and note.*: what the person sees, and notes pinned for them.

use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn sir() -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    s
}

#[test]
fn focus_describes_the_selection_and_what_the_app_reported() {
    let mut s = sir();
    run(&mut s, "edit.select", json!({"ids": ["S", "infect"]}));
    let f = run(&mut s, "view.focus", json!({}));
    assert_eq!(f["front_end"], "headless");
    assert!(f.get("open_result").is_none());
    assert_eq!(f["selection"][0]["kind"], "place");
    assert!(f["selection"][0]["tokens"].is_array());
    assert_eq!(f["selection"][1]["kind"], "transition");
    assert_eq!(f["selection"][1]["enabled"], true);
    let ode = run(&mut s, "sim.ode", json!({}));
    run(
        &mut s,
        "view.report_focus",
        json!({"artifact": ode["artifact"], "tab": "results", "window_visible": false}),
    );
    let f = run(&mut s, "view.focus", json!({}));
    assert_eq!(f["front_end"], "app");
    assert_eq!(f["window_visible"], false);
    assert_eq!(f["open_result"]["id"], ode["artifact"]);
    assert_eq!(f["open_result"]["kind"], "timeseries");
    assert_eq!(f["recent_commands"][0], "sim.ode");
}

#[test]
fn notes_pin_to_nodes_results_points_and_the_net() {
    let mut s = sir();
    let ode = run(&mut s, "sim.ode", json!({}));
    let a = run(
        &mut s,
        "note.add",
        json!({"node": "infect", "text": "the epidemic's engine"}),
    );
    assert_eq!(a["target"]["node"], "infect");
    assert_eq!(a["author"], "agent");
    run(
        &mut s,
        "note.add",
        json!({"artifact": ode["artifact"], "text": "peak near t = 5"}),
    );
    run(
        &mut s,
        "note.add",
        json!({"x": 10, "y": 20, "text": "empty corner", "author": "matt"}),
    );
    run(&mut s, "note.add", json!({"text": "an SIR model"}));
    assert_eq!(
        run(&mut s, "note.list", json!({}))
            .as_array()
            .unwrap()
            .len(),
        4
    );
    let f = run(&mut s, "view.focus", json!({}));
    assert_eq!(f["notes"].as_array().unwrap().len(), 4);
    // A deleted node orphans its note; it is listed, not lost.
    run(&mut s, "edit.delete", json!({"ids": ["infect"]}));
    let l = run(&mut s, "note.list", json!({}));
    assert_eq!(l[0]["orphaned"], true);
    // Clearing one author's notes leaves the rest.
    assert_eq!(
        run(&mut s, "note.clear", json!({"author": "matt"}))["deleted"],
        1
    );
    run(&mut s, "note.delete", json!({"id": a["id"]}));
    assert_eq!(
        run(&mut s, "note.list", json!({}))
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // Notes are not part of the document.
    let doc = run(&mut s, "file.export_json", json!({})).to_string();
    assert!(!doc.contains("peak near"));
}

#[test]
fn bad_notes_are_refused() {
    let mut s = sir();
    for bad in [
        json!({"text": ""}),
        json!({"text": "x", "node": "nowhere"}),
        json!({"text": "x", "artifact": "a99"}),
        json!({"text": "x", "x": 1}),
        json!({"text": "x", "node": "S", "x": 1, "y": 2}),
        json!({"text": "y".repeat(501)}),
    ] {
        assert!(s.execute("note.add", bad.clone()).is_err(), "{bad}");
    }
    assert!(s.execute("note.delete", json!({"id": "n9"})).is_err());
}

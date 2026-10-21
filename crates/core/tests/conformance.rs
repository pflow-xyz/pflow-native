//! data.conformance / data.replay / data.generate / data.discover.

use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn err(s: &mut Session, id: &str, args: Value) -> String {
    match s.execute(id, args) {
        Ok(v) => panic!("{id} should fail, got {v}"),
        Err(e) => e.to_string(),
    }
}

fn coffee_log(s: &mut Session) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/logs/coffee-orders.csv"
    );
    run(s, "data.load_log", json!({ "path": path }))["artifact"].clone()
}

/// A per-case workflow net: a, then b and c in parallel, then d or e.
fn workflow() -> Value {
    let p = |init: u32, x: u32| json!({"initial": [init], "capacity": [0], "x": x, "y": 100});
    let t = |label: &str, x: u32| json!({"label": label, "x": x, "y": 200});
    let arc = |s: &str, t: &str| json!({"source": s, "target": t, "weight": [1]});
    json!({
        "token": ["https://pflow.xyz/tokens/black"],
        "places": {"start": p(1, 0), "p1": p(0, 100), "p2": p(0, 200), "p3": p(0, 300), "p4": p(0, 400), "end": p(0, 500)},
        "transitions": {"a": t("Register", 50), "b": t("Check", 150), "c": t("Pay", 250), "d": t("Ship", 350), "e": t("Cancel", 450)},
        "arcs": [arc("start","a"), arc("a","p1"), arc("a","p2"), arc("p1","b"), arc("p2","c"),
                 arc("b","p3"), arc("c","p4"), arc("p3","d"), arc("p4","d"), arc("p3","e"), arc("p4","e"),
                 arc("d","end"), arc("e","end")]
    })
}

fn table(s: &Session, id: &Value) -> (Vec<String>, Vec<Vec<Value>>) {
    match &s.artifact(id.as_str().unwrap()).unwrap().data {
        ArtifactData::Table { columns, rows } => (columns.clone(), rows.clone()),
        other => panic!("not a table: {}", other.kind()),
    }
}

#[test]
fn coffee_orders_against_the_coffee_shop() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let log = coffee_log(&mut s);
    let c = run(&mut s, "data.conformance", json!({"log": log}));
    assert_eq!(c["total_traces"], 4);
    assert_eq!(c["variants"], 3);
    // Case 4 leaves without its coffee: one missing token, nothing else.
    assert_eq!(c["missing"], 1);
    assert_eq!(c["missing_by_place"], json!({"ServedCoffee": 1}));
    assert_eq!(c["deviations_by_transition"], json!({"CustomerLeaves": 1}));
    assert_eq!(c["residual"]["deviant_cases"], 1);
    assert_eq!(c["residual"]["summary"]["verdict"], "split");
    assert_eq!(c["residual"]["worst_cases"][0]["case"], "4");
    // A resource net keeps beans and cups: remaining tokens pull fitness down.
    let fit = c["fitness"].as_f64().unwrap();
    assert!(fit > 0.0 && fit < 1.0, "{fit}");
    assert!(c["token_fitness"].as_f64().unwrap() < 1.0);
    for k in ["precision", "generalization", "simplicity"] {
        let v = c[k].as_f64().unwrap();
        assert!((0.0..=1.0).contains(&v), "{k} = {v}");
    }

    let (cols, rows) = table(&s, &c["artifacts"]["table"]);
    assert_eq!(cols[0], "variant");
    assert_eq!(rows.len(), 3);
    let bad = rows.iter().find(|r| r[2] == "4").expect("case 4's variant");
    assert_eq!(bad[8], 1, "first deviation at event 1");
    assert_eq!(bad[9], "CustomerLeaves");

    // The residual set highlights the deviation on the net.
    let sets = c["artifacts"]["sets"].clone();
    run(&mut s, "artifact.show_set", json!({"id": sets, "index": 0}));
    let h = s.highlight.clone().unwrap();
    assert!(h.nodes.contains(&"CustomerLeaves".to_string()));
    assert!(h.nodes.contains(&"ServedCoffee".to_string()));

    // Online: the first divergence is the 12th event, repaired by brewing.
    let r = run(&mut s, "data.replay", json!({"log": log}));
    assert_eq!(r["conforming"], false);
    assert_eq!(r["divergences"], 1);
    let d = &r["first_divergence"];
    assert_eq!(d["index"], 11);
    assert_eq!(d["case"], "4");
    assert_eq!(d["kind"], "not_enabled");
    assert_eq!(d["repair"], json!(["BrewCoffee"]));
    assert_eq!(d["places"][0]["node"], "ServedCoffee");
    assert_eq!(d["places"][0]["role"], "input");
    assert!((r["fitness"].as_f64().unwrap() - 11.0 / 12.0).abs() < 1e-12);
    assert_eq!(r["traced_case"], "4");

    // The traced case replays on the canvas: initial marking, then events.
    let trace = r["artifacts"]["trace"].clone();
    let steps = match &s.artifact(trace.as_str().unwrap()).unwrap().data {
        ArtifactData::Trace { steps } => steps.clone(),
        _ => panic!("trace"),
    };
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].transition, "");
    assert_eq!(steps[0].marking["WaitingCustomers"], vec![2, 0, 0]);
    run(&mut s, "sim.goto", json!({"id": trace, "step": 1}));
    assert_eq!(s.marking["WaitingCustomers"], vec![3, 0, 0]);
    run(&mut s, "sim.goto", json!({"id": trace, "step": 2}));
    assert_eq!(s.highlight.as_ref().unwrap().nodes, vec!["CustomerLeaves"]);
    let ts = &s
        .artifact(r["artifacts"]["fitness"].as_str().unwrap())
        .unwrap()
        .data;
    match ts {
        ArtifactData::TimeSeries { x, series, .. } => {
            assert_eq!(x.len(), 12);
            assert_eq!(series[0].values[10], 1.0);
            assert!(series[0].values[11] < 1.0);
        }
        _ => panic!("timeseries"),
    }
}

#[test]
fn a_generated_log_fits_its_own_net() {
    let mut s = Session::default();
    run(&mut s, "file.load_json", json!({"json": workflow()}));
    let g = run(&mut s, "data.generate", json!({"cases": 40, "seed": 7}));
    assert_eq!(g["events"], 160, "every case runs a, b, c and d or e");
    assert_eq!(g["deadlocked"], 40);
    let c = run(&mut s, "data.conformance", json!({"log": g["artifact"]}));
    assert_eq!(c["fitness"], 1.0);
    assert_eq!(c["fitting_percent"], 100.0);
    assert_eq!(c["residual"]["summary"]["verdict"], "agree");
    // Interleavings of b ∥ c and the d | e choice: 4 variants, all observed.
    assert_eq!(c["variants"], 4);
    assert_eq!(c["precision"], 1.0);
    let r = run(&mut s, "data.replay", json!({"log": g["artifact"]}));
    assert_eq!(r["conforming"], true);
    assert_eq!(r["fitness"], 1.0);

    // On the (resource) coffee shop a generated log never lacks a token.
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let g = run(
        &mut s,
        "data.generate",
        json!({"cases": 20, "seed": 3, "max_steps": 30}),
    );
    assert_eq!(g["capped"], 20, "sources keep the shop alive");
    let c = run(&mut s, "data.conformance", json!({"log": g["artifact"]}));
    assert_eq!(c["missing"], 0);
    assert_eq!(c["token_fitness"], 1.0);
    let r = run(&mut s, "data.replay", json!({"log": g["artifact"]}));
    assert_eq!(r["conforming"], true);
}

#[test]
fn an_injected_event_is_found_and_replayable() {
    let mut s = Session::default();
    run(&mut s, "file.load_json", json!({"json": workflow()}));
    let g = run(&mut s, "data.generate", json!({"cases": 10, "seed": 11}));
    let (cols, mut rows) = table(&s, &g["artifact"]);
    // case-0003 ships straight after registering: skips b and c.
    let at = rows.iter().position(|r| r[0] == "case-0003").unwrap();
    let mut bad = rows[at].clone();
    bad[1] = json!("d");
    rows.insert(at + 1, bad);
    let put = json!({"kind": "table", "columns": cols, "rows": rows});
    let log = run(
        &mut s,
        "artifact.put",
        json!({"title": "bad log", "data": put}),
    )["artifact"]
        .clone();

    let c = run(&mut s, "data.conformance", json!({"log": log}));
    assert!(c["fitness"].as_f64().unwrap() < 1.0);
    assert_eq!(c["residual"]["deviant_cases"], 1);
    assert_eq!(c["residual"]["worst_cases"][0]["case"], "case-0003");

    let r = run(&mut s, "data.replay", json!({"log": log}));
    let d = &r["first_divergence"];
    assert_eq!(d["case"], "case-0003");
    assert_eq!(d["transition"], "d");
    assert_eq!(d["kind"], "not_enabled");
    assert_eq!(d["repair"], json!(["b", "c"]));
    assert_eq!(d["places"].as_array().unwrap().len(), 2);
    assert_eq!(r["traced_case"], "case-0003");

    let trace = r["artifacts"]["trace"].clone();
    let n = match &s.artifact(trace.as_str().unwrap()).unwrap().data {
        ArtifactData::Trace { steps } => steps.len(),
        _ => panic!("trace"),
    };
    assert_eq!(n, 6, "initial marking + a, d (injected), b, c, d|e");
    run(&mut s, "sim.goto", json!({"id": trace, "step": 2}));
    assert_eq!(s.marking["end"], vec![1], "forced d");
    assert_eq!(s.marking["p1"], vec![1], "b has not run yet");
    run(&mut s, "sim.goto", json!({"id": trace, "step": 0}));
    assert_eq!(s.marking["start"], vec![1]);

    // Inline events, one case, by label and ignoring case; an unknown one.
    let ev = json!([
        {"case": "x", "activity": "Register"},
        {"case": "x", "activity": "B"},
        {"case": "x", "activity": "teleport"},
        {"case": "x", "activity": "PAY"}
    ]);
    let r = run(
        &mut s,
        "data.replay",
        json!({"events": ev, "repair_depth": -1}),
    );
    assert_eq!(r["first_divergence"]["kind"], "unknown_activity");
    assert_eq!(r["first_divergence"]["index"], 2);
    assert_eq!(r["fitness"], 0.75);
    assert_eq!(r["mapping"]["B"]["by"], "id (ignoring case)");
    assert_eq!(r["mapping"]["PAY"]["by"], "label (ignoring case)");
    assert_eq!(r["mapping"]["Register"]["by"], "label");
    let r = run(
        &mut s,
        "data.replay",
        json!({"events": ev.to_string(), "case": "x"}),
    );
    assert_eq!(r["events"], 4);
}

#[test]
fn discovery_reproduces_a_net_that_replays_its_log() {
    let mut s = Session::default();
    run(&mut s, "file.load_json", json!({"json": workflow()}));
    let before = s.doc.clone();
    let g = run(&mut s, "data.generate", json!({"cases": 60, "seed": 5}));
    let out = run(&mut s, "data.discover", json!({"log": g["artifact"]}));
    assert_eq!(out["method"], "alpha");
    assert_eq!(out["transitions"], 5);
    assert_eq!(out["places"], 6, "start, end and the four causal places");
    assert_eq!(out["conformance"]["fitness"], 1.0);
    assert_eq!(out["conformance"]["fitting_percent"], 100.0);
    // The log survives the replacement and replays on the mined net.
    let c = run(&mut s, "data.conformance", json!({"log": out["log"]}));
    assert_eq!(c["fitness"], 1.0);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.doc, before, "one undo step restores the net");

    // Heuristic with a threshold; refusals.
    let log = out["log"].clone();
    let h = run(
        &mut s,
        "data.discover",
        json!({"log": log, "method": "heuristic", "threshold": 0.6}),
    );
    assert_eq!(h["transitions"], 5);
    let log = h["log"].clone();
    assert!(err(
        &mut s,
        "data.discover",
        json!({"log": log, "method": "nope"})
    )
    .contains("unknown method"));
    assert!(err(
        &mut s,
        "data.discover",
        json!({"log": log, "threshold": 0.5})
    )
    .contains("heuristic"));
}

#[test]
fn generation_is_deterministic_by_seed() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let args = json!({"cases": 8, "seed": 42, "max_steps": 25});
    let a = run(&mut s, "data.generate", args.clone());
    let ta = table(&s, &a["artifact"]);
    let b = run(&mut s, "data.generate", args);
    assert_eq!(ta, table(&s, &b["artifact"]));
    let c = run(
        &mut s,
        "data.generate",
        json!({"cases": 8, "seed": 43, "max_steps": 25}),
    );
    assert_ne!(ta, table(&s, &c["artifact"]));
    // Times start at sim's epoch and increase within a case.
    let (_, rows) = ta;
    assert!(rows[0][2].as_i64().unwrap() >= 1_767_600_000_000);
    for w in rows.windows(2).filter(|w| w[0][0] == w[1][0]) {
        assert!(w[1][2].as_i64() > w[0][2].as_i64());
    }
    // Rates: switching a source off removes its activity.
    let d = run(
        &mut s,
        "data.generate",
        json!({"cases": 5, "seed": 1, "max_steps": 40, "rates": {"RestockCups": 0}}),
    );
    let acts = d["summary"]["activities"].as_array().unwrap();
    assert!(!acts.contains(&json!("RestockCups")));
}

#[test]
fn caps_and_bad_arguments_are_refused() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let log = coffee_log(&mut s);
    let refusals = [
        ("data.generate", json!({"cases": 0}), "cases"),
        ("data.generate", json!({"cases": 2001}), "1..2000"),
        ("data.generate", json!({"max_steps": 10001}), "max_steps"),
        (
            "data.generate",
            json!({"cases": 2000, "max_steps": 1000}),
            "cap",
        ),
        (
            "data.generate",
            json!({"rates": {"nope": 1}}),
            "no transition",
        ),
        ("data.replay", json!({}), "exactly one"),
        (
            "data.replay",
            json!({"log": log, "events": []}),
            "exactly one",
        ),
        (
            "data.replay",
            json!({"log": log, "repair_depth": 65}),
            "limit",
        ),
        (
            "data.replay",
            json!({"log": log, "case": "zz"}),
            "no events",
        ),
        (
            "data.replay",
            json!({"events": [{"case": "1"}]}),
            "activity",
        ),
        (
            "data.replay",
            json!({"events": [{"case": "1", "activity": "a", "x": 1}]}),
            "unknown field",
        ),
        ("data.conformance", json!({"log": "a999"}), "no artifact"),
        (
            "data.conformance",
            json!({"log": log, "max_traces": 0}),
            "max_traces",
        ),
    ];
    for (cmd, args, want) in refusals {
        let e = err(&mut s, cmd, args.clone());
        assert!(e.contains(want), "{cmd} {args}: {e}");
    }
    let c = run(
        &mut s,
        "data.conformance",
        json!({"log": log, "max_traces": 2}),
    );
    assert_eq!(c["total_traces"], 2);
    assert_eq!(c["log_cases"], 4);
    // A dead net produces no log.
    let dead = json!({"places": {"p": {"initial": [0], "x": 0, "y": 0}},
        "transitions": {"t": {"x": 1, "y": 1}},
        "arcs": [{"source": "p", "target": "t", "weight": [1]}]});
    run(&mut s, "file.load_json", json!({"json": dead}));
    assert!(err(&mut s, "data.generate", json!({"cases": 3})).contains("no events"));
}

#[test]
fn ambiguous_labels_are_not_guessed() {
    let mut s = Session::default();
    let doc = json!({
        "places": {"p": {"initial": [1], "x": 0, "y": 0}, "q": {"initial": [0], "x": 0, "y": 0}},
        "transitions": {"t1": {"label": "Go", "x": 1, "y": 1}, "t2": {"label": "Go", "x": 2, "y": 2}},
        "arcs": [{"source": "p", "target": "t1", "weight": [1]}, {"source": "t1", "target": "q", "weight": [1]},
                 {"source": "p", "target": "t2", "weight": [1]}, {"source": "t2", "target": "q", "weight": [1]}]
    });
    run(&mut s, "file.load_json", json!({"json": doc}));
    let ev = json!([{"case": "1", "activity": "Go"}, {"case": "2", "activity": "t1"}]);
    let r = run(&mut s, "data.replay", json!({ "events": ev }));
    assert_eq!(r["first_divergence"]["kind"], "ambiguous_activity");
    assert_eq!(r["fitness"], 0.5);
    assert!(r["caveats"][0].as_str().unwrap().contains("ambiguous"));
}

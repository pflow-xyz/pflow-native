//! Phase 4, state-space half: analysis.reachability / verify / reach /
//! diagnose.

use std::collections::BTreeMap;

use pflow_native_core::artifact::{GraphNode, TraceStep};
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

fn example(name: &str) -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({ "name": name }));
    s
}

/// A single-color net from (place, initial, capacity) and (source, target,
/// weight, inhibit); every arc end that is not a place is a transition.
fn net(places: &[(&str, i64, i64)], arcs: &[(&str, &str, i64, bool)]) -> Session {
    let mut ps = serde_json::Map::new();
    for (i, (id, init, cap)) in places.iter().enumerate() {
        ps.insert(
            id.to_string(),
            json!({"@type": "Place", "offset": i, "initial": [init], "capacity": [cap],
                   "x": 100 + 80 * i, "y": 100}),
        );
    }
    let mut ts = serde_json::Map::new();
    for (src, dst, _, _) in arcs {
        for end in [src, dst] {
            if !places.iter().any(|p| p.0 == *end) && !ts.contains_key(*end) {
                let i = ts.len();
                ts.insert(
                    end.to_string(),
                    json!({"@type": "Transition", "x": 100 + 80 * i, "y": 200}),
                );
            }
        }
    }
    let arcs: Vec<Value> = arcs
        .iter()
        .map(|(s, t, w, inh)| {
            json!({"@type": "Arrow", "source": s, "target": t, "weight": [w], "inhibitTransition": inh})
        })
        .collect();
    let doc = json!({
        "@context": "https://pflow.xyz/schema", "@type": "PetriNet", "@version": "1.1",
        "token": ["https://pflow.xyz/tokens/black"],
        "places": ps, "transitions": ts, "arcs": arcs,
    });
    let mut s = Session::default();
    run(&mut s, "file.load_json", json!({ "json": doc }));
    s
}

fn data(s: &Session, id: &Value) -> ArtifactData {
    s.artifact(id.as_str().expect("artifact id"))
        .expect("artifact stored")
        .data
        .clone()
}

fn graph(s: &Session, id: &Value) -> (Vec<GraphNode>, Vec<pflow_native_core::artifact::GraphEdge>) {
    match data(s, id) {
        ArtifactData::Graph { nodes, edges } => (nodes, edges),
        other => panic!("expected a graph, got {}", other.kind()),
    }
}

fn trace(s: &Session, id: &Value) -> Vec<TraceStep> {
    match data(s, id) {
        ArtifactData::Trace { steps } => steps,
        other => panic!("expected a trace, got {}", other.kind()),
    }
}

fn mk(pairs: &[(&str, i64)]) -> BTreeMap<String, Vec<i64>> {
    pairs
        .iter()
        .map(|(p, n)| (p.to_string(), vec![*n]))
        .collect()
}

/// S + I -> 2I, I -> R with S=3, I=1.
fn small_sir() -> Session {
    net(
        &[("S", 3, 0), ("I", 1, 0), ("R", 0, 0)],
        &[
            ("S", "infect", 1, false),
            ("I", "infect", 1, false),
            ("infect", "I", 2, false),
            ("I", "recover", 1, false),
            ("recover", "R", 1, false),
        ],
    )
}

/// Two processes taking two locks in opposite orders: deadlocks when each
/// holds one.
fn two_locks() -> Session {
    net(
        &[
            ("l1", 1, 0),
            ("l2", 1, 0),
            ("p_idle", 1, 0),
            ("p_has1", 0, 0),
            ("p_cs", 0, 0),
            ("q_idle", 1, 0),
            ("q_has2", 0, 0),
            ("q_cs", 0, 0),
        ],
        &[
            ("p_idle", "tp1", 1, false),
            ("l1", "tp1", 1, false),
            ("tp1", "p_has1", 1, false),
            ("p_has1", "tp2", 1, false),
            ("l2", "tp2", 1, false),
            ("tp2", "p_cs", 1, false),
            ("p_cs", "tp3", 1, false),
            ("tp3", "p_idle", 1, false),
            ("tp3", "l1", 1, false),
            ("tp3", "l2", 1, false),
            ("q_idle", "tq1", 1, false),
            ("l2", "tq1", 1, false),
            ("tq1", "q_has2", 1, false),
            ("q_has2", "tq2", 1, false),
            ("l1", "tq2", 1, false),
            ("tq2", "q_cs", 1, false),
            ("q_cs", "tq3", 1, false),
            ("tq3", "q_idle", 1, false),
            ("tq3", "l1", 1, false),
            ("tq3", "l2", 1, false),
        ],
    )
}

#[test]
fn commands_are_registered_under_analyze() {
    let s = Session::default();
    for id in [
        "analysis.reachability",
        "analysis.verify",
        "analysis.reach",
        "analysis.diagnose",
    ] {
        let c = s.registry().find(id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!(c.menu, "Analyze");
        assert!(c.journal, "{id} produces results, so it is journaled");
    }
}

#[test]
fn sir_state_count_matches_the_hand_count() {
    // (s, i) with r = 4 - s - i: s=3 → i∈{0,1}; s=2 → 0..2; s=1 → 0..3;
    // s=0 → 0..4: 14 markings. infect is enabled where s,i ≥ 1 (6 states),
    // recover where i ≥ 1 (10): 16 edges. The 4 markings with i = 0 are
    // terminal, all non-empty: deadlocks by pflow-reachability's rule.
    let mut s = small_sir();
    let r = run(&mut s, "analysis.reachability", json!({}));
    assert_eq!(r["states"], 14);
    assert_eq!(r["edges"], 16);
    assert_eq!(r["bounded"], true);
    assert_eq!(r["truncated"], false);
    assert_eq!(r["deadlocks"].as_array().unwrap().len(), 4);
    assert_eq!(r["max_tokens"], json!({"S": 3, "I": 4, "R": 4}));
    let (nodes, edges) = graph(&s, &r["graph"]);
    assert_eq!((nodes.len(), edges.len()), (14, 16));
    assert_eq!(nodes[0].label, "S=3 I=1");
    assert_eq!(nodes[0].data["initial"], true);
    assert_eq!(
        nodes[0].data["marking"],
        json!({"S": [3], "I": [1], "R": [0]})
    );
    data(&s, &r["graph"]).validate().unwrap();
    // A report artifact carries the same summary.
    match data(&s, &r["report"]) {
        ArtifactData::Report(v) => assert_eq!(v["states"], 14),
        other => panic!("{}", other.kind()),
    }
    // Deterministic: the same graph again.
    let again = run(&mut s, "analysis.reachability", json!({}));
    assert_eq!(graph(&s, &again["graph"]).0, nodes);
}

#[test]
fn bounded_producer_consumer_graph() {
    // produce: slot -> buffer; consume: buffer + idle consumer -> busy;
    // finish: busy -> idle + slot. slots + buffer + busy = 2 and
    // idle + busy = 1 leave 5 markings and 7 firings, none terminal.
    let mut s = net(
        &[
            ("slots", 2, 0),
            ("buffer", 0, 0),
            ("idle", 1, 0),
            ("busy", 0, 0),
        ],
        &[
            ("slots", "produce", 1, false),
            ("produce", "buffer", 1, false),
            ("buffer", "consume", 1, false),
            ("idle", "consume", 1, false),
            ("consume", "busy", 1, false),
            ("busy", "finish", 1, false),
            ("finish", "idle", 1, false),
            ("finish", "slots", 1, false),
        ],
    );
    let r = run(&mut s, "analysis.reachability", json!({}));
    assert_eq!(
        (r["states"].as_u64(), r["edges"].as_u64()),
        (Some(5), Some(7))
    );
    assert_eq!(r["bounded"], true);
    assert_eq!(r["deadlocks"], json!([]));
    assert_eq!(r["never_fired"], json!([]));
    let (nodes, edges) = graph(&s, &r["graph"]);
    let labels: Vec<&str> = nodes.iter().map(|n| n.label.as_str()).collect();
    assert!(labels.contains(&"buffer=2 idle=1"), "{labels:?}");
    assert!(edges.iter().any(|e| e.label == "finish"));

    // The bundled example's weight-0 arc gates `produce` on an empty place,
    // so the token game has nothing enabled; the graph agrees.
    let mut ex = example("producer-consumer");
    assert_eq!(run(&mut ex, "sim.enabled", json!({})), json!([]));
    let r = run(&mut ex, "analysis.reachability", json!({}));
    assert_eq!(r["states"], 1);
    assert_eq!(r["deadlocks"], json!(["s0"]));
}

#[test]
fn unbounded_net_gets_omega() {
    // t0 keeps p0's token and adds one to p1 each time.
    let mut s = net(
        &[("p0", 1, 0), ("p1", 0, 0)],
        &[
            ("p0", "t0", 1, false),
            ("t0", "p0", 1, false),
            ("t0", "p1", 1, false),
        ],
    );
    let r = run(&mut s, "analysis.reachability", json!({}));
    assert_eq!(r["bounded"], false);
    assert_eq!(r["coverability"], true);
    assert_eq!(r["max_tokens"]["p1"], "ω");
    assert_eq!(
        (r["states"].as_u64(), r["edges"].as_u64()),
        (Some(2), Some(2))
    );
    let (nodes, _) = graph(&s, &r["graph"]);
    assert_eq!(nodes[1].label, "p0=1 p1=ω");
    assert_eq!(nodes[1].data["omega"], json!(["p1"]));
    assert_eq!(nodes[1].data["marking"]["p1"], json!([-1]));

    // verify: bounded is refuted by a pump witness, with a trace.
    let v = run(
        &mut s,
        "analysis.verify",
        json!({"properties": [{"kind": "bounded"}]}),
    );
    let b = &v["verdicts"][0];
    assert_eq!(
        (b["status"].as_str(), b["method"].as_str()),
        (Some("refuted"), Some("witness"))
    );
    let steps = trace(&s, &b["trace"]);
    assert_eq!(steps.len(), 2);
    assert_eq!(steps[1].marking, mk(&[("p0", 1), ("p1", 1)]));

    // The same pump with p1 capped at 3 is not monotone: no ω, the
    // capacity bounds the graph exactly (p1 = 0..3).
    let mut capped = net(
        &[("p0", 1, 0), ("p1", 0, 3)],
        &[
            ("p0", "t0", 1, false),
            ("t0", "p0", 1, false),
            ("t0", "p1", 1, false),
        ],
    );
    let r = run(&mut capped, "analysis.reachability", json!({}));
    assert_eq!(
        (r["states"].as_u64(), r["bounded"].as_bool()),
        (Some(4), Some(true))
    );
    assert_eq!(r["deadlocks"], json!(["s3"]));

    // An inhibitor on the pump: no acceleration either, so the plain search
    // runs into the state cap and the answer is unknown, not bounded.
    let mut inh = net(
        &[("p0", 1, 0), ("p1", 0, 0), ("stop", 0, 0)],
        &[
            ("p0", "t0", 1, false),
            ("stop", "t0", 1, true),
            ("t0", "p0", 1, false),
            ("t0", "p1", 1, false),
        ],
    );
    let r = run(&mut inh, "analysis.reachability", json!({"max_states": 50}));
    assert_eq!(r["truncated"], true);
    assert_eq!(r["bounded"], Value::Null);
    assert_eq!(r["states"], 50);
}

#[test]
fn deadlock_counterexample_replays_on_the_canvas() {
    let mut s = two_locks();
    let v = run(
        &mut s,
        "analysis.verify",
        json!({"properties": [
            {"kind": "deadlock-free"},
            {"kind": "mutual-exclusion", "places": ["p_cs", "q_cs"]},
            {"kind": "invariant", "expr": "l1 + p_has1 + p_cs + q_cs == 1"},
            {"kind": "bounded"},
            {"kind": "bounded", "bound": 1},
            {"kind": "live"},
            {"kind": "reachable", "target": {"p_cs": 1}},
            {"kind": "unreachable", "target": {"p_cs": 1, "q_cs": 1}},
            {"kind": "invariant", "expr": "p_idle + q_idle >= 1"},
        ]}),
    );
    let st: Vec<(&str, &str)> = v["verdicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| (x["status"].as_str().unwrap(), x["method"].as_str().unwrap()))
        .collect();
    assert_eq!(
        st,
        vec![
            ("refuted", "exhaustive"),
            ("proved", "structural"),
            ("proved", "structural"),
            ("proved", "structural"),
            ("proved", "structural"),
            ("proved", "exhaustive"),
            ("proved", "exhaustive"),
            ("proved", "exhaustive"),
            ("refuted", "exhaustive"),
        ],
        "{v:#}"
    );
    assert_eq!(
        (v["proved"].as_u64(), v["refuted"].as_u64()),
        (Some(7), Some(2))
    );
    assert_eq!(v["ok"], false);

    // The deadlock: each process holds one lock (shortest: two firings).
    let dl = &v["verdicts"][0];
    let steps = trace(&s, &dl["trace"]);
    let fired: Vec<&str> = steps.iter().map(|x| x.transition.as_str()).collect();
    assert_eq!(fired, vec!["", "tp1", "tq1"]);
    assert_eq!(
        steps[0].marking,
        pflow_native_core::sim::initial_marking(&s.doc)
    );
    let goto = run(&mut s, "sim.goto", json!({"id": dl["trace"], "step": 2}));
    assert_eq!(goto["marking"]["p_has1"], json!([1]));
    assert_eq!(goto["marking"]["q_has2"], json!([1]));
    assert_eq!(run(&mut s, "sim.enabled", json!({})), json!([]));
    // One undo step back to where we were.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(s.marking, pflow_native_core::sim::initial_marking(&s.doc));

    // p_idle + q_idle >= 1 fails once both are inside.
    let both = trace(&s, &v["verdicts"][8]["trace"]);
    assert_eq!(both.len(), 3);
}

#[test]
fn verify_defaults_and_refusals() {
    let mut s = small_sir();
    let v = run(&mut s, "analysis.verify", json!({}));
    let kinds: Vec<&str> = v["verdicts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["property"]["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, vec!["bounded", "deadlock-free"]);
    // S + I + R is conserved: structurally bounded; every epidemic ends
    // with I = 0 and tokens left: a deadlock by the shared rule.
    assert_eq!(v["verdicts"][0]["status"], "proved");
    assert_eq!(v["verdicts"][1]["status"], "refuted");
    // Properties may come as sim's JSON string, and conserves holds here.
    let v = run(
        &mut s,
        "analysis.verify",
        json!({"properties": "[{\"kind\":\"conserves\"},{\"kind\":\"terminating\"}]"}),
    );
    assert_eq!(v["verdicts"][0]["status"], "proved");
    assert_eq!(v["verdicts"][1]["status"], "proved");
    // k-bounded: I peaks at 4 after three infections, so 3 is refuted with
    // the shortest trace there; 4 holds exhaustively (no invariant bounds
    // I alone by 4: S + I + R = 4 does, structurally).
    let v = run(
        &mut s,
        "analysis.verify",
        json!({"properties": [{"kind": "bounded", "bound": 3}, {"kind": "bounded", "bound": 4}]}),
    );
    assert_eq!(v["verdicts"][0]["status"], "refuted");
    let steps = trace(&s, &v["verdicts"][0]["trace"]);
    assert_eq!(steps.len(), 4);
    assert_eq!(steps[3].marking["I"], vec![4]);
    assert_eq!(v["verdicts"][1]["status"], "proved");
    assert_eq!(v["verdicts"][1]["method"], "structural");

    assert!(err(
        &mut s,
        "analysis.verify",
        json!({"properties": [{"kind": "fair"}]})
    )
    .contains("unknown property kind"));
    assert!(err(
        &mut s,
        "analysis.verify",
        json!({"properties": [{"kind": "live", "x": 1}]})
    )
    .contains("not one of"));
    assert!(err(
        &mut s,
        "analysis.verify",
        json!({"properties": [{"kind": "invariant"}]})
    )
    .contains("needs expr"));
    assert!(err(&mut s, "analysis.verify", json!({"max_states": 0})).contains("outside"));
    // An expression naming a missing place is unknown, not a pass.
    let v = run(
        &mut s,
        "analysis.verify",
        json!({"properties": [{"kind": "invariant", "expr": "S + nope == 3"}]}),
    );
    assert_eq!(v["verdicts"][0]["status"], "unknown");
}

#[test]
fn reach_finds_a_shortest_win_and_refuses_the_impossible() {
    let mut s = example("tic-tac-toe");
    let r = run(&mut s, "analysis.reach", json!({"to": {"win_x": 1}}));
    assert_eq!(
        (r["answer"].as_str(), r["method"].as_str()),
        (Some("yes"), Some("trace"))
    );
    // x needs three marks and o gets two moves in between, then the win.
    let ids = r["witness"]["trace"]["steps"].as_array().unwrap();
    assert_eq!(ids.len(), 6);
    assert!(ids[5].as_str().unwrap().starts_with("x_win_"));
    let steps = trace(&s, &r["trace"]);
    assert_eq!(steps.len(), 7);
    assert_eq!(steps[0].transition, "");
    let end = run(&mut s, "sim.goto", json!({"id": r["trace"], "step": 6}));
    assert_eq!(end["marking"]["win_x"], json!([1]));
    assert_eq!(end["marking"]["x_turn"], json!([0]));

    // Both players winning: x_turn + o_turn + win_x + win_o = 1 forbids it,
    // and the free turn places can only add.
    let r = run(
        &mut s,
        "analysis.reach",
        json!({"to": {"win_x": 1, "win_o": 1}}),
    );
    assert_eq!(
        (r["answer"].as_str(), r["method"].as_str()),
        (Some("no"), Some("invariant"))
    );
    let w = &r["witness"]["invariant"];
    assert_eq!(
        w["coefficients"],
        json!({"o_turn": 1, "win_o": 1, "win_x": 1, "x_turn": 1})
    );
    assert_eq!(
        (w["atFrom"].as_i64(), w["atTarget"].as_i64()),
        (Some(1), Some(2))
    );
    assert!(r["reads"].as_str().unwrap().contains("unreachable"));
    assert!(matches!(data(&s, &r["artifact"]), ArtifactData::Report(_)));
    // A square holding two marks.
    let r = run(&mut s, "analysis.reach", json!({"to": {"x00": 2}}));
    assert_eq!(r["answer"], "no");

    // Unknown names and malformed counts are refused, naming the key.
    assert!(err(&mut s, "analysis.reach", json!({"to": {"nope": 1}})).contains("nope"));
    assert!(err(&mut s, "analysis.reach", json!({"to": {"win_x": 0.5}})).contains("whole"));
    assert!(err(&mut s, "analysis.reach", json!({"to": {"win_x": -1}})).contains("negative"));
    assert!(err(&mut s, "analysis.reach", json!({"to": {}})).contains("names no place"));
}

#[test]
fn reach_equality_exhaustive_unknown_and_from() {
    // S + I + R = 4 with every place named: pass A's equality certificate.
    let mut s = small_sir();
    let r = run(
        &mut s,
        "analysis.reach",
        json!({"to": {"S": 0, "I": 0, "R": 3}}),
    );
    assert_eq!(r["answer"], "no");
    assert_eq!(r["witness"]["invariant"]["pass"], "equality");
    assert_eq!(r["witness"]["invariant"]["atFrom"], 4);
    // From an override, the empty trace already matches.
    let r = run(
        &mut s,
        "analysis.reach",
        json!({"from": {"S": 0, "I": 0, "R": 4}, "to": {"R": 4}}),
    );
    assert_eq!(r["answer"], "yes");
    assert_eq!(trace(&s, &r["trace"]).len(), 1);
    // The epidemic burning out completely.
    let r = run(&mut s, "analysis.reach", json!({"to": {"S": 0, "I": 0}}));
    assert_eq!(r["witness"]["trace"]["steps"].as_array().unwrap().len(), 7);

    // A source capped at 2: 3 cannot be reached, and only search shows it.
    let mut c = net(&[("p", 0, 2)], &[("src", "p", 1, false)]);
    let r = run(&mut c, "analysis.reach", json!({"to": {"p": 3}}));
    assert_eq!(
        (r["answer"].as_str(), r["method"].as_str()),
        (Some("no"), Some("exhaustive"))
    );
    assert_eq!(r["witness"]["exhaustive"]["states"], 3);

    // Uncapped, a small budget leaves it open.
    let mut u = net(&[("p", 0, 0)], &[("src", "p", 1, false)]);
    let r = run(
        &mut u,
        "analysis.reach",
        json!({"to": {"p": 50}, "max_states": 10}),
    );
    assert_eq!(r["answer"], "unknown");
    assert_eq!(r["unknownBecause"][0]["code"], "state-budget");
    let r = run(&mut u, "analysis.reach", json!({"to": {"p": 50}}));
    assert_eq!(r["answer"], "yes");
}

#[test]
fn colored_nets_report_document_ids_per_color() {
    let mut s = example("coffee-shop");
    let r = run(
        &mut s,
        "analysis.reach",
        json!({"to": {"ServedCoffee": [3, 0, 0], "WaitingCustomers": [0, null, null]}}),
    );
    assert_eq!(r["answer"], "yes");
    let steps = trace(&s, &r["trace"]);
    let last = steps.last().unwrap();
    assert_eq!(last.marking["ServedCoffee"], vec![3, 0, 0]);
    assert_eq!(last.marking["CoffeeBeans"].len(), 3);
    run(
        &mut s,
        "sim.goto",
        json!({"id": r["trace"], "step": steps.len() - 1}),
    );
    assert_eq!(s.marking["ServedCoffee"], vec![3, 0, 0]);
    // A display name names one color; capacities bound the search.
    let r = run(
        &mut s,
        "analysis.reach",
        json!({"to": {"CoffeeBeans.brown": 11}}),
    );
    assert_eq!(
        (r["answer"].as_str(), r["method"].as_str()),
        (Some("no"), Some("exhaustive"))
    );
    assert!(err(
        &mut s,
        "analysis.reach",
        json!({"to": {"ServedCoffee": [0, 2, 0]}})
    )
    .contains("cannot hold color 1"));
    // The graph is bounded by the capacities, nodes in document ids.
    let g = run(&mut s, "analysis.reachability", json!({"max_states": 500}));
    assert_eq!(g["truncated"], true);
    let (nodes, _) = graph(&s, &g["graph"]);
    assert_eq!(nodes[0].data["marking"]["CoffeeBeans"], json!([0, 5, 0]));
    assert!(
        nodes[0].label.contains("CoffeeBeans.brown=5"),
        "{}",
        nodes[0].label
    );
}

#[test]
fn diagnose_ranks_recovery_above_infection_for_r() {
    // With S=999, I=1 and both rates 1 the outbreak is over almost at once
    // (R0 ≈ 999), so infection speed barely moves R(10); what is left of I
    // decays as e^{-γt}: at γ = 0.5, I(10) ≈ 1000·e^{-5} ≈ 6.7, at γ = 1.5
    // nothing. So recover's ±50% swings R(10) by ≈ 6.7 and infect's by far
    // less than one token.
    let mut s = example("sir");
    let d = run(&mut s, "analysis.diagnose", json!({}));
    assert_eq!(d["observable"], json!(["R"]));
    let knobs = d["knobs"].as_array().unwrap();
    assert_eq!(knobs[0]["name"], "rate recover");
    let inf = |name: &str| {
        knobs.iter().find(|k| k["name"] == name).unwrap()["influence"]
            .as_f64()
            .unwrap()
    };
    assert!(
        (inf("rate recover") - 6.7).abs() < 0.3,
        "{}",
        inf("rate recover")
    );
    assert!(inf("rate infect").abs() < 0.1);
    assert!(inf("rate recover") > inf("rate infect").abs());
    // One more token anywhere ends up recovered.
    assert!((inf("+1 S") - 1.0).abs() < 0.05);
    // The tornado chart is sorted the same way, around the baseline.
    match data(&s, &d["bars"]) {
        ArtifactData::Bars { bars, value_label } => {
            assert_eq!(value_label, "R at t=10");
            assert_eq!(bars[0].name, "rate recover");
            assert!(bars[0].lo.unwrap() < bars[0].hi.unwrap());
            assert!((bars[0].value - 1000.0).abs() < 0.1);
            assert_eq!(bars.len(), 5);
        }
        other => panic!("{}", other.kind()),
    }
    // The SSA noise floor separates recover (binds) from the rest.
    let floor = d["noise"]["floor"].as_f64().unwrap();
    assert!(floor < 6.0, "{floor}");
    assert_eq!(knobs[0]["inert"], false);
    assert_eq!(
        knobs.iter().find(|k| k["name"] == "rate infect").unwrap()["inert"],
        true
    );
    // Deterministic without noise.
    // (to the solver's last bits: FEATURES.md §D 7).
    let a = run(
        &mut s,
        "analysis.diagnose",
        json!({"noise": false, "observable": "I"}),
    );
    let b = run(
        &mut s,
        "analysis.diagnose",
        json!({"noise": false, "observable": "I"}),
    );
    for (x, y) in a["knobs"]
        .as_array()
        .unwrap()
        .iter()
        .zip(b["knobs"].as_array().unwrap())
    {
        assert_eq!(x["name"], y["name"]);
        let (p, q) = (
            x["influence"].as_f64().unwrap(),
            y["influence"].as_f64().unwrap(),
        );
        assert!((p - q).abs() < 1e-9, "{p} vs {q}");
    }
    assert_eq!(a["noise"], Value::Null);

    assert!(err(&mut s, "analysis.diagnose", json!({"delta": 0})).contains("delta"));
    assert!(err(&mut s, "analysis.diagnose", json!({"observable": "Q"})).contains("Q"));
}

#[test]
fn budgets_are_capped() {
    let mut s = small_sir();
    assert!(err(
        &mut s,
        "analysis.reachability",
        json!({"max_states": 20001})
    )
    .contains("1..20000"));
    assert!(err(
        &mut s,
        "analysis.reach",
        json!({"to": {"R": 1}, "max_states": 100001})
    )
    .contains("1..100000"));
    // SIR with 1000 tokens has ~500k markings: the graph stops at the cap.
    let mut big = example("sir");
    let r = run(&mut big, "analysis.reachability", json!({}));
    assert_eq!(
        (r["states"].as_u64(), r["truncated"].as_bool()),
        (Some(2000), Some(true))
    );
    assert_eq!(r["bounded"], Value::Null);
}

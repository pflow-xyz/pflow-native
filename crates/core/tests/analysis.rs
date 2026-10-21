//! Phase 4, structural half: analysis.incidence / invariants / siphons /
//! validate / canonical / classify.

use pflow_native_core::artifact::NodeSet;
use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn example(name: &str) -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({ "name": name }));
    s
}

/// A net from (places with initial tokens, transitions, arcs `[src, dst,
/// weight]`), in the pflow.xyz editor JSON.
fn net(places: &[(&str, i64)], transitions: &[&str], arcs: &[(&str, &str, i64)]) -> Session {
    let mut p = serde_json::Map::new();
    for (i, (id, init)) in places.iter().enumerate() {
        p.insert(
            id.to_string(),
            json!({"@type": "Place", "offset": i, "initial": [init], "capacity": [0], "x": 100 * i, "y": 100}),
        );
    }
    let mut t = serde_json::Map::new();
    for (i, id) in transitions.iter().enumerate() {
        t.insert(
            id.to_string(),
            json!({"@type": "Transition", "x": 100 * i, "y": 200}),
        );
    }
    let a: Vec<Value> = arcs
        .iter()
        .map(|(s, d, w)| json!({"@type": "Arrow", "source": s, "target": d, "weight": [w]}))
        .collect();
    let doc = json!({
        "@context": "https://pflow.xyz/schema",
        "@type": "PetriNet",
        "@version": "1.1",
        "token": ["https://pflow.xyz/tokens/black"],
        "places": p,
        "transitions": t,
        "arcs": a,
    });
    let mut s = Session::default();
    run(&mut s, "file.load_json", json!({ "json": doc }));
    s
}

fn sets(s: &Session, id: &Value) -> Vec<NodeSet> {
    match &s.artifact(id.as_str().unwrap()).unwrap().data {
        ArtifactData::Sets { sets } => sets.clone(),
        other => panic!("expected sets, got {}", other.kind()),
    }
}

/// Every set of a sets artifact highlights on the canvas.
fn show_all(s: &mut Session, id: &Value) {
    for i in 0..sets(s, id).len() {
        run(s, "artifact.show_set", json!({"id": id, "index": i}));
    }
}

#[test]
fn sir_incidence() {
    let mut s = example("sir");
    let r = run(&mut s, "analysis.incidence", json!({}));
    assert_eq!(r["rows"], json!(["S", "I", "R"]));
    assert_eq!(r["cols"], json!(["infect", "recover"]));
    assert_eq!(r["incidence"], json!([[-1, 0], [1, -1], [0, 1]]));
    assert_eq!(r["pre"], json!([[1, 0], [1, 1], [0, 0]]));
    assert_eq!(r["post"], json!([[0, 0], [2, 0], [0, 1]]));
    match &s.artifact(r["artifact"].as_str().unwrap()).unwrap().data {
        ArtifactData::Matrix { rows, cols, values } => {
            assert_eq!(rows, &["S", "I", "R"]);
            assert_eq!(cols, &["infect", "recover"]);
            assert_eq!(values[1], vec![1.0, -1.0]);
        }
        other => panic!("{}", other.kind()),
    }
}

#[test]
fn sir_conserves_its_population() {
    let mut s = example("sir");
    let r = run(&mut s, "analysis.invariants", json!({}));
    assert_eq!(r["p_invariants"].as_array().unwrap().len(), 1);
    assert_eq!(r["p_invariants"][0]["expression"], "S + I + R = 1000");
    assert_eq!(r["p_invariants"][0]["value"], 1000);
    assert_eq!(r["structurally_bounded"], true);
    // SIR never returns to a marking: no T-invariant.
    assert_eq!(r["t_invariants"], json!([]));
    assert_eq!(r["t_covered"], false);
    let ss = sets(&s, &r["sets"]);
    assert_eq!(ss[0].label, "S + I + R = 1000");
    assert_eq!(ss[0].nodes, vec!["S", "I", "R"]);
    assert_eq!(ss[0].weights["I"], 1.0);
    assert_eq!(r["matrices"].as_array().unwrap().len(), 2);
    show_all(&mut s, &r["sets"]);

    // Valued at the current marking: firing moves tokens, not the sum.
    run(&mut s, "sim.fire", json!({"id": "infect"}));
    let r = run(&mut s, "analysis.invariants", json!({"kind": "p"}));
    assert_eq!(r["p_invariants"][0]["value"], 1000);
    assert!(r.get("t_invariants").is_none());
    assert!(s
        .execute("analysis.invariants", json!({"kind": "x"}))
        .is_err());
}

#[test]
fn cycle_has_a_t_invariant() {
    let mut s = net(
        &[("a", 1), ("b", 0)],
        &["go", "back"],
        &[
            ("a", "go", 1),
            ("go", "b", 1),
            ("b", "back", 1),
            ("back", "a", 1),
        ],
    );
    let r = run(&mut s, "analysis.invariants", json!({"kind": "t"}));
    assert_eq!(r["t_invariants"].as_array().unwrap().len(), 1);
    assert_eq!(r["t_invariants"][0]["expression"], "go + back");
    assert_eq!(r["t_invariants"][0]["counts"], json!({"back": 1, "go": 1}));
    assert_eq!(r["t_covered"], true);

    // Weighted: 2·go fires per back.
    let mut s = net(
        &[("a", 2), ("b", 0)],
        &["go", "back"],
        &[
            ("a", "go", 1),
            ("go", "b", 1),
            ("b", "back", 2),
            ("back", "a", 2),
        ],
    );
    let r = run(&mut s, "analysis.invariants", json!({}));
    assert_eq!(r["t_invariants"][0]["expression"], "2·go + back");
    assert_eq!(r["p_invariants"][0]["expression"], "a + b = 2");
}

#[test]
fn marked_cycle_is_live_by_commoner() {
    let mut s = net(
        &[("a", 1), ("b", 0)],
        &["go", "back"],
        &[
            ("a", "go", 1),
            ("go", "b", 1),
            ("b", "back", 1),
            ("back", "a", 1),
        ],
    );
    let r = run(&mut s, "analysis.siphons", json!({}));
    assert_eq!(r["minimal_siphons"][0]["places"], json!(["a", "b"]));
    assert_eq!(r["minimal_traps"][0]["places"], json!(["a", "b"]));
    assert_eq!(r["commoner"], true);
    assert_eq!(r["free_choice"], true);
    assert!(r["verdict"].as_str().unwrap().starts_with("live"), "{r}");
    assert_eq!(r["deadlock_witnesses"], json!([]));
    show_all(&mut s, &r["sets"]);
}

#[test]
fn empty_siphon_is_a_deadlock_witness() {
    // `use` needs a and b; b is only refilled by `put`, which needs c, which
    // only `use` fills: {b, c} starts empty and stays empty.
    let mut s = net(
        &[("a", 1), ("b", 0), ("c", 0)],
        &["use", "put"],
        &[
            ("a", "use", 1),
            ("b", "use", 1),
            ("use", "c", 1),
            ("c", "put", 1),
            ("put", "b", 1),
        ],
    );
    let r = run(&mut s, "analysis.siphons", json!({}));
    let w = &r["deadlock_witnesses"];
    assert_eq!(w.as_array().unwrap().len(), 1, "{r}");
    assert_eq!(w[0]["siphon"], json!(["b", "c"]));
    assert_eq!(w[0]["unfireable_transitions"], json!(["use", "put"]));
    let ss = sets(&s, &r["sets"]);
    let wit = ss
        .iter()
        .find(|x| x.label.starts_with("deadlock witness"))
        .unwrap();
    assert_eq!(wit.nodes, vec!["b", "c", "use", "put"]);
    show_all(&mut s, &r["sets"]);
}

#[test]
fn crossed_locks_fail_commoner() {
    // Two processes take two locks in opposite order: the classic deadlock.
    let mut s = net(
        &[
            ("idle1", 1),
            ("has1", 0),
            ("idle2", 1),
            ("has2", 0),
            ("l1", 1),
            ("l2", 1),
        ],
        &["p1a", "p1b", "p2a", "p2b"],
        &[
            ("idle1", "p1a", 1),
            ("l1", "p1a", 1),
            ("p1a", "has1", 1),
            ("has1", "p1b", 1),
            ("l2", "p1b", 1),
            ("p1b", "idle1", 1),
            ("p1b", "l1", 1),
            ("p1b", "l2", 1),
            ("idle2", "p2a", 1),
            ("l2", "p2a", 1),
            ("p2a", "has2", 1),
            ("has2", "p2b", 1),
            ("l1", "p2b", 1),
            ("p2b", "idle2", 1),
            ("p2b", "l1", 1),
            ("p2b", "l2", 1),
        ],
    );
    let r = run(&mut s, "analysis.siphons", json!({}));
    assert_eq!(r["commoner"], false, "{r}");
    assert_eq!(r["deadlock_witnesses"], json!([]));
    // The deadlocking siphon: both locks and both idle states.
    let bad: Vec<&Value> = r["minimal_siphons"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["contains_marked_trap"] == false)
        .collect();
    assert!(!bad.is_empty());
    // Reach the deadlock: each process takes its first lock.
    run(&mut s, "sim.fire", json!({"id": "p1a"}));
    run(&mut s, "sim.fire", json!({"id": "p2a"}));
    let r = run(&mut s, "analysis.siphons", json!({}));
    assert!(
        !r["deadlock_witnesses"].as_array().unwrap().is_empty(),
        "{r}"
    );
}

#[test]
fn validate_runs_on_every_example() {
    for (name, _) in pflow_native_core::commands::file::EXAMPLES {
        let mut s = example(name);
        let r = run(&mut s, "analysis.validate", json!({}));
        let n = r["errors"].as_u64().unwrap()
            + r["warnings"].as_u64().unwrap()
            + r["info"].as_u64().unwrap();
        assert_eq!(r["issues"].as_array().unwrap().len() as u64, n, "{name}");
        match &s.artifact(r["table"].as_str().unwrap()).unwrap().data {
            ArtifactData::Table { columns, rows } => {
                assert_eq!(columns[0], "severity");
                assert_eq!(rows.len() as u64, n);
            }
            other => panic!("{}", other.kind()),
        }
        for issue in r["issues"].as_array().unwrap() {
            for node in issue["nodes"].as_array().unwrap() {
                assert!(
                    s.doc.kind_of(node.as_str().unwrap()).is_some(),
                    "{name}: {issue}"
                );
            }
        }
    }
    let mut s = example("sir");
    let r = run(
        &mut s,
        "analysis.validate",
        json!({"reachability": true, "max_states": 50}),
    );
    assert!(r["reachability"]["reachable"].as_u64().unwrap() >= 1);
}

#[test]
fn every_analysis_runs_on_every_example() {
    for (name, _) in pflow_native_core::commands::file::EXAMPLES {
        let mut s = example(name);
        for cmd in [
            "analysis.incidence",
            "analysis.invariants",
            "analysis.siphons",
            "analysis.canonical",
            "analysis.classify",
        ] {
            let t = std::time::Instant::now();
            let r = s
                .execute(cmd, json!({}))
                .unwrap_or_else(|e| panic!("{name} {cmd}: {e}"));
            eprintln!("{name:18} {cmd:22} {:?}", t.elapsed());
            if let Some(id) = r.get("sets") {
                show_all(&mut s, id);
            }
        }
    }
}

#[test]
fn colored_rows_name_the_color() {
    let mut s = example("coffee-shop");
    let r = run(&mut s, "analysis.incidence", json!({}));
    let rows: Vec<&str> = r["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(rows.iter().all(|r| r.contains('.')), "{rows:?}");
    // Every row is a colour some arc or marking uses.
    assert!(rows.len() < 4 * 3, "{rows:?}");
}

#[test]
fn canonical_id_is_isomorphism_invariant() {
    let mut a = example("sir");
    let ia = run(&mut a, "analysis.canonical", json!({}));
    let mut b = example("sir");
    run(
        &mut b,
        "net.rename",
        json!({"id": "S", "new_id": "healthy"}),
    );
    run(
        &mut b,
        "net.rename",
        json!({"id": "infect", "new_id": "zz_catch"}),
    );
    run(&mut b, "net.rename", json!({"id": "R", "new_id": "a_done"}));
    let ib = run(&mut b, "analysis.canonical", json!({}));
    assert_eq!(ia["canonical_id"], ib["canonical_id"]);
    assert_eq!(ia["rigid"], true);

    // A different marking is a different net (sim seeds on it).
    run(&mut b, "net.add_tokens", json!({"id": "I", "delta": 1}));
    let ic = run(&mut b, "analysis.canonical", json!({}));
    assert_ne!(ia["canonical_id"], ic["canonical_id"]);

    let mut p = example("producer-consumer");
    let ip = run(&mut p, "analysis.canonical", json!({}));
    assert_ne!(ia["canonical_id"], ip["canonical_id"]);

    // Same shape, different order of declaration.
    let mut x = net(
        &[("a", 1), ("b", 0)],
        &["go", "back"],
        &[
            ("a", "go", 1),
            ("go", "b", 1),
            ("b", "back", 1),
            ("back", "a", 1),
        ],
    );
    let mut y = net(
        &[("q", 0), ("p", 1)],
        &["u", "v"],
        &[("q", "u", 1), ("u", "p", 1), ("p", "v", 1), ("v", "q", 1)],
    );
    let ix = run(&mut x, "analysis.canonical", json!({}));
    let iy = run(&mut y, "analysis.canonical", json!({}));
    assert_eq!(ix["canonical_id"], iy["canonical_id"]);
}

#[test]
fn tic_tac_toe_has_board_symmetry() {
    let mut s = example("tic-tac-toe");
    let r = run(&mut s, "analysis.canonical", json!({}));
    let orbits: Vec<Vec<String>> = serde_json::from_value(r["orbits"].clone()).unwrap();
    let corners: Vec<String> = ["p00", "p02", "p20", "p22"].map(String::from).to_vec();
    assert!(orbits.contains(&corners), "{orbits:?}");
    assert!(orbits.contains(&["p01", "p10", "p12", "p21"].map(String::from).to_vec()));
    assert!(orbits.contains(&["x_win_anti", "x_win_diag"].map(String::from).to_vec()));
    assert!(r["generators"].as_u64().unwrap() > 0);
    let ss = sets(&s, &r["sets"]);
    assert_eq!(ss.len(), orbits.len());
    show_all(&mut s, &r["sets"]);

    let c = run(&mut s, "analysis.classify", json!({"scope": "places"}));
    let classes = c["classes"].as_array().unwrap();
    let corner = classes
        .iter()
        .find(|c| c["members"] == json!(corners))
        .unwrap_or_else(|| panic!("{c}"));
    assert_eq!(corner["kind"], "FungibleSet");
    assert_eq!(corner["verified"], true);
    assert_eq!(corner["id"], "p_*");
}

#[test]
fn classify_scopes() {
    let mut s = example("sir");
    // SIR's places are a state, not pools: sim would find no classes.
    let r = run(&mut s, "analysis.classify", json!({"scope": "pools"}));
    assert_eq!(r["classes"], json!([]));
    assert!(s
        .execute("analysis.classify", json!({"scope": "nope"}))
        .is_err());
    let mut s = net(
        &[("fork_0", 1), ("fork_1", 1), ("eat", 0)],
        &["take", "drop"],
        &[
            ("fork_0", "take", 1),
            ("fork_1", "take", 1),
            ("take", "eat", 1),
            ("eat", "drop", 1),
            ("drop", "fork_0", 1),
            ("drop", "fork_1", 1),
        ],
    );
    let r = run(&mut s, "analysis.classify", json!({}));
    assert_eq!(r["classes"][0]["id"], "fork_*", "{r}");
    assert_eq!(r["classes"][0]["verified"], true);
}

#[test]
fn large_or_symmetric_nets_are_refused_clearly() {
    // 130 token places: over the siphon search's 128-place mask.
    let names: Vec<String> = (0..130).map(|i| format!("p{i}")).collect();
    let places: Vec<(&str, i64)> = names.iter().map(|n| (n.as_str(), 0)).collect();
    let mut s = net(&places, &["t"], &[("p0", "t", 1)]);
    let e = s
        .execute("analysis.siphons", json!({}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("128-place cap"), "{e}");

    // A bidirectional ring of 60 places is too symmetric for the orbit
    // search's budget (sim refuses it too).
    let n = 60;
    let ps: Vec<String> = (0..n).map(|i| format!("r{i}")).collect();
    let ts: Vec<String> = (0..n).map(|i| format!("t{i}")).collect();
    let mut arcs: Vec<(&str, &str, i64)> = vec![];
    for i in 0..n {
        let (p, q, t) = (&ps[i], &ps[(i + 1) % n], &ts[i]);
        arcs.extend([(p.as_str(), t.as_str(), 1), (t.as_str(), q.as_str(), 1)]);
        arcs.extend([(q.as_str(), t.as_str(), 1), (t.as_str(), p.as_str(), 1)]);
    }
    let places: Vec<(&str, i64)> = ps.iter().map(|p| (p.as_str(), 1)).collect();
    let tr: Vec<&str> = ts.iter().map(String::as_str).collect();
    let mut s = net(&places, &tr, &arcs);
    let e = s
        .execute("analysis.canonical", json!({}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("budget"), "{e}");
    // classify still answers, leaving its classes as candidates.
    let r = run(&mut s, "analysis.classify", json!({}));
    assert!(r["orbits"].as_str().unwrap().contains("refused"), "{r}");
}

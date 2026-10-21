//! Ports, checked bindings, systems and pipeline runs (`system.*`).

use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn err(s: &mut Session, id: &str, args: Value) -> String {
    match s.execute(id, args) {
        Ok(v) => panic!("{id} should have been refused, got {v}"),
        Err(e) => e.to_string(),
    }
}

/// A pflow.xyz net: places (id, initial), transitions, arcs (source, target).
fn net(places: &[(&str, f64)], transitions: &[&str], arcs: &[(&str, &str)]) -> Value {
    let mut p = serde_json::Map::new();
    for (i, (id, init)) in places.iter().enumerate() {
        p.insert(
            (*id).into(),
            json!({"@type": "Place", "offset": i, "initial": [init], "capacity": [0], "x": 100 + 100 * i, "y": 100}),
        );
    }
    let mut t = serde_json::Map::new();
    for (i, id) in transitions.iter().enumerate() {
        t.insert(
            (*id).into(),
            json!({"@type": "Transition", "x": 150 + 100 * i, "y": 200}),
        );
    }
    let arcs: Vec<Value> = arcs
        .iter()
        .map(|(s, t)| json!({"@type": "Arrow", "source": s, "target": t, "weight": [1], "inhibitTransition": false}))
        .collect();
    json!({
        "@context": "https://pflow.xyz/schema",
        "@type": "PetriNet",
        "@version": "1.1",
        "token": ["https://pflow.xyz/tokens/black"],
        "places": p,
        "transitions": t,
        "arcs": arcs,
    })
}

/// `src`: 100 in stock shipped at rate 1 into `shipped` (output port);
/// `sink`: `arrive` (input port) fills `inbox`.
fn chain(s: &mut Session, from_unit: &str, to_unit: &str) {
    run(
        s,
        "model.import",
        json!({"name": "src", "json": net(&[("stock", 100.0), ("shipped", 0.0)], &["ship"], &[("stock", "ship"), ("ship", "shipped")])}),
    );
    run(
        s,
        "model.import",
        json!({"name": "sink", "json": net(&[("inbox", 0.0)], &["arrive"], &[("arrive", "inbox")])}),
    );
    run(
        s,
        "system.set_port",
        json!({"model": "src", "id": "shipped", "direction": "out", "kind": "Parcels", "unit": from_unit}),
    );
    run(
        s,
        "system.set_port",
        json!({"model": "sink", "id": "arrive", "direction": "in", "kind": "Parcels", "unit": to_unit}),
    );
}

fn bind(s: &mut Session, extra: Value) -> Value {
    let mut a = json!({"from_model": "src", "from_port": "shipped", "to_model": "sink", "to_port": "arrive"});
    for (k, v) in extra.as_object().unwrap() {
        a[k] = v.clone();
    }
    run(s, "system.bind", a)
}

fn last(v: &Value, model: &str, place: &str) -> f64 {
    v["final"][model][place].as_f64().unwrap()
}

#[test]
fn ports_persist_through_library_file_and_undo() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(
        &mut s,
        "system.set_port",
        json!({"id": "R", "direction": "output", "unit": "person", "description": "recovered"}),
    );
    run(
        &mut s,
        "system.set_port",
        json!({"id": "infect", "direction": "in"}),
    );
    let ports = run(&mut s, "system.ports", json!({"model": "current"}));
    assert_eq!(ports["ports"].as_array().unwrap().len(), 2);
    assert_eq!(ports["ports"][0]["element"], "place", "places first");
    assert!(s.is_dirty(), "a port is a document edit");

    // Saved into the library and read back.
    run(&mut s, "model.save", json!({"name": "epi"}));
    let got = run(&mut s, "model.get", json!({"name": "epi"}));
    assert_eq!(got["ports"][0]["id"], "R");
    assert_eq!(got["ports"][0]["unit"], "person");
    // A CID still seals with ports declared.
    run(&mut s, "file.cid", Value::Null);

    // Through a file.
    let path = std::env::temp_dir().join(format!("pn-systems-ports-{}.json", std::process::id()));
    let path = path.to_str().unwrap().to_string();
    run(&mut s, "file.save", json!({"path": path}));
    let mut t = Session::default();
    run(&mut t, "file.open", json!({"path": path}));
    let _ = std::fs::remove_file(&path);
    let ports = run(&mut t, "system.ports", json!({"model": "current"}));
    assert_eq!(ports["ports"].as_array().unwrap().len(), 2);
    assert_eq!(ports["ports"][0]["description"], "recovered");

    // Undoable on the open net; library models change in place.
    run(
        &mut t,
        "system.set_port",
        json!({"id": "R", "direction": "output", "remove": true}),
    );
    assert_eq!(
        run(&mut t, "system.ports", json!({"model": "current"}))["ports"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    run(&mut t, "edit.undo", Value::Null);
    assert_eq!(
        run(&mut t, "system.ports", json!({"model": "current"}))["ports"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(err(
        &mut t,
        "system.set_port",
        json!({"id": "nope", "direction": "in"})
    )
    .contains("no place or transition"));
}

#[test]
fn bind_checks_direction_units_conversion_and_delay() {
    let mut s = Session::default();
    chain(&mut s, "parcel", "order");

    // The input port named as the output end, and vice versa.
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "sink", "from_port": "arrive", "to_model": "src", "to_port": "shipped"}),
    );
    assert!(e.starts_with("no-output-port"), "{e}");
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "src", "from_port": "shipped", "to_model": "sink", "to_port": "inbox"}),
    );
    assert!(e.starts_with("no-input-port"), "{e}");
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "src", "from_port": "shipped", "to_model": "src", "to_port": "ship"}),
    );
    assert!(e.starts_with("self-loop"), "{e}");

    // parcel vs order: refused unless converted, and the conversion must
    // name exactly those units.
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "src", "from_port": "shipped", "to_model": "sink", "to_port": "arrive"}),
    );
    assert!(e.starts_with("unit-differs"), "{e}");
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "src", "from_port": "shipped", "to_model": "sink", "to_port": "arrive",
               "convert_from": "box", "convert_to": "order", "convert_factor": 2}),
    );
    assert!(e.starts_with("convert-unit-mismatch"), "{e}");
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "src", "from_port": "shipped", "to_model": "sink", "to_port": "arrive", "convert_from": "parcel"}),
    );
    assert!(e.contains("together"), "{e}");
    let ok = bind(
        &mut s,
        json!({"convert_from": "parcel", "convert_to": "order", "convert_factor": 3}),
    );
    assert_eq!(ok["check"]["verdict"], "ok");
    assert_eq!(ok["check"]["definition"], "binds-port-v2");
    assert_eq!(ok["check"]["unit"]["verdict"], "converted");
    assert_eq!(ok["check"]["unit"]["factor"], 3.0);
    assert_eq!(ok["id"].as_str().unwrap().len(), 24);
    assert_eq!(ok["existed"], false);
    let again = bind(
        &mut s,
        json!({"convert_from": "parcel", "convert_to": "order", "convert_factor": 3}),
    );
    assert_eq!(again["id"], ok["id"], "content-addressed");
    assert_eq!(again["existed"], true);

    // Delay and transform bounds.
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "src", "from_port": "shipped", "to_model": "sink", "to_port": "arrive",
               "convert_from": "parcel", "convert_to": "order", "convert_factor": 3, "delay": -1}),
    );
    assert!(e.starts_with("delay-invalid"), "{e}");
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "src", "from_port": "shipped", "to_model": "sink", "to_port": "arrive",
               "convert_from": "parcel", "convert_to": "order", "convert_factor": 3, "transform": 1e6}),
    );
    assert!(e.starts_with("scale-over-cap"), "{e}");

    // A kind mismatch.
    run(
        &mut s,
        "system.set_port",
        json!({"model": "sink", "id": "arrive", "direction": "in", "kind": "Orders", "unit": "order"}),
    );
    let e = err(
        &mut s,
        "system.bind",
        json!({"from_model": "src", "from_port": "shipped", "to_model": "sink", "to_port": "arrive",
               "convert_from": "parcel", "convert_to": "order", "convert_factor": 3}),
    );
    assert!(e.starts_with("kind-differs"), "{e}");

    // The stored binding is now refused on read (flagged legacy).
    let got = run(&mut s, "system.binding", json!({"id": ok["id"]}));
    assert_eq!(got["check"]["verdict"], "refused");
    assert!(got["legacy"].is_string());
    let list = run(&mut s, "system.bindings", json!({"model": "sink"}));
    assert_eq!(list.as_array().unwrap().len(), 1);
    run(&mut s, "system.unbind", json!({"id": ok["id"]}));
    assert!(run(&mut s, "system.bindings", Value::Null)
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn chain_runs_and_the_sink_receives_the_flow() {
    let mut s = Session::default();
    chain(&mut s, "parcel", "parcel");
    let b = bind(&mut s, json!({}));
    assert_eq!(b["check"]["unit"]["verdict"], "agree");
    let created = run(
        &mut s,
        "system.create",
        json!({"name": "ship", "bindings": [b["id"]]}),
    );
    assert_eq!(created["implied"], json!(["sink", "src"]));
    assert_eq!(created["pipeline"]["order"], json!(["src", "sink"]));

    let out = run(&mut s, "system.run", json!({"name": "ship", "hours": 8}));
    assert_eq!(out["verdict"], "acyclic");
    assert_eq!(out["order"], json!(["src", "sink"]));
    let shipped = last(&out, "src", "shipped");
    let inbox = last(&out, "sink", "inbox");
    // 100 (1 - e^-8) shipped, and every bit of it arrived.
    assert!(
        (shipped - 100.0 * (1.0 - (-8.0f64).exp())).abs() < 0.05,
        "{shipped}"
    );
    assert!(
        (inbox - shipped).abs() < 1e-6,
        "inbox {inbox} vs shipped {shipped}"
    );
    let flow = &out["flows"][0];
    assert!((flow["delivered"].as_f64().unwrap() - shipped).abs() < 1e-6);
    assert_eq!(flow["schedule"].as_array().unwrap().len(), 60);

    // A TimeSeries overlay, one group per model.
    let ts = s.artifact(out["timeseries"].as_str().unwrap()).unwrap();
    match &ts.data {
        ArtifactData::TimeSeries { x, series, .. } => {
            assert_eq!(x.len(), 61);
            assert_eq!(*x.last().unwrap(), 8.0);
            let groups: Vec<&str> = series.iter().filter_map(|s| s.group.as_deref()).collect();
            assert!(groups.contains(&"src") && groups.contains(&"sink"));
        }
        other => panic!("expected a time series, got {}", other.kind()),
    }

    // A transform scales the flow; a delay holds it back behind the
    // declared rate (1/h here) and leaves some in transit.
    let b2 = bind(&mut s, json!({"transform": "0.5", "delay": 2}));
    let out2 = run(
        &mut s,
        "system.run",
        json!({"bindings": [b2["id"]], "graph": true}),
    );
    let flow = &out2["flows"][0];
    let delivered = flow["delivered"].as_f64().unwrap();
    let transit = flow["inTransit"].as_f64().unwrap();
    let produced = 0.5 * shipped;
    assert!(
        (delivered - 2.0 - (produced - transit)).abs() < 1e-6,
        "seed 2×1 plus what arrived: {delivered} vs {produced} − {transit}"
    );
    assert!(out2["graph"].is_string());
    assert!(out2["assumptions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a.as_str().unwrap().contains("declared, not measured")));

    // sim's own engine runs the same pipeline, deterministically per seed.
    let a1 = run(
        &mut s,
        "system.run",
        json!({"name": "ship", "engine": "ssa", "seed": 7, "realizations": 4}),
    );
    let a2 = run(
        &mut s,
        "system.run",
        json!({"name": "ship", "engine": "ssa", "seed": 7, "realizations": 4}),
    );
    assert_eq!(a1["final"], a2["final"]);
    assert!(last(&a1, "sink", "inbox") > 50.0);
}

/// A: `recv` (input) fills `buf`, `proc` moves it to `done` (output).
/// B: `recv_b` (input) fills `got` (output).
fn loop_models(s: &mut Session) -> (String, String) {
    run(
        s,
        "model.import",
        json!({"name": "a", "json": net(&[("buf", 0.0), ("done", 0.0)], &["recv", "proc"], &[("recv", "buf"), ("buf", "proc"), ("proc", "done")])}),
    );
    run(
        s,
        "model.import",
        json!({"name": "b", "json": net(&[("got", 0.0)], &["recv_b"], &[("recv_b", "got")])}),
    );
    for (m, id, dir) in [
        ("a", "recv", "in"),
        ("a", "done", "out"),
        ("b", "recv_b", "in"),
        ("b", "got", "out"),
    ] {
        run(
            s,
            "system.set_port",
            json!({"model": m, "id": id, "direction": dir}),
        );
    }
    let ab = run(
        s,
        "system.bind",
        json!({"from_model": "a", "from_port": "done", "to_model": "b", "to_port": "recv_b"}),
    );
    let ba = run(
        s,
        "system.bind",
        json!({"from_model": "b", "from_port": "got", "to_model": "a", "to_port": "recv", "transform": 0.5, "delay": 2}),
    );
    (
        ab["id"].as_str().unwrap().into(),
        ba["id"].as_str().unwrap().into(),
    )
}

#[test]
fn delayed_loop_settles_to_a_fixed_point() {
    let mut s = Session::default();
    let (ab, ba) = loop_models(&mut s);
    let created = run(
        &mut s,
        "system.create",
        json!({"name": "loop", "bindings": [ab, ba]}),
    );
    assert_eq!(created["pipeline"]["loops"][0]["cut"], json!(ba));
    assert!(created["pipeline"]["maxHours"].as_f64().unwrap() >= 8.0);

    let out = run(&mut s, "system.run", json!({"name": "loop", "hours": 8}));
    assert_eq!(out["verdict"], "fixed-point", "{}", out["loop"]);
    let lp = &out["loop"];
    let sweeps = lp["sweepsRun"].as_u64().unwrap();
    let needed = lp["components"][0]["sweepsNeeded"].as_u64().unwrap();
    assert!((2..=needed).contains(&sweeps), "{sweeps} of {needed}");
    let res = lp["residual"].as_array().unwrap();
    assert_eq!(res.last().unwrap()["maxDiff"], 0.0);
    assert_eq!(res.last().unwrap()["exactThrough"], 8.0);
    assert!(out["assumptions"][0]
        .as_str()
        .unwrap()
        .contains("settled exactly"));
    // The answer is a fixed point: A's input after the delay is half of B's
    // output two hours earlier, and B received everything A finished.
    let done = last(&out, "a", "done");
    let got = last(&out, "b", "got");
    assert!((got - done).abs() < 1e-6, "{got} vs {done}");
    // Deterministic.
    let again = run(&mut s, "system.run", json!({"name": "loop", "hours": 8}));
    assert_eq!(again["final"], out["final"]);

    // A tolerance can stop earlier; too few sweeps do not settle.
    let loose = run(
        &mut s,
        "system.run",
        json!({"name": "loop", "tolerance": 1e3}),
    );
    assert_eq!(loose["verdict"], "converged");
    assert_eq!(loose["loop"]["sweepsRun"], 2);
    let short = run(&mut s, "system.run", json!({"name": "loop", "sweeps": 2}));
    assert_eq!(short["verdict"], "not-settled");
    assert!(short.get("timeseries").is_none());

    // Too long a horizon for the delay: the lag is refused, naming a remedy.
    let e = err(&mut s, "system.run", json!({"name": "loop", "hours": 60}));
    assert!(e.starts_with("loop-lag"), "{e}");
}

#[test]
fn undelayed_cycle_is_refused() {
    let mut s = Session::default();
    let (ab, _) = loop_models(&mut s);
    let ba = run(
        &mut s,
        "system.bind",
        json!({"from_model": "b", "from_port": "got", "to_model": "a", "to_port": "recv"}),
    );
    let e = err(
        &mut s,
        "system.create",
        json!({"name": "bad", "bindings": [ab, ba["id"]]}),
    );
    assert!(e.starts_with("closes-loop"), "{e}");
    let e = err(&mut s, "system.run", json!({"bindings": [ab, ba["id"]]}));
    assert!(e.starts_with("closes-loop"), "{e}");
    assert!(e.contains("a -> b -> a"), "{e}");
}

#[test]
fn show_graph_has_a_node_per_model_and_an_edge_per_binding() {
    let mut s = Session::default();
    let (ab, ba) = loop_models(&mut s);
    chain(&mut s, "", "");
    let b = bind(&mut s, json!({}));
    run(
        &mut s,
        "system.create",
        json!({"name": "all", "models": ["a", "b", "src", "sink"], "bindings": [ab, ba, b["id"]]}),
    );
    let v = run(&mut s, "system.show", json!({"name": "all"}));
    let g = s.artifact(v["graph"].as_str().unwrap()).unwrap();
    match &g.data {
        ArtifactData::Graph { nodes, edges } => {
            assert_eq!(nodes.len(), 4);
            assert_eq!(edges.len(), 3);
            assert!(edges.iter().any(|e| e.label == "got→recv (+2 h)"));
            let src = nodes.iter().find(|n| n.id == "src").unwrap();
            assert_eq!(src.data["ports"][0]["id"], "shipped");
        }
        other => panic!("expected a graph, got {}", other.kind()),
    }
    assert!(g.data.validate().is_ok());
    assert_eq!(v["shape"], "pipeline");
    // What binds to each port: shipped is bound to sink.arrive.
    let row = v["ports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["model"] == "src" && r["port"] == "shipped")
        .unwrap();
    assert_eq!(row["bound"][0]["model"], "sink");
    let cands = |row: &Value| -> Vec<String> {
        row["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                format!(
                    "{}.{}",
                    c["model"].as_str().unwrap(),
                    c["port"].as_str().unwrap()
                )
            })
            .collect()
    };
    // Kinds are compared exactly: a.recv (no kind) is not a candidate for
    // a Parcels output, b.recv_b is one for a.done.
    assert_eq!(cands(row), vec!["sink.arrive"]);
    let done = v["ports"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["model"] == "a" && r["port"] == "done")
        .unwrap();
    assert_eq!(cands(done), vec!["b.recv_b"]);
    let list = run(&mut s, "system.list", Value::Null);
    assert_eq!(list.as_array().unwrap().len(), 1);
    let e = err(&mut s, "system.unbind", json!({"id": ab}));
    assert!(e.contains("system.delete"), "{e}");
    run(&mut s, "system.delete", json!({"name": "all"}));
    run(&mut s, "system.unbind", json!({"id": ab}));
}

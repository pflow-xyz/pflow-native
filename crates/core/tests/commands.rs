use pflow_native_core::{Session, Step};
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn step(command: &str, args: Value) -> Step {
    Step {
        command: command.into(),
        args,
    }
}

/// p0 -(t0)-> p1 with 2 tokens in p0.
fn chain() -> Session {
    let mut s = Session::default();
    run(
        &mut s,
        "net.add_place",
        json!({"x": 100, "y": 100, "initial": 2}),
    );
    run(&mut s, "net.add_transition", json!({"x": 200, "y": 100}));
    run(&mut s, "net.add_place", json!({"x": 300, "y": 100}));
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "p0", "target": "t0"}),
    );
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "t0", "target": "p1"}),
    );
    s
}

#[test]
fn build_fire_undo() {
    let mut s = chain();
    assert_eq!(run(&mut s, "sim.enabled", Value::Null), json!(["t0"]));
    run(&mut s, "sim.fire", json!({"id": "t0"}));
    run(&mut s, "sim.fire", json!({"id": "t0"}));
    assert_eq!(s.tokens("p0"), 0);
    assert_eq!(s.tokens("p1"), 2);
    let err = s
        .execute("sim.fire", json!({"id": "t0"}))
        .unwrap_err()
        .to_string();
    assert!(err.contains("needs 1 token"), "{err}");
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.tokens("p1"), 1);
    run(&mut s, "edit.redo", Value::Null);
    assert_eq!(s.tokens("p1"), 2);
    run(&mut s, "sim.reset", Value::Null);
    assert_eq!(s.tokens("p0"), 2);
}

#[test]
fn rejects_bad_arcs_and_args() {
    let mut s = chain();
    assert!(s
        .execute("net.add_arc", json!({"source": "p0", "target": "p1"}))
        .is_err());
    assert!(s
        .execute("net.add_arc", json!({"source": "p0", "target": "t0"}))
        .is_err());
    let e = s
        .execute("net.add_place", json!({"x": 1}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("missing required argument \"y\""), "{e}");
    let e = s
        .execute("net.add_place", json!({"x": 1, "y": 2, "z": 3}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("unknown argument"), "{e}");
    let e = s
        .execute("net.add_plac", Value::Null)
        .unwrap_err()
        .to_string();
    assert!(e.contains("did you mean \"net.add_place\""), "{e}");
}

#[test]
fn inhibitor_blocks() {
    let mut s = chain();
    run(
        &mut s,
        "net.add_place",
        json!({"id": "guard", "x": 200, "y": 0, "initial": 1}),
    );
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "guard", "target": "t0", "inhibit": true}),
    );
    assert_eq!(run(&mut s, "sim.enabled", Value::Null), json!([]));
    run(
        &mut s,
        "net.add_tokens",
        json!({"id": "guard", "delta": -1}),
    );
    assert_eq!(run(&mut s, "sim.enabled", Value::Null), json!(["t0"]));
}

#[test]
fn batch_is_one_undo_step_and_rolls_back() {
    let mut s = Session::default();
    let steps = json!([
        {"command": "net.add_place", "args": {"x": 0, "y": 0}},
        {"command": "net.add_transition", "args": {"x": 50, "y": 0}},
        {"command": "net.add_arc", "args": {"source": "p0", "target": "t0"}}
    ]);
    run(&mut s, "command.batch", json!({"steps": steps}));
    assert_eq!(s.doc.arcs.len(), 1);
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.doc.places.is_empty() && s.doc.transitions.is_empty());

    let bad = json!([
        {"command": "net.add_place", "args": {"x": 0, "y": 0}},
        {"command": "net.add_arc", "args": {"source": "p0", "target": "nope"}}
    ]);
    let e = s
        .execute("command.batch", json!({"steps": bad}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("step 1"), "{e}");
    assert!(s.doc.places.is_empty(), "rolled back");
}

#[test]
fn macro_record_and_play() {
    let mut s = Session::default();
    run(&mut s, "macro.record", Value::Null);
    run(&mut s, "net.add_place", json!({"x": 0, "y": 0}));
    run(&mut s, "view.zoom_in", Value::Null); // not journaled
    run(&mut s, "net.add_place", json!({"x": 100, "y": 0}));
    let r = run(&mut s, "macro.stop", json!({"name": "two"}));
    assert_eq!(r["steps"], 2);
    run(&mut s, "macro.play", json!({"name": "two"}));
    assert_eq!(s.doc.places.len(), 4);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.doc.places.len(), 2, "a macro is one undo step");
    let _ = step("x", Value::Null);
}

#[test]
fn ode_sir_conserves_population() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": 0.005}),
    );
    let r = run(&mut s, "sim.ode", json!({"t_end": 20}));
    let fin = &r["final"];
    let total =
        fin["S"].as_f64().unwrap() + fin["I"].as_f64().unwrap() + fin["R"].as_f64().unwrap();
    assert!((total - 1000.0).abs() < 1e-3, "{fin}");
    assert!(fin["R"].as_f64().unwrap() > 500.0, "{fin}");
    let csv = run(&mut s, "export.artifact", json!({"format": "csv"}));
    assert!(csv.as_str().unwrap().starts_with("t,S,I,R\n"));
    let svg = run(&mut s, "export.artifact", json!({"format": "svg"}));
    assert!(svg.as_str().unwrap().contains("<polyline"));
}

#[test]
fn colored_coffee_shop_fires_per_color() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let en = run(&mut s, "sim.enabled", Value::Null);
    assert!(
        en.as_array().unwrap().contains(&json!("BrewCoffee")),
        "{en}"
    );
    run(&mut s, "sim.fire", json!({"id": "BrewCoffee"}));
    assert_eq!(s.marking["WaitingCustomers"], vec![1, 0, 0]);
    assert_eq!(s.marking["CoffeeBeans"], vec![0, 4, 0]);
    assert_eq!(s.marking["EmptyCups"], vec![0, 0, 4]);
    assert_eq!(s.marking["ServedCoffee"], vec![1, 0, 0]);
    let r = run(&mut s, "sim.ode", json!({"t_end": 1}));
    assert!(
        r["final"].get("CoffeeBeans.brown").is_some(),
        "{}",
        r["final"]
    );
}

#[test]
fn save_round_trip_and_svg() {
    let mut s = chain();
    let dir = std::env::temp_dir().join(format!("pflow-native-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("chain.json");
    run(&mut s, "file.save", json!({"path": path.to_str().unwrap()}));
    assert!(!s.is_dirty());
    let mut t = Session::default();
    run(&mut t, "file.open", json!({"path": path.to_str().unwrap()}));
    // The reloaded document also carries the file's own fields (`@type`,
    // `offset`, ...) verbatim; what it writes is identical.
    assert_eq!(t.doc.to_value(), s.doc.to_value());
    assert_eq!(t.doc.places[0].initial, s.doc.places[0].initial);
    let svg = run(&mut t, "export.svg", Value::Null);
    assert!(svg.as_str().unwrap().contains("<circle"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn every_command_has_unique_id_and_valid_schema() {
    let s = Session::default();
    for c in s.registry().commands() {
        assert!(c.id.contains('.'), "{}", c.id);
        let schema = c.args_schema();
        assert_eq!(schema["type"], "object");
    }
}

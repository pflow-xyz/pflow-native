//! game.value and game.move: pflow-metamodel's search, run as a SearchPlan
//! names it, on the bundled tic-tac-toe.

use pflow_metamodel::game::Solver;
use pflow_native_core::analysis::Structure;
use pflow_native_core::game::{game_rules, Dense};
use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn refused(s: &mut Session, id: &str, args: Value) -> String {
    match s.execute(id, args) {
        Ok(v) => panic!("{id} ran: {v}"),
        Err(e) => e.to_string(),
    }
}

fn ttt() -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "tic-tac-toe"}));
    s
}

fn rules() -> Value {
    json!({"sides": {"x_": 1, "o_": -1}, "reward": {"win_x": 1, "win_o": -1}, "auto": ["x_win_", "o_win_"]})
}

fn with(mut base: Value, extra: Value) -> Value {
    for (k, v) in extra.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

#[test]
fn tic_tac_toe_is_a_draw_and_its_best_move_keeps_it() {
    let mut s = ttt();
    let v = run(&mut s, "game.value", rules());
    assert_eq!(v["value"], json!(0.0), "{v}");
    assert_eq!(v["exact"], true);
    assert_eq!(v["mover"], json!(1.0));
    assert_eq!(v["moves"], 9);
    assert!(v["nodes"].as_u64().unwrap() > 0, "{v}");
    let m = run(&mut s, "game.move", rules());
    assert_eq!(m["value"], v["value"], "{m}");
    assert_eq!(m["exact"], true);
    let t = m["transition"].as_str().expect("a move").to_string();
    // Read-only: nothing fired.
    assert_eq!(run(&mut s, "game.value", rules())["value"], v["value"]);
    run(&mut s, "sim.fire", json!({ "id": t }));
    let after = run(&mut s, "game.value", rules());
    assert_eq!(after["value"], v["value"], "after {t}: {after}");
    assert_eq!(after["mover"], json!(-1.0));
}

#[test]
fn an_ode_ordered_root_finds_the_same_value() {
    let mut s = ttt();
    let plan = json!({"root": {"order": ["ode", "non_losing"]}});
    let v = run(&mut s, "game.value", with(rules(), json!({ "plan": plan })));
    assert_eq!(v["value"], json!(0.0), "{v}");
    // A plan given as a JSON string reads the same.
    let m = run(
        &mut s,
        "game.move",
        with(rules(), json!({ "plan": plan.to_string() })),
    );
    assert_eq!(m["value"], json!(0.0), "{m}");
    assert_eq!(m["exact"], true);
}

#[test]
fn refuses_a_clock_a_huge_budget_missing_rules_and_unknown_pieces() {
    let mut s = ttt();
    let ms = json!({"drivers": [{"budget": {"nodes": 1000, "ms": 50}}]});
    let e = refused(&mut s, "game.value", with(rules(), json!({ "plan": ms })));
    assert!(e.contains("budget.ms") && e.contains("machine"), "{e}");
    let e = refused(
        &mut s,
        "game.move",
        with(rules(), json!({"nodes": 200_000_000})),
    );
    assert!(e.contains("nodes"), "{e}");
    let big = json!({"drivers": [{"budget": {"nodes": 200_000_000}}]});
    let e = refused(&mut s, "game.value", with(rules(), json!({ "plan": big })));
    assert!(e.contains("at most"), "{e}");
    let both = json!({"drivers": [{"budget": {"nodes": 10}}]});
    let e = refused(
        &mut s,
        "game.value",
        with(rules(), json!({"plan": both, "nodes": 10})),
    );
    assert!(e.contains("once"), "{e}");
    let memo = json!({"memo": {"entries": 1u64 << 24}});
    let e = refused(&mut s, "game.value", with(rules(), json!({ "plan": memo })));
    assert!(e.contains("memo"), "{e}");
    let e = refused(
        &mut s,
        "game.value",
        json!({"reward": {"win_x": 1, "win_o": -1}}),
    );
    assert!(e.contains("sides"), "{e}");
    let bogus = json!({"order": ["bogus"]});
    let e = refused(&mut s, "game.move", with(rules(), json!({ "plan": bogus })));
    assert!(e.contains("unknown order piece \"bogus\""), "{e}");
}

#[test]
fn a_tiny_budget_gives_up_or_guesses() {
    let mut s = ttt();
    let tiny = with(rules(), json!({"nodes": 10}));
    let v = run(&mut s, "game.value", tiny.clone());
    assert_eq!(v["value"], Value::Null, "{v}");
    assert_eq!(v["exact"], false);
    let m = run(&mut s, "game.move", tiny.clone());
    assert_eq!(m["transition"], Value::Null, "{m}");
    assert_eq!(m["exact"], false);
    assert!(m["reason"].as_str().unwrap().contains("past the budget"));
    let play = with(tiny, json!({"plan": {"mode": "play"}}));
    let g = run(&mut s, "game.move", play);
    assert!(g["transition"].is_string(), "{g}");
    assert_eq!(g["exact"], false);
}

#[test]
fn a_final_position_is_its_score() {
    let mut s = ttt();
    for t in [
        "x_play_00",
        "o_play_10",
        "x_play_01",
        "o_play_11",
        "x_play_02",
    ] {
        run(&mut s, "sim.fire", json!({ "id": t }));
    }
    let v = run(&mut s, "game.value", rules());
    assert_eq!(v["value"], json!(1.0), "{v}");
    assert_eq!(v["exact"], true);
    assert_eq!(v["nodes"], 0);
    assert_eq!(v["mover"], Value::Null);
    let m = run(&mut s, "game.move", rules());
    assert_eq!(m["transition"], Value::Null, "{m}");
    assert_eq!(m["value"], json!(1.0));
}

/// A label cache as rnn writes one, for the positions after each first
/// move: exact values of every move there, in the mover's view.
fn labels(s: &Session) -> String {
    let st = Structure::new(&s.doc, &s.marking).expect("structure");
    let d = Dense::new(&st, &game_rules(&st, &rules()).expect("rules")).expect("dense");
    let r = &d.rules;
    let start = d.settled(&st, &s.marking).expect("settle");
    let mut solver = Solver::new(r, usize::MAX);
    let mut out = vec![d.label_header(usize::MAX).to_string()];
    for t in r.moves(&start) {
        let mk = r.play(&start, t).expect("play");
        let moves = r.moves(&mk);
        let side = r.mover(&moves).expect("one side");
        let vals: Vec<f64> = moves
            .iter()
            .map(|&u| side * solver.value(&r.play(&mk, u).unwrap()).unwrap().unwrap() + 0.0)
            .collect();
        out.push(json!({"mk": mk, "vals": vals}).to_string());
    }
    out.join("\n")
}

#[test]
fn a_book_keeps_the_value_and_saves_nodes() {
    let mut s = ttt();
    let plain = run(&mut s, "game.value", rules());
    let book = labels(&s);
    let booked = run(&mut s, "game.value", with(rules(), json!({ "book": book })));
    assert_eq!(booked["value"], plain["value"], "{booked}");
    assert!(booked["book"].as_u64().unwrap() >= 9, "{booked}");
    assert!(
        booked["nodes"].as_u64() < plain["nodes"].as_u64(),
        "{booked} vs {plain}"
    );
    let m = run(&mut s, "game.move", with(rules(), json!({ "book": book })));
    assert_eq!(m["value"], plain["value"], "{m}");
    // A book for other rules is refused.
    let other = json!({"sides": {"x_": 1, "o_": -1}, "reward": {"win_x": 1}, "auto": ["x_win_", "o_win_"], "book": book});
    let e = refused(&mut s, "game.value", other);
    assert!(e.contains("book"), "{e}");
}

#[test]
fn the_same_plan_and_budget_give_the_same_result() {
    let args = with(
        rules(),
        json!({"plan": {"root": {"order": ["ode"]}}, "nodes": 3000}),
    );
    let once = |id: &str| run(&mut ttt(), id, args.clone());
    for id in ["game.value", "game.move"] {
        assert_eq!(once(id), once(id), "{id}");
    }
    let full = |id: &str| run(&mut ttt(), id, rules());
    assert_eq!(full("game.move"), full("game.move"));
}

#[test]
fn runs_as_a_background_job() {
    assert!(pflow_native_core::jobs::allowed("game.value"));
    assert!(pflow_native_core::jobs::allowed("game.move"));
}

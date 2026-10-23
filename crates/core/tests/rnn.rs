//! Experimental RNN policies (feature `rnn`): rnn.train, rnn.play and
//! rnn.self_play on the bundled nets. `make test-rnn`.
#![cfg(feature = "rnn")]

use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn example(name: &str) -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": name}));
    s
}

fn small() -> Value {
    json!({"hidden": 16, "games": 60, "epochs": 15, "seed": 3})
}

#[test]
fn train_learns_toward_the_uniform_play_floor() {
    let mut s = example("tic-tac-toe");
    let out = run(&mut s, "rnn.train", small());
    let curve = s.artifact(out["artifact"].as_str().unwrap()).unwrap();
    let ArtifactData::TimeSeries { series, .. } = &curve.data else {
        panic!("loss is a time series")
    };
    let loss = &series[0].values;
    let floor = f(&out["training"]["floor"]);
    assert_eq!(loss.len(), 15);
    assert!(loss.last().unwrap() < &loss[0], "loss fell: {loss:?}");
    assert!(
        loss.last().unwrap() > &(floor * 0.9),
        "no better than imitating uniform play allows"
    );
    let model = s.artifact(out["model"].as_str().unwrap()).unwrap();
    let ArtifactData::Report(m) = &model.data else {
        panic!("model is a report")
    };
    assert_eq!(m["kind"], "rnn-model");
    assert_eq!(m["transitions"].as_array().unwrap().len(), 34);
}

#[test]
fn training_is_reproducible() {
    let a = run(&mut example("tic-tac-toe"), "rnn.train", small());
    let b = run(&mut example("tic-tac-toe"), "rnn.train", small());
    assert_eq!(a["training"], b["training"]);
}

#[test]
fn a_played_game_is_a_trace_the_firing_rule_accepts() {
    let mut s = example("tic-tac-toe");
    run(&mut s, "rnn.train", small());
    let g = run(&mut s, "rnn.play", json!({}));
    let steps = g["steps"].as_u64().unwrap();
    assert!(
        (5..=12).contains(&steps),
        "a tic-tac-toe game: {steps} firings"
    );
    assert_eq!(g["capped"], false);
    for m in g["moves"].as_array().unwrap() {
        let p = f(&m["p"]);
        assert!(p > 0.0 && p <= 1.0);
    }
    let w = run(
        &mut s,
        "provenance.check_witness",
        json!({"artifact": g["trace"]}),
    );
    assert_eq!(w["verdict"], "valid", "{w}");
    // The canvas can replay it.
    run(&mut s, "sim.goto", json!({"id": g["trace"], "step": steps}));
}

#[test]
fn greedy_play_is_deterministic_and_sampled_play_is_seeded() {
    let mut s = example("tic-tac-toe");
    run(&mut s, "rnn.train", small());
    let a = run(&mut s, "rnn.play", json!({}));
    let b = run(&mut s, "rnn.play", json!({}));
    assert_eq!(a["moves"], b["moves"]);
    let c = run(&mut s, "rnn.play", json!({"stochastic": true, "seed": 9}));
    let d = run(&mut s, "rnn.play", json!({"stochastic": true, "seed": 9}));
    assert_eq!(c["moves"], d["moves"]);
}

#[test]
fn self_play_scores_games_by_reward_and_stores_a_new_model() {
    let mut s = example("tic-tac-toe");
    let first = run(&mut s, "rnn.train", small());
    let out = run(
        &mut s,
        "rnn.self_play",
        json!({"reward": {"win_x": 1, "win_o": -1}, "rounds": 4, "games": 20, "seed": 2}),
    );
    assert_ne!(out["model"], first["model"]);
    assert_eq!(out["from_model"], first["model"]);
    for k in ["positive", "zero", "negative"] {
        let share = f(&out["last"][k]);
        assert!((0.0..=1.0).contains(&share));
    }
    // Zero-sum: each side is rewarded for its own result.
    let zs = run(
        &mut s,
        "rnn.self_play",
        json!({"reward": {"win_x": 1, "win_o": -1}, "sides": {"x_": 1, "o_": -1}, "rounds": 4, "games": 20, "seed": 2}),
    );
    assert_eq!(zs["from_model"], out["model"]);
    assert!(s
        .execute(
            "rnn.self_play",
            json!({"reward": {"win_x": 1}, "sides": {"q_": 1}})
        )
        .is_err());
    // The newest model is the self-played one, and it plays.
    let g = run(&mut s, "rnn.play", json!({}));
    assert_eq!(g["model"], zs["model"]);
}

#[test]
fn refuses_bad_arguments_and_other_nets() {
    let mut s = example("tic-tac-toe");
    assert!(s.execute("rnn.play", json!({})).is_err(), "no model yet");
    assert!(s.execute("rnn.train", json!({"hidden": 0})).is_err());
    assert!(s.execute("rnn.train", json!({"games": 1.5})).is_err());
    assert!(s
        .execute("rnn.train", json!({"learning_rate": -1}))
        .is_err());
    run(&mut s, "rnn.train", small());
    assert!(
        s.execute("rnn.self_play", json!({})).is_err(),
        "reward is required"
    );
    assert!(s
        .execute("rnn.self_play", json!({"reward": {"nowhere": 1}}))
        .is_err());
    // An edit keeps the stored model but changes the net under it.
    run(
        &mut s,
        "net.add_place",
        json!({"x": 0, "y": 0, "id": "extra"}),
    );
    let e = s.execute("rnn.play", json!({})).unwrap_err().to_string();
    assert!(e.contains("different places or transitions"), "{e}");
}

#[test]
fn games_on_a_net_that_never_stops_are_capped() {
    let mut s = Session::default();
    run(
        &mut s,
        "net.add_place",
        json!({"x": 0, "y": 0, "id": "a", "initial": 1}),
    );
    run(
        &mut s,
        "net.add_place",
        json!({"x": 200, "y": 0, "id": "b"}),
    );
    run(
        &mut s,
        "net.add_transition",
        json!({"x": 100, "y": -50, "id": "go"}),
    );
    run(
        &mut s,
        "net.add_transition",
        json!({"x": 100, "y": 50, "id": "back"}),
    );
    for (src, dst) in [("a", "go"), ("go", "b"), ("b", "back"), ("back", "a")] {
        run(&mut s, "net.add_arc", json!({"source": src, "target": dst}));
    }
    run(
        &mut s,
        "rnn.train",
        json!({"games": 5, "epochs": 2, "max_steps": 30}),
    );
    let g = run(&mut s, "rnn.play", json!({"max_steps": 30}));
    assert_eq!(g["steps"], 30);
    assert_eq!(g["capped"], true);
}

// ---------------------------------------------------------- the minimax teacher

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
fn the_teacher_solves_tic_tac_toe_and_the_model_opens_in_the_centre() {
    let mut s = example("tic-tac-toe");
    let out = run(
        &mut s,
        "rnn.imitate",
        with(rules(), json!({"games": 300, "epochs": 40})),
    );
    // Every position of the game, solved: every opening draws with best
    // play, and the centre is the best tie-break against random play.
    assert_eq!(out["training"]["positions_solved"], 5478);
    let opening = &out["teacher_opening"];
    assert_eq!(opening["best"], "x_play_11");
    for m in opening["moves"].as_array().unwrap() {
        assert_eq!(f(&m["value"]), 0.0, "{m}");
    }
    // The acceptance test: the trained model plays the centre first.
    let p = run(&mut s, "rnn.policy", json!({}));
    assert_eq!(p["moves"][0]["transition"], "x_play_11", "{p}");
    assert!(s
        .highlight
        .as_ref()
        .unwrap()
        .weights
        .contains_key("x_play_11"));
    // Against random play, X wins more than it loses. (This small RNN is
    // not perfect: against a minimax that varies among its best moves it
    // loses; the MLP test below is the no-loss one.)
    let e = run(&mut s, "rnn.evaluate", with(rules(), json!({"games": 100})));
    let x = &e["results"][0];
    assert_eq!(x["side"], "x_");
    assert!(x["wins"].as_u64() > x["losses"].as_u64(), "{e}");
}

#[test]
fn the_teacher_shows_move_values_on_the_canvas() {
    let mut s = example("tic-tac-toe");
    let p = run(
        &mut s,
        "rnn.policy",
        with(rules(), json!({"teacher": true})),
    );
    assert_eq!(p["moves"].as_array().unwrap().len(), 9);
    assert_eq!(s.highlight.as_ref().unwrap().weights.len(), 9);
}

#[test]
fn two_sided_commands_need_sides_and_one_mover() {
    let mut s = example("tic-tac-toe");
    let e = s
        .execute("rnn.imitate", json!({"reward": {"win_x": 1}}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("sides is required"), "{e}");
    // Without `auto`, a win claim is enabled on the other side's turn.
    let mut s = example("tic-tac-toe");
    let e = s
        .execute(
            "rnn.imitate",
            json!({"sides": {"x_": 1, "o_": -1}, "reward": {"win_x": 1, "win_o": -1}, "games": 5, "epochs": 1}),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("auto"), "{e}");
    assert!(s.execute("rnn.train", json!({"auto": ["nope_"]})).is_err());
    assert!(s
        .execute("rnn.evaluate", with(rules(), json!({"opponent": "god"})))
        .is_err());
}

#[test]
fn an_mlp_policy_learns_tic_tac_toe_from_the_teacher() {
    let mut s = example("tic-tac-toe");
    let out = run(
        &mut s,
        "rnn.imitate",
        with(
            rules(),
            json!({"arch": "mlp", "layers": [64, 32], "games": 1000, "epochs": 40}),
        ),
    );
    let m = s.artifact(out["model"].as_str().unwrap()).unwrap();
    let ArtifactData::Report(r) = &m.data else {
        panic!("the model is a report")
    };
    assert_eq!(r["arch"], "mlp");
    assert_eq!(r["layers"], json!([64, 32]));
    assert_eq!(r["weights"]["layers"].as_array().unwrap().len(), 3);
    assert!(m.title.starts_with("MLP model"), "{}", m.title);
    // It opens in the centre and never loses to perfect play, which picks
    // at random among its best moves, so the games cover many lines.
    let p = run(&mut s, "rnn.policy", json!({}));
    assert_eq!(p["moves"][0]["transition"], "x_play_11", "{p}");
    let e = run(
        &mut s,
        "rnn.evaluate",
        with(rules(), json!({"opponent": "minimax", "games": 100})),
    );
    for side in e["results"].as_array().unwrap() {
        assert_eq!(side["losses"], 0, "{e}");
    }
    // The other commands take it too: play, and self-play to a new MLP.
    let g = run(&mut s, "rnn.play", json!({"auto": ["x_win_", "o_win_"]}));
    assert!(g["moves_chosen"].as_u64().unwrap() >= 5, "{g}");
    let sp = run(
        &mut s,
        "rnn.self_play",
        with(rules(), json!({"rounds": 2, "games": 20})),
    );
    let next = s.artifact(sp["model"].as_str().unwrap()).unwrap();
    let ArtifactData::Report(r) = &next.data else {
        panic!("a report")
    };
    assert_eq!(r["arch"], "mlp");
}

#[test]
fn arch_and_its_shape_arguments_are_checked() {
    let mut s = example("tic-tac-toe");
    for (args, needle) in [
        (json!({"arch": "lstm"}), "arch must be rnn or mlp"),
        (json!({"layers": [8]}), "layers is for arch mlp"),
        (
            json!({"arch": "mlp", "hidden": 8}),
            "hidden is for arch rnn",
        ),
        (json!({"arch": "mlp", "layers": [0]}), "layers must be"),
        (
            json!({"arch": "mlp", "layers": [8, 8, 8, 8, 8]}),
            "layers must be",
        ),
        (json!({"arch": "mlp", "layers": [8.5]}), "layers must be"),
    ] {
        let e = s
            .execute("rnn.train", args.clone())
            .unwrap_err()
            .to_string();
        assert!(e.contains(needle), "{args}: {e}");
    }
    let out = run(
        &mut s,
        "rnn.train",
        json!({"arch": "mlp", "layers": 16, "games": 20, "epochs": 3}),
    );
    assert!(out["training"]["loss"].as_f64().unwrap().is_finite());
}

#[test]
fn the_ode_player_is_perfect_attacking_and_beatable_defending() {
    // sim_evaluate's play scorer on the plain net, held to the exact
    // teacher (sim-pflow-xyz experiments/ode-minimax, finding 10): with the
    // initiative it never loses; defending, with no policy structure, it
    // falls into the forks a move-order-free flow cannot see.
    let mut s = example("tic-tac-toe");
    let e = run(
        &mut s,
        "rnn.evaluate",
        with(
            rules(),
            json!({"player": "ode", "opponent": "minimax", "games": 40}),
        ),
    );
    let (x, o) = (&e["results"][0], &e["results"][1]);
    assert_eq!(e["model"], Value::Null);
    assert_eq!(
        (x["losses"].as_u64(), x["draws"].as_u64()),
        (Some(0), Some(40)),
        "{e}"
    );
    assert!(o["losses"].as_u64().unwrap() > 0, "{e}");
    // Deterministic on both sides: every game against itself is the same draw.
    let e = run(
        &mut s,
        "rnn.evaluate",
        with(
            rules(),
            json!({"player": "ode", "opponent": "ode", "games": 3}),
        ),
    );
    for side in e["results"].as_array().unwrap() {
        assert_eq!(side["draws"], 3, "{e}");
    }
    let bad = s
        .execute("rnn.evaluate", with(rules(), json!({"opponent": "model"})))
        .unwrap_err()
        .to_string();
    assert!(
        bad.contains("opponent must be random, minimax or ode"),
        "{bad}"
    );
}

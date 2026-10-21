//! Learning: data.calibrate (rates from an event log), opt.fit (ODE rate
//! fitting) and opt.distill (search distilled into the play scorer).

use std::collections::BTreeMap;
use std::time::Instant;

use pflow_native_core::learn::{Game, Search};
use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn near(a: f64, b: f64, rel: f64) -> bool {
    (a - b).abs() <= rel * b.abs().max(1e-12)
}

fn sir() -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": 0.001}),
    );
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 0.1}),
    );
    s
}

// ------------------------------------------------------------ opt.fit

#[test]
fn fit_recovers_sir_rates_from_a_noiseless_trajectory() {
    let mut s = sir();
    let ode = run(
        &mut s,
        "sim.ode",
        json!({"t_end": 40, "points": 81, "dtmax": 0.5}),
    );
    let data = ode["artifact"].as_str().unwrap().to_string();
    // Perturb the session rates, then fit them back.
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": 0.0018}),
    );
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 0.06}),
    );
    let t = Instant::now();
    let r = run(
        &mut s,
        "opt.fit",
        json!({"data": data, "dtmax": 0.5, "max_iters": 400}),
    );
    eprintln!(
        "opt.fit SIR: {:?}, {} iterations, {} evaluations, loss {} -> {}",
        t.elapsed(),
        r["iterations"],
        r["evaluations"],
        r["loss_before"],
        r["loss_after"]
    );
    assert!(
        near(f(&r["fitted_rates"]["infect"]), 0.001, 0.02),
        "infect {}",
        r["fitted_rates"]
    );
    assert!(
        near(f(&r["fitted_rates"]["recover"]), 0.1, 0.02),
        "recover {}",
        r["fitted_rates"]
    );
    assert_eq!(f(&r["initial_rates"]["infect"]), 0.0018);
    assert!(f(&r["loss_after"]) < 1e-2 * f(&r["loss_before"]));
    assert_eq!(r["parameters"], json!(["infect", "recover"]));
    assert_eq!(r["applied"], false);
    assert_eq!(s.rates["infect"], 0.0018, "no apply, no change");

    // The overlay: S, I, R twice, groups data and fit, on the data's times.
    let ts = s.artifact(r["timeseries"].as_str().unwrap()).unwrap();
    let ArtifactData::TimeSeries { x, series, .. } = &ts.data else {
        panic!("overlay is not a timeseries")
    };
    assert_eq!(x.len(), ode["data"]["x"].as_array().unwrap().len());
    assert_eq!(series.len(), 6);
    assert_eq!(
        series
            .iter()
            .filter(|s| s.group.as_deref() == Some("fit"))
            .count(),
        3
    );
    assert!(s.artifact(r["report"].as_str().unwrap()).is_some());
}

#[test]
fn fit_one_rate_and_apply() {
    let mut s = sir();
    let ode = run(
        &mut s,
        "sim.ode",
        json!({"t_end": 30, "points": 31, "dtmax": 0.5, "places": ["I"]}),
    );
    let data = ode["artifact"].as_str().unwrap().to_string();
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 0.2}),
    );
    let r = run(
        &mut s,
        "opt.fit",
        json!({"data": data, "parameters": ["recover"], "dtmax": 0.5, "apply": true}),
    );
    let fitted = f(&r["fitted_rates"]["recover"]);
    assert!(near(fitted, 0.1, 0.02), "recover {fitted}");
    assert!(r["fitted_rates"].get("infect").is_none());
    assert_eq!(r["applied"], true);
    assert_eq!(r["previous_rates"]["recover"], 0.2);
    assert_eq!(s.rates["recover"], fitted, "apply sets the session rate");
    assert_eq!(s.rates["infect"], 0.001, "an unfitted rate is untouched");
    assert_eq!(r["restore"][0]["command"], "sim.set_rate");
    // The restore steps put the old rate back.
    let steps = r["restore"].clone();
    for st in steps.as_array().unwrap() {
        run(&mut s, st["command"].as_str().unwrap(), st["args"].clone());
    }
    assert_eq!(s.rates["recover"], 0.2);
}

#[test]
fn fit_maps_series_by_label_or_mapping_and_refuses_strangers() {
    let mut s = sir();
    // Put data by hand, named by place label and by an unrelated name.
    let ode = run(
        &mut s,
        "sim.ode",
        json!({"t_end": 20, "points": 21, "dtmax": 0.5}),
    );
    let x = ode["data"]["x"].clone();
    let i_vals = ode["data"]["series"][1]["values"].clone();
    let put = |name: &str| {
        json!({"kind": "timeseries", "x_label": "t", "x": x.clone(),
               "series": [{"name": name, "values": i_vals.clone()}]})
    };
    let by_label = run(
        &mut s,
        "artifact.put",
        json!({"title": "obs1", "data": put("infected")}),
    )["artifact"]
        .as_str()
        .unwrap()
        .to_string();
    let r = run(
        &mut s,
        "opt.fit",
        json!({"data": by_label, "parameters": ["recover"], "max_iters": 5, "dtmax": 0.5}),
    );
    assert_eq!(r["observed"][0]["place"], "I");

    let other = run(
        &mut s,
        "artifact.put",
        json!({"title": "obs2", "data": put("cases")}),
    )["artifact"]
        .as_str()
        .unwrap()
        .to_string();
    let e = s
        .execute("opt.fit", json!({"data": other}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("names a place"), "{e}");
    let r = run(
        &mut s,
        "opt.fit",
        json!({"data": other, "places": {"cases": "I"}, "parameters": ["recover"], "max_iters": 5, "dtmax": 0.5}),
    );
    assert_eq!(r["observed"][0]["series"], "cases");

    for bad in [
        json!({"data": by_label, "method": "simplex"}),
        json!({"data": by_label, "parameters": ["nope"]}),
        json!({"data": by_label, "max_iters": 0}),
        json!({"data": by_label, "space": "exp"}),
    ] {
        assert!(s.execute("opt.fit", bad.clone()).is_err(), "{bad}");
    }
}

// ------------------------------------------------------------ data.calibrate

/// arrive (source) → queue → serve ("Serve Customer") → served → pack →
/// packed; `idle` never appears in the log.
fn shop() -> Session {
    let mut s = Session::default();
    let doc = json!({
        "@context": "https://pflow.xyz/schema", "@type": "PetriNet", "@version": "1.1",
        "token": ["https://pflow.xyz/tokens/black"],
        "places": {
            "queue": {"@type": "Place", "offset": 0, "initial": [0], "capacity": [0], "x": 100, "y": 100},
            "served": {"@type": "Place", "offset": 1, "initial": [0], "capacity": [0], "x": 300, "y": 100},
            "packed": {"@type": "Place", "offset": 2, "initial": [0], "capacity": [0], "x": 500, "y": 100}
        },
        "transitions": {
            "arrive": {"@type": "Transition", "x": 0, "y": 100},
            "serve": {"@type": "Transition", "label": "Serve Customer", "x": 200, "y": 100},
            "pack": {"@type": "Transition", "x": 400, "y": 100},
            "idle": {"@type": "Transition", "x": 600, "y": 100}
        },
        "arcs": [
            {"@type": "Arrow", "source": "arrive", "target": "queue", "weight": [1]},
            {"@type": "Arrow", "source": "queue", "target": "serve", "weight": [1]},
            {"@type": "Arrow", "source": "serve", "target": "served", "weight": [1]},
            {"@type": "Arrow", "source": "served", "target": "pack", "weight": [1]},
            {"@type": "Arrow", "source": "pack", "target": "packed", "weight": [1]},
            {"@type": "Arrow", "source": "packed", "target": "idle", "weight": [1]}
        ]
    });
    run(&mut s, "file.load_json", json!({"json": doc}));
    s
}

fn stamp(secs: i64) -> String {
    format!(
        "2026-10-01T{:02}:{:02}:{:02}Z",
        8 + secs / 3600,
        (secs / 60) % 60,
        secs % 60
    )
}

/// Three cases 10 minutes apart; serve gaps 120/240/180 s, pack gaps
/// 60/90/30 s; one unmapped `audit`.
fn shop_log(s: &mut Session) -> String {
    let mut csv = String::from("case_id,activity,timestamp\n");
    for (c, start, serve, pack) in [
        ("c1", 0, 120, 60),
        ("c2", 600, 240, 90),
        ("c3", 1200, 180, 30),
    ] {
        csv += &format!("{c},arrive,{}\n", stamp(start));
        csv += &format!("{c},serve customer,{}\n", stamp(start + serve));
        csv += &format!("{c},PACK,{}\n", stamp(start + serve + pack));
        if c == "c1" {
            csv += &format!("{c},audit,{}\n", stamp(start + serve + pack + 5));
        }
    }
    run(s, "data.load_log", json!({"text": csv}))["artifact"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn calibrate_learns_one_over_mean_gap_and_maps_by_id_and_label() {
    let mut s = shop();
    let log = shop_log(&mut s);
    let r = run(&mut s, "data.calibrate", json!({"log": log, "unit": "min"}));
    let rates = &r["learned_rates"];
    // Source: inter-arrival 10 min. Services: 1 / mean gap.
    assert!(near(f(&rates["arrive"]), 0.1, 1e-12), "{rates}");
    assert!(near(f(&rates["serve"]), 1.0 / 3.0, 1e-12), "{rates}");
    assert!(near(f(&rates["pack"]), 1.0, 1e-12), "{rates}");
    assert!(rates.get("idle").is_none());
    assert_eq!(r["mapping"]["arrive"], "arrive");
    assert_eq!(
        r["mapping"]["serve customer"], "serve",
        "by label, ignoring case"
    );
    assert_eq!(r["mapping"]["PACK"], "pack", "by id, ignoring case");
    assert_eq!(r["unmapped"], json!(["audit"]));
    assert_eq!(r["unobserved"], json!(["idle"]));
    let ev = &r["rate_evidence"]["serve"];
    assert_eq!(ev["n"], 3);
    assert!(near(f(&ev["mean_gap"]), 3.0, 1e-12));
    assert!(near(f(&ev["gap_cv"]), 1.0 / 3.0, 1e-12));
    assert_eq!(r["conformance"]["total_traces"], 3);
    assert_eq!(r["applied"], false);
    assert!(s.rates.is_empty(), "no apply, no change");

    // Per hour by default (as sim).
    let h = run(
        &mut s,
        "data.calibrate",
        json!({"log": log, "conformance": false}),
    );
    assert!(near(f(&h["learned_rates"]["pack"]), 60.0, 1e-12));
    assert!(h["conformance"].is_null());

    // The evidence table: one row per transition.
    let table = s.artifact(r["table"].as_str().unwrap()).unwrap();
    let ArtifactData::Table { columns, rows } = &table.data else {
        panic!("not a table")
    };
    assert_eq!(columns[0], "transition");
    assert_eq!(rows.len(), 4);
    assert_eq!(rows[3][2], "unobserved");
}

#[test]
fn calibrate_apply_sets_session_rates_and_reports_previous() {
    let mut s = shop();
    run(&mut s, "sim.set_rate", json!({"id": "serve", "rate": 5.0}));
    let log = shop_log(&mut s);
    let r = run(
        &mut s,
        "data.calibrate",
        json!({"log": log, "unit": "min", "apply": true}),
    );
    assert_eq!(r["applied"], true);
    assert_eq!(r["previous_rates"]["serve"], 5.0);
    assert_eq!(r["previous_rates"]["pack"], 1.0, "absent = 1");
    assert!(near(s.rates["serve"], 1.0 / 3.0, 1e-12));
    assert!(near(s.rates["arrive"], 0.1, 1e-12));
    assert!(!s.rates.contains_key("idle"));
}

#[test]
fn calibrate_splits_races_and_keeps_pickups() {
    // queue feeds both serve and reject: pooled gaps, split by share.
    let mut s = shop();
    run(&mut s, "net.add_transition", json!({"x": 200, "y": 200}));
    let t = s.doc.transitions.last().unwrap().id.clone();
    run(&mut s, "net.rename", json!({"id": t, "new_id": "reject"}));
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "queue", "target": "reject"}),
    );
    run(&mut s, "sim.set_rate", json!({"id": "pack", "rate": 720}));
    let mut csv = String::from("case_id,activity,timestamp\n");
    for (c, start, act, gap) in [
        ("c1", 0, "serve", 120),
        ("c2", 600, "serve", 240),
        ("c3", 1200, "reject", 60),
        ("c4", 1800, "serve", 180),
    ] {
        csv += &format!(
            "{c},arrive,{}\n{c},{act},{}\n",
            stamp(start),
            stamp(start + gap)
        );
        csv += &format!("{c},pack,{}\n", stamp(start + gap + 10));
    }
    let log = run(&mut s, "data.load_log", json!({"text": csv}))["artifact"]
        .as_str()
        .unwrap()
        .to_string();
    let r = run(&mut s, "data.calibrate", json!({"log": log, "unit": "min"}));
    // Pooled: 4 gaps over 600 s = 0.4/min; serve 3/4 of it, reject 1/4.
    assert!(
        near(f(&r["learned_rates"]["serve"]), 0.3, 1e-12),
        "{}",
        r["learned_rates"]
    );
    assert!(near(f(&r["learned_rates"]["reject"]), 0.1, 1e-12));
    assert!(r["learned_rates"].get("pack").is_none(), "pickup kept");
    assert!(r["caveats"].to_string().contains("instant-pickup"));
}

#[test]
fn calibrate_refuses_a_foreign_vocabulary() {
    let mut s = shop();
    let t = json!({"kind":"table","columns":["case","activity","time"],
                   "rows":[["1","x",0],["1","y",1000]]});
    let id = run(&mut s, "artifact.put", json!({"title":"log","data":t}))["artifact"].clone();
    let e = s
        .execute("data.calibrate", json!({"log": id}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("different vocabulary"), "{e}");
    assert!(s
        .execute("data.calibrate", json!({"log": id, "unit": "fortnight"}))
        .is_err());
}

// ------------------------------------------------------------ opt.distill

fn ttt() -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "tic-tac-toe"}));
    s
}

#[test]
fn search_finds_the_winning_move() {
    let mut s = ttt();
    for t in ["x_play_00", "o_play_10", "x_play_01", "o_play_11"] {
        run(&mut s, "sim.fire", json!({"id": t}));
    }
    let game = Game::new(&s.doc, &s.marking, None, None, None).unwrap();
    assert_eq!(
        game.players
            .iter()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["o", "x"]
    );
    assert!(game.players.iter().all(|p| p.moves.len() == 9));
    let x = game.players.iter().position(|p| p.name == "x").unwrap();
    assert!(game.players[x].maximizes);
    let mk = game.house_fire(game.net.model_marking(&s.marking));
    assert_eq!(game.mover(&mk), Some(x));
    let moves = game.moves(x, &mk);
    assert_eq!(moves.len(), 5);
    let mut search = Search::new(&game);
    let (vals, exact) = search.label(&mk, x, &moves).unwrap();
    assert!(exact);
    let best: Vec<&String> = moves
        .iter()
        .zip(&vals)
        .filter(|(_, v)| **v == 1.0)
        .map(|(t, _)| t)
        .collect();
    assert_eq!(best, ["x_play_02"], "{moves:?} {vals:?}");
    // Not blocking o's row loses.
    assert!(vals.iter().any(|v| *v == -1.0));
}

#[test]
fn distill_tic_tac_toe_reports_agreement() {
    let mut s = ttt();
    let t = Instant::now();
    let r = run(
        &mut s,
        "opt.distill",
        json!({"games": 6, "positions": 6, "iters": 3, "realizations": 4}),
    );
    eprintln!(
        "opt.distill ttt: {:?}, agreement {} -> {}, multipliers {}",
        t.elapsed(),
        r["agreement_before"],
        r["agreement_after"],
        r["multipliers"]
    );
    let positions = r["positions"].as_u64().unwrap();
    assert!((1..=6).contains(&positions));
    assert!(r["agreement_before"].as_u64().unwrap() <= positions);
    assert!(r["agreement_after"].as_u64().unwrap() <= positions);
    assert!(r["labeled"].as_u64().unwrap() <= positions);
    assert_eq!(r["engine"], "ssa");
    assert_eq!(r["objective"], "win_x - win_o");
    let groups: Vec<&String> = r["multipliers"].as_object().unwrap().keys().collect();
    assert_eq!(groups, ["house", "moves"]);
    assert!(r["multipliers"]
        .as_object()
        .unwrap()
        .values()
        .all(|m| f(m) > 0.0));
    let table = s.artifact(r["table"].as_str().unwrap()).unwrap();
    let ArtifactData::Table { rows, .. } = &table.data else {
        panic!("not a table")
    };
    assert_eq!(rows.len() as u64, positions);
    assert!(s.rates.is_empty(), "no apply, no change");

    // Deterministic for a seed.
    let again = run(
        &mut s,
        "opt.distill",
        json!({"games": 6, "positions": 6, "iters": 3, "realizations": 4}),
    );
    assert_eq!(again["multipliers"], r["multipliers"]);
    assert_eq!(again["agreement_after"], r["agreement_after"]);
}

#[test]
fn distill_with_explicit_groups_applies_and_refuses_bad_requests() {
    let mut s = ttt();
    let r = run(
        &mut s,
        "opt.distill",
        json!({"groups": {"detectors": ["x_win_row0", "o_win_row0"]},
               "games": 3, "positions": 3, "iters": 2, "realizations": 2, "apply": true}),
    );
    let m = f(&r["multipliers"]["detectors"]);
    assert_eq!(r["applied"], true);
    assert_eq!(s.rates["x_win_row0"], m);
    assert_eq!(s.rates["o_win_row0"], m);
    assert_eq!(s.rates.len(), 2);

    let bad = |s: &mut Session, args: Value| {
        s.execute("opt.distill", args.clone())
            .map(|_| ())
            .expect_err(&args.to_string())
            .to_string()
    };
    assert!(bad(&mut s, json!({"groups": {"a": ["nope"]}})).contains("not a transition"));
    assert!(bad(&mut s, json!({"groups": {"a": []}})).contains("no transitions"));
    assert!(bad(
        &mut s,
        json!({"groups": {"a": ["x_play_00"], "b": ["x_play_00"]}})
    )
    .contains("groups"));
    assert!(bad(&mut s, json!({"positions": 101})).contains("positions"));
    assert!(bad(&mut s, json!({"realizations": 200, "iters": 100})).contains("work"));
    assert!(bad(&mut s, json!({"engine": "ode"})).contains("ssa"));
    let many: BTreeMap<String, Vec<String>> = (0..17)
        .map(|i| (format!("g{i}"), vec![s.doc.transitions[i].id.clone()]))
        .collect();
    assert!(bad(&mut s, json!({"groups": many})).contains("cap"));

    let mut sir = sir();
    assert!(bad(&mut sir, json!({})).contains("turn-based"));
}

#[test]
fn applied_rates_are_one_undo_step() {
    let mut s = pflow_native_core::Session::default();
    s.execute("file.open_example", serde_json::json!({"name": "sir"}))
        .unwrap();
    s.execute(
        "sim.set_rate",
        serde_json::json!({"id": "recover", "rate": 0.2}),
    )
    .unwrap();
    let ode = s
        .execute("sim.ode", serde_json::json!({"t_end": 20, "points": 40}))
        .unwrap();
    let id = ode["artifact"].as_str().unwrap().to_string();
    s.execute(
        "sim.set_rate",
        serde_json::json!({"id": "recover", "rate": 0.05}),
    )
    .unwrap();
    s.execute(
        "opt.fit",
        serde_json::json!({"data": id, "parameters": ["recover"], "apply": true}),
    )
    .unwrap();
    let fitted = s.rates["recover"];
    assert!((fitted - 0.2).abs() < 0.01, "{fitted}");
    s.execute("edit.undo", serde_json::Value::Null).unwrap();
    assert_eq!(s.rates["recover"], 0.05, "undo restores the pre-apply rate");
    s.execute("edit.redo", serde_json::Value::Null).unwrap();
    assert_eq!(s.rates["recover"], fitted);
}

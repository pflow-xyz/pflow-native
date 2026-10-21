//! Phase 3, stochastic half: sim.ssa / sde / scenario / compare /
//! crosscheck / evaluate.

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

fn data(s: &Session, id: &Value) -> ArtifactData {
    s.artifact(id.as_str().expect("artifact id"))
        .expect("artifact stored")
        .data
        .clone()
}

/// (x, series) of a time-series artifact.
fn ts(s: &Session, id: &Value) -> (Vec<f64>, Vec<pflow_native_core::artifact::Series>) {
    match data(s, id) {
        ArtifactData::TimeSeries { x, series, .. } => (x, series),
        other => panic!("expected a time series, got {}", other.kind()),
    }
}

fn last(series: &[pflow_native_core::artifact::Series], name: &str, group: Option<&str>) -> f64 {
    let s = series
        .iter()
        .find(|s| s.name == name && s.group.as_deref() == group)
        .unwrap_or_else(|| panic!("no series {name} / {group:?}"));
    *s.values.last().unwrap()
}

#[test]
fn ssa_is_deterministic_and_bands_iff_runs() {
    let mut s = example("sir");
    // Re-running replaces the same-titled artifact, so keep the data.
    let a = run(
        &mut s,
        "sim.ssa",
        json!({"t_end": 5, "seed": 7, "runs": 8, "points": 21}),
    );
    let first = data(&s, &a["artifact"]);
    let b = run(
        &mut s,
        "sim.ssa",
        json!({"t_end": 5, "seed": 7, "runs": 8, "points": 21}),
    );
    assert_eq!(first, data(&s, &b["artifact"]));
    let (x, series) = ts(&s, &b["artifact"]);
    assert_eq!(x.len(), 21);
    assert_eq!(series.len(), 3);
    assert!(series
        .iter()
        .all(|s| s.band.as_ref().map(Vec::len) == Some(21)));
    // Every realization conserves S + I + R = 1000, so the means do too.
    for i in 0..x.len() {
        let total: f64 = series.iter().map(|s| s.values[i]).sum();
        assert!((total - 1000.0).abs() < 1e-9, "{total}");
    }
    let c = run(
        &mut s,
        "sim.ssa",
        json!({"t_end": 5, "seed": 8, "runs": 8, "points": 21}),
    );
    assert_ne!(first, data(&s, &c["artifact"]));

    let one = run(
        &mut s,
        "sim.ssa",
        json!({"t_end": 5, "seed": 7, "points": 21}),
    );
    let (_, series) = ts(&s, &one["artifact"]);
    assert!(series.iter().all(|s| s.band.is_none()));
    assert!(one["final_sd"].is_null());

    let e = err(&mut s, "sim.ssa", json!({"runs": 1000}));
    assert!(e.contains("outside 1..200"), "{e}");
}

#[test]
fn ssa_mean_tracks_ode_on_sir() {
    let mut s = example("sir");
    let ode = run(&mut s, "sim.ode", json!({"t_end": 3}));
    let ssa = run(
        &mut s,
        "sim.ssa",
        json!({"t_end": 3, "runs": 16, "seed": 3}),
    );
    for p in ["S", "I", "R"] {
        let o = ode["final"][p].as_f64().unwrap();
        let m = ssa["final"][p].as_f64().unwrap();
        assert!((o - m).abs() < 0.05 * 1000.0, "{p}: ode {o} vs ssa {m}");
    }
}

#[test]
fn sde_runs_and_is_deterministic() {
    let mut s = example("sir");
    let a = run(
        &mut s,
        "sim.sde",
        json!({"t_end": 3, "runs": 4, "seed": 2, "points": 31}),
    );
    let first = data(&s, &a["artifact"]);
    let b = run(
        &mut s,
        "sim.sde",
        json!({"t_end": 3, "runs": 4, "seed": 2, "points": 31}),
    );
    assert_eq!(first, data(&s, &b["artifact"]));
    let (x, series) = ts(&s, &b["artifact"]);
    assert_eq!(x.len(), 31);
    assert!(series.iter().all(|s| s.band.is_some()));
    let r = a["final"]["R"].as_f64().unwrap();
    assert!(r > 500.0, "most recover by t=3: {r}");
    // dt sets the grid; output is still thinned to points.
    let c = run(
        &mut s,
        "sim.sde",
        json!({"t_end": 3, "dt": 0.001, "points": 31}),
    );
    assert_eq!(ts(&s, &c["artifact"]).0.len(), 31);
    assert!((c["dt"].as_f64().unwrap() - 0.001).abs() < 1e-4);
}

#[test]
fn scenario_overrides_take_effect() {
    let mut s = example("sir");
    let base = run(
        &mut s,
        "sim.scenario",
        json!({"scenario": {"hours": 2, "realizations": 4, "seed": 5}}),
    );
    assert_eq!(base["artifacts"].as_array().unwrap().len(), 2);
    let sum = &base["summary"];
    assert_eq!(sum["engine"], "ssa");
    assert!(sum["finalStdDev"].is_object());
    let total: f64 = ["S", "I", "R"]
        .iter()
        .map(|p| sum["final"][p].as_f64().unwrap())
        .sum();
    assert!((total - 1000.0).abs() < 1e-9);

    // No infected and no infection: nothing moves.
    let quiet = run(
        &mut s,
        "sim.scenario",
        json!({"scenario": {"name": "quiet", "hours": 2, "realizations": 4, "seed": 5,
                            "marking": {"I": 0}, "rates": {"infect": 0}}}),
    );
    assert_eq!(quiet["summary"]["final"]["S"], json!(999.0));
    assert_eq!(quiet["summary"]["final"]["R"], json!(0.0));
    match data(&s, &quiet["table"]) {
        ArtifactData::Table { columns, rows } => {
            assert_eq!(columns[0], "place");
            assert_eq!(rows.len(), 3);
            assert_eq!(rows[0][0], json!("S"));
            assert_eq!(rows[0][1], json!(999.0));
        }
        other => panic!("{}", other.kind()),
    }

    // The ODE engine, and a string-encoded scenario.
    let ode = run(
        &mut s,
        "sim.scenario",
        json!({"scenario": "{\"engine\":\"ode\",\"hours\":2,\"samples\":11,\"summary\":true}"}),
    );
    assert!(ode["timeseries"].is_null());
    assert_eq!(ode["summary"]["engine"], "ode");
    assert!(ode["summary"].get("finalStdDev").is_none());

    // Refusals name the problem.
    for (sc, want) in [
        (json!({"bogus": 1}), "unknown scenario field"),
        (json!({"marking": {"Q": 1}}), "no token place"),
        (json!({"rates": {"nope": 1}}), "no transition"),
        (json!({"params": {"k": 2}}), "params are not supported"),
        (json!({"realizations": 500}), "outside 0..200"),
        (
            json!({"engine": "ode", "schedule": {"infect": [{"until": 1, "value": 2}]}}),
            "cannot vary a rate",
        ),
        (json!({"engine": "pde"}), "unknown engine"),
    ] {
        let e = err(&mut s, "sim.scenario", json!({ "scenario": sc }));
        assert!(e.contains(want), "{sc}: {e}");
    }

    // A schedule runs on the discrete engine.
    let sch = run(
        &mut s,
        "sim.scenario",
        json!({"scenario": {"hours": 2, "realizations": 2, "seed": 1, "samples": 5,
                            "schedule": {"recover": [{"until": 1, "value": 0}, {"until": 2, "value": 1}]}}}),
    );
    assert!(sch["summary"]["final"]["R"].as_f64().unwrap() > 0.0);
}

#[test]
fn compare_groups_by_scenario_on_one_seed() {
    let mut s = example("sir");
    let r = run(
        &mut s,
        "sim.compare",
        json!({"scenarios": [
            {"name": "today", "hours": 2, "realizations": 4},
            {"name": "slow", "hours": 2, "realizations": 4, "rates": {"infect": 0.0005}},
            {"hours": 2, "realizations": 4, "engine": "ode"}
        ], "seed": 9}),
    );
    assert_eq!(r["seed"], 9);
    let (_, series) = ts(&s, &r["timeseries"]);
    let mut groups: Vec<&str> = series.iter().filter_map(|s| s.group.as_deref()).collect();
    groups.dedup();
    assert_eq!(groups, ["today", "slow", "scenario 3"]);
    assert_eq!(series.len(), 9);
    assert!(last(&series, "S", Some("slow")) > last(&series, "S", Some("today")));
    match data(&s, &r["table"]) {
        ArtifactData::Table { columns, rows } => {
            assert_eq!(columns, ["scenario", "S", "I", "R"]);
            assert_eq!(rows.len(), 3);
        }
        other => panic!("{}", other.kind()),
    }
    let first = data(&s, &r["timeseries"]);
    let again = run(
        &mut s,
        "sim.compare",
        json!({"scenarios": [
            {"name": "today", "hours": 2, "realizations": 4},
            {"name": "slow", "hours": 2, "realizations": 4, "rates": {"infect": 0.0005}},
            {"hours": 2, "realizations": 4, "engine": "ode"}
        ], "seed": 9}),
    );
    // Seeded SSA groups repeat exactly; the ODE group only to ~1e-9, since
    // pflow-solver's derivative sums follow HashMap order (FEATURES §D 7).
    assert_close(&first, &data(&s, &again["timeseries"]));
    let e = err(
        &mut s,
        "sim.compare",
        json!({"scenarios": [{"hours": 2}, {"hours": 3}]}),
    );
    assert!(e.contains("one time grid"), "{e}");
}

#[test]
fn crosscheck_finds_sir_conservation_and_agreement() {
    let mut s = example("sir");
    let r = run(
        &mut s,
        "sim.crosscheck",
        json!({"t_end": 3, "runs": 16, "seed": 4}),
    );
    assert_eq!(r["laws"], json!(["S + I + R = 1000"]), "{r}");
    assert_eq!(r["verdict"], "agree", "{r}");
    let (_, series) = ts(&s, &r["timeseries"]);
    assert_eq!(
        series
            .iter()
            .filter(|s| s.group.as_deref() == Some("ssa"))
            .count(),
        3
    );
    assert_eq!(
        series
            .iter()
            .filter(|s| s.group.as_deref() == Some("ode"))
            .count(),
        3
    );
    match data(&s, &r["report"]) {
        ArtifactData::Report(rep) => {
            let inv = &rep["invariants"][0];
            assert_eq!(inv["holds"], true);
            assert_eq!(inv["ode_holds"], true);
            assert_eq!(rep["agreement"].as_array().unwrap().len(), 3);
            assert!(rep["agreement"][0]["max_abs_diff"].is_number());
        }
        other => panic!("{}", other.kind()),
    }
}

#[test]
fn crosscheck_on_producer_consumer_and_colored_nets() {
    // Two conservation laws in the producer-consumer buffer.
    let mut s = example("producer-consumer");
    let r = run(&mut s, "sim.crosscheck", json!({"t_end": 2, "runs": 4}));
    assert!(!r["laws"].as_array().unwrap().is_empty(), "{r}");
    assert_ne!(r["verdict"], "defect", "{r}");
    // Colors unfold: series are named place.color.
    let mut s = example("predator-prey");
    let r = run(&mut s, "sim.ssa", json!({"t_end": 1, "runs": 2}));
    let (_, series) = ts(&s, &r["artifact"]);
    let names: Vec<&str> = series.iter().map(|s| s.name.as_str()).collect();
    assert!(names.iter().all(|n| n.contains('.')), "{names:?}");
    let r = run(&mut s, "sim.crosscheck", json!({"t_end": 1, "runs": 2}));
    assert_ne!(r["verdict"], "defect", "{r}");
    let r = run(
        &mut s,
        "sim.scenario",
        json!({"scenario": {"hours": 1, "realizations": 2, "marking": {"Prey": 10}}}),
    );
    assert!(r["summary"]["final"].as_object().unwrap().len() >= 2);
}

#[test]
fn evaluate_ranks_tic_tac_toe_moves() {
    let mut s = example("tic-tac-toe");
    let r = run(
        &mut s,
        "sim.evaluate",
        json!({"player": "x", "rollouts": 20}),
    );
    assert_eq!(r["objective"], "win_x - win_o", "{r}");
    assert_eq!(r["engine"], "ssa");
    let moves = r["moves"].as_array().unwrap();
    assert_eq!(moves.len(), 9);
    assert!(moves
        .iter()
        .all(|m| m["transition"].as_str().unwrap().starts_with("x_play_")));
    let payouts: Vec<f64> = moves
        .iter()
        .map(|m| m["payout"].as_f64().unwrap())
        .collect();
    assert!(payouts.windows(2).all(|w| w[0] >= w[1]), "{payouts:?}");
    match data(&s, &r["artifact"]) {
        ArtifactData::Table { columns, rows } => {
            assert_eq!(columns, ["rank", "transition", "payout", "vs_baseline"]);
            assert_eq!(rows.len(), 9);
            assert_eq!(rows[0][1], moves[0]["transition"]);
        }
        other => panic!("{}", other.kind()),
    }
    let again = run(
        &mut s,
        "sim.evaluate",
        json!({"player": "x", "rollouts": 20}),
    );
    assert_eq!(r["moves"], again["moves"]);

    // Token-game playouts to the end of the game: the center is X's best
    // opening against random play.
    let t = run(
        &mut s,
        "sim.evaluate",
        json!({"player": "x", "depth": 20, "rollouts": 200, "seed": 1}),
    );
    assert_eq!(t["engine"], "token-game");
    assert_eq!(t["best"], "x_play_11", "{t}");

    // O has no move on X's turn.
    let e = err(&mut s, "sim.evaluate", json!({"player": "o"}));
    assert!(e.contains("no enabled move"), "{e}");
    let e = err(
        &mut s,
        "sim.evaluate",
        json!({"player": "x", "horizon": 1, "depth": 3}),
    );
    assert!(e.contains("not both"), "{e}");
}

#[test]
fn stochastic_commands_are_journaled() {
    let mut s = example("sir");
    run(&mut s, "sim.ssa", json!({"t_end": 1}));
    assert_eq!(s.journal.last().unwrap().command, "sim.ssa");
}

fn assert_close(a: &ArtifactData, b: &ArtifactData) {
    match (a, b) {
        (
            ArtifactData::TimeSeries {
                x: xa, series: sa, ..
            },
            ArtifactData::TimeSeries {
                x: xb, series: sb, ..
            },
        ) => {
            assert_eq!(xa, xb);
            assert_eq!(sa.len(), sb.len());
            for (p, q) in sa.iter().zip(sb) {
                assert_eq!((&p.name, &p.group), (&q.name, &q.group));
                let exact = p.group.as_deref() != Some("scenario 3");
                for (u, v) in p.values.iter().zip(&q.values) {
                    if exact {
                        assert_eq!(u, v, "{} {:?}", p.name, p.group);
                    } else {
                        assert!(
                            (u - v).abs() <= 1e-9 * u.abs().max(1.0),
                            "{} {u} vs {v}",
                            p.name
                        );
                    }
                }
            }
        }
        _ => assert_eq!(a, b),
    }
}

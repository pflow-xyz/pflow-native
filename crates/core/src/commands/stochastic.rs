//! Stochastic simulation commands: SSA, SDE, sim.pflow.xyz's scenario /
//! compare / crosscheck / evaluate. The engines live in
//! [`crate::stochastic`]; this file reads arguments and stores artifacts.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::artifact::{ArtifactData, Series};
use crate::registry::{always, arg, n, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::sim;
use crate::stochastic::{self as st, Ensemble, Net, Objective, Rollout, Scenario, ScenarioRun};
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "sim.ssa",
        label: "Run SSA (Gillespie)",
        help: "Gillespie stochastic simulation from the current marking with the session rates (pflow-solver's portable SSA, byte-exact across pflow implementations). runs > 1 stores the mean with a ± std-dev band on a fixed sample grid. Deterministic for a seed.",
        menu: "Simulate",
        shortcut: None,
        args: &[
            opt("t_end", Num, "end time (default 10, ≤ 10000)"),
            opt("seed", Int, "RNG seed (default 1); realization r uses seed + r"),
            opt("runs", Int, "realizations averaged (default 1, ≤ 200)"),
            opt("points", Int, "sample points from 0 to t_end (default 101, 2..5000)"),
        ],
        enabled: always,
        run: ssa,
        journal: true,
    },
    CommandSpec {
        id: "sim.sde",
        label: "Run SDE (Chemical Langevin)",
        help: "Chemical Langevin SDE (Euler-Maruyama over the mass-action propensities, pflow-solver) from the current marking. Same output shape as sim.ssa. Refuses gated nets (read/inhibitor arcs, capacities).",
        menu: "Simulate",
        shortcut: None,
        args: &[
            opt("t_end", Num, "end time (default 10, ≤ 10000)"),
            opt("seed", Int, "RNG seed (default 1)"),
            opt("dt", Num, "integration step (default: (t_end / (points-1)) / 20)"),
            opt("runs", Int, "realizations averaged (default 1, ≤ 200)"),
            opt("points", Int, "points returned (default 101, 2..5000)"),
        ],
        enabled: always,
        run: sde,
        journal: true,
    },
    CommandSpec {
        id: "sim.scenario",
        label: "Run Scenario",
        help: "A sim_scenario what-if from the current marking: {name, marking, rates, schedule, hours, samples, realizations, seed, summary, engine: ssa|ode}. Stores a mean±sd time series and a per-place finals table; returns finals, metrics, depletion and contention. Unknown fields are refused.",
        menu: "Simulate",
        shortcut: None,
        args: &[arg("scenario", Json, "scenario object (or a string holding one), e.g. {\"hours\":8,\"realizations\":16,\"marking\":{\"S\":500},\"rates\":{\"infect\":0.002}}")],
        enabled: always,
        run: scenario,
        journal: true,
    },
    CommandSpec {
        id: "sim.compare",
        label: "Compare Scenarios",
        help: "Run several scenarios on one shared seed (sim_compare): an overlay time series (group = scenario) and a scenario × place table of final means.",
        menu: "Simulate",
        shortcut: None,
        args: &[
            arg("scenarios", Json, "array (or a string holding one) of ≤ 20 scenarios, as sim.scenario takes; they must share hours and samples"),
            opt("seed", Int, "the shared seed (default: the first scenario's, else 20260809)"),
        ],
        enabled: always,
        run: compare,
        journal: true,
    },
    CommandSpec {
        id: "sim.crosscheck",
        label: "Cross-check SSA / ODE / Invariants",
        help: "sim_crosscheck: SSA ensemble mean against the mass-action ODE, and the net's P-invariants (Farkas) checked along both trajectories. Stores an overlay (group ssa/ode) and a report with per-place agreement and a verdict.",
        menu: "Simulate",
        shortcut: None,
        args: &[
            opt("t_end", Num, "horizon (default 8, ≤ 168)"),
            opt("runs", Int, "SSA realizations (default 24, ≤ 200)"),
            opt("seed", Int, "RNG seed (default 20260809)"),
            opt("points", Int, "sample points (default 101, 2..5000)"),
        ],
        enabled: always,
        run: crosscheck,
        journal: true,
    },
    CommandSpec {
        id: "sim.evaluate",
        label: "Evaluate Moves",
        help: "sim_evaluate (play method): fire each enabled move of a player and score the position it leaves by seeded rollouts — SSA over `horizon` (sim's engine), or random token-game playouts of at most `depth` firings. Stores a ranked table.",
        menu: "",
        shortcut: None,
        args: &[
            opt("player", Str, "transition role, or transition-id prefix (e.g. \"x\"); default: every enabled transition"),
            opt("objective", Json, "linear objective to maximize, e.g. \"win_x - win_o\" or {\"win_x\": 1, \"win_o\": -1}; default win_<player> minus the other win_* places"),
            opt("rollouts", Int, "rollouts per candidate (default 40, ≤ 200)"),
            opt("seed", Int, "seed shared by every candidate (default 11)"),
            opt("horizon", Num, "SSA rollout horizon in model time (default 3, ≤ 10000)"),
            opt("depth", Int, "use token-game playouts of at most this many firings instead of SSA (≤ 10000)"),
        ],
        enabled: always,
        run: evaluate,
        journal: true,
    },
];

fn source(command: &str, a: &Value) -> Step {
    Step {
        command: command.into(),
        args: a.clone(),
    }
}

fn uint(a: &Value, k: &str, default: usize) -> Result<usize> {
    match n(a, k) {
        None => Ok(default),
        Some(x) if x >= 0.0 => Ok(x as usize),
        Some(x) => Err(Error::BadArgs(format!("{k} must be ≥ 0, got {x}"))),
    }
}

fn seed(a: &Value, default: u64) -> Result<u64> {
    match a.get("seed").filter(|v| !v.is_null()) {
        Some(v) => st::parse_seed(v, "seed"),
        None => Ok(default),
    }
}

fn series(e: &Ensemble, group: Option<&str>, idx: Option<&[usize]>) -> Vec<Series> {
    let pick = |v: &Vec<f64>| -> Vec<f64> {
        match idx {
            Some(ix) => ix.iter().map(|&i| v[i]).collect(),
            None => v.clone(),
        }
    };
    e.names
        .iter()
        .enumerate()
        .map(|(i, name)| Series {
            name: name.clone(),
            values: pick(&e.mean[i]),
            band: e.sd.as_ref().map(|sd| pick(&sd[i])),
            group: group.map(str::to_string),
        })
        .collect()
}

fn final_sd(e: &Ensemble) -> Option<BTreeMap<String, f64>> {
    e.sd.as_ref().map(|sd| {
        e.names
            .iter()
            .zip(sd)
            .map(|(n, v)| (n.clone(), v.last().copied().unwrap_or(0.0)))
            .collect()
    })
}

fn ssa(sess: &mut Session, a: &Value) -> Result<Value> {
    let t_end = n(a, "t_end").unwrap_or(10.0);
    st::check_horizon("t_end", t_end, st::MAX_HORIZON)?;
    let runs = uint(a, "runs", 1)?;
    st::check_runs("runs", runs)?;
    let points = uint(a, "points", 101)?;
    st::check_samples("points", points)?;
    let seed = seed(a, 1)?;
    let net = Net::new(&sess.doc)?;
    let e = st::run_ssa(&net, &sess.marking, &sess.rates, t_end, points, runs, seed)?;
    let out = json!({
        "engine": "ssa", "runs": runs, "seed": seed, "t_end": t_end,
        "final": e.finals(), "final_sd": final_sd(&e),
    });
    let data = ArtifactData::TimeSeries {
        x_label: "t".into(),
        x: e.t.clone(),
        series: series(&e, None, None),
    };
    let id = sess.add_artifact(
        format!("SSA ({runs} runs, seed {seed})"),
        source("sim.ssa", a),
        data,
    );
    let mut out = out;
    out["artifact"] = json!(id);
    Ok(out)
}

fn sde(sess: &mut Session, a: &Value) -> Result<Value> {
    let t_end = n(a, "t_end").unwrap_or(10.0);
    st::check_horizon("t_end", t_end, st::MAX_HORIZON)?;
    let runs = uint(a, "runs", 1)?;
    st::check_runs("runs", runs)?;
    let points = uint(a, "points", 101)?;
    st::check_samples("points", points)?;
    let seed = seed(a, 1)?;
    // The SDE takes 20 Euler-Maruyama steps per sample interval, so a step
    // of `dt` means a grid of t_end / (20 dt) intervals.
    let samples = match n(a, "dt") {
        Some(dt) => {
            if !(dt > 0.0 && dt.is_finite()) {
                return Err(Error::BadArgs("dt must be a positive number".into()));
            }
            let k = (t_end / (20.0 * dt)).ceil();
            if k + 1.0 > st::MAX_SDE_STEPS as f64 {
                return Err(Error::BadArgs(format!(
                    "dt {dt} needs {} sample intervals over t_end {t_end}, over the {} cap; use a larger dt",
                    k as u64,
                    st::MAX_SDE_STEPS
                )));
            }
            (k as usize).max(1) + 1
        }
        None => points,
    };
    let net = Net::new(&sess.doc)?;
    let e = st::run_sde(&net, &sess.marking, &sess.rates, t_end, samples, runs, seed)?;
    let idx = sim::downsample(e.t.len(), points);
    let dt = t_end / (samples - 1) as f64 / 20.0;
    let out = json!({
        "engine": "sde", "runs": runs, "seed": seed, "t_end": t_end, "dt": dt,
        "final": e.finals(), "final_sd": final_sd(&e),
    });
    let data = ArtifactData::TimeSeries {
        x_label: "t".into(),
        x: idx.iter().map(|&i| e.t[i]).collect(),
        series: series(&e, None, Some(&idx)),
    };
    let id = sess.add_artifact(
        format!("SDE ({runs} runs, seed {seed})"),
        source("sim.sde", a),
        data,
    );
    let mut out = out;
    out["artifact"] = json!(id);
    Ok(out)
}

/// The per-place finals table of one scenario.
fn finals_table(run: &ScenarioRun) -> ArtifactData {
    let metric = |k: &str, p: &str| -> Value {
        run.metrics
            .get(k)
            .and_then(|m| m.get(p))
            .cloned()
            .unwrap_or(Value::Null)
    };
    let rows = run
        .finals
        .iter()
        .map(|(p, f)| {
            let depleted = run
                .depleted
                .iter()
                .find(|d| d["place"] == json!(p))
                .map(|d| d["at"].clone())
                .unwrap_or(Value::Null);
            vec![
                json!(p),
                json!(f.mean),
                f.sd.map_or(Value::Null, |x| json!(x)),
                json!(f.min),
                json!(f.max),
                metric("mean", p),
                metric("p95", p),
                depleted,
            ]
        })
        .collect();
    ArtifactData::Table {
        columns: [
            "place",
            "final_mean",
            "final_sd",
            "final_min",
            "final_max",
            "time_mean",
            "p95",
            "depleted_at",
        ]
        .map(String::from)
        .to_vec(),
        rows,
    }
}

fn scenario(sess: &mut Session, a: &Value) -> Result<Value> {
    let net = Net::new(&sess.doc)?;
    let sc = st::parse_scenario(&a["scenario"], &net)?;
    let run = st::run_scenario(&sess.doc, &net, &sess.marking, &sess.rates, &sc)?;
    let name = sc.name.clone().unwrap_or_else(|| "scenario".into());
    let summary = run.summary(&name, &sc);
    let mut artifacts = vec![];
    let mut ts = Value::Null;
    if !sc.summary {
        let data = ArtifactData::TimeSeries {
            x_label: "t".into(),
            x: run.series.t.clone(),
            series: series(&run.series, None, None),
        };
        let id = sess.add_artifact(
            format!(
                "Scenario {name} ({}, {} runs, seed {})",
                run.engine.name(),
                sc.realizations,
                sc.seed
            ),
            source("sim.scenario", a),
            data,
        );
        ts = json!(id);
        artifacts.push(id);
    }
    let table = sess.add_artifact(
        format!("Scenario {name}: finals"),
        source("sim.scenario", a),
        finals_table(&run),
    );
    artifacts.push(table.clone());
    Ok(json!({
        "artifacts": artifacts,
        "timeseries": ts,
        "table": table,
        "summary": summary,
    }))
}

fn compare(sess: &mut Session, a: &Value) -> Result<Value> {
    let net = Net::new(&sess.doc)?;
    let list = st::json_arg(&a["scenarios"], "scenarios")?;
    let list = list
        .as_array()
        .ok_or_else(|| Error::BadArgs("scenarios must be an array".into()))?;
    if list.is_empty() {
        return Err(Error::BadArgs("no scenarios to compare".into()));
    }
    if list.len() > st::MAX_COMPARE {
        return Err(Error::BadArgs(format!(
            "{} scenarios is over the {} cap",
            list.len(),
            st::MAX_COMPARE
        )));
    }
    let mut scs: Vec<(String, Scenario)> = vec![];
    for (i, v) in list.iter().enumerate() {
        let sc = st::parse_scenario(v, &net)
            .map_err(|e| Error::BadArgs(format!("scenario {}: {e}", i + 1)))?;
        let name = sc
            .name
            .clone()
            .unwrap_or_else(|| format!("scenario {}", i + 1));
        if scs.iter().any(|(n, _)| *n == name) {
            return Err(Error::BadArgs(format!(
                "two scenarios are named {name:?}; names label the overlay"
            )));
        }
        scs.push((name, sc));
    }
    let shared = match a.get("seed").filter(|v| !v.is_null()) {
        Some(v) => st::parse_seed(v, "seed")?,
        None => scs[0].1.seed,
    };
    let (h0, s0) = (scs[0].1.hours, scs[0].1.samples);
    for (name, sc) in &mut scs {
        if sc.hours != h0 || sc.samples != s0 {
            return Err(Error::BadArgs(format!(
                "scenario {name:?} runs {} hours at {} samples, the first {h0} at {s0}: an overlay needs one time grid",
                sc.hours, sc.samples
            )));
        }
        sc.seed = shared;
    }
    let mut runs = vec![];
    for (name, sc) in &scs {
        let r = st::run_scenario(&sess.doc, &net, &sess.marking, &sess.rates, sc)
            .map_err(|e| Error::Invalid(format!("{name}: {e}")))?;
        runs.push(r);
    }
    let mut overlay = vec![];
    let mut x = vec![];
    for ((name, sc), r) in scs.iter().zip(&runs) {
        if sc.summary {
            continue;
        }
        x = r.series.t.clone();
        overlay.extend(series(&r.series, Some(name), None));
    }
    let mut artifacts = vec![];
    let mut ts = Value::Null;
    if !overlay.is_empty() {
        let id = sess.add_artifact(
            format!("Compare {} scenarios (seed {shared})", scs.len()),
            source("sim.compare", a),
            ArtifactData::TimeSeries {
                x_label: "t".into(),
                x,
                series: overlay,
            },
        );
        ts = json!(id);
        artifacts.push(id);
    }
    let mut columns = vec!["scenario".to_string()];
    columns.extend(net.display.iter().cloned());
    let rows = scs
        .iter()
        .zip(&runs)
        .map(|((name, _), r)| {
            let mut row = vec![json!(name)];
            row.extend(r.finals.iter().map(|(_, f)| json!(f.mean)));
            row
        })
        .collect();
    let table = sess.add_artifact(
        format!("Compare {} scenarios: final means", scs.len()),
        source("sim.compare", a),
        ArtifactData::Table { columns, rows },
    );
    artifacts.push(table.clone());
    let scenarios: Vec<Value> = scs
        .iter()
        .zip(&runs)
        .map(|((name, sc), r)| r.summary(name, sc))
        .collect();
    Ok(json!({
        "artifacts": artifacts,
        "timeseries": ts,
        "table": table,
        "seed": shared,
        "scenarios": scenarios,
    }))
}

fn crosscheck(sess: &mut Session, a: &Value) -> Result<Value> {
    let t_end = n(a, "t_end").unwrap_or(st::SCENARIO_HOURS);
    st::check_horizon("t_end", t_end, st::MAX_CROSSCHECK_HORIZON)?;
    let runs = uint(a, "runs", 24)?;
    st::check_runs("runs", runs)?;
    let points = uint(a, "points", 101)?;
    st::check_samples("points", points)?;
    let seed = seed(a, st::SCENARIO_SEED)?;
    let net = Net::new(&sess.doc)?;
    let start = net.model_marking(&sess.marking);
    let e = st::run_ssa(&net, &sess.marking, &sess.rates, t_end, points, runs, seed)?;
    let gating = net.gating();
    let ode = if gating.is_empty() {
        Some(st::ode_on_grid(
            &sess.doc,
            &sess.marking,
            &sess.rates,
            &e.t,
        )?)
    } else {
        None
    };

    // 1. SSA mean vs ODE, per place. sim's final-state rule: a place whose
    // scale is ≥ 1 token is compared relatively, a sub-unit one absolutely,
    // and the headline is the counted place unless only sub-unit ones
    // disagree.
    let mut places = vec![];
    let mut ode_series: Vec<Vec<f64>> = vec![];
    let (mut rel, mut abs): ((String, f64, f64), (String, f64, f64)) =
        ((String::new(), 0.0, 0.0), (String::new(), 0.0, 0.0));
    for (i, name) in e.names.iter().enumerate() {
        let s = &e.mean[i];
        let sf = s.last().copied().unwrap_or(0.0);
        let mut row = json!({"place": name, "ssa_final": sf});
        if let Some(o) = ode.as_ref().and_then(|m| m.get(name)) {
            ode_series.push(o.clone());
            let of = o.last().copied().unwrap_or(0.0);
            let max_abs = s
                .iter()
                .zip(o)
                .map(|(x, y)| (x - y).abs())
                .fold(0.0, f64::max);
            let peak = s.iter().chain(o).map(|x| x.abs()).fold(0.0, f64::max);
            let max_rel = if peak > 0.0 { max_abs / peak } else { 0.0 };
            let scale = sf.abs().max(of.abs());
            if scale > 0.0 {
                let (w, gap) = if scale < 1.0 {
                    (&mut abs, (sf - of).abs())
                } else {
                    (&mut rel, (sf - of).abs() / scale)
                };
                if w.0.is_empty() || gap > w.1 {
                    *w = (name.clone(), gap, scale);
                }
            }
            row["ode_final"] = json!(of);
            row["max_abs_diff"] = json!(max_abs);
            row["max_rel_diff"] = json!(max_rel);
        } else {
            ode_series.push(vec![]);
        }
        places.push(row);
    }
    let finals_check = match &ode {
        None => json!({
            "question": format!("expected final state at t = {t_end}"),
            "readings": [{"reading": "ssa"}, {"reading": "ode", "skipped": format!("the continuous relaxation cannot express this net's gating: {}", gating.join("; "))}],
            "agreement": "single-reading",
            "detail": "only the discrete engine can answer; nothing to compare",
        }),
        Some(_) => {
            let head = if rel.0.is_empty() || (rel.1 < 0.05 && abs.1 >= 0.05) {
                &abs
            } else {
                &rel
            };
            let small = head.2 < 10.0;
            let (agreement, mut detail) = if head.0.is_empty() {
                ("close", "both readings put every place at zero".to_string())
            } else if head.1 < 0.05 {
                (
                    "close",
                    format!(
                        "discrete mean and mean-field agree within 5% everywhere (worst: {} at {:.1}%)",
                        head.0,
                        head.1 * 100.0
                    ),
                )
            } else if head.2 < 1.0 {
                (
                    "divergent",
                    format!(
                        "worst gap {:.2} tokens at {} (a sub-unit expectation, compared absolutely)",
                        head.1, head.0
                    ),
                )
            } else {
                (
                    "divergent",
                    format!("worst gap {:.0}% at {}", head.1 * 100.0, head.0),
                )
            };
            if !head.0.is_empty() {
                detail += match (agreement, small) {
                    ("close", true) => " — at small counts, where the two need not agree",
                    ("close", false) => " — the large-count regime",
                    (_, true) => " — small counts: this is the mean-field boundary, and the gap is the finding, not an error",
                    _ => "",
                };
            }
            json!({
                "question": format!("expected final state at t = {t_end}"),
                "readings": [{"reading": "ssa"}, {"reading": "ode"}],
                "agreement": agreement,
                "detail": detail,
            })
        }
    };

    // 2. Conservation: every P-invariant along both trajectories.
    let (laws, truncated) = st::p_invariants(&net, &start);
    let mut invariants = vec![];
    let mut violated: Option<String> = None;
    for law in &laws {
        let tol = 1e-6 * (law.value as f64).abs().max(1.0);
        let dev = |sums: Vec<f64>| -> f64 {
            sums.iter()
                .map(|x| (x - law.value as f64).abs())
                .fold(0.0, f64::max)
        };
        let ssa_dev = dev(law.sums(&e.names, &e.mean));
        let holds = ssa_dev <= tol;
        if !holds && violated.is_none() {
            violated = Some(format!(
                "{}: simulated deviation {ssa_dev:.3}",
                law.expression()
            ));
        }
        let mut row = json!({
            "expression": law.expression(),
            "coefficients": law.terms.iter().cloned().collect::<BTreeMap<String, i64>>(),
            "value": law.value,
            "ssa_max_deviation": ssa_dev,
            "holds": holds,
        });
        if ode.is_some() {
            let d = dev(law.sums(&e.names, &ode_series));
            row["ode_max_deviation"] = json!(d);
            row["ode_holds"] = json!(d <= 1e-3 * (law.value as f64).abs().max(1.0));
        }
        invariants.push(row);
    }
    let conservation = if laws.is_empty() {
        json!({
            "question": "conservation laws (algebra vs simulation)",
            "agreement": "single-reading",
            "detail": "the incidence matrix admits no semi-positive P-invariant",
        })
    } else {
        json!({
            "question": "conservation laws (algebra vs simulation)",
            "readings": [
                {"reading": "farkas", "summary": format!("{} law(s) derived{}", laws.len(), if truncated { " (elimination cut short; list may be incomplete)" } else { "" })},
                {"reading": "ssa", "summary": format!("{} law(s) hold along the simulated means", invariants.iter().filter(|r| r["holds"] == json!(true)).count())},
            ],
            "agreement": if violated.is_none() { "exact" } else { "divergent" },
            "detail": match &violated {
                None => "every derived law holds exactly along the simulated means".to_string(),
                Some(v) => format!("VIOLATED — {v}; an algebraic law failing in simulation indicates an engine or conversion defect"),
            },
        })
    };
    let verdict = if violated.is_some() {
        "defect"
    } else if ode.is_none() {
        "single-reading"
    } else if finals_check["agreement"] == json!("close") {
        "agree"
    } else {
        "diverge"
    };

    let mut overlay = series(&e, Some("ssa"), None);
    if let Some(o) = &ode {
        for name in &e.names {
            if let Some(v) = o.get(name) {
                overlay.push(Series {
                    name: name.clone(),
                    values: v.clone(),
                    band: None,
                    group: Some("ode".into()),
                });
            }
        }
    }
    let ts = sess.add_artifact(
        format!("Crosscheck SSA/ODE ({runs} runs, seed {seed})"),
        source("sim.crosscheck", a),
        ArtifactData::TimeSeries {
            x_label: "t".into(),
            x: e.t.clone(),
            series: overlay,
        },
    );
    let report = json!({
        "verdict": verdict,
        "t_end": t_end, "runs": runs, "seed": seed,
        "checks": [finals_check, conservation],
        "agreement": places,
        "invariants": invariants,
        "invariants_truncated": truncated,
    });
    let rep = sess.add_artifact(
        format!("Crosscheck report ({runs} runs, seed {seed})"),
        source("sim.crosscheck", a),
        ArtifactData::Report(report.clone()),
    );
    Ok(json!({
        "artifacts": [ts, rep],
        "timeseries": ts,
        "report": rep,
        "verdict": verdict,
        "checks": report["checks"],
        "laws": laws.iter().map(|l| l.expression()).collect::<Vec<_>>(),
    }))
}

/// `win_<player>` (or `<player>_win`) minus every other win place.
fn default_objective(net: &Net, player: Option<&str>) -> Result<Objective> {
    let need = || {
        Error::BadArgs(
            "sim.evaluate needs an objective: pass objective (e.g. \"win_x - win_o\"), or a player whose win_<player> place exists".into(),
        )
    };
    let p = player.ok_or_else(need)?;
    let key = p.trim_end_matches(['_', '-', '.']).to_lowercase();
    let mine = [
        format!("win_{key}"),
        format!("{key}_win"),
        format!("win{key}"),
    ];
    let mut ids: Vec<String> = net.places.iter().map(|q| net.base(q).0).collect();
    ids.dedup();
    let own: Vec<&String> = ids
        .iter()
        .filter(|q| mine.contains(&q.to_lowercase()))
        .collect();
    if own.is_empty() {
        return Err(need());
    }
    let mut obj = Map::new();
    for q in &own {
        obj.insert((*q).clone(), json!(1.0));
    }
    for q in &ids {
        let l = q.to_lowercase();
        if !own.contains(&q) && (l.starts_with("win_") || l.ends_with("_win")) {
            obj.insert(q.clone(), json!(-1.0));
        }
    }
    Objective::parse(&Value::Object(obj), net)
}

fn evaluate(sess: &mut Session, a: &Value) -> Result<Value> {
    let net = Net::new(&sess.doc)?;
    let player = s(a, "player");
    let start = net.model_marking(&sess.marking);
    let enabled = net.model.enabled_transitions(&start);
    let candidates: Vec<String> = match player {
        None => enabled,
        Some(p) => {
            let by_role: Vec<&str> = sess
                .doc
                .transitions
                .iter()
                .filter(|t| t.role.as_deref() == Some(p))
                .map(|t| t.id.as_str())
                .collect();
            let owned = |t: &String| {
                if by_role.is_empty() {
                    t.starts_with(p)
                } else {
                    by_role.contains(&t.as_str())
                }
            };
            if by_role.is_empty() && !net.model.transitions.iter().any(|t| t.id.starts_with(p)) {
                return Err(Error::NotFound(format!(
                    "no transition has role {p:?} or an id starting with {p:?}"
                )));
            }
            enabled.into_iter().filter(owned).collect()
        }
    };
    if candidates.is_empty() {
        return Err(Error::Invalid(match player {
            Some(p) => format!("player {p:?} has no enabled move at this marking"),
            None => "no transition is enabled at this marking".into(),
        }));
    }
    let objective = match a.get("objective").filter(|v| !v.is_null()) {
        Some(v) => Objective::parse(v, &net)?,
        None => default_objective(&net, player)?,
    };
    let rollouts = uint(a, "rollouts", 40)?;
    st::check_runs("rollouts", rollouts)?;
    let seed = seed(a, 11)?;
    let rollout = match (n(a, "horizon"), n(a, "depth")) {
        (Some(_), Some(_)) => {
            return Err(Error::BadArgs(
                "give horizon (SSA rollouts) or depth (token-game playouts), not both".into(),
            ))
        }
        (_, Some(d)) => {
            if !(d >= 1.0 && d <= st::MAX_DEPTH as f64) {
                return Err(Error::BadArgs(format!(
                    "depth {d} is outside 1..{}",
                    st::MAX_DEPTH
                )));
            }
            Rollout::Random { depth: d as usize }
        }
        (h, None) => {
            let h = h.unwrap_or(3.0);
            st::check_horizon("horizon", h, st::MAX_HORIZON)?;
            Rollout::Ssa { horizon: h }
        }
    };
    let engine = match rollout {
        Rollout::Ssa { .. } => "ssa",
        Rollout::Random { .. } => "token-game",
    };
    let (baseline, moves) = st::evaluate(
        &net,
        &sess.marking,
        &sess.rates,
        &candidates,
        &objective,
        rollout,
        rollouts,
        seed,
    )?;
    let rows = moves
        .iter()
        .enumerate()
        .map(|(i, m)| {
            vec![
                json!(i + 1),
                json!(m.transition),
                json!(m.payout),
                json!(m.delta),
            ]
        })
        .collect();
    let who = player.unwrap_or("all");
    let id = sess.add_artifact(
        format!("Evaluate {who} ({engine}, {rollouts} rollouts, seed {seed})"),
        source("sim.evaluate", a),
        ArtifactData::Table {
            columns: ["rank", "transition", "payout", "vs_baseline"]
                .map(String::from)
                .to_vec(),
            rows,
        },
    );
    Ok(json!({
        "artifact": id,
        "player": player,
        "engine": engine,
        "objective": objective.expression,
        "baseline": baseline,
        "best": moves.first().map(|m| m.transition.clone()),
        "moves": moves.iter().map(|m| json!({"transition": m.transition, "payout": m.payout, "vs_baseline": m.delta})).collect::<Vec<_>>(),
    }))
}

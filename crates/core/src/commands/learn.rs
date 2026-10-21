//! Learning commands: `data.calibrate` (rates from an event log, sim's
//! `sim_calibrate`), `opt.fit` (ODE rate fitting to observed series,
//! pflow-learn / pflow.xyz's `fitRates`) and `opt.distill` (rate
//! multipliers that make the play scorer agree with exact minimax, sim's
//! `sim_distill`). The engines live in [`crate::learn`].
//!
//! `apply: true` writes learned rates into the session rates. Session rates
//! are not part of the undo snapshots, so that step is **not undoable**: the
//! result carries `previous_rates` and the `restore` steps (`sim.set_rate`)
//! that put them back.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::artifact::{ArtifactData, Series};
use crate::commands::sim::ode_options;
use crate::eventlog;
use crate::learn::{self, DistillRequest, FitRequest, Game, Loss};
use crate::registry::{always, arg, b, n, opt, req, s, strs, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::stochastic as st;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "data.calibrate",
        label: "Calibrate Rates from Log…",
        help: "sim_calibrate: learn per-transition rates from an event-log artifact. Every event is a firing; \
               sources (no input place) get the case inter-arrival rate, others the rate of the gaps before their \
               events (racing transitions split by completion share); session rate ≥ 100 = instant pickup, kept. \
               Activities map to transitions by id, else by id or label ignoring case. Stores a rate-evidence table \
               and a report (rates, unmapped activities, token-replay conformance). `apply` sets the session rates \
               (not undoable: the result lists the previous rates and the steps that restore them).",
        menu: "Data",
        shortcut: None,
        args: &[
            arg("log", Str, "event-log artifact id (data.load_log, or a table with case/activity/time columns)"),
            opt("unit", Str, "model time unit the rates are per: hour (default, as sim), min, s, ms, day"),
            opt("apply", Bool, "set the learned rates as session rates (default false; not undoable)"),
            opt("conformance", Bool, "replay the log for fitness/precision (default true)"),
        ],
        enabled: always,
        run: calibrate,
        journal: true,
    },
    CommandSpec {
        id: "opt.fit",
        label: "Fit Rates to Data…",
        help: "Fit transition rates so the mass-action ODE (from the current marking, over the data's time span) \
               matches the observed series of a timeseries artifact (pflow-learn; pflow.xyz fitRates). Series are \
               matched to places by display name (S, CoffeeBeans.brown), base place id or place label, or through \
               `places`. Stores a report and an overlay (group \"data\" vs \"fit\"). `apply` sets the session rates \
               (not undoable).",
        menu: "Workbench",
        shortcut: None,
        args: &[
            arg("data", Str, "timeseries artifact id holding the observations"),
            opt("parameters", StrList, "transitions whose rates are fitted (default: every transition with a positive rate)"),
            opt("method", Str, "nelder-mead (default, as pflow.xyz), coordinate-descent, adam, gradient-descent"),
            opt("loss", Str, "mse (default), rmse, relative"),
            opt("space", Str, "log (default: fit ln(rate), steps are relative, rates stay positive) or linear (pflow.xyz)"),
            opt("max_iters", Int, "optimizer iterations, 1..5000 (default 1000, as pflow.xyz)"),
            opt("tolerance", Num, "convergence tolerance (default 1e-4, as pflow.xyz)"),
            opt("places", Json, "{series name: place} when series names are not place names"),
            opt("group", Str, "fit only series of this overlay group (default: ungrouped or \"data\")"),
            opt("apply", Bool, "set the fitted rates as session rates (default false; not undoable)"),
            opt("solver", Str, "ODE method: tsit5 (default), rk45, rk4, euler, heun, midpoint, bs32"),
            opt("dt", Num, "initial step (default 0.01)"),
            opt("abstol", Num, "absolute tolerance (default 1e-6)"),
            opt("reltol", Num, "relative tolerance (default 1e-3)"),
            opt("dtmax", Num, "largest step (default min(0.1, span/50))"),
        ],
        enabled: always,
        run: fit,
        journal: true,
    },
    CommandSpec {
        id: "opt.distill",
        label: "Distill Search into Play…",
        help: "sim_distill: fit rate multipliers for transition groups so sim.evaluate's play scorer (seeded SSA \
               rollouts) ranks moves as exact minimax does, on positions from seeded random self-play. The game is \
               read from places named <player>_turn (moves consume the turn token and pass the turn), or from \
               `players`. Reports agreement before/after; stores a report and a per-position table. `apply` sets \
               the multiplied rates (not undoable).",
        menu: "Workbench",
        shortcut: None,
        args: &[
            opt("player", Str, "the player who maximizes the objective (default: the one to move now)"),
            opt("players", Json, "{name: {turn, transitions?, maximizes?}} instead of the <name>_turn inference"),
            opt("objective", Json, "linear objective, e.g. \"win_x - win_o\" (default win_<player> minus the other win_* places)"),
            opt("groups", Json, "{group: [transition ids]} sharing one multiplier (≤ 16; default: moves and house)"),
            opt("games", Int, "self-play games to sample from, 1..100 (default 20)"),
            opt("positions", Int, "labeled positions, 1..100 (default 40)"),
            opt("iters", Int, "Nelder-Mead iterations, 1..100 (default 40)"),
            opt("horizon", Num, "SSA rollout horizon (default 3)"),
            opt("realizations", Int, "rollouts per candidate move, 1..200 (default 40)"),
            opt("seed", Int, "self-play and rollout seed (default 11)"),
            opt("engine", Str, "ssa (the only engine here)"),
            opt("apply", Bool, "multiply the groups' session rates by the fit (default false; not undoable)"),
        ],
        enabled: always,
        run: distill,
        journal: true,
    },
];

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

/// Writes `rates` into the session rates; returns (previous rates, the
/// `sim.set_rate` steps that restore them).
fn apply_rates(
    sess: &mut Session,
    rates: &BTreeMap<String, f64>,
) -> (Map<String, Value>, Vec<Step>) {
    let mut prev = Map::new();
    let mut restore = vec![];
    for (t, r) in rates {
        let old = sess.rates.get(t).copied().unwrap_or(1.0);
        prev.insert(t.clone(), json!(old));
        restore.push(Step {
            command: "sim.set_rate".into(),
            args: json!({ "id": t, "rate": old }),
        });
        sess.rates.insert(t.clone(), *r);
    }
    (prev, restore)
}

fn applied(sess: &mut Session, a: &Value, rates: &BTreeMap<String, f64>, out: &mut Value) {
    out["applied"] = json!(false);
    if b(a, "apply").unwrap_or(false) {
        let (prev, restore) = apply_rates(sess, rates);
        out["applied"] = json!(true);
        out["previous_rates"] = Value::Object(prev);
        out["restore"] = json!(restore);
        out["undo"] = json!("session rates are not in the undo history; run the `restore` steps to put the previous rates back");
    }
}

fn calibrate(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "log");
    let art = sess
        .artifact(id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
    let log = eventlog::from_artifact(art)?;
    let unit = s(a, "unit").unwrap_or("hour");
    let cal = learn::calibrate(&sess.doc, &sess.rates, &log, unit)?;
    let conformance = if b(a, "conformance").unwrap_or(true) {
        let mapped = learn::remap_log(&log, &cal.mapping);
        learn::conformance(&sess.doc, &mapped)?
    } else {
        Value::Null
    };
    let rows: Vec<Vec<Value>> = cal
        .rows
        .iter()
        .map(|r| {
            let ev = r.evidence.as_ref();
            vec![
                json!(r.transition),
                json!(r.activities.join(", ")),
                json!(r.kind),
                json!(ev.map_or(0, |e| e.n)),
                json!(ev.and_then(|e| e.mean_gap)),
                json!(ev.map(|e| e.gap_cv)),
                json!(r.rate),
                json!(r.previous),
                json!(ev.is_some_and(|e| e.insufficient)),
            ]
        })
        .collect();
    let columns = [
        "transition",
        "activities",
        "kind",
        "n",
        "mean_gap",
        "gap_cv",
        "rate",
        "previous_rate",
        "insufficient",
    ]
    .map(String::from)
    .to_vec();
    let evidence: Map<String, Value> = cal
        .rows
        .iter()
        .filter_map(|r| {
            r.evidence
                .as_ref()
                .map(|e| (r.transition.clone(), json!(e)))
        })
        .collect();
    let mut report = json!({
        "learned_rates": cal.rates,
        "unit": cal.unit,
        "rate_evidence": evidence,
        "mapping": cal.mapping.mapped,
        "unmapped": cal.mapping.unmapped,
        "ambiguous": cal.mapping.ambiguous,
        "unobserved": cal.unobserved,
        "events": cal.events,
        "conformance": conformance,
        "caveats": cal.caveats,
    });
    let title = format!("Calibration from {id} (per {unit})");
    let table = sess.add_artifact(
        format!("{title}: rate evidence"),
        source("data.calibrate", a),
        ArtifactData::Table { columns, rows },
    );
    let rep = sess.add_artifact(
        title,
        source("data.calibrate", a),
        ArtifactData::Report(report.clone()),
    );
    report["table"] = json!(table);
    report["report"] = json!(rep);
    let rates = cal.rates.clone();
    applied(sess, a, &rates, &mut report);
    Ok(report)
}

fn count(a: &Value, k: &str, default: usize) -> Result<usize> {
    match n(a, k) {
        None => Ok(default),
        Some(x) if x >= 0.0 => Ok(x as usize),
        Some(x) => Err(Error::BadArgs(format!("{k} must be ≥ 0, got {x}"))),
    }
}

fn fit(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "data");
    let art = sess
        .artifact(id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
    let places = match a.get("places").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => {
            let v = st::json_arg(v, "places")?;
            let m = v
                .as_object()
                .filter(|m| !m.is_empty())
                .ok_or_else(|| Error::BadArgs("places must be {series name: place}".into()))?;
            Some(
                m.iter()
                    .map(|(k, p)| {
                        p.as_str()
                            .map(|p| (k.clone(), p.to_string()))
                            .ok_or_else(|| {
                                Error::BadArgs(format!("places.{k} must be a place name"))
                            })
                    })
                    .collect::<Result<Vec<_>>>()?,
            )
        }
    };
    let space = s(a, "space").unwrap_or("log");
    let log_space = match space {
        "log" => true,
        "linear" => false,
        other => return Err(Error::BadArgs(format!("space {other:?}: log or linear"))),
    };
    let request = FitRequest {
        parameters: strs(a, "parameters"),
        method: learn::fit_method(s(a, "method").unwrap_or("nelder-mead"))?,
        loss: Loss::parse(s(a, "loss").unwrap_or("mse"))?,
        log_space,
        max_iters: count(a, "max_iters", 1000)?,
        tolerance: n(a, "tolerance").unwrap_or(1e-4),
        solver: s(a, "solver").unwrap_or("tsit5").to_string(),
        ode: ode_options(a),
        places,
        group: s(a, "group").map(str::to_string),
    };
    let out = learn::fit(&sess.doc, &sess.marking, &sess.rates, &art.data, &request)?;
    let mut series = vec![];
    for fs in &out.series {
        let mut d = Series::new(fs.place.clone(), fs.observed.clone());
        d.group = Some("data".into());
        let mut f = Series::new(fs.place.clone(), fs.fitted.clone());
        f.group = Some("fit".into());
        series.push(d);
        series.push(f);
    }
    let overlay = sess.add_artifact(
        format!("Fit to {id}: data vs fit"),
        source("opt.fit", a),
        ArtifactData::TimeSeries {
            x_label: "t".into(),
            x: out.times.clone(),
            series,
        },
    );
    let mut report = json!({
        "data": id,
        "parameters": out.parameters,
        "initial_rates": out.initial_rates,
        "fitted_rates": out.fitted_rates,
        "loss": out.loss,
        "loss_before": out.loss_before,
        "loss_after": out.loss_after,
        "iterations": out.iterations,
        "evaluations": out.evaluations,
        "converged": out.converged,
        "method": s(a, "method").unwrap_or("nelder-mead"),
        "space": space,
        "observed": out.series.iter().map(|f| json!({"series": f.series, "place": f.place})).collect::<Vec<_>>(),
        "ignored_series": out.ignored,
        "caveats": out.caveats,
    });
    let rep = sess.add_artifact(
        format!("Fit to {id}"),
        source("opt.fit", a),
        ArtifactData::Report(report.clone()),
    );
    report["timeseries"] = json!(overlay);
    report["report"] = json!(rep);
    let rates = out.fitted_rates.clone();
    applied(sess, a, &rates, &mut report);
    Ok(report)
}

fn parse_groups(v: &Value) -> Result<BTreeMap<String, Vec<String>>> {
    let v = st::json_arg(v, "groups")?;
    let bad = || Error::BadArgs("groups must be {group: [transition ids]}".into());
    let m = v.as_object().ok_or_else(bad)?;
    m.iter()
        .map(|(g, ts)| {
            let ts = ts.as_array().ok_or_else(bad)?;
            let ts = ts
                .iter()
                .map(|t| t.as_str().map(str::to_string).ok_or_else(bad))
                .collect::<Result<Vec<_>>>()?;
            Ok((g.clone(), ts))
        })
        .collect()
}

fn distill(sess: &mut Session, a: &Value) -> Result<Value> {
    if let Some(e) = s(a, "engine").filter(|e| *e != "ssa") {
        return Err(Error::BadArgs(format!(
            "engine {e:?}: only ssa (the play scorer's seeded rollouts) is implemented here"
        )));
    }
    let game = Game::new(
        &sess.doc,
        &sess.marking,
        s(a, "player"),
        a.get("players").filter(|v| !v.is_null()),
        a.get("objective").filter(|v| !v.is_null()),
    )?;
    let groups = match a.get("groups").filter(|v| !v.is_null()) {
        Some(v) => parse_groups(v)?,
        None => learn::default_groups(&game),
    };
    let seed = match a.get("seed").filter(|v| !v.is_null()) {
        Some(v) => st::parse_seed(v, "seed")?,
        None => learn::DISTILL_DEFAULT_SEED,
    };
    let request = DistillRequest {
        groups,
        games: count(a, "games", learn::DISTILL_DEFAULT_GAMES)?,
        positions: count(a, "positions", learn::DISTILL_DEFAULT_POSITIONS)?,
        iters: count(a, "iters", learn::DISTILL_DEFAULT_ITERS)?,
        horizon: n(a, "horizon").unwrap_or(learn::DISTILL_DEFAULT_HORIZON),
        realizations: count(a, "realizations", learn::DISTILL_DEFAULT_REALIZATIONS)?,
        seed,
    };
    let d = learn::distill(&game, &sess.marking, &sess.rates, &request)?;
    let rows: Vec<Vec<Value>> = d
        .detail
        .iter()
        .enumerate()
        .map(|(i, p)| {
            vec![
                json!(i),
                json!(p.player),
                json!(p.moves.join(" ")),
                json!(p.preferred.join(" ")),
                json!(p.before),
                json!(p.after),
                json!(p.preferred.contains(&p.before)),
                json!(p.preferred.contains(&p.after)),
                json!(p.exact),
            ]
        })
        .collect();
    let columns = [
        "position",
        "player",
        "moves",
        "search_best",
        "play_before",
        "play_after",
        "agrees_before",
        "agrees_after",
        "exact",
    ]
    .map(String::from)
    .to_vec();
    let mut report = serde_json::to_value(&d).expect("serializable");
    report["players"] = json!(game.players);
    report["objective"] = json!(game.objective.expression);
    report["seed"] = json!(request.seed);
    let title = format!(
        "Distill ({} positions, agreement {} → {})",
        d.positions, d.agreement_before, d.agreement_after
    );
    let table = sess.add_artifact(
        "Distill: positions".into(),
        source("opt.distill", a),
        ArtifactData::Table { columns, rows },
    );
    let rep = sess.add_artifact(
        title,
        source("opt.distill", a),
        ArtifactData::Report(report.clone()),
    );
    report["table"] = json!(table);
    report["report"] = json!(rep);
    let mut rates = BTreeMap::new();
    for (g, ts) in &d.groups {
        for t in ts {
            rates.insert(
                t.clone(),
                sess.rates.get(t).copied().unwrap_or(1.0) * d.multipliers[g],
            );
        }
    }
    applied(sess, a, &rates, &mut report);
    Ok(report)
}

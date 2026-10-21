//! The ODE workbench over rates: pflow.xyz's Rate Scan and Sweep tabs, and
//! sim.pflow.xyz's `sim_param_heatmap` and `sim_optimize`. Every one runs
//! many pflow-solver integrations of one unfolded net ([`Runner`]) from
//! the current marking and session rates, varying only the named rates.

use serde_json::{json, Map, Value};

use crate::artifact::{ArtifactData, Series};
use crate::commands::sim::ode_options;
use crate::registry::{always, arg, b, n, opt, req, s, strs, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::sim::OdeOptions;
use crate::sweep::{self, Runner, MAX_AXIS_STEPS, MAX_GRID_CELLS, MAX_SAMPLES, MAX_SCAN_STEPS};
use crate::{Error, Result};

const METHOD: &str = "tsit5 (default), rk45, rk4, euler, heun, midpoint, bs32";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "opt.rate_scan",
        label: "Rate Scan",
        help: "Sweep one transition's rate from `from` to `to` and plot each place's value at t_end against the rate (pflow.xyz Rate Scan).",
        menu: "Workbench",
        shortcut: None,
        args: &[
            arg("transition", Str, "transition whose rate is swept"),
            arg("from", Num, "first rate (≥ 0)"),
            arg("to", Num, "last rate (≥ 0)"),
            opt("steps", Int, "rate values, 2..25 (default 9)"),
            opt("t_end", Num, "horizon per run (default 50)"),
            opt("places", StrList, "places to plot: display names or base place ids (default all)"),
            opt("method", Str, METHOD),
            opt("dt", Num, "initial step (default 0.01)"),
            opt("abstol", Num, "absolute tolerance (default 1e-6)"),
            opt("reltol", Num, "relative tolerance (default 1e-3)"),
        ],
        enabled: always,
        run: rate_scan,
        journal: true,
    },
    CommandSpec {
        id: "opt.sweep",
        label: "Sweep",
        help: "Overlay one place's trajectory across values of one transition's rate (pflow.xyz Sweep); one series per value, grouped k=<rate>.",
        menu: "Workbench",
        shortcut: None,
        args: &[
            arg("transition", Str, "transition whose rate is swept"),
            arg("place", Str, "observed place (display name or base place id)"),
            arg("from", Num, "first rate (≥ 0)"),
            arg("to", Num, "last rate (≥ 0)"),
            opt("steps", Int, "rate values, 2..25 (default 9)"),
            opt("t_end", Num, "horizon per run (default 10)"),
            opt("points", Int, "points per trajectory, on a shared time grid (default 200)"),
            opt("method", Str, METHOD),
            opt("dt", Num, "initial step (default 0.01)"),
            opt("abstol", Num, "absolute tolerance (default 1e-6)"),
            opt("reltol", Num, "relative tolerance (default 1e-3)"),
        ],
        enabled: always,
        run: sweep_cmd,
        journal: true,
    },
    CommandSpec {
        id: "opt.heatmap",
        label: "Parameter Heatmap",
        help: "Vary two transitions' rates over a grid (≤ 40 × 40) and map the observable place's value at t_end (sim_param_heatmap). Uses coarser solver defaults (abstol 1e-4, dtmax t_end/20) since only the final state is read.",
        menu: "Workbench",
        shortcut: None,
        args: &[
            arg("param_x", Str, "transition on the x axis"),
            arg("param_y", Str, "transition on the y axis"),
            arg("x_from", Num, "first x rate (≥ 0)"),
            arg("x_to", Num, "last x rate (≥ 0)"),
            arg("y_from", Num, "first y rate (≥ 0)"),
            arg("y_to", Num, "last y rate (≥ 0)"),
            arg("observable", Str, "place whose final value fills the grid (a base id sums its colors)"),
            opt("steps", Int, "cells per axis, 2..40 (default 10)"),
            opt("x_steps", Int, "cells on x (overrides steps)"),
            opt("y_steps", Int, "cells on y (overrides steps)"),
            opt("t_end", Num, "horizon per run (default 10)"),
            opt("log", Bool, "space both axes in log10 (needs positive bounds)"),
            opt("method", Str, METHOD),
            opt("dt", Num, "initial step (default 0.01)"),
            opt("abstol", Num, "absolute tolerance (default 1e-4)"),
            opt("reltol", Num, "relative tolerance (default 1e-3)"),
        ],
        enabled: always,
        run: heatmap,
        journal: true,
    },
    CommandSpec {
        id: "opt.optimize",
        label: "Optimize (Pareto)",
        help: "Monte Carlo over transition rates (seeded): one ODE run per sample, objectives read at t_end, non-dominated samples flagged (sim_optimize). Stores a scatter (Pareto highlighted) and a table of every sample.",
        menu: "Workbench",
        shortcut: None,
        args: &[
            arg("parameters", Json, "[{transition, min, max, log?}] (or sim's {transition: [min, max]})"),
            arg("objectives", Json, "[{place, sense: \"max\"|\"min\"}] (`direction` is accepted for sense)"),
            opt("samples", Int, "Monte Carlo samples, 1..2000 (default 100)"),
            opt("seed", Int, "sampling seed (default 42)"),
            opt("t_end", Num, "horizon per run (default 10)"),
            opt("method", Str, METHOD),
            opt("dt", Num, "initial step (default 0.01)"),
            opt("abstol", Num, "absolute tolerance (default 1e-4)"),
            opt("reltol", Num, "relative tolerance (default 1e-3)"),
        ],
        enabled: always,
        run: optimize,
        journal: true,
    },
];

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

fn steps(a: &Value, key: &str, default: usize, max: usize) -> Result<usize> {
    let v = n(a, key).unwrap_or(default as f64);
    if !(2.0..=max as f64).contains(&v) {
        return Err(Error::BadArgs(format!("{key} {v} is outside 2..{max}")));
    }
    Ok(v as usize)
}

fn runner(sess: &Session, a: &Value, t_end: f64, base: OdeOptions) -> Result<Runner> {
    let method = s(a, "method").unwrap_or("tsit5");
    Runner::new(
        &sess.doc,
        &sess.marking,
        &sess.rates,
        t_end,
        method,
        &ode_options(a).or(base),
    )
}

fn rate_scan(sess: &mut Session, a: &Value) -> Result<Value> {
    let tid = req(a, "transition");
    let t_end = n(a, "t_end").unwrap_or(50.0);
    let r = runner(sess, a, t_end, OdeOptions::default())?;
    r.check_transition(tid)?;
    let keep = r.select_many(strs(a, "places").as_deref())?;
    let values = sweep::axis(
        n(a, "from").unwrap(),
        n(a, "to").unwrap(),
        steps(a, "steps", 9, MAX_SCAN_STEPS)?,
        false,
    )?;
    let finals: Vec<Vec<f64>> = values.iter().map(|&v| r.final_state(&[(tid, v)])).collect();
    let names: Vec<String> = keep.iter().map(|&i| r.order()[i].1.clone()).collect();
    let series = keep
        .iter()
        .zip(&names)
        .map(|(&i, name)| Series::new(name.clone(), finals.iter().map(|f| f[i]).collect()))
        .collect();
    let results: Vec<Map<String, Value>> = finals
        .iter()
        .map(|f| {
            keep.iter()
                .zip(&names)
                .map(|(&i, name)| (name.clone(), json!(f[i])))
                .collect()
        })
        .collect();
    let data = ArtifactData::TimeSeries {
        x_label: format!("rate({tid})"),
        x: values.clone(),
        series,
    };
    let id = sess.add_artifact(
        format!("Rate scan: {tid} (t_end={t_end})"),
        source("opt.rate_scan", a),
        data,
    );
    Ok(json!({
        "artifact": id,
        "transition": tid,
        "t_end": t_end,
        "values": values,
        "observables": names,
        "results": results,
    }))
}

fn sweep_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let tid = req(a, "transition");
    let place = req(a, "place");
    let t_end = n(a, "t_end").unwrap_or(10.0);
    let points = n(a, "points").unwrap_or(200.0).clamp(2.0, 10_000.0) as usize;
    let r = runner(sess, a, t_end, OdeOptions::default())?;
    r.check_transition(tid)?;
    let cols = r.select(place)?;
    let values = sweep::axis(
        n(a, "from").unwrap(),
        n(a, "to").unwrap(),
        steps(a, "steps", 9, MAX_SCAN_STEPS)?,
        false,
    )?;
    // Each adaptive run has its own time points; a shared uniform grid
    // lets the runs overlay in one TimeSeries.
    let grid: Vec<f64> = (0..points)
        .map(|i| t_end * i as f64 / (points - 1) as f64)
        .collect();
    let mut series = vec![];
    let mut trajectories = vec![];
    for &v in &values {
        let (t, ys) = r.trajectory(&[(tid, v)]);
        let group = format!("k={}", sweep::fmt_sig(v, 3));
        let mut fin = Map::new();
        for &c in &cols {
            let name = r.order()[c].1.clone();
            fin.insert(name.clone(), json!(ys[c].last().copied().unwrap_or(0.0)));
            let mut s = Series::new(name, sweep::resample(&t, &ys[c], &grid));
            s.group = Some(group.clone());
            series.push(s);
        }
        let total: f64 = cols.iter().filter_map(|&c| ys[c].last()).sum();
        trajectories.push(json!({ "rate": v, "group": group, "final": total, "finals": fin }));
    }
    let data = ArtifactData::TimeSeries {
        x_label: "t".into(),
        x: grid,
        series,
    };
    let id = sess.add_artifact(
        format!("Sweep: {place} across rate({tid})"),
        source("opt.sweep", a),
        data,
    );
    Ok(json!({
        "artifact": id,
        "transition": tid,
        "observable": place,
        "t_end": t_end,
        "trajectories": trajectories,
    }))
}

/// Sum over the series a place name selects, from a final state.
fn observe(cols: &[usize], fin: &[f64]) -> f64 {
    cols.iter().map(|&c| fin[c]).sum()
}

fn heatmap(sess: &mut Session, a: &Value) -> Result<Value> {
    let (px, py, obs) = (req(a, "param_x"), req(a, "param_y"), req(a, "observable"));
    let t_end = n(a, "t_end").unwrap_or(10.0);
    let r = runner(sess, a, t_end, OdeOptions::grid(t_end))?;
    r.check_transition(px)?;
    r.check_transition(py)?;
    if px == py {
        return Err(Error::BadArgs("param_x and param_y must differ".into()));
    }
    let cols = r.select(obs)?;
    let both = steps(a, "steps", 10, MAX_AXIS_STEPS)?;
    let nx = if a.get("x_steps").is_some_and(|v| !v.is_null()) {
        steps(a, "x_steps", both, MAX_AXIS_STEPS)?
    } else {
        both
    };
    let ny = if a.get("y_steps").is_some_and(|v| !v.is_null()) {
        steps(a, "y_steps", both, MAX_AXIS_STEPS)?
    } else {
        both
    };
    debug_assert!(nx * ny <= MAX_GRID_CELLS);
    let log = b(a, "log").unwrap_or(false);
    let xs = sweep::axis(n(a, "x_from").unwrap(), n(a, "x_to").unwrap(), nx, log)?;
    let ys = sweep::axis(n(a, "y_from").unwrap(), n(a, "y_to").unwrap(), ny, log)?;
    let z: Vec<Vec<f64>> = ys
        .iter()
        .map(|&y| {
            xs.iter()
                .map(|&x| observe(&cols, &r.final_state(&[(px, x), (py, y)])))
                .collect()
        })
        .collect();
    let (min, max) = z
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    let data = ArtifactData::Heatmap {
        x_label: format!("rate({px})"),
        y_label: format!("rate({py})"),
        xs: xs.clone(),
        ys: ys.clone(),
        z: z.clone(),
    };
    let id = sess.add_artifact(
        format!("Heatmap: {obs} over {px} × {py}"),
        source("opt.heatmap", a),
        data,
    );
    Ok(json!({
        "artifact": id,
        "param_x": px,
        "param_y": py,
        "observable": obs,
        "t_end": t_end,
        "xs": xs,
        "ys": ys,
        "z": z,
        "min": min,
        "max": max,
    }))
}

struct Param {
    transition: String,
    min: f64,
    max: f64,
    log: bool,
}

fn parse_params(v: &Value) -> Result<Vec<Param>> {
    let bad = || {
        Error::BadArgs(
            "parameters must be a non-empty [{transition, min, max, log?}] or {transition: [min, max]}"
                .into(),
        )
    };
    let out: Vec<Param> = match v {
        Value::Array(items) => items
            .iter()
            .map(|p| {
                Ok(Param {
                    transition: p
                        .get("transition")
                        .and_then(Value::as_str)
                        .ok_or_else(bad)?
                        .into(),
                    min: p.get("min").and_then(Value::as_f64).ok_or_else(bad)?,
                    max: p.get("max").and_then(Value::as_f64).ok_or_else(bad)?,
                    log: p.get("log").and_then(Value::as_bool).unwrap_or(false),
                })
            })
            .collect::<Result<_>>()?,
        Value::Object(m) => m
            .iter()
            .map(|(k, r)| {
                let r = r.as_array().filter(|r| r.len() == 2).ok_or_else(bad)?;
                Ok(Param {
                    transition: k.clone(),
                    min: r[0].as_f64().ok_or_else(bad)?,
                    max: r[1].as_f64().ok_or_else(bad)?,
                    log: false,
                })
            })
            .collect::<Result<_>>()?,
        _ => return Err(bad()),
    };
    if out.is_empty() {
        return Err(bad());
    }
    for (i, p) in out.iter().enumerate() {
        sweep::check_rate(&format!("{} min", p.transition), p.min)?;
        sweep::check_rate(&format!("{} max", p.transition), p.max)?;
        if p.min > p.max {
            return Err(Error::BadArgs(format!(
                "{}: min {} is above max {}",
                p.transition, p.min, p.max
            )));
        }
        if p.log && p.min <= 0.0 {
            return Err(Error::BadArgs(format!(
                "{}: log sampling needs min > 0",
                p.transition
            )));
        }
        if out[..i].iter().any(|q| q.transition == p.transition) {
            return Err(Error::BadArgs(format!(
                "{} appears twice in parameters",
                p.transition
            )));
        }
    }
    Ok(out)
}

/// (place, maximize) pairs.
fn parse_objectives(v: &Value) -> Result<Vec<(String, bool)>> {
    let bad = || {
        Error::BadArgs("objectives must be a non-empty [{place, sense: \"max\"|\"min\"}]".into())
    };
    let items = v.as_array().filter(|a| !a.is_empty()).ok_or_else(bad)?;
    items
        .iter()
        .map(|o| {
            let place = o.get("place").and_then(Value::as_str).ok_or_else(bad)?;
            let sense = o
                .get("sense")
                .or_else(|| o.get("direction"))
                .and_then(Value::as_str)
                .unwrap_or("max");
            match sense {
                "max" => Ok((place.to_string(), true)),
                "min" => Ok((place.to_string(), false)),
                other => Err(Error::BadArgs(format!(
                    "objective {place:?}: sense must be max or min, got {other:?}"
                ))),
            }
        })
        .collect()
}

fn optimize(sess: &mut Session, a: &Value) -> Result<Value> {
    let params = parse_params(&a["parameters"])?;
    let objectives = parse_objectives(&a["objectives"])?;
    let samples = n(a, "samples").unwrap_or(100.0);
    if !(1.0..=MAX_SAMPLES as f64).contains(&samples) {
        return Err(Error::BadArgs(format!(
            "samples {samples} is outside 1..{MAX_SAMPLES}"
        )));
    }
    let samples = samples as usize;
    let seed = n(a, "seed").unwrap_or(42.0) as i64;
    let t_end = n(a, "t_end").unwrap_or(10.0);
    let r = runner(sess, a, t_end, OdeOptions::grid(t_end))?;
    for p in &params {
        r.check_transition(&p.transition)?;
    }
    let cols: Vec<Vec<usize>> = objectives
        .iter()
        .map(|(place, _)| r.select(place))
        .collect::<Result<_>>()?;

    let mut rng = sweep::Rng::new(seed as u64);
    let mut rates: Vec<Vec<f64>> = Vec::with_capacity(samples);
    let mut values: Vec<Vec<f64>> = Vec::with_capacity(samples);
    for _ in 0..samples {
        let row: Vec<f64> = params
            .iter()
            .map(|p| {
                let u = rng.next_f64();
                if p.log {
                    10f64.powf(p.min.log10() + u * (p.max.log10() - p.min.log10()))
                } else {
                    p.min + u * (p.max - p.min)
                }
            })
            .collect();
        let overrides: Vec<(&str, f64)> = params
            .iter()
            .zip(&row)
            .map(|(p, &v)| (p.transition.as_str(), v))
            .collect();
        let fin = r.final_state(&overrides);
        values.push(cols.iter().map(|c| observe(c, &fin)).collect());
        rates.push(row);
    }
    let maximize: Vec<bool> = objectives.iter().map(|(_, m)| *m).collect();
    let front = sweep::pareto(&values, &maximize);
    let highlight: Vec<usize> = (0..samples).filter(|&i| front[i]).collect();

    let label =
        |(place, max): &(String, bool)| format!("{place} ({})", if *max { "max" } else { "min" });
    let (x_label, y_label, points): (String, String, Vec<(f64, f64)>) = if objectives.len() >= 2 {
        (
            label(&objectives[0]),
            label(&objectives[1]),
            values.iter().map(|v| (v[0], v[1])).collect(),
        )
    } else {
        (
            format!("rate({})", params[0].transition),
            label(&objectives[0]),
            rates
                .iter()
                .zip(&values)
                .map(|(r, v)| (r[0], v[0]))
                .collect(),
        )
    };
    let mut columns = vec!["sample".to_string()];
    columns.extend(params.iter().map(|p| format!("rate({})", p.transition)));
    columns.extend(objectives.iter().map(label));
    columns.push("pareto".into());
    let rows: Vec<Vec<Value>> = (0..samples)
        .map(|i| {
            let mut row = vec![json!(i)];
            row.extend(rates[i].iter().map(|v| json!(v)));
            row.extend(values[i].iter().map(|v| json!(v)));
            row.push(json!(front[i]));
            row
        })
        .collect();
    let pareto: Vec<Value> = highlight
        .iter()
        .map(|&i| {
            let rates: Map<String, Value> = params
                .iter()
                .zip(&rates[i])
                .map(|(p, v)| (p.transition.clone(), json!(v)))
                .collect();
            let vals: Map<String, Value> = objectives
                .iter()
                .zip(&values[i])
                .map(|((place, _), v)| (place.clone(), json!(v)))
                .collect();
            json!({ "index": i, "rates": rates, "values": vals })
        })
        .collect();
    let what: Vec<&str> = objectives.iter().map(|(p, _)| p.as_str()).collect();
    let title = format!("Optimize: {}", what.join(" × "));
    let scatter = sess.add_artifact(
        title.clone(),
        source("opt.optimize", a),
        ArtifactData::Scatter {
            x_label,
            y_label,
            points,
            highlight: highlight.clone(),
        },
    );
    let table = sess.add_artifact(
        format!("{title} (samples)"),
        source("opt.optimize", a),
        ArtifactData::Table { columns, rows },
    );
    Ok(json!({
        "scatter": scatter,
        "table": table,
        "samples": samples,
        "seed": seed,
        "t_end": t_end,
        "pareto_count": highlight.len(),
        "pareto": pareto,
    }))
}

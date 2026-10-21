use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::artifact::{ArtifactData, Series};
use crate::registry::{always, arg, n, opt, req, s, strs, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::sim;
use crate::sweep::Runner;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "sim.enabled",
        label: "Enabled Transitions",
        help: "Transitions enabled at the current marking.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: enabled,
        journal: false,
    },
    CommandSpec {
        id: "sim.fire",
        label: "Fire",
        help: "Fire an enabled transition (pflow-metamodel firing rule); refuses with the reason otherwise.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "transition id")],
        enabled: always,
        run: fire,
        journal: true,
    },
    CommandSpec {
        id: "sim.run",
        label: "Run Random Steps",
        help: "Fire up to `steps` randomly chosen enabled transitions (seeded, reproducible).",
        menu: "Simulate",
        shortcut: None,
        args: &[opt("steps", Int, "how many firings (default 10)"), opt("seed", Int, "RNG seed (default 1)")],
        enabled: always,
        run: run_steps,
        journal: true,
    },
    CommandSpec {
        id: "sim.reset",
        label: "Reset Marking",
        help: "Return the token game to the document's initial marking.",
        menu: "Simulate",
        shortcut: Some("R"),
        args: &[],
        enabled: always,
        run: reset,
        journal: true,
    },
    CommandSpec {
        id: "sim.marking",
        label: "Marking",
        help: "The current marking (tokens per place, per color).",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: marking,
        journal: false,
    },
    CommandSpec {
        id: "sim.set_rate",
        label: "Set Rate",
        help: "Set a transition's mass-action rate for the ODE (session-only, as in pflow.xyz; 0 disables).",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "transition id"), arg("rate", Num, "rate ≥ 0")],
        enabled: always,
        run: set_rate,
        journal: true,
    },
    CommandSpec {
        id: "sim.ode",
        label: "Run ODE",
        help: "Integrate the mass-action ODE from the current marking (pflow-solver) and store a time-series artifact.",
        menu: "Simulate",
        shortcut: Some("Cmd+R"),
        args: &[
            opt("t_end", Num, "end time (default 10)"),
            opt("method", Str, "tsit5 (default), rk45, rk4, euler, heun, midpoint, bs32"),
            opt("points", Int, "max points returned (default 200)"),
            opt("dt", Num, "initial step (default 0.01)"),
            opt("abstol", Num, "absolute tolerance (default 1e-6)"),
            opt("reltol", Num, "relative tolerance (default 1e-3)"),
            opt("dtmax", Num, "largest step (default min(0.1, t_end/50))"),
            opt("places", StrList, "series to keep: display names (S, CoffeeBeans.brown) or base place ids (default all)"),
        ],
        enabled: always,
        run: ode,
        journal: true,
    },
    CommandSpec {
        id: "sim.equilibrium",
        label: "Find Equilibrium",
        help: "Integrate the ODE from the current marking and rates until pflow-solver's equilibrium detector fires (|du/dt| below tolerance) or t_max runs out; stores a report.",
        menu: "Simulate",
        shortcut: None,
        args: &[
            opt("t_max", Num, "give up at this time (default 1000)"),
            opt("tolerance", Num, "largest |du/dt| counted as still (default 1e-6)"),
            opt("method", Str, "tsit5 (default), rk45, rk4, euler, heun, midpoint, bs32"),
        ],
        enabled: always,
        run: equilibrium,
        journal: true,
    },
];

/// The `dt` / `abstol` / `reltol` / `dtmax` overrides in a command's args.
pub(crate) fn ode_options(a: &Value) -> sim::OdeOptions {
    sim::OdeOptions {
        dt: n(a, "dt"),
        abstol: n(a, "abstol"),
        reltol: n(a, "reltol"),
        dtmax: n(a, "dtmax"),
    }
}

fn enabled(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(sess.enabled_transitions()))
}

fn fire(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    if sess.doc.transition(id).is_none() {
        return Err(Error::NotFound(format!("no transition {id:?}")));
    }
    sess.fire(id)?;
    Ok(json!({ "fired": id, "marking": sess.marking, "enabled": sess.enabled_transitions() }))
}

fn run_steps(sess: &mut Session, a: &Value) -> Result<Value> {
    let steps = n(a, "steps").unwrap_or(10.0).max(0.0) as usize;
    let mut x = (n(a, "seed").unwrap_or(1.0) as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    let mut fired = vec![];
    for _ in 0..steps {
        let en = sess.enabled_transitions();
        if en.is_empty() {
            break;
        }
        // xorshift64*: tiny, deterministic, no dependency.
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        let r = x.wrapping_mul(0x2545_F491_4F6C_DD1D);
        let t = en[(r % en.len() as u64) as usize].clone();
        sess.fire(&t)?;
        fired.push(t);
    }
    Ok(
        json!({ "fired": fired, "deadlocked": sess.enabled_transitions().is_empty(), "marking": sess.marking }),
    )
}

fn reset(sess: &mut Session, _: &Value) -> Result<Value> {
    sess.reset_marking();
    Ok(json!(sess.marking))
}

fn marking(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(sess.marking))
}

fn set_rate(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    let rate = n(a, "rate").unwrap();
    if sess.doc.transition(id).is_none() {
        return Err(Error::NotFound(format!("no transition {id:?}")));
    }
    if !(rate >= 0.0 && rate.is_finite()) {
        return Err(Error::BadArgs("rate must be a finite number ≥ 0".into()));
    }
    sess.rates.insert(id.to_string(), rate);
    Ok(json!({ "id": id, "rate": rate }))
}

fn ode(sess: &mut Session, a: &Value) -> Result<Value> {
    let t_end = n(a, "t_end").unwrap_or(10.0);
    let method = s(a, "method").unwrap_or("tsit5");
    let points = n(a, "points").unwrap_or(200.0).max(2.0) as usize;
    let mut r = sim::ode_with(
        &sess.doc,
        &sess.marking,
        &sess.rates,
        t_end,
        method,
        points,
        &ode_options(a),
    )?;
    if let Some(names) = strs(a, "places") {
        let order: Vec<(String, String)> = r
            .series
            .iter()
            .map(|(d, _)| (d.clone(), d.clone()))
            .collect();
        let mut keep = vec![false; order.len()];
        for name in &names {
            let idx = sim::select_series(&order, name);
            if idx.is_empty() {
                let known: Vec<&str> = order.iter().map(|(_, d)| d.as_str()).collect();
                return Err(Error::NotFound(format!(
                    "no place {name:?}; places are {known:?}"
                )));
            }
            for i in idx {
                keep[i] = true;
            }
        }
        let mut k = keep.into_iter();
        r.series.retain(|_| k.next().unwrap_or(false));
    }
    let initial = r.initial_state();
    let fin = r.final_state();
    let delta: BTreeMap<&String, f64> = fin.iter().map(|(k, v)| (k, v - initial[k])).collect();
    let data = ArtifactData::TimeSeries {
        x_label: "t".into(),
        x: r.t,
        series: r
            .series
            .into_iter()
            .map(|(name, values)| Series::new(name, values))
            .collect(),
    };
    let source = Step {
        command: "sim.ode".into(),
        args: a.clone(),
    };
    let id = sess.add_artifact(format!("ODE ({method}, t_end={t_end})"), source, data);
    let art = sess.artifact(&id).expect("just added");
    Ok(json!({
        "artifact": id,
        "initial": initial,
        "final": fin,
        "delta": delta,
        "data": art.data,
    }))
}

fn equilibrium(sess: &mut Session, a: &Value) -> Result<Value> {
    let t_max = n(a, "t_max").unwrap_or(1000.0);
    let method = s(a, "method").unwrap_or("tsit5");
    // pflow-solver's own default pairing (find_equilibrium): default
    // Options (dtmax 0.1) with the equilibrium detector. A larger dtmax lets
    // explicit steps hover around the fixed point instead of settling.
    let opts = sim::OdeOptions::default();
    let runner = Runner::new(&sess.doc, &sess.marking, &sess.rates, t_max, method, &opts)?;
    let eq = runner.equilibrium(n(a, "tolerance"))?;
    let report = serde_json::to_value(&eq).expect("serializable");
    let source = Step {
        command: "sim.equilibrium".into(),
        args: a.clone(),
    };
    let title = format!("Equilibrium (t_max={t_max})");
    let id = sess.add_artifact(title, source, ArtifactData::Report(report.clone()));
    let mut out = report;
    out["artifact"] = json!(id);
    Ok(out)
}

use serde_json::{json, Value};

use crate::artifact::{ArtifactData, Series};
use crate::registry::{always, arg, n, opt, req, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::sim;
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
        ],
        enabled: always,
        run: ode,
        journal: true,
    },
];

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
    let r = sim::ode(&sess.doc, &sess.marking, &sess.rates, t_end, method, points)?;
    let fin = r.final_state();
    let data = ArtifactData::TimeSeries {
        x_label: "t".into(),
        x: r.t,
        series: r
            .series
            .into_iter()
            .map(|(name, values)| Series {
                name,
                values,
                band: None,
            })
            .collect(),
    };
    let source = Step {
        command: "sim.ode".into(),
        args: a.clone(),
    };
    let id = sess.add_artifact(format!("ODE ({method}, t_end={t_end})"), source, data);
    let art = sess.artifact(&id).expect("just added");
    Ok(json!({ "artifact": id, "final": fin, "data": art.data }))
}

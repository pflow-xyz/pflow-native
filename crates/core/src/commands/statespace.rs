//! State-space analysis (phase 4): the reachability / coverability graph,
//! property verification (sim.pflow.xyz's `sim_verify` over pflow-verify),
//! reachability with a witness or a certificate (`sim_reach`), and the ODE
//! influence readings of `sim_diagnose`. The engines are in
//! [`crate::statespace`]; this file turns their answers into artifacts.
//!
//! Every search starts from the document's declared initial marking unless
//! `from_current` asks for the canvas marking, and reports markings in
//! document place ids with one count per color, so a trace replays onto the
//! canvas with `sim.goto`.

use serde_json::{json, Map, Value};

use crate::artifact::{ArtifactData, Bar, GraphEdge, GraphNode, TraceStep};
use crate::registry::{always, arg, b, n, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::sim::{self, OdeOptions};
use crate::statespace::{
    self as ss, KBounded, NoBecause, ReachAnswer, Space, DEFAULT_GRAPH_STATES,
    DEFAULT_REACH_STATES, MAX_GRAPH_STATES, MAX_REACH_STATES,
};
use crate::stochastic;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "analysis.reachability",
        label: "Reachability Graph",
        help: "Explore every marking reachable from the initial (or current) marking under the token game's firing rule. \
               When a marking strictly covers an ancestor through a pump of monotone transitions, the places that grew \
               become ω (Karp–Miller coverability), so an unbounded net still gives a finite graph. Stores a graph \
               (node per marking, edge per firing) and a report.",
        menu: "Analyze",
        shortcut: None,
        args: &[
            opt("max_states", Int, "markings to keep, 1..20000 (default 2000); beyond it the graph is truncated"),
            opt("from_current", Bool, "start from the canvas marking instead of the declared initial marking"),
        ],
        enabled: always,
        run: reachability,
        journal: true,
    },
    CommandSpec {
        id: "analysis.verify",
        label: "Verify Properties",
        help: "Check properties (sim_verify's JSON): deadlock-free, bounded (with bound k > 0: every place ≤ k), live, terminating, \
               reachable/unreachable {target}, invariant {expr}, mutual-exclusion {places, bound}, conserves. Each verdict \
               is proved / refuted / unknown with its method (structural holds for any initial marking; exhaustive for \
               this one; partial = truncated). Every refutation with a replayable counterexample stores a trace.",
        menu: "Analyze",
        shortcut: None,
        args: &[
            opt("properties", Json, "array of properties (or a JSON string of one); default bounded + deadlock-free"),
            opt("max_states", Int, "exhaustive-search budget, 1..100000 (default 20000)"),
            opt("from_current", Bool, "verify from the canvas marking instead of the declared initial marking"),
        ],
        enabled: always,
        run: verify,
        journal: true,
    },
    CommandSpec {
        id: "analysis.reach",
        label: "Reach Marking",
        help: "Is `to` reachable (sim_reach)? `to` constrains only the places it names, each to exactly that count. \
               yes: a shortest firing sequence, stored as a trace; no: a P-invariant whose weighted sum separates from \
               and to (valid whatever inhibitors, read arcs or capacities allow), or an exhaustive search; unknown: \
               the budget ran out (not a no).",
        menu: "Analyze",
        shortcut: None,
        args: &[
            arg("to", Json, "{place: n} or {place: [per color]} (null entries free); place.color names one color"),
            opt("from", Json, "sparse override on the start marking, same shape as to"),
            opt("from_current", Bool, "start from the canvas marking instead of the declared initial marking"),
            opt("max_states", Int, "search budget, 1..100000 (default 20000)"),
        ],
        enabled: always,
        run: reach,
        journal: true,
    },
    CommandSpec {
        id: "analysis.diagnose",
        label: "Diagnose (Influence)",
        help: "Which knobs move the outcome (sim_diagnose's influence reading, on the ODE): each transition's rate at \
               rate·(1 ± delta), and +1 initial token on each place, against the observable's value at t_end. Stores a \
               tornado chart ranked by |influence| and a report, with sim's SSA noise floor (spread across three seeds) \
               when `noise` is on.",
        menu: "Analyze",
        shortcut: None,
        args: &[
            opt("observable", Str, "place (display name, or base id summing colors); default: the sinks (produced, never consumed)"),
            opt("t_end", Num, "horizon (default 10)"),
            opt("delta", Num, "relative rate perturbation in (0, 1] (default 0.5, sim's ×1.5 probe)"),
            opt("noise", Bool, "estimate the SSA noise floor (default true)"),
            opt("seed", Int, "noise-floor seed (default 1)"),
            opt("from_current", Bool, "start from the canvas marking instead of the declared initial marking"),
        ],
        enabled: always,
        run: diagnose,
        journal: true,
    },
];

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

fn budget(a: &Value, default: usize, cap: usize) -> Result<usize> {
    let v = n(a, "max_states").unwrap_or(default as f64);
    if !(1.0..=cap as f64).contains(&v) {
        return Err(Error::BadArgs(format!(
            "max_states {v} is outside 1..{cap}"
        )));
    }
    Ok(v as usize)
}

/// The start marking: declared initial, or the canvas's.
fn start_marking(sess: &Session, a: &Value) -> sim::Marking {
    if b(a, "from_current").unwrap_or(false) {
        sess.marking.clone()
    } else {
        sim::initial_marking(&sess.doc)
    }
}

// ------------------------------------------------------------ reachability

fn reachability(sess: &mut Session, a: &Value) -> Result<Value> {
    let max = budget(a, DEFAULT_GRAPH_STATES, MAX_GRAPH_STATES)?;
    let space = Space::new(&sess.doc)?;
    let start = space.vector(&start_marking(sess, a));
    let initial_total: i64 = start.iter().sum();
    let ex = space.explore(start, max, true, true, None);

    let mut nodes = Vec::with_capacity(ex.states.len());
    let mut deadlocks = vec![];
    let mut terminals = 0;
    let mut omega_states = 0;
    for (i, v) in ex.states.iter().enumerate() {
        let terminal = ex.terminal[i];
        // pflow-reachability's rule: a terminal marking is a deadlock when
        // the run started with tokens and it is not empty.
        let deadlock = terminal && initial_total > 0 && v.iter().any(|&x| x != 0);
        let omega = space.omega_places(v);
        terminals += usize::from(terminal);
        if !omega.is_empty() {
            omega_states += 1;
        }
        if deadlock {
            deadlocks.push(format!("s{i}"));
        }
        nodes.push(GraphNode {
            id: format!("s{i}"),
            label: space.label(v),
            data: json!({
                "marking": space.doc_marking(v),
                "initial": i == 0,
                "terminal": terminal,
                "deadlock": deadlock,
                "expanded": ex.expanded[i],
                "omega": omega,
                "depth": ex.depth[i],
            }),
        });
    }
    let edges: Vec<GraphEdge> = ex
        .edges
        .iter()
        .map(|&(f, t, tr)| GraphEdge {
            source: format!("s{f}"),
            target: format!("s{t}"),
            label: space.transition_id(tr).to_string(),
            data: Value::Null,
        })
        .collect();
    let fired: std::collections::BTreeSet<&str> = ex
        .edges
        .iter()
        .map(|&(_, _, t)| space.transition_id(t))
        .collect();
    let never: Vec<&str> = space.transitions().filter(|t| !fired.contains(t)).collect();
    let bounded = if omega_states > 0 {
        json!(false)
    } else if ex.truncated {
        Value::Null
    } else {
        json!(true)
    };
    let mut max_tokens = Map::new();
    for p in 0..space.place_count() {
        let m = ex.states.iter().map(|v| v[p]).max().unwrap_or(0);
        max_tokens.insert(
            space.display(p).to_string(),
            if m >= ss::OMEGA / 2 {
                json!("ω")
            } else {
                json!(m)
            },
        );
    }
    let n_states = ex.states.len();
    let n_edges = edges.len();
    let graph_id = sess.add_artifact(
        "Reachability graph".into(),
        source("analysis.reachability", a),
        ArtifactData::Graph { nodes, edges },
    );
    let mut report = json!({
        "states": n_states,
        "edges": n_edges,
        "bounded": bounded,
        "coverability": omega_states > 0,
        "omega_states": omega_states,
        "deadlocks": deadlocks,
        "terminal_states": terminals,
        "truncated": ex.truncated,
        "max_states": max,
        "token_capped": ex.token_capped,
        "max_tokens": max_tokens,
        "never_fired": never,
        "complete": !ex.truncated && !ex.token_capped,
        "graph": graph_id,
        "notes": reach_notes(ex.truncated, omega_states > 0, never.is_empty()),
    });
    let report_id = sess.add_artifact(
        "Reachability report".into(),
        source("analysis.reachability", a),
        ArtifactData::Report(report.clone()),
    );
    report["report"] = json!(report_id);
    Ok(report)
}

fn reach_notes(truncated: bool, omega: bool, all_fired: bool) -> Vec<String> {
    let mut out = vec![
        "untimed interleaving under the token game's firing rule (read and inhibitor arcs, capacities); rates play no part".to_string(),
        "a deadlock is a terminal marking that is not empty, given the start had tokens (pflow-reachability's rule)".to_string(),
    ];
    if omega {
        out.push("ω marks a place a monotone pump grows without bound (Karp–Miller); ω nodes stand for infinitely many markings, so their edges over-approximate. Markings report ω as -1".into());
    }
    if truncated {
        out.push("truncated: deadlocks and never_fired are only what the explored part shows; bounded is unknown (null) unless a pump was found".into());
    } else if !all_fired {
        out.push(
            "never_fired transitions are dead from this start (the search was complete)".into(),
        );
    }
    out
}

// ------------------------------------------------------------------ verify

fn verify(sess: &mut Session, a: &Value) -> Result<Value> {
    let max = budget(a, DEFAULT_REACH_STATES, MAX_REACH_STATES)?;
    let space = Space::new(&sess.doc)?;
    let start = space.vector(&start_marking(sess, a));
    let raw = match a.get("properties") {
        None | Some(Value::Null) => json!([{"kind": "bounded"}, {"kind": "deadlock-free"}]),
        Some(v) => stochastic::json_arg(v, "properties")?,
    };
    let list = match raw {
        Value::Array(v) if !v.is_empty() => v,
        Value::Array(_) => return Err(Error::BadArgs("properties is an empty list".into())),
        obj @ Value::Object(_) => vec![obj],
        other => {
            return Err(Error::BadArgs(format!(
                "properties must be a JSON array of property objects, got {other}"
            )))
        }
    };
    let props = list
        .iter()
        .enumerate()
        .map(|(i, p)| {
            space
                .property(p)
                .map_err(|e| Error::BadArgs(format!("property {i}: {e}")))
        })
        .collect::<Result<Vec<_>>>()?;

    let verifier = pflow_verify::Verifier::new(&space.net.model)
        .with_initial_marking(space.model_marking(&start))
        .with_max_states(max);
    let mut verdicts = vec![];
    let mut traces: Vec<String> = vec![];
    let (mut proved, mut refuted, mut unknown) = (0, 0, 0);
    let mut exhaustive_ran = false;
    for (i, p) in props.into_iter().enumerate() {
        let mut out = match p.k_bound {
            Some(k) => k_bounded(&space, &start, k, max),
            None => {
                let v = verifier.check_one(p.property.clone());
                verdict_json(&space, &v)
            }
        };
        out["property"] = p.raw.clone();
        match out["status"].as_str() {
            Some("proved") => proved += 1,
            Some("refuted") => refuted += 1,
            _ => unknown += 1,
        }
        exhaustive_ran |= matches!(out["method"].as_str(), Some("exhaustive" | "partial"));
        // A counterexample trace from the start becomes a trace artifact.
        if let Some(trace) = out["counterexample"]["trace"].as_array().cloned() {
            let ids: Vec<String> = trace
                .iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect();
            match space.trace_ids(&start, &ids) {
                Ok(steps) => {
                    let title = format!(
                        "Counterexample {i}: {}",
                        if p.property.name.is_empty() {
                            p.kind.clone()
                        } else {
                            p.property.name.clone()
                        }
                    );
                    let id = sess.add_artifact(
                        title,
                        source("analysis.verify", a),
                        ArtifactData::Trace { steps },
                    );
                    out["trace"] = json!(id);
                    traces.push(id);
                }
                Err(e) => {
                    out["trace_note"] = json!(format!(
                        "the counterexample's firing sequence does not replay from the start marking ({e}); no trace stored"
                    ));
                }
            }
        }
        verdicts.push(out);
    }
    let (state_count, truncated) = if exhaustive_ran {
        let ex = space.explore(start.clone(), max, false, false, None);
        (ex.states.len(), ex.truncated || ex.token_capped)
    } else {
        (0, false)
    };
    let mut summary = format!("{proved} proved, {refuted} refuted, {unknown} unknown");
    if truncated {
        summary.push_str(&format!(" (state space truncated at {state_count} states)"));
    }
    let report = json!({
        "verdicts": verdicts,
        "proved": proved,
        "refuted": refuted,
        "unknown": unknown,
        "ok": refuted == 0 && unknown == 0,
        "state_count": state_count,
        "truncated": truncated,
        "summary": summary,
        "traces": traces,
        "caveats": verify_caveats(&space),
    });
    let id = sess.add_artifact(
        "Verification".into(),
        source("analysis.verify", a),
        ArtifactData::Report(report.clone()),
    );
    let mut out = report;
    out["report"] = json!(id);
    Ok(out)
}

fn verdict_json(space: &Space, v: &pflow_verify::Verdict) -> Value {
    let mut out = json!({
        "status": ss::status_str(v.status),
        "method": ss::method_str(v.method),
        "detail": v.detail,
    });
    if !v.evidence.is_empty() {
        out["evidence"] = json!(v.evidence);
    }
    if let Some(c) = &v.counterexample {
        let vec: Vec<i64> = space
            .net
            .places
            .iter()
            .map(|p| c.marking.get(p).copied().unwrap_or(0))
            .collect();
        out["counterexample"] = json!({
            "trace": c.trace,
            "marking": space.doc_marking(&vec),
            "explanation": c.explanation,
        });
    }
    out
}

fn k_bounded(space: &Space, start: &[i64], k: i64, max: usize) -> Value {
    match space.check_k_bounded(start, k, max) {
        KBounded::Proved {
            structural: true,
            evidence,
            ..
        } => json!({
            "status": "proved", "method": "structural",
            "detail": format!("every place is held at or under {k} by a P-invariant"),
            "evidence": evidence,
        }),
        KBounded::Proved { states, .. } => json!({
            "status": "proved", "method": "exhaustive",
            "detail": format!("every place holds at most {k} tokens across all {states} reachable states"),
        }),
        KBounded::Refuted {
            path,
            marking,
            place,
            partial,
        } => json!({
            "status": "refuted",
            "method": if partial { "partial" } else { "exhaustive" },
            "detail": format!("{place} exceeds {k} after {} firing(s)", path.len()),
            "counterexample": {
                "trace": path.iter().map(|&t| space.transition_id(t)).collect::<Vec<_>>(),
                "marking": space.doc_marking(&marking),
                "explanation": format!("{place} holds more than {k} tokens in this marking (shortest such trace)"),
            },
        }),
        KBounded::Unknown { states } => json!({
            "status": "unknown", "method": "partial",
            "detail": format!("no place exceeded {k} in the {states} states explored, but the state space was truncated"),
        }),
    }
}

fn verify_caveats(space: &Space) -> Vec<String> {
    let mut out = vec![
        "untimed: each firing is one atomic step under the token game's rule; rates play no part".to_string(),
        "live means every transition can fire from some reachable marking (L1), not L4; it does not imply deadlock-free".into(),
    ];
    if space.net.colors.is_some() {
        out.push("colored net: properties are checked on the unfolded net; a document place id in an expression, target or places list stands for the sum (target: the per-color list) of its colors".into());
    }
    let gated = space.net.gating();
    if !gated.is_empty() {
        out.push(format!(
            "gated firings ({}): a structural (P-invariant) proof still holds, since gates only remove firings; an unbounded witness never pumps through an inhibitor or a capacity",
            gated.join("; ")
        ));
    }
    out
}

// ------------------------------------------------------------------- reach

fn reach(sess: &mut Session, a: &Value) -> Result<Value> {
    let max = budget(a, DEFAULT_REACH_STATES, MAX_REACH_STATES)?;
    let space = Space::new(&sess.doc)?;
    let mut from = space.vector(&start_marking(sess, a));
    if let Some(f) = a.get("from").filter(|v| !v.is_null()) {
        for (p, n) in space.partial(f, "from")? {
            from[p] = n;
        }
    }
    let to = space.partial(a.get("to").expect("validated"), "to")?;
    if to.is_empty() {
        return Err(Error::BadArgs(
            "to names no place; name the places to constrain, e.g. {\"done\": 1}".into(),
        ));
    }
    let mut out = json!({
        "definition": "reach-v1",
        "from": space.doc_marking(&from),
        "to": space.constraints_json(&to),
        "assumptions": [
            "reachability here is untimed interleaving under the token game's firing rule: each firing is one atomic step, and rates play no part",
        ],
    });
    match space.reach(&from, &to, max) {
        ReachAnswer::Yes { path, states } => {
            let steps: Vec<TraceStep> = space.trace(&from, &path)?;
            let reached = steps.last().map(|s| s.marking.clone()).unwrap_or_default();
            let ids: Vec<&str> = path.iter().map(|&t| space.transition_id(t)).collect();
            out["answer"] = json!("yes");
            out["method"] = json!("trace");
            out["witness"] = json!({"kind": "trace", "trace": {"steps": ids, "reached": reached}});
            out["reads"] = json!(if path.is_empty() {
                "from already matches to, so the empty trace reaches it".to_string()
            } else {
                format!(
                    "the {}-step trace is a shortest firing sequence from from to a marking matching to",
                    path.len()
                )
            });
            out["search"] = json!({"states": states, "max_states": max});
            let id = sess.add_artifact(
                format!("Reach witness ({} steps)", path.len()),
                source("analysis.reach", a),
                ArtifactData::Trace { steps },
            );
            out["trace"] = json!(id);
            out["artifact"] = json!(id);
        }
        ReachAnswer::No(NoBecause::Invariant(c)) => {
            let w = space.certificate_json(&c);
            let reads = if c.free.is_empty() {
                format!(
                    "{} is invariant under every firing; it is {} at from but {} at to, so to is unreachable",
                    w["expression"].as_str().unwrap_or(""),
                    c.at_from,
                    c.at_target
                )
            } else {
                format!(
                    "{} is invariant under every firing and {} at from; to's named places alone already sum to {}, and the free places can only add, so to is unreachable",
                    w["expression"].as_str().unwrap_or(""),
                    c.at_from,
                    c.at_target
                )
            };
            out["answer"] = json!("no");
            out["method"] = json!("invariant");
            out["witness"] = json!({"kind": "invariant", "invariant": w});
            out["reads"] = json!(reads);
        }
        ReachAnswer::No(NoBecause::Exhaustive { states }) => {
            out["answer"] = json!("no");
            out["method"] = json!("exhaustive");
            out["witness"] = json!({"kind": "exhaustive", "exhaustive": {"states": states}});
            out["reads"] = json!(format!(
                "every one of the {states} markings reachable from from was searched and none matches to"
            ));
        }
        ReachAnswer::Unknown { reasons, states } => {
            out["answer"] = json!("unknown");
            out["unknownBecause"] = json!(reasons
                .iter()
                .map(|(code, detail)| json!({"code": code, "detail": detail}))
                .collect::<Vec<_>>());
            out["reads"] = json!("neither a separating invariant nor a trace was found, and the search could not cover every marking; this is open, not a no");
            out["search"] = json!({"states": states, "max_states": max});
        }
    }
    if out["answer"] != "yes" {
        let id = sess.add_artifact(
            "Reach answer".into(),
            source("analysis.reach", a),
            ArtifactData::Report(out.clone()),
        );
        out["artifact"] = json!(id);
    }
    Ok(out)
}

// ---------------------------------------------------------------- diagnose

fn diagnose(sess: &mut Session, a: &Value) -> Result<Value> {
    let t_end = n(a, "t_end").unwrap_or(10.0);
    sim::check_t_end(t_end)?;
    let delta = n(a, "delta").unwrap_or(0.5);
    ss::check_delta(delta)?;
    let mk = start_marking(sess, a);
    let space = Space::new(&sess.doc)?;
    let observable: Vec<String> = match s(a, "observable") {
        Some(o) => vec![o.to_string()],
        None => {
            let sinks = ss::sinks(&space);
            if sinks.is_empty() {
                return Err(Error::BadArgs(
                    "the net has no sink place (produced, never consumed) to read; name an observable".into(),
                ));
            }
            sinks
        }
    };
    let obs_name = observable.join(" + ");
    let opts = OdeOptions::grid(t_end);
    let d = ss::diagnose(
        &sess.doc,
        &mk,
        &sess.rates,
        &observable,
        t_end,
        delta,
        &opts,
    )?;
    let seed = n(a, "seed").unwrap_or(1.0);
    if !(seed >= 0.0 && seed.fract() == 0.0) {
        return Err(Error::BadArgs(format!(
            "seed must be a whole number ≥ 0, got {seed}"
        )));
    }
    let noise = if b(a, "noise").unwrap_or(true) {
        match ss::noise_floor(&sess.doc, &mk, &sess.rates, &observable, t_end, seed as u64) {
            Ok((floor, values)) => {
                json!({"floor": floor, "values": values, "seeds": ([1u64, 978, 1955].map(|k| seed as u64 + k))})
            }
            Err(e) => json!({"floor": null, "why": e.to_string()}),
        }
    } else {
        Value::Null
    };
    let floor = noise["floor"].as_f64();
    let bars: Vec<Bar> = d
        .knobs
        .iter()
        .map(|k| Bar {
            name: k.name.clone(),
            value: d.baseline,
            lo: k.lo,
            hi: Some(k.hi),
        })
        .collect();
    let knobs: Vec<Value> = d
        .knobs
        .iter()
        .map(|k| {
            let mut v = json!({
                "name": k.name, "kind": k.kind, "id": k.id, "base": k.base,
                "lo": k.lo, "hi": k.hi, "influence": k.influence,
            });
            if let Some(f) = floor {
                v["inert"] = json!(k.influence.abs() <= f);
            }
            v
        })
        .collect();
    let binds = floor.map(|f| d.knobs.iter().any(|k| k.influence.abs() > f));
    let mut findings = vec![json!({
        "check": "dormant-sources",
        "ok": d.dormant.is_empty(),
        "detail": if d.dormant.is_empty() {
            "every transition has a positive rate".to_string()
        } else {
            format!("rate 0 (never fires in the ODE): {}", d.dormant.join(", "))
        },
    })];
    findings.push(match binds {
        Some(true) => json!({"check": "something-binds", "ok": true,
            "detail": format!("{} beats the noise floor", d.knobs[0].name)}),
        Some(false) => json!({"check": "something-binds", "ok": false,
            "detail": "no knob moves the observable beyond the SSA noise floor"}),
        None => json!({"check": "something-binds", "ok": true, "na": true,
            "detail": "no noise floor was measured"}),
    });
    let bars_id = sess.add_artifact(
        format!("Influence on {obs_name}"),
        source("analysis.diagnose", a),
        ArtifactData::Bars {
            value_label: format!("{obs_name} at t={}", t_end),
            bars,
        },
    );
    let report = json!({
        "observable": d.observable,
        "t_end": t_end,
        "delta": delta,
        "baseline": d.baseline,
        "knobs": knobs,
        "noise": noise,
        "findings": findings,
        "dormant": d.dormant,
        "bars": bars_id,
        "reads": "influence is a finite difference of the observable's ODE value at t_end: hi − lo for a rate probed at rate·(1 ± delta), hi − baseline for one more initial token; knobs are ranked by |influence|, and a knob whose |influence| is within the SSA noise floor is inert",
        "omitted": [
            "sim's knob inference (pool/source/patience/parameter kinds) and its loss/success outcome scoring: here every transition's rate and every place's +1 token is a knob, read on one observable",
            "SSA-measured influence at several operating points and greedy relief rounds: influence is measured once, on the ODE, at the current rates",
            "the knee sweep, fungible classes, conservation and structural-deadlock findings, CTMC and constrained lumpability",
        ],
    });
    let report_id = sess.add_artifact(
        format!("Diagnosis of {obs_name}"),
        source("analysis.diagnose", a),
        ArtifactData::Report(report.clone()),
    );
    let mut out = report;
    out["report"] = json!(report_id);
    Ok(out)
}

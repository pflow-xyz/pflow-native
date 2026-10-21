//! Conformance and mining commands over event-log artifacts: whole-log
//! token replay (`data.conformance`, sim_conformance), online replay with
//! the first divergence (`data.replay`, sim_conformance_stream), synthetic
//! logs from the net (`data.generate`, sim_dataset) and discovery
//! (`data.discover`). The engines live in [`crate::conformance`] and
//! [`crate::mining`]; activities map to transitions by
//! [`crate::conformance::ActivityMap`] (id, label, then either ignoring case).

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::artifact::{Artifact, ArtifactData, Series};
use crate::conformance::{self, StreamEvent};
use crate::eventlog;
use crate::mining;
use crate::registry::{always, arg, n, opt, req, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::stochastic as st;
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "data.conformance",
        label: "Check Conformance…",
        help: "Token-replay conformance of an event-log artifact against the current net (sim_conformance): \
               fitness, precision (ETC), generalization, simplicity, a residual block, and per-variant diagnostics. \
               Each case replays from the initial marking; activities match transitions by id, then label, then \
               either ignoring case. Stores a report, a per-variant table and a sets artifact of the places and \
               transitions where tokens were missing or left over (show one on the net with artifact.show_set).",
        menu: "Data",
        shortcut: None,
        args: &[
            arg("log", Str, "event-log artifact id (data.load_log / data.generate)"),
            opt("max_traces", Int, "replay only the first N cases, in case-id order (≤ 100000)"),
        ],
        enabled: always,
        run: conformance_cmd,
        journal: true,
    },
    CommandSpec {
        id: "data.replay",
        label: "Replay Event Stream…",
        help: "Online conformance (sim_conformance_stream): replay an ordered event stream — a log artifact in row \
               order, or inline `events` [{case, activity, timestamp?}] — case by case from the initial marking, \
               force-firing divergent events so cases resynchronise. Reports the first divergence (blocking \
               places, marking, the shortest repair of unobserved transitions) and stores a running-fitness time \
               series (x = event index) and the replay of one case as a trace (step 0 = initial marking; \
               sim.goto replays it): `case`, else the first divergent case, else the first case.",
        menu: "Data",
        shortcut: None,
        args: &[
            opt("log", Str, "event-log artifact id; rows are taken in table order"),
            opt("events", Json, "[{\"case\": .., \"activity\": .., \"timestamp\": ..}] in stream order (or a string holding it)"),
            opt("case", Str, "replay only this case (and trace it)"),
            opt("repair_depth", Int, "most unobserved transitions a repair may use (default 4, ≤ 64; ≤ 0 turns repair off)"),
        ],
        enabled: always,
        run: replay_cmd,
        journal: true,
    },
    CommandSpec {
        id: "data.generate",
        label: "Generate Event Log…",
        help: "A synthetic event log from the current net (sim_dataset): `cases` seeded SSA playouts from the \
               initial marking, each until deadlock or `max_steps` firings; one event per firing (activity = \
               transition id). Time starts at 2026-01-05T08:00:00Z (sim's epoch), one model time unit = one hour, \
               each case starting where the previous ended. Deterministic for a seed. Stores an event-log table \
               (export.artifact as CSV; feeds data.conformance and data.discover).",
        menu: "Data",
        shortcut: None,
        args: &[
            opt("cases", Int, "playouts (default 200, ≤ 2000)"),
            opt("seed", Int, "RNG seed (default 1); case k uses seed + k - 1"),
            opt("max_steps", Int, "firings per case (default 200, ≤ 10000; cases × max_steps ≤ 1000000)"),
            opt("rates", Json, "{transition: rate} over the session rates (0 disables)"),
        ],
        enabled: always,
        run: generate_cmd,
        journal: true,
    },
    CommandSpec {
        id: "data.discover",
        label: "Discover Net from Log…",
        help: "Process discovery with pflow-mining: REPLACES the current net with the mined one (one undo step \
               restores it), laid out with Sugiyama. method: alpha (≤ 12 activities), heuristic (threshold = \
               dependency threshold, default 0.5), sequential or common-path. Returns the net's size and the \
               log's conformance against it; the log artifact is kept (under a new id, returned as `log`).",
        menu: "Data",
        shortcut: None,
        args: &[
            arg("log", Str, "event-log artifact id"),
            opt("method", Str, "alpha | heuristic | sequential | common-path (default alpha)"),
            opt("threshold", Num, "heuristic dependency threshold in [0, 1]"),
        ],
        enabled: always,
        run: discover_cmd,
        journal: true,
    },
];

fn source(command: &str, a: &Value) -> Step {
    Step {
        command: command.into(),
        args: a.clone(),
    }
}

fn log_artifact<'s>(sess: &'s Session, id: &str) -> Result<&'s Artifact> {
    sess.artifact(id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))
}

fn whole(a: &Value, k: &str) -> Option<i64> {
    n(a, k).map(|x| x as i64)
}

fn conformance_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "log");
    let log = eventlog::from_artifact(log_artifact(sess, id)?)?;
    let max = match whole(a, "max_traces") {
        Some(x) if x < 1 => return Err(Error::BadArgs(format!("max_traces must be ≥ 1, got {x}"))),
        x => x.map(|x| x as usize),
    };
    let c = conformance::check(&sess.doc, &log, max)?;
    let src = source("data.conformance", a);
    let report = sess.add_artifact(
        format!("Conformance of {id}"),
        src.clone(),
        ArtifactData::Report(c.report.clone()),
    );
    let table = sess.add_artifact(
        format!("Conformance of {id}: variants"),
        src.clone(),
        c.table,
    );
    let sets = sess.add_artifact(
        format!("Conformance of {id}: residual on the net"),
        src,
        c.sets,
    );
    let mut out = c.report;
    out["artifacts"] = json!({"report": report, "table": table, "sets": sets});
    Ok(out)
}

/// A log table's rows as a stream, in row order (not re-sorted): `case`
/// and `activity` columns required, `time` optional.
fn stream_from_table(art: &Artifact) -> Result<Vec<StreamEvent>> {
    let ArtifactData::Table { columns, rows } = &art.data else {
        return Err(Error::BadArgs(format!(
            "{} is a {}, not an event-log table",
            art.id,
            art.data.kind()
        )));
    };
    let col = |name: &str| columns.iter().position(|c| c == name);
    let (Some(ci), Some(ai)) = (col("case"), col("activity")) else {
        return Err(Error::BadArgs(format!(
            "{} has columns {columns:?}; an event log needs \"case\" and \"activity\"",
            art.id
        )));
    };
    let ti = col("time");
    Ok(rows
        .iter()
        .map(|r| StreamEvent {
            case: text(&r[ci]),
            activity: text(&r[ai]),
            timestamp: ti.map(|i| text(&r[i])).filter(|t| !t.is_empty()),
        })
        .collect())
}

fn text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn stream_from_json(v: &Value) -> Result<Vec<StreamEvent>> {
    let v = st::json_arg(v, "events")?;
    let arr = v
        .as_array()
        .ok_or_else(|| Error::BadArgs("events must be a JSON array of {case, activity}".into()))?;
    arr.iter()
        .enumerate()
        .map(|(i, e)| {
            let o = e
                .as_object()
                .ok_or_else(|| Error::BadArgs(format!("event {i} is not an object")))?;
            if let Some(k) = o
                .keys()
                .find(|k| !["case", "activity", "timestamp"].contains(&k.as_str()))
            {
                return Err(Error::BadArgs(format!(
                    "event {i}: unknown field {k:?}; takes case, activity, timestamp"
                )));
            }
            let get = |k: &str| o.get(k).map(text).unwrap_or_default();
            Ok(StreamEvent {
                case: get("case"),
                activity: get("activity"),
                timestamp: o.get("timestamp").map(text).filter(|t| !t.is_empty()),
            })
        })
        .collect()
}

fn replay_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let mut events = match (s(a, "log"), a.get("events").filter(|v| !v.is_null())) {
        (Some(id), None) => stream_from_table(log_artifact(sess, id)?)?,
        (None, Some(v)) => stream_from_json(v)?,
        _ => {
            return Err(Error::BadArgs(
                "give the stream as exactly one of `log` (artifact id) or `events`".into(),
            ))
        }
    };
    let case = s(a, "case");
    if let Some(c) = case {
        events.retain(|e| e.case == c);
        if events.is_empty() {
            return Err(Error::NotFound(format!("no events for case {c:?}")));
        }
    }
    let depth = whole(a, "repair_depth").unwrap_or(conformance::DEFAULT_REPAIR_DEPTH);
    let out = conformance::stream(&sess.doc, &events, case, depth)?;
    let src = source("data.replay", a);
    let what = s(a, "log").unwrap_or("events");
    let trace = sess.add_artifact(
        format!("Replay of case {}", out.case),
        src.clone(),
        ArtifactData::Trace { steps: out.trace },
    );
    let (fit, tok): (Vec<f64>, Vec<f64>) = out.running.iter().copied().unzip();
    let series = sess.add_artifact(
        format!("Running fitness of {what}"),
        src.clone(),
        ArtifactData::TimeSeries {
            x_label: "event".into(),
            x: (0..fit.len()).map(|i| i as f64).collect(),
            series: vec![
                Series::new("fitness", fit),
                Series::new("token_fitness", tok),
            ],
        },
    );
    let report = sess.add_artifact(
        format!("Stream conformance of {what}"),
        src,
        ArtifactData::Report(out.report.clone()),
    );
    let mut r = out.report;
    r["artifacts"] = json!({"trace": trace, "fitness": series, "report": report});
    Ok(r)
}

fn generate_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let cases = whole(a, "cases").unwrap_or(200);
    let max_steps = whole(a, "max_steps").unwrap_or(mining::DEFAULT_MAX_STEPS as i64);
    if cases < 1 || max_steps < 1 {
        return Err(Error::BadArgs("cases and max_steps must be ≥ 1".into()));
    }
    let seed = match a.get("seed").filter(|v| !v.is_null()) {
        Some(v) => st::parse_seed(v, "seed")?,
        None => 1,
    };
    let mut rates = sess.rates.clone();
    if let Some(v) = a.get("rates").filter(|v| !v.is_null()) {
        let v = st::json_arg(v, "rates")?;
        let m = v
            .as_object()
            .ok_or_else(|| Error::BadArgs("rates must be an object {transition: rate}".into()))?;
        for (t, r) in m {
            if sess.doc.transition(t).is_none() {
                return Err(Error::NotFound(format!("rates: no transition {t:?}")));
            }
            match r.as_f64() {
                Some(x) if x >= 0.0 && x.is_finite() => {
                    rates.insert(t.clone(), x);
                }
                _ => {
                    return Err(Error::BadArgs(format!(
                        "rates: {t:?} needs a finite rate ≥ 0, got {r}"
                    )))
                }
            }
        }
    }
    let g = mining::generate(&sess.doc, &rates, cases as usize, seed, max_steps as usize)?;
    let summary = eventlog::summary(&g.log);
    let id = sess.add_artifact(
        format!("Generated log ({cases} cases, seed {seed})"),
        source("data.generate", a),
        eventlog::to_table(&g.log),
    );
    Ok(json!({
        "artifact": id,
        "summary": summary,
        "cases": cases,
        "events": g.events,
        "seed": seed,
        "deadlocked": g.deadlocked,
        "capped": g.capped,
        "model_time": g.model_time,
        "epoch_ms": mining::EPOCH_MS,
        "time_unit_ms": mining::TIME_UNIT_MS,
        "export": {"command": "export.artifact", "args": {"id": id, "format": "csv"}},
    }))
}

fn discover_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "log").to_string();
    let art = log_artifact(sess, &id)?.clone();
    let log = eventlog::from_artifact(&art)?;
    let method = s(a, "method").unwrap_or("alpha").to_string();
    let (doc, info) = mining::discover(&log, &method, n(a, "threshold"))?;
    let c = conformance::check(&doc, &log, None)?;
    super::file::replace(sess, doc)?;
    // A mined net is a new document: don't let Save overwrite the file the
    // previous one came from.
    sess.path = None;
    // Replacing the document clears the results; the log is still the
    // input of whatever comes next, so it is kept.
    let log_id = sess.add_artifact(art.title.clone(), art.source.clone(), art.data.clone());
    let r = &c.report;
    let mut summary: BTreeMap<&str, Value> = BTreeMap::new();
    for k in [
        "fitness",
        "precision",
        "generalization",
        "simplicity",
        "f_score",
        "fitting_percent",
        "fitting_traces",
        "total_traces",
    ] {
        summary.insert(k, r[k].clone());
    }
    let out = json!({
        "method": method,
        "places": sess.doc.places.len(),
        "transitions": sess.doc.transitions.len(),
        "arcs": sess.doc.arcs.len(),
        "miner": info,
        "conformance": summary,
        "log": log_id,
        "replaced": "the current net was replaced by the mined one; edit.undo restores it",
    });
    let report = sess.add_artifact(
        format!("Discovery ({method}) of {id}"),
        source("data.discover", a),
        ArtifactData::Report(out.clone()),
    );
    let mut out = out;
    out["report"] = json!(report);
    Ok(out)
}

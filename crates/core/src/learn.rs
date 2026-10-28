//! Learning from data: rates from an event log ([`calibrate`], sim.pflow.xyz's
//! `Calibrate`), ODE rate fitting to observed series ([`fit`], pflow-learn,
//! the engine under pflow.xyz's `fitRates`), and distillation of exact game
//! search into the play scorer ([`distill`], sim's `Distill`).
//!
//! Everything here is deterministic for its inputs (seeded where random)
//! and bounded by the `MAX_*` caps below; over a cap is a refusal, never a
//! clamp.
//!
//! **Activity → transition mapping** (calibrate): an activity names a
//! transition when it equals the transition's id exactly; otherwise when it
//! equals exactly one transition's id or label ignoring case and surrounding
//! whitespace. An activity that matches several transitions that way is
//! ambiguous and, like one that matches none, stays unmapped (reported, and
//! counted as a missing activity by the conformance replay).

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use pflow_eventlog::{Event, EventLog};
use pflow_learn::optim::nelder_mead::{self, SimplexMethod, SimplexSettings};
use pflow_learn::ratefunc::{ConstantRateFunc, RateFunc, ScalarRateFunc};
use pflow_learn::{Dataset, FitMethod, FitOptions, LearnableProblem, SharedRateFunc};
use pflow_metamodel::Model;
use serde::Serialize;
use serde_json::{json, Value};

use crate::artifact::ArtifactData;
use crate::doc::Document;
use crate::sim::{self, Compiled, Marking, OdeOptions};
use crate::stochastic::{self as st, Net, Objective, Rollout};
use crate::sweep::Rng;
use crate::{Error, Result};

// ------------------------------------------------------------ caps

/// Events one calibration reads.
pub const MAX_LOG_EVENTS: usize = 500_000;
/// Observation times one fit compares against.
pub const MAX_FIT_POINTS: usize = 10_000;
/// Rates one fit adjusts.
pub const MAX_FIT_PARAMS: usize = 32;
/// Optimizer iterations one fit may take.
pub const MAX_FIT_ITERS: usize = 5_000;

/// sim's Distill caps and defaults (`distill.go`).
pub const DISTILL_DEFAULT_GAMES: usize = 20;
pub const DISTILL_DEFAULT_POSITIONS: usize = 40;
pub const DISTILL_DEFAULT_ITERS: usize = 40;
pub const DISTILL_DEFAULT_REALIZATIONS: usize = 40;
pub const DISTILL_DEFAULT_HORIZON: f64 = 3.0;
pub const DISTILL_DEFAULT_SEED: u64 = 11;
pub const MAX_DISTILL_GAMES: usize = 100;
pub const MAX_DISTILL_POSITIONS: usize = 100;
pub const MAX_DISTILL_ITERS: usize = 100;
pub const MAX_DISTILL_GROUPS: usize = 16;
/// Exact-search positions expanded per distillation (all labels share one
/// table), and the ply cap of one line.
pub const MAX_SEARCH_NODES: usize = 1_000_000;
pub const MAX_SEARCH_DEPTH: usize = 64;

// ------------------------------------------------------------ activities

/// How a log's activities name the net's transitions.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ActivityMap {
    /// activity → transition id.
    pub mapped: BTreeMap<String, String>,
    /// Activities that match no transition.
    pub unmapped: Vec<String>,
    /// Activities that match several transitions ignoring case.
    pub ambiguous: BTreeMap<String, Vec<String>>,
}

impl ActivityMap {
    pub fn transition(&self, activity: &str) -> Option<&str> {
        self.mapped.get(activity).map(String::as_str)
    }
}

/// Maps each activity to a transition (see the module docs for the rule).
pub fn map_activities(doc: &Document, activities: &[String]) -> ActivityMap {
    let mut out = ActivityMap::default();
    for a in activities {
        if doc.transition(a).is_some() {
            out.mapped.insert(a.clone(), a.clone());
            continue;
        }
        let key = a.trim().to_lowercase();
        let hits: Vec<String> = doc
            .transitions
            .iter()
            .filter(|t| {
                t.id.to_lowercase() == key
                    || t.label
                        .as_deref()
                        .is_some_and(|l| l.trim().to_lowercase() == key)
            })
            .map(|t| t.id.clone())
            .collect();
        match hits.as_slice() {
            [one] => {
                out.mapped.insert(a.clone(), one.clone());
            }
            [] => out.unmapped.push(a.clone()),
            _ => {
                out.ambiguous.insert(a.clone(), hits);
            }
        }
    }
    out
}

/// Seconds in one model time unit named by `unit`.
pub fn unit_seconds(unit: &str) -> Result<f64> {
    Ok(match unit {
        "ms" | "millisecond" | "milliseconds" => 0.001,
        "s" | "sec" | "second" | "seconds" => 1.0,
        "min" | "minute" | "minutes" => 60.0,
        "h" | "hour" | "hours" => 3600.0,
        "day" | "days" => 86_400.0,
        other => {
            return Err(Error::BadArgs(format!(
                "unit {other:?}: one of ms, s, min, hour, day"
            )))
        }
    })
}

// ------------------------------------------------------------ calibrate

/// What stands behind one learned rate (sim's `RateEvidence`, plus the mean
/// gap in model time units).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evidence {
    /// Gaps the estimate rests on: inter-arrival gaps for a source, gaps
    /// before this transition's own events otherwise.
    pub n: usize,
    /// Mean of those gaps, in the calibration's unit (absent when n = 0).
    pub mean_gap: Option<f64>,
    /// Sample standard deviation (n−1) over the mean; 0 when n < 2.
    pub gap_cv: f64,
    /// n < 2: no rate at n = 0, no spread at n = 1.
    pub insufficient: bool,
}

fn evidence(n: usize, sum: f64, sq: f64, unit_s: f64) -> Evidence {
    let mut ev = Evidence {
        n,
        mean_gap: (n > 0).then(|| sum / n as f64 / unit_s),
        gap_cv: 0.0,
        insufficient: n < 2,
    };
    if n >= 2 && sum > 0.0 {
        let mean = sum / n as f64;
        let var = (sq - n as f64 * mean * mean) / (n - 1) as f64;
        if var > 0.0 {
            ev.gap_cv = var.sqrt() / mean;
        }
    }
    ev
}

/// One transition's line of a calibration.
#[derive(Debug, Clone, Serialize)]
pub struct RateRow {
    pub transition: String,
    /// Log activities mapped to it.
    pub activities: Vec<String>,
    /// "source" (no input place: rate from case inter-arrivals), "service"
    /// (rate from the gaps before its events, race-corrected), "pickup"
    /// (declared rate ≥ 100: kept), or "unobserved" (not in the log: kept).
    pub kind: &'static str,
    pub evidence: Option<Evidence>,
    /// The learned rate, when there is one.
    pub rate: Option<f64>,
    /// The session rate before calibration (absent = 1).
    pub previous: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Calibration {
    /// Learned rates by transition id, per `unit`.
    pub rates: BTreeMap<String, f64>,
    pub rows: Vec<RateRow>,
    pub unobserved: Vec<String>,
    pub caveats: Vec<String>,
    pub mapping: ActivityMap,
    pub unit: String,
    /// Events read (after the lifecycle filter).
    pub events: usize,
}

/// The log with only `complete` events when it marks lifecycles at all
/// (an event is a firing), and whether it filtered anything.
fn completions(log: &EventLog) -> (EventLog, bool) {
    let marks = log.cases.values().any(|t| {
        t.events
            .iter()
            .any(|e| e.lifecycle.eq_ignore_ascii_case("complete"))
    });
    if !marks {
        return (log.clone(), false);
    }
    let mut out = EventLog::new();
    for t in log.traces() {
        for e in &t.events {
            if e.lifecycle.eq_ignore_ascii_case("complete") {
                out.add_event(e.clone());
            }
        }
    }
    out.sort_traces();
    (out, true)
}

/// Learns per-transition rates from `log`, after sim.pflow.xyz's
/// `Calibrate`: every event is a firing; a source transition (no input
/// place) gets the case inter-arrival rate, split across sources by the
/// cases each starts; any other transition gets the rate of the gaps
/// before its events, pooled with the transitions racing for the same
/// input places and split by completion share (exact for exponential
/// races); a transition at a declared (session) rate ≥ 100 is an instant
/// pickup and keeps it; one the log never mentions keeps its rate.
/// Rates are per `unit` (sim: per hour).
pub fn calibrate(
    doc: &Document,
    rates: &BTreeMap<String, f64>,
    log: &EventLog,
    unit: &str,
) -> Result<Calibration> {
    let unit_s = unit_seconds(unit)?;
    if log.num_events() > MAX_LOG_EVENTS {
        return Err(Error::BadArgs(format!(
            "the log has {} events, over the {MAX_LOG_EVENTS} cap",
            log.num_events()
        )));
    }
    let (log, filtered) = completions(log);
    if log.num_cases() == 0 {
        return Err(Error::Invalid("the event log contains no traces".into()));
    }
    let mapping = map_activities(doc, &log.activities());
    if mapping.mapped.is_empty() {
        let sample: Vec<&String> = log_activities_sample(&mapping);
        return Err(Error::Invalid(format!(
            "no log activity matches a transition id or label — the log speaks a different vocabulary (saw {sample:?})"
        )));
    }
    let model = Compiled::new(doc)?.model;
    let declared = |t: &str| rates.get(t).copied().unwrap_or(1.0);
    let mut is_source: HashMap<&str, bool> = HashMap::new();
    let mut race_key: HashMap<&str, String> = HashMap::new();
    for t in &doc.transitions {
        let mut ins: Vec<String> = model.inputs(&t.id).into_iter().map(|a| a.place).collect();
        ins.sort();
        ins.dedup();
        is_source.insert(&t.id, ins.is_empty());
        race_key.insert(&t.id, ins.join("|"));
    }
    let src = |t: &str| is_source.get(t).copied().unwrap_or(false);

    // Gaps in seconds, keyed by transition id.
    let mut gap_sum: HashMap<String, f64> = HashMap::new();
    let mut gap_sq: HashMap<String, f64> = HashMap::new();
    let mut gap_n: HashMap<String, usize> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut starts: Vec<i64> = vec![];
    let mut starts_by: HashMap<String, usize> = HashMap::new();
    let mut source_starts = 0usize;
    let mut events = 0usize;
    for tr in log.traces() {
        let Some(first) = tr.events.first() else {
            continue;
        };
        events += tr.events.len();
        starts.push(first.timestamp);
        for e in &tr.events {
            if let Some(t) = mapping.transition(&e.activity) {
                seen.insert(t.to_string());
            }
        }
        if let Some(t) = mapping.transition(&first.activity).filter(|t| src(t)) {
            *starts_by.entry(t.to_string()).or_default() += 1;
            source_starts += 1;
        }
        for w in tr.events.windows(2) {
            let gap = (w[1].timestamp - w[0].timestamp) as f64 / 1000.0;
            if gap < 0.0 {
                continue;
            }
            if let Some(t) = mapping.transition(&w[1].activity) {
                *gap_sum.entry(t.to_string()).or_default() += gap;
                *gap_sq.entry(t.to_string()).or_default() += gap * gap;
                *gap_n.entry(t.to_string()).or_default() += 1;
            }
        }
    }
    starts.sort();
    let (mut inter_sum, mut inter_sq, mut inter_n) = (0.0, 0.0, 0usize);
    for w in starts.windows(2) {
        let d = (w[1] - w[0]) as f64 / 1000.0;
        inter_sum += d;
        inter_sq += d * d;
        inter_n += 1;
    }
    let gs = |t: &str| gap_sum.get(t).copied().unwrap_or(0.0);
    let gq = |t: &str| gap_sq.get(t).copied().unwrap_or(0.0);
    let gn = |t: &str| gap_n.get(t).copied().unwrap_or(0);

    // Competing risks: pool each race group's gaps.
    let mut group_sum: HashMap<&str, f64> = HashMap::new();
    let mut group_n: HashMap<&str, usize> = HashMap::new();
    for t in &doc.transitions {
        if declared(&t.id) >= 100.0 || src(&t.id) {
            continue;
        }
        let k = race_key[t.id.as_str()].as_str();
        *group_sum.entry(k).or_default() += gs(&t.id);
        *group_n.entry(k).or_default() += gn(&t.id);
    }
    let seen_sources = doc
        .transitions
        .iter()
        .filter(|t| src(&t.id) && seen.contains(&t.id) && declared(&t.id) < 100.0)
        .count();

    let mut learned = BTreeMap::new();
    let mut rows = vec![];
    let (mut insufficient, mut unsplit, mut unobserved) = (vec![], vec![], vec![]);
    let mut pickups = 0;
    for t in &doc.transitions {
        let id = t.id.as_str();
        let activities: Vec<String> = mapping
            .mapped
            .iter()
            .filter(|(_, v)| *v == id)
            .map(|(k, _)| k.clone())
            .collect();
        let previous = declared(id);
        let mut row = RateRow {
            transition: id.to_string(),
            activities,
            kind: "unobserved",
            evidence: None,
            rate: None,
            previous,
        };
        if !seen.contains(id) {
            unobserved.push(id.to_string());
            rows.push(row);
            continue;
        }
        if previous >= 100.0 {
            pickups += 1;
            row.kind = "pickup";
            row.evidence = Some(evidence(gn(id), gs(id), gq(id), unit_s));
            rows.push(row);
            continue;
        }
        let ev = if src(id) {
            row.kind = "source";
            if inter_n > 0 && inter_sum > 0.0 {
                let total = unit_s * inter_n as f64 / inter_sum;
                let mine = starts_by.get(id).copied().unwrap_or(0);
                if source_starts > 0 && mine > 0 {
                    row.rate = Some(total * mine as f64 / source_starts as f64);
                } else if source_starts == 0 && seen_sources == 1 {
                    row.rate = Some(total);
                } else {
                    unsplit.push(id.to_string());
                }
            }
            evidence(inter_n, inter_sum, inter_sq, unit_s)
        } else {
            row.kind = "service";
            let k = race_key[id].as_str();
            let (sum, n) = (group_sum[k], group_n[k]);
            if gn(id) > 0 && sum > 0.0 {
                let total = unit_s * n as f64 / sum;
                row.rate = Some(total * gn(id) as f64 / n as f64);
            }
            evidence(gn(id), gs(id), gq(id), unit_s)
        };
        if ev.insufficient {
            insufficient.push(format!("{id} (n={})", ev.n));
        }
        row.evidence = Some(ev);
        if let Some(r) = row.rate {
            learned.insert(id.to_string(), r);
        }
        rows.push(row);
    }

    let mut caveats = vec![];
    if filtered {
        caveats.push(
            "the log marks lifecycles, so only `complete` events were read (an event is a firing)"
                .to_string(),
        );
    }
    if !mapping.unmapped.is_empty() || !mapping.ambiguous.is_empty() {
        caveats.push(format!(
            "{} activit(ies) name no transition (unmapped {:?}, ambiguous {:?}); their events still separate the gaps around them",
            mapping.unmapped.len() + mapping.ambiguous.len(),
            mapping.unmapped,
            mapping.ambiguous.keys().collect::<Vec<_>>()
        ));
    }
    if !unobserved.is_empty() {
        caveats.push(format!(
            "{} transition(s) never appear in the log; their session rates stand",
            unobserved.len()
        ));
    }
    if !insufficient.is_empty() {
        caveats.push(format!(
            "{} transition(s) rest on fewer than 2 observed gaps — no rate where n=0, no spread where n=1: {}",
            insufficient.len(),
            insufficient.join(", ")
        ));
    }
    if !unsplit.is_empty() {
        caveats.push(format!(
            "{} source transition(s) begin no case in the log, so the pooled arrival rate cannot be split to them; their rates stand: {}",
            unsplit.len(),
            unsplit.join(", ")
        ));
    }
    if pickups > 0 {
        caveats.push(format!(
            "{pickups} instant-pickup transition(s) (session rate >= 100) keep their rate: their observed gap is the queue wait, not the pickup"
        ));
    }
    Ok(Calibration {
        rates: learned,
        rows,
        unobserved,
        caveats,
        mapping,
        unit: unit.to_string(),
        events,
    })
}

fn log_activities_sample(m: &ActivityMap) -> Vec<&String> {
    m.unmapped
        .iter()
        .chain(m.ambiguous.keys())
        .take(5)
        .collect()
}

/// The log with every mapped activity renamed to its transition id.
pub fn remap_log(log: &EventLog, mapping: &ActivityMap) -> EventLog {
    let mut out = EventLog::new();
    for t in log.traces() {
        for e in &t.events {
            let mut e2: Event = e.clone();
            if let Some(id) = mapping.transition(&e.activity) {
                e2.activity = id.to_string();
            }
            out.add_event(e2);
        }
    }
    out.sort_traces();
    out
}

/// Token-replay conformance of `log` (activities already transition ids,
/// see [`remap_log`]) against the document's declared initial marking:
/// sim's headline `fitting_percent` (traces replayed with no missing token
/// or activity), the harsher `token_fitness`, ETC precision, and the five
/// worst traces. It is the same gate-aware replay as `data.conformance`
/// (`crate::conformance`), so read and inhibitor arcs count here too and the
/// two commands agree. (pflow-mining's `check_conformance` is gate-aware
/// since pflow-rs `a0a2eb1`, unchanged at the `aacb7eb` pin, but it ignores capacity,
/// reads weight-0 arcs as 1 and maps activities by label only, so it does
/// not replace this; see FEATURES.md §D 10.)
pub fn conformance(doc: &Document, log: &EventLog) -> Result<Value> {
    let c = crate::conformance::check(doc, log, None)?;
    let r = &c.report;
    let mut worst: Vec<Value> = vec![];
    if let crate::artifact::ArtifactData::Table { columns, rows } = &c.table {
        let col = |n: &str| columns.iter().position(|c| c == n);
        if let (Some(case), Some(fit), Some(ok), Some(miss), Some(dev)) = (
            col("example_case"),
            col("fitness"),
            col("fitting"),
            col("missing"),
            col("first_deviation"),
        ) {
            let mut bad: Vec<&Vec<Value>> = rows.iter().filter(|r| r[ok] == json!(false)).collect();
            bad.sort_by(|a, b| {
                let f = |r: &Vec<Value>| r[fit].as_f64().unwrap_or(0.0);
                f(a).total_cmp(&f(b))
            });
            worst = bad
                .into_iter()
                .take(5)
                .map(|row| {
                    json!({
                        "case": row[case],
                        "fitness": row[fit],
                        "missing_tokens": row[miss],
                        "first_deviation": row[dev],
                    })
                })
                .collect();
        }
    }
    Ok(json!({
        "fitting_percent": r["fitting_percent"],
        "token_fitness": r["token_fitness"],
        "total_traces": r["total_traces"],
        "precision": r["precision"],
        "worst_traces": worst,
        "note": "fitting_percent (full replays from the document's initial marking) is the headline; token_fitness counts a resource pool's leftover tokens as remaining, so it under-reads nets with pools. Same replay as data.conformance.",
    }))
}

// ------------------------------------------------------------ fit

/// `rate = exp(θ)`: a positive rate, fitted on a log scale, so the
/// optimizer's steps are relative and a rate can never go negative.
struct LogRate {
    theta: f64,
}

impl RateFunc for LogRate {
    fn eval(&self, _: &pflow_core::State, _: f64) -> f64 {
        self.theta.exp()
    }
    fn get_params(&self) -> Vec<f64> {
        vec![self.theta]
    }
    fn set_params(&mut self, params: &[f64]) {
        assert_eq!(params.len(), 1, "LogRate takes exactly 1 parameter");
        self.theta = params[0];
    }
    fn num_params(&self) -> usize {
        1
    }
    fn eval_grad(
        &self,
        _: &pflow_core::State,
        _: f64,
    ) -> Option<(f64, Vec<f64>, pflow_core::State)> {
        let v = self.theta.exp();
        Some((v, vec![v], pflow_core::State::new()))
    }
}

/// Loss compared between the fitted ODE and the data (pflow-learn's).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loss {
    Mse,
    Rmse,
    RelativeMse,
}

impl Loss {
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "mse" => Loss::Mse,
            "rmse" => Loss::Rmse,
            "relative" | "relative-mse" | "relative_mse" => Loss::RelativeMse,
            other => {
                return Err(Error::BadArgs(format!(
                    "loss {other:?}: mse, rmse or relative"
                )))
            }
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Loss::Mse => "mse",
            Loss::Rmse => "rmse",
            Loss::RelativeMse => "relative",
        }
    }

    fn value(self, sol: &pflow_solver::Solution, data: &Dataset) -> f64 {
        match self {
            Loss::Mse => pflow_learn::mse_loss(sol, data),
            Loss::Rmse => pflow_learn::rmse_loss(sol, data),
            Loss::RelativeMse => pflow_learn::relative_mse_loss(sol, data),
        }
    }
}

pub fn fit_method(s: &str) -> Result<FitMethod> {
    Ok(match s {
        "nelder-mead" | "nelder_mead" => FitMethod::NelderMead,
        "coordinate-descent" | "coordinate_descent" => FitMethod::CoordinateDescent,
        "adam" => FitMethod::Adam,
        "gradient-descent" | "gradient_descent" => FitMethod::GradientDescent,
        other => {
            return Err(Error::BadArgs(format!(
            "method {other:?}: nelder-mead (default), coordinate-descent, adam or gradient-descent"
        )))
        }
    })
}

/// What [`fit`] is asked.
#[derive(Debug, Clone)]
pub struct FitRequest {
    /// Transitions whose rates are fitted (default: every transition with
    /// a positive session rate).
    pub parameters: Option<Vec<String>>,
    pub method: FitMethod,
    pub loss: Loss,
    /// Fit `ln(rate)` (default) instead of the rate itself (pflow.xyz).
    pub log_space: bool,
    pub max_iters: usize,
    pub tolerance: f64,
    pub solver: String,
    pub ode: OdeOptions,
    /// series name → place, instead of matching series names.
    pub places: Option<Vec<(String, String)>>,
    /// Use only series of this overlay group (default: ungrouped or "data").
    pub group: Option<String>,
}

/// One observed series and the fitted ODE at the same times.
#[derive(Debug, Clone, Serialize)]
pub struct FittedSeries {
    pub series: String,
    /// Display name of the place it observes.
    pub place: String,
    pub observed: Vec<f64>,
    pub initial: Vec<f64>,
    pub fitted: Vec<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FitOutcome {
    pub parameters: Vec<String>,
    pub initial_rates: BTreeMap<String, f64>,
    pub fitted_rates: BTreeMap<String, f64>,
    pub loss: &'static str,
    pub loss_before: f64,
    pub loss_after: f64,
    pub iterations: usize,
    pub evaluations: usize,
    pub converged: bool,
    pub times: Vec<f64>,
    pub series: Vec<FittedSeries>,
    /// Series that name no place (skipped).
    pub ignored: Vec<String>,
    pub caveats: Vec<String>,
}

/// A data series name → (net place label, display name): a display name
/// (`S`, `CoffeeBeans.brown`), a base place id with one series, or a place
/// label ignoring case.
fn resolve_place(
    doc: &Document,
    order: &[(String, String)],
    name: &str,
) -> Result<Option<(String, String)>> {
    let mut idx = sim::select_series(order, name);
    if idx.is_empty() {
        let key = name.trim().to_lowercase();
        if let Some(p) = doc.places.iter().find(|p| {
            p.label
                .as_deref()
                .is_some_and(|l| l.trim().to_lowercase() == key)
        }) {
            idx = sim::select_series(order, &p.id);
        }
    }
    match idx.as_slice() {
        [] => Ok(None),
        [i] => Ok(Some(order[*i].clone())),
        _ => Err(Error::BadArgs(format!(
            "{name:?} names a place holding several colors; name one of {:?}",
            idx.iter().map(|&i| &order[i].1).collect::<Vec<_>>()
        ))),
    }
}

/// Fits transition rates so the mass-action ODE from marking `mk` matches
/// the observed series of a time-series artifact (pflow-learn's `fit`,
/// over its `LearnableProblem`: the same mass-action law as `sim.ode`).
/// The ODE starts from `mk` at the data's first time and runs to its last.
pub fn fit(
    doc: &Document,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    data: &ArtifactData,
    req: &FitRequest,
) -> Result<FitOutcome> {
    let ArtifactData::TimeSeries { x, series, .. } = data else {
        return Err(Error::BadArgs(format!(
            "fit needs a timeseries artifact, got a {}",
            data.kind()
        )));
    };
    if x.len() < 2 || x.len() > MAX_FIT_POINTS {
        return Err(Error::BadArgs(format!(
            "the data has {} time points; fit needs 2..{MAX_FIT_POINTS}",
            x.len()
        )));
    }
    if x.iter().any(|t| !t.is_finite()) || x.windows(2).any(|w| w[1] < w[0]) {
        return Err(Error::BadArgs(
            "the data's times must be finite and ascending".into(),
        ));
    }
    let (t0, t1) = (x[0], x[x.len() - 1]);
    if t1 <= t0 {
        return Err(Error::BadArgs("the data spans no time".into()));
    }
    if req.max_iters == 0 || req.max_iters > MAX_FIT_ITERS {
        return Err(Error::BadArgs(format!(
            "max_iters {} is outside 1..{MAX_FIT_ITERS}",
            req.max_iters
        )));
    }
    if !(req.tolerance > 0.0 && req.tolerance.is_finite()) {
        return Err(Error::BadArgs("tolerance must be a positive number".into()));
    }
    let solver = sim::solver(&req.solver)?;
    let solver_options = req.ode.solver_options(t1 - t0)?;
    let cn = sim::continuous(doc, mk, rates)?;

    // Which series observe which place.
    let candidates: Vec<&crate::artifact::Series> = series
        .iter()
        .filter(|s| match (&req.group, &s.group) {
            (Some(g), Some(sg)) => g == sg,
            (Some(_), None) => false,
            (None, None) => true,
            (None, Some(sg)) => sg == "data",
        })
        .collect();
    if candidates.is_empty() {
        let mut groups: Vec<&str> = series.iter().filter_map(|s| s.group.as_deref()).collect();
        groups.dedup();
        return Err(Error::BadArgs(format!(
            "no series to fit: {} (groups present: {groups:?})",
            match &req.group {
                Some(g) => format!("none in group {g:?}"),
                None => "none is ungrouped or in group \"data\"; pass group".into(),
            }
        )));
    }
    let mut chosen: Vec<(&crate::artifact::Series, String, String)> = vec![];
    let mut ignored = vec![];
    match &req.places {
        Some(pairs) => {
            for (sname, place) in pairs {
                let s = candidates
                    .iter()
                    .find(|s| &s.name == sname)
                    .ok_or_else(|| {
                        Error::NotFound(format!(
                            "places names series {sname:?}, which the data does not have"
                        ))
                    })?;
                let (label, display) = resolve_place(doc, &cn.order, place)?.ok_or_else(|| {
                    Error::NotFound(format!(
                        "places maps {sname:?} to {place:?}, which is not a place"
                    ))
                })?;
                chosen.push((s, label, display));
            }
        }
        None => {
            for s in &candidates {
                match resolve_place(doc, &cn.order, &s.name)? {
                    Some((label, display)) => chosen.push((s, label, display)),
                    None => ignored.push(s.name.clone()),
                }
            }
        }
    }
    if chosen.is_empty() {
        let known: Vec<&str> = cn.order.iter().map(|(_, d)| d.as_str()).collect();
        return Err(Error::BadArgs(format!(
            "no data series names a place (series {ignored:?}; places {known:?}); map them with places"
        )));
    }
    let mut used = HashSet::new();
    for (s, label, display) in &chosen {
        if !used.insert(label.clone()) {
            return Err(Error::BadArgs(format!(
                "two series observe place {display:?}"
            )));
        }
        if s.values.len() != x.len() || s.values.iter().any(|v| !v.is_finite()) {
            return Err(Error::BadArgs(format!(
                "series {:?} must hold one finite value per time point",
                s.name
            )));
        }
    }

    // Which rates are fitted.
    let parameters: Vec<String> = match &req.parameters {
        Some(p) => {
            for (i, id) in p.iter().enumerate() {
                if doc.transition(id).is_none() {
                    return Err(Error::NotFound(format!("no transition {id:?}")));
                }
                if p[..i].contains(id) {
                    return Err(Error::BadArgs(format!("{id} appears twice in parameters")));
                }
            }
            p.clone()
        }
        None => doc
            .transitions
            .iter()
            .filter(|t| rates.get(&t.id).copied().unwrap_or(1.0) > 0.0)
            .map(|t| t.id.clone())
            .collect(),
    };
    if parameters.is_empty() {
        return Err(Error::BadArgs("no rate to fit".into()));
    }
    if parameters.len() > MAX_FIT_PARAMS {
        return Err(Error::BadArgs(format!(
            "{} parameters is over the {MAX_FIT_PARAMS} cap",
            parameters.len()
        )));
    }

    let mut caveats = vec![];
    let mut prob = LearnableProblem::new(cn.net.clone(), cn.state.clone(), [t0, t1]);
    let mut handles: Vec<(String, SharedRateFunc)> = vec![];
    let mut initial_rates = BTreeMap::new();
    let mut labels: Vec<&String> = cn.rates.keys().collect();
    labels.sort();
    for label in labels {
        let rate = cn.rates[label];
        if parameters.contains(label) {
            let h: SharedRateFunc = if req.log_space {
                let start = if rate > 0.0 {
                    rate
                } else {
                    caveats.push(format!(
                        "{label} starts at rate 0, which has no logarithm; the fit started it at 1"
                    ));
                    1.0
                };
                Rc::new(RefCell::new(LogRate { theta: start.ln() }))
            } else {
                Rc::new(RefCell::new(ScalarRateFunc::new(rate)))
            };
            initial_rates.insert(label.clone(), rate);
            prob.set_rate_func(label, h.clone());
            handles.push((label.clone(), h));
        } else {
            prob.set_rate_func(label, Rc::new(RefCell::new(ConstantRateFunc::new(rate))));
        }
    }
    let observations: HashMap<String, Vec<f64>> = chosen
        .iter()
        .map(|(s, label, _)| (label.clone(), s.values.clone()))
        .collect();
    let place_labels: Vec<String> = chosen.iter().map(|(_, l, _)| l.clone()).collect();
    let dataset = Dataset::new(x.clone(), observations, place_labels)
        .map_err(|e| Error::Invalid(e.to_string()))?;

    let learn_err = |e: pflow_learn::LearnError| Error::Invalid(format!("fit: {e}"));
    let solve = |prob: &LearnableProblem| prob.solve(&solver, &solver_options).map_err(learn_err);
    let sol0 = solve(&prob)?;
    let loss_before = req.loss.value(&sol0, &dataset);
    let initial_curves: Vec<Vec<f64>> = chosen
        .iter()
        .map(|(_, l, _)| pflow_learn::interpolate_solution(&sol0, x, l))
        .collect();
    for (s, label, display) in &chosen {
        let u0 = cn.state.get(label).copied().unwrap_or(0.0);
        let d0 = s.values[0];
        if (u0 - d0).abs() > 1e-6 * (1.0 + d0.abs()) {
            caveats.push(format!(
                "series {:?} starts at {d0} but place {display} holds {u0} in the current marking, which the fit starts from",
                s.name
            ));
        }
    }

    let opts = FitOptions {
        max_iters: req.max_iters,
        tolerance: req.tolerance,
        method: req.method,
        solver_method: solver.clone(),
        solver_options: solver_options.clone(),
        ..FitOptions::default()
    };
    let loss = req.loss;
    let loss_fn = move |sol: &pflow_solver::Solution, d: &Dataset| loss.value(sol, d);
    let grad_fn = move |s: &pflow_learn::Sensitivities, d: &Dataset| match loss {
        Loss::Mse => pflow_learn::mse_loss_grad(s, d),
        Loss::Rmse => pflow_learn::rmse_loss_grad(s, d),
        Loss::RelativeMse => pflow_learn::relative_mse_loss_grad(s, d),
    };
    let res = pflow_learn::fit(&prob, &dataset, &loss_fn, &grad_fn, &opts).map_err(learn_err)?;

    let sol = solve(&prob)?;
    let loss_after = loss.value(&sol, &dataset);
    let mut fitted_rates = BTreeMap::new();
    for (label, h) in &handles {
        let p = h.borrow().get_params()[0];
        let r = if req.log_space { p.exp() } else { p };
        if r < 0.0 {
            caveats.push(format!(
                "{label} fitted to {r}, below 0; a negative rate fires nothing, so it is reported as 0"
            ));
        }
        fitted_rates.insert(label.clone(), r.max(0.0));
    }
    if !res.converged {
        caveats.push(format!(
            "the optimizer stopped at max_iters {} before converging (tolerance {})",
            req.max_iters, req.tolerance
        ));
    }
    let series_out = chosen
        .iter()
        .zip(initial_curves)
        .map(|((s, label, display), initial)| FittedSeries {
            series: s.name.clone(),
            place: display.clone(),
            observed: s.values.clone(),
            initial,
            fitted: pflow_learn::interpolate_solution(&sol, x, label),
        })
        .collect();
    // Report in document order.
    let order: Vec<String> = doc
        .transitions
        .iter()
        .map(|t| t.id.clone())
        .filter(|id| parameters.contains(id))
        .collect();
    Ok(FitOutcome {
        parameters: order,
        initial_rates,
        fitted_rates,
        loss: loss.name(),
        loss_before,
        loss_after,
        iterations: res.iters,
        evaluations: res.evals,
        converged: res.converged,
        times: x.clone(),
        series: series_out,
        ignored,
        caveats,
    })
}

// ------------------------------------------------------------ distill

/// One side of a turn-based game.
#[derive(Debug, Clone, Serialize)]
pub struct Player {
    pub name: String,
    /// The model place whose token says it is this player's move.
    pub turn: String,
    pub moves: Vec<String>,
    pub maximizes: bool,
}

/// A net read as a game: players with turn places and moves; every other
/// transition is the house (win detectors, draws), fired to quiescence
/// between moves; the objective is read from the final marking.
pub struct Game {
    pub net: Net,
    pub players: Vec<Player>,
    pub objective: Objective,
    owned: HashSet<String>,
    /// Model transition ids in model order.
    order: Vec<String>,
}

fn turn_key(id: &str) -> Option<String> {
    let l = id.to_lowercase();
    l.strip_suffix("_turn")
        .or_else(|| l.strip_prefix("turn_"))
        .filter(|k| !k.is_empty())
        .map(str::to_string)
}

/// Moves of the player whose turn token sits in `turn`: transitions that
/// consume it and pass the turn to another player's turn place; failing
/// that, transitions that consume it and whose id starts with the player's
/// name.
fn infer_moves(model: &Model, turn: &str, other_turns: &[String], name: &str) -> Vec<String> {
    let consumes = |t: &str| model.inputs(t).iter().any(|a| a.place == turn);
    let hands_over = |t: &str| {
        model
            .outputs(t)
            .iter()
            .any(|a| other_turns.contains(&a.place))
    };
    let ids = || model.transitions.iter().map(|t| t.id.as_str());
    let mut moves: Vec<String> = ids()
        .filter(|t| consumes(t) && hands_over(t))
        .map(str::to_string)
        .collect();
    if moves.is_empty() {
        let n = name.to_lowercase();
        moves = ids()
            .filter(|t| consumes(t) && t.to_lowercase().starts_with(&n))
            .map(str::to_string)
            .collect();
    }
    moves
}

/// `win_<player>` (or `<player>_win`) minus every other win place (as
/// `sim.evaluate`'s default).
fn default_objective(net: &Net, player: &str) -> Result<Objective> {
    let key = player.to_lowercase();
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
        return Err(Error::BadArgs(format!(
            "distill needs an objective: pass objective (e.g. \"win_x - win_o\"), or a player whose win_<player> place exists (player {player:?} has none)"
        )));
    }
    let mut obj = serde_json::Map::new();
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

impl Game {
    /// Reads the game from the document. `players` (JSON
    /// `{name: {turn, transitions?, maximizes?}}`) overrides the inference:
    /// a place named `<name>_turn` (or `turn_<name>`) makes a player, whose
    /// moves are the transitions that consume its turn token and hand the
    /// turn to another player. `player` maximizes the objective (default:
    /// the player whose turn place is marked at `start`); the others
    /// minimize it.
    pub fn new(
        doc: &Document,
        start: &Marking,
        player: Option<&str>,
        players: Option<&Value>,
        objective: Option<&Value>,
    ) -> Result<Self> {
        let net = Net::new(doc)?;
        let mut out: Vec<Player> = vec![];
        let mut explicit_max: HashMap<String, bool> = HashMap::new();
        match players {
            Some(v) => {
                let v = st::json_arg(v, "players")?;
                let m = v.as_object().filter(|m| !m.is_empty()).ok_or_else(|| {
                    Error::BadArgs(
                        "players must be {name: {turn, transitions?, maximizes?}}".into(),
                    )
                })?;
                let turns: Vec<String> = m
                    .values()
                    .filter_map(|p| p.get("turn").and_then(Value::as_str))
                    .map(|t| net.resolve_place(t))
                    .collect::<Result<_>>()?;
                for (name, p) in m {
                    let turn = p.get("turn").and_then(Value::as_str).ok_or_else(|| {
                        Error::BadArgs(format!("player {name:?} needs a turn place"))
                    })?;
                    let turn = net.resolve_place(turn)?;
                    let moves = match p.get("transitions") {
                        Some(Value::Array(ts)) => ts
                            .iter()
                            .map(|t| {
                                let id = t.as_str().ok_or_else(|| {
                                    Error::BadArgs(format!("player {name:?}: transitions are ids"))
                                })?;
                                if !net.has_transition(id) {
                                    return Err(Error::NotFound(format!("no transition {id:?}")));
                                }
                                Ok(id.to_string())
                            })
                            .collect::<Result<_>>()?,
                        None | Some(Value::Null) => {
                            let others: Vec<String> =
                                turns.iter().filter(|t| **t != turn).cloned().collect();
                            infer_moves(&net.model, &turn, &others, name)
                        }
                        Some(_) => {
                            return Err(Error::BadArgs(format!(
                                "player {name:?}: transitions must be a list of ids"
                            )))
                        }
                    };
                    if let Some(b) = p.get("maximizes").and_then(Value::as_bool) {
                        explicit_max.insert(name.clone(), b);
                    }
                    out.push(Player {
                        name: name.clone(),
                        turn,
                        moves,
                        maximizes: false,
                    });
                }
            }
            None => {
                let found: Vec<(String, String)> = doc
                    .places
                    .iter()
                    .filter_map(|p| turn_key(&p.id).map(|k| (k, p.id.clone())))
                    .collect();
                let turns: Vec<String> = found
                    .iter()
                    .filter_map(|(_, id)| net.resolve_place(id).ok())
                    .collect();
                for (name, id) in &found {
                    let turn = net.resolve_place(id)?;
                    let others: Vec<String> =
                        turns.iter().filter(|t| **t != turn).cloned().collect();
                    let moves = infer_moves(&net.model, &turn, &others, name);
                    if !moves.is_empty() {
                        out.push(Player {
                            name: name.clone(),
                            turn,
                            moves,
                            maximizes: false,
                        });
                    }
                }
            }
        }
        if out.is_empty() {
            return Err(Error::BadArgs(
                "distill needs a turn-based game: no place is named <player>_turn with moves that consume it; declare players {name: {turn, transitions}}".into(),
            ));
        }
        for p in &out {
            if p.moves.is_empty() {
                return Err(Error::BadArgs(format!("player {:?} has no moves", p.name)));
            }
        }
        let mk = net.model_marking(start);
        let me = match player {
            Some(p) => out
                .iter()
                .find(|q| q.name == p || q.name.eq_ignore_ascii_case(p))
                .map(|q| q.name.clone())
                .ok_or_else(|| {
                    Error::NotFound(format!(
                        "no player {p:?}; players are {:?}",
                        out.iter().map(|q| &q.name).collect::<Vec<_>>()
                    ))
                })?,
            None => out
                .iter()
                .find(|q| mk.get(&q.turn).copied().unwrap_or(0) > 0)
                .unwrap_or(&out[0])
                .name
                .clone(),
        };
        for p in &mut out {
            p.maximizes = explicit_max.get(&p.name).copied().unwrap_or(p.name == me);
        }
        let objective = match objective {
            Some(v) => Objective::parse(v, &net)?,
            None => default_objective(&net, &me)?,
        };
        let owned = out.iter().flat_map(|p| p.moves.iter().cloned()).collect();
        let order = net.model.transitions.iter().map(|t| t.id.clone()).collect();
        Ok(Game {
            net,
            players: out,
            objective,
            owned,
            order,
        })
    }

    /// Unowned transitions fired to quiescence (the referee), in model order.
    pub fn house_fire(&self, mut mk: HashMap<String, i64>) -> HashMap<String, i64> {
        for _ in 0..self.order.len() * 4 {
            let mut fired = false;
            for t in &self.order {
                if !self.owned.contains(t) && self.net.model.enabled(t, &mk) {
                    mk = self.net.model.fire(t, &mk);
                    fired = true;
                }
            }
            if !fired {
                break;
            }
        }
        mk
    }

    /// The player to move: the first whose turn place is marked.
    pub fn mover(&self, mk: &HashMap<String, i64>) -> Option<usize> {
        self.players
            .iter()
            .position(|p| mk.get(&p.turn).copied().unwrap_or(0) > 0)
    }

    pub fn moves(&self, player: usize, mk: &HashMap<String, i64>) -> Vec<String> {
        self.players[player]
            .moves
            .iter()
            .filter(|t| self.net.model.enabled(t, mk))
            .cloned()
            .collect()
    }

    fn key(&self, mk: &HashMap<String, i64>) -> Vec<i64> {
        self.net
            .places
            .iter()
            .map(|p| mk.get(p).copied().unwrap_or(0))
            .collect()
    }

    fn score(&self, mk: &HashMap<String, i64>) -> f64 {
        let f: HashMap<String, f64> = mk.iter().map(|(k, v)| (k.clone(), *v as f64)).collect();
        self.objective.value(&f)
    }

    /// The session-shaped marking (per place, per color) of a model marking.
    pub fn session_marking(&self, mk: &HashMap<String, i64>) -> Marking {
        let mut out = Marking::new();
        for id in &self.net.places {
            let (base, c) = self.net.base(id);
            let v = out.entry(base).or_default();
            if v.len() <= c {
                v.resize(c + 1, 0);
            }
            v[c] = mk.get(id).copied().unwrap_or(0);
        }
        out
    }
}

/// Exact minimax over a [`Game`], memoized by marking. A value is stored
/// only when no line under it was cut (depth cap or a repeated position),
/// so the table is sound across every position it labels.
pub struct Search<'a> {
    game: &'a Game,
    memo: HashMap<Vec<i64>, f64>,
    on_path: HashSet<Vec<i64>>,
    pub nodes: usize,
    pub depth_cuts: usize,
    pub repeat_cuts: usize,
}

impl<'a> Search<'a> {
    pub fn new(game: &'a Game) -> Self {
        Self {
            game,
            memo: HashMap::new(),
            on_path: HashSet::new(),
            nodes: 0,
            depth_cuts: 0,
            repeat_cuts: 0,
        }
    }

    /// (objective value under optimal play, exact).
    fn value(&mut self, mk: HashMap<String, i64>, depth: usize) -> Result<(f64, bool)> {
        let g = self.game;
        let mk = g.house_fire(mk);
        let k = g.key(&mk);
        if self.on_path.contains(&k) {
            self.repeat_cuts += 1;
            return Ok((g.score(&mk), false));
        }
        if depth >= MAX_SEARCH_DEPTH {
            self.depth_cuts += 1;
            return Ok((g.score(&mk), false));
        }
        if let Some(v) = self.memo.get(&k) {
            return Ok((*v, true));
        }
        self.nodes += 1;
        if self.nodes > MAX_SEARCH_NODES {
            return Err(Error::Invalid(format!(
                "exact search passed {MAX_SEARCH_NODES} positions; this game is too large to label by minimax"
            )));
        }
        let mover = g.mover(&mk);
        let moves = mover.map(|p| g.moves(p, &mk)).unwrap_or_default();
        let Some(p) = mover.filter(|_| !moves.is_empty()) else {
            let v = g.score(&mk);
            self.memo.insert(k, v);
            return Ok((v, true));
        };
        let max = g.players[p].maximizes;
        self.on_path.insert(k.clone());
        let mut best = 0.0;
        let mut clean = true;
        for (i, t) in moves.iter().enumerate() {
            let (v, c) = match self.value(g.net.model.fire(t, &mk), depth + 1) {
                Ok(r) => r,
                Err(e) => {
                    self.on_path.remove(&k);
                    return Err(e);
                }
            };
            clean &= c;
            if i == 0 || (max && v > best) || (!max && v < best) {
                best = v;
            }
        }
        self.on_path.remove(&k);
        if clean {
            self.memo.insert(k, best);
        }
        Ok((best, clean))
    }

    /// Each candidate's payout for `player` under optimal play after it,
    /// and whether every value is exact.
    pub fn label(
        &mut self,
        mk: &HashMap<String, i64>,
        player: usize,
        candidates: &[String],
    ) -> Result<(Vec<f64>, bool)> {
        let g = self.game;
        let root = g.key(&g.house_fire(mk.clone()));
        self.on_path.insert(root.clone());
        let mut out = vec![];
        let mut exact = true;
        for t in candidates {
            let r = self.value(g.net.model.fire(t, mk), 1);
            let (v, c) = match r {
                Ok(r) => r,
                Err(e) => {
                    self.on_path.remove(&root);
                    return Err(e);
                }
            };
            exact &= c;
            out.push(if g.players[player].maximizes { v } else { -v });
        }
        self.on_path.remove(&root);
        Ok((out, exact))
    }
}

/// What [`distill`] is asked (sim's `DistillRequest`; 0 = default is the
/// caller's business: here every field is already resolved).
#[derive(Debug, Clone)]
pub struct DistillRequest {
    /// group name → transitions sharing one multiplier, sorted by name.
    pub groups: BTreeMap<String, Vec<String>>,
    pub games: usize,
    pub positions: usize,
    pub iters: usize,
    pub horizon: f64,
    pub realizations: usize,
    pub seed: u64,
}

/// sim's work unit: one realization over the default horizon.
pub fn distill_evals(groups: usize, iters: usize) -> f64 {
    (1 + (groups + 1) + iters * (groups + 2) + 2) as f64
}

pub fn distill_work(
    groups: usize,
    iters: usize,
    moves: usize,
    realizations: usize,
    horizon: f64,
) -> f64 {
    let hf = (horizon / DISTILL_DEFAULT_HORIZON).max(1.0);
    distill_evals(groups, iters) * moves as f64 * realizations as f64 * hf
}

/// sim's `MaxDistillWork`: four runs at the defaults.
pub fn max_distill_work() -> f64 {
    4.0 * distill_work(
        1,
        DISTILL_DEFAULT_ITERS,
        DISTILL_DEFAULT_POSITIONS * 2,
        DISTILL_DEFAULT_REALIZATIONS,
        DISTILL_DEFAULT_HORIZON,
    )
}

fn work_error(
    work: f64,
    groups: usize,
    iters: usize,
    moves: usize,
    reals: usize,
    horizon: f64,
    what: &str,
) -> Error {
    Error::BadArgs(format!(
        "distill work {work:.0} is over the {:.0} cap ({:.0} loss evaluations at {groups} group(s) and {iters} iters × {moves} candidate moves {what} × {reals} realizations × horizon {horizon} / {DISTILL_DEFAULT_HORIZON}); lower iters, positions, realizations, horizon or the number of groups",
        max_distill_work(),
        distill_evals(groups, iters)
    ))
}

/// Refuses a request over sim's caps or its up-front work estimate (two
/// moves a position).
pub fn check_distill(req: &DistillRequest) -> Result<()> {
    for (name, v, lo, max) in [
        ("games", req.games, 1, MAX_DISTILL_GAMES),
        ("positions", req.positions, 1, MAX_DISTILL_POSITIONS),
        ("iters", req.iters, 1, MAX_DISTILL_ITERS),
        ("realizations", req.realizations, 1, st::MAX_RUNS),
    ] {
        if v < lo || v > max {
            return Err(Error::BadArgs(format!("{name} {v} is outside {lo}..{max}")));
        }
    }
    st::check_horizon("horizon", req.horizon, st::MAX_HORIZON)?;
    if req.groups.is_empty() {
        return Err(Error::BadArgs("distill needs at least one group".into()));
    }
    if req.groups.len() > MAX_DISTILL_GROUPS {
        return Err(Error::BadArgs(format!(
            "groups: {} groups is over the {MAX_DISTILL_GROUPS} cap",
            req.groups.len()
        )));
    }
    let mut owner: HashMap<&str, &str> = HashMap::new();
    for (g, ts) in &req.groups {
        if ts.is_empty() {
            return Err(Error::BadArgs(format!("group {g:?} lists no transitions")));
        }
        for t in ts {
            if let Some(prev) = owner.insert(t, g) {
                return Err(Error::BadArgs(if prev == g {
                    format!("group {g:?} lists transition {t:?} twice")
                } else {
                    format!("transition {t:?} is in groups {prev:?} and {g:?}; a transition takes one multiplier")
                }));
            }
        }
    }
    let w = distill_work(
        req.groups.len(),
        req.iters,
        req.positions * 2,
        req.realizations,
        req.horizon,
    );
    if w > max_distill_work() {
        return Err(work_error(
            w,
            req.groups.len(),
            req.iters,
            req.positions * 2,
            req.realizations,
            req.horizon,
            &format!("({} positions at the fewest, 2 each)", req.positions),
        ));
    }
    Ok(())
}

/// One sampled decision point.
#[derive(Debug, Clone, Serialize)]
pub struct Position {
    pub player: String,
    pub moves: Vec<String>,
    /// Search-optimal moves (payout within 1e-9 of the best).
    pub preferred: Vec<String>,
    /// Search payouts, aligned with `moves`.
    pub values: Vec<f64>,
    /// The label came from a search with no cut line.
    pub exact: bool,
    /// The play scorer's pick at multipliers 1 and after the fit.
    pub before: String,
    pub after: String,
    #[serde(skip)]
    mk: HashMap<String, i64>,
    #[serde(skip)]
    mover: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Distillation {
    pub multipliers: BTreeMap<String, f64>,
    pub groups: BTreeMap<String, Vec<String>>,
    pub positions: usize,
    /// Positions with both preferred and non-preferred moves.
    pub labeled: usize,
    pub agreement_before: usize,
    pub agreement_after: usize,
    pub train_loss: f64,
    pub loss_before: f64,
    pub iterations: usize,
    pub engine: &'static str,
    pub search_nodes: usize,
    pub caveats: Vec<String>,
    #[serde(skip)]
    pub detail: Vec<Position>,
}

/// Fits rate multipliers for `req.groups` so the play scorer (`sim.evaluate`'s
/// SSA rollouts, [`st::evaluate`]) ranks moves as exact minimax does, on
/// positions sampled by seeded random self-play from `start` (sim's
/// `Distill`: hinge rank loss, margin 0.0005, log-space Nelder-Mead from
/// multipliers 1, tolerance 1e-9).
pub fn distill(
    game: &Game,
    start: &Marking,
    rates: &BTreeMap<String, f64>,
    req: &DistillRequest,
) -> Result<Distillation> {
    check_distill(req)?;
    for (g, ts) in &req.groups {
        for t in ts {
            if !game.net.has_transition(t) {
                return Err(Error::NotFound(format!(
                    "group {g:?} names {t:?}, which is not a transition"
                )));
            }
        }
    }
    let m0 = game.net.model_marking(start);
    st::check_tokens(&m0, true)?;

    // Sample distinct decision points by random self-play.
    let model = &game.net.model;
    let mut rng = Rng::new(req.seed);
    let mut seen: HashSet<(usize, Vec<i64>)> = HashSet::new();
    let mut positions: Vec<Position> = vec![];
    let max_plies = (8 * (model.transitions.len() + model.places.len())).max(256);
    let mut truncated = 0;
    'games: for _ in 0..req.games {
        if positions.len() >= req.positions {
            break;
        }
        let mut mk = m0.clone();
        let mut ply = 0;
        while positions.len() < req.positions {
            if ply >= max_plies {
                truncated += 1;
                continue 'games;
            }
            ply += 1;
            mk = game.house_fire(mk);
            let Some(p) = game.mover(&mk) else { break };
            let moves = game.moves(p, &mk);
            if moves.is_empty() {
                break;
            }
            if moves.len() >= 2 && seen.insert((p, game.key(&mk))) {
                positions.push(Position {
                    player: game.players[p].name.clone(),
                    moves: moves.clone(),
                    preferred: vec![],
                    values: vec![],
                    exact: true,
                    before: String::new(),
                    after: String::new(),
                    mk: mk.clone(),
                    mover: p,
                });
            }
            let k = ((rng.next_f64() * moves.len() as f64) as usize).min(moves.len() - 1);
            mk = model.fire(&moves[k], &mk);
        }
    }
    if positions.is_empty() {
        return Err(Error::Invalid(
            "random self-play produced no decision point with two or more moves".into(),
        ));
    }
    let total_moves: usize = positions.iter().map(|p| p.moves.len()).sum();
    let w = distill_work(
        req.groups.len(),
        req.iters,
        total_moves,
        req.realizations,
        req.horizon,
    );
    if w > max_distill_work() {
        return Err(work_error(
            w,
            req.groups.len(),
            req.iters,
            total_moves,
            req.realizations,
            req.horizon,
            &format!("(summed over the {} sampled positions)", positions.len()),
        ));
    }

    // Label each position with exact search (one shared table).
    let mut search = Search::new(game);
    let (mut labeled, mut cut_labels) = (0, 0);
    for p in &mut positions {
        let (vals, exact) = search.label(&p.mk, p.mover, &p.moves)?;
        let best = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        p.preferred = p
            .moves
            .iter()
            .zip(&vals)
            .filter(|(_, v)| (best - **v).abs() < 1e-9)
            .map(|(t, _)| t.clone())
            .collect();
        if p.preferred.len() < p.moves.len() {
            labeled += 1;
        }
        if !exact {
            cut_labels += 1;
        }
        p.values = vals;
        p.exact = exact;
    }

    // The play scorer under a multiplier vector.
    let group_names: Vec<&String> = req.groups.keys().collect();
    let base = |t: &str| rates.get(t).copied().unwrap_or(1.0);
    let score_all = |mult: &[f64]| -> Result<(Vec<pflow_learn::Decision>, Vec<usize>)> {
        let mut r = rates.clone();
        for (g, m) in group_names.iter().zip(mult) {
            for t in &req.groups[*g] {
                r.insert(t.clone(), base(t) * m);
            }
        }
        let mut decisions = Vec::with_capacity(positions.len());
        let mut argmax = Vec::with_capacity(positions.len());
        for p in &positions {
            let session_mk = game.session_marking(&p.mk);
            let (_, scored) = st::evaluate(
                &game.net,
                &session_mk,
                &r,
                &p.moves,
                &game.objective,
                Rollout::Ssa {
                    horizon: req.horizon,
                },
                req.realizations,
                req.seed,
            )?;
            let sign = if game.players[p.mover].maximizes {
                1.0
            } else {
                -1.0
            };
            let scores: Vec<f64> = p
                .moves
                .iter()
                .map(|t| {
                    sign * scored
                        .iter()
                        .find(|m| &m.transition == t)
                        .map(|m| m.payout)
                        .unwrap_or(f64::NEG_INFINITY)
                })
                .collect();
            let mut best = 0;
            for (j, s) in scores.iter().enumerate() {
                if *s > scores[best] {
                    best = j;
                }
            }
            decisions.push(pflow_learn::Decision {
                preferred: p.moves.iter().map(|t| p.preferred.contains(t)).collect(),
                scores,
            });
            argmax.push(best);
        }
        Ok((decisions, argmax))
    };
    let agreement = |argmax: &[usize]| {
        positions
            .iter()
            .zip(argmax)
            .filter(|(p, &i)| p.preferred.contains(&p.moves[i]))
            .count()
    };

    const MARGIN: f64 = 0.0005;
    let ones = vec![1.0; group_names.len()];
    let (d0, before_arg) = score_all(&ones)?;
    let loss_before = pflow_learn::hinge_rank_loss(&d0, MARGIN);
    let failure: RefCell<Option<Error>> = RefCell::new(None);
    let loss = |logp: &[f64]| -> f64 {
        if failure.borrow().is_some() {
            return f64::INFINITY;
        }
        let mult: Vec<f64> = logp.iter().map(|x| x.exp()).collect();
        match score_all(&mult) {
            Ok((d, _)) => pflow_learn::hinge_rank_loss(&d, MARGIN),
            Err(e) => {
                *failure.borrow_mut() = Some(e);
                f64::INFINITY
            }
        }
    };
    let settings = SimplexSettings {
        max_iters: req.iters,
        tolerance: 1e-9,
        step_size: 0.01,
        verbose: false,
    };
    let res = nelder_mead::minimize(
        &loss,
        &vec![0.0; group_names.len()],
        SimplexMethod::NelderMead,
        &settings,
    );
    if let Some(e) = failure.into_inner() {
        return Err(e);
    }
    let mult: Vec<f64> = res.params.iter().map(|x| x.exp()).collect();
    let (_, after_arg) = score_all(&mult)?;
    let before = agreement(&before_arg);
    let after = agreement(&after_arg);
    for (i, p) in positions.iter_mut().enumerate() {
        p.before = p.moves[before_arg[i]].clone();
        p.after = p.moves[after_arg[i]].clone();
    }

    let mut caveats = vec![
        "labels are exact minimax over the game; the fit can only calibrate what the model's structure can express — if agreement does not move, the missing knowledge is structural (declare the prior as transitions, e.g. forced-reply copies) rather than a matter of rates".to_string(),
        "positions come from this implementation's seeded self-play (splitmix64), so they differ from sim.pflow.xyz's for the same seed; agreement counts are comparable in kind, not position for position".to_string(),
    ];
    if cut_labels > 0 {
        caveats.push(format!(
            "{cut_labels} of {} labels came from a search that scored some positions where they stood (the {MAX_SEARCH_DEPTH}-ply cap or a repeated position); those labels are not exact minimax",
            positions.len()
        ));
    }
    if truncated > 0 {
        caveats.push(format!(
            "{truncated} self-play game(s) were cut off at {max_plies} plies without ending"
        ));
    }
    if after < before {
        caveats.push("fitted multipliers agree with search on fewer training positions than multipliers of 1; prefer the identity — the hinge loss optimized margins at the cost of argmax on this sample".into());
    }
    Ok(Distillation {
        multipliers: group_names
            .iter()
            .zip(&mult)
            .map(|(g, m)| ((*g).clone(), *m))
            .collect(),
        groups: req.groups.clone(),
        positions: positions.len(),
        labeled,
        agreement_before: before,
        agreement_after: after,
        train_loss: res.final_loss,
        loss_before,
        iterations: res.iters,
        engine: "ssa",
        search_nodes: search.nodes,
        caveats,
        detail: positions,
    })
}

/// The default groups when a caller names none: `moves` (every player's
/// moves) and `house` (every other transition that consumes a token) —
/// the race between play and the referee is what the SSA scorer can tune.
pub fn default_groups(game: &Game) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let moves: Vec<String> = game
        .order
        .iter()
        .filter(|t| game.owned.contains(*t))
        .cloned()
        .collect();
    let house: Vec<String> = game
        .order
        .iter()
        .filter(|t| !game.owned.contains(*t) && !game.net.model.inputs(t).is_empty())
        .cloned()
        .collect();
    if !moves.is_empty() {
        out.insert("moves".to_string(), moves);
    }
    if !house.is_empty() {
        out.insert("house".to_string(), house);
    }
    out
}

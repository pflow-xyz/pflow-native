//! Conformance checking against the current net: whole-log token replay
//! (sim.pflow.xyz's `sim_conformance`, go-pflow's `mining.CheckFullConformance`
//! plus `sim.LogResidual`) and online, event-by-event replay with the first
//! divergence and a local repair (`sim_conformance_stream`, sim's
//! `Monitor.Observe`).
//!
//! # Activities → transitions
//!
//! Every data command maps a log activity to a transition with one rule,
//! [`ActivityMap::resolve`], tried tier by tier, the first tier with any match
//! deciding:
//!
//! 1. the transition **id**, exactly (`BrewCoffee`);
//! 2. the transition **label**, exactly (`Brew Coffee`);
//! 3. the id, ignoring case;
//! 4. the label, ignoring case.
//!
//! Two or more matches within the deciding tier make the activity
//! *ambiguous*; it is then treated like an unknown activity (and reported),
//! never resolved by document order. sim maps activities to transition ids
//! only (go-pflow by label, which sim sets to the id), so tier 1 is sim's
//! behaviour and the rest are additions for pflow.xyz nets, whose labels are
//! often the human-facing activity names.
//!
//! # Firing
//!
//! Replay runs on the document compiled to the metamodel (`stochastic::Net`):
//! colors are unfolded, so a trace that supplies the wrong color is a missing
//! token, not a summed pool. Consuming inputs, read and inhibitor arcs come
//! from `Model::inputs` / `outputs` / `tests`, and "can it fire" is
//! `Model::enabled` (which also enforces capacity). Only the token-replay
//! bookkeeping around a refusal (force-fire, count what was missing) lives
//! here, exactly as in go-pflow v0.33 (`rulesFor` / `gateShortfall`): an
//! unmet read arc counts its token shortfall, a blocked inhibitor counts one,
//! and (an addition, since go-pflow's net has no capacities) a firing refused
//! only by a capacity bound counts one.
//!
//! Each case replays from the document's initial marking, as sim does, so
//! the net should be the per-case workflow.

use std::collections::{BTreeMap, HashMap, HashSet};

use pflow_eventlog::EventLog;
use pflow_metamodel::{ArcRef, ArcType};
use serde_json::{json, Value};

use crate::artifact::{ArtifactData, NodeSet, TraceStep};
use crate::doc::Document;
use crate::sim;
use crate::stochastic::Net;
use crate::{Error, Result};

/// Cases one `data.conformance` replays.
pub const MAX_CASES: usize = 100_000;
/// Events one `data.conformance` replays.
pub const MAX_EVENTS: usize = 1_000_000;
/// Events one `data.replay` takes (sim's `maxStreamEvents`).
pub const MAX_STREAM_EVENTS: usize = 100_000;
/// Unobserved transitions a repair may use by default / at most (sim).
pub const DEFAULT_REPAIR_DEPTH: i64 = 4;
pub const MAX_REPAIR_DEPTH: i64 = 64;
/// Markings the repair search may visit (sim's `maxRepairStates`).
const MAX_REPAIR_STATES: usize = 5000;
/// Variant rows shown in the conformance table.
pub const MAX_TABLE_ROWS: usize = 1000;
/// Worst cases listed in the residual block (sim passes 3).
const WORST_CASES: usize = 3;

// ------------------------------------------------------------- mapping

/// How an activity matched its transition (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    Id,
    Label,
    IdIgnoreCase,
    LabelIgnoreCase,
}

impl Match {
    pub fn name(self) -> &'static str {
        match self {
            Match::Id => "id",
            Match::Label => "label",
            Match::IdIgnoreCase => "id (ignoring case)",
            Match::LabelIgnoreCase => "label (ignoring case)",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    Transition {
        id: String,
        by: Match,
    },
    Unknown,
    /// Several transitions match in the deciding tier.
    Ambiguous(Vec<String>),
}

impl Resolution {
    pub fn transition(&self) -> Option<&str> {
        match self {
            Resolution::Transition { id, .. } => Some(id),
            _ => None,
        }
    }
}

/// The one activity → transition rule every data command uses.
pub struct ActivityMap {
    /// (id, label) in document order.
    transitions: Vec<(String, Option<String>)>,
    cache: HashMap<String, Resolution>,
}

impl ActivityMap {
    pub fn new(doc: &Document) -> Self {
        Self {
            transitions: doc
                .transitions
                .iter()
                .map(|t| (t.id.clone(), t.label.clone()))
                .collect(),
            cache: HashMap::new(),
        }
    }

    /// Resolves an activity (cached).
    pub fn resolve(&mut self, activity: &str) -> Resolution {
        if let Some(r) = self.cache.get(activity) {
            return r.clone();
        }
        let r = self.resolve_uncached(activity);
        self.cache.insert(activity.to_string(), r.clone());
        r
    }

    fn resolve_uncached(&self, activity: &str) -> Resolution {
        let lower = activity.to_lowercase();
        let hit = |by: Match, (id, label): &(String, Option<String>)| match by {
            Match::Id => id == activity,
            Match::Label => label.as_deref() == Some(activity),
            Match::IdIgnoreCase => id.to_lowercase() == lower,
            Match::LabelIgnoreCase => label.as_ref().is_some_and(|l| l.to_lowercase() == lower),
        };
        for by in [
            Match::Id,
            Match::Label,
            Match::IdIgnoreCase,
            Match::LabelIgnoreCase,
        ] {
            let hits: Vec<String> = self
                .transitions
                .iter()
                .filter(|t| hit(by, t))
                .map(|(id, _)| id.clone())
                .collect();
            match hits.len() {
                0 => continue,
                1 => {
                    return Resolution::Transition {
                        id: hits.into_iter().next().unwrap(),
                        by,
                    }
                }
                _ => return Resolution::Ambiguous(hits),
            }
        }
        Resolution::Unknown
    }

    /// What every activity seen so far resolved to, sorted by activity.
    pub fn report(&self) -> Value {
        let mut keys: Vec<&String> = self.cache.keys().collect();
        keys.sort();
        let mut out = serde_json::Map::new();
        for k in keys {
            let v = match &self.cache[k] {
                Resolution::Transition { id, by } => json!({"transition": id, "by": by.name()}),
                Resolution::Unknown => json!({"unknown": true}),
                Resolution::Ambiguous(ids) => json!({"ambiguous": ids}),
            };
            out.insert(k.clone(), v);
        }
        Value::Object(out)
    }

    /// Caveats for activities that did not match by id.
    pub fn caveats(&self) -> Vec<String> {
        let mut keys: Vec<&String> = self.cache.keys().collect();
        keys.sort();
        let mut out = vec![];
        for k in keys {
            match &self.cache[k] {
                Resolution::Transition { id, by } if *by != Match::Id => out.push(format!(
                    "activity {k:?} matched transition {id:?} by {}",
                    by.name()
                )),
                Resolution::Ambiguous(ids) => out.push(format!(
                    "activity {k:?} is ambiguous (matches {ids:?}); replayed as unknown"
                )),
                _ => {}
            }
        }
        out
    }
}

// ------------------------------------------------------------- the net

/// One transition's arcs, classified by the metamodel's firing rule.
struct Rules {
    inputs: Vec<ArcRef>,
    outputs: Vec<ArcRef>,
    tests: Vec<ArcRef>,
}

/// The compiled net plus what replay needs, built once per command.
pub struct Replayer<'a> {
    pub net: Net,
    doc: &'a Document,
    pub map: ActivityMap,
    rules: HashMap<String, Rules>,
    /// Initial model marking over the token places.
    pub initial: HashMap<String, i64>,
    /// Token places nothing consumes from: where finished cases rest.
    finals: HashSet<String>,
}

pub type ModelMarking = HashMap<String, i64>;

impl<'a> Replayer<'a> {
    pub fn new(doc: &'a Document) -> Result<Self> {
        let net = Net::new(doc)?;
        let mut rules = HashMap::new();
        let mut consumed: HashSet<String> = HashSet::new();
        for t in &net.model.transitions {
            let r = Rules {
                inputs: net.model.inputs(&t.id),
                outputs: net.model.outputs(&t.id),
                tests: net.model.tests(&t.id),
            };
            consumed.extend(r.inputs.iter().map(|a| a.place.clone()));
            rules.insert(t.id.clone(), r);
        }
        let finals = net
            .places
            .iter()
            .filter(|p| !consumed.contains(*p))
            .cloned()
            .collect();
        let initial = net.model_marking(&sim::initial_marking(doc));
        Ok(Self {
            net,
            doc,
            map: ActivityMap::new(doc),
            rules,
            initial,
            finals,
        })
    }

    fn base(&self, model_place: &str) -> String {
        self.net.base(model_place).0
    }

    /// A model marking as the document's per-color marking (every place).
    pub fn doc_marking(&self, mk: &ModelMarking) -> BTreeMap<String, Vec<i64>> {
        let n = self.doc.colors();
        let mut out: BTreeMap<String, Vec<i64>> = self
            .doc
            .places
            .iter()
            .map(|p| (p.id.clone(), vec![0; n]))
            .collect();
        for p in &self.net.places {
            let (base, c) = self.net.base(p);
            if let Some(slot) = out.get_mut(&base).and_then(|v| v.get_mut(c)) {
                *slot = mk.get(p).copied().unwrap_or(0);
            }
        }
        out
    }

    /// Non-zero entries by display name (`place` or `place.color`).
    fn display_marking(&self, mk: &ModelMarking) -> BTreeMap<String, i64> {
        self.net
            .places
            .iter()
            .filter_map(|p| {
                let v = mk.get(p).copied().unwrap_or(0);
                (v != 0).then(|| (self.net.display_of(p), v))
            })
            .collect()
    }

    /// Token-replay firing of `t` at `mk` (go-pflow's `fireTransition`):
    /// returns `(missing, consumed, produced)` and the per-place shortfall,
    /// force-firing when not enabled.
    fn force_fire(
        &self,
        t: &str,
        mk: &mut ModelMarking,
        short_by_place: &mut BTreeMap<String, i64>,
    ) -> (i64, i64, i64) {
        let Some(r) = self.rules.get(t) else {
            return (1, 0, 0);
        };
        let enabled = self.net.model.enabled(t, mk);
        let mut missing = 0;
        let mut consumed = 0;
        for a in &r.inputs {
            let have = mk.get(&a.place).copied().unwrap_or(0);
            consumed += a.weight;
            if have < a.weight {
                missing += a.weight - have;
                *short_by_place.entry(self.base(&a.place)).or_insert(0) += a.weight - have;
            }
        }
        for a in &r.tests {
            let have = mk.get(&a.place).copied().unwrap_or(0);
            let short = match a.typ {
                ArcType::Inhibitor => i64::from(have >= a.weight),
                _ => (a.weight - have).max(0),
            };
            if short > 0 {
                missing += short;
                *short_by_place.entry(self.base(&a.place)).or_insert(0) += short;
            }
        }
        if missing == 0 && !enabled {
            // Refused by a capacity bound only: one unmet requirement.
            missing = 1;
        }
        for a in &r.inputs {
            let e = mk.entry(a.place.clone()).or_insert(0);
            *e = (*e - a.weight).max(0);
        }
        let mut produced = 0;
        for a in &r.outputs {
            *mk.entry(a.place.clone()).or_insert(0) += a.weight;
            produced += a.weight;
        }
        (missing, consumed, produced)
    }
}

// ------------------------------------------------------- whole-log replay

/// The replay of one variant (every case with that activity sequence
/// replays identically from the initial marking).
#[derive(Debug, Clone, Default)]
struct VariantReplay {
    fitness: f64,
    fitting: bool,
    missing: i64,
    remaining: i64,
    produced: i64,
    consumed: i64,
    /// Tokens missing at each step (sim's residual innovation).
    step_missing: Vec<i64>,
    first_deviation: Option<usize>,
    unknown: Vec<String>,
    missing_by_place: BTreeMap<String, i64>,
    remaining_by_place: BTreeMap<String, i64>,
    deviant_by_transition: BTreeMap<String, i64>,
    fired: BTreeMap<String, i64>,
}

fn replay_variant(rp: &mut Replayer, activities: &[String]) -> VariantReplay {
    let mut v = VariantReplay::default();
    let mut mk = rp.initial.clone();
    v.produced = mk.values().sum();
    for (i, act) in activities.iter().enumerate() {
        let missing = match rp.map.resolve(act) {
            Resolution::Transition { id, .. } => {
                let mut short = BTreeMap::new();
                let (m, c, p) = rp.force_fire(&id, &mut mk, &mut short);
                v.consumed += c;
                v.produced += p;
                *v.fired.entry(id.clone()).or_insert(0) += 1;
                for (k, n) in short {
                    *v.missing_by_place.entry(k).or_insert(0) += n;
                }
                if m > 0 {
                    *v.deviant_by_transition.entry(id).or_insert(0) += 1;
                }
                m
            }
            _ => {
                v.unknown.push(act.clone());
                1
            }
        };
        v.missing += missing;
        v.step_missing.push(missing);
        if missing > 0 && v.first_deviation.is_none() {
            v.first_deviation = Some(i);
        }
    }
    // Tokens resting in a final place mean the case completed; elsewhere
    // they are remaining. A net with finals that the case never reached is
    // unfinished: one more remaining (go-pflow).
    let mut completed = false;
    for p in &rp.net.places {
        let n = mk.get(p).copied().unwrap_or(0);
        if n <= 0 {
            continue;
        }
        if rp.finals.contains(p) {
            completed = true;
        } else {
            v.remaining += n;
            *v.remaining_by_place.entry(rp.base(p)).or_insert(0) += n;
        }
    }
    if !rp.finals.is_empty() && !completed {
        v.remaining += 1;
    }
    v.fitness = token_fitness(v.missing, v.consumed, v.remaining, v.produced);
    v.fitting = v.missing == 0 && v.remaining == 0;
    v
}

/// go-pflow's `0.5 (1 - missing/consumed) + 0.5 (1 - remaining/produced)`,
/// floored at 0; 1 when nothing was consumed or produced.
fn token_fitness(missing: i64, consumed: i64, remaining: i64, produced: i64) -> f64 {
    if consumed > 0 && produced > 0 {
        let f = 0.5 * (1.0 - missing as f64 / consumed as f64)
            + 0.5 * (1.0 - remaining as f64 / produced as f64);
        f.max(0.0)
    } else {
        1.0
    }
}

/// The variants of a log: (activities, case ids) in first-seen case order
/// (the log's case-id order).
pub fn variants(log: &EventLog, max_cases: usize) -> Vec<(Vec<String>, Vec<String>)> {
    let mut index: HashMap<Vec<String>, usize> = HashMap::new();
    let mut out: Vec<(Vec<String>, Vec<String>)> = vec![];
    for tr in log.traces().into_iter().take(max_cases) {
        let acts = tr.activity_variant();
        match index.get(&acts) {
            Some(&i) => out[i].1.push(tr.case_id.clone()),
            None => {
                index.insert(acts.clone(), out.len());
                out.push((acts, vec![tr.case_id.clone()]));
            }
        }
    }
    out
}

fn marking_key(mk: &ModelMarking) -> String {
    let mut keys: Vec<(&String, &i64)> = mk.iter().filter(|(_, v)| **v > 0).collect();
    keys.sort();
    keys.iter().map(|(k, v)| format!("{k}:{v},")).collect()
}

/// ETC precision (go-pflow `CheckPrecision`): over every state the replay
/// visits, enabled transitions never taken there are escaping edges.
fn precision(rp: &mut Replayer, vars: &[(Vec<String>, Vec<String>)]) -> (f64, usize, usize, usize) {
    let mut taken: HashMap<String, HashSet<String>> = HashMap::new();
    let mut enabled: HashMap<String, Vec<String>> = HashMap::new();
    for (acts, _) in vars {
        let mut mk = rp.initial.clone();
        for act in acts {
            let key = marking_key(&mk);
            if !enabled.contains_key(&key) {
                enabled.insert(key.clone(), rp.net.model.enabled_transitions(&mk));
                taken.insert(key.clone(), HashSet::new());
            }
            if let Some(t) = rp.map.resolve(act).transition().map(str::to_string) {
                taken.get_mut(&key).unwrap().insert(t.clone());
                // Silent firing: consume only what is there.
                if let Some(r) = rp.rules.get(&t) {
                    for a in &r.inputs {
                        let e = mk.entry(a.place.clone()).or_insert(0);
                        if *e >= a.weight {
                            *e -= a.weight;
                        }
                    }
                    for a in &r.outputs {
                        *mk.entry(a.place.clone()).or_insert(0) += a.weight;
                    }
                }
            }
        }
    }
    let (mut total, mut escaping) = (0usize, 0usize);
    for (key, en) in &enabled {
        for t in en {
            total += 1;
            if !taken[key].contains(t) {
                escaping += 1;
            }
        }
    }
    let p = if total > 0 {
        1.0 - escaping as f64 / total as f64
    } else {
        1.0
    };
    (p, escaping, total, enabled.len())
}

/// Everything `data.conformance` stores and returns.
pub struct Conformance {
    pub report: Value,
    pub table: ArtifactData,
    pub sets: ArtifactData,
}

/// Token-replay conformance of `log` against `doc` (sim_conformance).
pub fn check(doc: &Document, log: &EventLog, max_traces: Option<usize>) -> Result<Conformance> {
    let cases = log.num_cases();
    let limit = match max_traces {
        Some(n) if n == 0 || n > MAX_CASES => {
            return Err(Error::BadArgs(format!(
                "max_traces {n} is outside 1..{MAX_CASES}"
            )))
        }
        Some(n) => n,
        None if cases > MAX_CASES => {
            return Err(Error::BadArgs(format!(
                "the log has {cases} cases, over the {MAX_CASES} cap; pass max_traces to replay the first ones"
            )))
        }
        None => cases,
    };
    let vars = variants(log, limit);
    let replayed: usize = vars.iter().map(|(_, c)| c.len()).sum();
    let events: usize = vars.iter().map(|(a, c)| a.len() * c.len()).sum();
    if events > MAX_EVENTS {
        return Err(Error::BadArgs(format!(
            "{events} events to replay, over the {MAX_EVENTS} cap; lower max_traces"
        )));
    }
    if doc.transitions.is_empty() {
        return Err(Error::Invalid(
            "the net has no transitions to replay".into(),
        ));
    }
    let mut rp = Replayer::new(doc)?;
    let reps: Vec<VariantReplay> = vars
        .iter()
        .map(|(acts, _)| replay_variant(&mut rp, acts))
        .collect();

    // Aggregates, weighted by cases per variant.
    let (mut produced, mut consumed, mut missing, mut remaining) = (0i64, 0i64, 0i64, 0i64);
    let (mut fitting, mut fit_sum) = (0usize, 0.0f64);
    let mut fired: BTreeMap<String, i64> = BTreeMap::new();
    let mut miss_place: BTreeMap<String, i64> = BTreeMap::new();
    let mut rem_place: BTreeMap<String, i64> = BTreeMap::new();
    let mut dev_trans: BTreeMap<String, i64> = BTreeMap::new();
    let mut unknown: BTreeMap<String, i64> = BTreeMap::new();
    for ((_, ids), r) in vars.iter().zip(&reps) {
        let k = ids.len() as i64;
        produced += k * r.produced;
        consumed += k * r.consumed;
        missing += k * r.missing;
        remaining += k * r.remaining;
        if r.fitting {
            fitting += ids.len();
        }
        fit_sum += k as f64 * r.fitness;
        for (m, src) in [
            (&mut fired, &r.fired),
            (&mut miss_place, &r.missing_by_place),
            (&mut rem_place, &r.remaining_by_place),
            (&mut dev_trans, &r.deviant_by_transition),
        ] {
            for (key, n) in src {
                *m.entry(key.clone()).or_insert(0) += k * n;
            }
        }
        for a in &r.unknown {
            *unknown.entry(a.clone()).or_insert(0) += k;
        }
    }
    let fitness = if replayed == 0 {
        1.0
    } else if consumed > 0 && produced > 0 {
        0.5 * (1.0 - missing as f64 / consumed as f64)
            + 0.5 * (1.0 - remaining as f64 / produced as f64)
    } else {
        0.0
    };
    let (prec, escaping, total_enabled, states) = precision(&mut rp, &vars);
    let f_score = if fitness + prec > 0.0 {
        2.0 * fitness * prec / (fitness + prec)
    } else {
        0.0
    };
    // Generalization (Buijs et al.): 1 - mean over transitions of
    // 1/sqrt(executions); a transition never executed counts 1.
    let generalization = 1.0
        - doc
            .transitions
            .iter()
            .map(|t| match fired.get(&t.id) {
                Some(&n) if n > 0 => 1.0 / (n as f64).sqrt(),
                _ => 1.0,
            })
            .sum::<f64>()
            / doc.transitions.len() as f64;
    // Simplicity (arc degree): 1 / (1 + max(0, mean node degree - 2)).
    let nodes = (doc.places.len() + doc.transitions.len()).max(1) as f64;
    let mean_degree = 2.0 * doc.arcs.len() as f64 / nodes;
    let simplicity = 1.0 / (1.0 + (mean_degree - 2.0).max(0.0));
    let token_fit = if consumed > 0 {
        1.0 - missing as f64 / consumed as f64
    } else {
        1.0
    };

    let residual = residual(&vars, &reps, &unknown, replayed);
    let mut caveats = rp.map.caveats();
    caveats.push(
        "each case replays from the document's initial marking: a net whose resource places are shared across cases will not fit"
            .into(),
    );
    caveats.extend(rp.net.model.gating().into_iter().filter(|g| !g.is_empty()));
    let report = json!({
        "fitness": fitness,
        "precision": prec,
        "generalization": generalization,
        "simplicity": simplicity,
        "f_score": f_score,
        "token_fitness": token_fit,
        "fitting_traces": fitting,
        "total_traces": replayed,
        "log_cases": cases,
        "fitting_percent": if replayed > 0 { fitting as f64 / replayed as f64 * 100.0 } else { 0.0 },
        "avg_trace_fitness": if replayed > 0 { fit_sum / replayed as f64 } else { 0.0 },
        "variants": vars.len(),
        "events": events,
        "produced": produced,
        "consumed": consumed,
        "missing": missing,
        "remaining": remaining,
        "escaping_edges": escaping,
        "enabled_edges": total_enabled,
        "unique_states": states,
        "mean_arc_degree": mean_degree,
        "unknown_activities": unknown,
        "missing_by_place": miss_place,
        "remaining_by_place": rem_place,
        "deviations_by_transition": dev_trans,
        "executions": fired,
        "mapping": rp.map.report(),
        "residual": residual,
        "caveats": caveats,
    });

    // Table: one row per variant, most frequent first.
    let mut order: Vec<usize> = (0..vars.len()).collect();
    order.sort_by(|&a, &b| vars[b].1.len().cmp(&vars[a].1.len()).then(a.cmp(&b)));
    let rows = order
        .iter()
        .take(MAX_TABLE_ROWS)
        .enumerate()
        .map(|(rank, &i)| {
            let ((acts, ids), r) = (&vars[i], &reps[i]);
            vec![
                json!(rank + 1),
                json!(ids.len()),
                json!(ids[0]),
                json!(acts.len()),
                json!(r.fitness),
                json!(r.fitting),
                json!(r.missing),
                json!(r.remaining),
                r.first_deviation.map_or(Value::Null, |d| json!(d)),
                r.first_deviation.map_or(Value::Null, |d| json!(acts[d])),
                json!(acts.join(" > ")),
            ]
        })
        .collect();
    let table = ArtifactData::Table {
        columns: [
            "variant",
            "cases",
            "example_case",
            "length",
            "fitness",
            "fitting",
            "missing",
            "remaining",
            "first_deviation",
            "deviating_activity",
            "activities",
        ]
        .iter()
        .map(|c| c.to_string())
        .collect(),
        rows,
    };

    // Sets: where the replay went wrong, for the canvas highlight.
    let as_f = |m: &BTreeMap<String, i64>| -> BTreeMap<String, f64> {
        m.iter()
            .filter(|(_, v)| **v > 0)
            .map(|(k, v)| (k.clone(), *v as f64))
            .collect()
    };
    let mut all: BTreeMap<String, f64> = as_f(&dev_trans);
    for (k, v) in as_f(&miss_place).into_iter().chain(as_f(&rem_place)) {
        *all.entry(k).or_insert(0.0) += v;
    }
    let set = |label: &str, w: BTreeMap<String, f64>, note: &str| NodeSet {
        label: label.into(),
        nodes: w.keys().cloned().collect(),
        weights: w,
        note: note.into(),
    };
    let unknown_note = if unknown.is_empty() {
        String::new()
    } else {
        format!(
            "; activities not in the net: {}",
            unknown.keys().cloned().collect::<Vec<_>>().join(", ")
        )
    };
    let sets = ArtifactData::Sets {
        sets: vec![
            set(
                "Residual (deviations)",
                all,
                &format!("transitions that could not fire (count) and places that lacked or kept tokens (tokens){unknown_note}"),
            ),
            set(
                "Missing tokens",
                as_f(&miss_place),
                "tokens a firing needed and did not find, by place",
            ),
            set(
                "Remaining tokens",
                as_f(&rem_place),
                "tokens left outside final places when cases ended, by place",
            ),
            set(
                "Non-conforming transitions",
                as_f(&dev_trans),
                "transitions fired while not enabled, by count",
            ),
        ],
    };
    Ok(Conformance {
        report,
        table,
        sets,
    })
}

/// sim's `LogResidual` block over the replay: the innovation by position
/// along the cases, by activity, and the worst cases.
fn residual(
    vars: &[(Vec<String>, Vec<String>)],
    reps: &[VariantReplay],
    unknown: &BTreeMap<String, i64>,
    total_cases: usize,
) -> Value {
    // position -> (cases, deviant, missing sum over deviant)
    let mut pos: BTreeMap<usize, (usize, usize, f64)> = BTreeMap::new();
    let mut act: BTreeMap<String, (usize, usize, f64)> = BTreeMap::new();
    let mut deviant_cases = 0usize;
    let mut max_missing = 0i64;
    let mut worst: Vec<(i64, usize)> = vec![];
    for (i, ((acts, ids), r)) in vars.iter().zip(reps).enumerate() {
        let k = ids.len();
        for (j, (a, &m)) in acts.iter().zip(&r.step_missing).enumerate() {
            let p = pos.entry(j).or_default();
            let e = act.entry(a.clone()).or_default();
            p.0 += k;
            e.0 += k;
            if m > 0 {
                p.1 += k;
                p.2 += (k as i64 * m) as f64;
                e.1 += k;
                e.2 += (k as i64 * m) as f64;
                max_missing = max_missing.max(m);
            }
        }
        let total: i64 = r.step_missing.iter().sum();
        if total > 0 {
            deviant_cases += k;
            worst.push((total, i));
        }
    }
    let by_position: Vec<Value> = pos
        .iter()
        .map(|(i, (c, d, m))| {
            json!({"index": i, "cases": c, "deviant": d,
                   "mean_missing": if *d > 0 { m / *d as f64 } else { 0.0 }, "holds": *d == 0})
        })
        .collect();
    let mut by_activity: Vec<(String, usize, usize, f64)> = act
        .into_iter()
        .map(|(a, (n, d, m))| (a, n, d, if d > 0 { m / d as f64 } else { 0.0 }))
        .collect();
    by_activity.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    let by_activity: Vec<Value> = by_activity
        .into_iter()
        .map(|(a, n, d, m)| {
            json!({"activity": a, "attempts": n, "deviant": d, "mean_missing": m,
                   "unknown": unknown.contains_key(&a)})
        })
        .collect();
    worst.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut worst_cases = vec![];
    for (_, i) in &worst {
        for id in &vars[*i].1 {
            if worst_cases.len() >= WORST_CASES {
                break;
            }
            worst_cases.push(json!({
                "case": id,
                "steps": vars[*i].0.iter().zip(&reps[*i].step_missing).enumerate()
                    .map(|(j, (a, m))| json!({"step": j, "label": a, "residual": m, "holds": *m == 0}))
                    .collect::<Vec<_>>(),
            }));
        }
    }
    // Breaks: maximal runs of positions where some case deviates; distance
    // is the share of cases deviating there.
    let mut breaks: Vec<Value> = vec![];
    let mut cur: Option<(usize, usize, f64, usize)> = None;
    let mut holding = 0usize;
    let flush = |cur: &mut Option<(usize, usize, f64, usize)>, breaks: &mut Vec<Value>| {
        if let Some((from, to, max, at)) = cur.take() {
            breaks.push(json!({"from": from, "to": to, "max_distance": max, "at": at}));
        }
    };
    for (i, (c, d, _)) in &pos {
        if *d == 0 {
            holding += 1;
            flush(&mut cur, &mut breaks);
            continue;
        }
        let frac = *d as f64 / *c as f64;
        match &mut cur {
            Some(b) => {
                b.1 = *i;
                if frac >= b.2 {
                    b.2 = frac;
                    b.3 = *i;
                }
            }
            None => cur = Some((*i, *i, frac, *i)),
        }
    }
    flush(&mut cur, &mut breaks);
    let (verdict, reading) = if total_cases == 0 {
        ("indeterminate", "no cases to compare".to_string())
    } else if deviant_cases == 0 {
        (
            "agree",
            format!("fidelity holds at all {} positions", pos.len()),
        )
    } else if deviant_cases == total_cases {
        let b = &breaks[0];
        (
            "disagree",
            format!(
                "every case deviates; first at position {} (up to {:.0}% of cases at position {})",
                b["from"],
                100.0 * b["max_distance"].as_f64().unwrap_or(0.0),
                b["at"]
            ),
        )
    } else {
        let b = &breaks[0];
        (
            "split",
            format!(
                "{deviant_cases} of {total_cases} cases deviate, first at position {} (up to {:.0}% of cases at position {}); the rest replay cleanly",
                b["from"],
                100.0 * b["max_distance"].as_f64().unwrap_or(0.0),
                b["at"]
            ),
        )
    };
    json!({
        "supported": true,
        "by_position": by_position,
        "by_activity": by_activity,
        "worst_cases": worst_cases,
        "summary": {"verdict": verdict, "holding": holding, "total": pos.len(), "breaks": breaks, "reading": reading},
        "max_missing": max_missing,
        "total_cases": total_cases,
        "deviant_cases": deviant_cases,
    })
}

// ---------------------------------------------------------- online replay

/// One observed event of a stream.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamEvent {
    pub case: String,
    pub activity: String,
    pub timestamp: Option<String>,
}

/// What `data.replay` stores and returns.
pub struct StreamOutcome {
    pub report: Value,
    /// Running (event fitness, token fitness) after each event.
    pub running: Vec<(f64, f64)>,
    /// The traced case and its replay: the initial marking, then one step
    /// per event of that case.
    pub case: String,
    pub trace: Vec<TraceStep>,
}

/// Online token replay of an ordered event stream (sim's `CheckStream`):
/// per-case markings from the initial marking, the first divergence with
/// its blocking places and a bounded repair, and force-firing so a case
/// resynchronises. `trace_case` picks the case whose replay becomes the
/// trace; by default the first divergent one, else the first case.
/// `repair_depth` ≤ 0 turns repair off.
pub fn stream(
    doc: &Document,
    events: &[StreamEvent],
    trace_case: Option<&str>,
    repair_depth: i64,
) -> Result<StreamOutcome> {
    if events.is_empty() {
        return Err(Error::BadArgs("the stream has no events".into()));
    }
    if events.len() > MAX_STREAM_EVENTS {
        return Err(Error::BadArgs(format!(
            "the stream has {} events; the limit is {MAX_STREAM_EVENTS}",
            events.len()
        )));
    }
    if repair_depth > MAX_REPAIR_DEPTH {
        return Err(Error::BadArgs(format!(
            "repair_depth {repair_depth} is over the limit of {MAX_REPAIR_DEPTH}"
        )));
    }
    for (i, e) in events.iter().enumerate() {
        if e.case.is_empty() || e.activity.is_empty() {
            return Err(Error::BadArgs(format!(
                "event {i} needs both a case and an activity"
            )));
        }
    }
    let mut rp = Replayer::new(doc)?;
    let mut markings: HashMap<String, ModelMarking> = HashMap::new();
    let mut cases: Vec<String> = vec![];
    let (mut good, mut consumed, mut missing) = (0usize, 0i64, 0i64);
    let mut divergences = 0usize;
    let mut first: Option<Value> = None;
    let mut by_transition: BTreeMap<String, usize> = BTreeMap::new();
    let mut running = Vec::with_capacity(events.len());
    for (idx, ev) in events.iter().enumerate() {
        let mk = markings.entry(ev.case.clone()).or_insert_with(|| {
            cases.push(ev.case.clone());
            rp.initial.clone()
        });
        let res = rp.map.resolve(&ev.activity);
        let div: Option<Value> = match &res {
            Resolution::Transition { id, .. } if rp.net.model.enabled(id, mk) => {
                consumed += rp
                    .net
                    .model
                    .inputs(id)
                    .iter()
                    .map(|a| a.weight)
                    .sum::<i64>();
                *mk = rp.net.model.fire(id, mk);
                None
            }
            Resolution::Transition { id, .. } => {
                let before = mk.clone();
                let reason = match rp.net.model.enabled_why_not(id, mk) {
                    Err(e) => e.to_string(),
                    Ok(()) => String::new(),
                };
                let places = shortfalls(&rp, id, mk);
                let repair = if repair_depth > 0 {
                    repair_path(&rp, id, mk, repair_depth as usize)
                } else {
                    None
                };
                // Token replay's forced firing (sim counts input tokens only).
                for a in &rp.net.model.inputs(id) {
                    consumed += a.weight;
                    let have = mk.get(&a.place).copied().unwrap_or(0);
                    if have < a.weight {
                        missing += a.weight - have;
                    }
                }
                let mut short = BTreeMap::new();
                rp.force_fire(id, mk, &mut short);
                Some(json!({
                    "kind": "not_enabled",
                    "transition": id,
                    "reason": reason,
                    "places": places,
                    "marking": rp.display_marking(&before),
                    "repair": repair,
                }))
            }
            Resolution::Unknown => Some(json!({
                "kind": "unknown_activity",
                "reason": format!("no transition {:?} in the net (by id or label)", ev.activity),
                "marking": rp.display_marking(mk),
            })),
            Resolution::Ambiguous(ids) => Some(json!({
                "kind": "ambiguous_activity",
                "reason": format!("activity {:?} matches several transitions: {ids:?}", ev.activity),
                "marking": rp.display_marking(mk),
            })),
        };
        match div {
            None => good += 1,
            Some(mut d) => {
                divergences += 1;
                *by_transition.entry(ev.activity.clone()).or_insert(0) += 1;
                if first.is_none() {
                    d["index"] = json!(idx);
                    d["case"] = json!(ev.case);
                    d["activity"] = json!(ev.activity);
                    if let Some(t) = &ev.timestamp {
                        d["timestamp"] = json!(t);
                    }
                    first = Some(d);
                }
            }
        }
        let fit = good as f64 / (idx + 1) as f64;
        let tok = if consumed > 0 {
            1.0 - missing as f64 / consumed as f64
        } else {
            1.0
        };
        running.push((fit, tok));
    }
    let (fitness, token_fitness) = *running.last().expect("non-empty");

    // The traced case, replayed alone (cases do not interact).
    let case = match trace_case {
        Some(c) => {
            if !cases.iter().any(|x| x == c) {
                return Err(Error::NotFound(format!("no case {c:?} in the stream")));
            }
            c.to_string()
        }
        None => first
            .as_ref()
            .and_then(|d| d["case"].as_str().map(str::to_string))
            .unwrap_or_else(|| cases[0].clone()),
    };
    let mut mk = rp.initial.clone();
    let mut trace = vec![TraceStep {
        transition: String::new(),
        marking: rp.doc_marking(&mk),
    }];
    for ev in events.iter().filter(|e| e.case == case) {
        let label = match rp.map.resolve(&ev.activity) {
            Resolution::Transition { id, .. } => {
                let mut short = BTreeMap::new();
                rp.force_fire(&id, &mut mk, &mut short);
                id
            }
            _ => ev.activity.clone(),
        };
        trace.push(TraceStep {
            transition: label,
            marking: rp.doc_marking(&mk),
        });
    }

    let mut caveats = rp.map.caveats();
    caveats.extend(rp.net.model.gating().into_iter().filter(|g| !g.is_empty()));
    caveats.push(
        "each case replays from the document's initial marking: a net with resource places shared across cases will report false divergences"
            .into(),
    );
    caveats.push(
        "event order is the stream's order; timestamps are not checked against delays".into(),
    );
    let report = json!({
        "events": events.len(),
        "cases": cases.len(),
        "conforming": first.is_none(),
        "fitness": fitness,
        "token_fitness": token_fitness,
        "divergences": divergences,
        "first_divergence": first,
        "divergences_by_transition": by_transition,
        "traced_case": case,
        "mapping": rp.map.report(),
        "caveats": caveats,
    });
    Ok(StreamOutcome {
        report,
        running,
        case,
        trace,
    })
}

/// The places that blocked `t` (diagnosis only; `Model::enabled` decides).
fn shortfalls(rp: &Replayer, t: &str, mk: &ModelMarking) -> Vec<Value> {
    let Some(r) = rp.rules.get(t) else {
        return vec![];
    };
    let have = |p: &str| mk.get(p).copied().unwrap_or(0);
    let entry = |a: &ArcRef, role: &str| {
        json!({"place": rp.net.display_of(&a.place), "node": rp.base(&a.place),
               "need": a.weight, "have": have(&a.place), "role": role})
    };
    let mut out = vec![];
    for a in &r.inputs {
        if have(&a.place) < a.weight {
            out.push(entry(a, "input"));
        }
    }
    for a in &r.tests {
        match a.typ {
            ArcType::Inhibitor if have(&a.place) >= a.weight => out.push(entry(a, "inhibitor")),
            ArcType::Read if have(&a.place) < a.weight => out.push(entry(a, "read")),
            _ => {}
        }
    }
    out
}

/// sim's local repair: the shortest firing sequence from `mk` after which
/// `target` is enabled (breadth-first, transitions in declaration order).
fn repair_path(
    rp: &Replayer,
    target: &str,
    mk: &ModelMarking,
    depth: usize,
) -> Option<Vec<String>> {
    let model = &rp.net.model;
    let mut seen: HashSet<String> = HashSet::from([marking_key(mk)]);
    let mut frontier: Vec<(ModelMarking, Vec<String>)> = vec![(mk.clone(), vec![])];
    for _ in 0..depth {
        let mut next = vec![];
        for (m, path) in &frontier {
            for t in model.enabled_transitions(m) {
                let nm = model.fire(&t, m);
                if !seen.insert(marking_key(&nm)) {
                    continue;
                }
                let mut p = path.clone();
                p.push(t);
                if model.enabled(target, &nm) {
                    return Some(p);
                }
                next.push((nm, p));
            }
        }
        if next.is_empty() || seen.len() > MAX_REPAIR_STATES {
            return None;
        }
        frontier = next;
    }
    None
}

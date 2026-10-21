//! Logs from nets and nets from logs: synthetic event logs by seeded SSA
//! playouts of the current net (sim.pflow.xyz's `sim_dataset`), and process
//! discovery with pflow-mining (alpha, heuristic, sequential, common path).

use std::collections::{BTreeMap, HashMap};

use pflow_eventlog::{Event, EventLog};
use pflow_solver::ssa::{self, Xoshiro256};
use serde_json::{json, Value};

use crate::doc::{Arc, Document, Place, Transition};
use crate::sim;
use crate::stochastic::{self as st, Net};
use crate::{Error, Result};

/// Cases per `data.generate` (sim's MCP cap).
pub const MAX_CASES: usize = 2000;
/// Firings per case, default and cap (sim's default is 200 events a case).
pub const DEFAULT_MAX_STEPS: usize = 200;
pub const MAX_STEPS: usize = 10_000;
/// Events per generated log.
pub const MAX_EVENTS: usize = 1_000_000;
/// Wall-clock time of model time 0 for the first case: sim's eventgen
/// `DefaultEpoch`, 2026-01-05T08:00:00Z, in milliseconds since the epoch.
pub const EPOCH_MS: i64 = 1_767_600_000_000;
/// One model time unit is one hour of log time (sim's `TimeUnit`).
pub const TIME_UNIT_MS: f64 = 3_600_000.0;
/// Activities the alpha miner accepts (its place search is over subsets).
pub const MAX_ALPHA_ACTIVITIES: usize = 12;
/// Activities any miner accepts.
pub const MAX_ACTIVITIES: usize = 500;

/// A transition's consumed and produced (place index, weight).
type Flow = (Vec<(usize, i64)>, Vec<(usize, i64)>);

pub struct Generated {
    pub log: EventLog,
    pub events: usize,
    pub deadlocked: usize,
    pub capped: usize,
    /// Model time summed over every case.
    pub model_time: f64,
}

/// A synthetic log: `cases` independent seeded SSA playouts of `doc` from
/// its initial marking, case `k` (from 1, id `case-0001`) drawing from the
/// portable xoshiro256** stream seeded `seed + k - 1`. A case ends when no
/// transition can fire (deadlock) or after `max_steps` firings. Every firing
/// (a timer's completion, for a delayed transition) is one `complete` event
/// whose activity is the transition id. Case k starts where case k-1 ended;
/// time is [`EPOCH_MS`] plus model time in hours, rounded to the
/// millisecond and kept strictly increasing within a case so the log
/// re-reads in firing order.
pub fn generate(
    doc: &Document,
    rates: &BTreeMap<String, f64>,
    cases: usize,
    seed: u64,
    max_steps: usize,
) -> Result<Generated> {
    if !(1..=MAX_CASES).contains(&cases) {
        return Err(Error::BadArgs(format!(
            "cases {cases} is outside 1..{MAX_CASES}"
        )));
    }
    if !(1..=MAX_STEPS).contains(&max_steps) {
        return Err(Error::BadArgs(format!(
            "max_steps {max_steps} is outside 1..{MAX_STEPS}"
        )));
    }
    if cases * max_steps > MAX_EVENTS {
        return Err(Error::BadArgs(format!(
            "cases × max_steps = {} is over the {MAX_EVENTS}-event cap; lower one of them",
            cases * max_steps
        )));
    }
    let net = Net::new(doc)?;
    let start = net.model_marking(&sim::initial_marking(doc));
    st::check_tokens(&start, false)?;
    let model = net.ssa_model(&start, rates);
    if model.transitions.is_empty() {
        return Err(Error::Invalid(
            "no transition can fire: every rate is 0 (or the net has none)".into(),
        ));
    }
    let compiled = ssa::compile(&model).map_err(|e| Error::Invalid(format!("ssa: {e}")))?;
    let index: HashMap<&str, usize> = model
        .places
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), i))
        .collect();
    // Per transition (SSA order): consumed and produced (place, weight),
    // classified by the metamodel's firing rule.
    let flows: Vec<Flow> = model
        .transitions
        .iter()
        .map(|t| {
            let side = |arcs: Vec<pflow_metamodel::ArcRef>| {
                arcs.into_iter()
                    .filter_map(|a| index.get(a.place.as_str()).map(|&i| (i, a.weight)))
                    .collect()
            };
            (
                side(net.model.inputs(&t.id)),
                side(net.model.outputs(&t.id)),
            )
        })
        .collect();
    let timed = model.transitions.iter().any(|t| t.delay > 0.0);
    let initial: Vec<i64> = compiled.initial_marking().to_vec();

    let mut log = EventLog::new();
    let (mut events, mut deadlocked, mut capped) = (0usize, 0usize, 0usize);
    let mut offset_ms = EPOCH_MS;
    let mut model_time = 0.0;
    let mut props: Vec<f64> = Vec::with_capacity(model.transitions.len());
    for k in 1..=cases {
        let case = format!("case-{k:04}");
        let mut rng = Xoshiro256::new(seed.wrapping_add(k as u64 - 1));
        let mut mk = initial.clone();
        let mut queue: Vec<(f64, usize)> = vec![];
        let mut t = 0.0f64;
        let mut last_ms = i64::MIN;
        let mut steps = 0usize;
        let log_event = |log: &mut EventLog, j: usize, t: f64, last_ms: &mut i64| {
            let ms = (offset_ms + (t * TIME_UNIT_MS).round() as i64).max(last_ms.saturating_add(1));
            *last_ms = ms;
            let mut e = Event::new(case.clone(), model.transitions[j].id.clone(), ms);
            e.lifecycle = "complete".into();
            log.add_event(e);
        };
        let ended_dead = loop {
            if steps >= max_steps {
                break false;
            }
            if timed {
                start_timers(&net, &model, &flows, &mut mk, &mut queue, t);
            }
            let a0 = compiled.propensities(&mk, &mut props);
            if a0 <= 0.0 && queue.is_empty() {
                break true;
            }
            let mut dt = f64::INFINITY;
            if a0 > 0.0 {
                dt = -ssa::plog(1.0 - rng.uniform()) / a0;
            }
            if let Some(&(at, j)) = queue.first() {
                if at <= t + dt {
                    t = at;
                    queue.remove(0);
                    for &(p, w) in &flows[j].1 {
                        mk[p] += w;
                    }
                    log_event(&mut log, j, t, &mut last_ms);
                    steps += 1;
                    continue;
                }
            }
            t += dt;
            let r = rng.uniform() * a0;
            // Never a zero-propensity transition: rounding past the last
            // cumulative sum picks the last positive one.
            let (mut chosen, mut acc) = (usize::MAX, 0.0);
            for (j, a) in props.iter().enumerate() {
                if *a <= 0.0 {
                    continue;
                }
                chosen = j;
                acc += a;
                if r <= acc {
                    break;
                }
            }
            if chosen == usize::MAX {
                break true;
            }
            for &(p, w) in &flows[chosen].0 {
                mk[p] -= w;
            }
            for &(p, w) in &flows[chosen].1 {
                mk[p] += w;
            }
            log_event(&mut log, chosen, t, &mut last_ms);
            steps += 1;
        };
        if ended_dead {
            deadlocked += 1;
        } else {
            capped += 1;
        }
        events += steps;
        model_time += t;
        if last_ms > i64::MIN {
            offset_ms = last_ms;
        }
    }
    if events == 0 {
        return Err(Error::Invalid(
            "no transition is enabled at the initial marking: the playouts produced no events"
                .into(),
        ));
    }
    log.sort_traces();
    Ok(Generated {
        log,
        events,
        deadlocked,
        capped,
        model_time,
    })
}

/// Starts every enabled delayed transition at `t`, in declaration order,
/// until none is (the portable SSA's timed rule): inputs are taken now,
/// outputs land `delay` later.
fn start_timers(
    net: &Net,
    model: &ssa::SsaModel,
    flows: &[Flow],
    mk: &mut [i64],
    queue: &mut Vec<(f64, usize)>,
    t: f64,
) {
    loop {
        let mut started = false;
        for (j, tr) in model.transitions.iter().enumerate() {
            if tr.delay <= 0.0 {
                continue;
            }
            let m: HashMap<String, i64> = model
                .places
                .iter()
                .zip(mk.iter())
                .map(|(p, v)| (p.id.clone(), *v))
                .collect();
            if !net.model.enabled(&tr.id, &m) {
                continue;
            }
            for &(p, w) in &flows[j].0 {
                mk[p] -= w;
            }
            let at = t + tr.delay;
            let i = queue.iter().take_while(|(a, _)| *a <= at).count();
            queue.insert(i, (at, j));
            started = true;
        }
        if !started {
            break;
        }
    }
}

/// The discovery methods `data.discover` takes.
pub const METHODS: &[&str] = &["alpha", "heuristic", "sequential", "common-path"];

/// A mined net as a document, laid out by `layout` (Sugiyama), and what
/// the miner reported.
pub fn discover(log: &EventLog, method: &str, threshold: Option<f64>) -> Result<(Document, Value)> {
    if log.num_events() == 0 {
        return Err(Error::Invalid("the log has no events".into()));
    }
    let activities = log.activities();
    if activities.len() > MAX_ACTIVITIES {
        return Err(Error::BadArgs(format!(
            "the log has {} activities, over the {MAX_ACTIVITIES} cap",
            activities.len()
        )));
    }
    if threshold.is_some() && method != "heuristic" {
        return Err(Error::BadArgs(
            "threshold applies to the heuristic miner only".into(),
        ));
    }
    let result = match method {
        "alpha" => {
            if activities.len() > MAX_ALPHA_ACTIVITIES {
                return Err(Error::BadArgs(format!(
                    "the alpha miner searches subsets of activities; this log has {}, over its {MAX_ALPHA_ACTIVITIES} cap (use heuristic)",
                    activities.len()
                )));
            }
            pflow_mining::discover_alpha(log)
        }
        "heuristic" => {
            let mut o = pflow_mining::HeuristicMinerOptions::default();
            if let Some(th) = threshold {
                if !(0.0..=1.0).contains(&th) {
                    return Err(Error::BadArgs(format!("threshold {th} is outside [0, 1]")));
                }
                o.dependency_threshold = th;
            }
            pflow_mining::discover_heuristic(log, &o)
        }
        "sequential" | "common-path" => {
            pflow_mining::discover(log, method).map_err(Error::BadArgs)?
        }
        other => {
            return Err(Error::BadArgs(format!(
                "unknown method {other:?}; one of {METHODS:?}"
            )))
        }
    };
    let net = &result.net;
    if let Some(clash) = net.places.keys().find(|p| net.transitions.contains_key(*p)) {
        return Err(Error::Invalid(format!(
            "the miner named a place and a transition {clash:?}; rename that activity"
        )));
    }
    // pflow-core keeps nodes in hash maps: order them by the miner's own
    // coordinates (which follow activity order), then id.
    let mut places: Vec<(&String, &pflow_core::Place)> = net.places.iter().collect();
    places.sort_by(|a, b| {
        (a.1.x, a.1.y)
            .partial_cmp(&(b.1.x, b.1.y))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(b.0))
    });
    let mut transitions: Vec<(&String, &pflow_core::Transition)> = net.transitions.iter().collect();
    transitions.sort_by(|a, b| {
        (a.1.x, a.1.y)
            .partial_cmp(&(b.1.x, b.1.y))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.0.cmp(b.0))
    });
    let mut doc = Document {
        places: places
            .into_iter()
            .map(|(id, p)| Place {
                id: id.clone(),
                label: p.label_text.clone().filter(|l| l != id),
                initial: vec![p.initial.iter().sum::<f64>()],
                capacity: vec![0.0],
                x: p.x,
                y: p.y,
                extra: Default::default(),
            })
            .collect(),
        transitions: transitions
            .into_iter()
            .map(|(id, t)| Transition {
                id: id.clone(),
                label: t.label_text.clone().filter(|l| l != id),
                role: None,
                x: t.x,
                y: t.y,
                extra: Default::default(),
            })
            .collect(),
        ..Default::default()
    };
    for a in &net.arcs {
        let w: f64 = a.weight.iter().sum::<f64>().max(1.0);
        match doc
            .arcs
            .iter_mut()
            .find(|b| b.source == a.source && b.target == a.target)
        {
            Some(b) => b.weight[0] = b.weight[0].max(w),
            None => doc.arcs.push(Arc {
                source: a.source.clone(),
                target: a.target.clone(),
                weight: vec![w],
                inhibit: a.inhibit_transition,
                extra: Default::default(),
            }),
        }
    }
    for (id, x, y) in crate::layout::layout(&doc, crate::layout::Algorithm::Sugiyama) {
        if let Some(p) = doc.places.iter_mut().find(|p| p.id == id) {
            (p.x, p.y) = (x, y);
        } else if let Some(t) = doc.transitions.iter_mut().find(|t| t.id == id) {
            (t.x, t.y) = (x, y);
        }
    }
    let info = json!({
        "method": result.method,
        "variants": result.num_variants,
        "most_common_count": result.most_common_count,
        "coverage_percent": result.coverage_percent,
    });
    Ok((doc, info))
}

//! Stochastic engines over the document: the portable Gillespie SSA and
//! chemical Langevin SDE (`pflow_solver::ssa`), sim.pflow.xyz's scenario
//! engine (`pflow_solver::stochastic`: contention, depletion, time-weighted
//! metrics, rate schedules), conservation laws (Farkas P-invariants) and
//! move evaluation by rollouts.
//!
//! Everything starts from [`Net`]: the document compiled by pflow-parser to
//! the same `pflow_metamodel::Model` the token game fires (colors unfolded,
//! output-side inhibitors as read arcs), with its token places put in
//! document order and named the way `sim::continuous` names them (`place`,
//! or `place.color` on a colored net), so SSA, SDE and ODE series line up.
//!
//! Every run here is deterministic for a seed (the portable xoshiro256** +
//! ported-log sampler) and bounded by the `MAX_*` caps below; over a cap is a
//! refusal, never a clamp, as in sim.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use pflow_core::colors::ColorMap;
use pflow_metamodel::{ArcType, Model, RateSegment};
use pflow_solver::ssa::{self, ArcKind, SsaArc, SsaModel, SsaPlace, SsaTransition};
use pflow_solver::stochastic;
use serde_json::{json, Map, Value};

use crate::doc::Document;
use crate::sim::{self, Compiled, Marking};
use crate::{Error, Result};

/// Realizations (runs, rollouts) per call: sim's per-run ceiling.
pub const MAX_RUNS: usize = 200;
/// Reported sample points.
pub const MAX_SAMPLES: usize = 5000;
/// Horizon in model time.
pub const MAX_HORIZON: f64 = 10_000.0;
/// Horizon of a cross-check (sim's `MaxDiagnoseHours`).
pub const MAX_CROSSCHECK_HORIZON: f64 = 168.0;
/// Tokens in one place / across the net a run may start from.
pub const MAX_PLACE_TOKENS: i64 = 1_000_000;
pub const MAX_TOTAL_TOKENS: i64 = 4_000_000;
/// Scenarios in one comparison.
pub const MAX_COMPARE: usize = 20;
/// Schedule segments summed over every transition.
pub const MAX_SCHEDULE_SEGMENTS: usize = 100;
/// Firings per discrete rollout (`sim.evaluate {depth}`).
pub const MAX_DEPTH: usize = 10_000;
/// SDE Euler-Maruyama grid points (`sim.sde {dt}`).
pub const MAX_SDE_STEPS: usize = 50_000;
/// Rows the Farkas elimination may hold before it gives up.
const MAX_FARKAS_ROWS: usize = 4096;

/// The MCP defaults sim_scenario / sim_compare fill in.
pub const SCENARIO_HOURS: f64 = 8.0;
pub const SCENARIO_SAMPLES: usize = 60;
pub const SCENARIO_REALIZATIONS: usize = 16;
pub const SCENARIO_SEED: u64 = 20_260_809;

/// The document as the stochastic engines see it.
pub struct Net {
    pub model: Model,
    pub colors: Option<ColorMap>,
    /// Token places (model ids) in display order: document place order,
    /// then color.
    pub places: Vec<String>,
    /// Display names aligned with `places`.
    pub display: Vec<String>,
    short: Vec<String>,
}

impl Net {
    pub fn new(doc: &Document) -> Result<Self> {
        let c = Compiled::new(doc)?;
        let short = pflow_core::colors::short_color_names(&doc.token);
        let mut keyed: Vec<((usize, usize, String), String)> = Vec::new();
        for p in c.model.places.iter().filter(|p| p.is_token()) {
            let (base, color) = base_of(c.colors.as_ref(), &p.id);
            let di = doc
                .places
                .iter()
                .position(|q| q.id == base)
                .unwrap_or(usize::MAX);
            keyed.push(((di, color, p.id.clone()), p.id.clone()));
        }
        keyed.sort();
        let mut net = Net {
            model: c.model,
            colors: c.colors,
            places: keyed.into_iter().map(|(_, id)| id).collect(),
            display: vec![],
            short,
        };
        net.display = net.places.iter().map(|id| net.display_of(id)).collect();
        Ok(net)
    }

    /// (base document place, color index) of a model place.
    pub fn base(&self, id: &str) -> (String, usize) {
        base_of(self.colors.as_ref(), id)
    }

    pub fn display_of(&self, id: &str) -> String {
        match self.colors.as_ref().and_then(|cm| cm.base.get(id)) {
            Some(r) => {
                let c = self
                    .short
                    .get(r.color)
                    .cloned()
                    .unwrap_or_else(|| r.color.to_string());
                format!("{}.{}", r.place, c)
            }
            None => id.to_string(),
        }
    }

    /// The session marking as a model marking over the token places.
    pub fn model_marking(&self, mk: &Marking) -> HashMap<String, i64> {
        self.places
            .iter()
            .map(|id| {
                let (base, c) = self.base(id);
                let v = mk.get(&base).and_then(|v| v.get(c)).copied().unwrap_or(0);
                (id.clone(), v)
            })
            .collect()
    }

    /// A place named by a caller: a model id, a display name, or a document
    /// place id that unfolded to exactly one copy.
    pub fn resolve_place(&self, key: &str) -> Result<String> {
        if let Some(i) = self.places.iter().position(|p| p == key) {
            return Ok(self.places[i].clone());
        }
        if let Some(i) = self.display.iter().position(|p| p == key) {
            return Ok(self.places[i].clone());
        }
        let copies: Vec<&String> = self
            .places
            .iter()
            .filter(|p| self.base(p).0 == key)
            .collect();
        match copies.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(Error::NotFound(format!(
                "no token place {key:?}; places are {:?}",
                self.display
            ))),
            _ => Err(Error::BadArgs(format!(
                "place {key:?} holds several colors; name one of {:?}",
                copies
                    .iter()
                    .map(|p| self.display_of(p))
                    .collect::<Vec<_>>()
            ))),
        }
    }

    pub fn has_transition(&self, id: &str) -> bool {
        self.model.transitions.iter().any(|t| t.id == id)
    }

    /// What a continuous engine cannot express (`Model::gating`).
    pub fn gating(&self) -> Vec<String> {
        self.model.gating()
    }

    /// The portable SSA's ordered model, places in display order, starting
    /// at `start`. A transition at rate 0 is left out (0 disables it here,
    /// where the portable SSA would read 0 as its default rate 1).
    pub fn ssa_model(
        &self,
        start: &HashMap<String, i64>,
        rates: &BTreeMap<String, f64>,
    ) -> SsaModel {
        let places = self
            .places
            .iter()
            .map(|id| {
                let cap = self
                    .model
                    .places
                    .iter()
                    .find(|p| &p.id == id)
                    .map(|p| p.capacity.max(0))
                    .unwrap_or(0);
                SsaPlace {
                    id: id.clone(),
                    initial: start.get(id).copied().unwrap_or(0),
                    capacity: cap,
                }
            })
            .collect();
        let mut transitions = vec![];
        let mut arcs = vec![];
        for t in &self.model.transitions {
            let rate = rates.get(&t.id).copied().unwrap_or(1.0);
            if rate <= 0.0 {
                continue;
            }
            transitions.push(SsaTransition {
                id: t.id.clone(),
                rate,
                delay: t.delay.max(0.0),
            });
            let w = |w: i64| w.max(1);
            for a in self.model.inputs(&t.id) {
                arcs.push(SsaArc {
                    from: a.place.clone(),
                    to: t.id.clone(),
                    weight: w(a.weight),
                    kind: ArcKind::Flow,
                    kinetic: a.kinetic,
                });
            }
            for a in self.model.outputs(&t.id) {
                arcs.push(SsaArc {
                    from: t.id.clone(),
                    to: a.place.clone(),
                    weight: w(a.weight),
                    kind: ArcKind::Flow,
                    kinetic: true,
                });
            }
            for a in self.model.tests(&t.id) {
                arcs.push(SsaArc {
                    from: a.place.clone(),
                    to: t.id.clone(),
                    weight: w(a.weight),
                    kind: if a.typ == ArcType::Inhibitor {
                        ArcKind::Inhibitor
                    } else {
                        ArcKind::Read
                    },
                    kinetic: false,
                });
            }
        }
        SsaModel {
            places,
            transitions,
            arcs,
        }
    }
}

fn base_of(cm: Option<&ColorMap>, id: &str) -> (String, usize) {
    match cm.and_then(|cm| cm.base.get(id)) {
        Some(r) => (r.place.clone(), r.color),
        None => (id.to_string(), 0),
    }
}

/// Refuses a starting marking over the token caps. The per-place cap is
/// sim's guard for the scenario engine, whose time-in-state ledger is sized
/// by token count; the portable SSA/SDE only need the total bounded.
pub fn check_tokens(start: &HashMap<String, i64>, per_place: bool) -> Result<()> {
    let mut total = 0i64;
    let mut keys: Vec<&String> = start.keys().collect();
    keys.sort();
    for k in keys {
        let v = start[k];
        if v < 0 {
            return Err(Error::BadArgs(format!(
                "marking for {k:?} is negative ({v})"
            )));
        }
        if per_place && v > MAX_PLACE_TOKENS {
            return Err(Error::BadArgs(format!(
                "marking for {k:?} is {v}, over the {MAX_PLACE_TOKENS} cap"
            )));
        }
        total += v;
    }
    if total > MAX_TOTAL_TOKENS {
        return Err(Error::BadArgs(format!(
            "marking totals {total} tokens, over the {MAX_TOTAL_TOKENS} cap"
        )));
    }
    Ok(())
}

pub fn check_runs(what: &str, n: usize) -> Result<()> {
    if !(1..=MAX_RUNS).contains(&n) {
        return Err(Error::BadArgs(format!(
            "{what} {n} is outside 1..{MAX_RUNS}"
        )));
    }
    Ok(())
}

pub fn check_samples(what: &str, n: usize) -> Result<()> {
    if !(2..=MAX_SAMPLES).contains(&n) {
        return Err(Error::BadArgs(format!(
            "{what} {n} is outside 2..{MAX_SAMPLES}"
        )));
    }
    Ok(())
}

pub fn check_horizon(what: &str, h: f64, max: f64) -> Result<()> {
    if !(h > 0.0 && h <= max) {
        return Err(Error::BadArgs(format!("{what} {h} is outside (0, {max}]")));
    }
    Ok(())
}

/// Mean (and spread) trajectories on a fixed grid, in display order.
#[derive(Debug, Clone)]
pub struct Ensemble {
    pub t: Vec<f64>,
    pub names: Vec<String>,
    pub mean: Vec<Vec<f64>>,
    /// Standard deviation across realizations, present iff runs > 1.
    pub sd: Option<Vec<Vec<f64>>>,
}

impl Ensemble {
    pub fn finals(&self) -> BTreeMap<String, f64> {
        self.names
            .iter()
            .zip(&self.mean)
            .map(|(n, v)| (n.clone(), v.last().copied().unwrap_or(0.0)))
            .collect()
    }
}

/// Gillespie SSA (portable, byte-exact across pflow implementations) from
/// marking `mk`: `runs` realizations seeded `seed, seed+1, ...`, sampled at
/// `samples` evenly spaced times over `[0, t_end]`.
pub fn run_ssa(
    net: &Net,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    t_end: f64,
    samples: usize,
    runs: usize,
    seed: u64,
) -> Result<Ensemble> {
    let start = net.model_marking(mk);
    check_tokens(&start, false)?;
    let model = net.ssa_model(&start, rates);
    let opts = ssa::SsaOptions {
        horizon: t_end,
        samples,
        realizations: runs,
        seed,
    };
    let r = ssa::simulate(&model, &opts).map_err(|e| Error::Invalid(format!("ssa: {e}")))?;
    Ok(Ensemble {
        t: r.times,
        names: net.display.clone(),
        mean: r.values,
        sd: r.stddev,
    })
}

/// Chemical Langevin SDE (Euler-Maruyama, 20 internal steps per sample).
/// Refuses a gated net (read arc, inhibitor, reachable capacity), as
/// go-pflow does.
pub fn run_sde(
    net: &Net,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    t_end: f64,
    samples: usize,
    runs: usize,
    seed: u64,
) -> Result<Ensemble> {
    let start = net.model_marking(mk);
    check_tokens(&start, false)?;
    let model = net.ssa_model(&start, rates);
    let opts = ssa::SdeOptions {
        horizon: t_end,
        samples,
        realizations: runs,
        seed,
    };
    let r = ssa::simulate_sde(&model, &opts).map_err(|e| Error::Invalid(format!("sde: {e}")))?;
    if r.diverged {
        return Err(Error::Invalid(format!("sde refused: {}", r.reason)));
    }
    Ok(Ensemble {
        t: r.times,
        names: net.display.clone(),
        mean: r.values,
        sd: r.stddev,
    })
}

/// The mass-action ODE (tsit5, through `sim::continuous`) from `mk`,
/// interpolated linearly onto `times`. Keyed by display name.
pub fn ode_on_grid(
    doc: &Document,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    times: &[f64],
) -> Result<BTreeMap<String, Vec<f64>>> {
    use pflow_solver::{solve, Options, Problem};
    let t_end = times.last().copied().unwrap_or(0.0);
    if !(t_end > 0.0 && t_end.is_finite()) {
        return Err(Error::BadArgs("horizon must be a positive number".into()));
    }
    let cn = sim::continuous(doc, mk, rates)?;
    let mut opts = Options::default_opts();
    opts.dtmax = opts.dtmax.min(t_end / 50.0).max(opts.dtmin);
    let solver = sim::solver("tsit5")?;
    let prob = Problem::new(cn.net, cn.state, [0.0, t_end], cn.rates);
    let sol = solve(&prob, &solver, &opts);
    let mut out = BTreeMap::new();
    for (label, display) in cn.order {
        let col = sol.get_variable(&label);
        out.insert(display, interpolate(&sol.t, &col, times));
    }
    Ok(out)
}

/// Piecewise-linear `ys(xs)` sampled at `at` (held flat past either end).
pub fn interpolate(xs: &[f64], ys: &[f64], at: &[f64]) -> Vec<f64> {
    let n = xs.len().min(ys.len());
    if n == 0 {
        return vec![0.0; at.len()];
    }
    let mut j = 0usize;
    at.iter()
        .map(|&x| {
            while j + 1 < n && xs[j + 1] < x {
                j += 1;
            }
            if x <= xs[0] {
                return ys[0];
            }
            if j + 1 >= n {
                return ys[n - 1];
            }
            let (x0, x1, y0, y1) = (xs[j], xs[j + 1], ys[j], ys[j + 1]);
            if x1 <= x0 {
                y1
            } else {
                y0 + (y1 - y0) * (x - x0) / (x1 - x0)
            }
        })
        .collect()
}

// ------------------------------------------------------------ scenarios

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Ssa,
    Ode,
}

impl Engine {
    pub fn name(self) -> &'static str {
        match self {
            Engine::Ssa => "ssa",
            Engine::Ode => "ode",
        }
    }
}

/// A sim_scenario question, with sim's MCP defaults filled in.
///
/// Fields, against sim_scenario's input:
///
/// | field | here |
/// |---|---|
/// | `name` | supported |
/// | `marking` | supported: sparse override keyed by place (`place`, or `place.color`); absent places keep the **current session marking** (sim: the model's declared initial marking) |
/// | `rates` | supported: per transition id, over the session rates (`sim.set_rate`); 0 disables |
/// | `schedule` | supported (`{transition: [{until, value}, ...]}`, ssa only, ≤ 100 segments) |
/// | `hours` | supported (default 8, ≤ 10000); `t_end` is accepted as an alias |
/// | `samples` | supported (default 60, 2..5000) |
/// | `realizations` | supported (default 16, ≤ 200) |
/// | `seed` | supported (default 20260809) |
/// | `summary` | supported: no time-series artifact, finals only |
/// | `engine` | `ssa` (default, pflow-solver's stochastic engine) or `ode` (tsit5 mass action; refuses schedules and gated nets, as sim's Forecast does) |
/// | `params` | refused unless empty: pflow.xyz documents declare no structural parameters |
/// | `id` / `model` | refused: the model is always the open document |
///
/// Any other key is refused by name.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: Option<String>,
    /// Model place id → tokens.
    pub marking: BTreeMap<String, i64>,
    pub rates: BTreeMap<String, f64>,
    pub schedule: BTreeMap<String, Vec<RateSegment>>,
    pub hours: f64,
    pub samples: usize,
    pub realizations: usize,
    pub seed: u64,
    /// Whether the scenario set its own seed.
    pub seed_set: bool,
    pub summary: bool,
    pub engine: Engine,
}

const SCENARIO_FIELDS: &[&str] = &[
    "name",
    "marking",
    "rates",
    "schedule",
    "hours",
    "t_end",
    "samples",
    "realizations",
    "seed",
    "summary",
    "engine",
    "params",
];

/// A JSON object, or a string holding one (sim accepts both).
pub fn json_arg(v: &Value, what: &str) -> Result<Value> {
    match v {
        Value::String(s) => serde_json::from_str(s)
            .map_err(|e| Error::BadArgs(format!("{what}: not valid JSON: {e}"))),
        other => Ok(other.clone()),
    }
}

fn num(v: &Value, what: &str) -> Result<f64> {
    v.as_f64()
        .filter(|x| x.is_finite())
        .ok_or_else(|| Error::BadArgs(format!("{what} must be a finite number, got {v}")))
}

fn count(v: &Value, what: &str) -> Result<i64> {
    let x = num(v, what)?;
    if x.fract() != 0.0 {
        return Err(Error::BadArgs(format!(
            "{what} must be an integer, got {v}"
        )));
    }
    Ok(x as i64)
}

pub fn parse_seed(v: &Value, what: &str) -> Result<u64> {
    let s = count(v, what)?;
    if s < 0 {
        return Err(Error::BadArgs(format!("{what} must be ≥ 0, got {s}")));
    }
    Ok(s as u64)
}

/// Validates one scenario against the net (sim's `Scenario.Validate` order:
/// marking, rates, schedule, engine, realizations, hours, samples).
pub fn parse_scenario(v: &Value, net: &Net) -> Result<Scenario> {
    let v = json_arg(v, "scenario")?;
    let obj = v
        .as_object()
        .ok_or_else(|| Error::BadArgs(format!("a scenario must be a JSON object, got {v}")))?;
    for k in obj.keys() {
        if k == "id" || k == "model" {
            return Err(Error::BadArgs(format!(
                "scenario field {k:?} is not supported: the scenario always runs against the open document"
            )));
        }
        if !SCENARIO_FIELDS.contains(&k.as_str()) {
            return Err(Error::BadArgs(format!(
                "unknown scenario field {k:?}; supported: {SCENARIO_FIELDS:?}"
            )));
        }
    }
    if let Some(p) = obj.get("params") {
        if p.as_object().is_some_and(|m| !m.is_empty()) || !(p.is_null() || p.is_object()) {
            return Err(Error::BadArgs(
                "scenario params are not supported: pflow.xyz documents declare no structural parameters (sim's parameters block)".into(),
            ));
        }
    }
    let name = match obj.get("name") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(o) => {
            return Err(Error::BadArgs(format!(
                "scenario name must be a string, got {o}"
            )))
        }
    };

    let mut marking = BTreeMap::new();
    if let Some(m) = obj.get("marking").filter(|m| !m.is_null()) {
        let m = m
            .as_object()
            .ok_or_else(|| Error::BadArgs("scenario marking must be an object".into()))?;
        let mut keys: Vec<&String> = m.keys().collect();
        keys.sort();
        for k in keys {
            let id = net.resolve_place(k)?;
            let n = count(&m[k], &format!("marking for {k:?}"))?;
            if n < 0 {
                return Err(Error::BadArgs(format!(
                    "marking for {k:?} is negative ({n})"
                )));
            }
            if n > MAX_PLACE_TOKENS {
                return Err(Error::BadArgs(format!(
                    "marking for {k:?} is {n}, over the {MAX_PLACE_TOKENS} cap"
                )));
            }
            marking.insert(id, n);
        }
    }

    let mut rates = BTreeMap::new();
    if let Some(r) = obj.get("rates").filter(|r| !r.is_null()) {
        let r = r
            .as_object()
            .ok_or_else(|| Error::BadArgs("scenario rates must be an object".into()))?;
        for (id, x) in r {
            if !net.has_transition(id) {
                return Err(Error::NotFound(format!("no transition {id:?}")));
            }
            let x = x
                .as_f64()
                .filter(|x| x.is_finite() && *x >= 0.0)
                .ok_or_else(|| {
                    Error::BadArgs(format!(
                        "rate for {id:?} must be a finite, non-negative number"
                    ))
                })?;
            rates.insert(id.clone(), x);
        }
    }

    let mut schedule: BTreeMap<String, Vec<RateSegment>> = BTreeMap::new();
    if let Some(s) = obj.get("schedule").filter(|s| !s.is_null()) {
        let s = s
            .as_object()
            .ok_or_else(|| Error::BadArgs("scenario schedule must be an object".into()))?;
        let mut total = 0usize;
        for (id, segs) in s {
            if !net.has_transition(id) {
                return Err(Error::NotFound(format!("no transition {id:?}")));
            }
            let segs: Vec<RateSegment> = serde_json::from_value(segs.clone()).map_err(|e| {
                Error::BadArgs(format!(
                    "schedule for {id:?} must be a list of {{until, value}}: {e}"
                ))
            })?;
            if segs.is_empty() {
                return Err(Error::BadArgs(format!("schedule for {id:?} is empty")));
            }
            for (i, seg) in segs.iter().enumerate() {
                if !seg.value.is_finite() || seg.value < 0.0 {
                    return Err(Error::BadArgs(format!(
                        "schedule for {id:?}: segment {i} needs a finite, non-negative rate"
                    )));
                }
                if i > 0 && seg.until <= segs[i - 1].until {
                    return Err(Error::BadArgs(format!(
                        "schedule for {id:?}: segment {i} ends at {}, not after the previous segment's {}",
                        seg.until,
                        segs[i - 1].until
                    )));
                }
            }
            total += segs.len();
            schedule.insert(id.clone(), segs);
        }
        if total > MAX_SCHEDULE_SEGMENTS {
            return Err(Error::BadArgs(format!(
                "schedule has {total} segments, over the {MAX_SCHEDULE_SEGMENTS} cap"
            )));
        }
    }

    let engine = match obj.get("engine").and_then(Value::as_str) {
        None | Some("") | Some("ssa") => Engine::Ssa,
        Some("ode") => Engine::Ode,
        Some(other) => {
            return Err(Error::BadArgs(format!(
                "unknown engine {other:?} (use ssa or ode)"
            )))
        }
    };
    let realizations = match obj.get("realizations").filter(|v| !v.is_null()) {
        Some(v) => {
            let n = count(v, "realizations")?;
            if n < 0 || n as usize > MAX_RUNS {
                return Err(Error::BadArgs(format!(
                    "realizations {n} is outside 0..{MAX_RUNS}"
                )));
            }
            if n == 0 {
                SCENARIO_REALIZATIONS
            } else {
                n as usize
            }
        }
        None => SCENARIO_REALIZATIONS,
    };
    if obj.contains_key("hours") && obj.contains_key("t_end") {
        return Err(Error::BadArgs("give hours or t_end, not both".into()));
    }
    let hours = match obj
        .get("hours")
        .or_else(|| obj.get("t_end"))
        .filter(|v| !v.is_null())
    {
        Some(v) => {
            let h = num(v, "hours")?;
            if !(0.0..=MAX_HORIZON).contains(&h) {
                return Err(Error::BadArgs(format!(
                    "hours {h} is outside 0..{MAX_HORIZON}"
                )));
            }
            if h == 0.0 {
                SCENARIO_HOURS
            } else {
                h
            }
        }
        None => SCENARIO_HOURS,
    };
    let samples = match obj.get("samples").filter(|v| !v.is_null()) {
        Some(v) => {
            let n = count(v, "samples")?;
            if n < 0 || n as usize > MAX_SAMPLES {
                return Err(Error::BadArgs(format!(
                    "samples {n} is outside 0..{MAX_SAMPLES}"
                )));
            }
            if n < 2 {
                SCENARIO_SAMPLES
            } else {
                n as usize
            }
        }
        None => SCENARIO_SAMPLES,
    };
    let (seed, seed_set) = match obj.get("seed").filter(|v| !v.is_null()) {
        Some(v) => (parse_seed(v, "seed")?, true),
        None => (SCENARIO_SEED, false),
    };
    let summary = match obj.get("summary") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(o) => {
            return Err(Error::BadArgs(format!(
                "summary must be a boolean, got {o}"
            )))
        }
    };
    if engine == Engine::Ode && !schedule.is_empty() {
        return Err(Error::BadArgs(
            "the continuous engine cannot vary a rate over time; run the schedule on the discrete engine (drop engine: ode)".into(),
        ));
    }
    Ok(Scenario {
        name,
        marking,
        rates,
        schedule,
        hours,
        samples,
        realizations,
        seed,
        seed_set,
        summary,
        engine,
    })
}

/// One place's final value across realizations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Final {
    pub mean: f64,
    pub sd: Option<f64>,
    pub min: f64,
    pub max: f64,
}

/// A scenario's answer, in display names.
#[derive(Debug, Clone)]
pub struct ScenarioRun {
    pub engine: Engine,
    pub series: Ensemble,
    pub finals: Vec<(String, Final)>,
    /// Throughput (per transition), time-weighted mean / p95 (per place),
    /// utilization and in-flight, keys sorted.
    pub metrics: Value,
    pub depleted: Vec<Value>,
    pub contended: Vec<Value>,
    pub waiting_for_work: Vec<Value>,
    pub caveats: Vec<String>,
    pub assumptions: Vec<String>,
}

impl ScenarioRun {
    pub fn summary(&self, name: &str, sc: &Scenario) -> Value {
        let mut finals = Map::new();
        let mut sds = Map::new();
        for (p, f) in &self.finals {
            finals.insert(p.clone(), json!(f.mean));
            if let Some(sd) = f.sd {
                sds.insert(p.clone(), json!(sd));
            }
        }
        let mut out = json!({
            "name": name,
            "engine": self.engine.name(),
            "hours": sc.hours,
            "samples": sc.samples,
            "realizations": sc.realizations,
            "seed": sc.seed,
            "final": finals,
        });
        if !sds.is_empty() {
            out["finalStdDev"] = Value::Object(sds);
        }
        if !self.metrics.is_null() {
            out["metrics"] = self.metrics.clone();
        }
        out["depleted"] = json!(self.depleted);
        out["contended"] = json!(self.contended);
        out["waitingForWork"] = json!(self.waiting_for_work);
        out["caveats"] = json!(self.caveats);
        out["assumptions"] = json!(self.assumptions);
        out
    }
}

/// The scenario's starting model marking: the session marking with the
/// scenario's sparse overrides on top.
fn scenario_start(net: &Net, mk: &Marking, sc: &Scenario) -> Result<HashMap<String, i64>> {
    let mut start = net.model_marking(mk);
    for (k, v) in &sc.marking {
        start.insert(k.clone(), *v);
    }
    check_tokens(&start, sc.engine == Engine::Ssa)?;
    Ok(start)
}

fn sorted_map<K: AsRef<str>>(m: impl IntoIterator<Item = (K, f64)>) -> Value {
    let b: BTreeMap<String, f64> = m.into_iter().map(|(k, v)| (k.as_ref().into(), v)).collect();
    json!(b)
}

/// Answers one scenario from the session's marking and rates.
pub fn run_scenario(
    doc: &Document,
    net: &Net,
    mk: &Marking,
    session_rates: &BTreeMap<String, f64>,
    sc: &Scenario,
) -> Result<ScenarioRun> {
    let start = scenario_start(net, mk, sc)?;
    let mut rates = session_rates.clone();
    for (k, v) in &sc.rates {
        rates.insert(k.clone(), *v);
    }
    match sc.engine {
        Engine::Ode => run_scenario_ode(doc, net, &start, &rates, sc),
        Engine::Ssa => run_scenario_ssa(net, &start, &rates, sc),
    }
}

fn run_scenario_ode(
    doc: &Document,
    net: &Net,
    start: &HashMap<String, i64>,
    rates: &BTreeMap<String, f64>,
    sc: &Scenario,
) -> Result<ScenarioRun> {
    let gating = net.gating();
    if !gating.is_empty() {
        return Err(Error::Invalid(format!(
            "the continuous engine cannot express this net's gating ({}); drop engine: ode",
            gating.join("; ")
        )));
    }
    // The start as a session marking, so `sim::continuous` reads it.
    let mut mk: Marking = BTreeMap::new();
    let n = doc.colors();
    for p in &doc.places {
        mk.insert(p.id.clone(), vec![0; n]);
    }
    for (id, v) in start {
        let (base, c) = net.base(id);
        if let Some(slot) = mk.get_mut(&base).and_then(|s| s.get_mut(c)) {
            *slot = *v;
        }
    }
    let times = ssa::sample_grid(sc.hours, sc.samples);
    let by_name = ode_on_grid(doc, &mk, rates, &times)?;
    let mean: Vec<Vec<f64>> = net
        .display
        .iter()
        .map(|d| {
            by_name
                .get(d)
                .cloned()
                .unwrap_or_else(|| vec![0.0; times.len()])
        })
        .collect();
    let finals = net
        .display
        .iter()
        .zip(&mean)
        .map(|(d, v)| {
            let f = v.last().copied().unwrap_or(0.0);
            (
                d.clone(),
                Final {
                    mean: f,
                    sd: None,
                    min: f,
                    max: f,
                },
            )
        })
        .collect();
    Ok(ScenarioRun {
        engine: Engine::Ode,
        series: Ensemble {
            t: times,
            names: net.display.clone(),
            mean,
            sd: None,
        },
        finals,
        metrics: Value::Null,
        depleted: vec![],
        contended: vec![],
        waiting_for_work: vec![],
        caveats: vec![],
        assumptions: vec![
            "the continuous engine reads token counts as real-valued concentrations under mass action: no noise, no extinction".into(),
        ],
    })
}

fn run_scenario_ssa(
    net: &Net,
    start: &HashMap<String, i64>,
    rates: &BTreeMap<String, f64>,
    sc: &Scenario,
) -> Result<ScenarioRun> {
    let (token_order, _) = stochastic::token_places(&net.model)
        .map_err(|e| Error::Invalid(format!("scenario: {e}")))?;
    let init: Vec<i64> = token_order
        .iter()
        .map(|p| start.get(p).copied().unwrap_or(0))
        .collect();
    // Every realization's last marking, from the engine's on_fire hook.
    let ends: RefCell<Vec<Vec<i64>>> = RefCell::new(vec![init.clone(); sc.realizations]);
    let hook = |r: usize, _t: f64, _id: &str, mk: &[i64]| {
        let mut ends = ends.borrow_mut();
        if let Some(e) = ends.get_mut(r) {
            if mk.len() == e.len() {
                e.copy_from_slice(mk);
            }
        }
    };
    let opts = stochastic::Options {
        horizon: sc.hours,
        samples: sc.samples,
        rates: rates.iter().map(|(k, v)| (k.clone(), *v)).collect(),
        seed: sc.seed,
        realizations: sc.realizations,
        guard: None,
        schedule: sc
            .schedule
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        on_fire: Some(&hook),
    };
    let res = stochastic::simulate(&net.model, start, &opts)
        .map_err(|e| Error::Invalid(format!("scenario: {e}")))?;
    let ends = ends.into_inner();

    let n_samples = res.times.len();
    let mut mean = vec![];
    let mut sd: Option<Vec<Vec<f64>>> = (sc.realizations > 1).then(Vec::new);
    let mut finals = vec![];
    for (id, disp) in net.places.iter().zip(&net.display) {
        let s = res.series.iter().find(|s| &s.place == id);
        let values = s
            .map(|s| s.values.clone())
            .unwrap_or_else(|| vec![0.0; n_samples]);
        let spread = s.and_then(|s| s.std_dev.clone());
        let ti = token_order.iter().position(|p| p == id);
        let col: Vec<f64> = match ti {
            Some(i) => ends.iter().map(|e| e[i] as f64).collect(),
            None => vec![],
        };
        let fmean = res
            .final_
            .get(id)
            .copied()
            .unwrap_or_else(|| values.last().copied().unwrap_or(0.0));
        let fsd = spread.as_ref().and_then(|v| v.last().copied());
        let (lo, hi) = col
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| {
                (a.min(x), b.max(x))
            });
        finals.push((
            disp.clone(),
            Final {
                mean: fmean,
                sd: fsd,
                min: if col.is_empty() { fmean } else { lo },
                max: if col.is_empty() { fmean } else { hi },
            },
        ));
        if let Some(sd) = sd.as_mut() {
            sd.push(spread.unwrap_or_else(|| vec![0.0; n_samples]));
        }
        mean.push(values);
    }

    let disp = |id: &str| net.display_of(id);
    let metrics = match &res.metrics {
        Some(m) => json!({
            "throughput": sorted_map(m.throughput.iter().map(|(k, v)| (k.clone(), *v))),
            "mean": sorted_map(m.mean.iter().map(|(k, v)| (disp(k), *v))),
            "p95": sorted_map(m.p95.iter().map(|(k, v)| (disp(k), *v))),
            "utilization": sorted_map(m.utilization.iter().map(|(k, v)| (k.clone(), *v))),
            "inFlight": sorted_map(m.in_flight.iter().map(|(k, v)| (k.clone(), *v))),
        }),
        None => Value::Null,
    };
    let depleted = res
        .depleted
        .iter()
        .map(|d| json!({"place": disp(&d.place), "at": d.at, "recovered": d.recovered}))
        .collect();
    let mut contended = vec![];
    let mut waiting = vec![];
    for c in &res.contended {
        let v = json!({
            "place": disp(&c.place),
            "fraction": c.fraction,
            "kind": format!("{:?}", c.kind).to_lowercase(),
            "blocking": c.blocking,
        });
        if c.kind.is_capacity() {
            contended.push(v);
        } else {
            waiting.push(v);
        }
    }
    Ok(ScenarioRun {
        engine: Engine::Ssa,
        series: Ensemble {
            t: res.times.clone(),
            names: net.display.clone(),
            mean,
            sd,
        },
        finals,
        metrics,
        depleted,
        contended,
        waiting_for_work: waiting,
        caveats: res.caveats.clone(),
        assumptions: res.assumptions.clone(),
    })
}

// ------------------------------------------------------------ invariants

/// One P-invariant: `Σ coefficients[p] · m(p)` is constant on every run.
#[derive(Debug, Clone, PartialEq)]
pub struct Law {
    /// (display name, coefficient), coefficient > 0, in display order.
    pub terms: Vec<(String, i64)>,
    /// The sum at the starting marking.
    pub value: i64,
}

impl Law {
    pub fn expression(&self) -> String {
        let lhs: Vec<String> = self
            .terms
            .iter()
            .map(|(p, c)| {
                if *c == 1 {
                    p.clone()
                } else {
                    format!("{c}·{p}")
                }
            })
            .collect();
        format!("{} = {}", lhs.join(" + "), self.value)
    }

    /// The law's weighted sum along a trajectory set keyed by display name.
    pub fn sums(&self, names: &[String], series: &[Vec<f64>]) -> Vec<f64> {
        let n = series.first().map_or(0, Vec::len);
        let mut out = vec![0.0; n];
        for (p, c) in &self.terms {
            if let Some(i) = names.iter().position(|x| x == p) {
                for (o, v) in out.iter_mut().zip(&series[i]) {
                    *o += *c as f64 * v;
                }
            }
        }
        out
    }
}

/// Minimal semi-positive P-invariants ([`crate::analysis::farkas`] over the
/// incidence matrix of flow arcs; read and inhibitor arcs move nothing),
/// valued at `start`. The bool is true when the elimination hit its row cap
/// and the list may be incomplete.
pub fn p_invariants(net: &Net, start: &HashMap<String, i64>) -> (Vec<Law>, bool) {
    let n = net.places.len();
    let idx: HashMap<&str, usize> = net
        .places
        .iter()
        .enumerate()
        .map(|(i, p)| (p.as_str(), i))
        .collect();
    let ts = &net.model.transitions;
    let mut c = vec![vec![0i64; ts.len()]; n];
    for (j, t) in ts.iter().enumerate() {
        for a in net.model.inputs(&t.id) {
            if let Some(&p) = idx.get(a.place.as_str()) {
                c[p][j] -= a.weight.max(1);
            }
        }
        for a in net.model.outputs(&t.id) {
            if let Some(&p) = idx.get(a.place.as_str()) {
                c[p][j] += a.weight.max(1);
            }
        }
    }
    let crate::analysis::farkas::FarkasResult { basis, truncated } =
        crate::analysis::farkas::farkas(&c, MAX_FARKAS_ROWS);
    let mut laws: Vec<Law> = basis
        .iter()
        .map(|y| {
            let terms = (0..n)
                .filter(|&i| y[i] != 0)
                .map(|i| (net.display[i].clone(), y[i]))
                .collect();
            let value = (0..n)
                .map(|i| y[i] * start.get(&net.places[i]).copied().unwrap_or(0))
                .sum();
            Law { terms, value }
        })
        .collect();
    laws.sort_by(|a, b| a.terms.cmp(&b.terms));
    (laws, truncated)
}

// ------------------------------------------------------------ objectives

/// A linear objective over places: Σ coefficient · tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct Objective {
    /// (model place id, coefficient).
    pub terms: Vec<(String, f64)>,
    pub expression: String,
}

impl Objective {
    pub fn value(&self, final_: &HashMap<String, f64>) -> f64 {
        self.terms
            .iter()
            .map(|(p, c)| c * final_.get(p).copied().unwrap_or(0.0))
            .sum()
    }

    /// Parses `"win_x - win_o"`, `"2*a + b - 0.5 c"`, or a JSON object
    /// `{place: coefficient}`. A document place that unfolded to several
    /// colors counts every copy.
    pub fn parse(v: &Value, net: &Net) -> Result<Self> {
        let mut raw: Vec<(String, f64)> = vec![];
        match v {
            Value::Object(m) => {
                for (k, c) in m {
                    let c = c.as_f64().filter(|c| c.is_finite()).ok_or_else(|| {
                        Error::BadArgs(format!("objective coefficient for {k:?} must be a number"))
                    })?;
                    raw.push((k.clone(), c));
                }
            }
            Value::String(s) => raw = parse_linear(s)?,
            other => {
                return Err(Error::BadArgs(format!(
                    "objective must be an expression string or a {{place: coefficient}} object, got {other}"
                )))
            }
        }
        if raw.is_empty() {
            return Err(Error::BadArgs("objective names no place".into()));
        }
        let mut terms: Vec<(String, f64)> = vec![];
        for (k, c) in &raw {
            let ids: Vec<String> = match net.resolve_place(k) {
                Ok(id) => vec![id],
                Err(_) => {
                    let all: Vec<String> = net
                        .places
                        .iter()
                        .filter(|p| &net.base(p).0 == k)
                        .cloned()
                        .collect();
                    if all.is_empty() {
                        return Err(Error::NotFound(format!(
                            "objective names {k:?}, which is not a place"
                        )));
                    }
                    all
                }
            };
            for id in ids {
                match terms.iter_mut().find(|(p, _)| *p == id) {
                    Some(t) => t.1 += c,
                    None => terms.push((id, *c)),
                }
            }
        }
        let expression = raw
            .iter()
            .enumerate()
            .map(|(i, (p, c))| {
                let sign = if *c < 0.0 {
                    " - "
                } else if i == 0 {
                    ""
                } else {
                    " + "
                };
                let sign = if i == 0 && *c < 0.0 { "-" } else { sign };
                let a = c.abs();
                if a == 1.0 {
                    format!("{sign}{p}")
                } else {
                    format!("{sign}{a}*{p}")
                }
            })
            .collect();
        Ok(Objective { terms, expression })
    }
}

fn parse_linear(s: &str) -> Result<Vec<(String, f64)>> {
    let bad = || {
        Error::BadArgs(format!(
            "cannot read objective {s:?}; write e.g. \"win_x - win_o\" or \"2*a + b\""
        ))
    };
    let mut out = vec![];
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let skip = |i: &mut usize| {
        while *i < chars.len() && chars[*i].is_whitespace() {
            *i += 1;
        }
    };
    let ident = |c: char| c.is_alphanumeric() || matches!(c, '_' | '.' | '/' | ':');
    let mut first = true;
    loop {
        skip(&mut i);
        if i >= chars.len() {
            break;
        }
        let mut sign = 1.0;
        if chars[i] == '+' || chars[i] == '-' {
            if chars[i] == '-' {
                sign = -1.0;
            }
            i += 1;
            skip(&mut i);
        } else if !first {
            return Err(bad());
        }
        first = false;
        // Optional coefficient.
        let st = i;
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        let mut coef = 1.0;
        let num_str: String = chars[st..i].iter().collect();
        let followed_by_ident = i < chars.len() && ident(chars[i]) && !chars[i].is_whitespace();
        if !num_str.is_empty() && followed_by_ident && chars[i] != '*' {
            // A place id that starts with digits ("2a"): re-read as ident.
            i = st;
        } else if !num_str.is_empty() {
            coef = num_str.parse::<f64>().map_err(|_| bad())?;
            skip(&mut i);
            if i < chars.len() && chars[i] == '*' {
                i += 1;
                skip(&mut i);
            }
        }
        let st = i;
        while i < chars.len() && ident(chars[i]) {
            i += 1;
        }
        if st == i {
            return Err(bad());
        }
        out.push((chars[st..i].iter().collect(), sign * coef));
    }
    Ok(out)
}

// ------------------------------------------------------------ evaluate

/// How [`evaluate`] scores the position after a move.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Rollout {
    /// sim's play method: `rollouts` seeded SSA realizations (the scenario
    /// engine) over `horizon` model time; the mean final marking is scored.
    Ssa { horizon: f64 },
    /// Token-game playouts: uniformly random enabled transition (rate-0
    /// transitions excluded), at most `depth` firings or until deadlock.
    Random { depth: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub struct MoveScore {
    pub transition: String,
    pub payout: f64,
    /// Payout minus the baseline (the position before any move).
    pub delta: f64,
}

/// Scores each candidate by firing it (the metamodel's firing rule, as the
/// token game does) and averaging the objective over rollouts from the
/// position it leaves, with one seed shared across candidates so a
/// difference measures the move, not the dice. Best first.
#[allow(clippy::too_many_arguments)]
pub fn evaluate(
    net: &Net,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    candidates: &[String],
    objective: &Objective,
    rollout: Rollout,
    rollouts: usize,
    seed: u64,
) -> Result<(f64, Vec<MoveScore>)> {
    let start = net.model_marking(mk);
    check_tokens(&start, true)?;
    let payout = |from: &HashMap<String, i64>| -> Result<f64> {
        match rollout {
            Rollout::Ssa { horizon } => {
                let opts = stochastic::Options {
                    horizon,
                    samples: 2,
                    rates: rates.iter().map(|(k, v)| (k.clone(), *v)).collect(),
                    seed,
                    realizations: rollouts,
                    ..Default::default()
                };
                let res = stochastic::simulate(&net.model, from, &opts)
                    .map_err(|e| Error::Invalid(format!("evaluate: {e}")))?;
                Ok(objective.value(&res.final_))
            }
            Rollout::Random { depth } => {
                let mut total = 0.0;
                for r in 0..rollouts {
                    let mut rng = ssa::Xoshiro256::new(seed.max(1).wrapping_add(r as u64));
                    let mut m: HashMap<String, i64> = from.clone();
                    for _ in 0..depth {
                        let en: Vec<String> = net
                            .model
                            .enabled_transitions(&m)
                            .into_iter()
                            .filter(|t| rates.get(t).copied().unwrap_or(1.0) > 0.0)
                            .collect();
                        if en.is_empty() {
                            break;
                        }
                        let k = ((rng.uniform() * en.len() as f64) as usize).min(en.len() - 1);
                        m = net.model.fire(&en[k], &m);
                    }
                    let f: HashMap<String, f64> =
                        m.iter().map(|(k, v)| (k.clone(), *v as f64)).collect();
                    total += objective.value(&f);
                }
                Ok(total / rollouts as f64)
            }
        }
    };
    let base = payout(&start)?;
    let mut moves = vec![];
    for t in candidates {
        if !net.model.enabled(t, &start) {
            return Err(Error::Invalid(format!(
                "{t} is not enabled at this marking"
            )));
        }
        let next = net.model.fire(t, &start);
        let p = payout(&next)?;
        moves.push(MoveScore {
            transition: t.clone(),
            payout: p,
            delta: p - base,
        });
    }
    moves.sort_by(|a, b| b.payout.total_cmp(&a.payout));
    Ok((base, moves))
}

//! Execution against pflow-rs: discrete firing through the metamodel's one
//! firing rule (`Model::enabled` / `Model::fire`), and the mass-action ODE
//! through `pflow_solver`. The document is handed to pflow-rs as the same
//! editor JSON it would read from disk, so colors, inhibitor and read arcs
//! mean exactly what they mean everywhere else in the ecosystem.

use std::collections::{BTreeMap, HashMap};

use pflow_core::colors::ColorMap;
use pflow_metamodel::Model;
use serde::{Deserialize, Serialize};

use crate::doc::Document;
use crate::{Error, Result};

/// Tokens per place, one entry per color.
pub type Marking = BTreeMap<String, Vec<i64>>;

/// The document's initial marking.
pub fn initial_marking(doc: &Document) -> Marking {
    let n = doc.colors();
    doc.places
        .iter()
        .map(|p| {
            let mut v: Vec<i64> = p.initial.iter().map(|f| f.round() as i64).collect();
            v.resize(n, 0);
            (p.id.clone(), v)
        })
        .collect()
}

/// The document compiled to a metamodel [`Model`], with the color unfolding
/// it went through.
pub struct Compiled {
    pub model: Model,
    pub colors: Option<ColorMap>,
}

impl Compiled {
    pub fn new(doc: &Document) -> Result<Self> {
        let json = serde_json::to_vec(&doc.to_value()).expect("serializable");
        let (model, colors) =
            pflow_parser::model_from_json(&json).map_err(|e| Error::Parse(e.to_string()))?;
        Ok(Self { model, colors })
    }

    /// (base place, color index) for an unfolded model place id.
    fn base(&self, id: &str) -> (String, usize) {
        match self.colors.as_ref().and_then(|cm| cm.base.get(id)) {
            Some(r) => (r.place.clone(), r.color),
            None => (id.to_string(), 0),
        }
    }

    fn to_model_marking(&self, mk: &Marking) -> pflow_metamodel::Marking {
        self.model
            .places
            .iter()
            .map(|p| {
                let (base, c) = self.base(&p.id);
                let v = mk.get(&base).and_then(|v| v.get(c)).copied().unwrap_or(0);
                (p.id.clone(), v)
            })
            .collect()
    }

    pub fn enabled(&self, mk: &Marking) -> Vec<String> {
        self.model.enabled_transitions(&self.to_model_marking(mk))
    }

    pub fn is_enabled(&self, transition: &str, mk: &Marking) -> bool {
        self.model.enabled(transition, &self.to_model_marking(mk))
    }

    /// Fires `transition`, or explains why it cannot.
    pub fn fire(&self, transition: &str, mk: &Marking) -> Result<Marking> {
        let m = self.to_model_marking(mk);
        self.model
            .enabled_why_not(transition, &m)
            .map_err(|e| Error::Invalid(e.to_string()))?;
        let next = self.model.fire(transition, &m);
        let mut out = mk.clone();
        for (id, v) in next {
            let (base, c) = self.base(&id);
            if let Some(slot) = out.get_mut(&base).and_then(|v| v.get_mut(c)) {
                *slot = v;
            }
        }
        Ok(out)
    }
}

/// One ODE run, downsampled for plotting and for JSON callers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OdeResult {
    pub t: Vec<f64>,
    /// (state variable, values aligned with `t`) in document place order;
    /// colored places appear once per color as `place.color`.
    pub series: Vec<(String, Vec<f64>)>,
    pub t_end: f64,
}

impl OdeResult {
    pub fn final_state(&self) -> BTreeMap<String, f64> {
        self.series
            .iter()
            .map(|(k, v)| (k.clone(), v.last().copied().unwrap_or(0.0)))
            .collect()
    }

    /// The state at t = 0 (downsampling always keeps the first sample).
    pub fn initial_state(&self) -> BTreeMap<String, f64> {
        self.series
            .iter()
            .map(|(k, v)| (k.clone(), v.first().copied().unwrap_or(0.0)))
            .collect()
    }
}

/// Solver knobs a caller may override; `None` keeps the default. The
/// defaults are pflow.xyz's (`petri-solver.js`: dt 0.01, abstol 1e-6,
/// reltol 1e-3, dtmax 0.1), with dtmax further capped at `t_end / 50` so a
/// short run still gets a smooth curve.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OdeOptions {
    pub dt: Option<f64>,
    pub abstol: Option<f64>,
    pub reltol: Option<f64>,
    pub dtmax: Option<f64>,
}

impl OdeOptions {
    /// The coarser setting the grid commands (`opt.heatmap`, `opt.optimize`)
    /// default to: they only read the final state, so they let the step grow
    /// to `t_end / 20` and loosen abstol to 1e-4 (reltol stays 1e-3).
    pub fn grid(t_end: f64) -> Self {
        Self {
            dt: None,
            abstol: Some(1e-4),
            reltol: None,
            dtmax: Some(t_end / 20.0),
        }
    }

    /// Fills unset fields from `base`.
    pub fn or(self, base: OdeOptions) -> Self {
        Self {
            dt: self.dt.or(base.dt),
            abstol: self.abstol.or(base.abstol),
            reltol: self.reltol.or(base.reltol),
            dtmax: self.dtmax.or(base.dtmax),
        }
    }

    /// pflow-solver options for a run over `[0, t_end]`, refusing
    /// non-positive or non-finite overrides.
    pub fn solver_options(&self, t_end: f64) -> Result<pflow_solver::Options> {
        let mut o = pflow_solver::Options::default_opts();
        o.dtmax = o.dtmax.min(t_end / 50.0).max(o.dtmin);
        let pos = |name: &str, v: Option<f64>| -> Result<Option<f64>> {
            match v {
                Some(x) if !(x > 0.0 && x.is_finite()) => Err(Error::BadArgs(format!(
                    "{name} must be a positive number, got {x}"
                ))),
                other => Ok(other),
            }
        };
        if let Some(dt) = pos("dt", self.dt)? {
            o.dt = dt;
        }
        if let Some(a) = pos("abstol", self.abstol)? {
            o.abstol = a;
        }
        if let Some(r) = pos("reltol", self.reltol)? {
            o.reltol = r;
        }
        if let Some(m) = pos("dtmax", self.dtmax)? {
            o.dtmax = m.max(o.dtmin);
        }
        o.dt = o.dt.min(o.dtmax);
        Ok(o)
    }
}

pub const SOLVERS: &[&str] = &["tsit5", "rk45", "rk4", "euler", "heun", "midpoint", "bs32"];

/// The document as a continuous (single-color) pflow-core net, ready for
/// pflow-solver: colors unfolded, the state taken from a marking, rates
/// filled in. Shared by every continuous and stochastic engine.
pub struct ContinuousNet {
    pub net: pflow_core::PetriNet,
    /// Initial state by net place label.
    pub state: HashMap<String, f64>,
    /// Rate by transition id (absent in the session = 1.0).
    pub rates: HashMap<String, f64>,
    /// (net place label, display name) in document order; colored places
    /// appear once per color they can hold, as `place.color`.
    pub order: Vec<(String, String)>,
    pub colors: Option<ColorMap>,
}

pub fn continuous(
    doc: &Document,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
) -> Result<ContinuousNet> {
    let json = serde_json::to_vec(&doc.to_value()).expect("serializable");
    let net = pflow_core::from_json(&json).map_err(|e| Error::Parse(e.to_string()))?;
    let (net, cm) = if doc.colors() > 1 {
        net.expand_colors()
    } else {
        (net, None)
    };
    let short = pflow_core::colors::short_color_names(&doc.token);
    let mut order: Vec<(String, String)> = Vec::new();
    for p in &doc.places {
        match cm.as_ref() {
            Some(cm) => {
                // Only colors this place can hold: initially marked, or
                // moved by some arc. The rest stay 0 and only add noise.
                let used = |c: usize| {
                    p.initial.get(c).is_some_and(|v| *v > 0.0)
                        || doc.arcs.iter().any(|a| {
                            (a.source == p.id || a.target == p.id)
                                && a.weight.get(c).is_some_and(|w| *w > 0.0)
                        })
                };
                for (c, name) in cm.expanded.get(&p.id).into_iter().flatten().enumerate() {
                    if !used(c) {
                        continue;
                    }
                    let color = short.get(c).cloned().unwrap_or_else(|| c.to_string());
                    order.push((name.clone(), format!("{}.{}", p.id, color)));
                }
            }
            None => order.push((p.id.clone(), p.id.clone())),
        }
    }
    let mut state = HashMap::new();
    for label in net.places.keys() {
        let (base, c) = match cm.as_ref().and_then(|cm| cm.base.get(label)) {
            Some(r) => (r.place.clone(), r.color),
            None => (label.clone(), 0),
        };
        let v = mk.get(&base).and_then(|v| v.get(c)).copied().unwrap_or(0);
        state.insert(label.clone(), v as f64);
    }
    let rates: HashMap<String, f64> = doc
        .transitions
        .iter()
        .map(|t| (t.id.clone(), rates.get(&t.id).copied().unwrap_or(1.0)))
        .collect();
    let rates = net.set_rates(Some(&rates));
    Ok(ContinuousNet {
        net,
        state,
        rates,
        order,
        colors: cm,
    })
}

/// A pflow-solver method by name.
pub fn solver(method: &str) -> Result<pflow_solver::Solver> {
    use pflow_solver::methods;
    Ok(match method {
        "tsit5" => methods::tsit5(),
        "rk45" => methods::rk45(),
        "rk4" => methods::rk4(),
        "euler" => methods::euler(),
        "heun" => methods::heun(),
        "midpoint" => methods::midpoint(),
        "bs32" => methods::bs32(),
        other => {
            return Err(Error::BadArgs(format!(
                "unknown method {other:?}; one of {SOLVERS:?}"
            )))
        }
    })
}

/// Indices that thin `n` samples to at most `max_points`, keeping the last.
pub fn downsample(n: usize, max_points: usize) -> Vec<usize> {
    let stride = n.div_ceil(max_points.max(2)).max(1);
    let mut idx: Vec<usize> = (0..n).step_by(stride).collect();
    if n > 0 && idx.last() != Some(&(n - 1)) {
        idx.push(n - 1);
    }
    idx
}

/// Integrates the mass-action ODE from marking `mk` over `[0, t_end]`
/// with the default solver options.
pub fn ode(
    doc: &Document,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    t_end: f64,
    method: &str,
    max_points: usize,
) -> Result<OdeResult> {
    ode_with(
        doc,
        mk,
        rates,
        t_end,
        method,
        max_points,
        &OdeOptions::default(),
    )
}

/// [`ode`] with solver option overrides.
pub fn ode_with(
    doc: &Document,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    t_end: f64,
    method: &str,
    max_points: usize,
    opts: &OdeOptions,
) -> Result<OdeResult> {
    use pflow_solver::{solve, Problem};

    check_t_end(t_end)?;
    let solver = solver(method)?;
    let opts = opts.solver_options(t_end)?;
    let cn = continuous(doc, mk, rates)?;
    let prob = Problem::new(cn.net, cn.state, [0.0, t_end], cn.rates);
    let sol = solve(&prob, &solver, &opts);

    let idx = downsample(sol.t.len(), max_points);
    let t = idx.iter().map(|&i| sol.t[i]).collect();
    let series = cn
        .order
        .into_iter()
        .map(|(label, display)| {
            let col = sol.get_variable(&label);
            (display, idx.iter().map(|&i| col[i]).collect())
        })
        .collect();
    Ok(OdeResult { t, series, t_end })
}

/// Refuses a horizon that is not a positive finite number.
pub fn check_t_end(t_end: f64) -> Result<()> {
    if t_end > 0.0 && t_end.is_finite() {
        Ok(())
    } else {
        Err(Error::BadArgs("t_end must be a positive number".into()))
    }
}

/// Indices into `order` (a [`ContinuousNet::order`]) that `name` selects:
/// an exact display name (`S`, `CoffeeBeans.brown`) or a base place id,
/// which selects every color of that place.
pub fn select_series(order: &[(String, String)], name: &str) -> Vec<usize> {
    let exact: Vec<usize> = order
        .iter()
        .enumerate()
        .filter(|(_, (_, d))| d == name)
        .map(|(i, _)| i)
        .collect();
    if !exact.is_empty() {
        return exact;
    }
    let prefix = format!("{name}.");
    order
        .iter()
        .enumerate()
        .filter(|(_, (_, d))| d.starts_with(&prefix))
        .map(|(i, _)| i)
        .collect()
}

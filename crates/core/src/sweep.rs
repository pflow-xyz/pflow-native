//! Many ODE solves over one net: the engine under the `opt.*` workbench
//! commands (rate scan, sweep, heatmap, optimize) and `sim.equilibrium`.
//!
//! The document is unfolded into a continuous net once ([`sim::continuous`]);
//! each solve clones that net and overrides some transition rates, so a grid
//! of 1600 cells costs 1600 integrations, not 1600 parses and unfoldings.
//! Integration is pflow-solver's, unchanged.

use std::collections::BTreeMap;

use pflow_solver::{solve, Options, Problem, Solver};

use crate::doc::Document;
use crate::sim::{self, ContinuousNet, Marking, OdeOptions};
use crate::{Error, Result};

/// Most values a rate scan or sweep takes (the pflow.xyz editor's cap).
pub const MAX_SCAN_STEPS: usize = 25;
/// Most cells on one heatmap axis (sim.pflow.xyz's cap).
pub const MAX_AXIS_STEPS: usize = 40;
/// Most solves one heatmap runs (40 × 40).
pub const MAX_GRID_CELLS: usize = MAX_AXIS_STEPS * MAX_AXIS_STEPS;
/// Most Monte Carlo samples one optimize runs.
pub const MAX_SAMPLES: usize = 2000;

/// A continuous net ready to be solved repeatedly with varied rates.
pub struct Runner {
    cn: ContinuousNet,
    solver: Solver,
    opts: Options,
    t_end: f64,
}

impl Runner {
    pub fn new(
        doc: &Document,
        mk: &Marking,
        rates: &BTreeMap<String, f64>,
        t_end: f64,
        method: &str,
        opts: &OdeOptions,
    ) -> Result<Self> {
        sim::check_t_end(t_end)?;
        let solver = sim::solver(method)?;
        let opts = opts.solver_options(t_end)?;
        let cn = sim::continuous(doc, mk, rates)?;
        Ok(Self {
            cn,
            solver,
            opts,
            t_end,
        })
    }

    /// (net place label, display name) in document order.
    pub fn order(&self) -> &[(String, String)] {
        &self.cn.order
    }

    pub fn t_end(&self) -> f64 {
        self.t_end
    }

    /// Refuses a transition the net does not have.
    pub fn check_transition(&self, id: &str) -> Result<()> {
        if self.cn.rates.contains_key(id) {
            Ok(())
        } else {
            Err(Error::NotFound(format!("no transition {id:?}")))
        }
    }

    /// Series indices for a place name (display name or base place id);
    /// refuses a name that selects nothing.
    pub fn select(&self, name: &str) -> Result<Vec<usize>> {
        let idx = sim::select_series(&self.cn.order, name);
        if idx.is_empty() {
            let known: Vec<&str> = self.cn.order.iter().map(|(_, d)| d.as_str()).collect();
            return Err(Error::NotFound(format!(
                "no place {name:?}; places are {known:?}"
            )));
        }
        Ok(idx)
    }

    /// Series indices for an optional place filter (all when `None`), in
    /// display order and without duplicates.
    pub fn select_many(&self, names: Option<&[String]>) -> Result<Vec<usize>> {
        let Some(names) = names else {
            return Ok((0..self.cn.order.len()).collect());
        };
        let mut keep = vec![false; self.cn.order.len()];
        for n in names {
            for i in self.select(n)? {
                keep[i] = true;
            }
        }
        Ok((0..keep.len()).filter(|&i| keep[i]).collect())
    }

    fn problem(&self, overrides: &[(&str, f64)]) -> Problem {
        let mut rates = self.cn.rates.clone();
        for (id, r) in overrides {
            rates.insert((*id).to_string(), *r);
        }
        Problem::new(
            self.cn.net.clone(),
            self.cn.state.clone(),
            [0.0, self.t_end],
            rates,
        )
    }

    /// The state at `t_end`, one value per [`Runner::order`] entry.
    pub fn final_state(&self, overrides: &[(&str, f64)]) -> Vec<f64> {
        let sol = solve(&self.problem(overrides), &self.solver, &self.opts);
        let last = sol.u.last();
        self.cn
            .order
            .iter()
            .map(|(label, _)| last.and_then(|s| s.get(label)).copied().unwrap_or(0.0))
            .collect()
    }

    /// The full trajectory: accepted times, and one column per
    /// [`Runner::order`] entry.
    pub fn trajectory(&self, overrides: &[(&str, f64)]) -> (Vec<f64>, Vec<Vec<f64>>) {
        let sol = solve(&self.problem(overrides), &self.solver, &self.opts);
        let cols = self
            .cn
            .order
            .iter()
            .map(|(label, _)| sol.get_variable(label))
            .collect();
        (sol.t, cols)
    }

    /// Integrates until pflow-solver's equilibrium detector fires or
    /// `t_end` runs out.
    pub fn equilibrium(&self, tolerance: Option<f64>) -> Result<Equilibrium> {
        use pflow_solver::equilibrium::{solve_until_equilibrium, EquilibriumOptions};
        let mut eq = EquilibriumOptions::default_opts();
        if let Some(t) = tolerance {
            if !(t > 0.0 && t.is_finite()) {
                return Err(Error::BadArgs(format!(
                    "tolerance must be a positive number, got {t}"
                )));
            }
            eq.tolerance = t;
        }
        let (_, r) = solve_until_equilibrium(&self.problem(&[]), &self.solver, &self.opts, &eq);
        let state = self
            .cn
            .order
            .iter()
            .map(|(label, d)| (d.clone(), r.state.get(label).copied().unwrap_or(0.0).into()))
            .collect();
        Ok(Equilibrium {
            reached: r.reached,
            time: r.time,
            state,
            max_change: r.max_change,
            steps: r.steps,
            reason: r.reason,
            tolerance: eq.tolerance,
        })
    }
}

/// What [`Runner::equilibrium`] found.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Equilibrium {
    pub reached: bool,
    pub time: f64,
    /// By display name, in document order.
    pub state: serde_json::Map<String, serde_json::Value>,
    /// Largest |du/dt| at the end.
    pub max_change: f64,
    pub steps: usize,
    pub reason: String,
    pub tolerance: f64,
}

/// Refuses a rate that is negative or not finite.
pub fn check_rate(what: &str, v: f64) -> Result<()> {
    if v >= 0.0 && v.is_finite() {
        Ok(())
    } else {
        Err(Error::BadArgs(format!(
            "{what} {v} is not a rate (a finite number ≥ 0)"
        )))
    }
}

/// `n` values from `from` to `to` inclusive, evenly spaced, or evenly in
/// log10 when `log` (which needs positive bounds).
pub fn axis(from: f64, to: f64, n: usize, log: bool) -> Result<Vec<f64>> {
    check_rate("from", from)?;
    check_rate("to", to)?;
    if n < 2 {
        return Err(Error::BadArgs(format!("steps {n} is below 2")));
    }
    if log && (from <= 0.0 || to <= 0.0) {
        return Err(Error::BadArgs("log spacing needs positive bounds".into()));
    }
    Ok((0..n)
        .map(|i| {
            let f = i as f64 / (n - 1) as f64;
            if log {
                10f64.powf(from.log10() + f * (to.log10() - from.log10()))
            } else {
                from + f * (to - from)
            }
        })
        .collect())
}

/// Linear interpolation of the samples `(t, y)` at each of `at`; `t` is
/// ascending. Outside the samples the end value holds.
pub fn resample(t: &[f64], y: &[f64], at: &[f64]) -> Vec<f64> {
    let mut j = 0;
    at.iter()
        .map(|&x| {
            if t.is_empty() {
                return 0.0;
            }
            while j + 1 < t.len() && t[j + 1] < x {
                j += 1;
            }
            if j + 1 >= t.len() || x <= t[j] {
                return if x <= t[0] { y[0] } else { y[t.len() - 1] };
            }
            let (t0, t1) = (t[j], t[j + 1]);
            let w = if t1 > t0 { (x - t0) / (t1 - t0) } else { 1.0 };
            y[j] + w * (y[j + 1] - y[j])
        })
        .collect()
}

/// `v` to `sig` significant digits, printed shortest (JS
/// `+v.toPrecision(sig)`): 0.1 → "0.1", 1.0 → "1", 1.2375 → "1.24".
pub fn fmt_sig(v: f64, sig: usize) -> String {
    let p = sig.max(1) - 1;
    let r: f64 = format!("{v:.p$e}").parse().unwrap_or(v);
    format!("{r}")
}

/// splitmix64: a tiny deterministic PRNG, no dependency. The same seed
/// gives the same stream on every platform.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`, 53 bits.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }
}

/// Pareto flags: `values[i][k]` is sample `i`'s objective `k`, and
/// `maximize[k]` its sense. A sample is on the front when no other sample
/// is at least as good on every objective and strictly better on one
/// (sim.pflow.xyz's `dominates`).
pub fn pareto(values: &[Vec<f64>], maximize: &[bool]) -> Vec<bool> {
    let dominates = |a: &[f64], b: &[f64]| {
        let mut better = false;
        for (k, &max) in maximize.iter().enumerate() {
            let (va, vb) = if max { (a[k], b[k]) } else { (-a[k], -b[k]) };
            if va < vb {
                return false;
            }
            if va > vb {
                better = true;
            }
        }
        better
    };
    (0..values.len())
        .map(|i| !(0..values.len()).any(|j| j != i && dominates(&values[j], &values[i])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_linear_and_log() {
        assert_eq!(axis(0.0, 1.0, 3, false).unwrap(), vec![0.0, 0.5, 1.0]);
        let l = axis(0.01, 1.0, 3, true).unwrap();
        assert!((l[1] - 0.1).abs() < 1e-12 && (l[2] - 1.0).abs() < 1e-12);
        assert!(axis(0.0, 1.0, 3, true).is_err());
        assert!(axis(-1.0, 1.0, 3, false).is_err());
        assert!(axis(0.0, 1.0, 1, false).is_err());
    }

    #[test]
    fn resample_interpolates() {
        let t = [0.0, 1.0, 3.0];
        let y = [0.0, 2.0, 6.0];
        assert_eq!(
            resample(&t, &y, &[0.0, 0.5, 2.0, 3.0, 4.0]),
            vec![0.0, 1.0, 4.0, 6.0, 6.0]
        );
    }

    #[test]
    fn sig_digits() {
        assert_eq!(fmt_sig(0.1, 3), "0.1");
        assert_eq!(fmt_sig(1.0, 3), "1");
        assert_eq!(fmt_sig(1.2375, 3), "1.24");
        assert_eq!(fmt_sig(0.575, 3), "0.575");
    }

    #[test]
    fn rng_is_seeded() {
        let a: Vec<u64> = (0..4)
            .scan(Rng::new(7), |r, _| Some(r.next_u64()))
            .collect();
        let b: Vec<u64> = (0..4)
            .scan(Rng::new(7), |r, _| Some(r.next_u64()))
            .collect();
        assert_eq!(a, b);
        let f = Rng::new(1).next_f64();
        assert!((0.0..1.0).contains(&f));
    }

    #[test]
    fn pareto_front() {
        // maximize both: (3,1) (1,3) (2,2) are a front; (1,1) is dominated,
        // and so is the duplicate-free (2,1) by (2,2).
        let v = vec![
            vec![3.0, 1.0],
            vec![1.0, 3.0],
            vec![2.0, 2.0],
            vec![1.0, 1.0],
            vec![2.0, 1.0],
        ];
        assert_eq!(
            pareto(&v, &[true, true]),
            vec![true, true, true, false, false]
        );
        // minimize the second: (3,1) dominates everything with x ≤ 3.
        assert_eq!(
            pareto(&v, &[true, false]),
            vec![true, false, false, false, false]
        );
    }
}

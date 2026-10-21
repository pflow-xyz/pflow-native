//! State-space analysis over the token game's own firing rule: the
//! reachability graph (Karp–Miller coverability with ω when a pump is
//! found), shortest firing sequences to a partial marking, separating
//! P-invariant certificates (sim.pflow.xyz's `sim_reach`), the translation
//! of sim's `sim_verify` property JSON onto `pflow-verify`, and the ODE
//! influence readings behind `analysis.diagnose`.
//!
//! Every firing goes through `pflow_metamodel::Model::enabled` / `fire` on
//! the model `sim::Compiled` builds from the document (through
//! [`stochastic::Net`], which adds the display order), so inhibitor and read
//! arcs, capacities and colors behave exactly as on the canvas. To keep a
//! search cheap, each transition is tested against a one-transition copy of
//! the model holding only its own arcs and the places they touch: the same
//! rule, without rescanning every arc of the net for each enablement test.
//!
//! Markings inside a search are vectors aligned with [`Net::places`] (the
//! unfolded model's token places, in document order then color); results
//! are reported back in document place ids, one count per color.

use std::collections::{BTreeMap, HashMap, VecDeque};

use pflow_metamodel::{ArcType, Model};
use serde_json::{json, Map, Value};

use crate::artifact::TraceStep;
use crate::doc::Document;
use crate::sim::Marking;
use crate::stochastic::{self, Net};
use crate::sweep::Runner;
use crate::{Error, Result};

/// ω inside a search: large enough that no weight or capacity test can tell
/// it from infinity, small enough that saturating arithmetic never wraps.
pub const OMEGA: i64 = 1 << 40;
/// ω in reported markings (a count can never be negative otherwise).
pub const OMEGA_REPORTED: i64 = -1;

pub const DEFAULT_GRAPH_STATES: usize = 2_000;
pub const MAX_GRAPH_STATES: usize = 20_000;
pub const DEFAULT_REACH_STATES: usize = 20_000;
/// sim.pflow.xyz's `MaxReachStates`.
pub const MAX_REACH_STATES: usize = 100_000;
/// sim.pflow.xyz's scenario caps: per place, and in total.
pub const MAX_PLACE_TOKENS: i64 = 1_000_000;
pub const MAX_TOTAL_TOKENS: i64 = 4_000_000;

/// One transition, compiled to a model of just itself.
struct Local {
    id: String,
    model: Model,
    /// (index into `Net::places`, model place id) of every token place the
    /// transition's arcs touch.
    touch: Vec<(usize, String)>,
    /// Gated by an inhibitor arc or producing into a capacity-bounded
    /// place: repeating it is not monotone, so a pump through it proves
    /// nothing (pflow-reachability's rule for coverability witnesses).
    unsafe_pump: bool,
}

/// The document unfolded for exploration.
pub struct Space {
    pub net: Net,
    n_colors: usize,
    doc_places: Vec<String>,
    /// (index into `doc_places`, color) per model place.
    base: Vec<(usize, usize)>,
    local: Vec<Local>,
    /// Incidence matrix, `[place][transition]` (outputs minus inputs; read
    /// and inhibitor arcs move nothing).
    pub incidence: Vec<Vec<i64>>,
}

/// A predicate on search vectors: where a search stops.
pub type Target<'a> = &'a dyn Fn(&[i64]) -> bool;

/// What a breadth-first exploration found.
#[derive(Debug, Clone, Default)]
pub struct Explored {
    /// Markings in discovery order; 0 is the start.
    pub states: Vec<Vec<i64>>,
    /// (predecessor, transition index) on a shortest path from the start.
    pub parent: Vec<Option<(usize, usize)>>,
    pub depth: Vec<usize>,
    /// (from, to, transition index).
    pub edges: Vec<(usize, usize, usize)>,
    /// Expanded and nothing enabled.
    pub terminal: Vec<bool>,
    pub expanded: Vec<bool>,
    /// A new marking was dropped because the state cap was reached.
    pub truncated: bool,
    /// A firing past [`MAX_PLACE_TOKENS`] on a place was not followed.
    pub token_capped: bool,
    /// The first state the target predicate accepted.
    pub found: Option<usize>,
}

impl Explored {
    pub fn has_omega(&self, i: usize) -> bool {
        self.states[i].iter().any(|&x| x >= OMEGA / 2)
    }
}

/// A P-invariant `y` (yᵀC = 0) whose weighted sum separates `from` and
/// `to`: reachability preserves `y·m`, whatever inhibitors, read arcs and
/// capacities allow, since those only remove firings.
#[derive(Debug, Clone)]
pub struct Certificate {
    /// (place index, coefficient), nonzero.
    pub coefficients: Vec<(usize, i64)>,
    /// Places of `y` the target leaves unconstrained (semi-positive `y`
    /// only: they can only add to the target's side).
    pub free: Vec<usize>,
    pub at_from: i64,
    /// `y·to` over the constrained places.
    pub at_target: i64,
    /// "equality" (pass A: exact elimination over the target's places) or
    /// "semi-positive" (pass B: Farkas basis).
    pub pass: &'static str,
}

impl Space {
    pub fn new(doc: &Document) -> Result<Self> {
        let net = Net::new(doc)?;
        let doc_places: Vec<String> = doc.places.iter().map(|p| p.id.clone()).collect();
        let base = net
            .places
            .iter()
            .map(|id| {
                let (b, c) = net.base(id);
                (doc_places.iter().position(|p| *p == b).unwrap_or(0), c)
            })
            .collect();
        let index: HashMap<&str, usize> = net
            .places
            .iter()
            .enumerate()
            .map(|(i, p)| (p.as_str(), i))
            .collect();
        let m = &net.model;
        let capped = |id: &str| m.place_by_id(id).is_some_and(|p| p.capacity > 0);
        let mut local = Vec::with_capacity(m.transitions.len());
        let mut incidence = vec![vec![0i64; m.transitions.len()]; net.places.len()];
        for (j, t) in m.transitions.iter().enumerate() {
            let arcs: Vec<_> = m
                .arcs
                .iter()
                .filter(|a| a.from == t.id || a.to == t.id)
                .cloned()
                .collect();
            let mut ids: Vec<String> = Vec::new();
            for a in &arcs {
                let other = if a.from == t.id { &a.to } else { &a.from };
                if !ids.contains(other) {
                    ids.push(other.clone());
                }
            }
            let places: Vec<_> = ids
                .iter()
                .filter_map(|id| m.place_by_id(id).cloned())
                .collect();
            let touch = places
                .iter()
                .filter_map(|p| index.get(p.id.as_str()).map(|&i| (i, p.id.clone())))
                .collect();
            let unsafe_pump = arcs
                .iter()
                .any(|a| a.typ == ArcType::Inhibitor && a.to == t.id)
                || m.outputs(&t.id).iter().any(|o| capped(&o.place));
            for a in m.inputs(&t.id) {
                if let Some(&i) = index.get(a.place.as_str()) {
                    incidence[i][j] -= a.weight;
                }
            }
            for a in m.outputs(&t.id) {
                if let Some(&i) = index.get(a.place.as_str()) {
                    incidence[i][j] += a.weight;
                }
            }
            local.push(Local {
                id: t.id.clone(),
                model: Model {
                    name: m.name.clone(),
                    places,
                    transitions: vec![t.clone()],
                    arcs,
                    ..Default::default()
                },
                touch,
                unsafe_pump,
            });
        }
        Ok(Self {
            net,
            n_colors: doc.colors(),
            doc_places,
            base,
            local,
            incidence,
        })
    }

    pub fn transitions(&self) -> impl Iterator<Item = &str> {
        self.local.iter().map(|l| l.id.as_str())
    }

    pub fn transition_id(&self, t: usize) -> &str {
        &self.local[t].id
    }

    pub fn transition_index(&self, id: &str) -> Option<usize> {
        self.local.iter().position(|l| l.id == id)
    }

    pub fn place_count(&self) -> usize {
        self.net.places.len()
    }

    pub fn display(&self, p: usize) -> &str {
        &self.net.display[p]
    }

    // ------------------------------------------------------------ markings

    /// A document marking as a search vector.
    pub fn vector(&self, mk: &Marking) -> Vec<i64> {
        self.base
            .iter()
            .map(|&(dp, c)| {
                mk.get(&self.doc_places[dp])
                    .and_then(|v| v.get(c))
                    .copied()
                    .unwrap_or(0)
            })
            .collect()
    }

    /// A search vector as a document marking: every document place, one
    /// count per color; ω reads [`OMEGA_REPORTED`].
    pub fn doc_marking(&self, v: &[i64]) -> Marking {
        let mut out: Marking = self
            .doc_places
            .iter()
            .map(|p| (p.clone(), vec![0; self.n_colors]))
            .collect();
        for (i, &(dp, c)) in self.base.iter().enumerate() {
            if let Some(slot) = out.get_mut(&self.doc_places[dp]).and_then(|v| v.get_mut(c)) {
                *slot = if v[i] >= OMEGA / 2 {
                    OMEGA_REPORTED
                } else {
                    v[i]
                };
            }
        }
        out
    }

    /// A search vector over model place ids (what pflow-verify reads).
    pub fn model_marking(&self, v: &[i64]) -> HashMap<String, i64> {
        self.net
            .places
            .iter()
            .cloned()
            .zip(v.iter().copied())
            .collect()
    }

    /// "S=999 I=1": the nonzero places by display name; ω as `ω`.
    pub fn label(&self, v: &[i64]) -> String {
        let parts: Vec<String> = v
            .iter()
            .enumerate()
            .filter(|(_, &x)| x != 0)
            .map(|(i, &x)| {
                if x >= OMEGA / 2 {
                    format!("{}=ω", self.net.display[i])
                } else {
                    format!("{}={x}", self.net.display[i])
                }
            })
            .collect();
        if parts.is_empty() {
            "∅".into()
        } else {
            parts.join(" ")
        }
    }

    /// Display names of the ω places of `v`.
    pub fn omega_places(&self, v: &[i64]) -> Vec<String> {
        v.iter()
            .enumerate()
            .filter(|(_, &x)| x >= OMEGA / 2)
            .map(|(i, _)| self.net.display[i].clone())
            .collect()
    }

    // ------------------------------------------------------------- firing

    fn local_marking(&self, v: &[i64], t: usize) -> HashMap<String, i64> {
        self.local[t]
            .touch
            .iter()
            .map(|(i, id)| (id.clone(), v[*i]))
            .collect()
    }

    pub fn enabled(&self, v: &[i64], t: usize) -> bool {
        let l = &self.local[t];
        l.model.enabled(&l.id, &self.local_marking(v, t))
    }

    /// The marking after `t`, or `None` when it is not enabled. ω stays ω.
    pub fn fire(&self, v: &[i64], t: usize) -> Option<Vec<i64>> {
        let l = &self.local[t];
        let mk = self.local_marking(v, t);
        if !l.model.enabled(&l.id, &mk) {
            return None;
        }
        let next = l.model.fire(&l.id, &mk);
        let mut out = v.to_vec();
        for (i, id) in &l.touch {
            out[*i] = if v[*i] >= OMEGA / 2 {
                OMEGA
            } else {
                next.get(id).copied().unwrap_or(v[*i])
            };
        }
        Some(out)
    }

    /// Why `t` cannot fire at `v`, in the firing rule's own words.
    pub fn why_not(&self, v: &[i64], t: usize) -> String {
        let l = &self.local[t];
        match l.model.enabled_why_not(&l.id, &self.local_marking(v, t)) {
            Ok(()) => "enabled".into(),
            Err(e) => e.to_string(),
        }
    }

    /// Replays `path` from `start` into a trace whose step 0 is the start
    /// marking (transition ""), in document ids.
    pub fn trace(&self, start: &[i64], path: &[usize]) -> Result<Vec<TraceStep>> {
        let mut steps = vec![TraceStep {
            transition: String::new(),
            marking: self.doc_marking(start),
        }];
        let mut cur = start.to_vec();
        for (k, &t) in path.iter().enumerate() {
            cur = self.fire(&cur, t).ok_or_else(|| {
                Error::Invalid(format!(
                    "step {} ({}) does not replay: {}",
                    k + 1,
                    self.local[t].id,
                    self.why_not(&cur, t)
                ))
            })?;
            steps.push(TraceStep {
                transition: self.local[t].id.clone(),
                marking: self.doc_marking(&cur),
            });
        }
        Ok(steps)
    }

    /// [`Space::trace`] for a sequence of transition ids.
    pub fn trace_ids(&self, start: &[i64], ids: &[String]) -> Result<Vec<TraceStep>> {
        let path = ids
            .iter()
            .map(|id| {
                self.transition_index(id)
                    .ok_or_else(|| Error::NotFound(format!("no transition {id:?}")))
            })
            .collect::<Result<Vec<_>>>()?;
        self.trace(start, &path)
    }

    // ---------------------------------------------------------- exploring

    /// Breadth-first exploration from `start`, keeping at most `max_states`
    /// markings. With `cover`, Karp–Miller acceleration turns a place to ω
    /// when a marking strictly covers one of its ancestors along a pump of
    /// monotone transitions; the result is then the coverability graph
    /// (equal to the reachability graph whenever no pump exists). With a
    /// `target`, the search stops at the first marking it accepts, which is
    /// then a shortest firing sequence away.
    pub fn explore(
        &self,
        start: Vec<i64>,
        max_states: usize,
        cover: bool,
        keep_edges: bool,
        target: Option<Target<'_>>,
    ) -> Explored {
        let mut ex = Explored::default();
        let mut index: HashMap<Vec<i64>, usize> = HashMap::new();
        index.insert(start.clone(), 0);
        ex.states.push(start);
        ex.parent.push(None);
        ex.depth.push(0);
        ex.terminal.push(false);
        ex.expanded.push(false);
        if target.is_some_and(|f| f(&ex.states[0])) {
            ex.found = Some(0);
            return ex;
        }
        let mut queue = VecDeque::from([0usize]);
        while let Some(s) = queue.pop_front() {
            let cur = ex.states[s].clone();
            let mut any = false;
            for t in 0..self.local.len() {
                let Some(mut next) = self.fire(&cur, t) else {
                    continue;
                };
                any = true;
                if cover {
                    self.accelerate(&mut next, s, t, &ex);
                }
                if next.iter().any(|&x| x > MAX_PLACE_TOKENS && x < OMEGA / 2) {
                    ex.token_capped = true;
                    continue;
                }
                let to = match index.get(&next) {
                    Some(&i) => i,
                    None => {
                        if ex.states.len() >= max_states {
                            ex.truncated = true;
                            continue;
                        }
                        let i = ex.states.len();
                        index.insert(next.clone(), i);
                        let hit = target.is_some_and(|f| f(&next));
                        ex.states.push(next);
                        ex.parent.push(Some((s, t)));
                        ex.depth.push(ex.depth[s] + 1);
                        ex.terminal.push(false);
                        ex.expanded.push(false);
                        if hit {
                            ex.found = Some(i);
                            return ex;
                        }
                        queue.push_back(i);
                        i
                    }
                };
                if keep_edges {
                    ex.edges.push((s, to, t));
                }
            }
            ex.expanded[s] = true;
            ex.terminal[s] = !any;
        }
        ex
    }

    /// Karp–Miller: for each ancestor `a` on the shortest-path tree that
    /// `next` strictly covers, through a pump of monotone transitions, the
    /// places that grew become ω.
    fn accelerate(&self, next: &mut [i64], s: usize, t: usize, ex: &Explored) {
        let mut unsafe_pump = self.local[t].unsafe_pump;
        let mut at = Some(s);
        while let Some(a) = at {
            if !unsafe_pump {
                let am = &ex.states[a];
                let covers = next.iter().zip(am).all(|(x, y)| x >= y);
                if covers && next.iter().zip(am).any(|(x, y)| x > y) {
                    for (x, y) in next.iter_mut().zip(am) {
                        if *x > *y {
                            *x = OMEGA;
                        }
                    }
                }
            }
            at = ex.parent[a].map(|(p, pt)| {
                unsafe_pump |= self.local[pt].unsafe_pump;
                p
            });
        }
    }

    /// The transition indices from the start to state `i`.
    pub fn path(&self, ex: &Explored, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut at = i;
        while let Some((p, t)) = ex.parent[at] {
            out.push(t);
            at = p;
        }
        out.reverse();
        out
    }

    // ---------------------------------------------------- partial markings

    /// Model places of a document place, by color index.
    fn copies(&self, doc_place: &str) -> Vec<(usize, usize)> {
        self.base
            .iter()
            .enumerate()
            .filter(|(_, (dp, _))| self.doc_places[*dp] == doc_place)
            .map(|(i, (_, c))| (i, *c))
            .collect()
    }

    /// Model place indices a name stands for in an expression or a
    /// mutual-exclusion list: a model id or display name (one place), or a
    /// document place id (every color of it).
    pub fn resolve_sum(&self, name: &str) -> Option<Vec<usize>> {
        if let Some(i) = self.net.places.iter().position(|p| p == name) {
            return Some(vec![i]);
        }
        if let Some(i) = self.net.display.iter().position(|p| p == name) {
            return Some(vec![i]);
        }
        let c = self.copies(name);
        (!c.is_empty()).then(|| c.into_iter().map(|(i, _)| i).collect())
    }

    /// A caller's partial marking, `{place: n}` or `{place: [per color]}`
    /// (a `null` entry leaves that color free), as (place index, count)
    /// constraints. A display name (`place.color`) or model place id
    /// names one copy; a bare id of a multi-color place needs a list.
    pub fn partial(&self, v: &Value, what: &str) -> Result<Vec<(usize, i64)>> {
        let v = stochastic::json_arg(v, what)?;
        let obj = v.as_object().ok_or_else(|| {
            Error::BadArgs(format!(
                "{what} must be a JSON object of place -> count, e.g. {{\"done\": 1}}, got {v}"
            ))
        })?;
        let mut out: Vec<(usize, i64)> = Vec::new();
        let mut total = 0i64;
        let mut put = |i: usize, n: i64, key: &str| -> Result<()> {
            if let Some(&(_, old)) = out.iter().find(|(j, _)| *j == i) {
                if old != n {
                    return Err(Error::BadArgs(format!(
                        "{what}: {key} sets {} to {n}, but another key already set it to {old}",
                        self.net.display[i]
                    )));
                }
                return Ok(());
            }
            total += n;
            out.push((i, n));
            Ok(())
        };
        for (key, val) in obj {
            let single = self
                .net
                .places
                .iter()
                .position(|p| p == key)
                .or_else(|| self.net.display.iter().position(|p| p == key));
            match (single, val) {
                (Some(i), val) => {
                    let n = match val {
                        Value::Array(a) if a.len() == 1 => count(&a[0], what, key)?,
                        other => count(other, what, key)?,
                    };
                    put(i, n, key)?;
                }
                (None, Value::Array(a)) => {
                    let copies = self.copies(key);
                    if copies.is_empty() && self.doc_places.iter().all(|p| p != key) {
                        return Err(Error::NotFound(format!(
                            "{what}: no token place {key:?}; places are {:?}",
                            self.net.display
                        )));
                    }
                    for (c, x) in a.iter().enumerate() {
                        if x.is_null() {
                            continue;
                        }
                        let n = count(x, what, key)?;
                        match copies.iter().find(|(_, cc)| *cc == c) {
                            Some(&(i, _)) => put(i, n, key)?,
                            None if n == 0 => {}
                            None => {
                                return Err(Error::BadArgs(format!(
                                    "{what}: place {key:?} cannot hold color {c} (no arc moves it and it starts empty)"
                                )))
                            }
                        }
                    }
                }
                (None, val) => {
                    let copies = self.copies(key);
                    let n = count(val, what, key)?;
                    match copies.as_slice() {
                        [(i, _)] => put(*i, n, key)?,
                        [] => {
                            return Err(Error::NotFound(format!(
                                "{what}: no token place {key:?}; places are {:?}",
                                self.net.display
                            )))
                        }
                        many => {
                            return Err(Error::BadArgs(format!(
                                "{what}: place {key:?} holds several colors; give a per-color list or name one of {:?}",
                                many.iter()
                                    .map(|(i, _)| self.net.display[*i].clone())
                                    .collect::<Vec<_>>()
                            )))
                        }
                    }
                }
            }
        }
        if total > MAX_TOTAL_TOKENS {
            return Err(Error::BadArgs(format!(
                "{what}: the marking totals {total} tokens, over the {MAX_TOTAL_TOKENS} cap"
            )));
        }
        out.sort();
        Ok(out)
    }

    /// Constraints as `{display name: count}`.
    pub fn constraints_json(&self, c: &[(usize, i64)]) -> Value {
        Value::Object(
            c.iter()
                .map(|(i, n)| (self.net.display[*i].clone(), json!(n)))
                .collect(),
        )
    }

    // ----------------------------------------------------------- certificates

    /// A P-invariant separating `from` and the partial target `to`, if one
    /// is found: pass A (every `y` supported on `to`'s places, by exact
    /// elimination: complete for that class) and, when `to` leaves places
    /// free, pass B (the semi-positive Farkas basis).
    pub fn certificate(&self, from: &[i64], to: &[(usize, i64)]) -> Option<Certificate> {
        if let Some(c) = self.pass_a(from, to) {
            return Some(c);
        }
        self.pass_b(from, to)
    }

    fn pass_a(&self, from: &[i64], to: &[(usize, i64)]) -> Option<Certificate> {
        let t = self.local.len();
        // Rows [C_p | e_p] for the constrained places, fraction-free.
        let mut rows: Vec<(Vec<i128>, Vec<i128>)> = to
            .iter()
            .enumerate()
            .map(|(k, (p, _))| {
                let mut y = vec![0i128; to.len()];
                y[k] = 1;
                (self.incidence[*p].iter().map(|&x| x as i128).collect(), y)
            })
            .collect();
        let mut pivoted = vec![false; rows.len()];
        for j in 0..t {
            let Some(pv) = (0..rows.len()).find(|&r| !pivoted[r] && rows[r].0[j] != 0) else {
                continue;
            };
            pivoted[pv] = true;
            let (pc, py) = rows[pv].clone();
            for (r, row) in rows.iter_mut().enumerate() {
                if pivoted[r] || row.0[j] == 0 {
                    continue;
                }
                let (a, b) = (pc[j], row.0[j]);
                let mix = |x: &[i128], y: &[i128]| -> Option<Vec<i128>> {
                    x.iter()
                        .zip(y)
                        .map(|(u, v)| u.checked_mul(a)?.checked_sub(v.checked_mul(b)?))
                        .collect()
                };
                let (Some(mut c), Some(mut y)) = (mix(&row.0, &pc), mix(&row.1, &py)) else {
                    return None; // coefficients too large: no certificate
                };
                let g = c.iter().chain(&y).fold(0i128, |g, &x| gcd128(g, x));
                if g > 1 {
                    c.iter_mut().for_each(|x| *x /= g);
                    y.iter_mut().for_each(|x| *x /= g);
                }
                *row = (c, y);
            }
        }
        let mut best: Option<Certificate> = None;
        for (r, (_, y)) in rows.iter().enumerate() {
            if pivoted[r] || y.iter().all(|&x| x == 0) {
                continue;
            }
            let Some(y) = y
                .iter()
                .map(|&x| i64::try_from(x).ok())
                .collect::<Option<Vec<i64>>>()
            else {
                continue;
            };
            // Make the first nonzero coefficient positive, for readability.
            let sign = if y.iter().find(|&&x| x != 0).is_some_and(|&x| x < 0) {
                -1
            } else {
                1
            };
            let coefficients: Vec<(usize, i64)> = to
                .iter()
                .zip(&y)
                .filter(|(_, &c)| c != 0)
                .map(|((p, _), &c)| (*p, sign * c))
                .collect();
            let at_from: i64 = coefficients.iter().map(|(p, c)| c * from[*p]).sum();
            let at_target: i64 = coefficients
                .iter()
                .map(|(p, c)| c * to.iter().find(|(q, _)| q == p).map_or(0, |x| x.1))
                .sum();
            if at_from == at_target {
                continue;
            }
            let cand = Certificate {
                coefficients,
                free: vec![],
                at_from,
                at_target,
                pass: "equality",
            };
            if best.as_ref().is_none_or(|b| simpler(&cand, b)) {
                best = Some(cand);
            }
        }
        best
    }

    fn pass_b(&self, from: &[i64], to: &[(usize, i64)]) -> Option<Certificate> {
        let start = self.model_marking(from);
        let (laws, _) = stochastic::p_invariants(&self.net, &start);
        let mut best: Option<Certificate> = None;
        for law in laws {
            let coefficients: Vec<(usize, i64)> = law
                .terms
                .iter()
                .filter_map(|(name, c)| {
                    self.net
                        .display
                        .iter()
                        .position(|d| d == name)
                        .map(|i| (i, *c))
                })
                .collect();
            let at_from: i64 = coefficients.iter().map(|(p, c)| c * from[*p]).sum();
            let mut at_target = 0;
            let mut free = vec![];
            for (p, c) in &coefficients {
                match to.iter().find(|(q, _)| q == p) {
                    Some((_, n)) => at_target += c * n,
                    None => free.push(*p),
                }
            }
            // Free places hold ≥ 0 tokens, so a named sum already above the
            // invariant's value cannot be met; with none free, any
            // difference separates.
            let separates = if free.is_empty() {
                at_target != at_from
            } else {
                at_target > at_from
            };
            if !separates {
                continue;
            }
            let cand = Certificate {
                coefficients,
                free,
                at_from,
                at_target,
                pass: "semi-positive",
            };
            if best.as_ref().is_none_or(|b| simpler(&cand, b)) {
                best = Some(cand);
            }
        }
        best
    }

    pub fn certificate_json(&self, c: &Certificate) -> Value {
        let coeffs: Map<String, Value> = c
            .coefficients
            .iter()
            .map(|(p, k)| (self.net.display[*p].clone(), json!(k)))
            .collect();
        let expr = c
            .coefficients
            .iter()
            .enumerate()
            .map(|(k, (p, x))| {
                let name = &self.net.display[*p];
                let mag = x.abs();
                let term = if mag == 1 {
                    name.clone()
                } else {
                    format!("{mag}·{name}")
                };
                match (k, *x < 0) {
                    (0, true) => format!("-{term}"),
                    (0, false) => term,
                    (_, true) => format!(" - {term}"),
                    (_, false) => format!(" + {term}"),
                }
            })
            .collect::<String>();
        json!({
            "coefficients": coeffs,
            "expression": expr,
            "free": c.free.iter().map(|p| self.net.display[*p].clone()).collect::<Vec<_>>(),
            "atFrom": c.at_from,
            "atTarget": c.at_target,
            "pass": c.pass,
        })
    }
}

/// Fewer places first, then smaller coefficients (sim's candidate order).
fn simpler(a: &Certificate, b: &Certificate) -> bool {
    let max = |c: &Certificate| c.coefficients.iter().map(|x| x.1.abs()).max().unwrap_or(0);
    (a.coefficients.len(), max(a)) < (b.coefficients.len(), max(b))
}

fn gcd128(a: i128, b: i128) -> i128 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

fn count(v: &Value, what: &str, key: &str) -> Result<i64> {
    let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| {
        Error::BadArgs(format!(
            "{what}: count for {key:?} must be a number, got {v}"
        ))
    })?;
    if x.fract() != 0.0 {
        return Err(Error::BadArgs(format!(
            "{what}: count for {key:?} must be a whole number, got {v}"
        )));
    }
    if x < 0.0 {
        return Err(Error::BadArgs(format!(
            "{what}: count for {key:?} is negative ({v})"
        )));
    }
    if x > MAX_PLACE_TOKENS as f64 {
        return Err(Error::BadArgs(format!(
            "{what}: count for {key:?} is {v}, over the {MAX_PLACE_TOKENS} cap"
        )));
    }
    Ok(x as i64)
}

/// Whether `v` matches every constraint exactly.
pub fn matches(v: &[i64], to: &[(usize, i64)]) -> bool {
    to.iter().all(|(p, n)| v[*p] == *n)
}

// ================================================================== reach

/// sim_reach's answer, before it is turned into JSON and artifacts.
pub enum ReachAnswer {
    /// A shortest firing sequence (among the firings the search makes).
    Yes {
        path: Vec<usize>,
        states: usize,
    },
    No(NoBecause),
    Unknown {
        reasons: Vec<(String, String)>,
        states: usize,
    },
}

pub enum NoBecause {
    Invariant(Certificate),
    Exhaustive { states: usize },
}

impl Space {
    pub fn reach(&self, from: &[i64], to: &[(usize, i64)], max_states: usize) -> ReachAnswer {
        if matches(from, to) {
            return ReachAnswer::Yes {
                path: vec![],
                states: 1,
            };
        }
        if let Some(c) = self.certificate(from, to) {
            return ReachAnswer::No(NoBecause::Invariant(c));
        }
        let pred = |v: &[i64]| matches(v, to);
        let ex = self.explore(from.to_vec(), max_states, false, false, Some(&pred));
        let states = ex.states.len();
        if let Some(i) = ex.found {
            return ReachAnswer::Yes {
                path: self.path(&ex, i),
                states,
            };
        }
        if !ex.truncated && !ex.token_capped {
            return ReachAnswer::No(NoBecause::Exhaustive { states });
        }
        let mut reasons = vec![];
        if ex.truncated {
            reasons.push((
                "state-budget".to_string(),
                format!("the search stopped at its {max_states}-marking budget without meeting to; raise max_states (≤ {MAX_REACH_STATES}) or ask a narrower question"),
            ));
        }
        if ex.token_capped {
            reasons.push((
                "token-cap".to_string(),
                format!("some firing would pass {MAX_PLACE_TOKENS} tokens on a place and was not followed"),
            ));
        }
        ReachAnswer::Unknown { reasons, states }
    }
}

// ================================================================= verify

/// The property kinds sim's `sim_verify` accepts (go-pflow's `verify`).
pub const PROPERTY_KINDS: &[&str] = &[
    "deadlock-free",
    "bounded",
    "live",
    "terminating",
    "reachable",
    "unreachable",
    "invariant",
    "mutual-exclusion",
    "conserves",
];

const PROPERTY_FIELDS: &[&str] = &["kind", "name", "expr", "target", "places", "bound"];

/// One property as given, resolved against the net.
pub struct PropertyIn {
    /// The caller's JSON, echoed in the report.
    pub raw: Value,
    pub kind: String,
    /// `bounded` with a `bound`: every place ≤ k (checked here, since
    /// pflow-verify's `Bounded` has no k).
    pub k_bound: Option<i64>,
    pub property: pflow_verify::Property,
}

impl Space {
    /// Parses sim's property JSON: `{kind, name?, expr?, target?, places?,
    /// bound?}`. Place names may be document ids (every color summed),
    /// display names (`place.color`) or model ids.
    pub fn property(&self, raw: &Value) -> Result<PropertyIn> {
        use pflow_verify::{Kind, Property};
        let obj = raw.as_object().ok_or_else(|| {
            Error::BadArgs(format!(
                "a property must be an object like {{\"kind\": \"deadlock-free\"}}, got {raw}"
            ))
        })?;
        if let Some(k) = obj.keys().find(|k| !PROPERTY_FIELDS.contains(&k.as_str())) {
            return Err(Error::BadArgs(format!(
                "property field {k:?} is not one of {PROPERTY_FIELDS:?}"
            )));
        }
        let kind_raw = obj.get("kind").and_then(Value::as_str).ok_or_else(|| {
            Error::BadArgs(format!(
                "a property needs a kind, one of {PROPERTY_KINDS:?}"
            ))
        })?;
        let kind = match kind_raw.to_ascii_lowercase().replace('_', "-").as_str() {
            "deadlockfree" => "deadlock-free".to_string(),
            "mutex" | "mutualexclusion" => "mutual-exclusion".to_string(),
            k => k.to_string(),
        };
        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let bound = match obj.get("bound") {
            None | Some(Value::Null) => None,
            Some(b) => Some(count(b, "property", "bound")?),
        };
        let mut k_bound = None;
        let mut p = match kind.as_str() {
            "deadlock-free" => Property::new(Kind::DeadlockFree),
            "bounded" => {
                k_bound = bound.filter(|&b| b > 0);
                Property::new(Kind::Bounded)
            }
            "live" => Property::new(Kind::Live),
            "terminating" => Property::new(Kind::Terminating),
            "conserves" => Property::new(Kind::Conserves),
            "reachable" | "unreachable" => {
                let target = obj.get("target").ok_or_else(|| {
                    Error::BadArgs(format!("a {kind} property needs a target marking"))
                })?;
                let c = self.partial(target, "target")?;
                let t: HashMap<String, i64> = c
                    .into_iter()
                    .map(|(i, n)| (self.net.places[i].clone(), n))
                    .collect();
                if kind == "reachable" {
                    Property::reachable(t)
                } else {
                    Property::unreachable(t)
                }
            }
            "invariant" => {
                let expr = obj.get("expr").and_then(Value::as_str).ok_or_else(|| {
                    Error::BadArgs(
                        "an invariant property needs expr, e.g. \"a + 2*b == 10\"".into(),
                    )
                })?;
                Property::invariant(self.expand_expr(expr)?)
            }
            "mutual-exclusion" => {
                let names: Vec<String> = obj
                    .get("places")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                if names.is_empty() {
                    return Err(Error::BadArgs(
                        "a mutual-exclusion property needs places: [..]".into(),
                    ));
                }
                let mut places = vec![];
                for n in &names {
                    let idx = self.resolve_sum(n).ok_or_else(|| {
                        Error::NotFound(format!(
                            "mutual-exclusion: no place {n:?}; places are {:?}",
                            self.net.display
                        ))
                    })?;
                    for i in idx {
                        let id = self.net.places[i].clone();
                        if !places.contains(&id) {
                            places.push(id);
                        }
                    }
                }
                Property::mutual_exclusion(places, bound.unwrap_or(1))
            }
            other => {
                return Err(Error::BadArgs(format!(
                    "unknown property kind {other:?}; one of {PROPERTY_KINDS:?}"
                )))
            }
        };
        p.name = name;
        Ok(PropertyIn {
            raw: raw.clone(),
            kind,
            k_bound,
            property: p,
        })
    }

    /// Rewrites an expression's place names into model ids (a document id
    /// of a colored place becomes the sum of its colors), as go-pflow's
    /// `expandExprColors` does. Unknown names are kept, so pflow-verify
    /// reports them.
    fn expand_expr(&self, expr: &str) -> Result<String> {
        let parsed = pflow_verify::parse_expr(expr)
            .map_err(|e| Error::BadArgs(format!("expr {expr:?}: {e}")))?;
        let mut coeffs: HashMap<String, i64> = HashMap::new();
        for (name, c) in &parsed.coeffs {
            match self.resolve_sum(name) {
                Some(idx) => {
                    for i in idx {
                        *coeffs.entry(self.net.places[i].clone()).or_insert(0) += c;
                    }
                }
                None => *coeffs.entry(name.clone()).or_insert(0) += c,
            }
        }
        coeffs.retain(|_, c| *c != 0);
        Ok(pflow_verify::LinearExpr {
            coeffs,
            rel: parsed.rel,
            constant: parsed.constant,
        }
        .render())
    }

    /// Every place ≤ k from `start`: structurally (each place under a
    /// semi-positive P-invariant whose value bounds it by k), else by
    /// exhaustive search, with the shortest trace to a violation.
    pub fn check_k_bounded(&self, start: &[i64], k: i64, max_states: usize) -> KBounded {
        let (laws, _) = stochastic::p_invariants(&self.net, &self.model_marking(start));
        let mut bound: Vec<Option<i64>> = vec![None; self.place_count()];
        let mut why: Vec<String> = vec![String::new(); self.place_count()];
        for law in &laws {
            for (name, c) in &law.terms {
                if let Some(i) = self.net.display.iter().position(|d| d == name) {
                    let b = law.value / c;
                    if bound[i].is_none_or(|x| b < x) {
                        bound[i] = Some(b);
                        why[i] = law.expression();
                    }
                }
            }
        }
        if bound.iter().all(|b| b.is_some_and(|b| b <= k)) {
            let mut ev: Vec<String> = why.clone();
            ev.sort();
            ev.dedup();
            return KBounded::Proved {
                structural: true,
                states: 0,
                evidence: format!("P-invariants: {}", ev.join("; ")),
            };
        }
        let pred = |v: &[i64]| v.iter().any(|&x| x > k);
        let ex = self.explore(start.to_vec(), max_states, false, false, Some(&pred));
        if let Some(i) = ex.found {
            let v = &ex.states[i];
            let p = v.iter().position(|&x| x > k).unwrap_or(0);
            return KBounded::Refuted {
                path: self.path(&ex, i),
                marking: v.clone(),
                place: self.net.display[p].clone(),
                partial: ex.truncated,
            };
        }
        if ex.truncated || ex.token_capped {
            KBounded::Unknown {
                states: ex.states.len(),
            }
        } else {
            KBounded::Proved {
                structural: false,
                states: ex.states.len(),
                evidence: String::new(),
            }
        }
    }
}

pub enum KBounded {
    Proved {
        structural: bool,
        states: usize,
        evidence: String,
    },
    Refuted {
        path: Vec<usize>,
        marking: Vec<i64>,
        place: String,
        partial: bool,
    },
    Unknown {
        states: usize,
    },
}

pub fn status_str(s: pflow_verify::Status) -> &'static str {
    match s {
        pflow_verify::Status::Proved => "proved",
        pflow_verify::Status::Refuted => "refuted",
        pflow_verify::Status::Unknown => "unknown",
    }
}

pub fn method_str(m: pflow_verify::Method) -> &'static str {
    match m {
        pflow_verify::Method::Structural => "structural",
        pflow_verify::Method::Exhaustive => "exhaustive",
        pflow_verify::Method::Witness => "witness",
        pflow_verify::Method::Partial => "partial",
        pflow_verify::Method::None => "none",
    }
}

// =============================================================== diagnose

/// One knob of [`diagnose`].
#[derive(Debug, Clone)]
pub struct Knob {
    /// "rate infect" or "+1 S".
    pub name: String,
    /// "rate" or "token".
    pub kind: &'static str,
    pub id: String,
    /// The knob's own base value (a rate, or tokens).
    pub base: f64,
    /// The observable with the knob turned down (rates only) and up.
    pub lo: Option<f64>,
    pub hi: f64,
    /// Signed: hi − lo for a rate, hi − baseline for a token.
    pub influence: f64,
}

pub struct Diagnosis {
    pub observable: Vec<String>,
    pub baseline: f64,
    /// Sorted by |influence|, largest first.
    pub knobs: Vec<Knob>,
    /// Transitions at rate 0 (dormant: not knobs, as in sim).
    pub dormant: Vec<String>,
}

/// Sinks: token places something produces into and nothing consumes
/// (sim's terminal outcome places), by display name.
pub fn sinks(space: &Space) -> Vec<String> {
    let m = &space.net.model;
    space
        .net
        .places
        .iter()
        .enumerate()
        .filter(|(_, id)| {
            let produced = m
                .transitions
                .iter()
                .any(|t| m.outputs(&t.id).iter().any(|a| &a.place == *id));
            let consumed = m
                .transitions
                .iter()
                .any(|t| m.inputs(&t.id).iter().any(|a| &a.place == *id));
            produced && !consumed
        })
        .map(|(i, _)| space.net.display[i].clone())
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn diagnose(
    doc: &Document,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    observable: &[String],
    t_end: f64,
    delta: f64,
    opts: &crate::sim::OdeOptions,
) -> Result<Diagnosis> {
    let runner = Runner::new(doc, mk, rates, t_end, "tsit5", opts)?;
    let mut cols = vec![];
    for o in observable {
        for i in runner.select(o)? {
            if !cols.contains(&i) {
                cols.push(i);
            }
        }
    }
    let read = |r: &Runner, ov: &[(&str, f64)]| -> f64 {
        let f = r.final_state(ov);
        cols.iter().map(|&i| f[i]).sum()
    };
    let baseline = read(&runner, &[]);
    let mut knobs = vec![];
    let mut dormant = vec![];
    for t in &doc.transitions {
        let base = rates.get(&t.id).copied().unwrap_or(1.0);
        if base <= 0.0 {
            dormant.push(t.id.clone());
            continue;
        }
        let lo = read(&runner, &[(&t.id, base * (1.0 - delta))]);
        let hi = read(&runner, &[(&t.id, base * (1.0 + delta))]);
        knobs.push(Knob {
            name: format!("rate {}", t.id),
            kind: "rate",
            id: t.id.clone(),
            base,
            lo: Some(lo),
            hi,
            influence: hi - lo,
        });
    }
    // +1 initial token on each place (each color it can hold): sim's pool
    // probe, applied to every place rather than inferred pools only.
    let n_colors = doc.colors();
    let short = pflow_core::colors::short_color_names(&doc.token);
    let locate = |display: &str| -> (String, usize) {
        if n_colors > 1 {
            for p in &doc.places {
                for (c, name) in short.iter().enumerate() {
                    if display == format!("{}.{name}", p.id) {
                        return (p.id.clone(), c);
                    }
                }
            }
        }
        (display.to_string(), 0)
    };
    for (_, display) in runner.order() {
        let (place, color) = locate(display);
        let mut m2 = mk.clone();
        let slot = m2.entry(place.clone()).or_insert_with(|| vec![0; n_colors]);
        slot.resize(n_colors.max(slot.len()), 0);
        let base = slot[color];
        slot[color] += 1;
        let r2 = Runner::new(doc, &m2, rates, t_end, "tsit5", opts)?;
        let hi = read(&r2, &[]);
        knobs.push(Knob {
            name: format!("+1 {display}"),
            kind: "token",
            id: display.clone(),
            base: base as f64,
            lo: None,
            hi,
            influence: hi - baseline,
        });
    }
    knobs.sort_by(|a, b| {
        b.influence
            .abs()
            .partial_cmp(&a.influence.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(Diagnosis {
        observable: observable.to_vec(),
        baseline,
        knobs,
        dormant,
    })
}

/// sim's noise floor: the spread (max − min) of the observable's final
/// value across three single SSA realizations, seeds `seed+1+977·i`.
pub fn noise_floor(
    doc: &Document,
    mk: &Marking,
    rates: &BTreeMap<String, f64>,
    observable: &[String],
    t_end: f64,
    seed: u64,
) -> Result<(f64, Vec<f64>)> {
    let net = Net::new(doc)?;
    let mut vals = vec![];
    for i in 0..3u64 {
        let e = stochastic::run_ssa(&net, mk, rates, t_end, 2, 1, seed + 1 + i * 977)?;
        let f = e.finals();
        let order: Vec<(String, String)> = e.names.iter().map(|n| (n.clone(), n.clone())).collect();
        let mut v = 0.0;
        for o in observable {
            for k in crate::sim::select_series(&order, o) {
                v += f.get(&e.names[k]).copied().unwrap_or(0.0);
            }
        }
        vals.push(v);
    }
    let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    Ok((hi - lo, vals))
}

/// Refuses a relative perturbation outside (0, 1].
pub fn check_delta(delta: f64) -> Result<()> {
    if delta > 0.0 && delta <= 1.0 {
        Ok(())
    } else {
        Err(Error::BadArgs(format!(
            "delta {delta} must be in (0, 1]: rates are probed at rate·(1 ± delta)"
        )))
    }
}

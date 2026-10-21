//! Structural analysis of the document's net: incidence, P/T invariants,
//! siphons and traps, exact automorphism orbits with a canonical id, and
//! colour-refinement classes. Everything reads the same
//! `pflow_metamodel::Model` the token game fires (`sim::Compiled`, via
//! [`crate::stochastic::Net`]), so colours, inhibitor arcs and read arcs mean
//! what they mean everywhere else; results name model places, which
//! [`Structure`] maps back to document ids (`place`, or `place.color` as a
//! display name on a coloured net).
//!
//! The algorithms themselves ([`farkas`], [`siphons`], [`symmetry`]) take
//! plain matrices, bitmasks or a `Model`, with no document or session types,
//! so they can move upstream into pflow-reachability unchanged (FEATURES.md
//! §D gaps 2 and 3). Every one is deterministic and bounded: over a cap is a
//! refusal or an explicit `truncated`, never a hang.

pub mod farkas;
pub mod siphons;
pub mod symmetry;

use std::collections::HashMap;

use pflow_metamodel::{ArcType, Model};

use crate::doc::Document;
use crate::sim::Marking;
use crate::stochastic::Net;
use crate::Result;

/// The net as the structural analyses see it: the compiled model, its
/// token places in display order (document order, then colour; on a
/// coloured net only the colour copies something can hold), its transitions
/// in document order, and the flow-arc pre/post matrices over them.
pub struct Structure {
    pub net: Net,
    /// Model ids of the analysed token places, in display order.
    pub places: Vec<String>,
    /// Display names aligned with `places`: the document id, or
    /// `place.color` on a coloured net.
    pub display: Vec<String>,
    /// Document place id aligned with `places`.
    pub base: Vec<String>,
    /// Transition ids, in document order.
    pub transitions: Vec<String>,
    /// `pre[p][t]`: tokens transition `t` consumes from place `p` (flow arcs
    /// only; read and inhibitor arcs move nothing).
    pub pre: Vec<Vec<i64>>,
    /// `post[p][t]`: tokens `t` produces into `p`.
    pub post: Vec<Vec<i64>>,
}

impl Structure {
    /// Compiles `doc`. `mk` (the session marking) only decides which colour
    /// copies of a coloured place count as used: initially marked, marked
    /// now, or touched by any arc.
    pub fn new(doc: &Document, mk: &Marking) -> Result<Self> {
        let net = Net::new(doc)?;
        let current = net.model_marking(mk);
        let touched = |id: &str| net.model.arcs.iter().any(|a| a.from == id || a.to == id);
        let mut places = vec![];
        let mut display = vec![];
        let mut base = vec![];
        for (id, name) in net.places.iter().zip(&net.display) {
            if net.colors.is_some() {
                let initial = net.model.place_by_id(id).map_or(0, |p| p.initial);
                let now = current.get(id).copied().unwrap_or(0);
                if initial <= 0 && now <= 0 && !touched(id) {
                    continue;
                }
            }
            places.push(id.clone());
            display.push(name.clone());
            base.push(net.base(id).0);
        }
        let in_model: std::collections::HashSet<&str> = net
            .model
            .transitions
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        let mut transitions: Vec<String> = doc
            .transitions
            .iter()
            .filter(|t| in_model.contains(t.id.as_str()))
            .map(|t| t.id.clone())
            .collect();
        // Anything the parser added that the document does not list (none
        // today) still takes part, after the document's own.
        for t in &net.model.transitions {
            if !transitions.contains(&t.id) {
                transitions.push(t.id.clone());
            }
        }
        let pidx: HashMap<&str, usize> = places
            .iter()
            .enumerate()
            .map(|(i, p)| (p.as_str(), i))
            .collect();
        let mut pre = vec![vec![0i64; transitions.len()]; places.len()];
        let mut post = vec![vec![0i64; transitions.len()]; places.len()];
        for (j, t) in transitions.iter().enumerate() {
            for a in net.model.inputs(t) {
                if let Some(&p) = pidx.get(a.place.as_str()) {
                    pre[p][j] += a.weight;
                }
            }
            for a in net.model.outputs(t) {
                if let Some(&p) = pidx.get(a.place.as_str()) {
                    post[p][j] += a.weight;
                }
            }
        }
        Ok(Self {
            net,
            places,
            display,
            base,
            transitions,
            pre,
            post,
        })
    }

    pub fn model(&self) -> &Model {
        &self.net.model
    }

    pub fn is_colored(&self) -> bool {
        self.net.colors.is_some()
    }

    /// `C = post − pre`, places × transitions.
    pub fn incidence(&self) -> Vec<Vec<i64>> {
        self.pre
            .iter()
            .zip(&self.post)
            .map(|(a, b)| b.iter().zip(a).map(|(o, i)| o - i).collect())
            .collect()
    }

    /// The marking as a vector over `places`.
    pub fn marking(&self, mk: &Marking) -> Vec<i64> {
        let m = self.net.model_marking(mk);
        self.places
            .iter()
            .map(|p| m.get(p).copied().unwrap_or(0))
            .collect()
    }

    /// (document node id, display name) of a model node: a place maps to
    /// its base place, a transition to itself.
    pub fn node(&self, model_id: &str) -> (String, String) {
        if self.net.model.place_by_id(model_id).is_some() {
            (self.net.base(model_id).0, self.net.display_of(model_id))
        } else {
            (model_id.to_string(), model_id.to_string())
        }
    }

    /// Document ids of model nodes, deduplicated in first-seen order (two
    /// colours of one place are one canvas node).
    pub fn doc_nodes<'a>(&self, ids: impl IntoIterator<Item = &'a String>) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        for id in ids {
            let (base, _) = self.node(id);
            if !out.contains(&base) {
                out.push(base);
            }
        }
        out
    }

    /// Whether every flow arc has weight 1 and no arc only tests (no read
    /// or inhibitor arcs): the nets Commoner's theorem speaks about.
    pub fn is_ordinary(&self) -> bool {
        self.pre.iter().chain(&self.post).flatten().all(|&w| w <= 1)
            && self.model().arcs.iter().all(|a| a.typ == ArcType::Normal)
    }

    /// (free choice, extended free choice) over the flow arcs: two
    /// transitions sharing an input place have that place as their only
    /// input (FC), or have identical presets (EFC).
    pub fn free_choice(&self) -> (bool, bool) {
        let nt = self.transitions.len();
        let preset = |t: usize| -> Vec<usize> {
            (0..self.places.len())
                .filter(|&p| self.pre[p][t] > 0)
                .collect()
        };
        let presets: Vec<Vec<usize>> = (0..nt).map(preset).collect();
        let mut fc = true;
        let mut efc = true;
        for p in 0..self.places.len() {
            let outs: Vec<usize> = (0..nt).filter(|&t| self.pre[p][t] > 0).collect();
            if outs.len() < 2 {
                continue;
            }
            if outs.iter().any(|&t| presets[t].len() != 1) {
                fc = false;
            }
            if outs.iter().any(|&t| presets[t] != presets[outs[0]]) {
                efc = false;
            }
        }
        (fc && efc, efc)
    }
}

/// Go's `%g` for a float64 (`strconv.FormatFloat(v, 'g', -1, 64)`): the
/// shortest round-trip digits, in exponent form when the decimal exponent
/// is below −4 or at least 6. sim's colour-refinement seeds are written
/// with it, so a canonical id only matches sim's if rates print the same.
pub fn go_g(v: f64) -> String {
    if v.is_nan() {
        return "NaN".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { "+Inf" } else { "-Inf" }.into();
    }
    if v == 0.0 {
        return if v.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let sci = format!("{:e}", v.abs());
    let (mant, exp) = sci.split_once('e').expect("{:e} has an exponent");
    let exp: i32 = exp.parse().expect("integer exponent");
    let sign = if v < 0.0 { "-" } else { "" };
    if !(-4..6).contains(&exp) {
        let es = if exp < 0 { '-' } else { '+' };
        format!("{sign}{mant}e{es}{:02}", exp.abs())
    } else {
        format!("{sign}{}", v.abs())
    }
}

#[cfg(test)]
mod tests {
    use super::go_g;

    #[test]
    fn go_g_matches_go() {
        for (v, want) in [
            (1.0, "1"),
            (0.5, "0.5"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (123456.0, "123456"),
            (1_000_000.0, "1e+06"),
            (1_234_567.0, "1.234567e+06"),
            (-2.5, "-2.5"),
            (1e21, "1e+21"),
            (1.5e-7, "1.5e-07"),
            (1e100, "1e+100"),
        ] {
            assert_eq!(go_g(v), want, "{v}");
        }
    }
}

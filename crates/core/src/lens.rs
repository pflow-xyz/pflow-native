//! The lens test for a composition, and the fallback when it fails: a port
//! of pflow-views-prototype `js/model/lensfind.js`.
//!
//! Strict test: is the composition, as pflow-compose flattens it, a lens
//! composite? It would need (1) a boundary to compose along (a token or data
//! link: lens composition is sequential), (2) a backward map in every part,
//! and (3) the composite's backward pass to be the parts' backward maps
//! fired in step, as the forward pass is. Each is decided from the subnets,
//! the link rules ([`crate::composition::validate_link`], pflow-compose's
//! `Validate`) and the flat net (`Bundle::flatten_with_map`), never from a
//! reading.
//!
//! Fallback ("assume a backward pass"): every transition pair with a
//! backward pass found mechanically. A pair (t, u) in one subnet is a
//! forward/backward pair when some place r has t as its only producer and
//! u as its only consumer (r is the residual), and u returns what t moved
//! on every resource place. The forward pass is the `commit` scenario's
//! (judgment).

use std::collections::{BTreeMap, BTreeSet};

use pflow_compose::{Link, LinkKind, Subnet};
use pflow_metamodel::Model;
use serde::Serialize;

pub use crate::composition::Run;
use crate::composition::{self as comp, Flat, Verdict};

type Column = BTreeMap<String, i64>;

/// Incidence column per transition: place → post − pre (read and inhibitor
/// arcs move nothing).
pub fn deltas(s: &Subnet) -> BTreeMap<String, Column> {
    let mut d: BTreeMap<String, Column> = s
        .model
        .transitions
        .iter()
        .map(|t| (t.id.clone(), Column::new()))
        .collect();
    for a in &s.model.arcs {
        if a.is_read_only() {
            continue;
        }
        if let Some(c) = d.get_mut(&a.to) {
            *c.entry(a.from.clone()).or_default() -= a.weight;
        } else if let Some(c) = d.get_mut(&a.from) {
            *c.entry(a.to.clone()).or_default() += a.weight;
        }
    }
    d
}

fn at(c: &Column, p: &str) -> i64 {
    c.get(p).copied().unwrap_or(0)
}

/// A forward/backward pair in one subnet.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Pair {
    pub subnet: String,
    pub t: String,
    pub u: String,
    /// Places only t produces and only u consumes.
    pub residuals: Vec<String>,
    /// Resource places t moves tokens on.
    pub resources: Vec<String>,
    /// u's whole column is minus t's (a plain inverse).
    pub exact: bool,
}

/// Insertion-ordered set (JS `Set`).
fn push_unique(v: &mut Vec<String>, x: &str) {
    if !v.iter().any(|y| y == x) {
        v.push(x.to_string());
    }
}

/// The forward/backward pairs of one subnet (`backwardPairs`).
pub fn backward_pairs(s: &Subnet) -> Vec<Pair> {
    let d = deltas(s);
    let mut producers: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    let mut consumers: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for a in &s.model.arcs {
        if a.is_read_only() {
            continue;
        }
        if d.contains_key(&a.from) {
            push_unique(producers.entry(&a.to).or_default(), &a.from);
        }
        if d.contains_key(&a.to) {
            push_unique(consumers.entry(&a.from).or_default(), &a.to);
        }
    }
    let resources: Vec<&str> = s
        .model
        .places
        .iter()
        .filter(|p| p.resource)
        .map(|p| p.id.as_str())
        .collect();
    let mut out: Vec<Pair> = vec![];
    for p in &s.model.places {
        let (Some(pr), Some(co)) = (producers.get(p.id.as_str()), consumers.get(p.id.as_str()))
        else {
            continue;
        };
        if pr.len() != 1 || co.len() != 1 {
            continue;
        }
        let (t, u) = (&pr[0], &co[0]);
        if t == u {
            continue;
        }
        let (dt, du) = (&d[t], &d[u]);
        if !resources.iter().all(|r| at(du, r) == -at(dt, r)) {
            continue;
        }
        let places: BTreeSet<&String> = dt.keys().chain(du.keys()).collect();
        let exact = places.iter().all(|q| at(du, q) == -at(dt, q));
        match out.iter_mut().find(|x| &x.t == t && &x.u == u) {
            Some(x) => x.residuals.push(p.id.clone()),
            None => out.push(Pair {
                subnet: s.id.clone(),
                t: t.clone(),
                u: u.clone(),
                residuals: vec![p.id.clone()],
                resources: resources
                    .iter()
                    .filter(|r| at(dt, r) != 0)
                    .map(|r| r.to_string())
                    .collect(),
                exact,
            }),
        }
    }
    out
}

/// A constraint that holds at most one token over its places.
struct Exclusive {
    id: String,
    places: BTreeSet<String>,
}

/// Constraints of the form `tokens(a) + tokens(b) + … == 1` over two or
/// more places: a transition needing two of them can never fire.
fn exclusive_sets(m: &Model) -> Vec<Exclusive> {
    let mut sets = vec![];
    for c in &m.constraints {
        let Some(lhs) = c.expr.strip_suffix(" == 1") else {
            continue;
        };
        let terms: Vec<&str> = lhs.split(" + ").collect();
        let places: Vec<String> = terms
            .iter()
            .filter_map(|x| {
                x.trim()
                    .strip_prefix("tokens(\"")
                    .and_then(|r| r.strip_suffix("\")"))
                    .filter(|p| !p.contains('"'))
                    .map(str::to_string)
            })
            .collect();
        if places.len() == terms.len() && places.len() > 1 {
            sets.push(Exclusive {
                id: c.id.clone(),
                places: places.into_iter().collect(),
            });
        }
    }
    sets
}

/// Why a flat transition can never fire, by an exclusive constraint.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dead {
    pub constraint: String,
    pub places: Vec<String>,
}

/// `deadBy`: the first exclusive constraint two or more of `tid`'s input
/// places fall under.
pub fn dead_by(m: &Model, tid: &str) -> Option<Dead> {
    let mut pre: Vec<String> = vec![];
    for a in &m.arcs {
        if a.to == tid && !a.is_read_only() {
            push_unique(&mut pre, &a.from);
        }
    }
    for s in exclusive_sets(m) {
        let both: Vec<String> = pre
            .iter()
            .filter(|p| s.places.contains(*p))
            .cloned()
            .collect();
        if both.len() > 1 {
            return Some(Dead {
                constraint: s.id,
                places: both,
            });
        }
    }
    None
}

/// A pair with its ends qualified (`subnet/t`, as the JS port overrides
/// them) and placed in the flat net.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Partner {
    pub subnet: String,
    pub t: String,
    pub u: String,
    pub residuals: Vec<String>,
    pub resources: Vec<String>,
    pub exact: bool,
    pub t_flat: Option<String>,
    pub u_flat: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeadBack {
    pub flat: String,
    pub dead: Dead,
}

/// One forward rendezvous of the commit run, and whether it composes as a
/// lens.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LensStep {
    pub flat: String,
    pub members: Vec<String>,
    /// The members' backward partners.
    pub back: Vec<Partner>,
    pub back_flat: Vec<String>,
    /// The backward maps are more than one flat transition.
    pub split: bool,
    pub dead: Vec<DeadBack>,
    pub dead_forward: Option<Dead>,
    pub trips: Vec<Partner>,
    pub composes: bool,
    pub no_backward: bool,
}

/// The whole analysis (`analyse`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Analysis {
    /// False when the composition does not flatten (`errors` says why).
    pub ok: bool,
    /// Legal links among the subnets by kind, every ordered port pair.
    pub legal: BTreeMap<&'static str, usize>,
    /// Token and data links: boundaries glued.
    pub glued: usize,
    pub pairs: Vec<Pair>,
    /// Subnets with no backward pair.
    pub no_backward: Vec<String>,
    pub errors: Vec<String>,
    pub forward_flat: Vec<String>,
    pub forward_members: Vec<String>,
    /// Forward member → its backward partner (not itself forward).
    pub partner: Vec<Partner>,
    /// Pairs whose both ends are forward, oriented by the commit run.
    pub round_trips: Vec<Partner>,
    pub steps: Vec<LensStep>,
    /// Every scenario but `second-order`.
    pub runs: Vec<Run>,
    /// The strict test.
    pub holds: bool,
}

fn qualify(subnet: &str, local: &str) -> String {
    format!("{subnet}/{local}")
}

/// The analysis of `subnets` composed by `links`, given its flat net.
pub fn analyse(subnets: &[&Subnet], links: &[Link], flat: &Result<Flat, String>) -> Analysis {
    let legal = comp::legal_link_counts(subnets);
    let glued = links
        .iter()
        .filter(|l| matches!(l.kind, LinkKind::Token | LinkKind::Data))
        .count();
    let pairs: Vec<Pair> = subnets.iter().flat_map(|s| backward_pairs(s)).collect();
    let with_backward: BTreeSet<&str> = pairs.iter().map(|p| p.subnet.as_str()).collect();
    let no_backward: Vec<String> = subnets
        .iter()
        .filter(|s| !with_backward.contains(s.id.as_str()))
        .map(|s| s.id.clone())
        .collect();
    let mut a = Analysis {
        ok: false,
        legal,
        glued,
        pairs: pairs.clone(),
        no_backward,
        errors: vec![],
        forward_flat: vec![],
        forward_members: vec![],
        partner: vec![],
        round_trips: vec![],
        steps: vec![],
        runs: vec![],
        holds: false,
    };
    let flat = match flat {
        Ok(f) => f,
        Err(e) => {
            a.errors.push(e.clone());
            return a;
        }
    };
    a.ok = true;
    let flat_t = |m: &str| flat.transition_of(m).map(str::to_string);
    let members_of = flat.transition_members();
    let commit = comp::scenario("commit").expect("the commit scenario");
    for m in &commit.script {
        if let Some(f) = flat_t(m) {
            push_unique(&mut a.forward_flat, &f);
        }
    }
    for f in &a.forward_flat {
        for m in members_of.get(f).into_iter().flatten() {
            push_unique(&mut a.forward_members, m);
        }
    }
    let fwd_index = |m: &str| flat_t(m).and_then(|f| a.forward_flat.iter().position(|x| *x == f));
    let mut partner: Vec<Partner> = vec![];
    let mut round_trips: Vec<Partner> = vec![];
    for p in &pairs {
        let (t, u) = (qualify(&p.subnet, &p.t), qualify(&p.subnet, &p.u));
        if !a.forward_members.contains(&t) {
            continue;
        }
        let entry = Partner {
            subnet: p.subnet.clone(),
            t_flat: flat_t(&t),
            u_flat: flat_t(&u),
            t: t.clone(),
            u: u.clone(),
            residuals: p.residuals.clone(),
            resources: p.resources.clone(),
            exact: p.exact,
        };
        if a.forward_members.contains(&u) {
            if fwd_index(&t) < fwd_index(&u) {
                round_trips.push(entry);
            }
            continue;
        }
        // A later pair for the same forward member replaces the earlier one
        // in place, as a JS object key does.
        match partner.iter_mut().find(|x| x.t == t) {
            Some(x) => *x = entry,
            None => partner.push(entry),
        }
    }
    for f in &a.forward_flat {
        let members = members_of.get(f).cloned().unwrap_or_default();
        let back: Vec<Partner> = members
            .iter()
            .filter_map(|m| partner.iter().find(|x| &x.t == m).cloned())
            .collect();
        let mut back_flat: Vec<String> = vec![];
        for b in &back {
            if let Some(uf) = &b.u_flat {
                push_unique(&mut back_flat, uf);
            }
        }
        let dead: Vec<DeadBack> = back_flat
            .iter()
            .filter_map(|b| {
                dead_by(&flat.model, b).map(|d| DeadBack {
                    flat: b.clone(),
                    dead: d,
                })
            })
            .collect();
        let dead_forward = dead_by(&flat.model, f);
        let trips: Vec<Partner> = round_trips
            .iter()
            .filter(|r| r.t_flat.as_deref() == Some(f) || r.u_flat.as_deref() == Some(f))
            .cloned()
            .collect();
        let composes =
            dead_forward.is_none() && !back.is_empty() && back_flat.len() == 1 && dead.is_empty();
        a.steps.push(LensStep {
            flat: f.clone(),
            members,
            no_backward: back.is_empty(),
            split: back_flat.len() > 1,
            back,
            back_flat,
            dead,
            dead_forward,
            trips,
            composes,
        });
    }
    a.runs = comp::scenarios()
        .iter()
        .filter(|s| s.id != "second-order")
        .map(|s| comp::run_scenario(flat, s))
        .collect();
    a.holds = glued > 0
        && a.no_backward.is_empty()
        && round_trips.is_empty()
        && a.steps
            .iter()
            .all(|s| s.composes || (s.no_backward && s.dead_forward.is_none()));
    a.partner = partner;
    a.round_trips = round_trips;
    a
}

/// A candidate repair: one event link, added or moved.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fix {
    pub step: String,
    pub u: String,
    pub target: String,
    /// `"add"` (to the links) or `"move"` (in place of the links that fuse
    /// u elsewhere).
    pub how: &'static str,
    pub link: serde_json::Value,
    pub legal: bool,
    pub verdict: Verdict,
    pub removed: Vec<serde_json::Value>,
    /// The analysis with the repair in place, when it is legal and
    /// flattens.
    pub result: Option<Box<Analysis>>,
}

/// `tryFixes`: for each step that does not compose, try each backward map
/// u that does not fire with the step's saga backward map v as the link
/// u ⊗ v, added and in place of u's other event links; each candidate is
/// validated, flattened and analysed again (the scenarios re-run).
pub fn try_fixes(
    subnets: &[&Subnet],
    patterns: &[(&str, &str)],
    links: &[Link],
    a: &Analysis,
) -> Vec<Fix> {
    if !a.ok {
        return vec![];
    }
    let saga = patterns
        .iter()
        .find(|(_, p)| *p == "saga")
        .map(|(id, _)| *id);
    let port_of = |member: &str| -> Option<pflow_compose::Endpoint> {
        let (sid, tid) = member.split_once('/')?;
        let s = subnets.iter().find(|s| s.id == sid)?;
        let p = s
            .ports
            .iter()
            .find(|q| q.is_transition() && q.transition == tid)?;
        Some(comp::ep(sid, &p.id))
    };
    let touches = |l: &Link, member: &str| {
        [&l.from, &l.to].into_iter().any(|r| {
            subnets
                .iter()
                .find(|s| s.id == r.subnet)
                .and_then(|s| s.port_by_id(&r.port))
                .is_some_and(|p| {
                    !p.transition.is_empty() && qualify(&r.subnet, &p.transition) == member
                })
        })
    };
    let mut out = vec![];
    for s in &a.steps {
        let back: Vec<String> = s
            .back
            .iter()
            .map(|b| b.u.clone())
            .chain(
                s.trips
                    .iter()
                    .filter(|r| r.t_flat.as_deref() == Some(s.flat.as_str()))
                    .map(|r| r.u.clone()),
            )
            .collect();
        let target = back.iter().find(|u| {
            a.partner
                .iter()
                .find(|x| &&x.u == u)
                .is_some_and(|x| Some(x.subnet.as_str()) == saga)
        });
        let Some(target) = target else {
            continue;
        };
        for u in &back {
            if u == target {
                continue;
            }
            let (Some(from), Some(to)) = (port_of(u), port_of(target)) else {
                continue;
            };
            let l = comp::link(LinkKind::Event, from, to);
            let verdict = comp::validate_link(subnets, &l);
            for how in ["add", "move"] {
                let base: Vec<Link> = if how == "add" {
                    links.to_vec()
                } else {
                    links
                        .iter()
                        .filter(|x| !(x.kind == LinkKind::Event && touches(x, u)))
                        .cloned()
                        .collect()
                };
                if how == "move" && base.len() == links.len() {
                    continue;
                }
                let removed: Vec<serde_json::Value> = links
                    .iter()
                    .filter(|x| !base.iter().any(|y| comp::link_key(y) == comp::link_key(x)))
                    .map(comp::link_json)
                    .collect();
                let mut next = base;
                next.push(l.clone());
                let result = if verdict.ok {
                    let mut b = pflow_compose::Bundle::new("lens-fix");
                    for s in subnets {
                        b.add_subnet((*s).clone());
                    }
                    b.links = next.clone();
                    let f = comp::flatten(&b);
                    f.is_ok().then(|| Box::new(analyse(subnets, &next, &f)))
                } else {
                    None
                };
                out.push(Fix {
                    step: s.flat.clone(),
                    u: u.clone(),
                    target: target.clone(),
                    how,
                    link: comp::link_json(&l),
                    legal: verdict.ok,
                    verdict: verdict.clone(),
                    removed,
                    result,
                });
            }
        }
    }
    out
}

/// `bundle.lens`'s result for a composition: `{analysis, fixes}`.
pub fn report(c: &comp::Composition, fixes: bool) -> serde_json::Value {
    let subnets = c.subnets();
    let flat = comp::flatten(&c.bundle);
    let analysis = analyse(&subnets, &c.bundle.links, &flat);
    let fixes = if fixes {
        let patterns: Vec<(&str, &str)> = c
            .specs
            .iter()
            .map(|s| (s.id.as_str(), s.pattern.as_str()))
            .collect();
        try_fixes(&subnets, &patterns, &c.bundle.links, &analysis)
    } else {
        vec![]
    };
    serde_json::json!({ "analysis": analysis, "fixes": fixes })
}

//! Colour refinement, exact automorphism orbits and an isomorphism-invariant
//! canonical id: a port of sim.pflow.xyz's `classify.go` (1-WL refinement,
//! parameter classes) and `orbits.go` (individualization-refinement, McKay &
//! Piperno 2014), FEATURES.md §D gap 3.
//!
//! The port follows sim string for string, so the same `pflow_metamodel`
//! model gets the same colours, the same canonical form and so the same
//! `canonical_id` sim reports:
//!
//! - seeds: `P|<supply kind>|<initial>|<capacity>` and `T|<rate %g>` (or
//!   `T|delay=<%g>`) quantitatively, `P|<kind>` and `T` structurally, plus
//!   sorted, escaped `refine.*` tags;
//! - arc labels `in|out/<consume|inhibitor|read>/<weight>/<k|nk>`;
//! - each round recolours a node as the first 8 bytes (hex) of
//!   `sha256(colour || "||" || sorted "label:colour" neighbours)`;
//! - the search individualizes the smallest non-singleton cell (ties: the
//!   smaller colour), prunes with recorded automorphisms that fix the path,
//!   and keeps the lexicographically smallest leaf string; the id is its
//!   hex sha256. Work is charged in sim's refinement steps against the same
//!   1,000,000-step budget.
//!
//! What differs: the caller picks the node set (on a coloured net the colour
//! copies nothing can hold are left out) and may overlay rates
//! (`sim.set_rate` session rates); with neither, the seeds are sim's.

use std::collections::{BTreeMap, HashMap};

use pflow_metamodel::{ArcType, Model};
use pflow_solver::stochastic::{classify_supply, rates, SupplyKind};
use sha2::{Digest, Sha256};

use super::go_g;

/// sim's `OrbitSearchWorkBudget`, in refinement steps.
pub const WORK_BUDGET: usize = 1_000_000;

pub const METHOD: &str =
    "automorphism-orbit/exact (individualization-refinement, McKay & Piperno 2014)";

/// A labelled, coloured incidence structure over node indices.
#[derive(Debug, Clone)]
pub struct Incidence {
    /// Node ids, sorted.
    pub nodes: Vec<String>,
    /// Seed colour per node.
    pub seed: Vec<String>,
    /// Labelled incident edges per node: (label, neighbour index).
    pub nb: Vec<Vec<(String, usize)>>,
}

fn kind_name(k: SupplyKind) -> &'static str {
    match k {
        SupplyKind::Conserved => "conserved",
        SupplyKind::Bounded => "bounded",
        SupplyKind::Queue => "queue",
        SupplyKind::State => "state",
    }
}

/// sim's `tagSeedEscaper`.
fn escape_tag(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '%' => out.push_str("%25"),
            '|' => out.push_str("%7C"),
            ';' => out.push_str("%3B"),
            '=' => out.push_str("%3D"),
            ',' => out.push_str("%2C"),
            ':' => out.push_str("%3A"),
            '>' => out.push_str("%3E"),
            '#' => out.push_str("%23"),
            c => out.push(c),
        }
    }
    out
}

/// sim's `refineSeed`: the `refine.*` tags, sorted.
fn refine_seed(tags: Option<&HashMap<String, String>>) -> String {
    let Some(tags) = tags else {
        return String::new();
    };
    let mut parts: Vec<String> = tags
        .iter()
        .filter(|(k, _)| k.starts_with("refine."))
        .map(|(k, v)| format!("{}={}", escape_tag(k), escape_tag(v)))
        .collect();
    if parts.is_empty() {
        return String::new();
    }
    parts.sort();
    format!("|{}", parts.join(";"))
}

fn arc_label(a: &pflow_metamodel::schema::Arc, out: bool, quantitative: bool) -> String {
    let dir = if out { "out" } else { "in" };
    let kind = match a.typ {
        ArcType::Normal => "consume",
        ArcType::Inhibitor => "inhibitor",
        ArcType::Read => "read",
    };
    if !quantitative {
        return format!("{dir}/{kind}");
    }
    let w = if a.weight == 0 { 1 } else { a.weight };
    let k = if a.is_kinetic() { "k" } else { "nk" };
    format!("{dir}/{kind}/{w}/{k}")
}

impl Incidence {
    /// sim's `buildIncidence` over `m`, restricted to the places `keep`
    /// accepts (every transition is kept), with `rate_overlay` replacing
    /// the model's rates where given.
    pub fn build(
        m: &Model,
        quantitative: bool,
        keep: impl Fn(&str) -> bool,
        rate_overlay: &BTreeMap<String, f64>,
    ) -> Self {
        let mut rt = rates(m);
        for (k, v) in rate_overlay {
            if rt.contains_key(k) {
                rt.insert(k.clone(), *v);
            }
        }
        let kinds = classify_supply(m);
        let mut seed: BTreeMap<String, String> = BTreeMap::new();
        for p in m.places.iter().filter(|p| keep(&p.id)) {
            let kind = kinds.get(&p.id).map_or("", |k| kind_name(*k));
            let mut s = if quantitative {
                format!("P|{kind}|{}|{}", p.initial, p.capacity)
            } else {
                format!("P|{kind}")
            };
            s += &refine_seed(p.tags.as_ref());
            seed.insert(p.id.clone(), s);
        }
        for t in &m.transitions {
            let mut s = if !quantitative {
                "T".to_string()
            } else if t.delay > 0.0 {
                format!("T|delay={}", go_g(t.delay))
            } else {
                format!("T|{}", go_g(rt.get(&t.id).copied().unwrap_or(1.0)))
            };
            s += &refine_seed(t.tags.as_ref());
            seed.insert(t.id.clone(), s);
        }
        let nodes: Vec<String> = seed.keys().cloned().collect();
        let idx: HashMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), i))
            .collect();
        let mut nb = vec![vec![]; nodes.len()];
        for a in &m.arcs {
            let (Some(&f), Some(&t)) = (idx.get(a.from.as_str()), idx.get(a.to.as_str())) else {
                continue;
            };
            nb[f].push((arc_label(a, true, quantitative), t));
            nb[t].push((arc_label(a, false, quantitative), f));
        }
        let seed = nodes.iter().map(|n| seed[n].clone()).collect();
        Self { nodes, seed, nb }
    }

    /// One node and every incident edge: the cost of a refinement round.
    fn round_cost(&self) -> usize {
        self.nodes.len() + self.nb.iter().map(Vec::len).sum::<usize>()
    }

    /// sim's `wlRefineSpend`: rounds to a fixpoint from `colour`; `spend`
    /// is charged before each round and stops the refinement on `false`.
    pub fn refine(
        &self,
        mut colour: Vec<String>,
        mut spend: impl FnMut() -> bool,
    ) -> Option<Vec<String>> {
        let mut prev = class_count(&colour);
        for _ in 0..=self.nodes.len() {
            if !spend() {
                return None;
            }
            let next: Vec<String> = (0..self.nodes.len())
                .map(|i| {
                    let mut parts: Vec<String> = self.nb[i]
                        .iter()
                        .map(|(l, j)| format!("{l}:{}", colour[*j]))
                        .collect();
                    parts.sort();
                    let mut h = Sha256::new();
                    h.update(colour[i].as_bytes());
                    h.update(b"||");
                    h.update(parts.join(",").as_bytes());
                    hex(&h.finalize()[..8])
                })
                .collect();
            colour = next;
            let n = class_count(&colour);
            if n == prev {
                break;
            }
            prev = n;
        }
        Some(colour)
    }

    /// The stable colouring from the seeds (sim's `refine`).
    pub fn stable(&self) -> Vec<String> {
        self.refine(self.seed.clone(), || true).expect("unbounded")
    }
}

fn class_count(colour: &[String]) -> usize {
    colour
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len()
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The exact orbits and canonical form of an [`Incidence`].
#[derive(Debug, Clone, PartialEq)]
pub struct Orbits {
    /// Non-trivial orbits (size ≥ 2), each sorted by id, sorted by first
    /// member.
    pub orbits: Vec<Vec<String>>,
    pub canonical_form: String,
    /// hex sha256 of `canonical_form` (sim's `CanonicalIDOf`).
    pub canonical_id: String,
    pub leaves: usize,
    pub work: usize,
    /// Non-identity automorphisms recorded, each counted with its inverse
    /// (as sim's `Generators`). Zero means the net is rigid.
    pub generators: usize,
}

/// The search ran out of its step budget.
#[derive(Debug, Clone, PartialEq)]
pub struct BudgetExceeded {
    pub nodes: usize,
    pub work: usize,
    pub budget: usize,
}

impl std::fmt::Display for BudgetExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the exact individualization-refinement search over {} places and transitions spent its \
             {}-step budget ({} refinement steps) before completing; this net is too large or too \
             symmetric for this implementation's pruning",
            self.nodes, self.budget, self.work
        )
    }
}

struct Search<'a> {
    inc: &'a Incidence,
    budget: usize,
    work: usize,
    round_cost: usize,
    over: bool,
    leaves: usize,
    best: Option<String>,
    first_by_canon: HashMap<String, Vec<usize>>,
    /// Recorded automorphisms as full permutations (node → node).
    generators: Vec<Vec<usize>>,
    parent: Vec<usize>,
}

impl Search<'_> {
    fn spend(&mut self, n: usize) -> bool {
        if self.over {
            return false;
        }
        if self.work + n > self.budget {
            self.over = true;
            return false;
        }
        self.work += n;
        true
    }

    fn refine(&mut self, colour: Vec<String>) -> Option<Vec<String>> {
        let cost = self.round_cost;
        let inc = self.inc;
        inc.refine(colour, || self.spend(cost))
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]];
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra] = rb;
        }
    }

    fn search(&mut self, colour: Vec<String>, path: &mut Vec<usize>) {
        if self.over {
            return;
        }
        let Some(colour) = self.refine(colour) else {
            return;
        };
        let n = self.inc.nodes.len();
        if !self.spend(n) {
            return;
        }
        let mut cells: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, c) in colour.iter().enumerate() {
            cells.entry(c.as_str()).or_default().push(i);
        }
        // Smallest non-singleton cell; ties go to the smaller colour (the
        // BTreeMap's order), as sim.
        let target: Option<Vec<usize>> = cells
            .values()
            .filter(|m| m.len() >= 2)
            .fold(None, |best: Option<&Vec<usize>>, m| match best {
                Some(b) if b.len() <= m.len() => Some(b),
                _ => Some(m),
            })
            .cloned();
        let Some(target) = target else {
            let mut order: Vec<usize> = (0..n).collect();
            order.sort_by(|a, b| colour[*a].cmp(&colour[*b]));
            self.leaf(order);
            return;
        };
        // Members are in node (= id) order already.
        let mut tried: Vec<usize> = vec![];
        for &v in &target {
            if self.over {
                return;
            }
            if self.pruned(path, &tried, v) {
                continue;
            }
            if self.over || !self.spend(colour.len()) {
                return;
            }
            let mut next = colour.clone();
            next[v].push_str("#individualized");
            path.push(v);
            self.search(next, path);
            path.pop();
            tried.push(v);
        }
    }

    fn pruned(&mut self, path: &[usize], tried: &[usize], v: usize) -> bool {
        for g in 0..self.generators.len() {
            if !self.spend(path.len() + tried.len() + 1) {
                return true;
            }
            let p = &self.generators[g];
            if path.iter().any(|&x| p[x] != x) {
                continue;
            }
            if tried.iter().any(|&u| p[u] == v || p[v] == u) {
                return true;
            }
        }
        false
    }

    fn leaf(&mut self, order: Vec<usize>) {
        if !self.spend(2 * self.round_cost) {
            return;
        }
        self.leaves += 1;
        let canon = self.canonical_string(&order);
        if self.best.as_ref().is_none_or(|b| canon < *b) {
            self.best = Some(canon.clone());
        }
        match self.first_by_canon.get(&canon) {
            Some(prior) => {
                let prior = prior.clone();
                self.automorphism(&prior, &order);
            }
            None => {
                self.first_by_canon.insert(canon, order);
            }
        }
    }

    fn canonical_string(&self, order: &[usize]) -> String {
        let inc = self.inc;
        let mut pos = vec![0usize; order.len()];
        for (i, &id) in order.iter().enumerate() {
            pos[id] = i;
        }
        let mut s = String::new();
        for (i, &id) in order.iter().enumerate() {
            if i > 0 {
                s.push(';');
            }
            s.push_str(&inc.seed[id]);
            let mut edges: Vec<String> = inc.nb[id]
                .iter()
                .map(|(l, n)| format!("{l}>{}", pos[*n]))
                .collect();
            edges.sort();
            s.push('|');
            s.push_str(&edges.join(","));
        }
        s
    }

    fn automorphism(&mut self, from: &[usize], to: &[usize]) {
        let n = from.len();
        let mut g: Vec<usize> = (0..n).collect();
        let mut inv: Vec<usize> = (0..n).collect();
        let mut nontrivial = false;
        for i in 0..n {
            let (a, b) = (from[i], to[i]);
            if a != b {
                g[a] = b;
                inv[b] = a;
                nontrivial = true;
                self.union(a, b);
            }
        }
        if nontrivial {
            self.generators.push(g);
            self.generators.push(inv);
        }
    }
}

impl Incidence {
    /// sim's `ExactOrbits` (with `CanonicalIDOf`) under `budget` steps.
    pub fn orbits(&self, budget: usize) -> Result<Orbits, BudgetExceeded> {
        let n = self.nodes.len();
        let empty_id = hex(&Sha256::digest(b""));
        if n == 0 {
            return Ok(Orbits {
                orbits: vec![],
                canonical_form: String::new(),
                canonical_id: empty_id,
                leaves: 0,
                work: 0,
                generators: 0,
            });
        }
        let mut s = Search {
            inc: self,
            budget,
            work: 0,
            round_cost: self.round_cost(),
            over: false,
            leaves: 0,
            best: None,
            first_by_canon: HashMap::new(),
            generators: vec![],
            parent: (0..n).collect(),
        };
        let refused = |s: &Search| BudgetExceeded {
            nodes: n,
            work: s.work,
            budget,
        };
        // The union-find, charged like a copy.
        if !s.spend(n) {
            return Err(refused(&s));
        }
        let Some(start) = s.refine(self.seed.clone()) else {
            return Err(refused(&s));
        };
        s.search(start, &mut vec![]);
        if s.over {
            return Err(refused(&s));
        }
        let mut by_root: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for i in 0..n {
            let r = s.find(i);
            by_root.entry(r).or_default().push(self.nodes[i].clone());
        }
        let mut orbits: Vec<Vec<String>> = by_root
            .into_values()
            .filter(|m| m.len() >= 2)
            .map(|mut m| {
                m.sort();
                m
            })
            .collect();
        orbits.sort_by(|a, b| a[0].cmp(&b[0]));
        let form = s.best.clone().unwrap_or_default();
        Ok(Orbits {
            orbits,
            canonical_id: hex(&Sha256::digest(form.as_bytes())),
            canonical_form: form,
            leaves: s.leaves,
            work: s.work,
            generators: s.generators.len(),
        })
    }
}

// ------------------------------------------------------------ classes

/// sim's `groupBy`: classes of ≥ 2 among `ids` by colour, each sorted,
/// largest first then by first member.
pub fn group_by(ids: &[String], inc: &Incidence, colour: &[String]) -> Vec<Vec<String>> {
    let idx: HashMap<&str, usize> = inc
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.as_str(), i))
        .collect();
    let mut by: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for id in ids {
        if let Some(&i) = idx.get(id.as_str()) {
            by.entry(colour[i].as_str()).or_default().push(id.clone());
        }
    }
    let mut out: Vec<Vec<String>> = by
        .into_values()
        .filter(|m| m.len() >= 2)
        .map(|mut m| {
            m.sort();
            m
        })
        .collect();
    out.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a[0].cmp(&b[0])));
    out
}

/// sim's `nameParts`: split on non-alphanumerics and digit boundaries.
pub fn name_parts(id: &str) -> Vec<String> {
    let mut parts = vec![];
    let mut cur = String::new();
    let mut prev_digit = false;
    for r in id.chars() {
        let alnum = r.is_ascii_alphanumeric();
        let digit = r.is_ascii_digit();
        if !alnum {
            if !cur.is_empty() {
                parts.push(std::mem::take(&mut cur));
            }
            prev_digit = false;
            continue;
        }
        if !cur.is_empty() && digit != prev_digit {
            parts.push(std::mem::take(&mut cur));
        }
        cur.push(r);
        prev_digit = digit;
    }
    if !cur.is_empty() {
        parts.push(cur);
    }
    parts
}

/// sim's `label`: a class name from what its members share (`fork_*`), or
/// the members joined with `+`.
pub fn label(members: &[String]) -> String {
    let mut parts = name_parts(&members[0]);
    for m in &members[1..] {
        let p = name_parts(m);
        if p.len() != parts.len() {
            return members.join("+");
        }
        for (i, part) in parts.iter_mut().enumerate() {
            if p[i] != *part {
                *part = "*".into();
            }
        }
    }
    let joined = parts.join("_");
    if joined.trim_matches(|c| c == '_' || c == '*').is_empty() {
        members.join("+")
    } else {
        joined
    }
}

/// One discovered class (sim's `Class`, without the sampled experiment).
#[derive(Debug, Clone, PartialEq)]
pub struct Class {
    pub id: String,
    pub members: Vec<String>,
    /// "FungibleSet" or "Role".
    pub kind: &'static str,
    pub method: &'static str,
    /// Settled by the exact orbit search (one orbit ⇒ interchangeable).
    pub verified: bool,
    /// When the exact search split a colour class: the orbits it falls
    /// into (fixed points as singletons).
    pub split: Vec<Vec<String>>,
    pub evidence: String,
}

/// sim's `ClassesVerified` over `scope` (sim classifies its "pools"), with
/// the exact orbits standing in for the permutation experiment: pass one
/// groups by quantitative colour (fungible candidates; with `orbits`, one
/// orbit verifies the class and a split demotes it to a role), pass two by
/// structural colour among what pass one left (roles).
pub fn classes(
    scope: &[String],
    quantitative: &Incidence,
    structural: &Incidence,
    orbits: Option<&Orbits>,
) -> Vec<Class> {
    let mut out = vec![];
    let mut claimed: std::collections::HashSet<String> = Default::default();
    let q = quantitative.stable();
    for members in group_by(scope, quantitative, &q) {
        let mut c = Class {
            id: label(&members),
            members: members.clone(),
            kind: "FungibleSet",
            method: "colour-refinement/quantitative",
            verified: false,
            split: vec![],
            evidence: "the net does not distinguish these, given levels, rates and arc kinds \
                       (1-WL over-approximates isomorphism, so a shared colour is a candidate)"
                .into(),
        };
        if let Some(o) = orbits {
            let groups = orbit_groups(o, &members);
            if groups.len() == 1 {
                c.verified = true;
                c.method = "automorphism-orbit/exact";
                c.evidence = format!(
                    "proved by exact automorphism search, not sampled: {} are interchangeable",
                    members.join(", ")
                );
            } else {
                c.kind = "Role";
                c.evidence = format!(
                    "colour refinement placed {} in one class, but the exact automorphism group splits it into {}",
                    members.join(", "),
                    groups
                        .iter()
                        .map(|g| format!("{{{}}}", g.join(",")))
                        .collect::<Vec<_>>()
                        .join(" vs ")
                );
                c.split = groups;
            }
        }
        claimed.extend(members);
        out.push(c);
    }
    let st = structural.stable();
    for members in group_by(scope, structural, &st) {
        let fresh: Vec<String> = members
            .into_iter()
            .filter(|m| !claimed.contains(m))
            .collect();
        if fresh.len() < 2 {
            continue;
        }
        out.push(Class {
            id: label(&fresh),
            members: fresh,
            kind: "Role",
            method: "colour-refinement/structural",
            verified: false,
            split: vec![],
            evidence: "these hold the same position in the net, ignoring levels and rates; \
                       they differ in level or rate, so swapping them changes the answer"
                .into(),
        });
    }
    out
}

/// sim's `SameOrbit` groups: `members` partitioned by orbit, fixed points
/// as singletons, each sorted, sorted by first member.
pub fn orbit_groups(o: &Orbits, members: &[String]) -> Vec<Vec<String>> {
    let mut by: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for m in members {
        let key = o
            .orbits
            .iter()
            .find(|orb| orb.contains(m))
            .map_or_else(|| m.clone(), |orb| orb[0].clone());
        by.entry(key).or_default().push(m.clone());
    }
    let mut groups: Vec<Vec<String>> = by
        .into_values()
        .map(|mut g| {
            g.sort();
            g
        })
        .collect();
    groups.sort_by(|a, b| a[0].cmp(&b[0]));
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_like_sim() {
        let m = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(name_parts("fork_0"), vec!["fork", "0"]);
        assert_eq!(name_parts("item0"), vec!["item", "0"]);
        assert_eq!(label(&m(&["fork_0", "fork_1", "fork_2"])), "fork_*");
        assert_eq!(label(&m(&["a", "b"])), "a+b");
        assert_eq!(label(&m(&["p00", "p02"])), "p_*");
    }

    #[test]
    fn escapes_tags() {
        let mut t = HashMap::new();
        t.insert("refine.a".to_string(), "x;refine.b=y".to_string());
        t.insert("owner".to_string(), "me".to_string());
        assert_eq!(refine_seed(Some(&t)), "|refine.a=x%3Brefine.b%3Dy");
    }
}

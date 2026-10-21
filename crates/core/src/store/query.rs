//! Conjunctive queries over the store's edges: sim.pflow.xyz's `sim_query`
//! language (`pkg/modelstore/query.go`, `query_eval.go`) over the local
//! relations.
//!
//! A query is `{"select": [vars], "where": [patterns]}` (or just the
//! pattern list). A pattern is `{"subject", "predicate", "object", "mode",
//! "maxHops"}`; a term starting with `?` is a variable. Modes:
//!
//! - `edge` (default): one stored edge, read through the registry
//!   (a symmetric predicate matches either way round; an inverse-only name
//!   such as `narrower` is rewritten as its stored predicate, ends swapped).
//! - `path`: 1 to `maxHops` (≤ 8) edges of the predicate, breadth first.
//! - `closure`: any number of edges (≤ 8 hops) of a predicate the registry
//!   declares transitive.
//!
//! Patterns join on shared variables, planned most-bound first. A query
//! with no variables is a yes/no question. Limits and refusal texts follow
//! sim's (4 patterns, 8 hops, 200 rows, 10 000 intermediate rows, a work
//! budget), and every limit is a refusal or a reported truncation, never a
//! silent clamp.
//!
//! Not here: sim's stored Query/Result entities and cache, graph digests,
//! and witness re-checks on structural edges (the local store records
//! structural edges only through [`super::Store::put_edge_as`]; they are
//! walked as stored, and the answer says so).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{json, Map, Value};

use super::{canon, predicate, Relation, Snapshot};
use crate::{Error, Result};

pub const MAX_PATTERNS: usize = 4;
pub const MAX_TERM_BYTES: usize = 256;
pub const MAX_HOPS: usize = 8;
pub const MAX_ROWS: usize = 200;
pub const MAX_INTERMEDIATE: usize = 10_000;
pub const MAX_WORK: usize = 1_000_000;
pub const MAX_QUERY_BYTES: usize = 16_384;

const SHAPE: &str = r#"a query is {"select": [variables], "where": [{"subject": s, "predicate": p, "object": o, "mode": "edge"|"path"|"closure", "maxHops": n}]}, with ?name for a variable"#;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pattern {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub mode: String,
    pub max_hops: usize,
}

impl Pattern {
    fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("subject".into(), json!(self.subject));
        m.insert("predicate".into(), json!(self.predicate));
        m.insert("object".into(), json!(self.object));
        m.insert("mode".into(), json!(self.mode));
        if self.mode == "path" {
            m.insert("maxHops".into(), json!(self.max_hops));
        }
        Value::Object(m)
    }
}

fn refuse(msg: impl Into<String>) -> Error {
    Error::BadArgs(msg.into())
}

pub fn is_variable(t: &str) -> bool {
    let Some(rest) = t.strip_prefix('?') else {
        return false;
    };
    let mut cs = rest.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && rest.len() <= 32
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The strict parse: a JSON string is decoded first; an array is the
/// `where` list; unknown fields are refused.
pub fn parse(v: &Value) -> Result<(Option<Vec<String>>, Vec<Pattern>)> {
    let owned;
    let v = match v {
        Value::String(t) => {
            if t.len() > MAX_QUERY_BYTES {
                return Err(refuse(format!(
                    "the query is {} bytes, over the {MAX_QUERY_BYTES}-byte limit; split it into smaller queries joined on a shared id",
                    t.len()
                )));
            }
            owned = serde_json::from_str::<Value>(t)
                .map_err(|e| refuse(format!("query JSON: {e}; {SHAPE}")))?;
            &owned
        }
        v => v,
    };
    let (select, wher) = match v {
        Value::Array(_) => (None, v),
        Value::Object(m) => {
            if let Some(k) = m.keys().find(|k| *k != "select" && *k != "where") {
                return Err(refuse(format!("query JSON: unknown field {k:?}; {SHAPE}")));
            }
            let select = match m.get("select") {
                None | Some(Value::Null) => None,
                Some(Value::Array(a)) => Some(
                    a.iter()
                        .map(|x| {
                            x.as_str().map(str::to_string).ok_or_else(|| {
                                refuse(format!("select must be a list of strings; {SHAPE}"))
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                ),
                Some(_) => {
                    return Err(refuse(format!("select must be a list of strings; {SHAPE}")))
                }
            };
            (select, m.get("where").unwrap_or(&Value::Null))
        }
        _ => {
            return Err(refuse(format!(
                "query JSON must be an object or a list of patterns; {SHAPE}"
            )))
        }
    };
    let Value::Array(ps) = wher else {
        return Err(refuse(format!(
            r#"a query needs at least one pattern in "where"; {SHAPE}"#
        )));
    };
    let mut out = vec![];
    for (i, p) in ps.iter().enumerate() {
        let Value::Object(m) = p else {
            return Err(refuse(format!("where[{i}] must be an object; {SHAPE}")));
        };
        if let Some(k) = m
            .keys()
            .find(|k| !["subject", "predicate", "object", "mode", "maxHops"].contains(&k.as_str()))
        {
            return Err(refuse(format!("where[{i}]: unknown field {k:?}; {SHAPE}")));
        }
        let field = |k: &str| -> Result<String> {
            match m.get(k) {
                None | Some(Value::Null) => Ok(String::new()),
                Some(Value::String(s)) => Ok(s.clone()),
                Some(_) => Err(refuse(format!("where[{i}].{k} must be a string"))),
            }
        };
        let max_hops = match m.get("maxHops") {
            None | Some(Value::Null) => 0,
            Some(n) => match n.as_u64() {
                Some(h) if h as usize <= MAX_HOPS && h > 0 => h as usize,
                _ => {
                    return Err(refuse(format!(
                        "pattern {}: maxHops {n} is outside 1..{MAX_HOPS}; give a whole number of hops from 1 to {MAX_HOPS}",
                        i + 1
                    )))
                }
            },
        };
        out.push(Pattern {
            subject: field("subject")?,
            predicate: field("predicate")?,
            object: field("object")?,
            mode: field("mode")?,
            max_hops,
        });
    }
    Ok((select, out))
}

fn validate_pattern(n: usize, p: &Pattern) -> Result<()> {
    let fields = [
        ("subject", &p.subject),
        ("predicate", &p.predicate),
        ("object", &p.object),
    ];
    for (name, t) in fields {
        if t.is_empty() {
            return Err(refuse(format!(
                r#"pattern {n}: "{name}" is empty; give an entity id, a literal, or a variable such as ?m"#
            )));
        }
        if t.len() > MAX_TERM_BYTES {
            return Err(refuse(format!(
                r#"pattern {n}: "{name}" is {} bytes; a term is at most {MAX_TERM_BYTES}"#,
                t.len()
            )));
        }
        if t.starts_with('?') && !is_variable(t) {
            return Err(refuse(format!(
                r#"pattern {n}: "{t}" is not a variable name; write ? then a letter or underscore, then up to 31 letters, digits or underscores"#
            )));
        }
    }
    if is_variable(&p.predicate) {
        return Err(refuse(format!(
            r#"pattern {n}: the predicate is a variable ("{}"); every pattern names its predicate (store.info lists them)"#,
            p.predicate
        )));
    }
    let mode = if p.mode.is_empty() {
        "edge"
    } else {
        p.mode.as_str()
    };
    if !["edge", "path", "closure"].contains(&mode) {
        return Err(refuse(format!(
            r#"pattern {n}: mode "{}" is not one of edge, path or closure"#,
            p.mode
        )));
    }
    if p.max_hops != 0 && mode != "path" {
        return Err(refuse(format!(
            "pattern {n}: maxHops is set but mode is {mode}; maxHops applies to mode path only (closure walks until nothing new is reached, up to {MAX_HOPS} hops)"
        )));
    }
    if mode == "path" && p.max_hops == 0 {
        return Err(refuse(format!(
            "pattern {n}: mode path needs maxHops, a whole number from 1 to {MAX_HOPS}"
        )));
    }
    match predicate::lookup(&p.predicate) {
        None => {
            if mode != "edge" {
                return Err(refuse(format!(
                    r#"pattern {n}: predicate "{}" is not registered, so nothing declares it transitive and it is never walked or closed over; registered predicates are {}"#,
                    p.predicate,
                    predicate::registered_names()
                )));
            }
        }
        Some(pred) => {
            let structural = pred.axis == predicate::Axis::Structural;
            if pred.name == predicate::TYPE && mode != "edge" {
                return Err(refuse(format!(
                    "pattern {n}: @type is always one hop: its object is a ModelKind literal, which has no edges of its own"
                )));
            }
            if mode == "path" && p.max_hops > 1 && structural && !pred.transitive {
                return Err(refuse(format!(
                    r#"pattern {n}: predicate "{}" is structural and not transitive, so a path of two or more of its edges proves nothing about its ends; use maxHops 1, or mode edge"#,
                    pred.name
                )));
            }
            if mode == "closure" && !pred.transitive {
                return Err(refuse(if structural {
                    format!(
                        r#"pattern {n}: predicate "{}" is structural and not transitive, so it has no closure; use maxHops 1, or mode edge"#,
                        pred.name
                    )
                } else {
                    format!(
                        r#"pattern {n}: predicate "{}" is not transitive, so its closure would claim what the registry does not declare; use mode path with maxHops to walk it"#,
                        pred.name
                    )
                }));
            }
            let literal_object = matches!(pred.range, predicate::Range::Literal(_));
            for (name, t) in [("subject", &p.subject), ("object", &p.object)] {
                if is_variable(t) || (name == "object" && literal_object) {
                    continue;
                }
                if !canon::is_model_cid(t) && !canon::is_sha24(t) {
                    return Err(refuse(format!(
                        r#"pattern {n}: {name} "{t}" is neither a variable (?name) nor a content id (a model CID, or 24 hex for a collection, map or relation); store.list lists model CIDs"#
                    )));
                }
            }
            if literal_object
                && !is_variable(&p.object)
                && !predicate::MODEL_KINDS.contains(&p.object.as_str())
            {
                return Err(refuse(format!(
                    r#"pattern {n}: object "{}" is not a model kind; @type's object is one of {}"#,
                    p.object,
                    predicate::MODEL_KINDS.join(", ")
                )));
            }
        }
    }
    if p.subject == p.object {
        return Err(refuse(format!(
            r#"pattern {n}: subject and object are both "{}"; a pattern from a term to itself is not answered"#,
            p.subject
        )));
    }
    Ok(())
}

fn variables(ps: &[Pattern]) -> Vec<String> {
    let set: BTreeSet<&String> = ps
        .iter()
        .flat_map(|p| [&p.subject, &p.object])
        .filter(|t| is_variable(t))
        .collect();
    set.into_iter().cloned().collect()
}

/// Groups patterns (1-based) by shared variables.
fn components(ps: &[Pattern]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..ps.len()).collect();
    fn find(parent: &[usize], mut i: usize) -> usize {
        while parent[i] != i {
            i = parent[i];
        }
        i
    }
    let mut owner: HashMap<&str, usize> = HashMap::new();
    for (i, p) in ps.iter().enumerate() {
        for t in [&p.subject, &p.object] {
            if !is_variable(t) {
                continue;
            }
            match owner.get(t.as_str()) {
                Some(&j) => {
                    let (a, b) = (find(&parent, i), find(&parent, j));
                    if a != b {
                        parent[a.max(b)] = a.min(b);
                    }
                }
                None => {
                    owner.insert(t, i);
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..ps.len() {
        groups.entry(find(&parent, i)).or_default().push(i + 1);
    }
    groups.into_values().collect()
}

fn validate(select: &Option<Vec<String>>, ps: &[Pattern]) -> Result<()> {
    if ps.is_empty() {
        return Err(refuse(format!(
            r#"a query needs at least one pattern in "where"; {SHAPE}"#
        )));
    }
    if ps.len() > MAX_PATTERNS {
        return Err(refuse(format!(
            "the query has {} patterns; at most {MAX_PATTERNS} are answered: split it into smaller queries joined on a shared id",
            ps.len()
        )));
    }
    for (i, p) in ps.iter().enumerate() {
        validate_pattern(i + 1, p)?;
    }
    let vars: BTreeSet<String> = variables(ps).into_iter().collect();
    if let Some(sel) = select {
        if sel.is_empty() {
            return Err(refuse("select is empty; omit it to return every variable"));
        }
        if vars.is_empty() {
            return Err(refuse(
                "select is set but the query has no variables; a query with no variables is a yes/no question, so drop select",
            ));
        }
        let mut seen = BTreeSet::new();
        for v in sel {
            if !is_variable(v) {
                return Err(refuse(format!(r#"select: "{v}" is not a variable name"#)));
            }
            if !seen.insert(v) {
                return Err(refuse(format!(r#"select names "{v}" twice"#)));
            }
            if !vars.contains(v) {
                return Err(refuse(format!(
                    r#"select names "{v}", which no pattern uses; select only variables the patterns bind, or omit select for all of them"#
                )));
            }
        }
    }
    if ps.len() > 1 {
        for (i, p) in ps.iter().enumerate() {
            if !is_variable(&p.subject) && !is_variable(&p.object) {
                return Err(refuse(format!(
                    "pattern {} has no variable, so it is a separate yes/no question; ask it as its own query",
                    i + 1
                )));
            }
        }
        let comps = components(ps);
        if comps.len() > 1 {
            let j = |c: &[usize]| {
                c.iter()
                    .map(|x| x.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            return Err(refuse(format!(
                "patterns {{{}}} and {{{}}} share no variable, so the query asks for every combination of separate answers; join them on a shared variable or run them as separate queries",
                j(&comps[0]),
                j(&comps[1])
            )));
        }
    }
    Ok(())
}

/// A normalized query: modes defaulted, inverse names rewritten, symmetric
/// ends ordered, patterns sorted and de-duplicated, select defaulted.
pub struct Query {
    pub select: Vec<String>,
    pub patterns: Vec<Pattern>,
    pub origin: Vec<usize>,
    pub rewrites: Vec<String>,
}

pub fn normalize(select: Option<Vec<String>>, ps: Vec<Pattern>) -> Result<Query> {
    validate(&select, &ps)?;
    let mut rewrites = vec![];
    let mut keyed = vec![];
    for (i, mut p) in ps.into_iter().enumerate() {
        if p.mode.is_empty() {
            p.mode = "edge".into();
        }
        if p.mode != "path" {
            p.max_hops = 0;
        }
        if let Some(pred) = predicate::lookup(&p.predicate) {
            if let Some(stored) = pred.stored_as {
                rewrites.push(format!(
                    "pattern {}: {} read as {stored} with subject and object swapped",
                    i + 1,
                    pred.name
                ));
                p.predicate = stored.into();
                std::mem::swap(&mut p.subject, &mut p.object);
            } else if pred.symmetric && p.subject > p.object {
                std::mem::swap(&mut p.subject, &mut p.object);
            }
        }
        keyed.push((canon::go_json(&p.to_json()), p, i));
    }
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.dedup_by(|b, a| a.0 == b.0);
    let patterns: Vec<Pattern> = keyed.iter().map(|k| k.1.clone()).collect();
    let origin = keyed.iter().map(|k| k.2).collect();
    let select = select.unwrap_or_else(|| variables(&patterns));
    Ok(Query {
        select,
        patterns,
        origin,
        rewrites,
    })
}

/// Most-bound pattern first; ties by mode (edge, path, closure), then
/// index. A walk with neither end bound is refused.
fn plan(q: &Query) -> Result<Vec<usize>> {
    let mut bound: BTreeSet<&str> = BTreeSet::new();
    let rank = |m: &str| match m {
        "edge" => 0,
        "path" => 1,
        _ => 2,
    };
    let mut used = vec![false; q.patterns.len()];
    let mut order = vec![];
    while order.len() < q.patterns.len() {
        let mut best: Option<(usize, usize)> = None;
        for (i, p) in q.patterns.iter().enumerate() {
            if used[i] {
                continue;
            }
            let score = [&p.subject, &p.object]
                .iter()
                .filter(|t| !is_variable(t) || bound.contains(t.as_str()))
                .count();
            let better = match best {
                None => true,
                Some((b, bs)) => {
                    score > bs || (score == bs && rank(&p.mode) < rank(&q.patterns[b].mode))
                }
            };
            if better {
                best = Some((i, score));
            }
        }
        let (bi, score) = best.expect("a pattern remains");
        let p = &q.patterns[bi];
        if p.mode != "edge" && score == 0 {
            return Err(refuse(format!(
                "pattern {}: mode {} needs one end bound (a content id, or a variable an earlier pattern binds); {} and {} are both unbound there, which would ask for every pair it relates",
                q.origin[bi] + 1,
                p.mode,
                p.subject,
                p.object
            )));
        }
        used[bi] = true;
        order.push(bi);
        for t in [&p.subject, &p.object] {
            if is_variable(t) {
                bound.insert(t);
            }
        }
    }
    Ok(order)
}

#[derive(Clone)]
struct Binding {
    vals: BTreeMap<String, String>,
    support: Vec<Value>,
}

struct Match {
    vals: BTreeMap<String, String>,
    support: Value,
}

/// A bound reached part way: the run reports a truncation.
struct Stop;

/// Relation ids of one predicate, by subject and by object, sorted.
#[derive(Default)]
struct PredIndex<'a> {
    out: HashMap<&'a str, Vec<&'a String>>,
    inn: HashMap<&'a str, Vec<&'a String>>,
    all: Vec<&'a String>,
}

struct Run<'a> {
    rel: &'a BTreeMap<String, Relation>,
    idx: HashMap<&'a str, PredIndex<'a>>,
    work: usize,
    budget: usize,
    ask: bool,
    trunc: Option<Value>,
    hop_limit: BTreeSet<usize>,
}

impl<'a> Run<'a> {
    fn new(rel: &'a BTreeMap<String, Relation>, ask: bool) -> Self {
        let mut idx: HashMap<&str, PredIndex> = HashMap::new();
        for (id, r) in rel {
            let e = idx.entry(r.predicate.as_str()).or_default();
            e.out.entry(r.subject.as_str()).or_default().push(id);
            e.inn.entry(r.object.as_str()).or_default().push(id);
            e.all.push(id);
        }
        Self {
            rel,
            idx,
            work: 0,
            budget: MAX_WORK,
            ask,
            trunc: None,
            hop_limit: BTreeSet::new(),
        }
    }

    fn charge(&mut self, n: usize, pattern: usize) -> std::result::Result<(), Stop> {
        self.work += n;
        if self.work > self.budget {
            self.stop(json!({
                "bound": "work", "limit": self.budget, "reached": self.work, "pattern": pattern,
                "message": format!("truncated: the work budget of {} units ran out at pattern {pattern}; bind a constant or lower maxHops", self.budget),
            }));
            return Err(Stop);
        }
        Ok(())
    }

    fn stop(&mut self, t: Value) {
        if self.trunc.is_none() {
            self.trunc = Some(t);
        }
    }

    fn list(&self, pred: &str, end: Option<(&str, bool)>) -> Vec<&'a String> {
        let Some(ix) = self.idx.get(pred) else {
            return vec![];
        };
        match end {
            None => ix.all.clone(),
            Some((id, true)) => ix.out.get(id).cloned().unwrap_or_default(),
            Some((id, false)) => ix.inn.get(id).cloned().unwrap_or_default(),
        }
    }

    fn match_edge(
        &mut self,
        pi: usize,
        p: &Pattern,
        row: &Binding,
    ) -> std::result::Result<Vec<Match>, Stop> {
        let (sv, ov) = (resolve(&p.subject, row), resolve(&p.object, row));
        let sym = predicate::is_symmetric(&p.predicate);
        // (relation id, subject, object) as the pattern reads the edge.
        let mut cands: Vec<(&String, String, String)> = vec![];
        let mut seen = BTreeSet::new();
        let mut add = |c: (&'a String, &str, &str)| {
            if seen.insert((c.0.clone(), c.1.to_string(), c.2.to_string())) {
                cands.push((c.0, c.1.to_string(), c.2.to_string()));
            }
        };
        let mut examined = 0;
        match (&sv, &ov) {
            (Some(s), _) => {
                for id in self.list(&p.predicate, Some((s, true))) {
                    examined += 1;
                    let r = &self.rel[id];
                    if ov.as_ref().is_none_or(|o| &r.object == o) {
                        add((id, &r.subject, &r.object));
                    }
                }
                if sym {
                    for id in self.list(&p.predicate, Some((s, false))) {
                        examined += 1;
                        let r = &self.rel[id];
                        if ov.as_ref().is_none_or(|o| &r.subject == o) {
                            add((id, &r.object, &r.subject));
                        }
                    }
                }
            }
            (None, Some(o)) => {
                for id in self.list(&p.predicate, Some((o, false))) {
                    examined += 1;
                    let r = &self.rel[id];
                    add((id, &r.subject, &r.object));
                }
                if sym {
                    for id in self.list(&p.predicate, Some((o, true))) {
                        examined += 1;
                        let r = &self.rel[id];
                        add((id, &r.object, &r.subject));
                    }
                }
            }
            (None, None) => {
                for id in self.list(&p.predicate, None) {
                    examined += 1;
                    let r = &self.rel[id];
                    add((id, &r.subject, &r.object));
                    if sym {
                        add((id, &r.object, &r.subject));
                    }
                }
            }
        }
        self.charge(examined, pi)?;
        cands.sort_by(|a, b| a.0.cmp(b.0));
        Ok(cands
            .into_iter()
            .map(|(id, s, o)| {
                let mut vals = BTreeMap::new();
                if sv.is_none() {
                    vals.insert(p.subject.clone(), s);
                }
                if ov.is_none() {
                    vals.insert(p.object.clone(), o);
                }
                Match {
                    vals,
                    support: json!({"pattern": pi, "reading": "edge", "edges": [id]}),
                }
            })
            .collect())
    }

    /// One node's steps in relation-id order: along stored edges out of
    /// `cur` (into it, walking backward), plus the other side for a
    /// symmetric predicate.
    fn adjacency(&self, pred: &str, cur: &str, backward: bool) -> Vec<(&'a String, String)> {
        let mut steps: Vec<(&String, String)> = self
            .list(pred, Some((cur, !backward)))
            .into_iter()
            .map(|id| {
                let r = &self.rel[id];
                (
                    id,
                    if backward {
                        r.subject.clone()
                    } else {
                        r.object.clone()
                    },
                )
            })
            .collect();
        if predicate::is_symmetric(pred) {
            for id in self.list(pred, Some((cur, backward))) {
                let r = &self.rel[id];
                steps.push((
                    id,
                    if backward {
                        r.object.clone()
                    } else {
                        r.subject.clone()
                    },
                ));
            }
            steps.sort_by(|a, b| a.0.cmp(b.0));
        }
        steps
    }

    /// A breadth-first walk from the bound end; each reached node keeps the
    /// first (shortest) chain to it.
    fn match_walk(
        &mut self,
        pi: usize,
        p: &Pattern,
        row: &Binding,
    ) -> std::result::Result<Vec<Match>, Stop> {
        let (sv, ov) = (resolve(&p.subject, row), resolve(&p.object, row));
        let (start, backward) = match (&sv, &ov) {
            (Some(s), _) => (s.clone(), false),
            (None, Some(o)) => (o.clone(), true),
            (None, None) => unreachable!("plan binds one end of a walk"),
        };
        let target = if self.ask {
            ov.clone().filter(|_| sv.is_some())
        } else {
            None
        };
        let limit = if p.mode == "path" {
            p.max_hops
        } else {
            MAX_HOPS
        };
        let mut parent: HashMap<String, (String, String)> = HashMap::new(); // node -> (prev, via)
        let mut order: Vec<String> = vec![];
        let mut frontier = vec![start.clone()];
        let mut truncated = false;
        'walk: for depth in 0.. {
            if frontier.is_empty() {
                break;
            }
            if depth == limit {
                for cur in &frontier {
                    let steps = self.adjacency(&p.predicate, cur, backward);
                    self.charge(1 + steps.len(), pi)?;
                    if steps
                        .iter()
                        .any(|(_, nb)| *nb != start && !parent.contains_key(nb))
                    {
                        truncated = true;
                        break;
                    }
                }
                break;
            }
            let mut next = vec![];
            for cur in &frontier {
                let steps = self.adjacency(&p.predicate, cur, backward);
                self.charge(1 + steps.len(), pi)?;
                for (rid, nb) in steps {
                    if nb == start || parent.contains_key(&nb) {
                        continue;
                    }
                    parent.insert(nb.clone(), (cur.clone(), rid.clone()));
                    order.push(nb.clone());
                    if target.as_deref() == Some(nb.as_str()) {
                        break 'walk;
                    }
                    next.push(nb);
                }
            }
            frontier = next;
        }
        let both = sv.is_some() && ov.is_some();
        if truncated && (!both || !parent.contains_key(ov.as_deref().unwrap_or(""))) {
            if p.mode == "path" {
                self.hop_limit.insert(pi);
            } else {
                self.stop(json!({
                    "bound": "hops", "limit": MAX_HOPS, "reached": MAX_HOPS, "pattern": pi,
                    "message": format!("truncated: closure over {} from {start} was still reaching new entities at hop {MAX_HOPS}; partialRows covers {MAX_HOPS} hops and others may exist beyond. Start the walk nearer its target", p.predicate),
                }));
            }
        }
        let reading = if predicate::is_transitive(&p.predicate) {
            "closure"
        } else {
            "path"
        };
        let mut out = vec![];
        for y in order {
            if both && Some(&y) != ov.as_ref() {
                continue;
            }
            let mut edges = vec![];
            let mut cur = y.clone();
            while cur != start {
                let (prev, via) = &parent[&cur];
                edges.push(via.clone());
                cur = prev.clone();
            }
            if !backward {
                edges.reverse();
            }
            let mut vals = BTreeMap::new();
            if sv.is_none() {
                vals.insert(p.subject.clone(), y.clone());
            }
            if ov.is_none() {
                vals.insert(p.object.clone(), y.clone());
            }
            out.push(Match {
                vals,
                support: json!({"pattern": pi, "reading": reading, "edges": edges}),
            });
        }
        Ok(out)
    }

    fn extend(&mut self, pi: usize, p: &Pattern, table: Vec<Binding>) -> (Vec<Binding>, bool) {
        let mut next = vec![];
        for row in &table {
            let ms = if p.mode == "edge" {
                self.match_edge(pi, p, row)
            } else {
                self.match_walk(pi, p, row)
            };
            let Ok(ms) = ms else {
                return (next, true);
            };
            for m in ms {
                let mut vals = row.vals.clone();
                vals.extend(m.vals);
                let mut support = row.support.clone();
                support.push(m.support);
                next.push(Binding { vals, support });
                if next.len() > MAX_INTERMEDIATE {
                    self.stop(json!({
                        "bound": "intermediate", "limit": MAX_INTERMEDIATE, "reached": next.len(), "pattern": pi,
                        "message": format!("truncated: more than {MAX_INTERMEDIATE} intermediate rows at pattern {pi}; partialRows are true but not all of them. Bind a constant or reorder the question so a narrower pattern comes first"),
                    }));
                    next.truncate(MAX_INTERMEDIATE);
                    return (next, true);
                }
                if self.ask {
                    return (next, false);
                }
            }
            if self.trunc.is_some() && self.trunc.as_ref().is_some_and(|t| t["bound"] == "work") {
                return (next, true);
            }
        }
        (next, false)
    }
}

fn resolve(t: &str, row: &Binding) -> Option<String> {
    if is_variable(t) {
        row.vals.get(t).cloned()
    } else {
        Some(t.to_string())
    }
}

/// The outcome of [`run`]: the answer JSON plus the rows, for artifacts.
pub struct Answer {
    pub json: Value,
    pub select: Vec<String>,
    /// Projected values per row, in `select` order.
    pub rows: Vec<Vec<String>>,
    /// Every relation id the rows rest on.
    pub used: Vec<String>,
}

/// Parses, validates, normalizes and answers `query` over `snap`.
pub fn run(snap: &Snapshot, query: &Value) -> Result<Answer> {
    let (select, ps) = parse(query)?;
    let q = normalize(select, ps)?;
    let order = plan(&q)?;
    let ask = q.select.is_empty();
    let mut r = Run::new(&snap.relations, ask);

    // Constants that touch no edge of their pattern's predicate.
    let mut unmatched = vec![];
    for (pi, p) in q.patterns.iter().enumerate() {
        let sym = predicate::is_symmetric(&p.predicate);
        let out = |t: &str| !r.list(&p.predicate, Some((t, true))).is_empty();
        let inn = |t: &str| !r.list(&p.predicate, Some((t, false))).is_empty();
        if !is_variable(&p.subject) && !out(&p.subject) && !(sym && inn(&p.subject)) {
            unmatched.push(json!({"pattern": pi, "term": p.subject}));
        }
        if !is_variable(&p.object) && !inn(&p.object) && !(sym && out(&p.object)) {
            unmatched.push(json!({"pattern": pi, "term": p.object}));
        }
    }

    let mut table = vec![Binding {
        vals: BTreeMap::new(),
        support: vec![],
    }];
    let mut stopped_early = false;
    for (step, &pi) in order.iter().enumerate() {
        let p = q.patterns[pi].clone();
        let (next, stopped) = r.extend(pi, &p, table);
        table = next;
        if stopped {
            if step != order.len() - 1 {
                table.clear(); // no row is fully joined
            }
            stopped_early = true;
            break;
        }
        if ask && !table.is_empty() {
            break;
        }
    }
    let _ = stopped_early;

    // Project, de-duplicate on the projected tuple, sort.
    let mut seen = BTreeSet::new();
    let mut rows: Vec<(Vec<String>, Value, Vec<String>)> = vec![];
    for b in &table {
        let vals: Vec<String> = q
            .select
            .iter()
            .map(|v| b.vals.get(v).cloned().unwrap_or_default())
            .collect();
        if !seen.insert(vals.clone()) {
            continue;
        }
        let mut sup = b.support.clone();
        sup.sort_by_key(|s| s["pattern"].as_u64());
        let edges: Vec<String> = sup
            .iter()
            .flat_map(|s| s["edges"].as_array().cloned().unwrap_or_default())
            .filter_map(|e| e.as_str().map(str::to_string))
            .collect();
        let bindings: Map<String, Value> = q
            .select
            .iter()
            .zip(&vals)
            .map(|(k, v)| (k.clone(), json!(v)))
            .collect();
        rows.push((vals, json!({"bindings": bindings, "support": sup}), edges));
        if ask {
            break;
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    if !ask && rows.len() > MAX_ROWS {
        r.stop(json!({
            "bound": "rows", "limit": MAX_ROWS, "reached": rows.len(), "pattern": -1,
            "message": format!("truncated: more than {MAX_ROWS} rows match; partialRows holds the first {MAX_ROWS} in sorted order and others exist. Bind a constant or add a pattern that narrows the match"),
        }));
        rows.truncate(MAX_ROWS);
    }

    let mut out = json!({
        "query": {
            "select": q.select,
            "where": q.patterns.iter().map(Pattern::to_json).collect::<Vec<_>>(),
        },
        "status": "complete",
        "work": r.work,
        "budget": r.budget,
    });
    if !q.rewrites.is_empty() {
        out["rewrites"] = json!(q.rewrites);
    }
    if ask {
        out["answer"] = json!(if !rows.is_empty() {
            r.trunc = None;
            "yes"
        } else if r.trunc.is_some() {
            "unknown"
        } else {
            "no"
        });
    } else {
        out["select"] = json!(q.select);
    }
    let row_json: Vec<Value> = rows.iter().map(|r| r.1.clone()).collect();
    match r.trunc.take() {
        Some(t) => {
            out["status"] = json!("truncated");
            out["truncated"] = t;
            out["partialRows"] = json!(row_json);
        }
        None => out["rows"] = json!(row_json),
    }
    if !r.hop_limit.is_empty() {
        out["hopLimitReached"] = json!(r.hop_limit);
    }
    if !unmatched.is_empty() {
        out["unmatched"] = json!(unmatched);
    }
    let used: BTreeSet<String> = rows.iter().flat_map(|r| r.2.iter().cloned()).collect();
    out["used"] = json!({"edges": used});
    let mut caveats = vec![];
    if q.patterns.iter().any(|p| {
        predicate::lookup(&p.predicate).is_some_and(|p| p.axis == predicate::Axis::Structural)
    }) {
        caveats.push(
            "structural edges are walked as stored: the local store keeps no witnesses, so they are not re-checked as sim_query re-checks them".to_string(),
        );
    }
    if ask && rows.is_empty() && !r.hop_limit.is_empty() {
        caveats.push(
            "no within the hop limit only: the walk reached maxHops with edges beyond".into(),
        );
    }
    if !caveats.is_empty() {
        out["caveats"] = json!(caveats);
    }
    Ok(Answer {
        json: out,
        select: q.select,
        rows: rows.into_iter().map(|r| r.0).collect(),
        used: used.into_iter().collect(),
    })
}

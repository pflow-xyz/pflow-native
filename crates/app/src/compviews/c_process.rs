//! A run as a process: token identities and the wire layout, a pure
//! (egui-free) port of the prototype's `js/views/process.js` (`runProcess`,
//! `chooseLayout`).
//!
//! A run of a Petri net is a morphism in the monoidal category the net
//! generates (Meseguer and Montanari; Baez, Genovese, Master and Shulman):
//! tokens are wires labelled by their place, each firing a box from its
//! preset to its postset, time left to right. Which steps fire, why one
//! does not, and the marking after each are pflow-metamodel's
//! (`bundle.run`, `plan.opening`): this module reads them and assigns
//! identities first-in first-out per place (this view's choice), then lays
//! the wires out. It never fires anything.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Clone, Debug, PartialEq)]
pub enum ArcT {
    Normal,
    Read,
    Inhibitor,
}

#[derive(Clone, Debug)]
pub struct PArc {
    pub from: String,
    pub to: String,
    pub weight: i64,
    pub typ: ArcT,
}

#[derive(Clone, Debug)]
pub struct PPlace {
    pub id: String,
    pub initial: i64,
    pub capacity: i64,
}

/// The net a run is on: token places and arcs.
#[derive(Clone, Debug, Default)]
pub struct PModel {
    pub places: Vec<PPlace>,
    pub arcs: Vec<PArc>,
}

/// One step of a script, as the core's run reports it.
#[derive(Clone, Debug)]
pub struct PStep {
    pub member: String,
    pub tid: Option<String>,
    pub say: String,
    pub enabled: bool,
    pub why: String,
}

#[derive(Clone, Debug)]
pub struct Token {
    pub id: usize,
    pub place: String,
    /// 1-based serial on its place.
    pub n: usize,
    /// 0 = the domain, k = box k.
    pub born: usize,
    /// Some(k) = consumed by box k, None = still there at the codomain.
    pub died: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct PBox {
    pub k: usize,
    pub tid: String,
    pub member: String,
    pub say: String,
    pub ins: Vec<usize>,
    pub outs: Vec<usize>,
    pub reads: Vec<usize>,
    pub read_shared: Vec<String>,
    /// (place, weight, tokens it has).
    pub inhib: Vec<(String, i64, i64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NeedKind {
    Take,
    Read,
    Inhibitor,
    Capacity,
}

#[derive(Clone, Debug)]
pub struct Need {
    pub kind: NeedKind,
    pub place: String,
    pub need: i64,
    pub have: i64,
    pub tokens: Vec<usize>,
    pub ok: bool,
    pub delta: i64,
    pub after: i64,
}

#[derive(Clone, Debug)]
pub struct Stop {
    pub index: usize,
    pub member: String,
    pub tid: Option<String>,
    pub why: String,
    pub needs: Vec<Need>,
}

#[derive(Clone, Debug)]
pub struct Proc {
    pub initial: BTreeMap<String, i64>,
    pub final_: BTreeMap<String, i64>,
    pub tokens: Vec<Token>,
    pub boxes: Vec<PBox>,
    pub stop: Option<Stop>,
    pub serial: BTreeMap<String, usize>,
}

fn arcs_of<'a>(m: &'a PModel, tid: &str) -> (Vec<&'a PArc>, Vec<&'a PArc>, Vec<&'a PArc>) {
    let places: BTreeSet<&str> = m.places.iter().map(|p| p.id.as_str()).collect();
    let (mut inputs, mut outputs, mut tests) = (vec![], vec![], vec![]);
    for a in &m.arcs {
        let ro = a.typ != ArcT::Normal;
        if a.to == tid && places.contains(a.from.as_str()) {
            if ro {
                tests.push(a)
            } else {
                inputs.push(a)
            }
        } else if a.from == tid && !ro && places.contains(a.to.as_str()) {
            outputs.push(a);
        }
    }
    (inputs, outputs, tests)
}

/// Fires nothing: reads which of `steps` fired (`enabled`), gives each token
/// an identity (first-in first-out per place) and each box its preset and
/// postset. `markings[0]` is the initial marking and `markings[k]` the
/// marking after k firings (pflow-metamodel's).
pub fn run_process(m: &PModel, steps: &[PStep], markings: &[BTreeMap<String, i64>]) -> Proc {
    let initial: BTreeMap<String, i64> = markings
        .first()
        .cloned()
        .unwrap_or_else(|| m.places.iter().map(|p| (p.id.clone(), p.initial)).collect());
    let mut tokens: Vec<Token> = vec![];
    let mut fifo: BTreeMap<String, VecDeque<usize>> = BTreeMap::new();
    let mut serial: BTreeMap<String, usize> = BTreeMap::new();
    fn make(
        tokens: &mut Vec<Token>,
        fifo: &mut BTreeMap<String, VecDeque<usize>>,
        serial: &mut BTreeMap<String, usize>,
        place: &str,
        at: usize,
    ) -> usize {
        let n = serial.get(place).copied().unwrap_or(0) + 1;
        serial.insert(place.to_string(), n);
        let id = tokens.len();
        tokens.push(Token {
            id,
            place: place.to_string(),
            n,
            born: at,
            died: None,
        });
        fifo.entry(place.to_string()).or_default().push_back(id);
        id
    }
    for p in &m.places {
        for _ in 0..initial.get(&p.id).copied().unwrap_or(0) {
            make(&mut tokens, &mut fifo, &mut serial, &p.id, 0);
        }
    }
    let mut boxes: Vec<PBox> = vec![];
    let mut stop = None;
    for (i, st) in steps.iter().enumerate() {
        if !st.enabled || st.tid.is_none() {
            let mk = markings.get(boxes.len()).cloned().unwrap_or_default();
            let needs = st
                .tid
                .as_deref()
                .map(|t| needs_of(m, &mk, t, &fifo))
                .unwrap_or_default();
            stop = Some(Stop {
                index: i,
                member: st.member.clone(),
                tid: st.tid.clone(),
                why: st.why.clone(),
                needs,
            });
            break;
        }
        let tid = st.tid.clone().expect("checked");
        let k = boxes.len() + 1;
        let mk_before = markings.get(k - 1).cloned().unwrap_or_default();
        let (inputs, outputs, tests) = arcs_of(m, &tid);
        let mut b = PBox {
            k,
            tid: tid.clone(),
            member: st.member.clone(),
            say: st.say.clone(),
            ins: vec![],
            outs: vec![],
            reads: vec![],
            read_shared: vec![],
            inhib: vec![],
        };
        for a in &inputs {
            for _ in 0..a.weight.max(1) {
                if let Some(t) = fifo.get_mut(&a.from).and_then(VecDeque::pop_front) {
                    tokens[t].died = Some(k);
                    b.ins.push(t);
                }
            }
        }
        for a in &tests {
            if a.typ == ArcT::Read {
                // tested on the marking before the firing, so it may be
                // satisfied by a token this same firing takes
                let q: Vec<usize> = fifo
                    .get(&a.from)
                    .map(|d| d.iter().copied().collect())
                    .unwrap_or_default();
                let w = a.weight.max(1) as usize;
                for t in q.iter().take(w) {
                    if !b.reads.contains(t) {
                        b.reads.push(*t);
                    }
                }
                if q.len() < w {
                    b.read_shared.push(a.from.clone());
                }
            } else {
                b.inhib.push((
                    a.from.clone(),
                    a.weight.max(1),
                    mk_before.get(&a.from).copied().unwrap_or(0),
                ));
            }
        }
        for a in &outputs {
            for _ in 0..a.weight.max(1) {
                let t = make(&mut tokens, &mut fifo, &mut serial, &a.to, k);
                b.outs.push(t);
            }
        }
        boxes.push(b);
    }
    let final_ = markings
        .get(boxes.len())
        .cloned()
        .unwrap_or_else(|| initial.clone());
    Proc {
        initial,
        final_,
        tokens,
        boxes,
        stop,
        serial,
    }
}

/// What a refused transition needed, place by place, and which tokens it
/// could have used (preset, read and inhibitor arcs, and capacity on the
/// places the firing raises).
fn needs_of(
    m: &PModel,
    mk: &BTreeMap<String, i64>,
    tid: &str,
    fifo: &BTreeMap<String, VecDeque<usize>>,
) -> Vec<Need> {
    let (inputs, outputs, tests) = arcs_of(m, tid);
    let have = |p: &str| mk.get(p).copied().unwrap_or(0);
    let toks = |p: &str, w: i64| -> Vec<usize> {
        fifo.get(p)
            .map(|d| d.iter().copied().take(w.max(1) as usize).collect())
            .unwrap_or_default()
    };
    let mut rows: Vec<Need> = vec![];
    for a in &inputs {
        let h = have(&a.from);
        rows.push(Need {
            kind: NeedKind::Take,
            place: a.from.clone(),
            need: a.weight,
            have: h,
            tokens: toks(&a.from, a.weight),
            ok: h >= a.weight,
            delta: 0,
            after: 0,
        });
    }
    for a in &tests {
        let h = have(&a.from);
        if a.typ == ArcT::Read {
            rows.push(Need {
                kind: NeedKind::Read,
                place: a.from.clone(),
                need: a.weight,
                have: h,
                tokens: toks(&a.from, a.weight),
                ok: h >= a.weight,
                delta: 0,
                after: 0,
            });
        } else {
            rows.push(Need {
                kind: NeedKind::Inhibitor,
                place: a.from.clone(),
                need: a.weight,
                have: h,
                tokens: vec![],
                ok: h < a.weight,
                delta: 0,
                after: 0,
            });
        }
    }
    let mut net: BTreeMap<&str, i64> = BTreeMap::new();
    for a in &inputs {
        *net.entry(a.from.as_str()).or_default() -= a.weight;
    }
    for a in &outputs {
        *net.entry(a.to.as_str()).or_default() += a.weight;
    }
    for p in &m.places {
        if p.capacity <= 0 {
            continue;
        }
        let Some(&delta) = net.get(p.id.as_str()) else {
            continue;
        };
        if delta <= 0 {
            continue;
        }
        let h = have(&p.id);
        rows.push(Need {
            kind: NeedKind::Capacity,
            place: p.id.clone(),
            need: p.capacity,
            have: h,
            tokens: vec![],
            ok: h + delta <= p.capacity,
            delta,
            after: h + delta,
        });
    }
    rows
}

/// Tokens on places the run never touches (and a refused step does not
/// name): identity wires.
pub fn idle_tokens(p: &Proc) -> BTreeSet<usize> {
    let mut touched: BTreeSet<&str> = BTreeSet::new();
    for b in &p.boxes {
        for t in b.ins.iter().chain(&b.outs).chain(&b.reads) {
            touched.insert(p.tokens[*t].place.as_str());
        }
    }
    if let Some(s) = &p.stop {
        for r in &s.needs {
            touched.insert(r.place.as_str());
        }
    }
    p.tokens
        .iter()
        .filter(|t| !touched.contains(t.place.as_str()))
        .map(|t| t.id)
        .collect()
}

/// Steps k > 1 that use no token step k-1 gave, and take no token step k-1
/// read: neighbours on disjoint wires (the interchange law).
pub fn independent_steps(p: &Proc) -> Vec<usize> {
    p.boxes
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(i, b)| {
            let prev = &p.boxes[i - 1];
            !b.ins
                .iter()
                .chain(&b.reads)
                .any(|t| p.tokens[*t].born == prev.k)
                && !b.ins.iter().any(|t| prev.reads.contains(t))
        })
        .map(|(_, b)| b.k)
        .collect()
}

// --- lane layout ------------------------------------------------------------
// The wires alive between two events form an ordered list. Before a box, the
// list is permuted so the box's inputs are contiguous (the wires between
// them move out to the nearer side); the box then replaces them with its
// outputs. Several deterministic starting orders and output orders are tried
// and the one with the fewest crossings wins (ties: least movement).

fn inversions(a: &[usize], b: &[usize]) -> usize {
    let pos: BTreeMap<usize, usize> = b.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let seq: Vec<usize> = a.iter().filter_map(|id| pos.get(id).copied()).collect();
    let mut n = 0;
    for i in 0..seq.len() {
        for j in i + 1..seq.len() {
            if seq[i] > seq[j] {
                n += 1;
            }
        }
    }
    n
}

fn movement(a: &[usize], b: &[usize]) -> usize {
    let pos: BTreeMap<usize, usize> = b.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    a.iter()
        .enumerate()
        .map(|(i, id)| pos.get(id).map_or(0, |p| p.abs_diff(i)))
        .sum()
}

fn gather(cur: &[usize], inb: &BTreeSet<usize>) -> (Vec<usize>, usize) {
    let idx: Vec<usize> = cur
        .iter()
        .enumerate()
        .filter(|(_, id)| inb.contains(id))
        .map(|(i, _)| i)
        .collect();
    if idx.is_empty() {
        return (cur.to_vec(), cur.len());
    }
    let (lo, hi) = (idx[0], idx[idx.len() - 1]);
    let med = idx[(idx.len() - 1) >> 1];
    let (mut up, mut ins, mut down) = (vec![], vec![], vec![]);
    for (i, id) in cur.iter().enumerate().take(hi + 1).skip(lo) {
        if inb.contains(id) {
            ins.push(*id);
        } else if i < med {
            up.push(*id);
        } else {
            down.push(*id);
        }
    }
    let mut list: Vec<usize> = cur[..lo].to_vec();
    let p = lo + up.len();
    list.extend(up);
    list.extend(ins);
    list.extend(down);
    list.extend_from_slice(&cur[hi + 1..]);
    (list, p)
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Policy {
    Top,
    Bottom,
    Mid,
}

fn order_outputs(
    p: &Proc,
    outs: &[usize],
    list: &[usize],
    pp: usize,
    k: usize,
    policy: Policy,
) -> Vec<usize> {
    let pos: BTreeMap<usize, usize> = list.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mid = pp as f64 + (k as f64 - 1.0) / 2.0;
    let key = |t: usize| -> f64 {
        let tok = &p.tokens[t];
        match tok.died {
            None => match policy {
                Policy::Top => -1e9,
                Policy::Bottom => 1e9,
                Policy::Mid => mid,
            },
            Some(d) => {
                let b = &p.boxes[d - 1];
                let others: Vec<usize> = b
                    .ins
                    .iter()
                    .chain(&b.reads)
                    .copied()
                    .filter(|u| *u != t && pos.contains_key(u))
                    .collect();
                if others.is_empty() {
                    mid
                } else {
                    others.iter().map(|u| pos[u] as f64).sum::<f64>() / others.len() as f64
                }
            }
        }
    };
    let mut v: Vec<(usize, f64, usize)> = outs
        .iter()
        .enumerate()
        .map(|(i, t)| (*t, key(*t), i))
        .collect();
    v.sort_by(|a, b| {
        a.1.total_cmp(&b.1)
            .then(
                p.tokens[a.0]
                    .died
                    .map_or(1_000_000_000, |d| d as i64)
                    .cmp(&p.tokens[b.0].died.map_or(1_000_000_000, |d| d as i64)),
            )
            .then(a.2.cmp(&b.2))
    });
    v.into_iter().map(|x| x.0).collect()
}

#[derive(Clone, Debug, Default)]
pub struct Block {
    pub p: usize,
    pub h: usize,
    pub in_block: Vec<usize>,
    pub out_block: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct Sim {
    /// Wires alive just after event e.
    pub a: Vec<Vec<usize>>,
    /// Wires alive just before event e+1.
    pub b: Vec<Vec<usize>>,
    pub blocks: Vec<Block>,
    pub score: usize,
}

fn simulate(
    p: &Proc,
    vis: &BTreeSet<usize>,
    init: &[usize],
    policies: &[Policy],
    min_lanes: &[usize],
    ghost: &BTreeSet<usize>,
) -> Sim {
    let n = p.boxes.len();
    let mut a: Vec<Vec<usize>> = vec![init.to_vec()];
    let mut b: Vec<Vec<usize>> = vec![vec![]; n + 1];
    let mut blocks: Vec<Block> = vec![Block::default()];
    let mut cur = init.to_vec();
    let (mut cross, mut mv) = (0, 0);
    for e in 1..=n {
        let bx = &p.boxes[e - 1];
        let inb: BTreeSet<usize> = bx
            .ins
            .iter()
            .chain(&bx.reads)
            .copied()
            .filter(|t| vis.contains(t))
            .collect();
        let (list, pp) = gather(&cur, &inb);
        cross += inversions(&cur, &list);
        mv += movement(&cur, &list);
        b[e - 1] = list.clone();
        let in_block: Vec<usize> = list[pp..(pp + inb.len()).min(list.len())].to_vec();
        let read_ids: BTreeSet<usize> = bx.reads.iter().copied().collect();
        let outs: Vec<usize> = bx
            .outs
            .iter()
            .copied()
            .filter(|t| vis.contains(t))
            .collect();
        let outs = order_outputs(
            p,
            &outs,
            &list,
            pp,
            inb.len(),
            policies[(e - 1) % policies.len()],
        );
        let mut out_block: Vec<usize> = outs;
        for (i, id) in in_block.iter().enumerate() {
            if read_ids.contains(id) {
                let at = i.min(out_block.len());
                out_block.insert(at, *id);
            }
        }
        let h = in_block
            .len()
            .max(out_block.len())
            .max(min_lanes.get(e).copied().unwrap_or(1))
            .max(1);
        cur = list[..pp]
            .iter()
            .chain(&out_block)
            .chain(&list[(pp + inb.len()).min(list.len())..])
            .copied()
            .collect();
        blocks.push(Block {
            p: pp,
            h,
            in_block,
            out_block,
        });
        a.push(cur.clone());
    }
    if !ghost.is_empty() {
        let (list, _) = gather(&cur, ghost);
        cross += inversions(&cur, &list);
        mv += movement(&cur, &list);
        b[n] = list;
    } else {
        b[n] = cur;
    }
    Sim {
        a,
        b,
        blocks,
        score: cross * 100 + mv,
    }
}

/// The best layout over a few deterministic starting orders and output
/// policies. `group_of` and `place_ix` order tokens by their family.
pub fn choose_layout(
    p: &Proc,
    vis: &BTreeSet<usize>,
    idle: &BTreeSet<usize>,
    group_of: &dyn Fn(&Token) -> i64,
    place_ix: &dyn Fn(&Token) -> i64,
    min_lanes: &[usize],
    ghost: &BTreeSet<usize>,
) -> Sim {
    let at0: Vec<&Token> = p
        .tokens
        .iter()
        .filter(|t| t.born == 0 && vis.contains(&t.id))
        .collect();
    let touched: Vec<&Token> = at0
        .iter()
        .copied()
        .filter(|t| !idle.contains(&t.id))
        .collect();
    let mut idle_v: Vec<&Token> = at0
        .iter()
        .copied()
        .filter(|t| idle.contains(&t.id))
        .collect();
    idle_v.sort_by_key(|t| (place_ix(t), t.n));
    let first = |t: &Token| t.died.map_or(1_000_000_000i64, |d| d as i64);
    let last = |t: &Token| t.died.map_or(-1i64, |d| d as i64);
    type Cmp<'a> = Box<dyn Fn(&Token) -> (i64, i64, i64, i64) + 'a>;
    let inits: Vec<Cmp> = vec![
        Box::new(|t| (first(t), place_ix(t), t.n as i64, 0)),
        Box::new(|t| (last(t), place_ix(t), t.n as i64, 0)),
        Box::new(|t| (group_of(t), first(t), place_ix(t), t.n as i64)),
        Box::new(|t| (place_ix(t), t.n as i64, 0, 0)),
    ];
    let n = p.boxes.len();
    let pols = [Policy::Top, Policy::Bottom, Policy::Mid];
    let mut combos: Vec<Vec<Policy>> = vec![vec![]];
    if n == 0 {
        combos = vec![vec![Policy::Mid]];
    } else if n <= 5 {
        for _ in 0..n {
            combos = combos
                .into_iter()
                .flat_map(|c| {
                    pols.iter().map(move |x| {
                        let mut v = c.clone();
                        v.push(*x);
                        v
                    })
                })
                .collect();
        }
    } else {
        combos = pols.iter().map(|x| vec![*x]).collect();
    }
    let mut best: Option<Sim> = None;
    for cmp in &inits {
        let mut t = touched.clone();
        t.sort_by_key(|x| cmp(x));
        let init: Vec<usize> = t.iter().chain(&idle_v).map(|x| x.id).collect();
        for pol in &combos {
            let r = simulate(p, vis, &init, pol, min_lanes, ghost);
            if best.as_ref().is_none_or(|b| r.score < b.score) {
                best = Some(r);
            }
        }
    }
    best.expect("one starting order was tried")
}

/// Lane y positions (in lanes, from the top) of a list with (optionally)
/// one block that takes `h` lanes: (token -> lane centre in lane units
/// from the top, lanes used).
pub fn lane_ys(
    list: &[usize],
    block: Option<(usize, usize, usize)>,
) -> (BTreeMap<usize, f32>, usize) {
    // block = (p, h, size)
    let mut y: BTreeMap<usize, f32> = BTreeMap::new();
    let (mut lane, mut i) = (0usize, 0usize);
    while i < list.len() {
        if let Some((bp, bh, bs)) = block {
            if i == bp && bs > 0 {
                let off = (bh as f32 - bs as f32) / 2.0;
                for j in 0..bs {
                    if let Some(id) = list.get(i + j) {
                        y.insert(*id, lane as f32 + off + j as f32 + 0.5);
                    }
                }
                lane += bh;
                i += bs;
                continue;
            }
            if i == bp && bs == 0 {
                lane += bh;
            }
        }
        y.insert(list[i], lane as f32 + 0.5);
        lane += 1;
        i += 1;
    }
    if let Some((bp, bh, bs)) = block {
        if bp >= list.len() && bs == 0 {
            lane += bh;
        }
    }
    (y, lane)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> PModel {
        let pl = |id: &str, n: i64| PPlace {
            id: id.into(),
            initial: n,
            capacity: 0,
        };
        let arc = |f: &str, t: &str| PArc {
            from: f.into(),
            to: t.into(),
            weight: 1,
            typ: ArcT::Normal,
        };
        PModel {
            places: vec![pl("a", 2), pl("b", 0), pl("idle", 1)],
            arcs: vec![arc("a", "t"), arc("t", "b"), arc("b", "u"), arc("u", "a")],
        }
    }

    fn mk(v: &[(&str, i64)]) -> BTreeMap<String, i64> {
        v.iter().map(|(k, n)| (k.to_string(), *n)).collect()
    }

    #[test]
    fn tokens_are_first_in_first_out_and_wires_cross_boxes() {
        let steps = vec![
            PStep {
                member: "t".into(),
                tid: Some("t".into()),
                say: "".into(),
                enabled: true,
                why: "".into(),
            },
            PStep {
                member: "u".into(),
                tid: Some("u".into()),
                say: "".into(),
                enabled: true,
                why: "".into(),
            },
            PStep {
                member: "t".into(),
                tid: Some("t".into()),
                say: "".into(),
                enabled: true,
                why: "".into(),
            },
        ];
        let m = [
            mk(&[("a", 2), ("idle", 1)]),
            mk(&[("a", 1), ("b", 1), ("idle", 1)]),
            mk(&[("a", 2), ("idle", 1)]),
            mk(&[("a", 1), ("b", 1), ("idle", 1)]),
        ];
        let p = run_process(&model(), &steps, &m);
        assert_eq!(p.boxes.len(), 3);
        // a#1 is taken first, by box 1; the token b#1 it makes is taken by box 2
        assert_eq!(p.tokens[0].died, Some(1));
        let b1 = &p.boxes[0];
        assert_eq!(b1.outs.len(), 1);
        assert_eq!(p.tokens[b1.outs[0]].died, Some(2));
        // the idle place is never touched
        let idle = idle_tokens(&p);
        assert_eq!(idle.len(), 1);
        assert!(p.stop.is_none());
        assert_eq!(p.final_, m[3]);
    }

    #[test]
    fn a_refused_step_stops_the_run_and_names_what_it_needed() {
        let steps = vec![PStep {
            member: "u".into(),
            tid: Some("u".into()),
            say: "".into(),
            enabled: false,
            why: "no token in b".into(),
        }];
        let m = [mk(&[("a", 2), ("idle", 1)])];
        let p = run_process(&model(), &steps, &m);
        assert!(p.boxes.is_empty());
        let s = p.stop.expect("stopped");
        assert_eq!(s.index, 0);
        assert_eq!(s.needs.len(), 1);
        assert!(!s.needs[0].ok);
        assert_eq!(s.needs[0].place, "b");
    }

    #[test]
    fn the_layout_is_deterministic_and_keeps_every_wire() {
        let steps = vec![
            PStep {
                member: "t".into(),
                tid: Some("t".into()),
                say: "".into(),
                enabled: true,
                why: "".into(),
            },
            PStep {
                member: "u".into(),
                tid: Some("u".into()),
                say: "".into(),
                enabled: true,
                why: "".into(),
            },
        ];
        let m = [
            mk(&[("a", 2), ("idle", 1)]),
            mk(&[("a", 1), ("b", 1), ("idle", 1)]),
            mk(&[("a", 2), ("idle", 1)]),
        ];
        let p = run_process(&model(), &steps, &m);
        let vis: BTreeSet<usize> = p.tokens.iter().map(|t| t.id).collect();
        let idle = idle_tokens(&p);
        let l1 = choose_layout(
            &p,
            &vis,
            &idle,
            &|_| 0,
            &|t| t.place.len() as i64,
            &[0, 1, 1],
            &BTreeSet::new(),
        );
        let l2 = choose_layout(
            &p,
            &vis,
            &idle,
            &|_| 0,
            &|t| t.place.len() as i64,
            &[0, 1, 1],
            &BTreeSet::new(),
        );
        assert_eq!(l1.a, l2.a);
        assert_eq!(l1.a[0].len(), 3);
        assert_eq!(l1.b[2].len(), 3);
    }
}

//! Left-to-right layout of a hypergraph with boundary (a string diagram): a
//! pure (egui-free) port of the prototype's `js/layout/hypergraph.js`.
//!
//! Adapted from Chyp (<https://github.com/akissinger/chyp>), "an interactive
//! theorem prover for string diagrams", Copyright (C) 2022 Aleks Kissinger,
//! Apache License 2.0 (copy in `crates/app/assets/chyp/LICENSE-chyp`), at
//! commit 3d28483:
//!   - `chyp/term.py:30-109`   `layer_decomp`: layers as soon as inputs are
//!     ready, identity boxes for wires that skip a layer;
//!   - `chyp/layout.py:26-142` `convex_layout`: y-coordinates that make wires
//!     as straight as possible (L1), subject to boundary order (>= 1 apart) and
//!     box order in a layer (>= (s1+s2)/2), plus 0.1 x the spread of each
//!     ordered list;
//!   - `chyp/gui/graphscene.py:114-176` port offsets on a box and the tentacle
//!     cubic.
//!
//! No Python was copied. This file re-implements those ideas in Rust, through
//! the prototype's JavaScript, and changes them:
//!   - cvxpy is replaced by a deterministic solver: block coordinate descent
//!     over columns, each column solved exactly by isotonic regression (pool
//!     adjacent violators) under its ordering constraints; L1 by iteratively
//!     reweighted least squares;
//!   - identity boxes become dummy nodes, wire junctions get their own
//!     columns, so one vertex may have several producers and consumers;
//!   - cycles are broken by DFS and drawn as feedback wires below the diagram;
//!   - crossing reduction runs several sweeps and keeps the best ordering.
//!
//! Deterministic: no randomness, no iteration over unordered maps.
//!
//! The Laws view uses it where Chyp has no coordinates: the live plan's order
//! chain. Chyp's own coordinates (the export) are used everywhere else, and
//! `tests/chyp_layout_receipts.rs`-style checks below hold this solver to
//! cvxpy's optimum on the receipts of `assets/chyp/receipts/chyp-layout.json`.

#![allow(dead_code)] // the whole layout is returned, the live chain reads part of it

use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub struct HBox {
    pub id: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub width: Option<f64>,
    pub height: Option<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct HGraph {
    pub boxes: Vec<HBox>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Offset of port k of n on a box of height h, from the box centre.
pub type PortOffsets = Box<dyn Fn(usize, usize, f64) -> f64>;

pub struct Opts {
    pub box_width: f64,
    pub box_min_height: f64,
    pub port_pitch: f64,
    pub row_gap: f64,
    pub wire_gap: f64,
    pub gap: f64,
    pub margin: f64,
    pub feedback_gap: f64,
    /// `true`: the L1 objective (Chyp's); `false`: least squares.
    pub l1: bool,
    pub compact: f64,
    /// Offset of port k of n on a box of height h, from the box centre.
    pub port_offsets: Option<PortOffsets>,
    /// Fixes the order of boxes per layer (skips crossing reduction).
    pub layer_order: Option<Vec<Vec<String>>>,
    pub order_sweeps: usize,
    pub max_sweeps: usize,
    pub tol: f64,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            box_width: 64.0,
            box_min_height: 32.0,
            port_pitch: 16.0,
            row_gap: 16.0,
            wire_gap: 16.0,
            gap: 48.0,
            margin: 24.0,
            feedback_gap: 16.0,
            l1: true,
            compact: 0.1,
            port_offsets: None,
            layer_order: None,
            order_sweeps: 4,
            max_sweeps: 3000,
            tol: 1e-6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug)]
pub struct LBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub in_y: Vec<f64>,
    pub out_y: Vec<f64>,
    pub in_x: f64,
    pub out_x: f64,
    pub layer: usize,
}

#[derive(Clone, Debug)]
pub struct LVertex {
    pub id: String,
    pub x: f64,
    pub y: f64,
    /// One polyline per end, left to right: producer pin .. junction, junction .. consumer pin.
    pub paths: Vec<Vec<Pt>>,
    /// The whole wire when it has one producer and one consumer.
    pub route: Option<Vec<Pt>>,
    /// Orthogonal routes for ends that run right to left (cycles).
    pub feedback: Vec<Vec<Pt>>,
}

#[derive(Clone, Debug)]
pub struct Stats {
    pub crossings: usize,
    pub bend: f64,
    pub objective: f64,
    pub sweeps: usize,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub boxes: BTreeMap<String, LBox>,
    pub vertices: Vec<LVertex>,
    pub inputs: Vec<(Pt, String)>,
    pub outputs: Vec<(Pt, String)>,
    pub layers: Vec<Vec<String>>,
    pub width: f64,
    pub height: f64,
    pub stats: Stats,
}

// --- 1. the graph: vertices, ends, feedback ------------------------------------------

#[derive(Clone, Copy, Debug)]
enum End {
    In(usize),
    Out(usize),
    Box { b: usize, port: usize },
}

struct Vx {
    id: String,
    producers: Vec<End>,
    consumers: Vec<End>,
}

struct Norm {
    verts: Vec<Vx>,
    v_index: BTreeMap<String, usize>,
}

fn normalize(g: &HGraph) -> Norm {
    let mut n = Norm {
        verts: vec![],
        v_index: BTreeMap::new(),
    };
    fn vert<'a>(n: &'a mut Norm, id: &str) -> &'a mut Vx {
        if !n.v_index.contains_key(id) {
            n.v_index.insert(id.to_string(), n.verts.len());
            n.verts.push(Vx {
                id: id.to_string(),
                producers: vec![],
                consumers: vec![],
            });
        }
        let i = n.v_index[id];
        &mut n.verts[i]
    }
    for (i, v) in g.inputs.iter().enumerate() {
        vert(&mut n, v).producers.push(End::In(i));
    }
    for (bi, b) in g.boxes.iter().enumerate() {
        for (k, v) in b.inputs.iter().enumerate() {
            vert(&mut n, v).consumers.push(End::Box { b: bi, port: k });
        }
        for (k, v) in b.outputs.iter().enumerate() {
            vert(&mut n, v).producers.push(End::Box { b: bi, port: k });
        }
    }
    for (j, v) in g.outputs.iter().enumerate() {
        vert(&mut n, v).consumers.push(End::Out(j));
    }
    n
}

/// Layers: longest path over the DAG left after a DFS in box order marks back
/// edges and self-loops as feedback; as soon as inputs are ready (term.py:52-80).
fn assign_layers(g: &HGraph, n: &Norm) -> Vec<usize> {
    let nb = g.boxes.len();
    let mut succ: Vec<Vec<usize>> = vec![vec![]; nb];
    for (a, b) in g.boxes.iter().enumerate() {
        for v in &b.outputs {
            let vx = &n.verts[n.v_index[v]];
            for c in &vx.consumers {
                if let End::Box { b: cb, .. } = c {
                    succ[a].push(*cb);
                }
            }
        }
    }
    let mut state = vec![0u8; nb];
    let mut back: BTreeSet<(usize, usize)> = BTreeSet::new();
    for s in 0..nb {
        if state[s] != 0 {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(s, 0)];
        state[s] = 1;
        while let Some(&(a, i)) = stack.last() {
            if i < succ[a].len() {
                stack.last_mut().expect("non-empty").1 += 1;
                let b = succ[a][i];
                if state[b] == 1 {
                    back.insert((a, b));
                } else if state[b] == 0 {
                    state[b] = 1;
                    stack.push((b, 0));
                }
            } else {
                state[a] = 2;
                stack.pop();
            }
        }
    }
    let mut indeg = vec![0usize; nb];
    for (a, sa) in succ.iter().enumerate() {
        for &b in sa {
            if !back.contains(&(a, b)) {
                indeg[b] += 1;
            }
        }
    }
    let mut layer = vec![1usize; nb];
    let mut queue: Vec<usize> = (0..nb).filter(|a| indeg[*a] == 0).collect();
    let mut qi = 0;
    while qi < queue.len() {
        let a = queue[qi];
        qi += 1;
        for &b in &succ[a] {
            if back.contains(&(a, b)) {
                continue;
            }
            layer[b] = layer[b].max(layer[a] + 1);
            indeg[b] -= 1;
            if indeg[b] == 0 {
                queue.push(b);
            }
        }
    }
    layer
}

// --- 2. the layered graph: columns, nodes, springs -------------------------------------

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Box,
    Pin,
    Junction,
    Dummy,
}

#[derive(Clone, Debug)]
struct Node {
    kind: Kind,
    col: usize,
    /// Box index in `g.boxes` (boxes), pin index (pins).
    index: usize,
    w: f64,
    h: f64,
    in_off: Vec<f64>,
    out_off: Vec<f64>,
}

#[derive(Clone, Copy, Debug)]
struct ERef {
    node: usize,
    off: f64,
}

struct Spring {
    a: ERef,
    b: ERef,
    w: f64,
}

struct WireB {
    j: usize,
    prod_paths: Vec<Vec<ERef>>,
    cons_paths: Vec<Vec<ERef>>,
    feedback: Vec<(ERef, ERef)>,
}

struct Built {
    nodes: Vec<Node>,
    springs: Vec<Spring>,
    wires: Vec<WireB>,
    r: usize,
    n_layers: usize,
    box_node: Vec<usize>,
    in_pin: Vec<usize>,
    out_pin: Vec<usize>,
}

fn build(g: &HGraph, n: &Norm, layers: &[usize], o: &Opts) -> Built {
    let n_l = layers.iter().copied().max().unwrap_or(0);
    let r = 2 * n_l + 2;
    let mut nodes: Vec<Node> = vec![];
    let add = |nodes: &mut Vec<Node>, node: Node| -> usize {
        nodes.push(node);
        nodes.len() - 1
    };
    let offsets_for = |count: usize, h: f64| -> Vec<f64> {
        (0..count)
            .map(|k| match &o.port_offsets {
                Some(f) => f(k, count, h),
                None => ((k as f64 + 0.5) / count as f64 - 0.5) * h,
            })
            .collect()
    };
    let mut box_node = vec![];
    for (bi, b) in g.boxes.iter().enumerate() {
        let (n_in, n_out) = (b.inputs.len(), b.outputs.len());
        let w = b.width.unwrap_or(o.box_width);
        let h = b.height.unwrap_or_else(|| {
            o.box_min_height
                .max(n_in.max(n_out).max(1) as f64 * o.port_pitch)
        });
        box_node.push(add(
            &mut nodes,
            Node {
                kind: Kind::Box,
                col: 2 * layers[bi],
                index: bi,
                w,
                h,
                in_off: offsets_for(n_in, h),
                out_off: offsets_for(n_out, h),
            },
        ));
    }
    let pin = |col: usize, index: usize| Node {
        kind: Kind::Pin,
        col,
        index,
        w: 0.0,
        h: 0.0,
        in_off: vec![],
        out_off: vec![],
    };
    let in_pin: Vec<usize> = (0..g.inputs.len())
        .map(|i| add(&mut nodes, pin(0, i)))
        .collect();
    let out_pin: Vec<usize> = (0..g.outputs.len())
        .map(|j| add(&mut nodes, pin(r, j)))
        .collect();
    let end_of = |nodes: &[Node], e: &End, producer: bool| -> ERef {
        match e {
            End::In(i) => ERef {
                node: in_pin[*i],
                off: 0.0,
            },
            End::Out(j) => ERef {
                node: out_pin[*j],
                off: 0.0,
            },
            End::Box { b, port } => {
                let nd = &nodes[box_node[*b]];
                ERef {
                    node: box_node[*b],
                    off: if producer {
                        nd.out_off[*port]
                    } else {
                        nd.in_off[*port]
                    },
                }
            }
        }
    };
    let mut springs: Vec<Spring> = vec![];
    let mut wires: Vec<WireB> = vec![];
    for vx in &n.verts {
        let prod: Vec<ERef> = vx
            .producers
            .iter()
            .map(|e| end_of(&nodes, e, true))
            .collect();
        let cons: Vec<ERef> = vx
            .consumers
            .iter()
            .map(|e| end_of(&nodes, e, false))
            .collect();
        let pc: Vec<usize> = prod.iter().map(|e| nodes[e.node].col).collect();
        let cc: Vec<usize> = cons.iter().map(|e| nodes[e.node].col).collect();
        let c_j = if !pc.is_empty() {
            pc.iter().copied().max().expect("non-empty") + 1
        } else if !cc.is_empty() {
            cc.iter()
                .copied()
                .min()
                .expect("non-empty")
                .saturating_sub(1)
        } else {
            1
        };
        let j = add(
            &mut nodes,
            Node {
                kind: Kind::Junction,
                col: c_j,
                index: 0,
                w: 0.0,
                h: 0.0,
                in_off: vec![],
                out_off: vec![],
            },
        );
        let chain =
            |nodes: &mut Vec<Node>, springs: &mut Vec<Spring>, from: ERef, to: ERef| -> Vec<ERef> {
                let mut path = vec![from];
                let mut prev = from;
                let (c0, c1) = (nodes[from.node].col, nodes[to.node].col);
                for c in c0 + 1..c1 {
                    let d = ERef {
                        node: add(
                            nodes,
                            Node {
                                kind: Kind::Dummy,
                                col: c,
                                index: 0,
                                w: 0.0,
                                h: 0.0,
                                in_off: vec![],
                                out_off: vec![],
                            },
                        ),
                        off: 0.0,
                    };
                    springs.push(Spring {
                        a: prev,
                        b: d,
                        w: 1.0,
                    });
                    path.push(d);
                    prev = d;
                }
                springs.push(Spring {
                    a: prev,
                    b: to,
                    w: 1.0,
                });
                path.push(to);
                path
            };
        let j_end = ERef { node: j, off: 0.0 };
        let prod_paths: Vec<Vec<ERef>> = prod
            .iter()
            .map(|e| chain(&mut nodes, &mut springs, *e, j_end))
            .collect();
        let mut cons_paths = vec![];
        let mut feedback = vec![];
        for e in &cons {
            if nodes[e.node].col > c_j {
                cons_paths.push(chain(&mut nodes, &mut springs, j_end, *e));
            } else {
                feedback.push((j_end, *e));
            }
        }
        wires.push(WireB {
            j,
            prod_paths,
            cons_paths,
            feedback,
        });
    }
    Built {
        nodes,
        springs,
        wires,
        r,
        n_layers: n_l,
        box_node,
        in_pin,
        out_pin,
    }
}

// --- 3. ordering within columns (crossing reduction) -----------------------------------

fn columns_of(l: &Built) -> Vec<Vec<usize>> {
    let mut cols: Vec<Vec<usize>> = vec![vec![]; l.r + 1];
    for (i, nd) in l.nodes.iter().enumerate() {
        cols[nd.col].push(i);
    }
    // boundaries keep their given order; inner columns start in creation order
    cols[0].sort_by_key(|n| l.nodes[*n].index);
    let r = l.r;
    cols[r].sort_by_key(|n| l.nodes[*n].index);
    cols
}

fn subpos(l: &Built, e: &ERef) -> f64 {
    let nd = &l.nodes[e.node];
    if nd.kind != Kind::Box || nd.h <= 0.0 {
        return 0.0;
    }
    ((e.off / nd.h) * 0.9).clamp(-0.45, 0.45)
}

fn count_crossings(l: &Built, cols: &[Vec<usize>]) -> usize {
    let mut pos = vec![0.0f64; l.nodes.len()];
    for col in cols {
        for (i, n) in col.iter().enumerate() {
            pos[*n] = i as f64;
        }
    }
    let mut total = 0;
    for c in 0..cols.len().saturating_sub(1) {
        let ss: Vec<(f64, f64)> = l
            .springs
            .iter()
            .filter(|s| l.nodes[s.a.node].col == c && l.nodes[s.b.node].col == c + 1)
            .map(|s| {
                (
                    pos[s.a.node] + subpos(l, &s.a),
                    pos[s.b.node] + subpos(l, &s.b),
                )
            })
            .collect();
        for i in 0..ss.len() {
            for j in i + 1..ss.len() {
                if (ss[i].0 - ss[j].0) * (ss[i].1 - ss[j].1) < 0.0 {
                    total += 1;
                }
            }
        }
    }
    total
}

type Nbrs = Vec<Vec<(ERef, ERef)>>;

/// One barycentre pass over column c against column c + dir (dir = -1 forward, +1 back).
fn reorder(
    l: &Built,
    cols: &mut [Vec<usize>],
    c: usize,
    dir: i32,
    nbrs: &Nbrs,
    fixed: Option<&BTreeMap<usize, usize>>,
) {
    let oc = c as i32 + dir;
    if oc < 0 || oc as usize >= cols.len() || cols[c].len() < 2 {
        return;
    }
    let oc = oc as usize;
    let col = cols[c].clone();
    let other = &cols[oc];
    let other_pos: BTreeMap<usize, f64> = other
        .iter()
        .enumerate()
        .map(|(i, n)| (*n, (i as f64 + 0.5) / other.len() as f64))
        .collect();
    let mut bary: BTreeMap<usize, f64> = BTreeMap::new();
    for (i, n) in col.iter().enumerate() {
        let (mut sum, mut cnt) = (0.0, 0);
        for (mine, theirs) in &nbrs[*n] {
            if l.nodes[theirs.node].col != oc {
                continue;
            }
            sum += other_pos[&theirs.node] + subpos(l, theirs) / other.len() as f64
                - subpos(l, mine) / col.len() as f64;
            cnt += 1;
        }
        bary.insert(
            *n,
            if cnt > 0 {
                sum / cnt as f64
            } else {
                (i as f64 + 0.5) / col.len() as f64
            },
        );
    }
    let cur: BTreeMap<usize, usize> = col.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let mut sorted = col.clone();
    sorted.sort_by(|a, b| bary[a].total_cmp(&bary[b]).then(cur[a].cmp(&cur[b])));
    if let Some(fixed) = fixed {
        // keep the slot pattern (box or not) from the barycentre order, fill box slots in the given order
        let mut boxes: Vec<usize> = col
            .iter()
            .copied()
            .filter(|n| l.nodes[*n].kind == Kind::Box)
            .collect();
        boxes.sort_by_key(|n| fixed[&l.nodes[*n].index]);
        let mut k = 0;
        for slot in sorted.iter_mut() {
            if l.nodes[*slot].kind == Kind::Box {
                *slot = boxes[k];
                k += 1;
            }
        }
    }
    cols[c] = sorted;
}

fn order_columns(l: &Built, g: &HGraph, o: &Opts) -> (Vec<Vec<usize>>, usize) {
    let mut cols = columns_of(l);
    let mut nbrs: Nbrs = vec![vec![]; l.nodes.len()];
    for s in &l.springs {
        nbrs[s.a.node].push((s.a, s.b));
        nbrs[s.b.node].push((s.b, s.a));
    }
    let inner: Vec<usize> = (1..l.r).collect();
    if let Some(lo) = &o.layer_order {
        let box_index: BTreeMap<&str, usize> = g
            .boxes
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id.as_str(), i))
            .collect();
        let mut fixed: BTreeMap<usize, usize> = BTreeMap::new();
        for layer in lo {
            for (i, id) in layer.iter().enumerate() {
                if let Some(bi) = box_index.get(id.as_str()) {
                    fixed.insert(*bi, i);
                }
            }
        }
        for b in 0..g.boxes.len() {
            fixed.entry(b).or_insert(usize::MAX - (g.boxes.len() - b));
        }
        for &c in &inner {
            reorder(l, &mut cols, c, -1, &nbrs, Some(&fixed));
        }
        let x = count_crossings(l, &cols);
        return (cols, x);
    }
    for &c in &inner {
        reorder(l, &mut cols, c, -1, &nbrs, None);
    }
    let mut best = cols.clone();
    let mut best_x = count_crossings(l, &cols);
    let mut sweep = 0;
    while sweep < o.order_sweeps && best_x > 0 {
        for &c in inner.iter().rev() {
            reorder(l, &mut cols, c, 1, &nbrs, None);
        }
        let x = count_crossings(l, &cols);
        if x < best_x {
            best_x = x;
            best = cols.clone();
        }
        for &c in &inner {
            reorder(l, &mut cols, c, -1, &nbrs, None);
        }
        let x = count_crossings(l, &cols);
        if x < best_x {
            best_x = x;
            best = cols.clone();
        }
        sweep += 1;
    }
    (best, best_x)
}

// --- 4. y-coordinates -----------------------------------------------------------------
//
// Minimise   sum_springs w·|y_a + off_a - y_b - off_b|  +  compact · (span of each
//            boundary and box column)
// subject to y_next - y_prev >= ext_prev + ext_next for consecutive nodes in a column.
// That is convex_layout's problem (layout.py:82-117) over this layered graph. Every
// constraint lives in one column, so block coordinate descent over columns converges;
// each block is a weighted isotonic regression after shifting out the separations.

fn extent(nd: &Node, o: &Opts) -> f64 {
    if nd.kind == Kind::Box {
        nd.h / 2.0 + o.row_gap / 2.0
    } else {
        o.wire_gap / 2.0
    }
}

/// Weighted isotonic (non-decreasing) regression.
fn pava(t: &[f64], w: &[f64]) -> Vec<f64> {
    let (mut val, mut wt, mut cnt): (Vec<f64>, Vec<f64>, Vec<usize>) = (vec![], vec![], vec![]);
    for i in 0..t.len() {
        val.push(t[i]);
        wt.push(w[i]);
        cnt.push(1);
        while val.len() > 1 && val[val.len() - 2] > val[val.len() - 1] {
            let v2 = val.pop().expect("two");
            let w2 = wt.pop().expect("two");
            let c2 = cnt.pop().expect("two");
            let k = val.len() - 1;
            let ww = wt[k] + w2;
            val[k] = (val[k] * wt[k] + v2 * w2) / ww;
            wt[k] = ww;
            cnt[k] += c2;
        }
    }
    let mut out = Vec::with_capacity(t.len());
    for k in 0..val.len() {
        for _ in 0..cnt[k] {
            out.push(val[k]);
        }
    }
    out
}

struct Solved {
    y: Vec<f64>,
    sweeps: usize,
    bend: f64,
    objective: f64,
}

fn solve_y(l: &Built, cols: &[Vec<usize>], o: &Opts) -> Solved {
    let n = l.nodes.len();
    let mut y = vec![0.0f64; n];
    let seps: Vec<Vec<f64>> = cols
        .iter()
        .map(|col| {
            let mut s = vec![0.0];
            for j in 1..col.len() {
                let v = s[j - 1] + extent(&l.nodes[col[j - 1]], o) + extent(&l.nodes[col[j]], o);
                s.push(v);
            }
            s
        })
        .collect();
    for (c, col) in cols.iter().enumerate() {
        let sc = &seps[c];
        let mid = sc.last().copied().unwrap_or(0.0) / 2.0;
        for (j, nn) in col.iter().enumerate() {
            y[*nn] = sc[j] - mid;
        }
    }
    // per node: (spring k, own offset, other node, other offset)
    let mut adj: Vec<Vec<(usize, f64, usize, f64)>> = vec![vec![]; n];
    for (k, s) in l.springs.iter().enumerate() {
        adj[s.a.node].push((k, s.a.off, s.b.node, s.b.off));
        adj[s.b.node].push((k, s.b.off, s.a.node, s.a.off));
    }
    let is_compact = |c: usize| c == 0 || c == l.r || c.is_multiple_of(2);
    let unit = if o.wire_gap != 0.0 { o.wire_gap } else { 1.0 };
    let delta = 1e-3 * unit;
    let eps = 1e-9 / unit;
    let tol = o.tol * unit;
    let mut qw = vec![0.0f64; l.springs.len()];
    let mut sweeps = 0usize;
    let resid = |y: &[f64], s: &Spring| y[s.a.node] + s.a.off - y[s.b.node] - s.b.off;
    let objective = |y: &[f64]| -> f64 {
        let mut f = 0.0;
        for s in &l.springs {
            f += s.w * resid(y, s).abs();
        }
        for (c, col) in cols.iter().enumerate() {
            if is_compact(c) && col.len() > 1 {
                f += o.compact * (y[col[col.len() - 1]] - y[col[0]]);
            }
        }
        f
    };
    let fwd: Vec<usize> = (0..cols.len()).collect();
    let bwd: Vec<usize> = fwd.iter().rev().copied().collect();
    // One Gauss-Seidel sweep: each column, in turn, minimises the current quadratic
    // (springs weighted by qw, plus the linear spread term) exactly under its order.
    let sweep = |y: &mut Vec<f64>, qw: &[f64], sweeps: &mut usize| -> f64 {
        *sweeps += 1;
        let mut moved = 0.0f64;
        let order = if *sweeps % 2 == 1 { &fwd } else { &bwd };
        for &c in order {
            let col = &cols[c];
            let m = col.len();
            if m == 0 {
                continue;
            }
            let (mut a_, mut b_, mut t) = (vec![0.0; m], vec![0.0; m], vec![0.0; m]);
            for j in 0..m {
                let (mut a, mut b) = (eps, 0.0);
                for &(k, own, other, other_off) in &adj[col[j]] {
                    a += qw[k];
                    b += qw[k] * (y[other] + other_off - own);
                }
                a_[j] = a;
                b_[j] = b;
            }
            if is_compact(c) && m > 1 {
                b_[0] += o.compact;
                b_[m - 1] -= o.compact;
            }
            let sc = &seps[c];
            for j in 0..m {
                t[j] = b_[j] / a_[j] - sc[j];
            }
            let z = pava(&t, &a_);
            for j in 0..m {
                let ny = z[j] + sc[j];
                moved = moved.max((ny - y[col[j]]).abs());
                y[col[j]] = ny;
            }
        }
        moved
    };
    // Least squares first (cost w·r²/(2·unit)): the 'l2' objective, and the start for 'l1'.
    for (k, s) in l.springs.iter().enumerate() {
        qw[k] = s.w / unit;
    }
    while sweeps < o.max_sweeps && sweep(&mut y, &qw, &mut sweeps) >= tol {}
    if o.l1 {
        // L1 by iteratively reweighted least squares. The reweighted quadratic
        // majorises the delta-smoothed L1 cost and touches it at the current
        // point, so every sweep that lowers the quadratic lowers the true cost.
        // Straight wires get weight 1/delta and act as rods that one-column
        // moves shift only slowly, so delta is annealed from half a wire gap
        // down to 1e-3 of one.
        let mut levels: Vec<f64> = [0.5, 0.2, 0.1, 0.05, 0.02, 0.01, 0.005, 0.002]
            .iter()
            .map(|d| d * unit)
            .filter(|d| *d > delta)
            .collect();
        levels.push(delta);
        let mut li = 0;
        while li < levels.len() && sweeps < o.max_sweeps {
            let d = levels[li];
            let last = li == levels.len() - 1;
            let level_tol = if last { tol } else { tol.max(1e-4 * unit) };
            let mut prev = f64::INFINITY;
            while sweeps < o.max_sweeps {
                for (k, s) in l.springs.iter().enumerate() {
                    qw[k] = s.w / resid(&y, s).abs().max(d);
                }
                let moved = sweep(&mut y, &qw, &mut sweeps);
                let f = objective(&y);
                if moved < level_tol && (prev - f).abs() <= 1e-10 * (1.0 + f) {
                    break;
                }
                prev = f;
            }
            li += 1;
        }
    }
    let bend: f64 = l.springs.iter().map(|s| resid(&y, s).abs()).sum();
    let obj = objective(&y);
    Solved {
        y,
        sweeps,
        bend,
        objective: obj,
    }
}

// --- 5. coordinates --------------------------------------------------------------------

fn round(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

pub fn layout_hypergraph(g: &HGraph, o: &Opts) -> Layout {
    let norm = normalize(g);
    let layer = assign_layers(g, &norm);
    let l = build(g, &norm, &layer, o);
    let (cols, crossings) = order_columns(&l, g, o);
    let sv = solve_y(&l, &cols, o);
    let y = &sv.y;
    // x: boundary columns have no width, wire columns `gap`, box columns their widest box
    let col_w: Vec<f64> = cols
        .iter()
        .enumerate()
        .map(|(c, col)| {
            if c == 0 || c == l.r {
                0.0
            } else if c % 2 == 1 {
                o.gap
            } else {
                col.iter()
                    .map(|n| {
                        if l.nodes[*n].kind == Kind::Box {
                            l.nodes[*n].w
                        } else {
                            0.0
                        }
                    })
                    .fold(0.0, f64::max)
            }
        })
        .collect();
    let (mut col_x, mut col_l, mut col_rt) = (vec![], vec![], vec![]);
    let mut cursor = o.margin;
    for w in &col_w {
        col_l.push(cursor);
        col_x.push(cursor + w / 2.0);
        col_rt.push(cursor + w);
        cursor += w;
    }
    let width = cursor + o.margin;
    let (mut top, mut bottom) = (f64::INFINITY, f64::NEG_INFINITY);
    for (i, nd) in l.nodes.iter().enumerate() {
        let half = if nd.kind == Kind::Box {
            nd.h / 2.0
        } else {
            0.0
        };
        top = top.min(y[i] - half);
        bottom = bottom.max(y[i] + half);
    }
    if !top.is_finite() {
        top = 0.0;
        bottom = 0.0;
    }
    let dy = o.margin - top;
    let yy = |n: usize| round(y[n] + dy);
    let pt = |e: &ERef, role: &str| -> Pt {
        let nd = &l.nodes[e.node];
        if nd.kind == Kind::Box {
            Pt {
                x: round(if role == "producer" {
                    col_x[nd.col] + nd.w / 2.0
                } else {
                    col_x[nd.col] - nd.w / 2.0
                }),
                y: round(y[e.node] + dy + e.off),
            }
        } else {
            Pt {
                x: round(col_x[nd.col]),
                y: yy(e.node),
            }
        }
    };
    let path_points = |path: &[ERef], first: &str, last: &str| -> Vec<Pt> {
        let mut out = vec![];
        for (i, e) in path.iter().enumerate() {
            let nd = &l.nodes[e.node];
            if i == 0 {
                out.push(pt(e, first));
            } else if i == path.len() - 1 {
                out.push(pt(e, last));
            } else if nd.kind == Kind::Dummy && nd.col.is_multiple_of(2) {
                // through a box column: level, across it
                out.push(Pt {
                    x: round(col_l[nd.col]),
                    y: yy(e.node),
                });
                out.push(Pt {
                    x: round(col_rt[nd.col]),
                    y: yy(e.node),
                });
            } else {
                out.push(Pt {
                    x: round(col_x[nd.col]),
                    y: yy(e.node),
                });
            }
        }
        out
    };
    let mut boxes: BTreeMap<String, LBox> = BTreeMap::new();
    let mut layers: Vec<Vec<String>> = vec![vec![]; l.n_layers];
    for (c, col) in cols.iter().enumerate() {
        if c % 2 != 0 || c == 0 || c == l.r {
            continue;
        }
        for &n in col.iter().filter(|n| l.nodes[**n].kind == Kind::Box) {
            let nd = &l.nodes[n];
            let b = &g.boxes[nd.index];
            layers[c / 2 - 1].push(b.id.clone());
            boxes.insert(
                b.id.clone(),
                LBox {
                    x: round(col_x[c]),
                    y: yy(n),
                    w: nd.w,
                    h: nd.h,
                    in_y: nd.in_off.iter().map(|off| round(y[n] + dy + off)).collect(),
                    out_y: nd
                        .out_off
                        .iter()
                        .map(|off| round(y[n] + dy + off))
                        .collect(),
                    in_x: round(col_x[c] - nd.w / 2.0),
                    out_x: round(col_x[c] + nd.w / 2.0),
                    layer: c / 2,
                },
            );
        }
    }
    let mut lane = 0usize;
    let mut vertices = vec![];
    for (vi, w) in l.wires.iter().enumerate() {
        let mut paths: Vec<Vec<Pt>> = vec![];
        for p in &w.prod_paths {
            paths.push(path_points(p, "producer", "junction"));
        }
        for p in &w.cons_paths {
            paths.push(path_points(p, "junction", "consumer"));
        }
        let (np, nc) = (w.prod_paths.len(), w.cons_paths.len());
        let route = if np <= 1 && nc <= 1 && np + nc > 0 && w.feedback.is_empty() {
            let a = if np > 0 {
                path_points(&w.prod_paths[0], "producer", "junction")
            } else {
                vec![]
            };
            let b = if nc > 0 {
                path_points(&w.cons_paths[0], "junction", "consumer")
            } else {
                vec![]
            };
            Some(if !a.is_empty() && !b.is_empty() {
                a.into_iter().chain(b.into_iter().skip(1)).collect()
            } else if !a.is_empty() {
                a
            } else {
                b
            })
        } else {
            None
        };
        let mut feedback = vec![];
        for (j_end, e) in &w.feedback {
            lane += 1;
            let lane_y = round(bottom + dy + lane as f64 * o.feedback_gap);
            let j = pt(j_end, "junction");
            let p = pt(e, "consumer");
            let ecol = l.nodes[e.node].col;
            let turn_in = if ecol >= 1 {
                round(col_x[ecol - 1])
            } else {
                p.x
            };
            feedback.push(vec![
                j,
                Pt { x: j.x, y: lane_y },
                Pt {
                    x: turn_in,
                    y: lane_y,
                },
                Pt { x: turn_in, y: p.y },
                p,
            ]);
        }
        let j = pt(
            &ERef {
                node: w.j,
                off: 0.0,
            },
            "junction",
        );
        vertices.push(LVertex {
            id: norm.verts[vi].id.clone(),
            x: j.x,
            y: j.y,
            paths,
            route,
            feedback,
        });
    }
    let height = round(bottom + dy + lane as f64 * o.feedback_gap + o.margin);
    Layout {
        boxes,
        vertices,
        inputs: g
            .inputs
            .iter()
            .enumerate()
            .map(|(i, v)| {
                (
                    Pt {
                        x: round(col_x[0]),
                        y: yy(l.in_pin[i]),
                    },
                    v.clone(),
                )
            })
            .collect(),
        outputs: g
            .outputs
            .iter()
            .enumerate()
            .map(|(j, v)| {
                (
                    Pt {
                        x: round(col_x[l.r]),
                        y: yy(l.out_pin[j]),
                    },
                    v.clone(),
                )
            })
            .collect(),
        layers,
        width: round(width),
        height,
        stats: Stats {
            crossings,
            bend: round(sv.bend),
            objective: round(sv.objective),
            sweeps: sv.sweeps,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const RECEIPT: &str = include_str!("../../assets/chyp/receipts/chyp-layout.json");

    fn sv(v: &Value) -> Vec<String> {
        v.as_array()
            .map(|a| {
                a.iter()
                    .map(|x| x.as_str().unwrap_or("").to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// A receipt graph (boxes carry Chyp's `box_size`, 1 or 2 units, as their height).
    fn graph(v: &Value) -> HGraph {
        HGraph {
            boxes: v["boxes"]
                .as_array()
                .expect("boxes")
                .iter()
                .map(|b| HBox {
                    id: b["id"].as_str().unwrap_or("").into(),
                    inputs: sv(&b["inputs"]),
                    outputs: sv(&b["outputs"]),
                    width: None,
                    height: b["box_size"].as_f64(),
                })
                .collect(),
            inputs: sv(&v["inputs"]),
            outputs: sv(&v["outputs"]),
        }
    }

    /// Chyp's geometry in its own units (layout.py:82-111, graphscene.py:152-165): boundary
    /// vertices >= 1 apart, boxes >= (s1+s2)/2 apart with box_size s in {1, 2}, ports at
    /// i/(n-1) - 0.5, L1 objective with 0.1 x spread.
    fn chyp_opts() -> Opts {
        Opts {
            row_gap: 0.0,
            wire_gap: 1.0,
            margin: 0.0,
            gap: 1.9,
            box_width: 1.1,
            port_offsets: Some(Box::new(|i, n, _| {
                if n <= 1 {
                    0.0
                } else {
                    i as f64 / (n as f64 - 1.0) - 0.5
                }
            })),
            ..Opts::default()
        }
    }

    /// `convex_layout`'s objective (layout.py:82-117) on any layout of a monogamous graph.
    fn chyp_objective(g: &HGraph, lay: &Layout, layers: &[Vec<String>]) -> f64 {
        let mut f = 0.0;
        let spread = |ys: &[f64], f: &mut f64| {
            for i in 1..ys.len() {
                *f += 0.1 * (ys[i] - ys[i - 1]).abs();
            }
        };
        spread(
            &lay.inputs.iter().map(|p| p.0.y).collect::<Vec<_>>(),
            &mut f,
        );
        spread(
            &lay.outputs.iter().map(|p| p.0.y).collect::<Vec<_>>(),
            &mut f,
        );
        for l in layers {
            spread(
                &l.iter().map(|id| lay.boxes[id].y).collect::<Vec<_>>(),
                &mut f,
            );
        }
        let (mut from, mut to): (BTreeMap<&str, f64>, BTreeMap<&str, f64>) = Default::default();
        for (i, v) in g.inputs.iter().enumerate() {
            from.entry(v).or_insert(lay.inputs[i].0.y);
        }
        for (j, v) in g.outputs.iter().enumerate() {
            to.entry(v).or_insert(lay.outputs[j].0.y);
        }
        for b in &g.boxes {
            for (k, v) in b.outputs.iter().enumerate() {
                from.entry(v).or_insert(lay.boxes[&b.id].out_y[k]);
            }
            for (k, v) in b.inputs.iter().enumerate() {
                to.entry(v).or_insert(lay.boxes[&b.id].in_y[k]);
            }
        }
        for (v, a) in &from {
            if let Some(b) = to.get(v) {
                f += (a - b).abs();
            }
        }
        f
    }

    #[test]
    fn the_receipt_is_chyps_own_optimum_and_ours_has_the_same_layers() {
        let r: Value = serde_json::from_str(RECEIPT).expect("the receipt is JSON");
        assert!(r["chyp"]["commit"]
            .as_str()
            .unwrap_or("")
            .starts_with("3d28483"));
        let cases = r["cases"].as_array().expect("cases");
        assert!(cases.len() >= 10);
        for c in cases {
            let name = c["name"].as_str().unwrap_or("");
            let (a, b) = (
                c["objective"].as_f64().expect("objective"),
                c["objective_recomputed"].as_f64().expect("recomputed"),
            );
            assert!(
                (a - b).abs() < 1e-6,
                "{name}: the receipt's objective is recomputable"
            );
            // same layers as layer_decomp for every real box
            let g = graph(&c["graph"]);
            let lay = layout_hypergraph(&g, &chyp_opts());
            for (li, l) in c["layers"].as_array().expect("layers").iter().enumerate() {
                for id in sv(l) {
                    if let Some(bx) = lay.boxes.get(&id) {
                        assert_eq!(bx.layer, li + 1, "{name}: {id}");
                    }
                }
            }
        }
    }

    #[test]
    fn given_chyps_layers_and_order_the_solver_reaches_cvxpys_optimum() {
        let r: Value = serde_json::from_str(RECEIPT).expect("JSON");
        for c in r["cases"].as_array().expect("cases") {
            let name = c["name"].as_str().unwrap_or("");
            // Chyp's graph after layer_decomp, identity boxes included
            let g = graph(&c["expanded"]);
            let layers: Vec<Vec<String>> = c["layers"]
                .as_array()
                .expect("layers")
                .iter()
                .map(sv)
                .collect();
            let o = Opts {
                layer_order: Some(layers.clone()),
                ..chyp_opts()
            };
            let lay = layout_hypergraph(&g, &o);
            assert_eq!(lay.layers, layers, "{name}");
            let ours = chyp_objective(&g, &lay, &layers);
            let theirs = c["objective"].as_f64().expect("objective");
            assert!(
                ours >= theirs - 1e-4,
                "{name}: below Chyp's optimum? {ours} < {theirs}"
            );
            assert!(
                ours <= theirs * 1.005 + 1e-6,
                "{name}: {ours} vs Chyp {theirs}"
            );
            assert!(
                (lay.stats.objective - ours).abs() < 1e-3,
                "{name}: the internal objective is Chyp's"
            );
        }
    }

    #[test]
    fn layouts_are_deterministic_and_keep_the_boundary_order() {
        let g = HGraph {
            boxes: vec![
                HBox {
                    id: "orders".into(),
                    inputs: vec!["orders.in".into()],
                    outputs: vec!["dequeue".into(), "orders.out".into()],
                    width: None,
                    height: None,
                },
                HBox {
                    id: "workers".into(),
                    inputs: vec!["dequeue".into()],
                    outputs: vec!["release".into()],
                    width: None,
                    height: None,
                },
                HBox {
                    id: "payment".into(),
                    inputs: vec!["dequeue".into(), "release".into()],
                    outputs: vec!["completed".into(), "rolled_back".into()],
                    width: None,
                    height: None,
                },
            ],
            inputs: vec!["orders.in".into()],
            outputs: vec![
                "orders.out".into(),
                "completed".into(),
                "rolled_back".into(),
            ],
        };
        let a = layout_hypergraph(&g, &Opts::default());
        let b = layout_hypergraph(&g, &Opts::default());
        assert_eq!(a.stats.objective, b.stats.objective);
        assert_eq!(a.layers, b.layers);
        // the right boundary keeps its order, top to bottom
        let ys: Vec<f64> = a.outputs.iter().map(|p| p.0.y).collect();
        assert!(ys.windows(2).all(|w| w[0] < w[1]), "{ys:?}");
        // the fan-out wire of `dequeue` has two consumers: no single route
        let dq = a
            .vertices
            .iter()
            .find(|v| v.id == "dequeue")
            .expect("dequeue");
        assert!(dq.route.is_none() && dq.paths.len() == 3);
    }

    #[test]
    fn a_cycle_is_drawn_as_a_feedback_wire_below_the_diagram() {
        let g = HGraph {
            boxes: vec![
                HBox {
                    id: "enqueue".into(),
                    inputs: vec!["in".into(), "slots".into()],
                    outputs: vec!["items".into()],
                    width: None,
                    height: None,
                },
                HBox {
                    id: "dequeue".into(),
                    inputs: vec!["items".into()],
                    outputs: vec!["out".into(), "slots".into()],
                    width: None,
                    height: None,
                },
            ],
            inputs: vec!["in".into()],
            outputs: vec!["out".into()],
        };
        let lay = layout_hypergraph(&g, &Opts::default());
        let slots = lay
            .vertices
            .iter()
            .find(|v| v.id == "slots")
            .expect("slots");
        assert_eq!(slots.feedback.len(), 1);
        assert!(
            lay.height
                > lay
                    .boxes
                    .values()
                    .map(|b| b.y + b.h / 2.0)
                    .fold(0.0, f64::max)
        );
    }
}

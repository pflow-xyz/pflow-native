//! Automatic layout: a port of the pflow.xyz editor's client-side layout
//! algorithms (`pflow-xyz/public/petri-view.js`, "layout algorithm
//! implementations", pv:5956–6320 at 06c3c63).
//!
//! Each algorithm is a faithful port: the same node order, the same
//! layering and barycenter heuristics, the same spacing constants and
//! origin, `Math.round` on every coordinate, and the same final shift so no
//! coordinate is below 100 (`_normalizeNodePositions(100)`, pv:5960). For a
//! given net the output equals what pflow.xyz writes into the model, which
//! the tests check against values captured from the JS itself. Like the JS,
//! no grid snap is applied.
//!
//! The String Diagram layout (pv:6381) is not ported here.

use std::collections::{HashMap, HashSet};

use crate::doc::{Document, NodeKind};

/// A layout algorithm from the pflow.xyz "Layout Algorithms" dialog (pv:5837).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// Layered, top to bottom: DFS cycle breaking, longest-path levels,
    /// barycenter crossing minimization (pv:5992).
    Sugiyama,
    /// A near-square grid, nodes sorted by degree (pv:6144).
    Grid,
    /// Places in a left column, transitions in a right one, barycenter
    /// sorted (pv:6197).
    Bipartite,
    /// All nodes on one circle, starting at the top (pv:6281).
    Circular,
}

impl Algorithm {
    pub const ALL: [Algorithm; 4] = [
        Algorithm::Sugiyama,
        Algorithm::Grid,
        Algorithm::Bipartite,
        Algorithm::Circular,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Algorithm::Sugiyama => "sugiyama",
            Algorithm::Grid => "grid",
            Algorithm::Bipartite => "bipartite",
            Algorithm::Circular => "circular",
        }
    }

    /// Parses a name from [`Algorithm::name`], ignoring ASCII case.
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|a| a.name().eq_ignore_ascii_case(s.trim()))
    }
}

/// New positions for every place and transition, as `(id, x, y)`: places
/// first, then transitions, each in the order pflow.xyz iterates them.
/// An empty net gives an empty vector.
pub fn layout(doc: &Document, algo: Algorithm) -> Vec<(String, f64, f64)> {
    let nodes = nodes(doc);
    if nodes.is_empty() {
        return vec![];
    }
    let mut pos = match algo {
        Algorithm::Sugiyama => sugiyama(doc, &nodes),
        Algorithm::Grid => grid(doc, &nodes),
        Algorithm::Bipartite => bipartite(doc, &nodes),
        Algorithm::Circular => circular(&nodes),
    };
    normalize(&mut pos, 100.0);
    nodes
        .iter()
        .zip(pos)
        .map(|(n, (x, y))| (n.id.to_string(), x, y))
        .collect()
}

struct Node<'a> {
    id: &'a str,
    kind: NodeKind,
}

/// Places then transitions, each in JS `Object.keys` order, which is how
/// every algorithm walks `this._model.places` / `.transitions`.
///
/// Ids are unique across places and transitions (`net.add_*` refuses a
/// clash), so indices stand in for the JS id-keyed maps.
fn nodes(doc: &Document) -> Vec<Node<'_>> {
    let places = js_key_order(doc.places.iter().map(|p| p.id.as_str()));
    let transitions = js_key_order(doc.transitions.iter().map(|t| t.id.as_str()));
    places
        .into_iter()
        .map(|id| Node {
            id,
            kind: NodeKind::Place,
        })
        .chain(transitions.into_iter().map(|id| Node {
            id,
            kind: NodeKind::Transition,
        }))
        .collect()
}

/// JS own-property order: array-index keys ("0", "7", "10"; canonical
/// decimal below 2^32 - 1) ascending by value, then the rest in insertion
/// order. The document keeps the file's order, so a net with ids like "2"
/// and "10" would otherwise be walked differently than pflow.xyz walks it.
fn js_key_order<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let index = |id: &str| -> Option<u32> {
        let v: u32 = id.parse().ok()?;
        (v != u32::MAX && v.to_string() == id).then_some(v)
    };
    let mut ints = vec![];
    let mut rest = vec![];
    for id in ids {
        match index(id) {
            Some(v) => ints.push((v, id)),
            None => rest.push(id),
        }
    }
    ints.sort_by_key(|&(v, _)| v);
    ints.into_iter().map(|(_, id)| id).chain(rest).collect()
}

/// `Math.round`: halves round toward +∞ (Rust's `round` goes away from 0).
fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

/// `_normalizeNodePositions(minMargin)` (pv:5960): shift everything so the
/// smallest x and the smallest y are at least `min_margin`.
fn normalize(pos: &mut [(f64, f64)], min_margin: f64) {
    let min_x = pos.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = pos.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let dx = if min_x < min_margin {
        min_margin - min_x
    } else {
        0.0
    };
    let dy = if min_y < min_margin {
        min_margin - min_y
    } else {
        0.0
    };
    if dx > 0.0 || dy > 0.0 {
        for p in pos {
            p.0 += dx;
            p.1 += dy;
        }
    }
}

/// `_applySugiyamaLayout` (pv:5992). Levels run down the y axis.
fn sugiyama(doc: &Document, nodes: &[Node]) -> Vec<(f64, f64)> {
    let n = nodes.len();
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();

    // Adjacency, one entry per arc (parallel arcs repeat), pv:6004.
    let mut outgoing: Vec<Vec<usize>> = vec![vec![]; n];
    let mut incoming: Vec<Vec<usize>> = vec![vec![]; n];
    for arc in &doc.arcs {
        if let (Some(&s), Some(&t)) = (
            index.get(arc.source.as_str()),
            index.get(arc.target.as_str()),
        ) {
            outgoing[s].push(t);
            incoming[t].push(s);
        }
    }

    // Phase 1: DFS cycle breaking (pv:6018). An edge to a node still on the
    // DFS stack is a back edge and is ignored from here on. The JS recurses;
    // this explicit stack visits in the same order.
    let mut visited = vec![false; n];
    let mut on_stack = vec![false; n];
    let mut back: HashSet<(usize, usize)> = HashSet::new();
    for root in 0..n {
        if visited[root] {
            continue;
        }
        visited[root] = true;
        on_stack[root] = true;
        let mut stack = vec![(root, 0usize)];
        while let Some(top) = stack.last_mut() {
            let (u, next) = (top.0, top.1);
            if let Some(&t) = outgoing[u].get(next) {
                top.1 += 1;
                if !visited[t] {
                    visited[t] = true;
                    on_stack[t] = true;
                    stack.push((t, 0));
                } else if on_stack[t] {
                    back.insert((u, t));
                }
            } else {
                on_stack[u] = false;
                stack.pop();
            }
        }
    }

    // Phase 2: longest-path levels over the forward edges (pv:6035).
    let mut level = vec![-1i64; n];
    for v in 0..n {
        if incoming[v].iter().all(|&s| back.contains(&(s, v))) {
            level[v] = 0;
        }
    }
    let mut changed = true;
    while changed {
        changed = false;
        for u in 0..n {
            if level[u] < 0 {
                continue;
            }
            for &t in &outgoing[u] {
                if back.contains(&(u, t)) {
                    continue;
                }
                let new_level = level[u] + 1;
                if new_level > level[t] {
                    level[t] = new_level;
                    changed = true;
                }
            }
        }
    }
    for l in &mut level {
        if *l < 0 {
            *l = 0;
        }
    }

    // Group by level, in node order (pv:6062).
    let max_level = level.iter().copied().max().unwrap_or(0) as usize;
    let mut levels: Vec<Vec<usize>> = vec![vec![]; max_level + 1];
    for v in 0..n {
        levels[level[v] as usize].push(v);
    }

    // Phase 3: barycenter crossing minimization, 4 passes of a down sweep
    // then an up sweep (pv:6071). A node with no neighbor in the adjacent
    // layer keeps its current index as its barycenter. Sorts are stable,
    // as `Array.prototype.sort` is.
    let mut pos_of = vec![0.0f64; n];
    for layer in &levels {
        for (i, &v) in layer.iter().enumerate() {
            pos_of[v] = i as f64;
        }
    }
    let mut bary = vec![0.0f64; n];
    for _pass in 0..4 {
        for (lvl, layer) in levels.iter_mut().enumerate().skip(1) {
            for &v in layer.iter() {
                let (mut sum, mut count) = (0.0, 0);
                for &s in &incoming[v] {
                    if back.contains(&(s, v)) {
                        continue;
                    }
                    if level[s] == lvl as i64 - 1 {
                        sum += pos_of[s];
                        count += 1;
                    }
                }
                bary[v] = if count > 0 {
                    sum / count as f64
                } else {
                    pos_of[v]
                };
            }
            sort_layer(layer, &bary, &mut pos_of);
        }
        for (lvl, layer) in levels.iter_mut().enumerate().take(max_level).rev() {
            for &v in layer.iter() {
                let (mut sum, mut count) = (0.0, 0);
                for &t in &outgoing[v] {
                    if back.contains(&(v, t)) {
                        continue;
                    }
                    if level[t] == lvl as i64 + 1 {
                        sum += pos_of[t];
                        count += 1;
                    }
                }
                bary[v] = if count > 0 {
                    sum / count as f64
                } else {
                    pos_of[v]
                };
            }
            sort_layer(layer, &bary, &mut pos_of);
        }
    }

    // Phase 4: coordinates (pv:6116). Each layer is centred on x = 400;
    // layers are 150 apart from y = 100, nodes 120 apart within a layer.
    const LEVEL_SPACING: f64 = 150.0;
    const NODE_SPACING: f64 = 120.0;
    const START_Y: f64 = 100.0;
    let mut out = vec![(0.0, 0.0); n];
    for (lvl, layer) in levels.iter().enumerate() {
        let total_width = (layer.len() as f64 - 1.0) * NODE_SPACING;
        let start_x = 400.0 - total_width / 2.0;
        for (i, &v) in layer.iter().enumerate() {
            out[v] = (
                js_round(start_x + i as f64 * NODE_SPACING),
                js_round(START_Y + lvl as f64 * LEVEL_SPACING),
            );
        }
    }
    out
}

fn sort_layer(layer: &mut [usize], bary: &[f64], pos_of: &mut [f64]) {
    layer.sort_by(|&a, &b| bary[a].total_cmp(&bary[b]));
    for (i, &v) in layer.iter().enumerate() {
        pos_of[v] = i as f64;
    }
}

/// `_applyGridLayout` (pv:6144).
fn grid(doc: &Document, nodes: &[Node]) -> Vec<(f64, f64)> {
    // Degree counts every arc end, inhibitors and parallel arcs included.
    let mut degree: HashMap<&str, usize> = HashMap::new();
    for arc in &doc.arcs {
        *degree.entry(arc.source.as_str()).or_default() += 1;
        *degree.entry(arc.target.as_str()).or_default() += 1;
    }
    let deg = |v: usize| degree.get(nodes[v].id).copied().unwrap_or(0);

    // Highest degree first; ties keep node order (stable sort), pv:6164.
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by_key(|&v| std::cmp::Reverse(deg(v)));

    // A ceil(sqrt(n))-column grid at 120 spacing, centred on (400, 300).
    const SPACING: f64 = 120.0;
    let total = nodes.len();
    let cols = (total as f64).sqrt().ceil() as usize;
    let rows = total.div_ceil(cols);
    let offset_x = 400.0 - (cols as f64 - 1.0) * SPACING / 2.0;
    let offset_y = 300.0 - (rows as f64 - 1.0) * SPACING / 2.0;

    let mut out = vec![(0.0, 0.0); total];
    for (i, &v) in order.iter().enumerate() {
        let (col, row) = (i % cols, i / cols);
        out[v] = (
            js_round(offset_x + col as f64 * SPACING),
            js_round(offset_y + row as f64 * SPACING),
        );
    }
    out
}

/// `_applyBipartiteLayout` (pv:6197).
fn bipartite(doc: &Document, nodes: &[Node]) -> Vec<(f64, f64)> {
    let index: HashMap<&str, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let is_place = |id: &str| {
        index
            .get(id)
            .is_some_and(|&i| nodes[i].kind == NodeKind::Place)
    };
    let is_transition = |id: &str| {
        index
            .get(id)
            .is_some_and(|&i| nodes[i].kind == NodeKind::Transition)
    };

    // Neighbors regardless of arc direction, one entry per arc (pv:6204).
    let mut neighbors: HashMap<&str, Vec<&str>> = HashMap::new();
    for arc in &doc.arcs {
        let (s, t) = (arc.source.as_str(), arc.target.as_str());
        neighbors.entry(s).or_default().push(t);
        neighbors.entry(t).or_default().push(s);
    }
    let nbrs = |v: usize| neighbors.get(nodes[v].id).map_or(&[][..], Vec::as_slice);

    let mut places: Vec<usize> = (0..nodes.len())
        .filter(|&v| nodes[v].kind == NodeKind::Place)
        .collect();
    let mut transitions: Vec<usize> = (0..nodes.len())
        .filter(|&v| nodes[v].kind == NodeKind::Transition)
        .collect();
    let mut pos_of = vec![0.0f64; nodes.len()];
    for (i, &v) in places.iter().enumerate() {
        pos_of[v] = i as f64;
    }
    for (i, &v) in transitions.iter().enumerate() {
        pos_of[v] = i as f64;
    }

    // 4 rounds: transitions by the mean index of their neighboring places,
    // then places by the mean index of their neighboring transitions
    // (pv:6222).
    let mut bary = vec![0.0f64; nodes.len()];
    for _iter in 0..4 {
        for &v in &transitions {
            let (mut sum, mut count) = (0.0, 0);
            for nb in nbrs(v) {
                if is_place(nb) {
                    sum += pos_of[index[nb]];
                    count += 1;
                }
            }
            bary[v] = if count > 0 {
                sum / count as f64
            } else {
                pos_of[v]
            };
        }
        sort_layer(&mut transitions, &bary, &mut pos_of);

        for &v in &places {
            let (mut sum, mut count) = (0.0, 0);
            for nb in nbrs(v) {
                if is_transition(nb) {
                    sum += pos_of[index[nb]];
                    count += 1;
                }
            }
            bary[v] = if count > 0 {
                sum / count as f64
            } else {
                pos_of[v]
            };
        }
        sort_layer(&mut places, &bary, &mut pos_of);
    }

    // Columns at x = 250 and 550, each centred on y = 300, 120 apart
    // (pv:6255).
    const COL_GAP: f64 = 300.0;
    const V_SPACING: f64 = 120.0;
    const LEFT_X: f64 = 250.0;
    const RIGHT_X: f64 = LEFT_X + COL_GAP;
    const CENTER_Y: f64 = 300.0;
    let mut out = vec![(0.0, 0.0); nodes.len()];
    for (column, x) in [(&places, LEFT_X), (&transitions, RIGHT_X)] {
        let start_y = CENTER_Y - (column.len() as f64 - 1.0) * V_SPACING / 2.0;
        for (i, &v) in column.iter().enumerate() {
            out[v] = (x, js_round(start_y + i as f64 * V_SPACING));
        }
    }
    out
}

/// `_applyCircularLayout` (pv:6281): node order around a circle centred on
/// (500, 400), radius min(300, 50 + 15n), the first node at the top and
/// the rest clockwise (y grows downward).
fn circular(nodes: &[Node]) -> Vec<(f64, f64)> {
    const CENTER_X: f64 = 500.0;
    const CENTER_Y: f64 = 400.0;
    let n = nodes.len() as f64;
    let radius = (50.0 + n * 15.0).min(300.0);
    (0..nodes.len())
        .map(|i| {
            let angle = (2.0 * std::f64::consts::PI * i as f64) / n - std::f64::consts::PI / 2.0;
            (
                js_round(CENTER_X + radius * angle.cos()),
                js_round(CENTER_Y + radius * angle.sin()),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const COFFEE: &[u8] = include_bytes!("../../../examples/coffee-shop.json");
    const SIR: &[u8] = include_bytes!("../../../examples/sir.json");
    /// A 5-cycle (b→t→10→u→b), a 2-cycle (v⇄z) hanging off it, an isolated
    /// place, and array-index ids ("10", "2") that JS walks first.
    const MIXED: &[u8] = br#"{"token":[],
      "places":{"b":{"initial":[1]},"10":{},"2":{},"a":{},"z":{}},
      "transitions":{"t":{},"u":{},"v":{}},
      "arcs":[{"source":"b","target":"t"},{"source":"t","target":"10"},
              {"source":"10","target":"u"},{"source":"u","target":"b"},
              {"source":"a","target":"v"},{"source":"v","target":"z"},
              {"source":"z","target":"v"},{"source":"u","target":"a"}]}"#;

    fn doc(bytes: &[u8]) -> Document {
        Document::from_json(bytes).unwrap()
    }

    fn at(out: &[(String, f64, f64)], id: &str) -> (f64, f64) {
        out.iter()
            .find(|(i, _, _)| i == id)
            .map(|&(_, x, y)| (x, y))
            .unwrap_or_else(|| panic!("no position for {id}"))
    }

    /// Captured by running pv:5956–6320 under node on the same files.
    fn golden(net: &str, algo: Algorithm) -> &'static [(&'static str, f64, f64)] {
        use Algorithm::*;
        match (net, algo) {
            ("sir", Sugiyama) => &[
                ("S", 400.0, 100.0),
                ("I", 400.0, 400.0),
                ("R", 400.0, 700.0),
                ("infect", 400.0, 250.0),
                ("recover", 400.0, 550.0),
            ],
            ("sir", Grid) => &[
                ("S", 280.0, 360.0),
                ("I", 280.0, 240.0),
                ("R", 400.0, 360.0),
                ("infect", 400.0, 240.0),
                ("recover", 520.0, 240.0),
            ],
            ("sir", Bipartite) => &[
                ("S", 250.0, 180.0),
                ("I", 250.0, 300.0),
                ("R", 250.0, 420.0),
                ("infect", 550.0, 240.0),
                ("recover", 550.0, 360.0),
            ],
            ("sir", Circular) => &[
                ("S", 500.0, 275.0),
                ("I", 619.0, 361.0),
                ("R", 573.0, 501.0),
                ("infect", 427.0, 501.0),
                ("recover", 381.0, 361.0),
            ],
            ("coffee", Sugiyama) => &[
                ("CoffeeBeans", 400.0, 250.0),
                ("EmptyCups", 520.0, 250.0),
                ("ServedCoffee", 400.0, 550.0),
                ("WaitingCustomers", 280.0, 250.0),
                ("BrewCoffee", 400.0, 400.0),
                ("CustomerArrives", 280.0, 100.0),
                ("CustomerLeaves", 400.0, 700.0),
                ("RestockBeans", 400.0, 100.0),
                ("RestockCups", 520.0, 100.0),
            ],
            ("coffee", Grid) => &[
                ("CoffeeBeans", 400.0, 180.0),
                ("EmptyCups", 520.0, 180.0),
                ("ServedCoffee", 280.0, 300.0),
                ("WaitingCustomers", 400.0, 300.0),
                ("BrewCoffee", 280.0, 180.0),
                ("CustomerArrives", 520.0, 300.0),
                ("CustomerLeaves", 280.0, 420.0),
                ("RestockBeans", 400.0, 420.0),
                ("RestockCups", 520.0, 420.0),
            ],
            ("coffee", Bipartite) => &[
                ("CoffeeBeans", 250.0, 160.0),
                ("EmptyCups", 250.0, 280.0),
                ("ServedCoffee", 250.0, 400.0),
                ("WaitingCustomers", 250.0, 520.0),
                ("BrewCoffee", 550.0, 340.0),
                ("CustomerArrives", 550.0, 580.0),
                ("CustomerLeaves", 550.0, 460.0),
                ("RestockBeans", 550.0, 100.0),
                ("RestockCups", 550.0, 220.0),
            ],
            ("coffee", Circular) => &[
                ("CoffeeBeans", 500.0, 215.0),
                ("EmptyCups", 619.0, 258.0),
                ("ServedCoffee", 682.0, 368.0),
                ("WaitingCustomers", 660.0, 493.0),
                ("BrewCoffee", 563.0, 574.0),
                ("CustomerArrives", 437.0, 574.0),
                ("CustomerLeaves", 340.0, 493.0),
                ("RestockBeans", 318.0, 368.0),
                ("RestockCups", 381.0, 258.0),
            ],
            ("mixed", Sugiyama) => &[
                ("2", 340.0, 100.0),
                ("10", 460.0, 100.0),
                ("b", 340.0, 400.0),
                ("a", 460.0, 400.0),
                ("z", 400.0, 700.0),
                ("t", 340.0, 550.0),
                ("u", 400.0, 250.0),
                ("v", 460.0, 550.0),
            ],
            ("mixed", Grid) => &[
                ("2", 400.0, 420.0),
                ("10", 520.0, 180.0),
                ("b", 280.0, 300.0),
                ("a", 400.0, 300.0),
                ("z", 520.0, 300.0),
                ("t", 280.0, 420.0),
                ("u", 280.0, 180.0),
                ("v", 400.0, 180.0),
            ],
            ("mixed", Bipartite) => &[
                ("2", 250.0, 100.0),
                ("10", 250.0, 220.0),
                ("b", 250.0, 340.0),
                ("a", 250.0, 460.0),
                ("z", 250.0, 580.0),
                ("t", 550.0, 220.0),
                ("u", 550.0, 340.0),
                ("v", 550.0, 460.0),
            ],
            ("mixed", Circular) => &[
                ("2", 500.0, 230.0),
                ("10", 620.0, 280.0),
                ("b", 670.0, 400.0),
                ("a", 620.0, 520.0),
                ("z", 500.0, 570.0),
                ("t", 380.0, 520.0),
                ("u", 330.0, 400.0),
                ("v", 380.0, 280.0),
            ],
            _ => unreachable!(),
        }
    }

    const NETS: [(&str, &[u8]); 3] = [("sir", SIR), ("coffee", COFFEE), ("mixed", MIXED)];

    #[test]
    fn matches_pflow_xyz_exactly() {
        for (name, bytes) in NETS {
            let d = doc(bytes);
            for algo in Algorithm::ALL {
                let got = layout(&d, algo);
                let want: Vec<(String, f64, f64)> = golden(name, algo)
                    .iter()
                    .map(|&(id, x, y)| (id.to_string(), x, y))
                    .collect();
                assert_eq!(got, want, "{name} {}", algo.name());
            }
        }
    }

    #[test]
    fn every_node_placed_distinct_and_deterministic() {
        for (name, bytes) in NETS {
            let d = doc(bytes);
            for algo in Algorithm::ALL {
                let out = layout(&d, algo);
                assert_eq!(out.len(), d.places.len() + d.transitions.len());
                for id in d
                    .places
                    .iter()
                    .map(|p| &p.id)
                    .chain(d.transitions.iter().map(|t| &t.id))
                {
                    let (x, y) = at(&out, id);
                    assert!(x >= 100.0 && y >= 100.0, "{name} {algo:?} {id} margin");
                }
                let mut seen = HashSet::new();
                for (id, x, y) in &out {
                    assert!(
                        seen.insert((*x as i64, *y as i64)),
                        "{name} {algo:?}: {id} overlaps another node at ({x}, {y})"
                    );
                }
                assert_eq!(out, layout(&d, algo), "{name} {algo:?} not deterministic");
            }
        }
    }

    #[test]
    fn sugiyama_flows_down_the_sir_chain() {
        let out = layout(&doc(SIR), Algorithm::Sugiyama);
        let y: Vec<f64> = ["S", "infect", "I", "recover", "R"]
            .iter()
            .map(|id| at(&out, id).1)
            .collect();
        assert!(y.windows(2).all(|w| w[0] < w[1]), "{y:?}");
    }

    #[test]
    fn empty_net_and_names() {
        let empty = Document::default();
        for algo in Algorithm::ALL {
            assert!(layout(&empty, algo).is_empty());
            assert_eq!(Algorithm::parse(algo.name()), Some(algo));
        }
        assert_eq!(Algorithm::parse(" Sugiyama "), Some(Algorithm::Sugiyama));
        assert_eq!(Algorithm::parse("force"), None);
    }

    #[test]
    fn single_node_and_self_loop() {
        let d = doc(br#"{"places":{"p":{}},"transitions":{"t":{}},
            "arcs":[{"source":"p","target":"t"},{"source":"t","target":"p"}]}"#);
        let out = layout(&d, Algorithm::Sugiyama);
        assert_eq!(at(&out, "p"), (400.0, 100.0));
        assert_eq!(at(&out, "t"), (400.0, 250.0));
        let one = doc(br#"{"places":{"p":{}},"transitions":{},"arcs":[]}"#);
        for algo in Algorithm::ALL {
            assert_eq!(layout(&one, algo).len(), 1);
        }
    }

    #[test]
    fn js_round_and_key_order() {
        assert_eq!(js_round(2.5), 3.0);
        assert_eq!(js_round(-2.5), -2.0);
        assert_eq!(js_round(0.49999999999999994), 0.0);
        let ids = ["b", "10", "2", "01", "a", "4294967295"];
        assert_eq!(
            js_key_order(ids.into_iter()),
            ["2", "10", "b", "01", "a", "4294967295"]
        );
    }
}

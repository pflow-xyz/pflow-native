//! The flat view's layout (a port of `layoutOrders` in the prototype's
//! `js/views/flat.js`): every flat element sits at its member's home inside
//! its cluster, a fused one in the gap between the cluster rows, and each
//! place's label takes the free one of eight positions. A pure function of the
//! flat net and the subnets, in the prototype's design coordinates.

// The loops index `nodes` while reading other entries of it, which an
// iterator over `nodes` cannot do.
#![allow(clippy::needless_range_loop)]

use super::*;

pub const PR: f32 = 16.0;
const U: f32 = 150.0;
const ROW_C: f32 = 0.0;
const ROW_W: f32 = 70.0;
const ROW_M: f32 = 174.0;
const ROW_F: f32 = 278.0;
const SAGA_H: f32 = ROW_F;
const BAND_GAP: f32 = 100.0;
const PAD: f32 = 12.0;
const HEAD: f32 = 20.0;
pub const FS_PRE: f32 = 8.5;
pub const FS_LOC: f32 = 10.5;
pub const FS_FUSED: f32 = 10.0;

/// Width of monospace text of `size` (labels are flat ids set in the mono face).
pub fn mw(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * 0.61
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Lab {
    Below,
    Above,
    Right,
    Left,
    Bl,
    Br,
    Al,
    Ar,
    None,
}

const LABEL_POS: [Lab; 8] = [
    Lab::Below,
    Lab::Above,
    Lab::Right,
    Lab::Left,
    Lab::Bl,
    Lab::Br,
    Lab::Al,
    Lab::Ar,
];

#[derive(Clone, Copy, Debug)]
pub struct B {
    pub x0: f32,
    pub x1: f32,
    pub y0: f32,
    pub y1: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct R {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug)]
pub struct Node {
    pub id: String,
    pub place: bool,
    pub data_place: bool,
    pub fused: bool,
    pub members: Vec<String>,
    pub lines: Vec<String>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub r: f32,
    pub lab: Lab,
    pub cluster: Option<usize>,
}

pub struct Cluster {
    pub sid: String,
    pub pattern: String,
    pub extra: bool,
    pub rect: R,
}

pub struct Lay {
    pub nodes: Vec<Node>,
    pub idx: BTreeMap<String, usize>,
    pub clusters: Vec<Cluster>,
    pub home: BTreeMap<String, (f32, f32)>,
    pub legend: R,
    pub w: f32,
    pub h: f32,
}

impl Lay {
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.idx.get(id).map(|i| &self.nodes[*i])
    }
}

#[derive(Clone, Copy)]
struct Home {
    x: f32,
    y: f32,
    lab: Lab,
}

fn split_id(id: &str) -> (String, String) {
    match id.find('/') {
        None => (String::new(), id.to_string()),
        Some(i) => (id[..=i].to_string(), id[i + 1..].to_string()),
    }
}

/// Label lines of a flat element: its go-pflow flat id, broken where it reads best.
pub fn lines_of(id: &str) -> Vec<String> {
    if let Some(rest) = id.strip_prefix("fused:") {
        return rest
            .split('+')
            .enumerate()
            .map(|(i, p)| {
                if i == 0 {
                    format!("fused:{p}")
                } else {
                    format!("+{p}")
                }
            })
            .collect();
    }
    if let Some(rest) = id.strip_prefix("wire:") {
        let (pre, loc) = split_id(rest);
        return vec![format!("wire:{pre}"), loc];
    }
    let (pre, loc) = split_id(id);
    vec![pre, loc]
}

fn geom(place: bool, lines: &[String], fused: bool) -> (f32, f32, f32) {
    // (w, h, r)
    if !place {
        if fused {
            let w = lines
                .iter()
                .map(|l| mw(l, FS_FUSED) * 1.04)
                .fold(0.0, f32::max)
                + 26.0;
            return (w, 13.0 * lines.len() as f32 + 13.0, 0.0);
        }
        let w = 46f32.max(mw(&lines[0], FS_PRE)).max(mw(&lines[1], FS_LOC)) + 16.0;
        return (w, 30.0, 0.0);
    }
    let r = if fused { PR + 3.0 } else { PR };
    (0.0, 0.0, r)
}

/// The label block of a place (two lines: the subnet prefix, then the rest of
/// the flat id), as a box, for one of eight positions around the circle.
pub fn label_box(n: &Node, lab: Lab) -> (B, Anchor, f32) {
    let w = mw(&n.lines[0], FS_PRE).max(mw(&n.lines[1], FS_LOC));
    let (r, h) = (n.r, 26.0);
    let below = n.y + r + 2.0;
    let above = n.y - r - 2.0 - h;
    let bx = |x0: f32, y0: f32, a: Anchor, tx: f32| {
        (
            B {
                x0,
                x1: x0 + w,
                y0,
                y1: y0 + h,
            },
            a,
            tx,
        )
    };
    match lab {
        Lab::Above => bx(n.x - w / 2.0, above, Anchor::Middle, n.x),
        Lab::Right => bx(n.x + r + 5.0, n.y - h / 2.0, Anchor::Start, n.x + r + 5.0),
        Lab::Left => bx(n.x - r - 5.0 - w, n.y - h / 2.0, Anchor::End, n.x - r - 5.0),
        Lab::Bl => bx(n.x + 6.0 - w, below, Anchor::End, n.x + 6.0),
        Lab::Br => bx(n.x - 6.0, below, Anchor::Start, n.x - 6.0),
        Lab::Al => bx(n.x + 6.0 - w, above, Anchor::End, n.x + 6.0),
        Lab::Ar => bx(n.x - 6.0, above, Anchor::Start, n.x - 6.0),
        _ => bx(n.x - w / 2.0, below, Anchor::Middle, n.x),
    }
}

fn overlaps(a: &B, b: &B) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

fn inside(a: &B, r: &R) -> bool {
    a.x0 >= r.x && a.x1 <= r.x + r.w && a.y0 >= r.y && a.y1 <= r.y + r.h
}

/// Does segment (x1,y1)-(x2,y2) cross box `b` (Liang-Barsky)?
pub fn seg_hits(x1: f32, y1: f32, x2: f32, y2: f32, b: &B) -> bool {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    let (dx, dy) = (x2 - x1, y2 - y1);
    for (p, q) in [
        (-dx, x1 - b.x0),
        (dx, b.x1 - x1),
        (-dy, y1 - b.y0),
        (dy, b.y1 - y1),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
            continue;
        }
        let t = q / p;
        if p < 0.0 {
            if t > t1 {
                return false;
            }
            if t > t0 {
                t0 = t;
            }
        } else {
            if t < t0 {
                return false;
            }
            if t < t1 {
                t1 = t;
            }
        }
    }
    true
}

pub fn box_of(n: &Node, pad: f32) -> B {
    if n.place {
        B {
            x0: n.x - n.r - pad,
            x1: n.x + n.r + pad,
            y0: n.y - n.r - pad,
            y1: n.y + n.r + pad,
        }
    } else {
        B {
            x0: n.x - n.w / 2.0 - pad,
            x1: n.x + n.w / 2.0 + pad,
            y0: n.y - n.h / 2.0 - pad,
            y1: n.y + n.h / 2.0 + pad,
        }
    }
}

pub fn extent_of(n: &Node) -> B {
    if !n.place {
        return box_of(n, 0.0);
    }
    let (lb, _, _) = label_box(n, n.lab);
    B {
        x0: (n.x - n.r).min(lb.x0),
        x1: (n.x + n.r).max(lb.x1),
        y0: (n.y - n.r).min(lb.y0),
        y1: (n.y + n.r).max(lb.y1),
    }
}

/// Picks each place's label position: the pattern layout's preference unless an
/// arc, a node or another label is in the way. Deterministic (model order).
fn place_labels(
    nodes: &mut [Node],
    idx: &BTreeMap<String, usize>,
    arcs: &[(String, String)],
    clusters: &[Cluster],
) {
    let segs: Vec<(f32, f32, f32, f32)> = arcs
        .iter()
        .filter_map(|(f, t)| {
            let (a, b) = (&nodes[*idx.get(f)?], &nodes[*idx.get(t)?]);
            Some((a.x, a.y, b.x, b.y))
        })
        .collect();
    let shapes: Vec<(usize, B)> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (i, box_of(n, 0.0)))
        .collect();
    let mut taken: Vec<B> = vec![];
    for i in 0..nodes.len() {
        if !nodes[i].place {
            continue;
        }
        let mut prefs: Vec<Lab> = vec![if nodes[i].lab == Lab::None {
            Lab::Below
        } else {
            nodes[i].lab
        }];
        for l in LABEL_POS {
            if !prefs.contains(&l) {
                prefs.push(l);
            }
        }
        let mut best: Option<(Lab, B, f32)> = None;
        for (k, lab) in prefs.iter().enumerate() {
            let (b0, _, _) = label_box(&nodes[i], *lab);
            let b = B {
                x0: b0.x0 + 1.0,
                x1: b0.x1 - 1.0,
                y0: b0.y0 + 2.0,
                y1: b0.y1 - 2.0,
            };
            let mut score = k as f32;
            for (x1, y1, x2, y2) in &segs {
                if seg_hits(*x1, *y1, *x2, *y2, &b) {
                    score += 12.0;
                }
            }
            let grown = B {
                x0: b.x0 - 4.0,
                x1: b.x1 + 4.0,
                y0: b.y0 - 5.0,
                y1: b.y1 + 5.0,
            };
            for (j, sh) in &shapes {
                if *j == i {
                    continue;
                }
                if overlaps(sh, &b) {
                    score += 100.0;
                } else if overlaps(sh, &grown) {
                    score += 6.0;
                }
            }
            for t in &taken {
                if overlaps(t, &b) {
                    score += 60.0;
                } else if overlaps(t, &grown) {
                    score += 14.0;
                }
            }
            if let Some(ci) = nodes[i].cluster {
                if !inside(&b0, &clusters[ci].rect) {
                    score += 20.0;
                }
            }
            if best.as_ref().is_none_or(|bb| score < bb.2) {
                best = Some((*lab, b, score));
            }
        }
        let (lab, b, _) = best.expect("at least one position");
        nodes[i].lab = lab;
        taken.push(b);
    }
}

/// An arc drawn straight through a node it does not touch reads as that
/// node's arc. Each fused element tries a fixed list of spots in its gap and
/// keeps the one where the fewest arcs cross other nodes.
fn refine_fused(
    nodes: &mut [Node],
    idx: &BTreeMap<String, usize>,
    arcs: &[(String, String)],
    gaps: &[(f32, f32)],
    per_gap: &[Vec<usize>],
) {
    let pairs: Vec<(usize, usize)> = arcs
        .iter()
        .filter_map(|(f, t)| Some((*idx.get(f)?, *idx.get(t)?)))
        .collect();
    let cost = |nodes: &[Node], n: usize| -> f32 {
        let mut hits = 0;
        let through = |a: usize, b: usize, m: usize| {
            m != a
                && m != b
                && seg_hits(
                    nodes[a].x,
                    nodes[a].y,
                    nodes[b].x,
                    nodes[b].y,
                    &box_of(&nodes[m], -2.0),
                )
        };
        for &(a, b) in &pairs {
            if a == n || b == n {
                for m in 0..nodes.len() {
                    if through(a, b, m) {
                        hits += 1;
                    }
                }
            } else if through(a, b, n) {
                hits += 1;
            }
        }
        let me = extent_of(&nodes[n]);
        let grown = B {
            x0: me.x0 - 10.0,
            x1: me.x1 + 10.0,
            y0: me.y0 - 4.0,
            y1: me.y1 + 4.0,
        };
        let mut clash = 0;
        for m in 0..nodes.len() {
            if m != n && overlaps(&grown, &extent_of(&nodes[m])) {
                clash += 1;
            }
        }
        hits as f32 * 100.0 + clash as f32 * 1000.0
    };
    const DX: [f32; 11] = [
        0.0, 36.0, -36.0, 72.0, -72.0, 110.0, -110.0, 150.0, -150.0, 200.0, -200.0,
    ];
    const DY: [f32; 3] = [0.0, -22.0, 22.0];
    for (gi, g) in gaps.iter().enumerate() {
        let mut order = per_gap[gi].clone();
        order.sort_by(|a, b| nodes[*a].id.cmp(&nodes[*b].id));
        for n in order {
            let (x0, y0) = (nodes[n].x, nodes[n].y);
            let (half, half_w) = if nodes[n].place {
                (nodes[n].r, nodes[n].r)
            } else {
                (nodes[n].h / 2.0, nodes[n].w / 2.0)
            };
            let mut best = (cost(nodes, n), x0, y0);
            for dy in DY {
                if dy != 0.0 && (y0 + dy - half < g.0 + 4.0 || y0 + dy + half > g.1 - 4.0) {
                    continue;
                }
                for dx in DX {
                    if (dx == 0.0 && dy == 0.0) || x0 + dx - half_w < 8.0 {
                        continue;
                    }
                    nodes[n].x = x0 + dx;
                    nodes[n].y = y0 + dy;
                    let c = cost(nodes, n) + dx.abs() * 0.05 + dy.abs() * 0.1;
                    if c < best.0 {
                        best = (c, nodes[n].x, nodes[n].y);
                    }
                }
            }
            nodes[n].x = best.1;
            nodes[n].y = best.2;
        }
    }
}

// --- pattern layouts: each element's home inside its cluster (local coordinates)

fn saga_homes(params: &Value, arcs: &[Value], flip: bool) -> BTreeMap<String, Home> {
    let steps: Vec<String> = arr(&params["steps"])
        .iter()
        .map(|st| s(&st["id"]).to_string())
        .collect();
    let mut accounts: Vec<String> = params["accounts"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    accounts.sort();
    let n = steps.len();
    let c = |k: usize| k as f32 * U;
    let mut h: BTreeMap<String, Home> = BTreeMap::new();
    let mut put = |id: String, x: f32, y: f32, lab: Lab| {
        let lab = if flip {
            match lab {
                Lab::Above => Lab::Below,
                Lab::Below => Lab::Above,
                o => o,
            }
        } else {
            lab
        };
        h.insert(
            id,
            Home {
                x,
                y: if flip { SAGA_H - y } else { y },
                lab,
            },
        );
    };
    let q_up = ROW_W + 0.6 * (ROW_M - ROW_W);
    let q_down = ROW_M + 0.4 * (ROW_F - ROW_M);
    // Which step moves an account, and whether do_<step> takes from it (debit)
    // or pays it (credit).
    let mut moved: BTreeMap<String, (String, bool)> = BTreeMap::new();
    for a in arcs {
        let (from, to) = (s(&a["from"]), s(&a["to"]));
        let step = to
            .strip_prefix("do_")
            .or_else(|| from.strip_prefix("do_"))
            .map(str::to_string);
        let Some(step) = step else { continue };
        let acct = if to.starts_with("do_") { from } else { to };
        if !accounts.iter().any(|x| x == acct) || moved.contains_key(acct) {
            continue;
        }
        moved.insert(acct.to_string(), (step, to.starts_with("do_")));
    }
    for (i, st) in steps.iter().enumerate() {
        let moves_tokens = moved.values().any(|v| v.0 == *st);
        put(format!("next_{st}"), c(2 * i), ROW_F, Lab::Below);
        put(format!("do_{st}"), c(2 * i + 1), ROW_F, Lab::Below);
        put(format!("fail_{st}"), c(2 * i), ROW_M, Lab::Below);
        put(format!("undo_{st}"), c(2 * i + 1), ROW_W, Lab::Below);
        put(format!("unwind_{st}"), c(2 * i + 2), ROW_W, Lab::Above);
        put(
            format!("{st}_done"),
            c(2 * i + 1),
            ROW_M,
            if moves_tokens { Lab::Left } else { Lab::Right },
        );
        put(
            format!("{st}_pending"),
            c(2 * i) + U / 2.0,
            q_down,
            Lab::Below,
        );
        put(format!("{st}_failed"), c(2 * i) + U / 2.0, q_up, Lab::Above);
        put(format!("{st}_compensated"), c(2 * i + 1), ROW_C, Lab::Right);
    }
    put("committing".into(), c(2 * n), ROW_F, Lab::Below);
    put("commit".into(), c(2 * n + 1), ROW_F, Lab::Below);
    put("completed".into(), c(2 * n + 2), ROW_F, Lab::Below);
    put("fail_commit".into(), c(2 * n), ROW_M, Lab::Below);
    put("rolled_back".into(), c(0), ROW_W, Lab::Above);
    let mut spare = 0.0;
    for acct in &accounts {
        if let Some((step, debit)) = moved.get(acct) {
            let i = steps.iter().position(|x| x == step).unwrap_or(0);
            put(
                acct.clone(),
                c(2 * i + 1) + U * 0.6,
                if *debit { ROW_M + 60.0 } else { ROW_M - 60.0 },
                Lab::Right,
            );
        } else {
            put(
                acct.clone(),
                c(2 * n + 2),
                ROW_M - 50.0 + 64.0 * spare,
                Lab::Right,
            );
            spare += 1.0;
        }
    }
    h
}

fn queue_homes() -> BTreeMap<String, Home> {
    let mut h = BTreeMap::new();
    let mut put = |id: &str, x: f32, y: f32, lab: Lab| {
        h.insert(id.to_string(), Home { x, y, lab });
    };
    put("enqueue", 0.0, 38.0, Lab::Below);
    put("items", 100.0, 0.0, Lab::Above);
    put("dequeue", 200.0, 38.0, Lab::Below);
    put("slots", 100.0, 76.0, Lab::Below);
    h
}

fn pool_homes(w: f32) -> BTreeMap<String, Home> {
    let mut h = BTreeMap::new();
    let mut put = |id: &str, x: f32, y: f32, lab: Lab| {
        h.insert(id.to_string(), Home { x, y, lab });
    };
    put("acquire", 0.0, 38.0, Lab::Below);
    put("available", w / 2.0, 0.0, Lab::Above);
    put("in_use", w / 2.0, 76.0, Lab::Below);
    put("release", w, 38.0, Lab::Below);
    h
}

fn grid_homes(sub: &Value) -> BTreeMap<String, Home> {
    let mut h = BTreeMap::new();
    for (i, p) in arr(&sub["places"]).iter().enumerate() {
        h.insert(
            s(&p["id"]).to_string(),
            Home {
                x: i as f32 * 120.0,
                y: 0.0,
                lab: Lab::Below,
            },
        );
    }
    for (i, t) in arr(&sub["transitions"]).iter().enumerate() {
        h.insert(
            s(&t["id"]).to_string(),
            Home {
                x: i as f32 * 120.0,
                y: 110.0,
                lab: Lab::Below,
            },
        );
    }
    h
}

fn is_place(sub: &Value, local: &str) -> bool {
    arr(&sub["places"]).iter().any(|p| s(&p["id"]) == local)
}

/// Positions for every flat element. A flat element with one member sits at
/// that member's home inside its cluster; a fused one sits in the gap between
/// the cluster rows, at the mean x of its members' homes.
pub fn layout(subnets: &[Value], flat: &Value) -> Lay {
    let by_pattern =
        |p: &str| -> Vec<&Value> { subnets.iter().filter(|x| s(&x["pattern"]) == p).collect() };
    let sagas = by_pattern("saga");
    let queues = by_pattern("queue");
    let pools = by_pattern("resource-pool");
    let others: Vec<&Value> = subnets
        .iter()
        .filter(|x| !["saga", "queue", "resource-pool"].contains(&s(&x["pattern"])))
        .collect();

    // (subnet, homes, rect), homes absolute once placed
    struct Cl {
        sid: String,
        pattern: String,
        extra: bool,
        homes: BTreeMap<String, Home>,
        rect: R,
    }
    let member_node = |sub: &Value, local: &str, home: &Home| -> Node {
        let place = is_place(sub, local);
        let lines = vec![format!("{}/", s(&sub["id"])), local.to_string()];
        let (w, h, r) = geom(place, &lines, false);
        Node {
            id: String::new(),
            place,
            data_place: false,
            fused: false,
            members: vec![],
            lines,
            x: home.x,
            y: home.y,
            w,
            h,
            r,
            lab: home.lab,
            cluster: None,
        }
    };
    let local_extent = |sub: &Value, homes: &BTreeMap<String, Home>| -> B {
        let mut b = B {
            x0: f32::MAX,
            x1: f32::MIN,
            y0: f32::MAX,
            y1: f32::MIN,
        };
        for (local, home) in homes {
            let e = extent_of(&member_node(sub, local, home));
            b = B {
                x0: b.x0.min(e.x0),
                x1: b.x1.max(e.x1),
                y0: b.y0.min(e.y0),
                y1: b.y1.max(e.y1),
            };
        }
        b
    };
    let mut clusters: Vec<Cl> = vec![];
    let place_cluster = |clusters: &mut Vec<Cl>,
                         sub: &Value,
                         homes: BTreeMap<String, Home>,
                         left: f32,
                         top: f32|
     -> R {
        let e = local_extent(sub, &homes);
        let (ox, oy) = (left + PAD - e.x0, top + PAD + HEAD - e.y0);
        let abs: BTreeMap<String, Home> = homes
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    Home {
                        x: v.x + ox,
                        y: v.y + oy,
                        lab: v.lab,
                    },
                )
            })
            .collect();
        let rect = R {
            x: left,
            y: top,
            w: e.x1 - e.x0 + 2.0 * PAD,
            h: e.y1 - e.y0 + 2.0 * PAD + HEAD,
        };
        clusters.push(Cl {
            sid: s(&sub["id"]).to_string(),
            pattern: s(&sub["pattern"]).to_string(),
            extra: sub["extra"] == true,
            homes: abs,
            rect,
        });
        rect
    };

    const LEFT: f32 = 24.0;
    let mut top = 14.0;
    let mut bands: Vec<(f32, f32)> = vec![];
    // Band 0: the first saga, forward track at the bottom (facing the queue and pool).
    let mut commit_x: Option<f32> = None;
    let mut saga_left = LEFT;
    if let Some(sg) = sagas.first() {
        let homes = saga_homes(&sg["params"], arr(&sg["arcs"]), false);
        let r = place_cluster(&mut clusters, sg, homes, LEFT, top);
        bands.push((r.y, r.y + r.h));
        commit_x = clusters.last().unwrap().homes.get("commit").map(|h| h.x);
        saga_left = r.x;
        top = r.y + r.h + BAND_GAP;
    }
    // Band 1: queues, then pools, then anything else, left to right.
    let mut x = saga_left;
    let mut row: Vec<(char, &Value)> = vec![];
    row.extend(queues.iter().map(|v| ('q', *v)));
    row.extend(pools.iter().map(|v| ('p', *v)));
    row.extend(others.iter().map(|v| ('o', *v)));
    let first_band1 = clusters.len();
    let n_row = row.len();
    for (i, (k, sub)) in row.iter().enumerate() {
        let homes = match k {
            'q' => queue_homes(),
            'p' => {
                let mut w = 230.0f32;
                if pools.len() == 1 {
                    if let Some(cx) = commit_x {
                        let acc_half = geom(
                            false,
                            &[format!("{}/", s(&sub["id"])), "acquire".into()],
                            false,
                        )
                        .0 / 2.0;
                        w = w.max(560f32.min(cx - 26.0 - (x + PAD + acc_half)));
                    }
                }
                pool_homes(w)
            }
            _ => grid_homes(sub),
        };
        let r = place_cluster(&mut clusters, sub, homes, x, top);
        x = r.x + r.w + if i + 1 < n_row { 44.0 } else { 0.0 };
    }
    let band1_right = x;
    let mut top2 = top;
    if clusters.len() > first_band1 {
        // Make the band-1 clusters one height, so the row reads as a row.
        let b1h = clusters[first_band1..]
            .iter()
            .map(|c| c.rect.h)
            .fold(0.0, f32::max);
        for c in &mut clusters[first_band1..] {
            let dy = (b1h - c.rect.h) / 2.0;
            c.rect.h = b1h;
            for h in c.homes.values_mut() {
                h.y += dy;
            }
        }
        bands.push((top, top + b1h));
        top2 = top + b1h;
    } else if !bands.is_empty() {
        bands.push((top, top));
    }
    // Band 2...: further sagas, mirrored so their forward track faces band 1.
    for sg in sagas.iter().skip(1) {
        top2 += BAND_GAP;
        let homes = saga_homes(&sg["params"], arr(&sg["arcs"]), true);
        let r = place_cluster(&mut clusters, sg, homes, saga_left, top2);
        bands.push((r.y, r.y + r.h));
        top2 = r.y + r.h;
    }
    if bands.is_empty() {
        bands.push((0.0, 0.0));
    }

    // Homes, absolute, by qualified member id.
    let mut home: BTreeMap<String, (f32, f32)> = BTreeMap::new();
    let mut home_lab: BTreeMap<String, Home> = BTreeMap::new();
    for cl in &clusters {
        for (local, h) in &cl.homes {
            let q = format!("{}/{}", cl.sid, local);
            home.insert(q.clone(), (h.x, h.y));
            home_lab.insert(q, *h);
        }
    }

    // Flat nodes: places then transitions, in the flat net's order.
    let mut nodes: Vec<Node> = vec![];
    let mut idx: BTreeMap<String, usize> = BTreeMap::new();
    for (place, list) in [
        (true, arr(&flat["places"])),
        (false, arr(&flat["transitions"])),
    ] {
        for el in list {
            let id = s(&el["id"]).to_string();
            let members = strs(&el["members"]);
            let fused = members.len() > 1;
            let lines = lines_of(&id);
            let (w, h, r) = geom(place, &lines, fused);
            let mut n = Node {
                id: id.clone(),
                place,
                data_place: place && s(&el["kind"]) == "data",
                fused,
                members: members.clone(),
                lines,
                x: 0.0,
                y: 0.0,
                w,
                h,
                r,
                lab: Lab::Below,
                cluster: None,
            };
            if !fused {
                if let Some(m) = members.first() {
                    if let Some(hm) = home_lab.get(m) {
                        n.x = hm.x;
                        n.y = hm.y;
                        n.lab = hm.lab;
                    }
                    let sid = split_id(m).0.trim_end_matches('/').to_string();
                    n.cluster = clusters.iter().position(|c| c.sid == sid);
                }
            }
            idx.insert(id, nodes.len());
            nodes.push(n);
        }
    }
    let arcs: Vec<(String, String)> = arr(&flat["arcs"])
        .iter()
        .map(|a| (s(&a["from"]).to_string(), s(&a["to"]).to_string()))
        .collect();

    // Fused elements: into the gap between bands nearest the mean of their homes.
    let mut gaps: Vec<(f32, f32)> = vec![];
    for i in 0..bands.len().saturating_sub(1) {
        gaps.push((bands[i].1, bands[i + 1].0));
    }
    if gaps.is_empty() {
        gaps.push((bands[0].1, bands[0].1 + BAND_GAP));
    }
    let mut per_gap: Vec<Vec<usize>> = vec![vec![]; gaps.len()];
    for ni in 0..nodes.len() {
        if !nodes[ni].fused {
            continue;
        }
        let hs: Vec<(f32, f32)> = nodes[ni]
            .members
            .iter()
            .filter_map(|m| home.get(m).copied())
            .collect();
        let k = hs.len().max(1) as f32;
        let cx = hs.iter().map(|h| h.0).sum::<f32>() / k;
        let cy = hs.iter().map(|h| h.1).sum::<f32>() / k;
        let mut gi = 0;
        let mut best = f32::MAX;
        for (i, g) in gaps.iter().enumerate() {
            let d = ((g.0 + g.1) / 2.0 - cy).abs();
            if d < best - 1e-6 {
                best = d;
                gi = i;
            }
        }
        nodes[ni].x = cx;
        nodes[ni].y = (gaps[gi].0 + gaps[gi].1) / 2.0;
        nodes[ni].lab = if nodes[ni].place {
            Lab::Right
        } else {
            Lab::None
        };
        per_gap[gi].push(ni);
    }
    // No two fused elements in one gap overlap: sweep left to right.
    for list in &mut per_gap {
        list.sort_by(|a, b| {
            nodes[*a]
                .x
                .total_cmp(&nodes[*b].x)
                .then_with(|| nodes[*a].id.cmp(&nodes[*b].id))
        });
        for i in 1..list.len() {
            let (a, b) = (list[i - 1], list[i]);
            let ext = |n: &Node| {
                if !n.place {
                    n.w / 2.0
                } else {
                    n.r + 6.0 + n.lines.iter().map(|l| mw(l, FS_LOC)).fold(0.0, f32::max)
                }
            };
            let min_dx = (if !nodes[a].place {
                nodes[a].w / 2.0
            } else {
                ext(&nodes[a])
            }) + (if !nodes[b].place {
                nodes[b].w / 2.0
            } else {
                nodes[b].r
            }) + 24.0;
            if nodes[b].x - nodes[a].x < min_dx {
                nodes[b].x = nodes[a].x + min_dx;
            }
        }
    }
    refine_fused(&mut nodes, &idx, &arcs, &gaps, &per_gap);

    // A gap with no fused element in it closes up, so an unlinked composite is
    // not split by blank bands.
    const CLOSE: f32 = BAND_GAP - 40.0;
    let shift_at = |y: f32| -> f32 {
        gaps.iter()
            .enumerate()
            .map(|(i, g)| {
                if per_gap[i].is_empty() && y >= g.1 - 0.5 {
                    CLOSE
                } else {
                    0.0
                }
            })
            .sum()
    };
    for c in &mut clusters {
        let d = shift_at(c.rect.y);
        if d != 0.0 {
            c.rect.y -= d;
            for h in c.homes.values_mut() {
                h.y -= d;
            }
        }
    }
    for n in &mut nodes {
        let d = shift_at(n.y);
        n.y -= d;
    }
    for b in &mut bands {
        let d = shift_at(b.0);
        b.0 -= d;
        b.1 -= d;
    }
    // home positions follow the shift (the merge animation starts there)
    for cl in &clusters {
        for (local, h) in &cl.homes {
            home.insert(format!("{}/{}", cl.sid, local), (h.x, h.y));
        }
    }

    let cl_out: Vec<Cluster> = clusters
        .iter()
        .map(|c| Cluster {
            sid: c.sid.clone(),
            pattern: c.pattern.clone(),
            extra: c.extra,
            rect: c.rect,
        })
        .collect();
    place_labels(&mut nodes, &idx, &arcs, &cl_out);

    // Canvas.
    let (mut x1, mut y1) = (0.0f32, 0.0f32);
    for c in &cl_out {
        x1 = x1.max(c.rect.x + c.rect.w);
        y1 = y1.max(c.rect.y + c.rect.h);
    }
    for n in &nodes {
        let e = extent_of(n);
        x1 = x1.max(e.x1);
        y1 = y1.max(e.y1);
    }
    // Legend: right of band 1 when it fits, else under everything.
    const LW: f32 = 236.0;
    const LH: f32 = 178.0;
    let w0 = 1150f32.max(x1 + 24.0);
    let legend;
    if band1_right + 24.0 + LW <= w0 - 12.0 && bands.len() > 1 {
        legend = R {
            x: w0 - 12.0 - LW,
            y: bands[1].0 + 0f32.max((bands[1].1 - bands[1].0 - LH) / 2.0),
            w: LW,
            h: LH,
        };
    } else {
        legend = R {
            x: LEFT,
            y: y1 + 18.0,
            w: LW,
            h: LH,
        };
        y1 = legend.y + LH;
    }
    Lay {
        nodes,
        idx,
        clusters: cl_out,
        home,
        legend,
        w: w0,
        h: (y1 + 16.0).ceil(),
    }
}

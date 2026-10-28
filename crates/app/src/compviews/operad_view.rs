//! The operad view (see `docs/composition-views-scene.md`): Spivak's
//! wiring diagrams. `orders` draws the bundle as an outer box with one
//! typed hole per instance, event links as junctions, token and data links
//! as cables, unlinked ports exposed on the outer boundary; `connect4`
//! (`a_operad_c4.rs`) draws the search plan as nested boxes.
//!
//! A port of the prototype's `js/views/operad.js`; layout is deterministic
//! and in design coordinates (the prototype's SVG viewBox), mapped by [`Xf`].

use super::a_operad_theme::*;
use super::a_draw::*;
use super::*;

const OW: f32 = 1150.0;
const DY: f32 = 26.0;
const HEAD: f32 = 62.0;
const PADB: f32 = 12.0;
const LBL: f32 = 10.5;
/// Links run in the channel below row 0 (the mid channel with two rows).
const LINK_CH: i32 = 1;
const NC: usize = 3;

fn p2(x: f32, y: f32) -> Pos2 {
    Pos2::new(x, y)
}

fn key_of(sub: &str, port: &str) -> String {
    format!("{sub}|{port}")
}

fn num_sub(n: usize) -> String {
    n.to_string()
}

/// A hole of the outer box: an instance, its ports sorted onto sides.
struct Cell {
    sub: Value,
    id: String,
    c: usize,
    r: usize,
    idx: usize,
    l: Vec<Value>,
    r_ports: Vec<Value>,
    t: Vec<Value>,
    title: String,
    a: String,
    b: String,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Clone, Copy)]
struct Pt {
    x: f32,
    y: f32,
    /// 'L' 'R' 'T' 'B'.
    side: char,
}

struct Anchor {
    key: String,
    cell: usize,
    port: Value,
    l: Option<Pt>,
    r: Option<Pt>,
    t: Option<Pt>,
    b: Option<Pt>,
}

#[derive(Clone)]
struct End {
    pt: Pt,
    /// ('g', gutter) or ('h', channel).
    lane: (char, i32),
    key: String,
}

struct Grid {
    x0: f32,
    x1: f32,
    y0: f32,
    y1: f32,
    col0: Vec<f32>,
    col1: Vec<f32>,
    row_y: Vec<f32>,
    row_h: Vec<f32>,
    nr: usize,
    g_use: BTreeMap<i32, usize>,
    h_use: BTreeMap<i32, usize>,
}

const GOFF: [f32; 9] = [0.0, -12.0, 12.0, -24.0, 24.0, -36.0, 36.0, -48.0, 48.0];
const HOFF: [f32; 9] = [0.0, -14.0, 14.0, -28.0, 28.0, -42.0, 42.0, -56.0, 56.0];

impl Grid {
    fn gx0(&self, g: i32) -> f32 {
        if g == 0 {
            self.x0
        } else {
            self.col1[g as usize - 1]
        }
    }
    fn gx1(&self, g: i32) -> f32 {
        if g as usize == NC {
            self.x1
        } else {
            self.col0[g as usize]
        }
    }
    fn gcx(&self, g: i32) -> f32 {
        (self.gx0(g) + self.gx1(g)) / 2.0
    }
    fn hy0(&self, h: i32) -> f32 {
        if h == 0 {
            self.y0
        } else {
            self.row_y[h as usize - 1] + self.row_h[h as usize - 1]
        }
    }
    fn hy1(&self, h: i32) -> f32 {
        if h as usize == self.nr {
            self.y1
        } else {
            self.row_y[h as usize]
        }
    }
    fn hcy(&self, h: i32) -> f32 {
        (self.hy0(h) + self.hy1(h)) / 2.0
    }
    fn alloc_g(&mut self, g: i32) -> f32 {
        let k = *self.g_use.get(&g).unwrap_or(&0);
        self.g_use.insert(g, k + 1);
        let half = (self.gx1(g) - self.gx0(g)) / 2.0 - 8.0;
        self.gcx(g) + GOFF[k % GOFF.len()].clamp(-half, half)
    }
    fn alloc_h(&mut self, h: i32) -> f32 {
        let k = *self.h_use.get(&h).unwrap_or(&0);
        self.h_use.insert(h, k + 1);
        let half = (self.hy1(h) - self.hy0(h)) / 2.0 - 8.0;
        self.hcy(h) + HOFF[k % HOFF.len()].clamp(-half, half)
    }
}

/// An orthogonal polyline with its collinear and repeated points dropped
/// (`orth`'s corner list); drawn rounded by `rounded_pts`.
fn orth(points: &[Pos2]) -> Vec<Pos2> {
    let mut p: Vec<Pos2> = vec![];
    for q in points {
        if p.last()
            .is_none_or(|l| (l.x - q.x).abs() > 0.01 || (l.y - q.y).abs() > 0.01)
        {
            p.push(*q);
        }
    }
    if p.len() < 2 {
        return p;
    }
    let mut s = vec![p[0]];
    for i in 1..p.len() - 1 {
        let (a, b, c) = (*s.last().unwrap(), p[i], p[i + 1]);
        let col = ((a.x - b.x).abs() < 0.01 && (b.x - c.x).abs() < 0.01)
            || ((a.y - b.y).abs() < 0.01 && (b.y - c.y).abs() < 0.01);
        if !col {
            s.push(b);
        }
    }
    s.push(*p.last().unwrap());
    s
}

fn crosses(a: (Pos2, Pos2), b: (Pos2, Pos2)) -> bool {
    let h1 = (a.0.y - a.1.y).abs() < 0.01;
    let h2 = (b.0.y - b.1.y).abs() < 0.01;
    if h1 == h2 {
        return false;
    }
    let (h, v) = if h1 { (a, b) } else { (b, a) };
    let (x, y) = (v.0.x, h.0.y);
    let (hx0, hx1) = (h.0.x.min(h.1.x), h.0.x.max(h.1.x));
    let (vy0, vy1) = (v.0.y.min(v.1.y), v.0.y.max(v.1.y));
    x > hx0 + 0.5 && x < hx1 - 0.5 && y > vy0 + 0.5 && y < vy1 - 0.5
}

fn segs(pts: &[Pos2]) -> Vec<(Pos2, Pos2)> {
    pts.windows(2).map(|w| (w[0], w[1])).collect()
}

fn dash_of(kind: &str) -> Option<(f32, f32)> {
    let d = s(&comp::ontology()["link_style"][kind]["dash"]);
    let mut it = d.split(' ').filter_map(|x| x.parse::<f32>().ok());
    Some((it.next()?, it.next()?))
}

/// The prototype's `sidesOf`: place ports first, then transition ports;
/// in left, out right, inout both, observe a tap.
fn sides_of(sub: &Value) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let (mut l, mut r, mut t) = (vec![], vec![], vec![]);
    let ports = arr(&sub["ports"]);
    let ordered = ports
        .iter()
        .filter(|p| s(&p["target"]) == "place")
        .chain(ports.iter().filter(|p| s(&p["target"]) == "transition"));
    for p in ordered {
        match s(&p["kind"]) {
            "in" => l.push(p.clone()),
            "out" => r.push(p.clone()),
            "inout" => {
                l.push(p.clone());
                r.push(p.clone());
            }
            _ => t.push(p.clone()),
        }
    }
    (l, r, t)
}

fn header_of(sub: &Value) -> (String, String, String) {
    let pat = &comp::ontology()["patterns"][s(&sub["pattern"])];
    let title = format!("{} : {}", s(&sub["id"]), s(&pat["prefLabel"]));
    let place = |id: &str| arr(&sub["places"]).iter().find(|p| s(&p["id"]) == id);
    let (mut a, mut b) = (String::new(), String::new());
    match s(&sub["pattern"]) {
        "queue" => {
            if let Some(it) = place("items") {
                let cap = it["capacity"].as_i64().unwrap_or(0);
                let ini = it["initial"].as_i64().unwrap_or(0);
                a = if cap > 0 {
                    format!("capacity {cap} · initial {ini}")
                } else {
                    format!("unbounded · initial {ini}")
                };
            }
            b = if arr(&sub["ports"])
                .iter()
                .any(|p| s(&p["pattern_port"]) == "queue-port-payload")
            {
                "with payload"
            } else {
                "no payload"
            }
            .into();
        }
        "resource-pool" => {
            if let Some(av) = place("available") {
                a = format!("capacity {}", av["capacity"].as_i64().unwrap_or(0));
            }
            b = "units: available ⇄ in_use".into();
        }
        "saga" => {
            let steps: Vec<String> = arr(&sub["params"]["steps"])
                .iter()
                .map(|x| s(&x["id"]).to_string())
                .collect();
            a = format!("steps: {}", steps.join(" → "));
            let accts: Vec<String> = sub["params"]["accounts"]
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default();
            let shared = accts.iter().any(|acc| {
                place(acc).is_some_and(|p| s(&p["description"]).contains("shared"))
                    || sub["params"]["accounts"][acc]["shared"] == true
            });
            b = format!(
                "accounts: {}{}",
                accts.join(", "),
                if shared { " (shared)" } else { "" }
            );
        }
        _ => {}
    }
    (title, a, b)
}

pub(super) fn operad_orders(c: &mut Ctx, area: Rect) {
    let subs = subnets(c);
    if subs.is_empty() {
        return;
    }
    let ls = links(c);
    let ls_all = ls.clone();
    let ont = comp::ontology();
    // ---- grid: column by pattern, row by instance number ----
    let mut seen: BTreeMap<usize, usize> = BTreeMap::new();
    let mut cells: Vec<Cell> = vec![];
    for (idx, sub) in subs.iter().enumerate() {
        let col = match s(&sub["pattern"]) {
            "queue" => 0,
            "resource-pool" => 1,
            _ => 2,
        };
        let r = seen.get(&col).map_or(0, |n| n + 1);
        seen.insert(col, r);
        let (l, rp, t) = sides_of(sub);
        let (title, a, b) = header_of(sub);
        cells.push(Cell {
            id: s(&sub["id"]).into(),
            sub: sub.clone(),
            c: col,
            r,
            idx,
            l,
            r_ports: rp,
            t,
            title,
            a,
            b,
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        });
    }
    let nr = cells.iter().map(|x| x.r).max().unwrap_or(0) + 1;
    for cell in cells.iter_mut() {
        let tap_bottom = cell.r > 0 && !cell.t.is_empty();
        let n = cell.l.len().max(cell.r_ports.len());
        let lw = cell
            .l
            .iter()
            .map(|p| tw(s(&p["id"]), LBL, true))
            .fold(0.0, f32::max);
        let rw = cell
            .r_ports
            .iter()
            .map(|p| tw(s(&p["id"]), LBL, true))
            .fold(0.0, f32::max);
        let w = [
            206.0,
            18.0 * 2.0 + 12.0 + lw + rw + 36.0,
            tw(&cell.title, 12.5, false)
                + 34.0
                + if !cell.t.is_empty() && !tap_bottom {
                    64.0
                } else {
                    0.0
                },
            tw(&cell.a, 10.5, false) + 34.0,
            tw(&cell.b, 10.5, false) + 34.0,
        ]
        .into_iter()
        .fold(0.0, f32::max);
        cell.w = w.ceil();
        cell.h = HEAD + n as f32 * DY + PADB + if tap_bottom { 20.0 } else { 0.0 };
    }
    let col_w: Vec<f32> = (0..NC)
        .map(|k| {
            cells
                .iter()
                .filter(|x| x.c == k)
                .map(|x| x.w)
                .fold(0.0, f32::max)
        })
        .collect();
    let row_h: Vec<f32> = (0..nr)
        .map(|r| {
            cells
                .iter()
                .filter(|x| x.r == r)
                .map(|x| x.h)
                .fold(0.0, f32::max)
        })
        .collect();
    let (x0, x1) = (22.0, OW - 22.0);
    let rest = x1 - x0 - col_w.iter().sum::<f32>();
    let unit = rest / (2.0 * 0.55 + (NC as f32 - 1.0));
    let outer_g = unit * 0.55;
    let (mut col0, mut col1) = (vec![], vec![]);
    let mut cx = x0 + outer_g;
    for w in &col_w {
        col0.push(cx);
        col1.push(cx + w);
        cx += w + unit;
    }
    let y0 = 78.0;
    let (ch_top, ch_mid) = (50.0, 124.0);
    let ch_bot = if nr == 1 { 96.0 } else { 50.0 };
    let mut row_y = vec![];
    let mut cy = y0 + ch_top;
    for (r, rh) in row_h.iter().enumerate() {
        row_y.push(cy);
        cy += rh + if r < nr - 1 { ch_mid } else { 0.0 };
    }
    let y1 = cy + ch_bot;
    let oh2 = y1 + 108.0;
    for cell in cells.iter_mut() {
        cell.x = col0[cell.c] + (col_w[cell.c] - cell.w) / 2.0;
        cell.y = row_y[cell.r];
    }
    let mut grid = Grid {
        x0,
        x1,
        y0,
        y1,
        col0,
        col1,
        row_y,
        row_h,
        nr,
        g_use: BTreeMap::new(),
        h_use: BTreeMap::new(),
    };
    let cell_of = |id: &str| cells.iter().position(|x| x.id == id);

    // ---- port anchors ----
    let mut anchors: Vec<Anchor> = vec![];
    for (ci, cell) in cells.iter().enumerate() {
        let tap_side = if cell.r == 0 { 'T' } else { 'B' };
        let mut put = |p: &Value, side: char, pt: Pt| {
            let k = key_of(&cell.id, s(&p["id"]));
            let i = match anchors.iter().position(|a| a.key == k) {
                Some(i) => i,
                None => {
                    anchors.push(Anchor {
                        key: k,
                        cell: ci,
                        port: p.clone(),
                        l: None,
                        r: None,
                        t: None,
                        b: None,
                    });
                    anchors.len() - 1
                }
            };
            match side {
                'L' => anchors[i].l = Some(pt),
                'R' => anchors[i].r = Some(pt),
                'T' => anchors[i].t = Some(pt),
                _ => anchors[i].b = Some(pt),
            }
        };
        for (i, p) in cell.l.iter().enumerate() {
            put(
                p,
                'L',
                Pt {
                    x: cell.x,
                    y: cell.y + HEAD + DY * (i as f32 + 0.5),
                    side: 'L',
                },
            );
        }
        for (i, p) in cell.r_ports.iter().enumerate() {
            put(
                p,
                'R',
                Pt {
                    x: cell.x + cell.w,
                    y: cell.y + HEAD + DY * (i as f32 + 0.5),
                    side: 'R',
                },
            );
        }
        for (i, p) in cell.t.iter().enumerate() {
            let x = cell.x + cell.w - 36.0 - i as f32 * 54.0;
            let y = if tap_side == 'T' {
                cell.y
            } else {
                cell.y + cell.h
            };
            put(
                p,
                tap_side,
                Pt {
                    x,
                    y,
                    side: tap_side,
                },
            );
        }
    }
    let lane_of = |cell: &Cell, pt: Pt| -> (char, i32) {
        match pt.side {
            'L' => ('g', cell.c as i32),
            'R' => ('g', cell.c as i32 + 1),
            'T' => ('h', cell.r as i32),
            _ => ('h', cell.r as i32 + 1),
        }
    };
    let end_for = |sub: &str, port: &str, other: Option<&str>| -> Option<End> {
        let k = key_of(sub, port);
        let a = anchors.iter().find(|a| a.key == k)?;
        let cell = &cells[a.cell];
        let pt = if let Some(p) = a.t.or(a.b) {
            p
        } else if let (Some(l), Some(r)) = (a.l, a.r) {
            let oc = other.and_then(cell_of).map(|i| cells[i].c);
            match oc {
                Some(o) if o > cell.c => r,
                Some(o) if o < cell.c => l,
                _ => {
                    if cell.c == 0 {
                        l
                    } else {
                        r
                    }
                }
            }
        } else {
            a.l.or(a.r)?
        };
        Some(End {
            pt,
            lane: lane_of(cell, pt),
            key: k,
        })
    };

    // routing of a non-event cable
    let route = |grid: &mut Grid, a: &End, b: &End| -> Vec<Pos2> {
        fn go(grid: &mut Grid, a: &End, b: &End) -> Vec<Pos2> {
            let (aa, bb) = (p2(a.pt.x, a.pt.y), p2(b.pt.x, b.pt.y));
            if a.lane.0 == 'g' && b.lane.0 == 'g' {
                if a.lane.1 == b.lane.1 {
                    let t = grid.alloc_g(a.lane.1);
                    return vec![aa, p2(t, a.pt.y), p2(t, b.pt.y), bb];
                }
                let ty = grid.alloc_h(LINK_CH);
                let ta = grid.alloc_g(a.lane.1);
                let tb = grid.alloc_g(b.lane.1);
                return vec![
                    aa,
                    p2(ta, a.pt.y),
                    p2(ta, ty),
                    p2(tb, ty),
                    p2(tb, b.pt.y),
                    bb,
                ];
            }
            if a.lane.0 == 'g' && b.lane.0 == 'h' {
                let ta = grid.alloc_g(a.lane.1);
                let ty = grid.alloc_h(b.lane.1);
                return vec![aa, p2(ta, a.pt.y), p2(ta, ty), p2(b.pt.x, ty), bb];
            }
            if a.lane.0 == 'h' && b.lane.0 == 'g' {
                let mut r = go(grid, b, a);
                r.reverse();
                return r;
            }
            if a.lane.1 == b.lane.1 {
                let ty = grid.alloc_h(a.lane.1);
                return vec![aa, p2(a.pt.x, ty), p2(b.pt.x, ty), bb];
            }
            let mut g = 0;
            let mut best = f32::INFINITY;
            for k in 0..=NC as i32 {
                let d = (grid.gcx(k) - (a.pt.x + b.pt.x) / 2.0).abs();
                if d < best {
                    best = d;
                    g = k;
                }
            }
            let tg = grid.alloc_g(g);
            let ya = grid.alloc_h(a.lane.1);
            let yb = grid.alloc_h(b.lane.1);
            vec![
                aa,
                p2(a.pt.x, ya),
                p2(tg, ya),
                p2(tg, yb),
                p2(b.pt.x, yb),
                bb,
            ]
        }
        go(grid, a, b)
    };

    // ---- event classes: one junction per fused class (union-find on keys) ----
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    fn find(parent: &mut BTreeMap<String, String>, x: &str) -> String {
        parent.entry(x.to_string()).or_insert_with(|| x.to_string());
        let mut cur = x.to_string();
        while parent[&cur] != cur {
            cur = parent[&cur].clone();
        }
        cur
    }
    let ev_links: Vec<&Value> = ls.iter().filter(|l| s(&l["kind"]) == "event").collect();
    let lk = |l: &Value, end: &str| key_of(s(&l[end]["subnet"]), s(&l[end]["port"]));
    for l in &ev_links {
        let (ra, rb) = (
            find(&mut parent, &lk(l, "from")),
            find(&mut parent, &lk(l, "to")),
        );
        if ra != rb {
            let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
            parent.insert(hi, lo);
        }
    }
    struct Class {
        root: String,
        members: Vec<(String, String)>,
        links: Vec<Value>,
    }
    let mut classes: Vec<Class> = vec![];
    for l in &ev_links {
        let root = find(&mut parent, &lk(l, "from"));
        let i = match classes.iter().position(|c| c.root == root) {
            Some(i) => i,
            None => {
                classes.push(Class {
                    root,
                    members: vec![],
                    links: vec![],
                });
                classes.len() - 1
            }
        };
        classes[i].links.push((*l).clone());
        for e in ["from", "to"] {
            let m = (s(&l[e]["subnet"]).to_string(), s(&l[e]["port"]).to_string());
            if !classes[i].members.contains(&m) {
                classes[i].members.push(m);
            }
        }
    }
    struct Spoke {
        end: End,
        pts: Vec<Pos2>,
    }
    struct Junction {
        ci: usize,
        jx: f32,
        jy: f32,
        spokes: Vec<Spoke>,
        leak: bool,
    }
    let mut link_segs: Vec<(Pos2, Pos2)> = vec![];
    let mut junctions: Vec<Junction> = vec![];
    for (ci, cl) in classes.iter().enumerate() {
        let ends: Vec<End> = cl
            .members
            .iter()
            .filter_map(|(sb, pt)| end_for(sb, pt, None))
            .collect();
        let mut by_g: BTreeMap<i32, Vec<&End>> = BTreeMap::new();
        for e in &ends {
            by_g.entry(e.lane.1).or_default().push(e);
        }
        let (mut gj, mut cnt) = (None, 0);
        for (g, es) in &by_g {
            if es.len() > cnt {
                cnt = es.len();
                gj = Some(*g);
            }
        }
        let (jx, jy);
        let mut spokes: Vec<Spoke> = vec![];
        if let (true, Some(gj)) = (cnt >= 2, gj) {
            jx = grid.alloc_g(gj);
            let local = &by_g[&gj];
            jy = local.iter().map(|e| e.pt.y).sum::<f32>() / local.len() as f32;
            let mut ty: Option<f32> = None;
            for e in &ends {
                let pts = if e.lane.1 == gj {
                    vec![p2(e.pt.x, e.pt.y), p2(jx, e.pt.y), p2(jx, jy)]
                } else {
                    let ty = *ty.get_or_insert_with(|| grid.alloc_h(LINK_CH));
                    let t = grid.alloc_g(e.lane.1);
                    vec![
                        p2(e.pt.x, e.pt.y),
                        p2(t, e.pt.y),
                        p2(t, ty),
                        p2(jx, ty),
                        p2(jx, jy),
                    ]
                };
                spokes.push(Spoke {
                    end: e.clone(),
                    pts,
                });
            }
        } else {
            jy = grid.alloc_h(LINK_CH);
            let ts: Vec<(&End, f32)> = ends.iter().map(|e| (e, grid.alloc_g(e.lane.1))).collect();
            jx = ts.iter().map(|x| x.1).sum::<f32>() / ts.len().max(1) as f32;
            for (e, t) in ts {
                spokes.push(Spoke {
                    end: e.clone(),
                    pts: vec![p2(e.pt.x, e.pt.y), p2(t, e.pt.y), p2(t, jy), p2(jx, jy)],
                });
            }
        }
        for sp in spokes.iter_mut() {
            sp.pts = orth(&sp.pts);
            link_segs.extend(segs(&sp.pts));
        }
        junctions.push(Junction {
            ci,
            jx,
            jy,
            spokes,
            leak: cl.links.iter().any(|l| l["leak"] == true),
        });
    }
    struct Wire {
        l: Value,
        pts: Vec<Pos2>,
    }
    let mut wires: Vec<Wire> = vec![];
    for l in ls.iter().filter(|l| s(&l["kind"]) != "event") {
        let a = end_for(
            s(&l["from"]["subnet"]),
            s(&l["from"]["port"]),
            Some(s(&l["to"]["subnet"])),
        );
        let b = end_for(
            s(&l["to"]["subnet"]),
            s(&l["to"]["port"]),
            Some(s(&l["from"]["subnet"])),
        );
        let (Some(a), Some(b)) = (a, b) else { continue };
        let pts = orth(&route(&mut grid, &a, &b));
        link_segs.extend(segs(&pts));
        wires.push(Wire { l: l.clone(), pts });
    }

    // ---- exposure leaders for unlinked ports ----
    let mut linked: BTreeSet<String> = BTreeSet::new();
    for l in &ls {
        linked.insert(lk(l, "from"));
        linked.insert(lk(l, "to"));
    }
    struct Leader {
        cell: usize,
        port: Value,
        pts: Vec<Pos2>,
        mark: Pos2,
    }
    struct Pending {
        cell: usize,
        port: Value,
        pt: Pt,
        up: bool,
        sgn: f32,
        edge: (usize, char),
    }
    let mut leaders: Vec<Leader> = vec![];
    let mut pending: Vec<Pending> = vec![];
    for a in &anchors {
        if linked.contains(&a.key) {
            continue;
        }
        let cell = &cells[a.cell];
        let pt = if let Some(p) = a.t.or(a.b) {
            p
        } else if let (Some(l), Some(r)) = (a.l, a.r) {
            if cell.c == NC - 1 {
                r
            } else {
                l
            }
        } else {
            match a.l.or(a.r) {
                Some(p) => p,
                None => continue,
            }
        };
        let (cx_, cy_) = (pt.x, pt.y);
        let mk = |pts: Vec<Pos2>, mark: Pos2| Leader {
            cell: a.cell,
            port: a.port.clone(),
            pts,
            mark,
        };
        if pt.side == 'T' {
            leaders.push(mk(vec![p2(cx_, cy_), p2(cx_, y0)], p2(cx_, y0)));
        } else if pt.side == 'B' {
            leaders.push(mk(vec![p2(cx_, cy_), p2(cx_, y1)], p2(cx_, y1)));
        } else if pt.side == 'L' && cell.c == 0 {
            leaders.push(mk(vec![p2(cx_, cy_), p2(x0, cy_)], p2(x0, cy_)));
        } else if pt.side == 'R' && cell.c == NC - 1 {
            leaders.push(mk(vec![p2(cx_, cy_), p2(x1, cy_)], p2(x1, cy_)));
        } else {
            let sgn = if pt.side == 'L' { -1.0 } else { 1.0 };
            let probe = cx_ + sgn * 10.0;
            let count = |y_end: f32| -> usize {
                segs(&[p2(cx_, cy_), p2(probe, cy_), p2(probe, y_end)])
                    .iter()
                    .map(|sg| link_segs.iter().filter(|ls| crosses(*sg, **ls)).count())
                    .sum()
            };
            let up_c = count(y0) + if cell.r > 0 { 2 * cell.r } else { 0 };
            let down_c = count(y1) + (nr - 1 - cell.r) * 2;
            let up = if up_c != down_c {
                up_c < down_c
            } else {
                cell.r == 0
            };
            let g = if pt.side == 'L' { cell.c } else { cell.c + 1 };
            pending.push(Pending {
                cell: a.cell,
                port: a.port.clone(),
                pt,
                up,
                sgn,
                edge: (g, pt.side),
            });
        }
    }
    let mut edges: Vec<(usize, char)> = vec![];
    for p in &pending {
        if !edges.contains(&p.edge) {
            edges.push(p.edge);
        }
    }
    const LEAD_GAP: f32 = 16.0;
    for e in edges {
        let group: Vec<&Pending> = pending.iter().filter(|p| p.edge == e).collect();
        let mut ups: Vec<&&Pending> = group.iter().filter(|p| p.up).collect();
        ups.sort_by(|a, b| a.pt.y.total_cmp(&b.pt.y));
        let mut downs: Vec<&&Pending> = group.iter().filter(|p| !p.up).collect();
        downs.sort_by(|a, b| b.pt.y.total_cmp(&a.pt.y));
        let g = e.0 as i32;
        let room = (grid.gx1(g) - grid.gx0(g)) / 2.0 - 18.0;
        let n = group.len();
        let gap = if n > 1 {
            10.0_f32.max(LEAD_GAP.min(room / (n as f32 - 1.0)))
        } else {
            LEAD_GAP
        };
        let fan = gap < LEAD_GAP - 0.01;
        let mut place = |list: &[&&Pending], k0: usize| {
            for (i, p) in list.iter().enumerate() {
                let lx = p.pt.x + p.sgn * (10.0 + gap * (k0 + i) as f32);
                let y_end = if p.up { y0 } else { y1 };
                let (pts, mark) = if !fan {
                    (
                        vec![p2(p.pt.x, p.pt.y), p2(lx, p.pt.y), p2(lx, y_end)],
                        p2(lx, y_end),
                    )
                } else {
                    let j = (list.len() - 1 - i) as f32;
                    let mx = p.pt.x - p.sgn * (4.0 + LEAD_GAP * j);
                    let jy = y_end + if p.up { 1.0 } else { -1.0 } * 31.0_f32.min(6.0 + 5.0 * j);
                    (
                        vec![
                            p2(p.pt.x, p.pt.y),
                            p2(lx, p.pt.y),
                            p2(lx, jy),
                            p2(mx, jy),
                            p2(mx, y_end),
                        ],
                        p2(mx, y_end),
                    )
                };
                leaders.push(Leader {
                    cell: p.cell,
                    port: p.port.clone(),
                    pts,
                    mark,
                });
            }
        };
        place(&ups, 0);
        place(&downs, ups.len());
    }

    // ================= draw =================
    let xf = Xf::fit(area, OW, oh2, 1.6);
    let sp = |p: Pos2| xf.pt(p.x, p.y);
    let spts = |v: &[Pos2]| -> Vec<Pos2> { v.iter().map(|p| sp(*p)).collect() };
    let anim = c.anim(|e| Some(e.clone()));
    let ids: Vec<String> = cells.iter().map(|x| x.id.clone()).collect();
    let n_ev = ev_links.len();
    let count_kind = |k: &str| ls.iter().filter(|l| s(&l["kind"]) == k).count();
    let (n_tok, n_data, n_guard) = (count_kind("token"), count_kind("data"), count_kind("guard"));
    let mut cable_parts: Vec<String> = vec![];
    if n_ev > 0 {
        cable_parts.push(format!(
            "{n_ev} event link{} in {} junction{}",
            if n_ev > 1 { "s" } else { "" },
            classes.len(),
            if classes.len() > 1 { "s" } else { "" }
        ));
    }
    if n_tok > 0 {
        cable_parts.push(format!("{n_tok} token"));
    }
    if n_data > 0 {
        cable_parts.push(format!("{n_data} data"));
    }
    if n_guard > 0 {
        cable_parts.push(format!("{n_guard} guard (flagged)"));
    }
    let cable_txt = if cable_parts.is_empty() {
        "no cables: the composite is the monoidal product of the instances".to_string()
    } else {
        cable_parts.join(", ")
    };
    let ev_col = kind_color("event");

    // title
    let mut parts: Vec<(String, bool)> = vec![("Φ(".into(), false)];
    for (i, cell) in cells.iter().enumerate() {
        parts.push((if i > 0 { ", x" } else { "x" }.into(), false));
        parts.push((num_sub(cell.idx + 1), true));
    }
    parts.push((
        format!(
            "): one wiring diagram with {} typed holes, filled by {}",
            cells.len(),
            ids.join(", ")
        ),
        false,
    ));
    c.rich(xf, 14.0, 24.0, &parts, tx(15.0, OP_INK).bold());
    c.txt(
        xf,
        14.0,
        43.0,
        Al::L,
        &format!(
            "Filling a hole with a box whose ports match its interface is operadic composition. {}",
            if ls.is_empty() {
                "No cables: the composite is the monoidal product (disjoint union) of the instances.".to_string()
            } else {
                format!("Cables: {cable_txt}; every one is a link go-pflow's validateLink allowed.")
            }
        ),
        tx(11.0, OP_MUTED),
    );

    // outer box
    let outer_label = format!("outer box: {}", ids.join(" ⊗ "));
    let olw = tw(&outer_label, 12.0, false) + 20.0;
    c.rrect(
        xf,
        x0,
        y0,
        x1 - x0,
        y1 - y0,
        16.0,
        Some(Color32::WHITE),
        Some((OP_INK, 1.8)),
        None,
    );
    c.rrect(
        xf,
        x0 + 14.0,
        y0 - 24.0,
        olw,
        19.0,
        5.0,
        Some(OP_INK),
        None,
        None,
    );
    c.txt(
        xf,
        x0 + 14.0 + olw / 2.0,
        y0 - 10.5,
        Al::M,
        &outer_label,
        tx(12.0, Color32::WHITE).bold(),
    );
    let flat = c.d.flat.clone().unwrap_or(Value::Null);
    let flat_ok = flat["ok"] == true;
    let pre_leaders = leaders.len();
    c.hover(
        xf.rect(x0, y0, x1 - x0, y1 - y0),
        info(format!("outer box: {}", ids.join(" ⊗ ")))
            .fact("holes", cells.iter().map(|x| format!("x{} = {}", x.idx + 1, x.id)).collect::<Vec<_>>().join(", "))
            .fact("cables", cable_txt.clone())
            .fact("exposed ports", format!("{pre_leaders} (every port no link uses)"))
            .fact(
                "flat net",
                if flat_ok {
                    format!(
                        "{} places, {} transitions",
                        arr(&flat["places"]).len(),
                        arr(&flat["transitions"]).len()
                    )
                } else {
                    "flatten refused".into()
                },
            )
            .note("One operation of the wiring-diagram operad: its arguments are the holes x1…xn (typed by pattern interfaces), its cables are the links, and its own ports are the exposed ones on the boundary. Which ports the outer box exposes is this view's choice: a go-pflow Bundle declares no ports of its own.")
            .cite(cite_of("goPflow", "metamodel/compose.go", 207, 227, "Bundle: subnets, links, constraints; no ports of its own"))
            .cite(cite_of("goPflow", "metamodel/compose_flatten.go", 55, 80, "place and transition classes by union-find")),
    );

    // leaders (faint)
    for ld in &leaders {
        let pts = rounded_pts(&orth(&ld.pts), 5.0);
        c.dash_poly(&spts(&pts), FAINT, 1.0, xf.len(2.0), xf.len(3.0));
    }

    // non-event cables
    let anim_link_key = |a: &Option<(Event, f32)>| -> Option<(String, f32)> {
        match a {
            Some((Event::LinkAdded { key, .. }, t)) => Some((key.clone(), *t)),
            _ => None,
        }
    };
    let added = anim_link_key(&anim);
    for w in &wires {
        let kind = s(&w.l["kind"]);
        let col = kind_color(kind);
        let width = if kind == "guard" { 1.8 } else { 2.2 };
        let rp = rounded_pts(&w.pts, 7.0);
        let sp_pts = spts(&rp);
        let mine = added.as_ref().filter(|(k, _)| k == s(&w.l["key"]));
        let (alpha, reveal) = match mine {
            Some((_, t)) => ((*t - 0.6).max(0.0) / 0.4, Some(*t)),
            None => (1.0, None),
        };
        let draw_col = col.gamma_multiply(alpha.clamp(0.0, 1.0));
        match dash_of(kind) {
            Some((d, g)) => c.dash_poly(&sp_pts, draw_col, xf.len(width), xf.len(d), xf.len(g)),
            None => c.poly(&sp_pts, draw_col, xf.len(width)),
        }
        if let Some(t) = reveal {
            // the wire draws itself on, then the cable settles
            let shown = partial(&sp_pts, (t / 0.8).min(1.0));
            let fade = if t > 0.8 { 1.0 - (t - 0.8) / 0.2 } else { 1.0 };
            c.poly(
                &shown,
                col.gamma_multiply(fade.clamp(0.0, 1.0)),
                xf.len(3.2),
            );
        }
        // glyph on the longest segment
        let mut best = (w.pts[0], w.pts[1]);
        let mut bl = -1.0;
        for (a, b) in segs(&w.pts) {
            let l = a.distance(b);
            if l > bl {
                bl = l;
                best = (a, b);
            }
        }
        let mid = lerp2(best.0, best.1, 0.5);
        let m = sp(mid);
        let gl_alpha = alpha.clamp(0.0, 1.0);
        match kind {
            "token" => {
                let r = xf.len(6.0);
                c.p.add(Shape::convex_polygon(
                    vec![
                        m - Vec2::new(0.0, r),
                        m + Vec2::new(r, 0.0),
                        m + Vec2::new(0.0, r),
                        m - Vec2::new(r, 0.0),
                    ],
                    col.gamma_multiply(gl_alpha),
                    Stroke::new(1.2, Color32::WHITE.gamma_multiply(gl_alpha)),
                ));
            }
            "data" => {
                let ang = (best.1.y - best.0.y).atan2(best.1.x - best.0.x);
                let (sn, cs) = ang.sin_cos();
                let rot = |x: f32, y: f32| m + Vec2::new(x * cs - y * sn, x * sn + y * cs) * xf.k;
                c.p.add(Shape::convex_polygon(
                    vec![rot(-5.0, -6.0), rot(7.0, 0.0), rot(-5.0, 6.0)],
                    Color32::WHITE.gamma_multiply(gl_alpha),
                    Stroke::new(1.6, col.gamma_multiply(gl_alpha)),
                ));
            }
            _ => {
                c.p.circle(m, xf.len(7.0), Color32::WHITE, Stroke::new(1.6, col));
                let d = xf.len(4.5);
                c.line(m + Vec2::new(-d, d), m + Vec2::new(d, -d), col, 1.6);
            }
        }
        let li = link_info(&w.l);
        let i = if kind == "guard" {
            ext_note(li, "A guard is not a cable: it restricts one subnet (W_RESTRICTIVE_LINK) and sits outside the free-SMC fragment. Drawn and flagged; no operadic composite is claimed for it.")
                .cite(cite_of("goPflow", "metamodel/compose.go", 687, 706, "validateLink: W_RESTRICTIVE_LINK"))
        } else {
            let reading = if kind == "token" {
                "a cable joining two place ports. What it means, place gluing, is a pushout of open nets (Baez-Master); the operad's own substitution is a different pushout, of cable sets (Spivak Ex. 2.1.7)"
            } else {
                "a cable for read-only observation: the observer moves nothing on the observed place"
            };
            li.fact("operad reading", reading)
                .cite(cite_of(
                    "goPflow",
                    "metamodel/compose.go",
                    667,
                    673,
                    "port schema tags must agree when both are set (E_SCHEMA_MISMATCH)",
                ))
                .cite(cite_of(
                    "goPflow",
                    "metamodel/compose_flatten.go",
                    55,
                    80,
                    "place and transition classes by union-find",
                ))
        };
        c.hover_poly(&sp_pts, 6.0, i);
        if kind == "guard" {
            let tg = c.txt(
                xf,
                mid.x,
                mid.y - 11.0,
                Al::M,
                "contextual",
                tx(9.5, col).halo(PAPER),
            );
            c.hover(tg, link_info(&w.l));
        }
        if c.is_clicked_in(Rect::from_center_size(m, Vec2::splat(14.0))) {
            c.command("bundle.remove_link", json!({"index": w.l["index"]}));
        }
        c.scene
            .emit("junction", s(&w.l["key"]), json!({"kind": kind}));
    }

    // event spokes (under the holes)
    for j in &junctions {
        let cl = &classes[j.ci];
        let mine = added
            .as_ref()
            .filter(|(k, _)| cl.links.iter().any(|l| s(&l["key"]) == k));
        let new_ends: Vec<String> = match mine {
            Some((k, _)) => cl
                .links
                .iter()
                .filter(|l| s(&l["key"]) == k)
                .flat_map(|l| [lk(l, "from"), lk(l, "to")])
                .collect(),
            None => vec![],
        };
        for sp_ in &j.spokes {
            let pts = spts(&rounded_pts(&sp_.pts, 7.0));
            let new = new_ends.contains(&sp_.end.key);
            if let (true, Some((_, t))) = (new, mine) {
                let a = ((*t - 0.55) / 0.45).clamp(0.0, 1.0);
                c.poly(&pts, ev_col.gamma_multiply(a), xf.len(2.6));
                c.poly(
                    &partial(&pts, (*t / 0.8).min(1.0)),
                    ev_col.gamma_multiply(1.0 - a * 0.7),
                    xf.len(3.4),
                );
            } else {
                c.poly(&pts, ev_col, xf.len(2.6));
            }
        }
    }
    // hover of spokes is registered after the cables, before the holes
    for j in &junctions {
        let cl = &classes[j.ci];
        let i = junction_info(c, cl.members.as_slice(), &cl.links, &flat);
        for sp_ in &j.spokes {
            let pts = spts(&rounded_pts(&sp_.pts, 7.0));
            c.hover_poly(&pts, 6.0, i.clone());
        }
    }

    // holes, instance boxes and ports
    for (ci, cell) in cells.iter().enumerate() {
        let pat = &ont["patterns"][s(&cell.sub["pattern"])];
        let pat_label = s(&pat["prefLabel"]);
        let off = match &anim {
            Some((Event::InstanceAdded { id }, t)) if *id == cell.id => 30.0 * (1.0 - t),
            _ => 0.0,
        };
        let (hx, hy) = (cell.x, cell.y + off);
        c.rrect(
            xf,
            hx,
            hy,
            cell.w,
            cell.h,
            9.0,
            Some(Color32::WHITE),
            Some((hex("#8e96a1"), 1.2)),
            Some((6.0, 4.0)),
        );
        let tab_parts = vec![
            ("hole x".to_string(), false),
            (num_sub(cell.idx + 1), true),
            (format!(" : {pat_label} interface"), false),
        ];
        let tab_end = c.rich(
            xf,
            hx + 4.0,
            hy - 8.0,
            &tab_parts,
            tx(10.5, OP_MUTED).halo(PAPER),
        );
        let mut ti = info(format!("hole x{}: the {pat_label} interface", cell.idx + 1))
            .note("A hole of the outer box, typed by a pattern's ports (ns:PatternPort): their kinds, targets and schemas. Any box with exactly these ports can fill it.");
        for p in arr(&cell.sub["ports"]) {
            let sc = s(&p["schema"]);
            ti = ti.fact(
                s(&p["id"]),
                format!(
                    "{} {}{}",
                    s(&p["kind"]),
                    s(&p["target"]),
                    if sc.is_empty() {
                        String::new()
                    } else {
                        format!(" · {sc}")
                    }
                ),
            );
        }
        c.hover(
            Rect::from_min_max(xf.pt(hx + 2.0, hy - 18.0), xf.pt(tab_end, hy - 2.0)),
            ti,
        );
        // the instance box that fills it
        let (bx, by, bw, bh) = (hx + 5.0, hy + 5.0, cell.w - 10.0, cell.h - 10.0);
        c.rrect(
            xf,
            bx,
            by,
            bw,
            bh,
            6.0,
            Some(Color32::WHITE),
            Some((OP_INK, 1.5)),
            None,
        );
        // header tint
        c.p.rect_filled(
            xf.rect(bx + 1.0, by + 1.0, bw - 2.0, HEAD - 9.0),
            egui::CornerRadius {
                nw: xf.len(6.0) as u8,
                ne: xf.len(6.0) as u8,
                sw: 0,
                se: 0,
            },
            proto_pattern_tint(s(&cell.sub["pattern"])),
        );
        c.line(
            xf.pt(bx, by + HEAD - 8.0),
            xf.pt(bx + bw, by + HEAD - 8.0),
            hex("#d5d9df"),
            1.0,
        );
        c.txt(
            xf,
            bx + 12.0,
            by + 18.0,
            Al::L,
            &cell.title,
            tx(12.5, OP_INK).bold(),
        );
        c.txt(xf, bx + 12.0, by + 32.0, Al::L, &cell.a, tx(10.5, SLATE));
        c.txt(xf, bx + 12.0, by + 44.5, Al::L, &cell.b, tx(10.5, OP_MUTED));
        let bi = subnet_info(&cell.sub)
            .fact("fills", format!("hole x{}: its ports are the hole's ports, one for one", cell.idx + 1))
            .note("Filling a hole with this box is operadic composition (Spivak 2013, Ex. 2.1.7: substitution is a pushout of cables).");
        c.hover(xf.rect(bx, by, bw, bh), bi);
        // ports on the hole's edge
        let draw_port = |c: &mut Ctx, p: &Value, pt: Pt| {
            let col = proto_port_color(p);
            let is_t = s(&p["target"]) == "transition";
            let observe = s(&p["kind"]) == "observe";
            let at = xf.pt(pt.x, pt.y + off);
            let hit = Rect::from_center_size(at, Vec2::splat(xf.len(18.0).max(14.0)));
            let k = key_of(&cell.id, s(&p["id"]));
            let cables: Vec<String> = ls_all
                .iter()
                .filter(|l| lk(l, "from") == k || lk(l, "to") == k)
                .map(|l| {
                    format!(
                        "{} {} {}:{} — {}:{}",
                        kind_glyph(s(&l["kind"])),
                        s(&l["kind"]),
                        s(&l["from"]["subnet"]),
                        s(&l["from"]["port"]),
                        s(&l["to"]["subnet"]),
                        s(&l["to"]["port"])
                    )
                })
                .collect();
            let side_txt = match s(&p["kind"]) {
                "in" => "left: an in-port",
                "out" => "right: an out-port",
                "inout" => "both sides: an inout port (drawn twice, one port)",
                _ => "a side tap: observe",
            };
            let pulse = match &added {
                Some((key, t))
                    if ls_all.iter().any(|l| {
                        s(&l["key"]) == key && (lk(l, "from") == k || lk(l, "to") == k)
                    }) =>
                {
                    if *t < 0.35 {
                        1.0 + 1.1 * t / 0.35
                    } else {
                        2.1 - 1.1 * (t - 0.35) / 0.65
                    }
                }
                _ => 1.0,
            };
            let z = c.port_zone(hit, &cell.id, p, |i| {
                ext_note(
                    i.fact("side", side_txt).fact(
                        "cables",
                        if cables.is_empty() { "none: exposed on the outer box".to_string() } else { cables.join("; ") },
                    ),
                    "The side is go-pflow's port kind, kept for reading; Spivak's operad treats a box's ports as a set.",
                )
            });
            let dir = match pt.side {
                'L' => Vec2::new(5.0, 0.0),
                'R' => Vec2::new(-5.0, 0.0),
                'T' => Vec2::new(0.0, 5.0),
                _ => Vec2::new(0.0, -5.0),
            } * xf.k;
            let col = if z.dim { col.gamma_multiply(0.35) } else { col };
            c.line(at, at + dir, col, 1.5);
            if is_t {
                let r = Rect::from_center_size(at, Vec2::splat(xf.len(10.0 * pulse)));
                c.p.rect(
                    r,
                    xf.len(1.5),
                    col,
                    Stroke::new(1.2, Color32::WHITE),
                    StrokeKind::Middle,
                );
                c.zone_ring_rect(r.expand(3.0), &z);
            } else {
                let r = xf.len(5.5 * pulse);
                if observe {
                    c.p.circle(at, r, Color32::WHITE, Stroke::new(2.0, col));
                } else {
                    c.p.circle(at, r, col, Stroke::new(1.2, Color32::WHITE));
                }
                c.zone_ring_circle(at, r + 4.0, &z);
            }
            // chevron and label inside the box
            let lbl_c = if is_t { SLATE } else { OP_INK };
            if pt.side == 'L' || pt.side == 'R' {
                let cxv = if pt.side == 'L' {
                    bx + 9.0
                } else {
                    bx + bw - 9.0
                };
                let py = pt.y + off;
                let tri = |ax: f32, tip: f32| -> Vec<Pos2> {
                    vec![
                        xf.pt(ax + tip, py),
                        xf.pt(ax, py - 3.0),
                        xf.pt(ax, py + 3.0),
                    ]
                };
                let cc = col.gamma_multiply(0.85);
                if s(&p["kind"]) == "inout" {
                    c.p.add(Shape::convex_polygon(
                        tri(cxv - 1.0, -3.0),
                        cc,
                        Stroke::NONE,
                    ));
                    c.p.add(Shape::convex_polygon(tri(cxv + 1.0, 3.0), cc, Stroke::NONE));
                } else {
                    c.p.add(Shape::convex_polygon(
                        vec![
                            xf.pt(cxv - 2.5, py - 3.5),
                            xf.pt(cxv + 2.5, py),
                            xf.pt(cxv - 2.5, py + 3.5),
                        ],
                        cc,
                        Stroke::NONE,
                    ));
                }
                let (lx, al) = if pt.side == 'L' {
                    (bx + 17.0, Al::L)
                } else {
                    (bx + bw - 17.0, Al::R)
                };
                c.txt(xf, lx, py + 3.6, al, s(&p["id"]), tx(LBL, lbl_c).mono());
            } else {
                let ly = if pt.side == 'T' {
                    by + 20.0
                } else {
                    by + bh - 8.0
                };
                c.txt(xf, pt.x, ly, Al::M, s(&p["id"]), tx(LBL, lbl_c).mono());
            }
        };
        let hole_port = |c: &mut Ctx, p: &Value, side: char| {
            if let Some(a) = anchors
                .iter()
                .find(|a| a.key == key_of(&cell.id, s(&p["id"])))
            {
                let pt = match side {
                    'L' => a.l,
                    'R' => a.r,
                    _ => a.t.or(a.b),
                };
                if let Some(pt) = pt {
                    draw_port(c, p, pt);
                }
            }
        };
        for p in &cell.l {
            hole_port(c, p, 'L');
        }
        for p in &cell.r_ports {
            hole_port(c, p, 'R');
        }
        for p in &cell.t {
            hole_port(c, p, 'T');
        }
        c.scene.emit("hole", &cell.id, json!({}));
        let _ = ci;
    }

    // event junctions (dots, over the holes)
    for j in &junctions {
        let cl = &classes[j.ci];
        let at = sp(p2(j.jx, j.jy));
        let mine = added
            .as_ref()
            .filter(|(k, _)| cl.links.iter().any(|l| s(&l["key"]) == k));
        let scale = match mine {
            Some((_, t)) => {
                if *t < 0.5 {
                    0.2
                } else if *t < 0.75 {
                    0.2 + 1.4 * (t - 0.5) / 0.25
                } else {
                    1.6 - 0.6 * (t - 0.75) / 0.25
                }
            }
            None => 1.0,
        };
        let r = xf.len(7.5 * scale);
        c.p.circle(at, r, ev_col, Stroke::new(1.5, Color32::WHITE));
        let d = xf.len(3.6 * scale);
        c.line(
            at + Vec2::new(-d, -d),
            at + Vec2::new(d, d),
            Color32::WHITE,
            1.7,
        );
        c.line(
            at + Vec2::new(d, -d),
            at + Vec2::new(-d, d),
            Color32::WHITE,
            1.7,
        );
        if let Some((_, t)) = mine {
            if *t > 0.5 {
                let rr = xf.len(8.0 + 22.0 * (t - 0.5) / 0.5);
                c.p.circle_stroke(
                    at,
                    rr,
                    Stroke::new(2.5, ev_col.gamma_multiply(0.9 * (1.0 - (t - 0.5) / 0.5))),
                );
            }
        }
        if j.leak {
            c.dash_poly(
                &circle_pts(at, xf.len(11.5)),
                OP_LEAK,
                1.6,
                xf.len(3.0),
                xf.len(2.0),
            );
            let plate = xf.rect(j.jx - 42.0, j.jy - 55.0, 84.0, 40.0);
            c.p.rect(
                plate,
                xf.len(5.0),
                Color32::WHITE.gamma_multiply(0.9),
                Stroke::new(1.0, OP_LEAK.gamma_multiply(0.25)),
                StrokeKind::Middle,
            );
            c.txt(
                xf,
                j.jx,
                j.jy - 44.0,
                Al::M,
                "pool leak:",
                tx(10.5, OP_LEAK).bold(),
            );
            c.txt(
                xf,
                j.jx,
                j.jy - 31.0,
                Al::M,
                "on rollback",
                tx(10.0, OP_LEAK),
            );
            c.txt(
                xf,
                j.jx,
                j.jy - 19.0,
                Al::M,
                "no port fires",
                tx(10.0, OP_LEAK),
            );
        }
        let ji = junction_info(c, &cl.members, &cl.links, &flat);
        c.hover(Rect::from_center_size(at, Vec2::splat(xf.len(22.0))), ji);
        for l in &cl.links {
            c.scene
                .emit("junction", s(&l["key"]), json!({"kind": s(&l["kind"])}));
        }
    }

    // the outer box's exposed ports
    for ld in &leaders {
        let col = proto_port_color(&ld.port);
        let m = sp(ld.mark);
        let is_t = s(&ld.port["target"]) == "transition";
        let hit = Rect::from_center_size(m, Vec2::splat(xf.len(16.0).max(14.0)));
        let cell = &cells[ld.cell];
        let z = c.port_zone(hit, &cell.id, &ld.port, |i| {
            ext_note(
                i.fact("on the outer box", "exposed: no link uses this port, so it stays on the composite's interface"),
                "Exposure is this view's reading; a go-pflow Bundle declares no outer ports (compose.go:207-227).",
            )
            .cite(cite_of("goPflow", "metamodel/compose.go", 207, 227, "Bundle: subnets, links, constraints; no ports of its own"))
        });
        let col = if z.dim {
            col.gamma_multiply(0.3)
        } else {
            col.gamma_multiply(0.75)
        };
        if is_t {
            let r = Rect::from_center_size(m, Vec2::splat(xf.len(7.0)));
            c.p.rect(
                r,
                0.0,
                Color32::WHITE,
                Stroke::new(1.4, col),
                StrokeKind::Middle,
            );
            c.zone_ring_rect(r.expand(4.0), &z);
        } else {
            c.p.circle(m, xf.len(3.5), Color32::WHITE, Stroke::new(1.4, col));
            c.zone_ring_circle(m, xf.len(3.5) + 4.0, &z);
        }
        c.scene.emit(
            "exposed",
            &key_of_scene(&cell.id, s(&ld.port["id"])),
            json!({}),
        );
    }

    // ---- legend ----
    let ly = y1 + 30.0;
    let mut lx = 14.0;
    let mut schemas: Vec<String> = vec![];
    for sub in &subs {
        for p in arr(&sub["ports"])
            .iter()
            .filter(|p| s(&p["target"]) == "place")
        {
            let sc = s(&p["schema"]).to_string();
            if !schemas.contains(&sc) {
                schemas.push(sc);
            }
        }
    }
    let item = |c: &Ctx, lx: &mut f32, ly: f32, w: f32, label: &str, draw: &dyn Fn(&Ctx, f32)| {
        draw(c, *lx);
        *lx += w;
        c.txt(xf, *lx, ly + 4.0, Al::L, label, tx(11.0, SLATE));
        *lx += tw(label, 11.0, false) + 18.0;
    };
    for sc in &schemas {
        let col = proto_schema_color(sc);
        item(c, &mut lx, ly, 20.0, sc, &|c, x| {
            c.p.circle_filled(xf.pt(x + 6.0, ly), xf.len(5.5), col);
        });
    }
    item(
        c,
        &mut lx,
        ly,
        20.0,
        "transition port (no schema)",
        &|c, x| {
            c.p.rect_filled(
                xf.rect(x + 1.0, ly - 5.0, 10.0, 10.0),
                xf.len(1.5),
                hex("#3c4656"),
            );
        },
    );
    item(c, &mut lx, ly, 20.0, "observe tap", &|c, x| {
        c.p.circle(
            xf.pt(x + 6.0, ly),
            xf.len(5.0),
            Color32::WHITE,
            Stroke::new(2.0, proto_schema_color("queue:items")),
        );
    });
    lx = 14.0;
    let ly2 = ly + 24.0;
    item(
        c,
        &mut lx,
        ly2,
        28.0,
        "event junction (one per fused class)",
        &|c, x| {
            c.line(xf.pt(x, ly2), xf.pt(x + 22.0, ly2), ev_col, 2.6);
            c.p.circle_filled(xf.pt(x + 11.0, ly2), xf.len(6.0), ev_col);
            let d = xf.len(2.8);
            let m = xf.pt(x + 11.0, ly2);
            c.line(
                m + Vec2::new(-d, -d),
                m + Vec2::new(d, d),
                Color32::WHITE,
                1.5,
            );
            c.line(
                m + Vec2::new(d, -d),
                m + Vec2::new(-d, d),
                Color32::WHITE,
                1.5,
            );
        },
    );
    item(c, &mut lx, ly2, 28.0, "token cable", &|c, x| {
        let col = kind_color("token");
        let (d, g) = dash_of("token").unwrap_or((6.0, 4.0));
        c.dash_poly(
            &[xf.pt(x, ly2), xf.pt(x + 22.0, ly2)],
            col,
            xf.len(2.2),
            xf.len(d),
            xf.len(g),
        );
        let m = xf.pt(x + 11.0, ly2);
        let r = xf.len(5.0);
        c.p.add(Shape::convex_polygon(
            vec![
                m - Vec2::new(0.0, r),
                m + Vec2::new(r, 0.0),
                m + Vec2::new(0.0, r),
                m - Vec2::new(r, 0.0),
            ],
            col,
            Stroke::NONE,
        ));
    });
    item(c, &mut lx, ly2, 28.0, "data cable", &|c, x| {
        let (d, g) = dash_of("data").unwrap_or((1.0, 4.0));
        c.dash_poly(
            &[xf.pt(x, ly2), xf.pt(x + 22.0, ly2)],
            kind_color("data"),
            xf.len(2.2),
            xf.len(d),
            xf.len(g),
        );
    });
    item(
        c,
        &mut lx,
        ly2,
        28.0,
        "guard (flagged, not a cable)",
        &|c, x| {
            let (d, g) = dash_of("guard").unwrap_or((6.0, 4.0));
            c.dash_poly(
                &[xf.pt(x, ly2), xf.pt(x + 22.0, ly2)],
                kind_color("guard"),
                xf.len(1.8),
                xf.len(d),
                xf.len(g),
            );
        },
    );
    item(
        c,
        &mut lx,
        ly2,
        26.0,
        "exposed on the outer box",
        &|c, x| {
            c.dash_poly(
                &[xf.pt(x, ly2), xf.pt(x + 14.0, ly2)],
                FAINT,
                1.0,
                xf.len(2.0),
                xf.len(3.0),
            );
            c.p.circle(
                xf.pt(x + 17.0, ly2),
                xf.len(3.8),
                Color32::WHITE,
                Stroke::new(1.4, FAINT),
            );
        },
    );
    item(
        c,
        &mut lx,
        ly2,
        26.0,
        "in (left) · out (right) · inout (both sides)",
        &|c, x| {
            let t = |a: Pos2, b: Pos2, d: Pos2| {
                c.p.add(Shape::convex_polygon(vec![a, b, d], SLATE, Stroke::NONE));
            };
            t(
                xf.pt(x + 1.0, ly2 - 3.5),
                xf.pt(x + 6.0, ly2),
                xf.pt(x + 1.0, ly2 + 3.5),
            );
            t(
                xf.pt(x + 10.0, ly2),
                xf.pt(x + 13.0, ly2 - 3.0),
                xf.pt(x + 13.0, ly2 + 3.0),
            );
            t(
                xf.pt(x + 18.0, ly2),
                xf.pt(x + 15.0, ly2 - 3.0),
                xf.pt(x + 15.0, ly2 + 3.0),
            );
        },
    );
    c.txt(
        xf,
        14.0,
        ly2 + 28.0,
        Al::L,
        "Drag from any port, in a hole or on the outer box: only the ports go-pflow's validateLink accepts light up. Colors and glyphs are go-pflow RenderDOT's (composedot.go:28-33).",
        tx(10.5, OP_MUTED),
    );

    if n_guard > 0 {
        c.note("na", "Guard links are not cables: a guard restricts one subnet on another's place (W_RESTRICTIVE_LINK) and sits outside the free-SMC fragment (ROADMAP Phase 6). It is drawn and flagged; no operadic composite is claimed.");
    }
    c.note("judgment", "Exposed ports: the outer box's own ports are every port no link uses, drawn faintly on its boundary. That choice of interface is this view's reading; a go-pflow Bundle declares subnets, links and constraints but no ports of its own (compose.go:207-227).");
    c.note("honesty", "Spivak's operad of wiring diagrams (arXiv:1305.0297, Ex. 2.1.7) is undirected: a wiring diagram is a cospan, every port is soldered onto a cable, and filling a hole is a pushout. A directed version appears there only in §6.3 (Future work), as the subject of a paper then in preparation (Spivak-Stapleton). So an event link is drawn as a junction (⊗) on one cable, not as a wire from an output to an input: it is a synchronisation (Bruni et al.), as in the cospan operad of Fong-Spivak §6.5 (Ex. 6.94), whose wiring diagrams have ports but no left and right. In / out sides are go-pflow port kinds, kept for reading; the operad does not see them.");
}

fn key_of_scene(sub: &str, port: &str) -> String {
    format!("{sub}:{port}")
}

/// A dashed ring's points.
fn circle_pts(c: Pos2, r: f32) -> Vec<Pos2> {
    (0..=32)
        .map(|i| {
            let a = std::f32::consts::TAU * i as f32 / 32.0;
            Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
        })
        .collect()
}

/// The first `frac` of a polyline by length (a path drawing itself on).
fn partial(pts: &[Pos2], frac: f32) -> Vec<Pos2> {
    let total: f32 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut left = total * frac.clamp(0.0, 1.0);
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let l = w[0].distance(w[1]);
        if l >= left {
            if l > 0.0 {
                out.push(lerp2(w[0], w[1], left / l));
            }
            return out;
        }
        left -= l;
        out.push(w[1]);
    }
    out
}

fn junction_info(c: &Ctx, members: &[(String, String)], links: &[Value], flat: &Value) -> Info {
    let names: Vec<String> = members.iter().map(|(a, b)| format!("{a}:{b}")).collect();
    let flat_name = members.first().and_then(|(sb, pt)| {
        let sub = subnets(c).into_iter().find(|x| s(&x["id"]) == sb)?;
        let t = arr(&sub["ports"]).iter().find(|p| s(&p["id"]) == pt)?["transition"]
            .as_str()?
            .to_string();
        let want = format!("{sb}/{t}");
        flat["fused_groups"]
            .as_object()?
            .iter()
            .find(|(_, ms)| arr(ms).iter().any(|m| s(m) == want))
            .map(|(k, _)| k.clone())
    });
    let mut i = info(format!("junction: {}", names.join(" = ")))
        .fact(
            "links",
            links
                .iter()
                .map(|l| {
                    format!(
                        "{}:{} — {}:{}{}",
                        s(&l["from"]["subnet"]),
                        s(&l["from"]["port"]),
                        s(&l["to"]["subnet"]),
                        s(&l["to"]["port"]),
                        if s(&l["idiom"]).is_empty() {
                            String::new()
                        } else {
                            format!(" (pat:{})", s(&l["idiom"]))
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
        )
        .fact(
            "members",
            format!(
                "{} ports, one junction (fusion is over equivalence classes)",
                names.len()
            ),
        )
        .fact(
            "flat transition",
            flat_name.unwrap_or_else(|| "(see the flat view)".into()),
        )
        .fact(
            "semantics",
            "synchronisation (Bruni et al.), not place gluing",
        );
    i.note = "One cable soldered to every transition port in a fused class: when one fires, all fire, atomically (a rendezvous). Drawn as an undirected junction, not as a wire from an output to an input.".into();
    let idioms = arr(&comp::ontology()["idioms"]).to_vec();
    for l in links {
        if let Some(idm) = idioms.iter().find(|x| s(&x["key"]) == s(&l["idiom"])) {
            let cav = s(&idm["caveat"]);
            if !cav.is_empty() {
                i = i.fact("caveat", cav);
            }
        }
    }
    i.cite(cite_of(
        "goPflow",
        "metamodel/compose_fuse.go",
        9,
        20,
        "event fusion over equivalence classes, so it is associative",
    ))
    .cite(cite_of(
        "goPflow",
        "metamodel/compose_flatten.go",
        55,
        80,
        "place and transition classes by union-find",
    ))
    .cites_of(
        &comp::ontology()["link_kinds"]["event"]["cites"]
            .as_array()
            .map(|a| json!(a.iter().take(2).collect::<Vec<_>>()))
            .unwrap_or(Value::Null),
    )
    .cite(cite_of(
        "goPflow",
        "metamodel/composedot.go",
        28,
        33,
        "linkStyles: color, style and glyph per link kind",
    ))
}

pub(super) fn operad_c4(c: &mut Ctx, area: Rect) {
    super::a_operad_c4::draw(c, area);
}

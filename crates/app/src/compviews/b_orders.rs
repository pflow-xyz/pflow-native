//! The string view, pattern level, orders example: the prototype's
//! `ordersPattern` (js/views/string.js:248-924) on egui. Pattern instances
//! are boxes side by side (⊗), place ports are typed wires (in/inout on the
//! top edge, out/inout on the bottom, observe a tap on the right edge),
//! transition ports are squares on the left (in) and right (out) edges, and
//! the links are exactly `bundle.state`'s.
//!
//! The layout is the prototype's, deterministic: longest-path rows over
//! the token links (diagram-viewer.js's layering, dv:108-169), boxes under
//! the box their token wire comes from, lanes and channels for the event
//! connectors, leads, glyph and label placement by first fit.

use super::b_draw::{dv, theme, *};
use super::*;

const ROW: f32 = 24.0;
const LANE_OUT: f32 = 32.0;
const MARGIN: f32 = 44.0;

// ------------------------------------------------------------ sources

pub(super) fn dv_cite(from: u32, to: u32, note: &str) -> String {
    format!("pflow.xyz public/diagram-viewer.js:{from}-{to} @{DV_REV} ({note})")
}
/// The pflow-xyz commit diagram-viewer.js was last changed in (its line
/// cites were re-checked against this revision's file).
pub(super) const DV_REV: &str = "954b75c";
/// "go-pflow metamodel/composedot.go:28-33 @69257b3 (note)": the lines were
/// re-read at the revision the ontology records (`repos.goPflow`).
pub(super) fn go_cite(path: &str, from: u32, to: u32, note: &str) -> String {
    let r = &comp::ontology()["repos"]["goPflow"];
    format!(
        "go-pflow {path}:{from}-{to} @{} ({note})",
        r["rev"].as_str().unwrap_or("")
    )
}
pub(super) fn road_cite(from: u32, to: u32, note: &str) -> String {
    let r = &comp::ontology()["repos"]["ontologyRoadmap"];
    format!(
        "ontology-pflow-xyz ROADMAP.md:{from}-{to} @{} ({note})",
        r["rev"].as_str().unwrap_or("")
    )
}
fn ttl_cite(from: u32, to: u32, note: &str) -> String {
    let r = &comp::ontology()["repos"]["ontology"];
    format!(
        "ontology-pflow-xyz ns/patterns.ttl:{from}-{to} @{} ({note})",
        r["rev"].as_str().unwrap_or("")
    )
}

// ------------------------------------------------------------ boxes

#[derive(Clone)]
struct PortV {
    id: String,
    kind: String,
    place: bool,
    schema: String,
    raw: Value,
}

#[derive(Clone, Copy)]
struct Cp {
    x: f32,
    y: f32,
    side: &'static str,
    port: usize,
}

struct Bx {
    id: String,
    sub: Value,
    pattern: String,
    sub_line: String,
    ports: Vec<PortV>,
    top: Vec<usize>,
    bot: Vec<usize>,
    obs: Vec<usize>,
    left: Vec<usize>,
    right: Vec<usize>,
    w: f32,
    h: f32,
    title_y: f32,
    sub_y: f32,
    rows_y: f32,
    x: f32,
    y: f32,
    pos: Vec<(String, Vec<Cp>)>,
}

fn split_id(id: &str) -> (Option<&str>, &str) {
    match id.find(':') {
        Some(i) if i > 0 => (Some(&id[..=i]), &id[i + 1..]),
        _ => (None, id),
    }
}

impl Bx {
    fn slot_w(&self, d: &D, p: usize) -> f32 {
        let p = &self.ports[p];
        let (a, b) = split_id(&p.id);
        48.0f32
            .max(d.tw(b, 9.5) + 14.0)
            .max(a.map_or(0.0, |a| d.tw(a, 8.5) + 14.0))
            .max(d.tw(&p.schema, 10.0) + 18.0)
            .ceil()
    }

    fn measure(d: &D, sub: &Value) -> Bx {
        let ports: Vec<PortV> = arr(&sub["ports"])
            .iter()
            .map(|p| PortV {
                id: s(&p["id"]).into(),
                kind: s(&p["kind"]).into(),
                place: s(&p["target"]) == "place",
                schema: s(&p["schema"]).into(),
                raw: p.clone(),
            })
            .collect();
        let idx = |f: &dyn Fn(&PortV) -> bool| -> Vec<usize> {
            (0..ports.len()).filter(|&i| f(&ports[i])).collect()
        };
        let top = idx(&|p| p.place && (p.kind == "in" || p.kind == "inout"));
        let bot = idx(&|p| p.place && (p.kind == "out" || p.kind == "inout"));
        let obs = idx(&|p| p.place && p.kind == "observe");
        let left = idx(&|p| !p.place && p.kind == "in");
        let right = idx(&|p| !p.place && p.kind != "in");
        let pattern = s(&sub["pattern"]).to_string();
        let pat_label = comp::ontology()["patterns"][&pattern]["prefLabel"]
            .as_str()
            .unwrap_or(&pattern)
            .to_string();
        let sub_line = format!("{pat_label} · {}", s(&sub["net_type"]));
        let mut b = Bx {
            id: s(&sub["id"]).into(),
            sub: sub.clone(),
            pattern,
            sub_line,
            ports,
            top,
            bot,
            obs,
            left,
            right,
            w: 0.0,
            h: 0.0,
            title_y: 0.0,
            sub_y: 0.0,
            rows_y: 0.0,
            x: 0.0,
            y: 0.0,
            pos: vec![],
        };
        let sum = |b: &Bx, l: &[usize]| l.iter().map(|&p| b.slot_w(d, p)).sum::<f32>();
        let lw = b
            .left
            .iter()
            .map(|&p| d.tw(&b.ports[p].id, 10.0))
            .fold(0.0, f32::max);
        let rw = b
            .right
            .iter()
            .map(|&p| d.tw(&b.ports[p].id, 10.0))
            .chain(
                b.obs
                    .iter()
                    .map(|&p| d.tw(&format!("{} observe", b.ports[p].id), 10.0)),
            )
            .fold(0.0, f32::max);
        b.w = 150.0f32
            .max(sum(&b, &b.top) + 24.0)
            .max(sum(&b, &b.bot) + 24.0)
            .max(lw + rw + 52.0)
            .max(d.tw(&b.id, 13.0) + 40.0)
            .max(d.tw(&b.sub_line, 10.0) + 28.0)
            .ceil();
        let two = |b: &Bx, l: &[usize]| l.iter().any(|&p| split_id(&b.ports[p].id).0.is_some());
        let top_lab = if b.top.is_empty() {
            4.0
        } else if two(&b, &b.top) {
            27.0
        } else {
            17.0
        };
        b.title_y = top_lab + 15.0;
        b.sub_y = b.title_y + 13.0;
        b.rows_y = b.sub_y + 9.0;
        let n_rows = 1usize.max(b.left.len()).max(b.right.len() + b.obs.len());
        let bot_lab = if b.bot.is_empty() {
            6.0
        } else if two(&b, &b.bot) {
            27.0
        } else {
            17.0
        };
        b.h = b.rows_y + n_rows as f32 * ROW + bot_lab + 4.0;
        b
    }

    fn place_at(&mut self, d: &D, x0: f32, y0: f32) {
        self.x = x0;
        self.y = y0;
        let mut pos: Vec<(String, Vec<Cp>)> = vec![];
        let mut add = |id: &str, cp: Cp| match pos.iter_mut().find(|(k, _)| k == id) {
            Some((_, v)) => v.push(cp),
            None => pos.push((id.to_string(), vec![cp])),
        };
        for (list, y, side) in [(&self.top, y0, "top"), (&self.bot, y0 + self.h, "bottom")] {
            let total: f32 = list.iter().map(|&p| self.slot_w(d, p)).sum();
            let mut x = x0 + (self.w - total) / 2.0;
            for &p in list.iter() {
                let sw = self.slot_w(d, p);
                add(
                    &self.ports[p].id,
                    Cp {
                        x: x + sw / 2.0,
                        y,
                        side,
                        port: p,
                    },
                );
                x += sw;
            }
        }
        for (i, &p) in self.left.iter().enumerate() {
            add(
                &self.ports[p].id,
                Cp {
                    x: x0,
                    y: y0 + self.rows_y + i as f32 * ROW + ROW / 2.0,
                    side: "left",
                    port: p,
                },
            );
        }
        for (i, &p) in self.right.iter().chain(self.obs.iter()).enumerate() {
            add(
                &self.ports[p].id,
                Cp {
                    x: x0 + self.w,
                    y: y0 + self.rows_y + i as f32 * ROW + ROW / 2.0,
                    side: "right",
                    port: p,
                },
            );
        }
        self.pos = pos;
    }

    fn copies(&self, id: &str) -> &[Cp] {
        self.pos
            .iter()
            .find(|(k, _)| k == id)
            .map_or(&[][..], |(_, v)| v.as_slice())
    }
    /// `posOf`: the copy on `prefer`'s side, else the first.
    fn pos_of(&self, id: &str, prefer: Option<&str>) -> Option<Cp> {
        let all = self.copies(id);
        prefer
            .and_then(|p| all.iter().find(|c| c.side == p))
            .or(all.first())
            .copied()
    }
    fn port_of(&self, id: &str) -> Option<&PortV> {
        self.ports.iter().find(|p| p.id == id)
    }
    fn copy_from(&self, p: &PortV) -> Option<Cp> {
        self.pos_of(
            &p.id,
            Some(if p.kind == "observe" {
                "right"
            } else {
                "bottom"
            }),
        )
    }
    fn to_copy(&self, p: &PortV) -> Option<Cp> {
        self.pos_of(
            &p.id,
            Some(if p.kind == "observe" { "right" } else { "top" }),
        )
    }
}

/// An observe tap is met at its circle, 13px out from the edge.
fn attach(c: Cp, ports: &[PortV]) -> Cp {
    if c.side == "right" && ports[c.port].place {
        Cp { x: c.x + 13.0, ..c }
    } else {
        c
    }
}

pub(super) type R4 = (f32, f32, f32, f32); // x, y, w, h

fn hits(a: R4, b: R4) -> bool {
    a.0 < b.0 + b.2 && b.0 < a.0 + a.2 && a.1 < b.1 + b.3 && b.1 < a.1 + a.3
}

// ------------------------------------------------------------ the layout

type Slide = (BTreeMap<String, (f32, f32)>, f32);

struct Link {
    kind: String,
    key: String,
    from: (String, String),
    to: (String, String),
    leak: bool,
    idiom: String,
    index: i64,
}

/// Everything the drawing needs, computed from the subnets and links alone.
struct Layout {
    boxes: Vec<Bx>,
    ids: Vec<String>,
    live: Vec<Link>,
    layer: BTreeMap<String, usize>,
    rows: Vec<Vec<usize>>,
    row_top: Vec<f32>,
    lane_of: BTreeMap<String, f32>,
    chan_of: BTreeMap<String, f32>,
    w: f32,
    h: f32,
    back_margin: f32,
    wired: BTreeSet<String>,
    obstacles: Vec<R4>,
    dom: Vec<String>,
    cod_lines: Vec<String>,
    taps_text: String,
    taps_own_line: bool,
    cod_y: f32,
    legend_y: f32,
}

impl Layout {
    fn bi(&self, id: &str) -> Option<usize> {
        self.boxes.iter().position(|b| b.id == id)
    }
    fn layer_of(&self, id: &str) -> usize {
        self.layer.get(id).copied().unwrap_or(0)
    }
    fn is_side_link(&self, l: &Link) -> bool {
        l.kind == "event"
            || (l.kind == "guard" && self.to_port(l).is_some_and(|p| p.kind == "observe"))
    }
    fn port_from(&self, l: &Link) -> Option<&PortV> {
        self.boxes[self.bi(&l.from.0)?].port_of(&l.from.1)
    }
    fn to_port(&self, l: &Link) -> Option<&PortV> {
        self.boxes[self.bi(&l.to.0)?].port_of(&l.to.1)
    }
    fn is_back_token(&self, l: &Link) -> bool {
        l.kind == "token" && self.layer_of(&l.to.0) <= self.layer_of(&l.from.0)
    }
}

fn copy_key(subnet: &str, port: &str, side: &str) -> String {
    format!("{subnet}|{port}|{side}")
}

fn mono_w(t: &str) -> f32 {
    t.chars().count() as f32 * 11.0 * 0.62
}

/// `monoW` over several strings joined, for the wrap of cod.
fn layout(d: &D, subs: &[Value], links_in: &[Value]) -> Layout {
    let mut boxes: Vec<Bx> = subs.iter().map(|sb| Bx::measure(d, sb)).collect();
    let ids: Vec<String> = boxes.iter().map(|b| b.id.clone()).collect();
    let by: BTreeMap<String, usize> = ids.iter().cloned().zip(0..).collect();
    let live: Vec<Link> = links_in
        .iter()
        .map(|l| Link {
            kind: s(&l["kind"]).into(),
            key: s(&l["key"]).into(),
            from: (s(&l["from"]["subnet"]).into(), s(&l["from"]["port"]).into()),
            to: (s(&l["to"]["subnet"]).into(), s(&l["to"]["port"]).into()),
            leak: l["leak"] == true,
            idiom: s(&l["idiom"]).into(),
            index: l["index"].as_i64().unwrap_or(0),
        })
        .filter(|l| {
            by.get(&l.from.0)
                .is_some_and(|&i| boxes[i].port_of(&l.from.1).is_some())
                && by
                    .get(&l.to.0)
                    .is_some_and(|&i| boxes[i].port_of(&l.to.1).is_some())
        })
        .collect();

    // Rows: longest-path layering over token links with DFS cycle breaking
    // (diagram-viewer.js dv:108-169). A token link into a lower row is a plain
    // wire; one that cannot point down is a back edge.
    let n = ids.len();
    let mut out_e: Vec<Vec<usize>> = vec![vec![]; n];
    for l in &live {
        if l.kind == "token" && l.from.0 != l.to.0 {
            out_e[by[&l.from.0]].push(by[&l.to.0]);
        }
    }
    let mut visited = vec![false; n];
    let mut on_stack = vec![false; n];
    let mut back_pairs: BTreeSet<(usize, usize)> = BTreeSet::new();
    fn dfs(
        u: usize,
        out_e: &[Vec<usize>],
        visited: &mut [bool],
        on_stack: &mut [bool],
        back: &mut BTreeSet<(usize, usize)>,
    ) {
        visited[u] = true;
        on_stack[u] = true;
        for &t in &out_e[u] {
            if !visited[t] {
                dfs(t, out_e, visited, on_stack, back);
            } else if on_stack[t] {
                back.insert((u, t));
            }
        }
        on_stack[u] = false;
    }
    for u in 0..n {
        if !visited[u] {
            dfs(u, &out_e, &mut visited, &mut on_stack, &mut back_pairs);
        }
    }
    let mut lv = vec![0usize; n];
    for _ in 0..64 {
        let mut changed = false;
        for u in 0..n {
            for &t in &out_e[u] {
                if back_pairs.contains(&(u, t)) {
                    continue;
                }
                if lv[u] + 1 > lv[t] {
                    lv[t] = lv[u] + 1;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    let layer: BTreeMap<String, usize> = ids.iter().cloned().zip(lv.iter().copied()).collect();
    let n_rows = lv.iter().copied().max().unwrap_or(0) + 1;
    let rows: Vec<Vec<usize>> = (0..n_rows)
        .map(|r| (0..n).filter(|&i| lv[i] == r).collect())
        .collect();
    let n_back = live
        .iter()
        .filter(|l| l.kind == "token" && lv[by[&l.to.0]] <= lv[by[&l.from.0]])
        .count();

    let max_row = rows.iter().map(Vec::len).max().unwrap_or(0) as f32;
    let g = 84.0f32.max(124.0 - 12.0 * (max_row - 3.0).max(0.0));
    let back_margin = 36.0 + 14.0 * n_back as f32;
    let row_w = |boxes: &[Bx], r: usize| -> f32 {
        rows[r].iter().map(|&i| boxes[i].w).sum::<f32>() + g * (rows[r].len() as f32 - 1.0).max(0.0)
    };
    let mut w = 960.0f32.max(row_w(&boxes, 0) + 2.0 * MARGIN + back_margin);
    let mut row_top = vec![186.0f32];
    let mut lane_of: BTreeMap<String, f32> = BTreeMap::new();
    let mut chan_of: BTreeMap<String, f32> = BTreeMap::new();
    let mut bottom_y = 0.0f32;
    let sidelink = |boxes: &[Bx], l: &Link| -> bool {
        l.kind == "event"
            || (l.kind == "guard"
                && by
                    .get(&l.to.0)
                    .and_then(|&i| boxes[i].port_of(&l.to.1))
                    .is_some_and(|p| p.kind == "observe"))
    };

    for r in 0..n_rows {
        let y0 = row_top[r];
        if r == 0 {
            let mut x = MARGIN.max((w - back_margin - row_w(&boxes, 0)) / 2.0);
            for &i in &rows[0] {
                boxes[i].place_at(d, x, y0);
                x += boxes[i].w + g;
            }
        } else {
            // Under the box the token wire comes from, so the wire runs straight down.
            let mut want: Vec<(usize, f32, usize)> = vec![];
            for (k, &i) in rows[r].iter().enumerate() {
                boxes[i].place_at(d, 0.0, y0);
                let id = boxes[i].id.clone();
                let x = live
                    .iter()
                    .find(|l| l.kind == "token" && l.to.0 == id && lv[by[&l.from.0]] < r)
                    .and_then(|l| {
                        let fb = &boxes[by[&l.from.0]];
                        let a = fb.copy_from(fb.port_of(&l.from.1)?)?;
                        let t = boxes[i].to_copy(boxes[i].port_of(&l.to.1)?)?;
                        Some(a.x - t.x)
                    })
                    .unwrap_or(f32::NEG_INFINITY);
                want.push((i, x, k));
            }
            want.sort_by(|p, q| p.1.total_cmp(&q.1).then(p.2.cmp(&q.2)));
            let mut cursor = MARGIN;
            for (i, x, _) in want {
                let x = cursor.max(x);
                boxes[i].place_at(d, x, y0);
                cursor = x + boxes[i].w + g;
            }
        }
        // Lanes: side-to-side links inside this row whose ports do not face
        // each other across one gutter.
        let row_idx: BTreeMap<&str, usize> = rows[r]
            .iter()
            .enumerate()
            .map(|(k, &i)| (ids[i].as_str(), k))
            .collect();
        let (mut k, mut lane_max) = (0usize, 0.0f32);
        for l in &live {
            if !sidelink(&boxes, l) || lv[by[&l.from.0]] != r || lv[by[&l.to.0]] != r {
                continue;
            }
            let (fb, tb) = (&boxes[by[&l.from.0]], &boxes[by[&l.to.0]]);
            let (Some(p), Some(q)) = (fb.pos_of(&l.from.1, None), tb.pos_of(&l.to.1, None)) else {
                continue;
            };
            let (ia, ib) = (row_idx[l.from.0.as_str()], row_idx[l.to.0.as_str()]);
            let direct = ia.abs_diff(ib) == 1
                && ((ia < ib && p.side == "right" && q.side == "left")
                    || (ia > ib && p.side == "left" && q.side == "right"));
            if direct {
                continue;
            }
            let ex = |c: Cp| {
                c.x + if c.side == "right" {
                    LANE_OUT
                } else {
                    -LANE_OUT
                }
            };
            let (ex1, ex2) = (ex(p), ex(q));
            let (lo, hi) = (ex1.min(ex2), ex1.max(ex2));
            let under: Vec<&Bx> = rows[r]
                .iter()
                .map(|&i| &boxes[i])
                .filter(|b| b.x < hi && b.x + b.w > lo)
                .collect();
            let base = if under.is_empty() {
                (fb.y + fb.h).max(tb.y + tb.h)
            } else {
                under.iter().map(|b| b.y + b.h).fold(0.0, f32::max)
            };
            let y = base + 30.0 + 13.0 * k as f32;
            k += 1;
            lane_of.insert(l.key.clone(), y);
            lane_max = lane_max.max(y);
        }
        let row_bottom = rows[r]
            .iter()
            .map(|&i| boxes[i].y + boxes[i].h)
            .fold(0.0, f32::max);
        // Channels: side-to-side links between this row and a lower one run
        // along a corridor below this row's boundary stubs and above the next row's.
        let (mut kc, mut chan_max) = (0usize, 0.0f32);
        for l in &live {
            if !sidelink(&boxes, l) {
                continue;
            }
            let (la, lb) = (lv[by[&l.from.0]], lv[by[&l.to.0]]);
            if la == lb || la.min(lb) != r {
                continue;
            }
            let y = (row_bottom + 60.0).max(lane_max + 20.0) + 13.0 * kc as f32;
            kc += 1;
            chan_of.insert(l.key.clone(), y);
            chan_max = chan_max.max(y);
        }
        bottom_y = (row_bottom + 44.0).max(lane_max + 16.0);
        if r + 1 < n_rows {
            row_top.push(
                (row_bottom + 120.0)
                    .max(lane_max + 90.0)
                    .max(if chan_max > 0.0 { chan_max + 58.0 } else { 0.0 }),
            );
        }
    }
    w = w.max(boxes.iter().map(|b| b.x + b.w).fold(0.0, f32::max) + back_margin + MARGIN);

    // Which drawn copies a token/data link replaces with its wire.
    let mut wired: BTreeSet<String> = BTreeSet::new();
    for l in &live {
        if l.kind != "token" && l.kind != "data" {
            continue;
        }
        let (fb, tb) = (&boxes[by[&l.from.0]], &boxes[by[&l.to.0]]);
        if let Some(p) = fb.port_of(&l.from.1).and_then(|p| fb.copy_from(p)) {
            wired.insert(copy_key(&l.from.0, &l.from.1, p.side));
        }
        if let Some(q) = tb.port_of(&l.to.1).and_then(|p| tb.to_copy(p)) {
            wired.insert(copy_key(&l.to.0, &l.to.1, q.side));
        }
    }
    // What a label or glyph must not land on.
    let mut obstacles: Vec<R4> = vec![
        (0.0, 0.0, w, 60.0),
        (-1e4, -1e4, 1e4 + 6.0, 3e4),
        (w - 6.0, -1e4, 1e4, 3e4),
    ];
    if live.iter().any(|l| l.leak) {
        obstacles.push((0.0, 0.0, w, row_top[0] - 66.0));
    }
    for b in &boxes {
        obstacles.push((b.x - 3.0, b.y - 3.0, b.w + 6.0, b.h + 6.0));
        for (pid, copies) in &b.pos {
            for c in copies {
                if !b.ports[c.port].place {
                    continue;
                }
                if c.side == "right" {
                    obstacles.push((c.x, c.y - 9.0, 24.0, 18.0));
                    continue;
                }
                if wired.contains(&copy_key(&b.id, pid, c.side)) {
                    continue;
                }
                let lw = 30.0f32.max(d.tw(&b.ports[c.port].schema, dv::LABEL)) + 6.0;
                obstacles.push(if c.side == "top" {
                    (c.x - lw / 2.0, c.y - 42.0, lw, 42.0)
                } else {
                    (c.x - lw / 2.0, c.y, lw, 42.0)
                });
            }
        }
    }

    // The composite's boundary: every copy no link wires.
    let (mut dom, mut cod, mut taps) = (vec![], vec![], vec![]);
    for r in &rows {
        for &i in r {
            let b = &boxes[i];
            for &p in &b.top {
                if !wired.contains(&copy_key(&b.id, &b.ports[p].id, "top")) {
                    dom.push(b.ports[p].schema.clone());
                }
            }
            for &p in &b.bot {
                if !wired.contains(&copy_key(&b.id, &b.ports[p].id, "bottom")) {
                    cod.push(b.ports[p].schema.clone());
                }
            }
            for &p in &b.obs {
                if !wired.contains(&copy_key(&b.id, &b.ports[p].id, "right")) {
                    taps.push(format!(
                        "{} ({}:{})",
                        b.ports[p].schema, b.id, b.ports[p].id
                    ));
                }
            }
        }
    }
    let mut cod_lines: Vec<String> = vec![];
    {
        let room = w - 2.0 * MARGIN - mono_w("cod ");
        let mut cur = String::new();
        let list: Vec<String> = if cod.is_empty() {
            vec!["I (nothing)".into()]
        } else {
            cod.clone()
        };
        for t in list {
            let next = if cur.is_empty() {
                t.clone()
            } else {
                format!("{cur} ⊗ {t}")
            };
            if !cur.is_empty() && mono_w(&next) > room {
                cod_lines.push(format!("{cur} ⊗"));
                cur = t;
            } else {
                cur = next;
            }
        }
        cod_lines.push(cur);
    }
    let taps_text = if taps.is_empty() {
        String::new()
    } else {
        format!("taps: {}", taps.join(", "))
    };
    let taps_own_line = !taps_text.is_empty()
        && mono_w(&format!("cod {}", cod_lines[cod_lines.len() - 1]))
            + 14.0
            + d.tw(&taps_text, 11.0)
            > w - 2.0 * MARGIN;
    let cod_y = bottom_y + 22.0;
    let legend_y = cod_y
        + 34.0
        + 15.0 * (cod_lines.len() as f32 - 1.0 + if taps_own_line { 1.0 } else { 0.0 });
    let h = legend_y + 62.0;
    Layout {
        boxes,
        ids,
        live,
        layer,
        rows,
        row_top,
        lane_of,
        chan_of,
        w,
        h,
        back_margin,
        wired,
        obstacles,
        dom,
        cod_lines,
        taps_text,
        taps_own_line,
        cod_y,
        legend_y,
    }
}

// ------------------------------------------------------------ info

fn pat_info(sub: &Value) -> Value {
    comp::ontology()["patterns"][s(&sub["pattern"])].clone()
}

fn subnet_info_b(b: &Bx) -> Info {
    let p = pat_info(&b.sub);
    let dom = b
        .top
        .iter()
        .map(|&i| b.ports[i].schema.clone())
        .collect::<Vec<_>>()
        .join(" ⊗ ");
    let cod = b
        .bot
        .iter()
        .map(|&i| b.ports[i].schema.clone())
        .collect::<Vec<_>>()
        .join(" ⊗ ");
    let mut i = info(format!(
        "{}: a {} instance",
        b.id,
        s(&p["prefLabel"])
    ))
    .fact("concept", format!("{} (ns:NetPattern)", s(&p["iri"])))
    .fact("constructor", s(&p["constructor"]))
    .fact("net kind", format!("{} (ns:netKind)", s(&b.sub["net_type"])))
    .fact(
        "places / transitions",
        format!(
            "{} / {}",
            arr(&b.sub["places"]).len(),
            arr(&b.sub["transitions"]).len()
        ),
    )
    .fact(
        "ports",
        b.ports
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    )
    .fact("parameters (judgment)", s(&b.sub["why"]))
    .fact(
        "as a morphism (ROADMAP Phase 6)",
        format!(
            "{} → {}",
            if dom.is_empty() { "I" } else { &dom },
            if cod.is_empty() { "I" } else { &cod }
        ),
    )
    .fact(
        "sync boundary",
        format!(
            "{} in (left), {} out (right) transition ports",
            b.left.len(),
            b.right.len()
        ),
    )
    .note(s(&p["definition"]))
    .cites_of(&p["cites"])
    .cite(road_cite(
        415,
        416,
        "wire types are port schemas; direction gives dom/cod; observe = side tap; inout on both sides",
    ))
    .cite(dv_cite(503, 526, "boxes: rx 6, white, #333 1.5"));
    if !b.obs.is_empty() {
        i = i.fact(
            "side taps",
            b.obs
                .iter()
                .map(|&p| format!("{}: {}", b.ports[p].id, b.ports[p].schema))
                .collect::<Vec<_>>()
                .join(", "),
        );
    }
    i
}

fn port_info_b(c: &Ctx, b: &Bx, p: &PortV, where_: &str) -> Info {
    let pat = s(&p.raw["pattern_port"]);
    let mut i = info(format!("port {}:{}", b.id, p.id))
        .fact("kind (ns:portKind)", p.kind.as_str())
        .fact(
            "target (ns:portTarget)",
            if p.place { "place" } else { "transition" },
        )
        .fact(
            if p.place { "place" } else { "transition" },
            if p.place {
                s(&p.raw["place"])
            } else {
                s(&p.raw["transition"])
            },
        )
        .fact(
            "schema (ns:portSchema)",
            if p.schema.is_empty() {
                "(none: transition ports carry no schema)"
            } else {
                &p.schema
            },
        )
        .fact("ontology", pat)
        .fact("drawn on", where_)
        .fact(
            "drag",
            "from here to link: the lit ports are the ones validateLink allows",
        );
    if let Some(pp) = c.port_concept(&b.id, pat) {
        i = i.note(s(&pp["comment"])).cites_of(&pp["cites"]);
    }
    i.cite(road_cite(415, 416, "wire types are port schemas"))
}

fn link_info_b(l: &Link, lay: &Layout) -> Info {
    let o = comp::ontology();
    let lk = &o["link_kinds"][&l.kind];
    let idiom = o["idioms"]
        .as_array()
        .and_then(|a| a.iter().find(|i| s(&i["key"]) == l.idiom));
    let fp = lay.port_from(l);
    let tp = lay.to_port(l);
    let joins = |p: Option<&PortV>| {
        p.map_or("?".to_string(), |p| {
            if p.place {
                s(&p.raw["place"]).to_string()
            } else {
                s(&p.raw["transition"]).to_string()
            }
        })
    };
    let mut i = info(format!(
        "{} link {}:{} → {}:{}",
        l.kind, l.from.0, l.from.1, l.to.0, l.to.1
    ))
    .fact(
        "concept",
        match idiom {
            Some(x) => format!(
                "{} (ns:CompositionIdiom, ns:linkKind lk:{})",
                s(&x["iri"]),
                l.kind
            ),
            None => format!(
                "{} (ns:PatternLink, ns:linkKind lk:{})",
                s(&lk["iri"]),
                l.kind
            ),
        },
    )
    .fact("link kind", format!("lk:{}: {}", l.kind, s(&lk["comment"])))
    .fact(
        "joins",
        format!(
            "{} ({}) and {} ({})",
            joins(fp),
            l.from.0,
            joins(tp),
            l.to.0
        ),
    )
    .fact(
        "asserted by",
        match idiom {
            Some(x) => format!(
                "{} (an idiom: a wiring the source suggests; legality is go-pflow's)",
                s(&x["assertedBy"])
            ),
            None => "drawn on this page; legality is go-pflow's".into(),
        },
    )
    .note(match idiom {
        Some(x) => format!("{} {}", s(&x["rationale"]), s(&x["caveat"])),
        None => s(&lk["comment"]).to_string(),
    });
    match l.kind.as_str() {
        "token" => {
            i = i.fact(
                "ROADMAP op",
                "op:glue: a pushout of open nets along the shared place (cospan composite)",
            )
        }
        "data" => {
            i = i.fact(
                "ROADMAP op",
                "op:tap: a read-only observation of the place",
            )
        }
        "event" => {
            i = i.fact(
                "ROADMAP op",
                "op:sync: transition fusion (Bruni et al.), not a wire",
            )
        }
        _ => {
            i = i.fact("ROADMAP op", "op:gate (flagged contextual)").fact(
                "category theory",
                "outside the free-SMC fragment (ROADMAP Phase 6): drawn and flagged, no functorial semantics claimed",
            )
        }
    }
    if let Some(x) = idiom {
        i = i.cites_of(&x["cites"]);
    }
    i.cites_of(&lk["cites"])
}

// ------------------------------------------------------------ snapping

/// One drawn copy of a port as a drag source and a drop target. Same rules as
/// `Ctx::port` (the lit ring only where `bundle.candidates` says the link
/// is legal), drawn by the caller. Returns (ring colour, dimmed).
pub(super) fn snap_port(
    c: &mut Ctx,
    hit: Rect,
    sub: &str,
    pid: &str,
    info_i: Info,
) -> (Option<Color32>, bool) {
    let key = format!("{sub}:{pid}");
    let mut ring: Option<Color32> = None;
    let mut verdict: Option<Value> = None;
    if let Some(Drag::Port {
        subnet: fs,
        port: fp,
        cands,
    }) = c.drag
    {
        if fs == sub && fp == pid {
            ring = Some(INK);
        } else if let Some(cd) = cands
            .as_ref()
            .and_then(|c| arr(&c["candidates"]).iter().find(|x| s(&x["key"]) == key))
        {
            if cd["ok"] == true {
                ring = Some(kind_color(s(&cd["link"]["kind"])));
            }
            verdict = Some(cd.clone());
        }
    }
    let dim = c.drag.is_some_and(|d| matches!(d, Drag::Port { .. })) && ring.is_none();
    let mut i = info_i;
    if let Some(v) = &verdict {
        let ok = v["ok"] == true;
        i = info(if ok {
            format!("{key}: legal ({} link)", s(&v["link"]["kind"]))
        } else {
            format!("{key}: refused")
        });
        for e in arr(&v["verdict"]["errors"]) {
            i = i.fact(s(&e["code"]), s(&e["message"]));
            if s(&e["rule"]) == "page" {
                i = i.cite("page rule (pflow-native composition.rs)");
            }
        }
        for pp in arr(&v["verdict"]["passed"]) {
            i = i
                .fact("passed", s(&pp["text"]))
                .cite(cite_label(&pp["cite"]));
        }
    }
    c.hover(hit, i);
    if c.drag_started && c.press_origin.is_some_and(|q| hit.contains(q)) {
        c.start_drag = Some(Drag::Port {
            subnet: sub.to_string(),
            port: pid.to_string(),
            cands: None,
        });
    }
    if c.released && c.pointer.is_some_and(|q| hit.contains(q)) {
        if let Some(v) = verdict.filter(|v| v["ok"] == true) {
            let l = &v["link"];
            c.command(
                "bundle.add_link",
                json!({"kind": l["kind"], "from": l["from"], "to": l["to"]}),
            );
        }
    }
    (ring, dim)
}

/// Hover on a path: the pointer within `tol` window pixels of it.
pub(super) fn hover_path(c: &mut Ctx, d: &D, pts: &[Pt], tol: f32, i: Info) {
    let Some(ptr) = c.pointer else { return };
    let v: Vec<Pos2> = pts.iter().map(|p| d.pt(*p)).collect();
    if dist_to_path(ptr, &v) <= tol {
        c.hover(Rect::from_center_size(ptr, Vec2::splat(1.0)), i);
    }
}

// ------------------------------------------------------------ drawing

pub(super) fn draw(c: &mut Ctx, area: Rect) {
    let subs = subnets(c);
    let ls = port_links(c);
    let off_port = off_port_count(c);
    let probe = D {
        p: c.p.clone(),
        xf: Xf {
            origin: area.left_top(),
            k: 1.0,
        },
    };
    let mut lay = layout(&probe, &subs, &ls);
    // the merge animation: boxes slide from where the previous state put them
    // (the layout without the link just added), the link then draws itself on
    let ev = c.anim(|e| match e {
        Event::LinkAdded { key, kind } => Some((key.clone(), kind.clone(), false)),
        Event::InstanceAdded { id } => Some((id.clone(), String::new(), true)),
        _ => None,
    });
    let mut link_t = 1.0f32; // progress of the link's own drawing
    let mut pop_t = 1.0f32;
    let mut added: Option<(String, String)> = None; // (key, kind)
    let mut slide: Option<Slide> = None;
    if let Some(((key, kind, instance), t)) = &ev {
        if *instance {
            // a new instance drops in from above
            let id = key.clone();
            if let Some(b) = lay.boxes.iter().find(|b| b.id == id) {
                let mut m = BTreeMap::new();
                m.insert(id, (b.x, b.y - 18.0));
                slide = Some((m, (*t / 0.6).min(1.0)));
            }
        } else {
            let prev_links: Vec<Value> =
                ls.iter().filter(|l| s(&l["key"]) != key).cloned().collect();
            let prev = layout(&probe, &subs, &prev_links);
            let mut m = BTreeMap::new();
            let mut moved = false;
            for b in &prev.boxes {
                if let Some(nb) = lay.boxes.iter().find(|x| x.id == b.id) {
                    if (nb.x - b.x).abs() + (nb.y - b.y).abs() > 0.5 {
                        moved = true;
                    }
                    m.insert(b.id.clone(), (b.x, b.y));
                }
            }
            let t_slide = if moved { 0.45 } else { 0.0 };
            slide = moved.then(|| (m, (*t / t_slide).min(1.0)));
            link_t = ((*t - t_slide) / (1.0 - t_slide)).clamp(0.0, 1.0);
            pop_t = ((link_t - 0.5) / 0.5).clamp(0.0, 1.0);
            added = Some((key.clone(), kind.clone()));
        }
    }
    // the slide offsets, applied to the layout's boxes for drawing
    let mut offset: BTreeMap<String, (f32, f32)> = BTreeMap::new();
    if let Some((from, p)) = &slide {
        let e = ease(*p);
        for b in &lay.boxes {
            if let Some(&(fx, fy)) = from.get(&b.id) {
                offset.insert(
                    b.id.clone(),
                    ((fx - b.x) * (1.0 - e), (fy - b.y) * (1.0 - e)),
                );
            }
        }
    }
    let k = (area.width() / lay.w).min(area.height() / lay.h).min(1.4);
    let xf = Xf {
        origin: area.left_top(),
        k,
    };
    let d = D { p: c.p.clone(), xf };
    // the whole drawing, translated per box while sliding: draw with the
    // layout as is when nothing slides, else shift the boxes (the connectors
    // follow the next frame's resting layout once the slide has run)
    if !offset.is_empty() {
        for b in &mut lay.boxes {
            if let Some(&(dx, dy)) = offset.get(&b.id) {
                let (x, y) = (b.x + dx, b.y + dy);
                b.place_at(&d, x, y);
            }
        }
    }
    paper(&d, &lay);
    head(&d, &lay, off_port);
    draw_links(c, &d, &lay, &added, link_t, pop_t);
    draw_boxes(c, &d, &lay, &added, pop_t);
    foot(c, &d, &lay);
    c.note("honesty", "Reading a pattern instance as a box dom → cod is the ROADMAP Phase 6 proposal (wire types are port schemas, port direction gives the domain and codomain). It is not a theorem: an inout port is in both, and an observe port is a tap outside both.");
    c.note("honesty", "Only the unlinked product is exactly ⊗ (disjoint union). A token link is a pushout along the shared place (Baez–Master), drawn as a wire from an out port into an in port. An event link is transition fusion (Bruni et al.), drawn as a connector between transition ports, not as a wire. The green ⊗ is RenderDOT's event glyph, not the tensor.");
    c.note("honesty", "Guard links are outside the free-SMC fragment. They are drawn dashed with ⊘ and flagged, and no functorial meaning is claimed for them.");
    c.note("honesty", "Putting transition ports on the left (in) and right (out) edges is this view's choice, after the left and right boundaries of nets with boundaries. diagram-viewer.js has no transition ports, and this view's pattern-level boxes are taller than its BOX_HEIGHT 40 so they can hold the ports.");
    if let Some(flat) = &c.d.flat {
        if flat["ok"] != true {
            let e = s(&flat["error"]).to_string();
            c.note(
                "warn",
                format!("go-pflow Flatten (pflow-compose) refuses this composition: {e}"),
            );
        }
    }
}

fn paper(d: &D, lay: &Layout) {
    d.rect(
        0.0,
        0.0,
        lay.w,
        lay.h,
        4.0,
        dv::PAPER,
        Color32::TRANSPARENT,
        0.0,
        None,
    );
}

fn head(d: &D, lay: &Layout, off_port: usize) {
    let names = lay.ids.join(" ⊗ ");
    d.text(
        (MARGIN, 28.0),
        Align2::LEFT_BOTTOM,
        &names,
        15.0,
        theme::PAPER_INK,
        false,
    );
    let counts: Vec<String> = ["event", "token", "data", "guard"]
        .iter()
        .filter_map(|k| {
            let n = lay.live.iter().filter(|l| l.kind == *k).count();
            (n > 0).then(|| format!("{n} {k} link{}", if n == 1 { "" } else { "s" }))
        })
        .collect();
    let sub = if counts.is_empty() && off_port > 0 {
        format!(
            "side by side (⊗); {off_port} link{} end{} on places or transitions, not ports, and {} not drawn here",
            if off_port == 1 { "" } else { "s" },
            if off_port == 1 { "s" } else { "" },
            if off_port == 1 { "is" } else { "are" }
        )
    } else if counts.is_empty() {
        "side by side (⊗), no links: the composite is exactly the monoidal product".to_string()
    } else {
        format!("side by side (⊗), with {}", counts.join(", "))
    };
    d.text(
        (MARGIN + d.tw(&names, 15.0) + 14.0, 28.0),
        Align2::LEFT_BOTTOM,
        &sub,
        11.0,
        theme::MUTED,
        false,
    );
    let dom = if lay.dom.is_empty() {
        "I (nothing)".to_string()
    } else {
        lay.dom.join(" ⊗ ")
    };
    d.text(
        (MARGIN, 50.0),
        Align2::LEFT_BOTTOM,
        "dom",
        11.0,
        Color32::from_gray(0x44),
        false,
    );
    d.text(
        (MARGIN + d.tw("dom ", 11.0), 50.0),
        Align2::LEFT_BOTTOM,
        &dom,
        11.0,
        Color32::from_gray(0x44),
        true,
    );
    let hint = "top-edge wires: in and inout ports (ROADMAP Phase 6 reading)";
    if mono_w(&format!("dom {dom}")) + 10.0 + d.tw(hint, 11.0) < lay.w - 2.0 * MARGIN {
        d.text(
            (MARGIN + mono_w(&format!("dom {dom}")) + 10.0, 50.0),
            Align2::LEFT_BOTTOM,
            hint,
            11.0,
            Color32::from_gray(0x88),
            false,
        );
    }
}

fn foot(c: &mut Ctx, d: &D, lay: &Layout) {
    d.text(
        (MARGIN, lay.cod_y),
        Align2::LEFT_BOTTOM,
        "cod",
        11.0,
        Color32::from_gray(0x44),
        false,
    );
    let cod_x = MARGIN + mono_w("cod ");
    for (i, t) in lay.cod_lines.iter().enumerate() {
        d.text(
            (cod_x, lay.cod_y + 15.0 * i as f32),
            Align2::LEFT_BOTTOM,
            t,
            11.0,
            Color32::from_gray(0x44),
            true,
        );
    }
    if !lay.taps_text.is_empty() {
        let last = lay.cod_lines.len() - 1;
        let (x, y) = if lay.taps_own_line {
            (cod_x, lay.cod_y + 15.0 * (last + 1) as f32)
        } else {
            (
                cod_x + mono_w(&lay.cod_lines[last]) + 14.0,
                lay.cod_y + 15.0 * last as f32,
            )
        };
        d.text(
            (x, y),
            Align2::LEFT_BOTTOM,
            &lay.taps_text,
            11.0,
            Color32::from_gray(0x88),
            false,
        );
    }
    legend(c, d, MARGIN, lay.legend_y, lay.w - 2.0 * MARGIN);
}

fn legend(c: &mut Ctx, d: &D, x0: f32, y0: f32, width: f32) {
    d.line(
        (x0, y0 - 16.0),
        (x0 + width, y0 - 16.0),
        Color32::from_rgb(0xe3, 0xe6, 0xea),
        1.0,
    );
    let col_w = width / 4.0;
    let ink = Color32::from_gray(0x44);
    let item = |i: usize, draw: &dyn Fn(f32, f32), lines: &[&str]| {
        let x = x0 + (i % 4) as f32 * col_w;
        let y = y0 + (i / 4) as f32 * 34.0;
        draw(x, y);
        for (k, t) in lines.iter().enumerate() {
            let ty = y + if lines.len() > 1 { -2.0 } else { 3.5 } + 12.0 * k as f32;
            d.text(
                (x + 34.0, ty + 3.0),
                Align2::LEFT_BOTTOM,
                t,
                10.0,
                ink,
                false,
            );
        }
    };
    item(
        0,
        &|x, y| {
            d.rect(
                x,
                y - 8.0,
                26.0,
                16.0,
                dv::RX,
                Color32::WHITE,
                dv::STROKE,
                dv::SW,
                None,
            )
        },
        &["box: a pattern instance,", "a morphism dom → cod"],
    );
    item(
        1,
        &|x, y| {
            d.line((x + 13.0, y - 12.0), (x + 13.0, y + 12.0), dv::WIRE, 1.5);
            d.dot((x + 13.0, y + 2.0), 4.6, theme::schema("resource"));
        },
        &["wire: a place port,", "labelled by its schema"],
    );
    item(
        2,
        &|x, y| {
            for dx in [4.0, 16.0] {
                d.rect(
                    x + dx,
                    y - 4.5,
                    9.0,
                    9.0,
                    0.0,
                    theme::TRANSITION_PORT,
                    Color32::TRANSPARENT,
                    0.0,
                    None,
                );
            }
        },
        &["transition port: in on the", "left edge, out on the right"],
    );
    item(
        3,
        &|x, y| {
            d.dot((x + 7.0, y), 4.6, theme::schema("account"));
            d.dot((x + 7.0, y), 1.6, Color32::WHITE);
            d.disc(
                (x + 21.0, y),
                3.6,
                Color32::WHITE,
                theme::schema("queue:items"),
                1.6,
            );
        },
        &["inout port (on both edges);", "observe tap (right edge)"],
    );
    item(
        4,
        &|x, y| {
            d.line((x, y), (x + 26.0, y), kind_color("event"), 2.6);
            glyph(d, "event", (x + 13.0, y), 7.0, 1.0);
        },
        &["event link: the transitions", "fuse (RenderDOT's ⊗)"],
    );
    item(
        5,
        &|x, y| {
            d.line((x + 13.0, y - 12.0), (x + 13.0, y + 12.0), dv::WIRE, 1.5);
            glyph(d, "token", (x + 13.0, y), 6.0, 1.0);
        },
        &["token link: one shared", "place, wired out → in"],
    );
    item(
        6,
        &|x, y| {
            d.path(
                &[(x, y), (x + 26.0, y)],
                kind_color("guard"),
                1.8,
                Some((6.0, 4.0)),
            );
            glyph(d, "guard", (x + 13.0, y), 7.0, 1.0);
        },
        &["guard link: flagged,", "outside the free SMC"],
    );
    item(
        7,
        &|x, y| {
            d.cubic(
                [
                    (x, y + 7.0),
                    (x + 18.0, y + 7.0),
                    (x + 22.0, y - 6.0),
                    (x + 26.0, y - 6.0),
                ],
                dv::BACK,
                1.5,
                Some(dv::BACK_DASH),
            );
            d.arrow((x + 26.0, y - 6.0), (x + 22.0, y - 6.0), dv::BACK);
        },
        &["back edge: feedback,", "round the right side"],
    );
    let area = d.rect_r(x0, y0 - 14.0, width, 62.0);
    c.hover(
        area,
        info("legend: diagram-viewer.js conventions, extended")
            .note("Boxes, wires, port dots and back edges follow diagram-viewer.js; the link glyphs and colours are go-pflow RenderDOT's; transition ports and taps are this view's.")
            .cite(dv_cite(503, 526, "boxes: rx 6, white, #333 1.5; port dots r 4 #666"))
            .cite(dv_cite(470, 481, "wires: #888, 1.5px"))
            .cite(dv_cite(490, 501, "back edges: dashed #c44 (6,3), round the right side, arrowhead"))
            .cite(go_cite("metamodel/composedot.go", 28, 33, "linkStyles: RenderDOT's color, style and glyph per link kind")),
    );
}

/// Token, data, event and guard links, the leak notes.
fn draw_links(
    c: &mut Ctx,
    d: &D,
    lay: &Layout,
    added: &Option<(String, String)>,
    link_t: f32,
    pop_t: f32,
) {
    let w = lay.w;
    let mut obstacles = lay.obstacles.clone();
    // Leads: where a routed connector leaves a box side.
    let mut lead_req: BTreeMap<String, BTreeMap<String, (f32, &'static str)>> = BTreeMap::new();
    for l in &lay.live {
        if !lay.is_side_link(l) {
            continue;
        }
        let (lane, chan) = (lay.lane_of.get(&l.key), lay.chan_of.get(&l.key));
        if lane.is_none() && chan.is_none() {
            continue;
        }
        let (fb, tb) = (
            &lay.boxes[lay.bi(&l.from.0).unwrap()],
            &lay.boxes[lay.bi(&l.to.0).unwrap()],
        );
        let (Some(p), Some(q)) = (fb.pos_of(&l.from.1, None), tb.pos_of(&l.to.1, None)) else {
            continue;
        };
        let up = lay.layer_of(&l.from.0).min(lay.layer_of(&l.to.0));
        let dirp = if lane.is_some() || lay.layer_of(&l.from.0) == up {
            "down"
        } else {
            "up"
        };
        let dirq = if lane.is_some() || lay.layer_of(&l.to.0) == up {
            "down"
        } else {
            "up"
        };
        for (sub, cp, port, dir) in [(&l.from.0, p, &l.from.1, dirp), (&l.to.0, q, &l.to.1, dirq)] {
            lead_req
                .entry(format!("{sub}|{}", cp.side))
                .or_default()
                .entry(port.clone())
                .or_insert((cp.y, dir));
        }
    }
    let mut lead_idx: BTreeMap<String, usize> = BTreeMap::new();
    for (k, m) in &lead_req {
        let mut ups: Vec<_> = m.iter().filter(|(_, v)| v.1 == "up").collect();
        let mut downs: Vec<_> = m.iter().filter(|(_, v)| v.1 == "down").collect();
        ups.sort_by(|a, b| a.1 .0.total_cmp(&b.1 .0).then(a.0.cmp(b.0)));
        downs.sort_by(|a, b| b.1 .0.total_cmp(&a.1 .0).then(a.0.cmp(b.0)));
        for (i, (pid, _)) in ups.into_iter().chain(downs).enumerate() {
            lead_idx.insert(format!("{k}|{pid}"), i);
        }
    }
    let mut lead_x = |sub: &str, cp: Cp, port: &str| -> f32 {
        let k = format!("{sub}|{}|{port}", cp.side);
        if !lead_idx.contains_key(&k) {
            let pre = format!("{sub}|{}|", cp.side);
            let n = lead_idx.keys().filter(|x| x.starts_with(&pre)).count();
            lead_idx.insert(k.clone(), n);
        }
        let off = LANE_OUT + 10.0 * (lead_idx[&k].min(3) as f32);
        cp.x + if cp.side == "right" { off } else { -off }
    };

    let mut verticals: Vec<(f32, f32, f32)> = vec![];
    let mut glyph_at: BTreeMap<String, Pt> = BTreeMap::new();
    let mut guards: Vec<usize> = vec![];
    let n_live = lay.live.len();

    // Where a glyph sits on a horizontal run (the prototype's glyphOnRun).
    let glyph_box = |x: f32, y: f32| -> R4 { (x - 10.0, y - 10.0, 20.0, 20.0) };
    let glyph_on_run = |lay: &Layout,
                        obstacles: &[R4],
                        r: usize,
                        x1: f32,
                        x2: f32,
                        y: f32,
                        prefer: f32,
                        fallback: f32|
     -> f32 {
        let (lo, hi) = (x1.min(x2) + 10.0, x1.max(x2) - 10.0);
        let mut rb: Vec<&Bx> = lay.rows[r].iter().map(|&i| &lay.boxes[i]).collect();
        rb.sort_by(|p, q| p.x.total_cmp(&q.x));
        let mut gutters: Vec<f32> = rb
            .windows(2)
            .map(|w2| (w2[0].x + w2[0].w + w2[1].x) / 2.0)
            .filter(|&x| x > lo && x < hi)
            .collect();
        gutters.sort_by(|a, b| {
            (a - prefer)
                .abs()
                .total_cmp(&(b - prefer).abs())
                .then(a.total_cmp(b))
        });
        let mut xs = vec![];
        for x in gutters.iter().copied().chain([fallback]) {
            for dx in [0.0, 24.0, -24.0, 48.0, -48.0] {
                if x + dx >= lo - 10.0 && x + dx <= hi + 10.0 {
                    xs.push(x + dx);
                }
            }
        }
        xs.into_iter()
            .find(|&x| !obstacles.iter().any(|o| hits(glyph_box(x, y), *o)))
            .unwrap_or_else(|| gutters.first().copied().unwrap_or(fallback))
    };

    let wires_first = |kind: &str| kind == "token" || kind == "data";
    // pass 1: token and data wires (under the boxes), events; guards after
    for li in 0..n_live {
        let (fb_i, tb_i) = (
            lay.bi(&lay.live[li].from.0).unwrap(),
            lay.bi(&lay.live[li].to.0).unwrap(),
        );
        let kind = lay.live[li].kind.clone();
        let key = lay.live[li].key.clone();
        let fb = &lay.boxes[fb_i];
        let tb = &lay.boxes[tb_i];
        let (fp, tp) = (lay.live[li].from.1.clone(), lay.live[li].to.1.clone());
        let (Some(fpv), Some(tpv)) = (fb.port_of(&fp), tb.port_of(&tp)) else {
            continue;
        };
        let is_new = added.as_ref().is_some_and(|(k, _)| *k == key);
        let prog = if is_new { link_t } else { 1.0 };
        let col = kind_color(&kind);
        let info_l = link_info_b(&lay.live[li], lay);
        if wires_first(&kind) {
            let (Some(p0), Some(q0)) = (fb.copy_from(fpv), tb.to_copy(tpv)) else {
                continue;
            };
            let (p, q) = (attach(p0, &fb.ports), attach(q0, &tb.ports));
            if kind == "token" && lay.is_back_token(&lay.live[li]) {
                let i = lay
                    .live
                    .iter()
                    .take(li)
                    .filter(|l| lay.is_back_token(l))
                    .count();
                let right_x = w - 22.0 - 14.0 * i as f32;
                let below = if p.side == "bottom" {
                    p.y + 18.0 + 6.0 * i as f32
                } else {
                    p.y
                };
                let above = if q.side == "top" {
                    lay.row_top[lay.layer_of(&tb.id)] - 52.0 - 8.0 * i as f32
                } else {
                    q.y
                };
                let pts = ortho(
                    &[
                        (p.x, p.y),
                        (p.x, below),
                        (right_x, below),
                        (right_x, above),
                        (q.x, above),
                        (q.x, q.y),
                    ],
                    14.0,
                );
                let drawn = trim_path(&pts, prog);
                d.path(&drawn, dv::PAPER, 5.0, None);
                d.path(&drawn, dv::BACK, 1.5, Some(dv::BACK_DASH));
                if prog >= 1.0 {
                    let n = pts.len();
                    d.arrow(pts[n - 1], pts[n - 2], dv::BACK);
                    d.halo(
                        (right_x - 5.0, (below + above) / 2.0 - 3.0),
                        Align2::RIGHT_BOTTOM,
                        &format!("◆ {}", fpv.schema),
                        dv::LABEL,
                        dv::BACK,
                    );
                }
                let inf = info_l
                    .fact("drawn as", "a back edge: the target is not below the source, so the wire goes round the right side, dashed red (diagram-viewer.js:490-501)")
                    .cite(dv_cite(490, 501, "back edges: dashed #c44 (6,3), routed around the right side, arrowhead"));
                hover_path(c, d, &pts, 6.0, inf);
            } else {
                let cub = if p.side == "bottom" && q.side == "top" {
                    let my = (p.y + q.y) / 2.0;
                    [(p.x, p.y), (p.x, my), (q.x, my), (q.x, q.y)]
                } else {
                    cubic_between((p.x, p.y), p.side, (q.x, q.y), q.side, 50.0)
                };
                let pts = flatten(cub, 24);
                let (mx, my) = bez(cub, 0.5);
                let stroke = if kind == "token" { dv::WIRE } else { col };
                let dash = (kind == "data").then_some((1.0, 4.0));
                let drawn = trim_path(&pts, prog);
                d.path(&drawn, stroke, 1.5, dash);
                if prog >= 1.0 {
                    let sch = if fpv.schema.is_empty() {
                        &tpv.schema
                    } else {
                        &fpv.schema
                    };
                    d.text(
                        (mx + 12.0, my + 3.5),
                        Align2::LEFT_BOTTOM,
                        sch,
                        dv::LABEL,
                        dv::LABEL_FILL,
                        false,
                    );
                    glyph(d, &kind, (mx, my), 6.5, pop_scale(is_new, pop_t));
                    let inf = info_l
                        .fact(
                            "drawn as",
                            if kind == "token" {
                                "a wire from the out port's bottom stub into the in port's top stub, labelled by the shared schema (the place is the object it carries)"
                            } else {
                                "a dotted read-only tap, RenderDOT's ▷"
                            },
                        )
                        .cite(dv_cite(470, 481, "wires: #888, 1.5px, vertical cubics; 10px #666 label at the midpoint"))
                        .fact("remove", "click the glyph");
                    let gr = d.rect_r(mx - 9.0, my - 9.0, 18.0, 18.0);
                    c.hover(gr, inf.clone());
                    click_remove(c, gr, lay.live[li].index);
                    hover_path(c, d, &pts, 6.0, inf);
                }
                obstacles.push(glyph_box(mx, my));
            }
            continue;
        }
        if kind == "guard" {
            guards.push(li);
            continue;
        }
        // event: a bold green connector between two transition ports
        let (Some(p), Some(q)) = (fb.pos_of(&fp, None), tb.pos_of(&tp, None)) else {
            continue;
        };
        let l = &lay.live[li];
        let route = side_route(lay, &mut lead_x, l, p, q, &obstacles, &glyph_on_run);
        let (da, db) = trim_two(&route.pts, prog);
        if prog >= 1.0 {
            d.path(&route.pts, dv::PAPER, 7.0, None);
        }
        d.path(&da, col, 2.6, None);
        d.path(&db, col, 2.6, None);
        if prog >= 1.0 {
            glyph(
                d,
                "event",
                (route.gx, route.gy),
                8.5,
                pop_scale(is_new, pop_t),
            );
        }
        let flat_name = flat_member(c, &l.from.0, fpv);
        let inf = info_l
            .fact("flat transition", flat_name)
            .fact(
                "drawn as",
                format!(
                    "a bold green connector with RenderDOT's ⊗ (composedot.go:31), ROADMAP Slice B{}",
                    if lay.chan_of.contains_key(&key) { "; between rows it runs along the channel between them" } else { "" }
                ),
            )
            .cite(road_cite(476, 482, "Slice B: diagram-viewer.js conventions; event fusion drawn with RenderDOT's ⊗"))
            .fact("remove", "click the glyph");
        let gr = d.rect_r(route.gx - 10.0, route.gy - 10.0, 20.0, 20.0);
        c.hover(gr, inf.clone());
        if prog >= 1.0 {
            click_remove(c, gr, l.index);
        }
        hover_path(c, d, &route.pts, 6.0, inf);
        glyph_at.insert(key.clone(), (route.gx, route.gy));
        obstacles.push(glyph_box(route.gx, route.gy));
        if let Some(runs) = &route.runs {
            run_obstacles(runs, &mut obstacles, &mut verticals);
        }
    }

    // Guard links: dashed, RenderDOT's ⊘, flagged.
    for &li in &guards {
        let l = &lay.live[li];
        let (fb, tb) = (
            &lay.boxes[lay.bi(&l.from.0).unwrap()],
            &lay.boxes[lay.bi(&l.to.0).unwrap()],
        );
        let (Some(fpv), Some(tpv)) = (fb.port_of(&l.from.1), tb.port_of(&l.to.1)) else {
            continue;
        };
        let is_new = added.as_ref().is_some_and(|(k, _)| *k == l.key);
        let prog = if is_new { link_t } else { 1.0 };
        let col = kind_color("guard");
        let Some(p) = fb.pos_of(&fpv.id, None) else {
            continue;
        };
        let want = if tpv.kind == "observe" {
            "right"
        } else {
            "top"
        };
        let Some(q0) = tb.pos_of(&tpv.id, Some(want)) else {
            continue;
        };
        let q = attach(q0, &tb.ports);
        let (pts, samples, runs): (Vec<Pt>, Vec<Pt>, Option<Vec<Pt>>);
        if lay.is_side_link(l) {
            let r = side_route(lay, &mut lead_x, l, p, q, &obstacles, &glyph_on_run);
            let smp = r.samples.clone();
            pts = r.pts;
            samples = smp;
            runs = r.runs;
        } else {
            let mut cub = cubic_between((p.x, p.y), p.side, (q.x, q.y), q.side, 60.0);
            if q.side == "top" || q.side == "bottom" {
                let sg = if p.x == q.x {
                    1.0
                } else {
                    (p.x - q.x).signum()
                };
                cub[2].0 += sg * 70.0f32.min((p.x - q.x).abs() / 2.0);
            }
            let inside = |x: f32, y: f32| {
                lay.boxes.iter().any(|b| {
                    x > b.x + 2.0 && x < b.x + b.w - 2.0 && y > b.y + 2.0 && y < b.y + b.h - 2.0
                })
            };
            let crosses = (0..17).any(|i| {
                let (x, y) = bez(cub, 0.1 + i as f32 * 0.05);
                inside(x, y)
            });
            if !crosses {
                pts = flatten(cub, 24);
                samples = [0.5, 0.42, 0.58, 0.34, 0.66, 0.26, 0.74, 0.2, 0.8]
                    .iter()
                    .map(|&t| bez(cub, t))
                    .collect();
                runs = None;
            } else {
                let ex = lead_x(&l.from.0, p, &l.from.1);
                let top = q.side == "top";
                let y_r = if top { q.y - 54.0 } else { q.y + 54.0 };
                let sg = if ex - q.x >= 0.0 { 1.0f32 } else { -1.0 };
                let nb = tb
                    .pos
                    .iter()
                    .flat_map(|(_, v)| v.iter())
                    .filter(|c2| c2.side == q.side && (c2.x - q.x).signum() == sg && c2.x != q.x)
                    .min_by(|a, b| (a.x - q.x).abs().total_cmp(&(b.x - q.x).abs()));
                let qx = nb.map_or(q.x + sg * 34.0, |n| (q.x + n.x) / 2.0);
                let y_in = q.y + if top { -12.0 } else { 12.0 };
                let r = vec![(p.x, p.y), (ex, p.y), (ex, y_r), (qx, y_r), (qx, y_in)];
                let mut v = ortho(&r, 9.0);
                v.push((q.x + sg * 4.0, y_in));
                v.push((q.x, q.y));
                pts = v;
                samples = run_samples(&r);
                runs = Some(r);
            }
        }
        // the label: one line where it fits, else three short, else the glyph alone
        const TS: f32 = 9.5;
        let label = "guard: contextual (flagged)";
        let one = (vec![label.to_string()], d.tw(label, TS) * 1.08, 12.0);
        let three_l = vec![
            "guard".to_string(),
            "contextual".to_string(),
            "(flagged)".to_string(),
        ];
        let three_w = three_l.iter().map(|t| d.tw(t, TS)).fold(0.0, f32::max) * 1.08;
        let three = (three_l, three_w, 34.0);
        struct Cand {
            rects: Vec<R4>,
            x: f32,
            y: f32,
            lines: Option<Vec<String>>,
            tx: f32,
            ty: f32,
            anchor: Align2,
        }
        let mut cands: Vec<Cand> = vec![];
        for lw in [&one, &three] {
            for &(x, y) in &samples {
                let gb = glyph_box(x, y);
                let mut mk = |lx: f32, ly: f32, a: u8| {
                    let rx = match a {
                        0 => lx,
                        1 => lx - lw.1,
                        _ => lx - lw.1 / 2.0,
                    };
                    cands.push(Cand {
                        rects: vec![gb, (rx, ly, lw.1, lw.2)],
                        x,
                        y,
                        lines: Some(lw.0.clone()),
                        tx: lx,
                        ty: ly + 9.0,
                        anchor: match a {
                            0 => Align2::LEFT_BOTTOM,
                            1 => Align2::RIGHT_BOTTOM,
                            _ => Align2::CENTER_BOTTOM,
                        },
                    });
                };
                mk(x + 12.0, y - lw.2 / 2.0, 0);
                mk(x - 12.0, y - lw.2 / 2.0, 1);
                mk(x, y + 11.0, 2);
                mk(x, y - 11.0 - lw.2, 2);
            }
        }
        let own: Vec<R4> = match &runs {
            Some(r) => r
                .windows(2)
                .map(|w2| {
                    (
                        w2[0].0.min(w2[1].0) - 2.0,
                        w2[0].1.min(w2[1].1) - 2.0,
                        (w2[1].0 - w2[0].0).abs() + 4.0,
                        (w2[1].1 - w2[0].1).abs() + 4.0,
                    )
                })
                .collect(),
            None => samples
                .iter()
                .map(|&(x, y)| (x - 3.0, y - 3.0, 6.0, 6.0))
                .collect(),
        };
        let first = samples.first().copied().unwrap_or((p.x, p.y));
        let pick = cands.into_iter().find(|cd| {
            !obstacles.iter().any(|o| hits(cd.rects[0], *o))
                && !cd.rects[1..]
                    .iter()
                    .any(|r| obstacles.iter().chain(own.iter()).any(|o| hits(*r, *o)))
        });
        let (gx, gy) = pick.as_ref().map_or(first, |p| (p.x, p.y));
        let drawn = trim_path(&pts, prog);
        d.path(&drawn, dv::PAPER, 6.0, None);
        d.path(&drawn, col, 1.8, Some((6.0, 4.0)));
        if prog >= 1.0 {
            glyph(d, "guard", (gx, gy), 8.5, pop_scale(is_new, pop_t));
            if let Some(Cand {
                lines: Some(ls_),
                tx,
                ty,
                anchor,
                ..
            }) = &pick
            {
                for (i, t) in ls_.iter().enumerate() {
                    d.halo((*tx, *ty + 11.0 * i as f32 + 3.0), *anchor, t, TS, col);
                }
            }
        }
        let inf = link_info_b(l, lay)
            .fact("drawn as", "dashed, RenderDOT's ⊘, from the gated transition to the observed place")
            .note("A guard link is outside the free-SMC fragment (ROADMAP Phase 6, which cites the book's Appendix E and pflow-jl's ContextualArcError): it is drawn and flagged, and no functorial semantics is claimed for it.")
            .cite(road_cite(405, 413, "what the categorical reading is: guard outside the free SMC"))
            .fact("remove", "click the glyph");
        let gr = d.rect_r(gx - 10.0, gy - 10.0, 20.0, 20.0);
        c.hover(gr, inf.clone());
        click_remove(c, gr, l.index);
        hover_path(c, d, &pts, 6.0, inf);
        glyph_at.insert(l.key.clone(), (gx, gy));
        if let Some(pk) = &pick {
            for r in &pk.rects {
                obstacles.push(*r);
            }
        }
        if let Some(r) = &runs {
            run_obstacles(r, &mut obstacles, &mut verticals);
        }
    }

    // Leak notes: the idiom caveat, verbatim, in red on its spider.
    let mut leak_n = 0;
    for l in lay.live.iter().filter(|l| l.leak) {
        let Some(&(gx, gy)) = glyph_at.get(&l.key) else {
            continue;
        };
        let idiom = comp::ontology()["idioms"]
            .as_array()
            .and_then(|a| a.iter().find(|i| s(&i["key"]) == l.idiom))
            .cloned()
            .unwrap_or(Value::Null);
        let leak_col = theme::LEAK;
        if leak_n == 0 {
            let lines = wrap(
                d,
                &format!("pool leak: {}", s(&idiom["caveat"])),
                500.0,
                10.0,
            );
            let ny = lay.row_top[0] - 100.0 - (lines.len() as f32 - 3.0) * 12.0;
            let nx = gx.max(MARGIN + 262.0).min(w - lay.back_margin - 262.0);
            let ly1 = ny + lines.len() as f32 * 12.0 - 4.0;
            let lx = [0.0f32, 26.0, -26.0, 52.0, -52.0]
                .iter()
                .map(|dx| gx + dx)
                .find(|&x| {
                    x > nx - 250.0
                        && x < nx + 250.0
                        && !verticals
                            .iter()
                            .any(|v| (v.0 - x).abs() < 6.0 && v.2 > ly1 && v.1 < gy - 10.0)
                })
                .unwrap_or(gx);
            d.path(
                &[(lx, ly1), (gx, gy - 10.0)],
                leak_col,
                1.0,
                Some((2.0, 2.0)),
            );
            d.rect(
                nx - 260.0,
                ny - 12.0,
                520.0,
                lines.len() as f32 * 12.0 + 8.0,
                4.0,
                hex("#fdf0ee"),
                leak_col,
                0.8,
                None,
            );
            for (i, t) in lines.iter().enumerate() {
                d.text(
                    (nx, ny + 12.0 * i as f32 + 3.0),
                    Align2::CENTER_BOTTOM,
                    t,
                    10.0,
                    leak_col,
                    false,
                );
            }
            d.text(
                (nx + 260.0, ny - 16.0 + 3.0),
                Align2::RIGHT_BOTTOM,
                &format!("pat:{} · patterns.ttl:493-498", l.idiom),
                8.5,
                leak_col,
                false,
            );
            let r = d.rect_r(
                nx - 260.0,
                ny - 12.0,
                520.0,
                lines.len() as f32 * 12.0 + 8.0,
            );
            c.hover(r, leak_info(l, lay, &idiom));
        } else {
            let h = 15.0;
            let mut cands: Vec<(R4, String)> = vec![];
            for text in ["pool leak: the same caveat", "pool leak"] {
                let tw_ = d.tw(text, 9.0) + 12.0;
                for (x, y) in [
                    (gx + 8.0, gy - 26.0),
                    (gx - 8.0 - tw_, gy - 26.0),
                    (gx + 8.0, gy + 11.0),
                    (gx - 8.0 - tw_, gy + 11.0),
                    (gx - tw_ / 2.0, gy - 28.0),
                    (gx - tw_ / 2.0, gy + 13.0),
                    (gx + 13.0, gy - h / 2.0),
                    (gx - 13.0 - tw_, gy - h / 2.0),
                ] {
                    cands.push(((x, y, tw_, h), text.to_string()));
                }
            }
            let pick = cands
                .iter()
                .find(|(r, _)| !obstacles.iter().any(|o| hits(*r, *o)))
                .or_else(|| cands.iter().find(|(_, t)| t == "pool leak"))
                .cloned()
                .unwrap();
            let (r4, text) = pick;
            d.rect(
                r4.0,
                r4.1,
                r4.2,
                r4.3,
                7.5,
                hex("#fdf0ee"),
                leak_col,
                0.8,
                None,
            );
            d.text(
                (r4.0 + r4.2 / 2.0, r4.1 + 10.5 + 2.0),
                Align2::CENTER_BOTTOM,
                &text,
                9.0,
                leak_col,
                false,
            );
            obstacles.push(r4);
            c.hover(d.rect_r(r4.0, r4.1, r4.2, r4.3), leak_info(l, lay, &idiom));
        }
        leak_n += 1;
    }
}

fn leak_info(l: &Link, lay: &Layout, idiom: &Value) -> Info {
    let mut i = link_info_b(l, lay);
    i.title = format!("caveat on {}", i.title);
    i.note = "The page's refund scenario (the `refund` scenario of bundle.scenarios) expects this: the worker never returns to the pool. ROADMAP Phase 4 names it as the expected failure of the rollback scenario.".into();
    i.cites.insert(
        0,
        ttl_cite(
            493,
            498,
            "pat:idiom-pool-release-saga-commit and its caveat",
        ),
    );
    i.cites.insert(
        1,
        road_cite(
            240,
            242,
            "4.1: the pool-leak caveat shows up on the rollback path, the expected result",
        ),
    );
    i.cites.insert(
        2,
        road_cite(
            276,
            278,
            "worked expectation: the rollback scenario fails, the worker never returns",
        ),
    );
    i.lines
        .push(("caveat".into(), s(&idiom["caveat"]).to_string()));
    i
}

fn pop_scale(is_new: bool, pop_t: f32) -> f32 {
    if is_new {
        // scale 0 -> 1.45 (at 70%) -> 1
        let t = pop_t;
        if t < 0.7 {
            1.45 * t / 0.7
        } else {
            1.45 - 0.45 * (t - 0.7) / 0.3
        }
    } else {
        1.0
    }
}

fn click_remove(c: &mut Ctx, r: Rect, index: i64) {
    if c.clicked && c.pointer.is_some_and(|p| r.contains(p)) && c.drag.is_none() {
        c.command("bundle.remove_link", json!({"index": index}));
    }
}

fn flat_member(c: &Ctx, subnet: &str, p: &PortV) -> String {
    let Some(flat) = &c.d.flat else {
        return "(flatten refused)".into();
    };
    if flat["ok"] != true {
        return "(flatten refused)".into();
    }
    flat["transition_map"][subnet][s(&p.raw["transition"])]
        .as_str()
        .unwrap_or("(flatten refused)")
        .to_string()
}

/// The first `t` (0..1) of a path by length.
fn trim_path(pts: &[Pt], t: f32) -> Vec<Pt> {
    if t >= 1.0 || pts.len() < 2 {
        return pts.to_vec();
    }
    let total: f32 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
    let mut left = total * t.max(0.0);
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let l = dist(w[0], w[1]);
        if left >= l {
            out.push(w[1]);
            left -= l;
        } else {
            let f = if l > 0.0 { left / l } else { 0.0 };
            out.push((
                w[0].0 + (w[1].0 - w[0].0) * f,
                w[0].1 + (w[1].1 - w[0].1) * f,
            ));
            break;
        }
    }
    out
}

/// An event link draws from both ends towards the middle: its first and last
/// `t/2` by length.
fn trim_two(pts: &[Pt], t: f32) -> (Vec<Pt>, Vec<Pt>) {
    if t >= 1.0 {
        return (pts.to_vec(), vec![]);
    }
    let a = trim_path(pts, t / 2.0);
    let mut rev: Vec<Pt> = pts.to_vec();
    rev.reverse();
    (a, trim_path(&rev, t / 2.0))
}

fn dist(a: Pt, b: Pt) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Points along an orthogonal route to try a glyph at: longest run first,
/// from its middle outwards.
fn run_samples(pts: &[Pt]) -> Vec<Pt> {
    let mut segs: Vec<(Pt, Pt, f32)> = pts
        .windows(2)
        .map(|w| (w[0], w[1], dist(w[0], w[1])))
        .filter(|s| s.2 > 24.0)
        .collect();
    segs.sort_by(|a, b| b.2.total_cmp(&a.2));
    segs.iter()
        .flat_map(|(a, b, _)| {
            [0.5f32, 0.35, 0.65, 0.2, 0.8, 0.1, 0.9]
                .iter()
                .map(move |&t| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t))
        })
        .collect()
}

fn run_obstacles(pts: &[Pt], obstacles: &mut Vec<R4>, verticals: &mut Vec<(f32, f32, f32)>) {
    for w in pts.windows(2) {
        let ((x1, y1), (x2, y2)) = (w[0], w[1]);
        if (x1 - x2).abs() < 0.5 && (y1 - y2).abs() > 0.5 {
            verticals.push((x1, y1.min(y2), y1.max(y2)));
        }
        obstacles.push((
            x1.min(x2) - 2.0,
            y1.min(y2) - 2.0,
            (x2 - x1).abs() + 4.0,
            (y2 - y1).abs() + 4.0,
        ));
    }
}

struct Route {
    pts: Vec<Pt>,
    gx: f32,
    gy: f32,
    runs: Option<Vec<Pt>>,
    samples: Vec<Pt>,
}

type GlyphOnRun<'a> = dyn Fn(&Layout, &[R4], usize, f32, f32, f32, f32, f32) -> f32 + 'a;

/// The route of a side-to-side link: along its channel between rows, along its
/// lane under the row, straight across one gutter, or a cubic between facing ports.
fn side_route(
    lay: &Layout,
    lead_x: &mut dyn FnMut(&str, Cp, &str) -> f32,
    l: &Link,
    p: Cp,
    q: Cp,
    obstacles: &[R4],
    glyph_on_run: &GlyphOnRun,
) -> Route {
    let (lane_y, chan_y) = (lay.lane_of.get(&l.key), lay.chan_of.get(&l.key));
    if lane_y.is_some() || chan_y.is_some() {
        let y = *chan_y.or(lane_y).unwrap();
        let ex1 = lead_x(&l.from.0, p, &l.from.1);
        let ex2 = lead_x(&l.to.0, q, &l.to.1);
        let pts = vec![
            (p.x, p.y),
            (ex1, p.y),
            (ex1, y),
            (ex2, y),
            (ex2, q.y),
            (q.x, q.y),
        ];
        let gx = if chan_y.is_some() {
            let up = lay.layer_of(&l.from.0).min(lay.layer_of(&l.to.0));
            glyph_on_run(
                lay,
                obstacles,
                up,
                ex1,
                ex2,
                y,
                (ex1 + ex2) / 2.0,
                if lay.layer_of(&l.from.0) == up {
                    ex1
                } else {
                    ex2
                },
            )
        } else {
            glyph_on_run(
                lay,
                obstacles,
                lay.layer_of(&l.to.0),
                ex1,
                ex2,
                y,
                ex2,
                (ex1 + ex2) / 2.0,
            )
        };
        let mut samples = vec![(gx, y)];
        samples.extend(run_samples(&pts));
        return Route {
            pts: ortho(&pts, 9.0),
            gx,
            gy: y,
            runs: Some(pts),
            samples,
        };
    }
    if lay.layer_of(&l.from.0) == lay.layer_of(&l.to.0) && p.y == q.y {
        let g = ((p.x + q.x) / 2.0, p.y);
        return Route {
            pts: vec![(p.x, p.y), (q.x, q.y)],
            gx: g.0,
            gy: g.1,
            runs: None,
            samples: vec![g],
        };
    }
    let cub = cubic_between((p.x, p.y), p.side, (q.x, q.y), q.side, 50.0);
    let (gx, gy) = bez(cub, 0.5);
    Route {
        pts: flatten(cub, 24),
        gx,
        gy,
        runs: None,
        samples: [0.5, 0.42, 0.58, 0.34, 0.66]
            .iter()
            .map(|&t| bez(cub, t))
            .collect(),
    }
}

fn draw_boxes(c: &mut Ctx, d: &D, lay: &Layout, added: &Option<(String, String)>, pop_t: f32) {
    let new_ends: Vec<(String, String)> = added
        .as_ref()
        .and_then(|(k, _)| lay.live.iter().find(|l| l.key == *k))
        .map(|l| vec![l.from.clone(), l.to.clone()])
        .unwrap_or_default();
    let new_kind = added.as_ref().map(|(_, k)| k.clone());
    for b in &lay.boxes {
        // boundary stubs: a port no token/data link wires is a wire of the composite's boundary
        for (pid, copies) in &b.pos {
            for cp in copies {
                let port = &b.ports[cp.port];
                if !port.place || cp.side == "right" {
                    continue;
                }
                if lay.wired.contains(&copy_key(&b.id, pid, cp.side)) {
                    continue;
                }
                let inn = cp.side == "top";
                let (y1, y2) = if inn {
                    (cp.y - 40.0, cp.y)
                } else {
                    (cp.y, cp.y + 40.0)
                };
                d.line((cp.x, y1), (cp.x, y2), dv::WIRE, 1.5);
                d.halo(
                    (cp.x, y1 + (y2 - y1) / 2.0 - 3.0 + 3.0),
                    Align2::CENTER_BOTTOM,
                    &port.schema,
                    dv::LABEL,
                    dv::LABEL_FILL,
                );
                let hit = d.rect_r(cp.x - 5.0, y1, 10.0, y2 - y1);
                let mut i = port_info_b(c, b, port, "");
                i.title = format!("wire {}: {}:{}", port.schema, b.id, pid);
                i = i
                    .fact(
                        "drawn as",
                        format!(
                            "a boundary wire {}; its label is the port schema (ROADMAP: wire types are port schemas)",
                            if inn { "entering the top edge (an input: part of dom)" } else { "leaving the bottom edge (an output: part of cod)" }
                        ),
                    )
                    .cite(dv_cite(if inn { 441 } else { 528 }, if inn { 468 } else { 543 }, if inn { "boundary inputs enter from the top" } else { "boundary outputs exit at the bottom" }));
                c.hover(hit, i);
            }
        }
        // the box body
        let flash = new_ends.iter().any(|(s_, _)| *s_ == b.id);
        let (stroke, sw) = if flash {
            let col = kind_color(new_kind.as_deref().unwrap_or("event"));
            let e = (pop_t * std::f32::consts::PI).sin();
            (lerp_c(dv::STROKE, col, e), 1.5 + 1.5 * e)
        } else {
            (dv::STROKE, dv::SW)
        };
        d.rect(b.x, b.y, b.w, b.h, dv::RX, Color32::WHITE, stroke, sw, None);
        d.rect(
            b.x + 1.0,
            b.y + b.title_y - 14.0,
            b.w - 2.0,
            31.0,
            0.0,
            theme::pattern_tint(&b.pattern).gamma_multiply(0.7),
            Color32::TRANSPARENT,
            0.0,
            None,
        );
        d.text(
            (b.x + b.w / 2.0, b.y + b.title_y + 4.0),
            Align2::CENTER_BOTTOM,
            &b.id,
            12.5,
            dv::STROKE,
            false,
        );
        d.text(
            (b.x + b.w / 2.0, b.y + b.sub_y + 3.0),
            Align2::CENTER_BOTTOM,
            &b.sub_line,
            10.0,
            dv::LABEL_FILL,
            false,
        );
        c.scene.emit("box", &b.id, json!({}));
        c.hover(d.rect_r(b.x, b.y, b.w, b.h), subnet_info_b(b));
        // port labels inside the box
        for (pid, copies) in &b.pos {
            for cp in copies {
                let p = &b.ports[cp.port];
                if p.place && (cp.side == "top" || cp.side == "bottom") {
                    let (pre, rest) = split_id(pid);
                    let y_a = if cp.side == "top" {
                        b.y + 12.0
                    } else {
                        b.y + b.h - if pre.is_some() { 17.0 } else { 6.0 }
                    };
                    let y0 = if pre.is_some() {
                        y_a
                    } else if cp.side == "top" {
                        b.y + 14.0
                    } else {
                        y_a
                    };
                    if let Some(pre) = pre {
                        d.text(
                            (cp.x, y0 + 3.0),
                            Align2::CENTER_BOTTOM,
                            pre,
                            8.5,
                            Color32::from_gray(0x88),
                            false,
                        );
                        d.text(
                            (cp.x, y0 + 11.0 + 3.0),
                            Align2::CENTER_BOTTOM,
                            rest,
                            9.5,
                            Color32::from_gray(0x44),
                            false,
                        );
                    } else {
                        d.text(
                            (cp.x, y0 + 3.0),
                            Align2::CENTER_BOTTOM,
                            rest,
                            9.5,
                            Color32::from_gray(0x44),
                            false,
                        );
                    }
                } else if cp.side == "left" {
                    d.text(
                        (b.x + 11.0, cp.y + 3.5 + 3.0),
                        Align2::LEFT_BOTTOM,
                        pid,
                        10.0,
                        dv::STROKE,
                        false,
                    );
                } else {
                    let obs = p.kind == "observe";
                    if obs {
                        let wobs = d.tw(" observe", 8.5);
                        d.text(
                            (b.x + b.w - 11.0 - wobs, cp.y + 3.5 + 3.0),
                            Align2::RIGHT_BOTTOM,
                            pid,
                            10.0,
                            dv::STROKE,
                            false,
                        );
                        d.text(
                            (b.x + b.w - 11.0, cp.y + 3.5 + 3.0),
                            Align2::RIGHT_BOTTOM,
                            " observe",
                            8.5,
                            Color32::from_gray(0x88),
                            false,
                        );
                    } else {
                        d.text(
                            (b.x + b.w - 11.0, cp.y + 3.5 + 3.0),
                            Align2::RIGHT_BOTTOM,
                            pid,
                            10.0,
                            dv::STROKE,
                            false,
                        );
                    }
                }
            }
        }
    }
    // ports: above the links, so a connector never covers one; each copy is a snap target
    for b in &lay.boxes {
        for (pid, copies) in &b.pos {
            for cp in copies {
                let p = &b.ports[cp.port];
                let at = (cp.x, cp.y);
                let where_ = match (cp.side, p.place) {
                    ("top", _) => "top edge: an input (dom)",
                    ("bottom", _) => "bottom edge: an output (cod)",
                    ("left", _) => "left edge: an in transition port (sync boundary)",
                    (_, false) => "right edge: an out transition port (sync boundary)",
                    _ => "right edge: a side tap (observe), outside dom and cod",
                };
                let where_s = if p.kind == "inout" {
                    format!("{where_}; inout ports are drawn on both edges (ROADMAP)")
                } else {
                    where_.to_string()
                };
                let hit_c = if p.place && cp.side == "right" {
                    (cp.x + 13.0, cp.y)
                } else {
                    at
                };
                let hit = Rect::from_center_size(d.pt(hit_c), Vec2::splat(18.0 * d.k().max(0.7)));
                let i = port_info_b(c, b, p, &where_s);
                let (ring, dim) = snap_port(c, hit, &b.id, &p.id, i);
                let col = theme::port(!p.place, &p.schema);
                let col = if dim { col.gamma_multiply(0.3) } else { col };
                let pulse = new_ends.iter().any(|(s_, pt)| *s_ == b.id && pt == pid);
                let ps = if pulse {
                    1.0 + (pop_t * std::f32::consts::PI).sin()
                } else {
                    1.0
                };
                if !p.place {
                    d.rect(
                        cp.x - 4.5 * ps,
                        cp.y - 4.5 * ps,
                        9.0 * ps,
                        9.0 * ps,
                        0.0,
                        col,
                        Color32::WHITE,
                        1.0,
                        None,
                    );
                } else if cp.side == "right" {
                    d.line((cp.x, cp.y), (cp.x + 10.0, cp.y), col, 1.5);
                    d.disc((cp.x + 13.0, cp.y), 3.6 * ps, Color32::WHITE, col, 1.6);
                    d.dot((cp.x, cp.y), 2.4, col);
                } else {
                    d.disc((cp.x, cp.y), 4.6 * ps, col, Color32::WHITE, 1.0);
                    if p.kind == "inout" {
                        d.dot((cp.x, cp.y), 1.6, Color32::WHITE);
                    }
                }
                if let Some(rc) = ring {
                    d.ring(hit_c, 9.0, rc, 2.5);
                }
                if p.place {
                    // one wire per place port (D10), whatever its copies
                    c.scene.emit(
                        "wire",
                        &format!("{}:{}", b.id, pid),
                        json!({"schema": p.schema}),
                    );
                }
            }
        }
    }
    // fusion: one per link, every kind (D10)
    for l in &lay.live {
        c.scene.emit("fusion", &l.key, json!({}));
    }
}

fn lerp_c(a: Color32, b: Color32, t: f32) -> Color32 {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(f(a.r(), b.r()), f(a.g(), b.g()), f(a.b(), b.b()))
}

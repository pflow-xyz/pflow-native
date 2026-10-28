//! The process view (see `docs/composition-views-scene.md`), after the
//! prototype's `js/views/process.js`: a run as a morphism, every token a wire
//! through the script's steps, each firing a box from its preset to its
//! postset, time left to right. Orders: a scenario's script fired on the flat
//! net (`bundle.run`), the leaked worker as a wire that never comes back.
//! Connect-4: the fixed four-drop opening (`plan.opening`).
//!
//! Which steps fire, why one does not, and the marking after each are
//! pflow-metamodel's; `c_process.rs` assigns token identities and lays the
//! wires out. Nothing here fires anything.

#![allow(clippy::too_many_arguments)]

use std::collections::{BTreeMap, BTreeSet};

use super::c_draw::*;
use super::c_process::*;
use super::*;

const W: f32 = 1150.0;
const PAD: f32 = 16.0;
const LANE: f32 = 28.0;
const BACK: Color32 = Color32::from_rgb(0x1e, 0x7a, 0x46);

fn go(path: &str, a: u32, b: u32, note: &str) -> String {
    let o = comp::ontology();
    let rev = o["repos"]["goPflow"]["rev"].as_str().unwrap_or("");
    let lines = if b != a {
        format!(":{a}-{b}")
    } else {
        format!(":{a}")
    };
    format!("go-pflow {path}{lines} @{rev} ({note})")
}

fn ttl(a: u32, note: &str) -> String {
    let o = comp::ontology();
    let rev = o["repos"]["ontology"]["rev"].as_str().unwrap_or("");
    format!("ontology-pflow-xyz ns/patterns.ttl:{a} @{rev} ({note})")
}

fn rs(path: &str, a: u32, b: u32, note: &str) -> String {
    rs_cite(path, a, b, note)
}

struct Cites;
#[allow(non_snake_case)]
impl Cites {
    fn arcs() -> String {
        go(
            "metamodel/firing.go",
            79,
            120,
            "Inputs, Outputs, Tests: a box's preset, postset and read arcs",
        )
    }
    fn why_not() -> String {
        go(
            "metamodel/firing.go",
            140,
            196,
            "EnabledWhyNot: the reason quoted where a run stops",
        )
    }
    fn capacity() -> String {
        go(
            "metamodel/firing.go",
            172,
            194,
            "capacity: a post-firing bound on the places a firing raises; not part of its preset",
        )
    }
    fn fire() -> String {
        go(
            "metamodel/firing.go",
            198,
            212,
            "Fire: the box takes its preset and gives its postset",
        )
    }
    fn fifo_not() -> String {
        ttl(
            201,
            "queue caveat: the net does not order items, so FIFO identity is this view's choice",
        )
    }
    fn anon() -> String {
        ttl(251, "pool caveat: in_use is an anonymous count")
    }
    fn one_case() -> String {
        ttl(429, "saga caveat: one saga is one case")
    }
    fn leak() -> String {
        ttl(
            493,
            "pat:idiom-pool-release-saga-commit and its pool-leak caveat (:496)",
        )
    }
    fn no_either() -> String {
        ttl(434, "saga caveat: no port fires on either outcome")
    }
    fn game_auto() -> String {
        rs(
            "src/game.rs",
            3,
            11,
            "automatic transitions fire as soon as enabled; every other transition is a move",
        )
    }
    fn settle() -> String {
        rs(
            "src/game.rs",
            279,
            279,
            "settle: automatic transitions, lowest index first, until none is enabled",
        )
    }
    fn by_name() -> String {
        rs(
            "tests/common/mod.rs",
            5,
            8,
            "the fixtures name their parts by convention; the tests declare them",
        )
    }
    fn game() -> String {
        rs("tests/common/mod.rs", 18, 40, "game(): auto = name contains \"win\", x_ moves are +1, reward win_x - win_o, settled start")
    }
    fn fixture() -> String {
        rs(
            "tests/fixtures/connect-4.json",
            0,
            0,
            "examples/connect-4.json is a byte-identical copy",
        )
    }
}

fn with_cites(mut i: Info, cs: &[String]) -> Info {
    for c in cs {
        if !i.cites.contains(c) {
            i.cites.push(c.clone());
        }
    }
    i
}

// ------------------------------------------------------------ spec

#[derive(Clone)]
struct Span {
    text: String,
    color: Color32,
    size: f32,
    bold: bool,
}

fn sp(text: &str, color: Color32) -> Span {
    Span {
        text: text.into(),
        color,
        size: 10.0,
        bold: false,
    }
}

fn spb(text: &str, color: Color32) -> Span {
    Span {
        text: text.into(),
        color,
        size: 10.0,
        bold: true,
    }
}

#[derive(Clone)]
struct Mark {
    color: Color32,
    under: bool,
    tag: Option<String>,
    tag_color: Option<Color32>,
}

struct Ghost {
    header: Vec<Vec<Span>>,
    info: Info,
}

struct Spec {
    proc: Proc,
    vis: BTreeSet<usize>,
    idle: BTreeSet<usize>,
    place_ix: BTreeMap<String, i64>,
    group: BTreeMap<String, i64>,
    colors: BTreeMap<String, Color32>,
    default_color: Color32,
    fused_places: BTreeSet<String>,
    fused_box: Vec<bool>,
    box_lines: Vec<Vec<(String, Color32)>>,
    headers: Vec<Vec<Vec<Span>>>,
    box_info: Vec<Info>,
    wire_info: Vec<Info>,
    boundary_info: [Info; 2],
    ghost: Option<Ghost>,
    marks: BTreeMap<usize, Mark>,
    right_extra: f32,
    caption_r: String,
}

impl Spec {
    fn color_of(&self, place: &str) -> Color32 {
        self.colors
            .get(place)
            .copied()
            .unwrap_or(self.default_color)
    }
}

fn split_place(id: &str) -> (String, String) {
    match id.rfind('/') {
        Some(i) => (id[..=i].to_string(), id[i + 1..].to_string()),
        None => (String::new(), id.to_string()),
    }
}

fn tw_(t: &str, size: f32) -> f32 {
    t.chars().count() as f32 * size * 0.58
}

fn twfit(t: &str, size: f32) -> f32 {
    t.chars().count() as f32 * size * 0.56
}

fn row_tail(r: &Need) -> String {
    match r.kind {
        NeedKind::Inhibitor => format!("needs < {}, has {}", r.need, r.have),
        NeedKind::Read => format!("reads {}, has {}", r.need, r.have),
        NeedKind::Capacity => format!("gives {}: {}→{}, cap {}", r.delta, r.have, r.after, r.need),
        NeedKind::Take => format!("takes {}, has {}", r.need, r.have),
    }
}

fn wrap_to(t: &str, max: usize) -> Vec<String> {
    // break long tokens after '+' or '/', never inside a word
    let mut words: Vec<String> = vec![];
    for w in t.split_whitespace() {
        if w.chars().count() <= max {
            words.push(w.into());
        } else {
            let mut cur = String::new();
            for part in w.split_inclusive(['+', '/']) {
                if !cur.is_empty() && cur.chars().count() + part.chars().count() > max {
                    words.push(std::mem::take(&mut cur));
                }
                cur.push_str(part);
            }
            if !cur.is_empty() {
                words.push(cur);
            }
        }
    }
    let mut lines = vec![];
    let mut line = String::new();
    for w in words {
        if line.is_empty() {
            line = w;
        } else if line.chars().count() + 1 + w.chars().count() <= max {
            line.push(' ');
            line.push_str(&w);
        } else {
            lines.push(std::mem::take(&mut line));
            line = w;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

// ------------------------------------------------------------ geometry

struct Lay {
    sim: Sim,
    xs: Vec<f32>,
    bw: Vec<f32>,
    xl: f32,
    xr: f32,
    right_w: f32,
    ghost_w: f32,
    header_top: f32,
    header_h: f32,
    lanes_top: f32,
    lanes_bottom: f32,
    ys_a: Vec<BTreeMap<usize, f32>>,
    ys_b: Vec<BTreeMap<usize, f32>>,
    ghost: Option<GhostGeo>,
    bottom: f32,
}

struct GhostGeo {
    gx: f32,
    gy: f32,
    h: f32,
    why: Vec<String>,
}

impl Lay {
    fn x_left(&self, e: usize, n: usize) -> f32 {
        if e == n + 1 {
            self.xr
        } else {
            self.xs[e]
        }
    }
    fn x_right(&self, e: usize) -> f32 {
        if e == 0 {
            self.xl
        } else {
            self.xs[e] + self.bw[e]
        }
    }
}

fn layout(spec: &Spec, top: f32) -> Lay {
    let p = &spec.proc;
    let n = p.boxes.len();
    let many = |t: &Token| p.serial.get(&t.place).copied().unwrap_or(0) > 1;
    let ghost_ids: BTreeSet<usize> = p
        .stop
        .as_ref()
        .map(|s| {
            s.needs
                .iter()
                .flat_map(|r| r.tokens.iter().copied())
                .filter(|t| spec.vis.contains(t))
                .collect()
        })
        .unwrap_or_default();
    let min_lanes: Vec<usize> = std::iter::once(0)
        .chain(
            spec.box_lines
                .iter()
                .map(|l| ((l.len() as f32 * 12.0 + 10.0) / LANE).ceil() as usize),
        )
        .collect();
    let pix = |t: &Token| spec.place_ix.get(&t.place).copied().unwrap_or(0);
    let grp = |t: &Token| spec.group.get(&t.place).copied().unwrap_or(9);
    let sim = choose_layout(p, &spec.vis, &spec.idle, &grp, &pix, &min_lanes, &ghost_ids);
    let label_w = |t: &Token, size: f32| {
        let (pre, local) = split_place(&t.place);
        twfit(&pre, size - 1.5)
            + twfit(&local, size)
            + if many(t) {
                twfit(&format!(" #{}", t.n), size - 1.5)
            } else {
                0.0
            }
    };
    let initials: Vec<&Token> = sim.a[0].iter().map(|i| &p.tokens[*i]).collect();
    let finals: Vec<&Token> = sim.b[n].iter().map(|i| &p.tokens[*i]).collect();
    let mark_w = |t: &Token| {
        spec.marks
            .get(&t.id)
            .and_then(|m| m.tag.as_ref())
            .map_or(0.0, |g| tw_(g, 9.0))
    };
    let left_w = initials
        .iter()
        .map(|t| label_w(t, 10.5))
        .fold(70.0, f32::max)
        + 16.0;
    let right_w = finals
        .iter()
        .map(|t| label_w(t, 10.5).max(mark_w(t)))
        .fold(70.0, f32::max)
        + 16.0;
    let ghost_w = p.stop.as_ref().map_or(0.0, |s| {
        s.needs
            .iter()
            .map(|r| {
                let (pre, local) = split_place(&r.place);
                22.0 + twfit("✓ ", 10.0)
                    + twfit(&pre, 8.5)
                    + twfit(&local, 10.0)
                    + twfit(&format!("  {}", row_tail(r)), 10.0)
                    + 10.0
            })
            .fold(0.0, f32::max)
            .clamp(258.0, 330.0)
    });
    let reserved = if ghost_w > 0.0 { ghost_w + 36.0 } else { 0.0 }
        + if spec.right_extra > 0.0 {
            spec.right_extra + 20.0
        } else {
            0.0
        };
    let avail = W - 2.0 * PAD - left_w - right_w - reserved;
    let mut bw = vec![0.0f32];
    for l in &spec.box_lines {
        bw.push((l.iter().map(|x| tw_(&x.0, 9.5)).fold(0.0, f32::max) + 12.0).clamp(56.0, 170.0));
    }
    let mid_label_w = |t: &Token| {
        twfit(
            &format!(
                "{}{}",
                split_place(&t.place).1,
                if many(t) {
                    format!(" #{}", t.n)
                } else {
                    String::new()
                }
            ),
            9.0,
        )
    };
    let mut need: Vec<f32> = vec![];
    for e in 0..=n {
        let born: Vec<&Token> = p
            .tokens
            .iter()
            .filter(|t| spec.vis.contains(&t.id) && t.born == e && e > 0 && t.died.is_some())
            .collect();
        need.push(
            born.iter()
                .map(|t| mid_label_w(t) + 14.0)
                .fold(if e == 0 || e == n { 44.0 } else { 56.0 }, f32::max),
        );
    }
    let mut gaps = need.clone();
    let fixed: f32 = bw.iter().sum();
    let sum_need: f32 = gaps.iter().sum();
    if n == 0 {
        gaps = vec![avail.clamp(120.0, 220.0)];
    } else if fixed + sum_need > avail {
        gaps = gaps
            .iter()
            .map(|x| x * (avail - fixed) / sum_need)
            .collect();
    } else {
        let target = avail.min(fixed + (n as f32 + 1.0) * 150.0);
        let mut extra = target - fixed - sum_need;
        // raise the narrowest gaps first, so the columns even out
        let mut guard = 0;
        while extra > 0.5 && guard < 1000 {
            guard += 1;
            let lo = gaps.iter().copied().fold(f32::MAX, f32::min);
            let at: Vec<usize> = gaps
                .iter()
                .enumerate()
                .filter(|(_, x)| **x <= lo + 0.01)
                .map(|(i, _)| i)
                .collect();
            let next = gaps
                .iter()
                .copied()
                .filter(|x| *x > lo + 0.01)
                .fold(f32::MAX, f32::min);
            let step = (extra / at.len() as f32).min(next - lo);
            for i in at.iter() {
                gaps[*i] += step;
            }
            extra -= step * at.len() as f32;
        }
    }
    let span = fixed + gaps.iter().sum::<f32>();
    let slack = (avail - span).max(0.0);
    let xl = PAD + left_w + slack / 2.0;
    let mut xs = vec![xl];
    for e in 1..=n {
        xs.push(xs[e - 1] + bw[e - 1] + gaps[e - 1]);
    }
    let xr = if n > 0 {
        xs[n] + bw[n] + gaps[n]
    } else {
        xl + gaps[0]
    };
    // vertical
    let header_h = spec
        .headers
        .iter()
        .map(|h| 14.0 + h.len() as f32 * 12.0)
        .fold(28.0, f32::max);
    let header_top = top;
    let lanes_top = header_top + header_h + 16.0;
    let mut ys_a = vec![];
    let mut ys_b = vec![];
    let mut lanes = 1usize;
    for (e, list) in sim.a.iter().enumerate() {
        let blk = (e >= 1).then(|| {
            (
                sim.blocks[e].p,
                sim.blocks[e].h,
                sim.blocks[e].out_block.len(),
            )
        });
        let (y, l) = lane_ys(list, blk);
        lanes = lanes.max(l);
        ys_a.push(y);
    }
    for (e, list) in sim.b.iter().enumerate() {
        let blk = (e < n).then(|| {
            (
                sim.blocks[e + 1].p,
                sim.blocks[e + 1].h,
                sim.blocks[e + 1].in_block.len(),
            )
        });
        let (y, l) = lane_ys(list, blk);
        lanes = lanes.max(l);
        ys_b.push(y);
    }
    let abs = |m: &BTreeMap<usize, f32>| -> BTreeMap<usize, f32> {
        m.iter().map(|(k, v)| (*k, lanes_top + v * LANE)).collect()
    };
    let ys_a: Vec<BTreeMap<usize, f32>> = ys_a.iter().map(abs).collect();
    let ys_b: Vec<BTreeMap<usize, f32>> = ys_b.iter().map(abs).collect();
    // the refused next box, past the codomain
    let mut ghost = None;
    if let (Some(stop), Some(g)) = (&p.stop, &spec.ghost) {
        let gx = xr + right_w + 28.0;
        let chars = ((ghost_w - 20.0) / (10.0 * 0.58)).floor() as usize;
        let why = wrap_to(&stop.why, chars.max(10));
        let rows = stop.needs.len();
        let rows_at =
            30.0 + g.header.len() as f32 * 12.0 + 6.0 + 12.0 + why.len() as f32 * 12.0 + 8.0 + 14.0;
        let h = rows_at + (rows.max(1) as f32 - 1.0) * 16.0 + 14.0;
        let present: Vec<f32> = ghost_ids
            .iter()
            .filter_map(|id| ys_b[n].get(id).copied())
            .collect();
        let mid = if present.is_empty() {
            lanes_top + lanes as f32 * LANE / 2.0
        } else {
            present.iter().sum::<f32>() / present.len() as f32
        };
        let gy = (lanes_top - 24.0).max(mid - rows_at - (rows.max(1) as f32 - 1.0) * 16.0 / 2.0);
        lanes = lanes.max(((gy + h - lanes_top) / LANE).ceil() as usize);
        ghost = Some(GhostGeo { gx, gy, h, why });
    }
    let lanes_bottom = lanes_top + lanes as f32 * LANE;
    Lay {
        sim,
        xs,
        bw,
        xl,
        xr,
        right_w,
        ghost_w,
        header_top,
        header_h,
        lanes_top,
        lanes_bottom,
        ys_a,
        ys_b,
        ghost,
        bottom: lanes_bottom + 14.0 + 22.0,
    }
}

// ------------------------------------------------------------ painting

/// Draws spans left to right from `x`; returns the width in design units.
fn spans(
    c: &Ctx,
    xf: &Xf,
    x: f32,
    y: f32,
    al: Align2,
    ss: &[Span],
    halo_bg: Option<Color32>,
) -> f32 {
    let total: f32 = ss.iter().map(|s| tw(c, xf, &s.text, s.size, false)).sum();
    let mut cx = match al {
        Align2::CENTER_CENTER | Align2::CENTER_TOP | Align2::CENTER_BOTTOM => x - total / 2.0,
        Align2::RIGHT_CENTER | Align2::RIGHT_TOP | Align2::RIGHT_BOTTOM => x - total,
        _ => x,
    };
    for s in ss {
        if let Some(bg) = halo_bg {
            halo(
                c,
                xf,
                cx,
                y,
                Align2::LEFT_CENTER,
                &s.text,
                s.size,
                s.color,
                bg,
                false,
            );
        } else if s.bold {
            dbold(
                c,
                xf,
                cx,
                y,
                Align2::LEFT_CENTER,
                &s.text,
                s.size,
                s.color,
                false,
            );
        } else {
            dtext(c, xf, cx, y, Align2::LEFT_CENTER, &s.text, s.size, s.color);
        }
        cx += tw(c, xf, &s.text, s.size, false);
    }
    total
}

struct Painted {
    box_x: BTreeMap<String, Pos2>,
}

fn token_label(spec: &Spec, t: &Token, local_only: bool) -> Vec<Span> {
    let (pre, local) = split_place(&t.place);
    let many = spec.proc.serial.get(&t.place).copied().unwrap_or(0) > 1;
    let mark = spec.marks.get(&t.id);
    let color = mark.map_or_else(|| spec.color_of(&t.place), |m| m.color);
    let mut v = vec![];
    if !local_only && !pre.is_empty() {
        v.push(Span {
            text: pre,
            color: P_MUTED,
            size: 9.0,
            bold: false,
        });
    }
    v.push(Span {
        text: local,
        color,
        size: if local_only { 9.0 } else { 10.5 },
        bold: mark.is_some(),
    });
    if many {
        v.push(Span {
            text: format!(" #{}", t.n),
            color: P_MUTED,
            size: 9.0,
            bold: false,
        });
    }
    v
}

fn paint(
    c: &mut Ctx,
    xf: &Xf,
    spec: &Spec,
    lay: &Lay,
    sweep: Option<f32>,
    fuse: Option<(&str, Color32, f32)>,
) -> Painted {
    let p = &spec.proc;
    let n = p.boxes.len();
    let sim = &lay.sim;
    let (xl, xr) = (lay.xl, lay.xr);
    let mut painted = Painted {
        box_x: BTreeMap::new(),
    };
    // boundaries
    let boundary = |c: &mut Ctx, x: f32, caption: &str, sub: &str, i: &Info, anchor_end: bool| {
        let (a, b) = (
            xf.pt(x, lay.lanes_top - 6.0),
            xf.pt(x, lay.lanes_bottom + 2.0),
        );
        c.line(a, b, P_INK, 1.5);
        let (tx, al) = if anchor_end {
            (x + 4.0, Align2::RIGHT_CENTER)
        } else {
            (x - 4.0, Align2::LEFT_CENTER)
        };
        let (tx, al) = if anchor_end {
            (tx, al)
        } else {
            (x + 4.0 - 8.0, al)
        };
        dbold(
            c,
            xf,
            tx,
            lay.lanes_top - 26.0,
            al,
            caption,
            11.0,
            P_INK,
            false,
        );
        dtext(c, xf, tx, lay.lanes_top - 13.0, al, sub, 10.0, P_MUTED);
        c.hover(
            Rect::from_min_max(Pos2::new(a.x - 6.0, a.y), Pos2::new(b.x + 6.0, b.y)),
            i.clone(),
        );
    };
    boundary(
        c,
        xl,
        "domain",
        "initial marking",
        &spec.boundary_info[0],
        true,
    );
    boundary(
        c,
        xr,
        "codomain",
        &spec.caption_r,
        &spec.boundary_info[1],
        false,
    );
    // step headers and guides
    for (i, b) in p.boxes.iter().enumerate() {
        let e = i + 1;
        let x = lay.xs[e] + lay.bw[e] / 2.0;
        if sweep.is_some_and(|s| x > s) {
            continue;
        }
        dtext(
            c,
            xf,
            x,
            lay.header_top + 10.0,
            Align2::CENTER_CENTER,
            &format!("STEP {}", b.k),
            10.0,
            P_MUTED,
        );
        let mut hr = Rect::NOTHING;
        for (j, line) in spec.headers[i].iter().enumerate() {
            let tot: f32 = line.iter().map(|s| tw(c, xf, &s.text, s.size, false)).sum();
            spans(
                c,
                xf,
                x,
                lay.header_top + 24.0 + j as f32 * 12.0,
                Align2::CENTER_CENTER,
                line,
                None,
            );
            hr = hr.union(Rect::from_center_size(
                xf.pt(x, lay.header_top + 24.0 + j as f32 * 12.0),
                Vec2::new(tot * xf.k + 8.0, 12.0 * xf.k),
            ));
        }
        c.hover(
            hr.union(Rect::from_center_size(
                xf.pt(x, lay.header_top + 10.0),
                Vec2::new(50.0 * xf.k, 12.0 * xf.k),
            )),
            spec.box_info[i].clone(),
        );
        let blk = &sim.blocks[e];
        dpoly(
            c,
            &[
                xf.pt(x, lay.header_top + lay.header_h + 2.0),
                xf.pt(x, lay.lanes_top + blk.p as f32 * LANE - 6.0),
            ],
            hex("#d5dae0"),
            1.0,
            (2.0, 3.0),
        );
    }
    // wires
    let wire_path = |t: &Token| -> Vec<Pos2> {
        let a = t.born;
        let b = t.died.unwrap_or(n + 1);
        let ya = |e: usize| lay.ys_a[e].get(&t.id).copied();
        let yb = |e: usize| lay.ys_b[e].get(&t.id).copied();
        let mut pts: Vec<(f32, f32)> = vec![(lay.x_right(a), ya(a).unwrap_or(0.0))];
        for g in a..b {
            let (x0, y0, x1, y1) = (
                lay.x_right(g),
                ya(g).unwrap_or(0.0),
                lay.x_left(g + 1, n),
                yb(g).unwrap_or(0.0),
            );
            if (y0 - y1).abs() < 0.5 {
                pts.push((x1, y1));
            } else {
                let (s0, s1) = (x0 + 6.0, x1 - 6.0);
                let m = (s1 - s0) / 2.0;
                pts.push((s0, y0));
                let cub = {
                    let steps = 14;
                    (1..=steps)
                        .map(|i| {
                            let t = i as f32 / steps as f32;
                            let u = 1.0 - t;
                            let (c1, c2) = ((s0 + m, y0), (s1 - m, y1));
                            (
                                u * u * u * s0
                                    + 3.0 * u * u * t * c1.0
                                    + 3.0 * u * t * t * c2.0
                                    + t * t * t * s1,
                                u * u * u * y0
                                    + 3.0 * u * u * t * c1.1
                                    + 3.0 * u * t * t * c2.1
                                    + t * t * t * y1,
                            )
                        })
                        .collect::<Vec<_>>()
                };
                pts.extend(cub);
                pts.push((x1, y1));
            }
            if g + 1 < b {
                let y2 = ya(g + 1).unwrap_or(y1);
                if (y2 - y1).abs() < 0.5 {
                    pts.push((lay.x_right(g + 1), y1));
                } else {
                    let cx = lay.xs[g + 1] + lay.bw[g + 1] / 2.0;
                    pts.push((cx, y1));
                    pts.push((cx, y2));
                    pts.push((lay.x_right(g + 1), y2));
                }
            }
        }
        pts.into_iter()
            .filter(|(x, _)| sweep.is_none_or(|s| *x <= s))
            .map(|(x, y)| xf.pt(x, y))
            .collect()
    };
    let drawn: Vec<&Token> = p
        .tokens
        .iter()
        .filter(|t| spec.vis.contains(&t.id))
        .collect();
    // under-glow first
    for t in &drawn {
        if let Some(m) = spec.marks.get(&t.id) {
            if m.under {
                let pts = wire_path(t);
                poly(c, &pts, m.color.gamma_multiply(0.16), 10.0 * xf.k.max(0.8));
            }
        }
    }
    for t in &drawn {
        let pts = wire_path(t);
        if pts.len() < 2 {
            continue;
        }
        let color = spec.color_of(&t.place);
        let is_idle = spec.idle.contains(&t.id);
        let mark = spec.marks.get(&t.id);
        let stroke = mark.map_or(color, |m| m.color);
        if spec.fused_places.contains(&t.place) {
            poly(c, &pts, stroke, 4.6 * xf.k.max(0.8));
            poly(c, &pts, PAPER, 1.5 * xf.k.max(0.8));
        } else if is_idle {
            dpoly(
                c,
                &pts,
                stroke.gamma_multiply(0.75),
                1.3 * xf.k.max(0.8),
                (4.0 * xf.k, 3.0 * xf.k),
            );
        } else {
            poly(
                c,
                &pts,
                stroke,
                if mark.is_some() { 2.6 } else { 2.0 } * xf.k.max(0.8),
            );
        }
        hover_path(c, &pts, 6.0, spec.wire_info[t.id].clone());
    }
    // wire labels: domain, codomain, and in between for wires born and consumed inside
    for i in &sim.a[0] {
        let t = &p.tokens[*i];
        if let Some(y) = lay.ys_a[0].get(i) {
            spans(
                c,
                xf,
                xl - 8.0,
                *y,
                Align2::RIGHT_CENTER,
                &token_label(spec, t, false),
                Some(PAPER),
            );
        }
    }
    let mut r_label_end: BTreeMap<usize, f32> = BTreeMap::new();
    for i in &sim.b[n] {
        let t = &p.tokens[*i];
        let Some(y) = lay.ys_b[n].get(i).copied() else {
            continue;
        };
        if sweep.is_some_and(|s| xr > s) {
            continue;
        }
        let ls = token_label(spec, t, false);
        let wd = spans(c, xf, xr + 8.0, y, Align2::LEFT_CENTER, &ls, Some(PAPER));
        r_label_end.insert(*i, xr + 8.0 + wd + 4.0);
        if let Some(m) = spec.marks.get(i) {
            if let Some(tag) = &m.tag {
                halo(
                    c,
                    xf,
                    xr + 8.0,
                    y + 11.0,
                    Align2::LEFT_CENTER,
                    tag,
                    9.0,
                    m.tag_color.unwrap_or(m.color),
                    PAPER,
                    false,
                );
            }
        }
    }
    for t in &drawn {
        if t.born == 0 || t.died.is_none() {
            continue;
        }
        let Some(y) = lay.ys_a[t.born].get(&t.id) else {
            continue;
        };
        let x0 = lay.x_right(t.born);
        if sweep.is_some_and(|s| x0 > s) {
            continue;
        }
        let gap_w = lay.x_left(t.born + 1, n) - x0;
        let ls = token_label(spec, t, true);
        let total: f32 = ls.iter().map(|s| tw(c, xf, &s.text, s.size, false)).sum();
        if total < gap_w - 8.0 {
            spans(
                c,
                xf,
                x0 + 5.0,
                y - 7.0,
                Align2::LEFT_CENTER,
                &ls,
                Some(PAPER),
            );
        }
    }
    // boxes
    for (i, b) in p.boxes.iter().enumerate() {
        let e = i + 1;
        let blk = &sim.blocks[e];
        let bwd = lay.bw[e];
        let cxm = lay.xs[e] + bwd / 2.0;
        if sweep.is_some_and(|s| cxm > s) {
            continue;
        }
        let x = lay.xs[e];
        let y = lay.lanes_top + blk.p as f32 * LANE + 3.0;
        let h = blk.h as f32 * LANE - 6.0;
        let fusing = fuse.filter(|f| f.0 == b.tid);
        let (stroke, sw) = match fusing {
            Some((_, col, t)) => (col, 1.5 + 2.5 * (1.0 - t)),
            None => (P_INK, 1.5),
        };
        let r = dbox(c, xf, x, y, bwd, h, 6.0, Color32::WHITE, stroke, sw, false);
        if spec.fused_box[i] {
            dbox(
                c,
                xf,
                x + 3.0,
                y + 3.0,
                bwd - 6.0,
                h - 6.0,
                4.0,
                Color32::TRANSPARENT,
                P_INK,
                1.0,
                false,
            );
        }
        let lines = &spec.box_lines[i];
        let ly0 = y + h / 2.0 - (lines.len() as f32 - 1.0) * 12.0 / 2.0;
        for (j, (txt, col)) in lines.iter().enumerate() {
            let dy = match fusing {
                Some((_, _, t)) => (j as f32 - (lines.len() as f32 - 1.0) / 2.0) * 26.0 * (1.0 - t),
                None => 0.0,
            };
            dbold(
                c,
                xf,
                cxm,
                ly0 + j as f32 * 12.0 + dy,
                Align2::CENTER_CENTER,
                txt,
                9.5,
                *col,
                false,
            );
        }
        let tab_x = if h < 30.0 { x + 1.0 } else { cxm };
        c.p.circle_filled(xf.pt(tab_x, y), 7.5 * xf.k, P_INK);
        dbold(
            c,
            xf,
            tab_x,
            y + 0.5,
            Align2::CENTER_CENTER,
            &b.k.to_string(),
            9.0,
            Color32::WHITE,
            false,
        );
        for id in &blk.in_block {
            if let Some(yy) = lay.ys_b[e - 1].get(id) {
                let t = &p.tokens[*id];
                c.p.circle_filled(
                    xf.pt(lay.xs[e], *yy),
                    2.4 * xf.k,
                    spec.marks
                        .get(id)
                        .map_or(spec.color_of(&t.place), |m| m.color),
                );
            }
        }
        for id in &blk.out_block {
            if let Some(yy) = lay.ys_a[e].get(id) {
                let t = &p.tokens[*id];
                c.p.circle_filled(
                    xf.pt(lay.x_right(e), *yy),
                    2.4 * xf.k,
                    spec.marks
                        .get(id)
                        .map_or(spec.color_of(&t.place), |m| m.color),
                );
            }
        }
        for t in b.reads.iter().filter(|t| spec.vis.contains(t)) {
            let col = spec.color_of(&p.tokens[*t].place);
            for (xx, yy) in [
                (lay.xs[e], lay.ys_b[e - 1].get(t)),
                (lay.x_right(e), lay.ys_a[e].get(t)),
            ] {
                if let Some(yy) = yy {
                    let ctr = xf.pt(xx, *yy);
                    let d = 4.5 * xf.k;
                    c.p.add(Shape::convex_polygon(
                        vec![
                            ctr + Vec2::new(-d, 0.0),
                            ctr + Vec2::new(0.0, -d),
                            ctr + Vec2::new(d, 0.0),
                            ctr + Vec2::new(0.0, d),
                        ],
                        Color32::WHITE,
                        Stroke::new(1.3, col),
                    ));
                }
            }
        }
        c.hover(r, spec.box_info[i].clone());
        painted.box_x.insert(b.tid.clone(), xf.pt(cxm, y + h / 2.0));
    }
    // the refused next box, past the codomain
    if let (Some(stop), Some(g), Some(gg)) = (&p.stop, &spec.ghost, &lay.ghost) {
        let (gx, gy, gw) = (gg.gx, gg.gy, lay.ghost_w);
        let r = dbox(
            c,
            xf,
            gx,
            gy,
            gw,
            gg.h,
            6.0,
            hex("#fff7f6"),
            P_LEAK,
            1.4,
            true,
        );
        let mut y = gy + 16.0;
        dbold(
            c,
            xf,
            gx + 10.0,
            y,
            Align2::LEFT_CENTER,
            &format!("✗ step {} does not fire", stop.index + 1),
            10.5,
            P_LEAK,
            false,
        );
        y += 14.0;
        for line in &g.header {
            spans(c, xf, gx + 10.0, y, Align2::LEFT_CENTER, line, None);
            y += 12.0;
        }
        y += 6.0;
        dtext(
            c,
            xf,
            gx + 10.0,
            y,
            Align2::LEFT_CENTER,
            "go-pflow EnabledWhyNot:",
            9.5,
            P_MUTED,
        );
        y += 12.0;
        for l in &gg.why {
            dtext(c, xf, gx + 10.0, y, Align2::LEFT_CENTER, l, 10.0, P_INK);
            y += 12.0;
        }
        y += 8.0;
        dtext(
            c,
            xf,
            gx + 10.0,
            y,
            Align2::LEFT_CENTER,
            if stop.needs.iter().any(|r| r.kind == NeedKind::Capacity) {
                "its domain and bounds, against the codomain:"
            } else {
                "its domain, against the codomain:"
            },
            9.5,
            P_MUTED,
        );
        y += 14.0;
        for rw in &stop.needs {
            let (pre, local) = split_place(&rw.place);
            let ok = rw.ok;
            let ss = vec![
                Span {
                    text: if ok { "✓ " } else { "✗ " }.into(),
                    color: if ok { BACK } else { P_LEAK },
                    size: 10.0,
                    bold: true,
                },
                Span {
                    text: pre,
                    color: P_MUTED,
                    size: 8.5,
                    bold: false,
                },
                Span {
                    text: local,
                    color: spec.color_of(&rw.place),
                    size: 10.0,
                    bold: true,
                },
                Span {
                    text: format!("  {}", row_tail(rw)),
                    color: P_MUTED,
                    size: 10.0,
                    bold: false,
                },
            ];
            spans(c, xf, gx + 22.0, y, Align2::LEFT_CENTER, &ss, None);
            let stub = xf.pt(
                if rw.kind == NeedKind::Capacity {
                    gx + gw
                } else {
                    gx
                },
                y,
            );
            c.p.circle(
                stub,
                3.5 * xf.k,
                if ok {
                    spec.color_of(&rw.place)
                } else {
                    Color32::WHITE
                },
                Stroke::new(1.5, if ok { spec.color_of(&rw.place) } else { P_LEAK }),
            );
            for t in &rw.tokens {
                if !spec.vis.contains(t) {
                    continue;
                }
                let (Some(y0), Some(x0)) = (lay.ys_b[n].get(t), r_label_end.get(t)) else {
                    continue;
                };
                let m = (gx - x0) / 2.0;
                let pts = dcubic(xf, [(*x0, *y0), (x0 + m, *y0), (gx - m, y), (gx - 4.0, y)]);
                dpoly(
                    c,
                    &pts,
                    spec.color_of(&p.tokens[*t].place).gamma_multiply(0.8),
                    1.2 * xf.k.max(0.8),
                    (3.0 * xf.k, 3.0 * xf.k),
                );
            }
            y += 16.0;
        }
        c.hover(r, g.info.clone());
    }
    // time axis
    let ay = lay.lanes_bottom + 14.0;
    c.line(xf.pt(xl, ay), xf.pt(xr - 6.0, ay), hex("#b9c0c9"), 1.0);
    arrow(
        c,
        xf.pt(xr - 12.0, ay),
        xf.pt(xr, ay),
        hex("#b9c0c9"),
        7.0 * xf.k.max(0.7),
    );
    dtext(
        c,
        xf,
        (xl + xr) / 2.0,
        ay + 13.0,
        Align2::CENTER_CENTER,
        &if n > 0 {
            format!("time →   {n} firing{}, composed in sequence (∘); wires side by side are the tensor (⊗)", if n == 1 { "" } else { "s" })
        } else {
            "no firing: the identity on the initial marking".into()
        },
        9.5,
        P_MUTED,
    );
    painted
}

// ------------------------------------------------------------ ui state

#[allow(deprecated)]
fn idle_flag(c: &Ctx) -> bool {
    c.p.ctx()
        .data(|d| d.get_temp::<bool>(egui::Id::new("cv-process-idle")))
        .unwrap_or(false)
}

#[allow(deprecated)]
fn set_idle(c: &Ctx, on: bool) {
    c.p.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::new("cv-process-idle"), on));
}

/// The idle toggle (a view-local checkbox: it changes the drawing, not the
/// composition). Returns the new flag.
fn idle_toggle(c: &mut Ctx, at: Pos2, n_idle: usize, tip: &str) -> bool {
    let on = idle_flag(c) && n_idle > 0;
    let label = format!("idle tokens as identity wires ({n_idle})");
    let w = label.chars().count() as f32 * 6.2 + 26.0;
    let r = Rect::from_min_size(at - Vec2::new(w, 0.0), Vec2::new(w, 18.0));
    let bx = Rect::from_min_size(r.left_top() + Vec2::new(0.0, 2.0), Vec2::splat(13.0));
    c.p.rect(
        bx,
        2.0,
        if on { hex("#2f6fb3") } else { Color32::WHITE },
        Stroke::new(1.0, MUTED),
        StrokeKind::Middle,
    );
    if on {
        c.text(
            bx.center(),
            Align2::CENTER_CENTER,
            "✓",
            10.0,
            Color32::WHITE,
        );
    }
    c.text(
        r.left_center() + Vec2::new(18.0, 0.0),
        Align2::LEFT_CENTER,
        &label,
        12.0,
        if n_idle > 0 {
            MUTED
        } else {
            MUTED.gamma_multiply(0.55)
        },
    );
    c.hover(
        r,
        info("idle tokens").note(if n_idle > 0 {
            tip
        } else {
            "This run touches every marked place: there are no idle tokens."
        }),
    );
    if n_idle > 0 && c.clicked && c.pointer.is_some_and(|p| r.contains(p)) {
        set_idle(c, !on);
        c.p.ctx().request_repaint();
    }
    on
}

// ------------------------------------------------------------ orders

fn hues(pattern: &str, i: usize) -> Color32 {
    let t: [&str; 3] = match pattern {
        "queue" => ["#2f6fb3", "#1d4f86", "#5a8fca"],
        "resource-pool" => ["#c47a12", "#8f5a0c", "#d9963a"],
        "saga" => ["#7b4cc2", "#56338f", "#9b72d8"],
        _ => ["#555555", "#555555", "#555555"],
    };
    hex(t[i % 3])
}

fn flat_model(c: &Ctx) -> PModel {
    let Some(f) = c.d.flat.as_ref() else {
        return PModel::default();
    };
    PModel {
        places: arr(&f["places"])
            .iter()
            .filter(|p| s(&p["kind"]) == "token" || s(&p["kind"]).is_empty())
            .map(|p| PPlace {
                id: s(&p["id"]).into(),
                initial: p["initial"].as_i64().unwrap_or(0),
                capacity: p["capacity"].as_i64().unwrap_or(0),
            })
            .collect(),
        arcs: arr(&f["arcs"])
            .iter()
            .map(|a| PArc {
                from: s(&a["from"]).into(),
                to: s(&a["to"]).into(),
                weight: a["weight"].as_i64().unwrap_or(1),
                typ: match s(&a["type"]) {
                    "read" => ArcT::Read,
                    "inhibitor" => ArcT::Inhibitor,
                    _ => ArcT::Normal,
                },
            })
            .collect(),
    }
}

fn flat_t_info(c: &Ctx, tid: &str) -> Info {
    let t = c.d.flat.as_ref().and_then(|f| {
        arr(&f["transitions"])
            .iter()
            .find(|t| s(&t["id"]) == tid)
            .cloned()
    });
    let mut i = info(format!("flat transition {tid}"));
    if let Some(t) = &t {
        i = i
            .fact("members", strs(&t["members"]).join(", "))
            .fact("fused", t["fused"].to_string())
            .note(s(&t["description"]));
    }
    with_cites(i, &[Cites::arcs()])
}

fn flat_p_info(c: &Ctx, pid: &str) -> Info {
    let p = c.d.flat.as_ref().and_then(|f| {
        arr(&f["places"])
            .iter()
            .find(|p| s(&p["id"]) == pid)
            .cloned()
    });
    let mut i = info(format!("flat place {pid}"));
    if let Some(p) = &p {
        i = i
            .fact("initial", p["initial"].to_string())
            .fact(
                "capacity",
                if p["capacity"].as_i64().unwrap_or(0) > 0 {
                    p["capacity"].to_string()
                } else {
                    "0 (unbounded)".into()
                },
            )
            .fact("members", strs(&p["members"]).join(", "))
            .note(s(&p["description"]));
    }
    i
}

fn member_subnet(m: &str) -> &str {
    m.split('/').next().unwrap_or("")
}

pub(super) fn process_orders(c: &mut Ctx, area: Rect, scenario: Option<&str>) {
    let subs = subnets(c);
    let scs = comp::scenarios();
    let runs = c.d.runs.clone();
    // emit: one run per scenario (D15)
    for r in &runs {
        let mut fired: Vec<String> = arr(&r["steps"])
            .iter()
            .filter(|st| st["enabled"] == true)
            .map(|st| s(&st["flat"]).to_string())
            .collect();
        fired.sort();
        c.scene.emit(
            "run",
            s(&r["id"]),
            json!({"fired": fired, "stopped_step": s(&r["stopped_step"])}),
        );
    }
    // colours by subnet
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut sub_color: BTreeMap<String, Color32> = BTreeMap::new();
    let mut sub_order: BTreeMap<String, i64> = BTreeMap::new();
    for (i, sb) in subs.iter().enumerate() {
        let pat = s(&sb["pattern"]).to_string();
        let k = seen.entry(pat.clone()).and_modify(|x| *x += 1).or_insert(0);
        sub_color.insert(s(&sb["id"]).into(), hues(&pat, *k));
        sub_order.insert(s(&sb["id"]).into(), i as i64);
    }
    // controls
    let sel = scenario
        .filter(|x| scs.iter().any(|s| s.id == *x))
        .unwrap_or("refund");
    c.text(
        area.left_top() + Vec2::new(0.0, 4.0),
        Align2::LEFT_TOP,
        "Run a scenario (judgment):",
        12.0,
        MUTED,
    );
    let mut x = area.left() + 170.0;
    for r in &runs {
        let id = s(&r["id"]).to_string();
        let script_len = scs
            .iter()
            .find(|x| x.id == id)
            .map_or(0, |x| x.script.len());
        let fails = arr(&r["expect"])
            .iter()
            .filter(|e| e["pass"] != true)
            .count();
        let ran = r["completed"] == true;
        let n_exp = arr(&r["expect"]).len();
        let res = if !ran {
            format!(
                "stops at {}/{}",
                r["stopped_at"].as_i64().unwrap_or(0) + 1,
                script_len
            )
        } else {
            format!("{script_len} firings · {}/{n_exp} expected", n_exp - fails)
        };
        let dot = if !ran {
            hex("#c47a12")
        } else if fails > 0 {
            P_LEAK
        } else {
            BACK
        };
        let label = format!("{id}  {res}");
        let wd = label.chars().count() as f32 * 6.4 + 30.0;
        let rect = Rect::from_min_size(Pos2::new(x, area.top()), Vec2::new(wd, 22.0));
        c.button(
            rect,
            &format!("    {label}"),
            id == sel,
            "view.composition",
            json!({"scenario": id}),
        );
        c.p.circle_filled(rect.left_center() + Vec2::new(11.0, 0.0), 4.0, dot);
        let title = scs.iter().find(|x| x.id == id).map_or("", |x| x.title);
        c.hover(rect, info(format!("scenario {id}")).fact("title", title).fact("result", res).note("A scenario's firing script (orders.js in the prototype, judgment, frozen), fired on the flat net with pflow-metamodel's firing rule (bundle.run)."));
        x = rect.right() + 6.0;
    }
    let Some(run) = runs.iter().find(|r| s(&r["id"]) == sel).cloned() else {
        return;
    };
    let sc = scs.iter().find(|x| x.id == sel).expect("selected");
    c.text(
        area.left_top() + Vec2::new(0.0, 28.0),
        Align2::LEFT_TOP,
        &format!(
            "{} — script: {}{}",
            sc.title,
            sc.script.join(" → "),
            if sc.note.is_empty() {
                String::new()
            } else {
                format!(". Scenario note: {}", sc.note)
            }
        ),
        11.5,
        MUTED,
    );
    // the model and the run
    let model = flat_model(c);
    let steps: Vec<PStep> = arr(&run["steps"])
        .iter()
        .map(|st| PStep {
            member: s(&st["member"]).into(),
            tid: st["flat"].as_str().map(str::to_string),
            say: String::new(),
            enabled: st["enabled"] == true,
            why: s(&st["why"]).into(),
        })
        .collect();
    let markings: Vec<BTreeMap<String, i64>> = arr(&run["markings"])
        .iter()
        .map(|m| {
            m.as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    let proc = run_process(&model, &steps, &markings);
    let idle = idle_tokens(&proc);
    let show_idle = idle_toggle(
        c,
        Pos2::new(area.right() - 4.0, area.top() + 44.0),
        idle.len(),
        "Tokens on places the run never touches are identity wires tensored on; hidden by default.",
    );
    let vis: BTreeSet<usize> = proc
        .tokens
        .iter()
        .filter(|t| !idle.contains(&t.id) || show_idle)
        .map(|t| t.id)
        .collect();
    let flat = c.d.flat.clone().unwrap_or(Value::Null);
    let place_members = |pid: &str| -> Vec<String> {
        arr(&flat["places"])
            .iter()
            .find(|p| s(&p["id"]) == pid)
            .map(|p| strs(&p["members"]))
            .unwrap_or_default()
    };
    let trans_members = |tid: &str| -> (Vec<String>, bool) {
        arr(&flat["transitions"])
            .iter()
            .find(|p| s(&p["id"]) == tid)
            .map(|p| (strs(&p["members"]), p["fused"] == true))
            .unwrap_or_default()
    };
    let place_subnets = |pid: &str| -> Vec<String> {
        let mut v: Vec<String> = vec![];
        for m in place_members(pid) {
            let sn = member_subnet(&m).to_string();
            if !v.contains(&sn) {
                v.push(sn);
            }
        }
        v
    };
    let mut colors: BTreeMap<String, Color32> = BTreeMap::new();
    let mut group: BTreeMap<String, i64> = BTreeMap::new();
    let mut fused_places: BTreeSet<String> = BTreeSet::new();
    let mut place_ix: BTreeMap<String, i64> = BTreeMap::new();
    for (i, p) in model.places.iter().enumerate() {
        place_ix.insert(p.id.clone(), i as i64);
        let ss = place_subnets(&p.id);
        colors.insert(
            p.id.clone(),
            if ss.len() == 1 {
                sub_color.get(&ss[0]).copied().unwrap_or(INK)
            } else {
                INK
            },
        );
        group.insert(
            p.id.clone(),
            ss.first()
                .and_then(|x| sub_order.get(x))
                .copied()
                .unwrap_or(99),
        );
        if place_members(&p.id).len() > 1 {
            fused_places.insert(p.id.clone());
        }
    }
    let tok_name = |t: &Token| {
        format!(
            "{}{}",
            t.place,
            if proc.serial.get(&t.place).copied().unwrap_or(0) > 1 {
                format!(" #{}", t.n)
            } else {
                String::new()
            }
        )
    };
    // the worker's wire: available -> (acquire) -> in_use -> (release) -> available
    let pm = |sid: &str, name: &str| -> String {
        flat["place_map"][sid][name]
            .as_str()
            .unwrap_or("")
            .to_string()
    };
    let tm = |sid: &str, name: &str| -> String {
        flat["transition_map"][sid][name]
            .as_str()
            .unwrap_or("")
            .to_string()
    };
    let saga_outcome = |sid: &str| -> &'static str {
        let at = |p: &str| proc.final_.get(&pm(sid, p)).copied().unwrap_or(0);
        if at("completed") > 0 {
            "completed"
        } else if at("rolled_back") > 0 {
            "rolled back"
        } else {
            "not finished"
        }
    };
    let saga_ids: Vec<String> = subs
        .iter()
        .filter(|x| s(&x["pattern"]) == "saga")
        .map(|x| s(&x["id"]).to_string())
        .collect();
    let outcomes = saga_ids
        .iter()
        .map(|sid| format!("{sid} {}", saga_outcome(sid)))
        .collect::<Vec<_>>()
        .join(", ");
    let pattern_of = |sid: &str| {
        subs.iter()
            .find(|x| s(&x["id"]) == sid)
            .map(|x| s(&x["pattern"]).to_string())
            .unwrap_or_default()
    };
    let why_no_release = |sid: &str| -> (bool, String) {
        let (mem, _) = trans_members(&tm(sid, "release"));
        let others: Vec<String> = mem
            .into_iter()
            .filter(|m| *m != format!("{sid}/release"))
            .collect();
        let commits: Vec<&String> = others
            .iter()
            .filter(|m| pattern_of(member_subnet(m)) == "saga" && m.ends_with("/commit"))
            .collect();
        let idiom = commits
            .iter()
            .any(|m| saga_outcome(member_subnet(m)) == "rolled back");
        let why = if others.is_empty() {
            format!("{sid}/release is not linked to anything, so no step of this script fires it")
        } else if idiom {
            format!("{sid}/release fires only with {}, the saga rolled back, and no port fires on either outcome (pat:idiom-pool-release-saga-commit; patterns.ttl:434, :496)", others.join(" + "))
        } else {
            format!(
                "{sid}/release fires only with {}, which this script does not fire",
                others.join(" + ")
            )
        };
        (idiom, why)
    };
    let mut marks: BTreeMap<usize, Mark> = BTreeMap::new();
    struct WNote {
        t0: usize,
        t1: usize,
        k: usize,
        sid: String,
        pred: Value,
        why: String,
        idiom: bool,
    }
    let mut worker_notes: Vec<WNote> = vec![];
    for sb in subs.iter().filter(|x| s(&x["pattern"]) == "resource-pool") {
        let sid = s(&sb["id"]);
        let (av, iu) = (pm(sid, "available"), pm(sid, "in_use"));
        let pred = arr(&run["expect"])
            .iter()
            .find(|e| s(&e["place"]) == format!("{sid}/available"))
            .cloned();
        for b in &proc.boxes {
            let took: Vec<usize> = b
                .ins
                .iter()
                .copied()
                .filter(|t| proc.tokens[*t].place == av)
                .collect();
            let gave: Vec<usize> = b
                .outs
                .iter()
                .copied()
                .filter(|t| proc.tokens[*t].place == iu)
                .collect();
            for (i, t0) in took.iter().enumerate() {
                let Some(t1) = gave.get(i) else { continue };
                match proc.tokens[*t1].died {
                    None => {
                        if let Some(pr) = pred
                            .as_ref()
                            .filter(|p| proc.stop.is_none() && p["pass"] != true)
                        {
                            let (idiom, why) = why_no_release(sid);
                            marks.insert(
                                *t0,
                                Mark {
                                    color: P_LEAK,
                                    under: true,
                                    tag: None,
                                    tag_color: None,
                                },
                            );
                            marks.insert(
                                *t1,
                                Mark {
                                    color: P_LEAK,
                                    under: true,
                                    tag: Some(
                                        if idiom {
                                            "never comes back: the leak"
                                        } else {
                                            "never comes back"
                                        }
                                        .into(),
                                    ),
                                    tag_color: None,
                                },
                            );
                            worker_notes.push(WNote {
                                t0: *t0,
                                t1: *t1,
                                k: b.k,
                                sid: sid.into(),
                                pred: pr.clone(),
                                why,
                                idiom,
                            });
                        }
                    }
                    Some(d) => {
                        let nb = &proc.boxes[d - 1];
                        let idx = nb
                            .ins
                            .iter()
                            .filter(|u| proc.tokens[**u].place == iu)
                            .position(|u| u == t1);
                        let back = idx.and_then(|ix| {
                            nb.outs
                                .iter()
                                .copied()
                                .filter(|u| proc.tokens[*u].place == av)
                                .nth(ix)
                        });
                        if let Some(bk) = back {
                            let col = sub_color.get(sid).copied().unwrap_or(INK);
                            marks.insert(
                                *t0,
                                Mark {
                                    color: col,
                                    under: true,
                                    tag: None,
                                    tag_color: None,
                                },
                            );
                            marks.insert(
                                *t1,
                                Mark {
                                    color: col,
                                    under: true,
                                    tag: None,
                                    tag_color: None,
                                },
                            );
                            marks.insert(
                                bk,
                                Mark {
                                    color: col,
                                    under: true,
                                    tag: if proc.tokens[bk].died.is_none() {
                                        Some(format!("returns to the pool at step {}", nb.k))
                                    } else {
                                        None
                                    },
                                    tag_color: Some(BACK),
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    let name_lines = |tid: &str| -> Vec<Vec<Span>> {
        let (mem, fused) = trans_members(tid);
        if !fused {
            let (pre, local) = split_place(tid);
            return vec![vec![sp(&pre, P_MUTED), spb(&local, P_INK)]];
        }
        let mut v = vec![vec![Span {
            text: "fused:".into(),
            color: P_MUTED,
            size: 9.0,
            bold: false,
        }]];
        for (j, m) in mem.iter().enumerate() {
            let (pre, local) = split_place(m);
            v.push(vec![
                sp(if j > 0 { "+" } else { "" }, P_MUTED),
                sp(&pre, P_MUTED),
                spb(
                    &local,
                    sub_color.get(member_subnet(m)).copied().unwrap_or(INK),
                ),
            ]);
        }
        v
    };
    let mut box_lines = vec![];
    let mut fused_box = vec![];
    let mut headers = vec![];
    let mut box_info = vec![];
    for b in &proc.boxes {
        let (mem, fused) = trans_members(&b.tid);
        let mem = if mem.is_empty() {
            vec![b.tid.clone()]
        } else {
            mem
        };
        let locals: Vec<String> = mem.iter().map(|m| split_place(m).1).collect();
        box_lines.push(
            mem.iter()
                .enumerate()
                .map(|(i, m)| {
                    let dup = locals.iter().filter(|l| **l == locals[i]).count() > 1;
                    (
                        if dup { m.clone() } else { locals[i].clone() },
                        sub_color.get(member_subnet(m)).copied().unwrap_or(INK),
                    )
                })
                .collect::<Vec<_>>(),
        );
        fused_box.push(fused);
        headers.push(name_lines(&b.tid));
        let mut i = flat_t_info(c, &b.tid);
        i.title = format!("step {}: {}", b.k, b.tid);
        i = i
            .fact("script step", format!("{}. {}", b.k, b.member))
            .fact(
                "takes (preset)",
                if b.ins.is_empty() {
                    "nothing".into()
                } else {
                    b.ins
                        .iter()
                        .map(|t| tok_name(&proc.tokens[*t]))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            )
            .fact(
                "gives (postset)",
                if b.outs.is_empty() {
                    "nothing".into()
                } else {
                    b.outs
                        .iter()
                        .map(|t| tok_name(&proc.tokens[*t]))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            );
        if !b.reads.is_empty() {
            i = i.fact(
                "reads, moves nothing",
                b.reads
                    .iter()
                    .map(|t| tok_name(&proc.tokens[*t]))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        if !b.read_shared.is_empty() {
            i = i.fact("reads the place it takes from", format!("{}: go-pflow tests the read on the marking before the firing, so the token it takes satisfies it", b.read_shared.join(", ")));
        }
        if !b.inhib.is_empty() {
            i = i.fact(
                "inhibited by",
                b.inhib
                    .iter()
                    .map(|x| format!("{} (< {}, has {})", x.0, x.1, x.2))
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        i = i.note("One firing is one box: a morphism from its preset to its postset, in the category the net generates. Which token (#n) it took is this view's first-in first-out choice.");
        box_info.push(with_cites(i, &[Cites::fire(), Cites::arcs()]));
    }
    let mut wire_info = vec![];
    for t in &proc.tokens {
        let mut i = flat_p_info(c, &t.place);
        let mark = marks.get(&t.id);
        let leak = mark.is_some_and(|m| m.color == P_LEAK);
        i.title = format!("token {}", tok_name(t));
        i = i
            .fact(
                "token",
                format!(
                    "#{} on this place, counted first-in first-out (this view's choice)",
                    t.n
                ),
            )
            .fact(
                "from",
                if t.born > 0 {
                    format!("step {}: {}", t.born, proc.boxes[t.born - 1].tid)
                } else {
                    "the domain (initial marking)".into()
                },
            )
            .fact(
                "to",
                match t.died {
                    Some(d) => format!("step {d}: {}", proc.boxes[d - 1].tid),
                    None if idle.contains(&t.id) => {
                        "the codomain: an identity wire, the run never touches this place".into()
                    }
                    None => "the codomain (still there when the run ends)".into(),
                },
            );
        if leak {
            if let Some(w) = worker_notes.first() {
                i = i.note(format!(
                    "{}: the worker taken here is still on in_use at the codomain ({outcomes}), because {}. Which unit counts as \"the worker\" is this view's pairing (first taken, first given back); the pool itself only counts (patterns.ttl:251).",
                    if w.idiom { "The leak is a wire that never comes back" } else { "A wire that never comes back" },
                    w.why
                ));
                i = with_cites(
                    i,
                    &if w.idiom {
                        vec![Cites::leak(), Cites::no_either(), Cites::anon()]
                    } else {
                        vec![Cites::anon()]
                    },
                );
            }
        } else if mark.is_some() {
            i = i.note("Highlighted: the worker's path, available → in_use → available. Which unit counts as \"the worker\" is this view's pairing; the pool itself only counts (patterns.ttl:251).");
        }
        if t.place.ends_with("/items") {
            i = with_cites(i, &[Cites::fifo_not()]);
        }
        wire_info.push(i);
    }
    let marking_facts = |mk: &BTreeMap<String, i64>| {
        mk.iter()
            .filter(|(_, n)| **n != 0)
            .map(|(p, n)| format!("{p}: {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let b_info = |which: &str, mk: &BTreeMap<String, i64>, sim: &[usize]| -> Info {
        let _ = sim;
        info(if which == "domain" { "domain: the initial marking".to_string() } else if proc.stop.is_some() { format!("codomain: the marking after step {}", proc.boxes.len()) } else { "codomain: the final marking".to_string() })
            .fact("whole marking", marking_facts(mk))
            .fact("not drawn", if show_idle { "nothing" } else { "tokens on places the run never touches (identity wires; toggle above)" })
            .note("A marking is a multiset of places: an element of the free commutative monoid on the places (Meseguer and Montanari). Drawn as a list of wires, in an order the layout picks.")
    };
    let ghost = proc.stop.as_ref().map(|stop| {
        let sk_domain_ok = !stop.needs.iter().any(|n| !n.ok && n.kind == NeedKind::Take) && stop.tid.is_some();
        let side = stop.needs.iter().find(|n| !n.ok).map(|n| match n.kind {
            NeedKind::Read => "a read arc",
            NeedKind::Inhibitor => "an inhibitor arc",
            NeedKind::Capacity => "a capacity bound",
            NeedKind::Take => "a rule this drawing does not model",
        });
        let spent = stop.needs.iter().any(|n| !n.ok && n.kind == NeedKind::Take && n.place.rsplit('/').next().is_some_and(|l| l.starts_with("next_")));
        let mut i = info(format!("step {} refused: {}", stop.index + 1, stop.tid.clone().unwrap_or_else(|| stop.member.clone())))
            .fact("script step", format!("{}. {}", stop.index + 1, stop.member))
            .fact("go-pflow EnabledWhyNot", stop.why.clone());
        for n in &stop.needs {
            i = i.fact(
                &format!("{} {}", if n.ok { "✓" } else { "✗" }, n.place),
                match n.kind {
                    NeedKind::Capacity => format!("capacity {}: holds {}, this firing gives {}", n.need, n.have, n.delta),
                    NeedKind::Take => format!("takes {}, has {}", n.need, n.have),
                    NeedKind::Read => format!("reads {}, has {}", n.need, n.have),
                    NeedKind::Inhibitor => format!("needs fewer than {}, has {}", n.need, n.have),
                },
            );
        }
        i = i.note(if sk_domain_ok {
            format!("Its domain is in the codomain, so as a box it would compose; go-pflow refuses the firing on {}. The string diagram has no wire for that, so the refusal is drawn as this box outside the morphism.", side.unwrap_or("a rule this drawing does not model"))
        } else {
            "The next box does not compose: its domain is not contained in the codomain of the run so far.".to_string()
        });
        let mut cs = vec![Cites::why_not()];
        if side == Some("a capacity bound") {
            cs.push(Cites::capacity());
        }
        if spent {
            cs.push(Cites::one_case());
        }
        let header = match &stop.tid {
            Some(t) => name_lines(t),
            None => vec![vec![sp(&stop.member, P_INK)]],
        };
        Ghost { header, info: with_cites(i, &cs) }
    });
    let spec = Spec {
        boundary_info: [
            b_info("domain", &proc.initial, &[]),
            b_info("codomain", &proc.final_, &[]),
        ],
        caption_r: if proc.stop.is_some() {
            format!("marking after step {}", proc.boxes.len())
        } else {
            "final marking".into()
        },
        proc: proc.clone(),
        vis,
        idle: idle.clone(),
        place_ix,
        group,
        colors,
        default_color: INK,
        fused_places,
        fused_box,
        box_lines,
        headers,
        box_info,
        wire_info,
        ghost,
        marks,
        right_extra: 0.0,
    };
    // the interface strip (every port, snap targets) takes the bottom
    let iface_h = 150.0;
    let body = Rect::from_min_max(
        area.left_top() + Vec2::new(0.0, 48.0),
        Pos2::new(area.right(), area.bottom() - iface_h - 6.0),
    );
    // legend
    let top_legend = 0.0;
    let lay = layout(&spec, 34.0 + top_legend);
    let total_h = lay.bottom + 26.0;
    let xf = Xf::fit(body, W, total_h, 1.3);
    let xf = &xf;
    // legend (subnet colours)
    let mut lx = PAD;
    dtext(
        c,
        xf,
        lx,
        16.0,
        Align2::LEFT_CENTER,
        "wires colored by subnet:",
        10.0,
        P_MUTED,
    );
    lx += 124.0;
    for sb in &subs {
        let id = s(&sb["id"]);
        let pat = comp::ontology()["patterns"][s(&sb["pattern"])]["prefLabel"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let col = sub_color.get(id).copied().unwrap_or(INK);
        poly(
            c,
            &[xf.pt(lx, 13.0), xf.pt(lx + 18.0, 13.0)],
            col,
            2.5 * xf.k.max(0.8),
        );
        dtext(c, xf, lx + 23.0, 16.0, Align2::LEFT_CENTER, id, 10.5, P_INK);
        dtext(
            c,
            xf,
            lx + 23.0 + tw_(id, 10.5) + 4.0,
            16.0,
            Align2::LEFT_CENTER,
            &pat,
            9.5,
            P_MUTED,
        );
        let r = Rect::from_min_max(
            xf.pt(lx, 6.0),
            xf.pt(lx + 23.0 + tw_(id, 10.5) + 8.0 + tw_(&pat, 9.5), 22.0),
        );
        c.hover(r, subnet_info(sb));
        lx += 23.0 + tw_(id, 10.5) + 8.0 + tw_(&pat, 9.5) + 18.0;
    }
    poly(c, &[xf.pt(lx, 10.5), xf.pt(lx + 18.0, 10.5)], P_INK, 1.4);
    poly(c, &[xf.pt(lx, 15.5), xf.pt(lx + 18.0, 15.5)], P_INK, 1.4);
    dtext(
        c,
        xf,
        lx + 23.0,
        16.0,
        Align2::LEFT_CENTER,
        "double: fused",
        10.0,
        P_MUTED,
    );
    // merge animation
    let sweep = c
        .anim(|e| match e {
            Event::LinkAdded { .. } | Event::LinkRemoved { .. } | Event::InstanceAdded { .. } => {
                Some(())
            }
            _ => None,
        })
        .map(|(_, t)| lay.xl - 4.0 + t * (lay.xr + lay.right_w - lay.xl));
    let fuse = c.anim(|e| match e {
        Event::LinkAdded { key, kind } if kind == "event" => Some(key.clone()),
        _ => None,
    });
    let fuse_tid: Option<(String, Color32, f32)> = fuse.and_then(|(key, t)| {
        let from = key.strip_prefix("event:")?.split("->").next()?.to_string();
        let (sid, port) = from.split_once(':')?;
        let sb = subs.iter().find(|x| s(&x["id"]) == sid)?;
        let pt = arr(&sb["ports"]).iter().find(|p| s(&p["id"]) == port)?;
        let tid = tm(sid, s(&pt["transition"]));
        Some((tid, kind_color("event"), t))
    });
    paint(
        c,
        xf,
        &spec,
        &lay,
        sweep,
        fuse_tid.as_ref().map(|f| (f.0.as_str(), f.1, f.2)),
    );
    // expected predicates, evaluated on the codomain
    let ey = lay.bottom + 6.0;
    let mut ex = lay.xl;
    dtext(
        c,
        xf,
        ex - 8.0,
        ey,
        Align2::RIGHT_CENTER,
        if proc.stop.is_some() {
            "expected, at the stop:"
        } else {
            "expected (judgment):"
        },
        10.0,
        P_MUTED,
    );
    for e in arr(&run["expect"]) {
        let pass = e["pass"] == true;
        let txt = format!(
            "{} {} {} {}{}",
            if pass { "✓" } else { "✗" },
            s(&e["place"]),
            s(&e["op"]),
            e["n"],
            if pass {
                String::new()
            } else {
                format!(" (has {})", e["actual"])
            }
        );
        let r = dtext(
            c,
            xf,
            ex,
            ey,
            Align2::LEFT_CENTER,
            &txt,
            10.5,
            if pass { BACK } else { P_LEAK },
        );
        c.hover(
            r,
            info(format!("expected: {} {} {}", s(&e["place"]), s(&e["op"]), e["n"]))
                .fact("says", s(&e["says"]))
                .fact("flat place", s(&e["flat"]))
                .fact("at the codomain", e["actual"].to_string())
                .fact("holds", if pass { "yes" } else { "no" })
                .note("Scenario predicates are this page's judgment, frozen as ROADMAP 4.4 says scenarios are; whether a step fires is pflow-metamodel's rule."),
        );
        ex += tw_(&txt, 10.5) + 22.0;
    }
    // notes
    let notes_before = c.notes.len();
    let _ = notes_before;
    if let Some(w) = worker_notes.first() {
        c.note("warn", format!(
            "Pool leak: worker {} goes into {} at step {} and is still there at the codomain ({outcomes}). {}: {}. Expected {} {} {}, has {}.",
            tok_name(&proc.tokens[w.t0]), tok_name(&proc.tokens[w.t1]), w.k,
            if w.idiom { "The leak is a wire that never comes back" } else { "It is a wire that never comes back" },
            w.why, s(&w.pred["place"]), s(&w.pred["op"]), w.pred["n"], w.pred["actual"]
        ));
        let _ = &w.sid;
    }
    if let Some(stop) = &proc.stop {
        let missing: Vec<String> = stop
            .needs
            .iter()
            .filter(|n| !n.ok && n.kind == NeedKind::Take)
            .map(|n| n.place.clone())
            .collect();
        let what = if missing.is_empty() && stop.tid.is_some() {
            format!("Nothing it takes is missing from the codomain: go-pflow refuses it on a side condition no wire shows ({}), so no wire shows it.", stop.needs.iter().filter(|n| !n.ok).map(|n| n.place.clone()).collect::<Vec<_>>().join(", "))
        } else {
            format!("Missing from the codomain: {}.", missing.join(", "))
        };
        c.note("warn", format!("The run stops at step {} of {} ({}). go-pflow: \"{}\". {what} Steps after it are not drawn; the codomain is the marking where it stopped.", stop.index + 1, sc.script.len(), stop.member, stop.why));
    }
    let read_in: Vec<usize> = proc
        .boxes
        .iter()
        .filter(|b| !b.reads.is_empty() || !b.read_shared.is_empty())
        .map(|b| b.k)
        .collect();
    if !read_in.is_empty() {
        c.note("na", format!("Step{} {} read{} a place without moving it: a guard link, lowered to a read arc as go-pflow does. The read token is drawn passing its box (◇), the nearest thing a string diagram has. Read arcs are outside the free-SMC fragment (ROADMAP Phase 6): drawn, with no functorial semantics claimed.", if read_in.len() > 1 { "s" } else { "" }, read_in.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(", "), if read_in.len() > 1 { "" } else { "s" }));
    }
    let indep = independent_steps(&proc);
    if !indep.is_empty() {
        c.note("honesty", format!("Step{} {} take{} nothing the step before produced or read: as boxes, each could be drawn before its left neighbour (interchange law). The columns follow the script.", if indep.len() > 1 { "s" } else { "" }, indep.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(", "), if indep.len() > 1 { "" } else { "s" }));
    }
    c.note("honesty", "Individual tokens are a choice here: each token is its own wire, told apart first-in first-out on its place (#1, #2); the net itself does not order tokens (patterns.ttl:201, :251). The collective-token reading (Petri nets generate commutative monoidal categories) identifies runs that differ only in which token was used, and makes the crossings symmetries; Baez, Genovese, Master and Shulman treat both readings (Σ-nets).");
    // the composite's interface: every port, snap targets
    let iface = Rect::from_min_max(
        Pos2::new(area.left(), area.bottom() - iface_h),
        area.right_bottom(),
    );
    c.line(iface.left_top(), iface.right_top(), hex("#e1e5ea"), 1.0);
    c.text(
        iface.left_top() + Vec2::new(0.0, 4.0),
        Align2::LEFT_TOP,
        "The composite's interface",
        11.0,
        INK,
    );
    c.text(iface.left_top() + Vec2::new(150.0, 5.0), Align2::LEFT_TOP, "every port of every instance. Drag one onto another to link them (go-pflow's validateLink decides); the run above replays on the new flat net.", 10.5, MUTED);
    let laid = draw_boxes(
        c,
        Rect::from_min_max(
            iface.left_top() + Vec2::new(0.0, 28.0),
            iface.right_bottom(),
        ),
        62.0,
        true,
    );
    for l in links(c) {
        draw_link(c, &laid, &l);
    }
}

// ------------------------------------------------------------ connect-4

const C4: [(&str, &str); 6] = [
    ("x", "#b0413e"),
    ("o", "#2f6fb3"),
    ("open", "#8a93a0"),
    ("turn", "#3c4656"),
    ("active", "#178f7f"),
    ("win", "#7b4cc2"),
];

fn c4_color(fam: &str) -> Color32 {
    hex(C4
        .iter()
        .find(|(k, _)| *k == fam)
        .map_or("#3c4656", |x| x.1))
}

fn c4_family(pid: &str) -> &'static str {
    if pid.starts_with("open_") {
        "open"
    } else if pid == "x_turn" || pid == "o_turn" {
        "turn"
    } else if pid == "game_active" {
        "active"
    } else if pid.starts_with("win_") {
        "win"
    } else if pid.starts_with("x_") {
        "x"
    } else if pid.starts_with("o_") {
        "o"
    } else {
        "turn"
    }
}

fn c4_group(fam: &str) -> i64 {
    match fam {
        "turn" => 0,
        "open" => 1,
        "x" => 2,
        "o" => 3,
        "active" => 4,
        _ => 5,
    }
}

fn c4_model(doc: &Document) -> PModel {
    let places: BTreeSet<&str> = doc.places.iter().map(|p| p.id.as_str()).collect();
    PModel {
        places: doc
            .places
            .iter()
            .map(|p| PPlace {
                id: p.id.clone(),
                initial: p.initial.first().copied().unwrap_or(0.0) as i64,
                capacity: p.capacity.first().copied().unwrap_or(0.0) as i64,
            })
            .collect(),
        arcs: doc
            .arcs
            .iter()
            .map(|a| {
                let w = a.weight.first().copied().unwrap_or(1.0) as i64;
                if !a.inhibit {
                    PArc {
                        from: a.source.clone(),
                        to: a.target.clone(),
                        weight: w,
                        typ: ArcT::Normal,
                    }
                } else if places.contains(a.source.as_str()) {
                    PArc {
                        from: a.source.clone(),
                        to: a.target.clone(),
                        weight: w,
                        typ: ArcT::Inhibitor,
                    }
                } else {
                    PArc {
                        from: a.target.clone(),
                        to: a.source.clone(),
                        weight: w,
                        typ: ArcT::Read,
                    }
                }
            })
            .collect(),
    }
}

pub(super) fn process_c4(c: &mut Ctx, area: Rect) {
    let op = c.d.opening.clone();
    let fired = op["fired"].as_i64().unwrap_or(0);
    c.scene.emit("run", "opening", json!({"fired": fired}));
    let Some(doc) = c.d.c4.clone() else { return };
    let model = c4_model(&doc);
    let says = [
        "X into column 3, row 0",
        "O into column 3, row 1",
        "X into column 2, row 0",
        "O into column 4, row 0",
    ];
    let steps: Vec<PStep> = arr(&op["steps"])
        .iter()
        .enumerate()
        .map(|(i, st)| PStep {
            member: s(&st["transition"]).into(),
            tid: Some(s(&st["transition"]).into()),
            say: says.get(i).unwrap_or(&"").to_string(),
            enabled: st["enabled"] == true,
            why: s(&st["why"]).into(),
        })
        .collect();
    let markings: Vec<BTreeMap<String, i64>> = arr(&op["markings"])
        .iter()
        .map(|m| {
            m["marking"]
                .as_object()
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| (k.clone(), v.as_i64().unwrap_or(0)))
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect();
    let en: Vec<(Vec<String>, Vec<String>)> = arr(&op["markings"])
        .iter()
        .map(|m| (strs(&m["moves"]), strs(&m["autos"])))
        .collect();
    let auto_count = doc
        .transitions
        .iter()
        .filter(|t| t.id.contains("win"))
        .count();
    let proc = run_process(&model, &steps, &markings);
    let idle = idle_tokens(&proc);
    c.text(
        area.left_top() + Vec2::new(0.0, 4.0),
        Align2::LEFT_TOP,
        "One play: a fixed opening, both sides into the centre column, then its neighbours",
        12.0,
        MUTED,
    );
    let show_idle = idle_toggle(
        c,
        Pos2::new(area.right() - 4.0, area.top() + 2.0),
        idle.len(),
        "The other open slots and game_active: identity wires the play never touches.",
    );
    let vis: BTreeSet<usize> = proc
        .tokens
        .iter()
        .filter(|t| !idle.contains(&t.id) || show_idle)
        .map(|t| t.id)
        .collect();
    let tok_name = |t: &Token| {
        format!(
            "{}{}",
            t.place,
            if proc.serial.get(&t.place).copied().unwrap_or(0) > 1 {
                format!(" #{}", t.n)
            } else {
                String::new()
            }
        )
    };
    let side = |tid: &str| {
        if tid.starts_with("x_") {
            "X (+1, maximizes the reward)"
        } else {
            "O (-1)"
        }
    };
    let place_desc = |pid: &str| -> String {
        let parts: Vec<&str> = pid.split('_').collect();
        if parts.len() == 3
            && ["open", "x", "o"].contains(&parts[0])
            && parts[1].parse::<u8>().is_ok()
        {
            return if parts[0] == "open" {
                format!(
                    "the open slot of column {}, row {}: the next drop there lands in row {}",
                    parts[1], parts[2], parts[2]
                )
            } else {
                format!(
                    "a {} disc in column {}, row {} (row 0 is the bottom)",
                    parts[0].to_uppercase(),
                    parts[1],
                    parts[2]
                )
            };
        }
        match pid {
            "x_turn" => "X to move",
            "o_turn" => "O to move",
            "game_active" => "the game is on; every win detector consumes it",
            "win_x" => "reward place: X won (+1)",
            "win_o" => "reward place: O won (-1)",
            _ => "",
        }
        .to_string()
    };
    let mut colors = BTreeMap::new();
    let mut group = BTreeMap::new();
    let mut place_ix = BTreeMap::new();
    for (i, p) in model.places.iter().enumerate() {
        colors.insert(p.id.clone(), c4_color(c4_family(&p.id)));
        group.insert(p.id.clone(), c4_group(c4_family(&p.id)));
        place_ix.insert(p.id.clone(), i as i64);
    }
    let mut box_lines = vec![];
    let mut headers = vec![];
    let mut box_info = vec![];
    for b in &proc.boxes {
        let col = c4_color(&b.tid[..1]);
        box_lines.push(vec![(b.tid.clone(), col)]);
        headers.push(vec![
            vec![spb(&b.tid, col)],
            vec![Span {
                text: b.say.clone(),
                color: P_MUTED,
                size: 9.5,
                bold: false,
            }],
        ]);
        let (moves, autos) = en.get(b.k).cloned().unwrap_or_default();
        let sides: BTreeSet<String> = moves.iter().map(|t| t[..1].to_uppercase()).collect();
        let i = info(format!("step {}: {}", b.k, b.tid))
            .fact("side", side(&b.tid))
            .fact("automatic", "no: the name has no \"win\" (tests/common/mod.rs:28)")
            .fact("takes (preset)", b.ins.iter().map(|t| tok_name(&proc.tokens[*t])).collect::<Vec<_>>().join(", "))
            .fact("gives (postset)", b.outs.iter().map(|t| tok_name(&proc.tokens[*t])).collect::<Vec<_>>().join(", "))
            .fact("then enabled", format!("{} moves ({}), {} of {auto_count} automatic", moves.len(), sides.into_iter().collect::<Vec<_>>().join(", "), autos.len()))
            .note(format!("{} One drop is one box. Enabled by pflow-metamodel's firing rule; pflow-rs would also settle automatic transitions after it, and {}.", b.say, if autos.is_empty() { "none is enabled here".to_string() } else { format!("{} is enabled here, which this drawing omits", autos.len()) }));
        box_info.push(with_cites(
            i,
            &[Cites::fire(), Cites::game(), Cites::fixture()],
        ));
    }
    let mut wire_info = vec![];
    for t in &proc.tokens {
        let init = model
            .places
            .iter()
            .find(|p| p.id == t.place)
            .map_or(0, |p| p.initial);
        let i = info(format!("token {}", tok_name(t)))
            .fact("initial", init.to_string())
            .fact(
                "token",
                format!(
                    "#{} on this place, first-in first-out (this view's choice)",
                    t.n
                ),
            )
            .fact(
                "from",
                if t.born > 0 {
                    format!("step {}: {}", t.born, proc.boxes[t.born - 1].tid)
                } else {
                    "the domain (initial marking)".into()
                },
            )
            .fact(
                "to",
                match t.died {
                    Some(d) => format!("step {d}: {}", proc.boxes[d - 1].tid),
                    None if idle.contains(&t.id) => "the codomain: an identity wire".into(),
                    None => "the codomain".into(),
                },
            )
            .note(place_desc(&t.place));
        wire_info.push(with_cites(i, &[Cites::fixture()]));
    }
    let mk_s = |mk: &BTreeMap<String, i64>| {
        mk.iter()
            .filter(|(_, n)| **n != 0)
            .map(|(p, n)| format!("{p}: {n}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let boundary = |which: &str, mk: &BTreeMap<String, i64>| {
        info(if which == "domain" {
            "domain: the initial marking"
        } else {
            "codomain: the marking after four drops"
        })
        .fact("whole marking", mk_s(mk))
        .note(
            "A marking is a multiset of places (Meseguer and Montanari), drawn as a list of wires.",
        )
    };
    const BOARD_W: f32 = 150.0;
    let ghost = proc.stop.as_ref().map(|stop| Ghost {
        header: vec![vec![sp(&stop.member, P_INK)]],
        info: with_cites(
            info(format!("refused: {}", stop.member))
                .fact("go-pflow EnabledWhyNot", stop.why.clone()),
            &[Cites::why_not()],
        ),
    });
    let spec = Spec {
        boundary_info: [
            boundary("domain", &proc.initial),
            boundary("codomain", &proc.final_),
        ],
        caption_r: "final marking".into(),
        proc: proc.clone(),
        vis,
        idle,
        place_ix,
        group,
        colors,
        default_color: P_INK,
        fused_places: BTreeSet::new(),
        fused_box: vec![false; proc.boxes.len()],
        box_lines,
        headers,
        box_info,
        wire_info,
        ghost,
        marks: BTreeMap::new(),
        right_extra: BOARD_W,
    };
    let body = Rect::from_min_max(area.left_top() + Vec2::new(0.0, 28.0), area.right_bottom());
    let lay = layout(&spec, 34.0);
    let board_bottom = lay.lanes_top - 4.0 + 17.0 * 6.0 + 40.0;
    let total_h = (lay.bottom + 40.0).max(board_bottom) + 10.0;
    let xf = Xf::fit(body, W, total_h, 1.3);
    let xf = &xf;
    // legend: place families, by the fixture's naming convention
    let mut lx = PAD;
    dtext(
        c,
        xf,
        lx,
        16.0,
        Align2::LEFT_CENTER,
        "wires colored by place name (the fixture's convention):",
        10.0,
        P_MUTED,
    );
    lx += 262.0;
    for (fam, label) in [
        ("x", "x_c_r  X disc"),
        ("o", "o_c_r  O disc"),
        ("open", "open_c_r  open slot"),
        ("turn", "x_turn / o_turn"),
        ("active", "game_active"),
    ] {
        poly(
            c,
            &[xf.pt(lx, 13.0), xf.pt(lx + 18.0, 13.0)],
            c4_color(fam),
            2.5 * xf.k.max(0.8),
        );
        dtext(
            c,
            xf,
            lx + 23.0,
            16.0,
            Align2::LEFT_CENTER,
            label,
            10.0,
            P_INK,
        );
        lx += 23.0 + tw_(label, 10.0) + 18.0;
    }
    let sweep = c
        .anim(|e| match e {
            Event::PieceAdded { .. } => Some(()),
            _ => None,
        })
        .map(|(_, t)| lay.xl - 4.0 + t * (lay.xr + lay.right_w - lay.xl));
    paint(c, xf, &spec, &lay, sweep, None);
    // what is enabled at each marking along the play
    let ey = lay.bottom + 4.0;
    dtext(
        c,
        xf,
        lay.xl - 8.0,
        ey,
        Align2::RIGHT_CENTER,
        "enabled here:",
        10.0,
        P_MUTED,
    );
    let n = proc.boxes.len();
    for e in 0..=n {
        let Some((moves, autos)) = en.get(e) else {
            continue;
        };
        let x = (lay.x_right(e) + lay.x_left(e + 1, n)) / 2.0;
        let sides: BTreeSet<String> = moves.iter().map(|t| t[..1].to_uppercase()).collect();
        let t1 = format!(
            "{} {} drops",
            moves.len(),
            sides.into_iter().collect::<Vec<_>>().join("/")
        );
        let t2 = format!("{} of {auto_count} automatic", autos.len());
        let r1 = dtext(c, xf, x, ey, Align2::CENTER_CENTER, &t1, 10.0, P_INK);
        let r2 = dtext(
            c,
            xf,
            x,
            ey + 12.0,
            Align2::CENTER_CENTER,
            &t2,
            9.5,
            if autos.is_empty() { P_MUTED } else { P_LEAK },
        );
        c.hover(
            r1.union(r2),
            info(if e > 0 { format!("enabled after step {e}") } else { "enabled at the initial marking".into() })
                .fact("moves", moves.join(", "))
                .fact("automatic (name contains \"win\")", if autos.is_empty() { format!("none of {auto_count}") } else { autos.join(", ") })
                .note("Checked with pflow-metamodel's firing rule on every transition of the fixture (plan.opening). Automatic transitions are read by the fixture's naming convention, as the pflow-rs tests do."),
        );
    }
    // the codomain read as a board
    let (bx, by, cell) = (lay.xr + lay.right_w + 24.0, lay.lanes_top - 4.0, 17.0f32);
    dbold(
        c,
        xf,
        bx,
        by - 8.0,
        Align2::LEFT_CENTER,
        "the codomain as a board",
        10.0,
        P_INK,
        false,
    );
    let br = dbox(
        c,
        xf,
        bx,
        by,
        cell * 7.0 + 6.0,
        cell * 6.0 + 6.0,
        5.0,
        hex("#eef1f5"),
        hex("#c9d0d8"),
        1.0,
        false,
    );
    for col in 0..7 {
        for row in 0..6 {
            let (x, y) = (
                bx + 3.0 + col as f32 * cell + cell / 2.0,
                by + 3.0 + (5 - row) as f32 * cell + cell / 2.0,
            );
            let at = |k: &str| {
                proc.final_
                    .get(&format!("{k}_{col}_{row}"))
                    .copied()
                    .unwrap_or(0)
            };
            let (fx, fo, op_) = (at("x"), at("o"), at("open"));
            let ctr = xf.pt(x, y);
            let rad = (cell / 2.0 - 2.0) * xf.k;
            c.p.circle_filled(
                ctr,
                rad,
                if fx > 0 {
                    c4_color("x")
                } else if fo > 0 {
                    c4_color("o")
                } else {
                    Color32::WHITE
                },
            );
            if op_ > 0 {
                let pts: Vec<Pos2> = (0..=16)
                    .map(|k| ctr + Vec2::angled(k as f32 / 16.0 * std::f32::consts::TAU) * rad)
                    .collect();
                dpoly(
                    c,
                    &pts,
                    c4_color("open"),
                    1.6 * xf.k.max(0.8),
                    (2.0 * xf.k, 1.5 * xf.k),
                );
            } else {
                c.p.circle_stroke(ctr, rad, Stroke::new(1.0, hex("#d5dae0")));
            }
        }
        dtext(
            c,
            xf,
            bx + 3.0 + col as f32 * cell + cell / 2.0,
            by + cell * 6.0 + 17.0,
            Align2::CENTER_CENTER,
            &col.to_string(),
            8.5,
            P_MUTED,
        );
    }
    dtext(
        c,
        xf,
        bx,
        by + cell * 6.0 + 30.0,
        Align2::LEFT_CENTER,
        "dashed ring: open_c_r marked",
        9.0,
        P_MUTED,
    );
    let list = |k: &str| {
        proc.final_
            .iter()
            .filter(|(p, n)| {
                p.starts_with(&format!("{k}_"))
                    && p.chars().nth(2).is_some_and(|ch| ch.is_ascii_digit())
                    && **n > 0
            })
            .map(|(p, _)| p.clone())
            .collect::<Vec<_>>()
            .join(", ")
    };
    c.hover(
        br,
        with_cites(
            info("the final marking, read as a board")
                .fact("X discs", list("x"))
                .fact("O discs", list("o"))
                .fact("to move", if proc.final_.get("x_turn").copied().unwrap_or(0) > 0 { "X" } else if proc.final_.get("o_turn").copied().unwrap_or(0) > 0 { "O" } else { "nobody" })
                .note("x_<c>_<r> and o_<c>_<r> marked are discs; open_<c>_<r> marked is the next free slot of column c. Row 0 is the bottom (fixture description). A reading of the codomain, not a separate model."),
            &[Cites::fixture()],
        ),
    );
    let autos_seen = en.iter().any(|x| !x.1.is_empty());
    c.note("na", "No search is run and the search plan is not drawn here, so the toolbar's plan pieces have no slot to snap into in this view. The plan (plan.rs) computes one value over all plays from a position; one play is one process. This is a fixed four-drop opening, each drop checked enabled by the firing rule (plan.opening).");
    let indep = independent_steps(&proc);
    if !indep.is_empty() {
        c.note("honesty", format!("Step{} {} take{} nothing the step before produced: as boxes, each could be drawn before its left neighbour (interchange law). The columns follow the play.", if indep.len() > 1 { "s" } else { "" }, indep.iter().map(|k| k.to_string()).collect::<Vec<_>>().join(", "), if indep.len() > 1 { "" } else { "s" }));
    }
    c.note(if autos_seen { "warn" } else { "honesty" }, format!("Automatic transitions: the tests read every transition whose name contains \"win\" as automatic ({auto_count} here; tests/common/mod.rs:28) and pflow-rs fires them as soon as they are enabled, lowest index first (game.rs settle, :279). {}", if autos_seen { "One is enabled in this play, which this drawing omits." } else { "None is enabled after any of the four drops (checked on every transition), so the play has no automatic boxes. Token identities are first-in first-out per place, this view's choice." }));
    let _ = (Cites::game_auto(), Cites::settle(), Cites::by_name());
}

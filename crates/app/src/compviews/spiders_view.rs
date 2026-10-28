//! The spiders view (see `docs/composition-views-scene.md`).
//!
//! A port of `js/views/spiders.js`, drawn in the prototype's design
//! coordinates and scaled into the window (`Xf`). Orders: opaque instance
//! boxes between two bands, red place classes (token and data links) on the
//! left and green transition classes (event links) on the right, each class a
//! spider with a leg to every port that joins it, read from `bundle.flatten`
//! and never computed here. Connect-4: the fixed core, the two shared tables
//! as junctions (not spiders), and the pieces that touch them.

use super::*;

// ------------------------------------------------------------ colours

fn green() -> Color32 {
    kind_color("event")
}
fn red() -> Color32 {
    kind_color("token")
}
const BAND_R: Color32 = Color32::from_rgb(0xfb, 0xf3, 0xf3);
const BAND_G: Color32 = Color32::from_rgb(0xf2, 0xf7, 0xf3);
const BAND_T: Color32 = Color32::from_rgb(0xf1, 0xf7, 0xf6);
const TABLE: Color32 = Color32::from_rgb(0x17, 0x8f, 0x7f);

// ------------------------------------------------------------ cites

/// A go-pflow cite at the revision the ontology records (69257b3).
fn go(path: &str, a: u64, b: u64, note: &str) -> String {
    cite_of("goPflow", &format!("metamodel/{path}"), a, b, note)
}
fn road(a: u64, b: u64, note: &str) -> String {
    cite_of("ontologyRoadmap", "ROADMAP.md", a, b, note)
}
fn ttl(a: u64, b: u64, note: &str) -> String {
    cite_of("ontology", "ns/patterns.ttl", a, b, note)
}
/// pflow-rs at aacb7eb.
fn rs(path: &str, a: u64, b: u64, note: &str) -> String {
    rs_cite(path, a, b, note)
}

struct Cites;
impl Cites {
    fn uf_doc() -> String {
        go("unionfind.go", 5, 14, "the representative is the lexicographically smallest member, so Flatten is associative")
    }
    fn uf_union() -> String {
        go("unionfind.go", 44, 55, "union: the smaller root wins")
    }
    fn fuse_doc() -> String {
        go(
            "compose_fuse.go",
            9,
            20,
            "fusion over equivalence classes, not pairwise; a cycle a↔b is one two-member class",
        )
    }
    fn place_classes() -> String {
        go(
            "compose_flatten.go",
            55,
            70,
            "place classes: token and data links, one union-find",
        )
    }
    fn trans_classes() -> String {
        go(
            "compose_flatten.go",
            72,
            88,
            "transition classes: event links, a second union-find",
        )
    }
    fn flat_names() -> String {
        go(
            "compose_flatten.go",
            201,
            227,
            "flat names: wire:<smallest member>, fused:<sorted members joined by +>",
        )
    }
    fn dot() -> String {
        go(
            "composedot.go",
            28,
            33,
            "RenderDOT colours and glyphs per link kind",
        )
    }
    fn token_dir() -> String {
        go(
            "compose.go",
            676,
            683,
            "token link direction rule, checked by Validate before Flatten",
        )
    }
    fn schema() -> String {
        go(
            "compose.go",
            667,
            674,
            "port schema tags must agree when both are set",
        )
    }
    fn leak() -> String {
        ttl(
            493,
            497,
            "pat:idiom-pool-release-saga-commit and its caveat",
        )
    }
    fn road_spider() -> String {
        road(
            410,
            411,
            "lk:event: draw it as a synchronising spider joining two boxes",
        )
    }
    fn road_leak() -> String {
        road(
            481,
            482,
            "the pool-leak caveat as a red annotation on the release ⊗ commit spider",
        )
    }
    fn road_scenario() -> String {
        road(
            289,
            291,
            "4.4: the rollback scenario fails with the pool-leak evidence",
        )
    }
    // pflow-rs @aacb7eb (the prototype's lines were read at c396d46; the
    // solver moved its table into memo.rs and bounded it since)
    fn tt_shared() -> String {
        rs("game.rs", 25, 29, "the yes/no searches share one transposition table (crate::memo), bounded and persisting across calls")
    }
    fn entry() -> String {
        rs(
            "memo.rs",
            53,
            60,
            "Entry { lo, hi, best }: the value lies in [lo, hi], and best did best",
        )
    }
    fn table_policy() -> String {
        rs("memo.rs", 15, 24, "a bounded table: a new key takes an empty slot or the weakest; it never grows past its entries and forgetting never changes a value")
    }
    fn fields() -> String {
        rs(
            "game.rs",
            566,
            567,
            "Solver.tt (a memo::Table) and Solver.history",
        )
    }
    fn memo_default() -> String {
        rs(
            "game.rs",
            588,
            594,
            "Solver::MEMO = 1 << 23: the entries the table holds at most, by default",
        )
    }
    fn hist_init() -> String {
        rs("game.rs", 619, 619, "history: one counter per transition")
    }
    fn with_memo() -> String {
        rs(
            "game.rs",
            633,
            642,
            "with_memo: keeps the table to at most `entries`, inside a search and across calls",
        )
    }
    fn store() -> String {
        rs("game.rs", 1181, 1185, "store: remember that the value lies in [lo, hi], with what the table knew (Table::store)")
    }
    fn store_leaf() -> String {
        rs("game.rs", 1082, 1085, "store at a final position")
    }
    fn store_after() -> String {
        rs(
            "game.rs",
            1168,
            1177,
            "bounds from the window, merged with the old entry, then stored",
        )
    }
    fn solve_one() -> String {
        rs(
            "game.rs",
            943,
            959,
            "solve: one search over the whole reward range when null_window is off",
        )
    }
    fn tt_read() -> String {
        rs(
            "game.rs",
            1069,
            1079,
            "tt.get: bounds lo/hi and the remembered best move (first = e.best)",
        )
    }
    fn ctx_build() -> String {
        rs(
            "game.rs",
            1090,
            1095,
            "Ctx { first, history: &self.history }",
        )
    }
    fn cutoff() -> String {
        rs(
            "game.rs",
            1164,
            1164,
            "chain.cutoff(&mut self.history, t): a cutoff is reported to the pieces",
        )
    }
    fn search() -> String {
        rs(
            "game.rs",
            1050,
            1179,
            "Solver::search: fail-soft alpha-beta",
        )
    }
    fn ctx_doc() -> String {
        rs(
            "order.rs",
            15,
            24,
            "Ctx: what a piece may read (first, history)",
        )
    }
    fn cutoff_default() -> String {
        rs(
            "order.rs",
            36,
            37,
            "Orderer::cutoff: a no-op unless a piece overrides it",
        )
    }
    fn tt_best_key() -> String {
        rs("order.rs", 52, 58, "TtBest::key: t == cx.first")
    }
    fn hist_key() -> String {
        rs("order.rs", 68, 71, "History::key reads cx.history[t]")
    }
    fn hist_cutoff() -> String {
        rs(
            "order.rs",
            73,
            75,
            "History::cutoff: history[t] += 1, the only writer",
        )
    }
    fn chain_cutoff() -> String {
        rs(
            "order.rs",
            277,
            281,
            "Chain::cutoff calls every piece's cutoff",
        )
    }
    fn chain_ties() -> String {
        rs(
            "order.rs",
            197,
            199,
            "ties keep the net's own order of the moves",
        )
    }
    fn chain_default() -> String {
        rs(
            "order.rs",
            205,
            216,
            "Chain::default: tt_best, immediate_win, history",
        )
    }
    fn plan_fixed() -> String {
        rs(
            "plan.rs",
            3,
            4,
            "the search is fixed; a plan chooses what swaps in",
        )
    }
    fn plan_drivers() -> String {
        rs(
            "plan.rs",
            29,
            30,
            "null_window on by default, off when drivers is given without it",
        )
    }
    fn plan_filters() -> String {
        rs(
            "plan.rs",
            41,
            43,
            "filters: Filter names; default Filter::DEFAULT; [] runs none",
        )
    }
    fn plan_memo() -> String {
        rs(
            "plan.rs",
            425,
            427,
            "solver_with: with_memo when the plan has a memo",
        )
    }
    fn plan_check() -> String {
        rs(
            "plan.rs",
            378,
            392,
            "check: a filter or heuristic that can change the value is refused in exact mode",
        )
    }
}

/// A bibliography entry as a cite line.
fn bib(key: &str) -> String {
    let b = &comp::ontology()["bib"][key];
    format!(
        "{}, {} ({}) arXiv:{}",
        s(&b["authors"]),
        s(&b["title"]),
        b["year"],
        s(&b["arxiv"])
    )
}

// ------------------------------------------------------------ geometry

#[derive(Clone, Copy)]
struct R4 {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn overlap(a: R4, b: R4) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// `monoW` / `sansW`: the prototype's width estimates.
fn mono_w(t: &str, size: f32) -> f32 {
    t.chars().count() as f32 * size * 0.61
}
fn sans_w(t: &str, size: f32) -> f32 {
    t.chars().count() as f32 * size * 0.54
}

fn mean(v: &[f32]) -> f32 {
    v.iter().sum::<f32>() / v.len().max(1) as f32
}

/// `curveX`: horizontal-tangent cubic between two design points.
fn curve_x(a: Pos2, b: Pos2) -> Vec<Pos2> {
    let mx = (a.x + b.x) / 2.0;
    cubic_pts(a, Pos2::new(mx, a.y), Pos2::new(mx, b.y), b, 24)
}

/// `curveY`: vertical-tangent cubic.
fn curve_y(a: Pos2, b: Pos2) -> Vec<Pos2> {
    let my = (a.y + b.y) / 2.0;
    cubic_pts(a, Pos2::new(a.x, my), Pos2::new(b.x, my), b, 24)
}

/// `cubicPoints`: the interior samples of a `curveX`.
fn cubic_samples(a: Pos2, b: Pos2, n: usize) -> Vec<Pos2> {
    let mx = (a.x + b.x) / 2.0;
    let all = cubic_pts(a, Pos2::new(mx, a.y), Pos2::new(mx, b.y), b, n);
    all[1..n].to_vec()
}

fn nearest_on(r: R4, x: f32, y: f32) -> Pos2 {
    Pos2::new(x.clamp(r.x, r.x + r.w), y.clamp(r.y, r.y + r.h))
}

struct Obs {
    rects: Vec<R4>,
    pts: Vec<Pos2>,
    bounds: R4,
}

impl Obs {
    fn new(bounds: R4) -> Self {
        Self {
            rects: vec![],
            pts: vec![],
            bounds,
        }
    }
    fn cost(&self, r: R4) -> f32 {
        let b = self.bounds;
        let mut n = 0.0;
        if r.x < b.x || r.y < b.y || r.x + r.w > b.x + b.w || r.y + r.h > b.y + b.h {
            n += 1000.0;
        }
        for q in &self.rects {
            if overlap(r, *q) {
                n += 400.0;
            }
        }
        for p in &self.pts {
            if p.x >= r.x - 8.0
                && p.x <= r.x + r.w + 8.0
                && p.y >= r.y - 6.0
                && p.y <= r.y + r.h + 6.0
            {
                n += 1.0;
            }
        }
        n
    }
    fn best(&self, cands: &[R4]) -> Option<usize> {
        let mut best: Option<(usize, f32)> = None;
        for (i, c) in cands.iter().enumerate() {
            let k = self.cost(*c);
            if best.is_none_or(|(_, bc)| k < bc) {
                best = Some((i, k));
            }
            if k == 0.0 {
                break;
            }
        }
        best.map(|b| b.0)
    }
}

/// `placeBlock`: where a w×h block goes near a spider at (sx, sy). Returns
/// the block and its distance `d` (> 0: it needs a leader line).
#[allow(clippy::too_many_arguments)]
fn place_block(
    obs: &Obs,
    sx: f32,
    sy: f32,
    w: f32,
    h: f32,
    side: f32,
    band: (f32, f32),
    prefer: &str,
) -> (R4, f32) {
    let near: Vec<(f32, f32)> = if prefer == "below" {
        vec![
            (-w / 2.0 + 20.0 * side, 22.0),
            (8.0 * side, 26.0),
            (-w / 2.0, 26.0),
        ]
    } else if side > 0.0 {
        vec![
            (19.0, -h / 2.0),
            (19.0, 9.0),
            (19.0, -h - 9.0),
            (-w / 2.0, 19.0),
            (-w / 2.0, -19.0 - h),
        ]
    } else {
        vec![
            (-19.0 - w, -h / 2.0),
            (-w / 2.0, 19.0),
            (-w / 2.0, -19.0 - h),
            (-19.0 - w, 9.0),
            (-19.0 - w, -h - 9.0),
        ]
    };
    let mut cands: Vec<(R4, f32, f32)> = near
        .iter()
        .enumerate()
        .map(|(i, (dx, dy))| {
            (
                R4 {
                    x: sx + dx,
                    y: sy + dy,
                    w,
                    h,
                },
                0.0,
                i as f32,
            )
        })
        .collect();
    let mut i = cands.len() as f32;
    let mut rr = 34.0;
    while rr <= 320.0 {
        for k in 0..32 {
            let a = k as f32 / 32.0 * std::f32::consts::TAU;
            let (ca, sa) = (a.cos(), a.sin());
            let (px, py) = (sx + ca * rr, sy + sa * rr);
            let x = if ca.abs() < 0.3 {
                px - w / 2.0
            } else if ca > 0.0 {
                px
            } else {
                px - w
            };
            let y = if sa.abs() < 0.3 {
                py - h / 2.0
            } else if sa > 0.0 {
                py
            } else {
                py - h
            };
            cands.push((R4 { x, y, w, h }, rr, i));
            i += 1.0;
        }
        rr += 14.0;
    }
    let mut best: Option<((R4, f32), f32)> = None;
    for (r, d, idx) in &cands {
        if r.x < band.0 || r.x + r.w > band.1 {
            continue;
        }
        let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
        let mut pen = 0.0;
        if prefer == "side" && (cx - sx) * side < -4.0 {
            pen += 60.0;
        }
        if prefer == "below" && cy < sy {
            pen += 120.0;
        }
        let k = obs.cost(*r) * 100000.0 + (d + pen) * 10.0 + idx * 0.001;
        if best.is_none_or(|(_, bk)| k < bk) {
            best = Some(((*r, *d), k));
        }
    }
    best.map(|b| b.0).unwrap_or((
        R4 {
            x: sx + 19.0 * side,
            y: sy - h / 2.0,
            w,
            h,
        },
        0.0,
    ))
}

// ------------------------------------------------------------ drawing helpers

/// Text at a design point; `h` 0 left, 1 centre, 2 right of `x`, `y` the
/// SVG baseline.
#[allow(clippy::too_many_arguments)]
fn tx(
    c: &Ctx,
    xf: Xf,
    x: f32,
    y: f32,
    h: u8,
    t: &str,
    size: f32,
    col: Color32,
    halo: Option<Color32>,
    mono: bool,
) -> Rect {
    let al = match h {
        0 => Align2::LEFT_BOTTOM,
        1 => Align2::CENTER_BOTTOM,
        _ => Align2::RIGHT_BOTTOM,
    };
    c.dt(
        xf,
        Pos2::new(x, y + size * 0.2),
        al,
        t,
        size,
        col,
        halo,
        mono,
    )
}

fn scr(xf: Xf, pts: &[Pos2]) -> Vec<Pos2> {
    pts.iter().map(|p| xf.pt(p.x, p.y)).collect()
}

fn frect(c: &Ctx, xf: Xf, r: R4, round: f32, fill: Color32, stroke: Color32, w: f32) {
    c.p.rect(
        xf.rect(r.x, r.y, r.w, r.h),
        round * xf.k,
        fill,
        Stroke::new(w * xf.k, stroke),
        StrokeKind::Middle,
    );
}

fn dashed_rect(c: &Ctx, xf: Xf, r: R4, fill: Color32, stroke: Color32, w: f32, dash: (f32, f32)) {
    c.p.rect_filled(xf.rect(r.x, r.y, r.w, r.h), 8.0 * xf.k, fill);
    let (x1, y1, x2, y2) = (r.x, r.y, r.x + r.w, r.y + r.h);
    let pts = [
        Pos2::new(x1, y1),
        Pos2::new(x2, y1),
        Pos2::new(x2, y2),
        Pos2::new(x1, y2),
        Pos2::new(x1, y1),
    ];
    c.dash_poly(
        &scr(xf, &pts),
        stroke,
        w * xf.k,
        dash.0 * xf.k,
        dash.1 * xf.k,
    );
}

fn fcirc(c: &Ctx, xf: Xf, at: Pos2, r: f32, fill: Color32, stroke: Color32, w: f32) {
    c.p.circle(
        xf.pt(at.x, at.y),
        r * xf.k,
        fill,
        Stroke::new(w * xf.k, stroke),
    );
}

/// A link kind's leg style: (width, dash).
fn leg_style(kind: &str) -> (f32, Option<(f32, f32)>) {
    match kind {
        "event" => (2.2, None),
        "data" => (2.0, Some((1.5, 4.0))),
        _ => (2.0, Some((6.0, 4.0))),
    }
}

fn stroke_pts(c: &Ctx, xf: Xf, pts: &[Pos2], col: Color32, w: f32, dash: Option<(f32, f32)>) {
    let p = scr(xf, pts);
    match dash {
        None => c.poly(&p, col, w * xf.k),
        Some((d, g)) => c.dash_poly(&p, col, w * xf.k, d * xf.k, g * xf.k),
    }
}

/// `spiderGlyph`: a filled dot with the link glyph in white.
#[allow(clippy::too_many_arguments)]
fn spider_glyph(
    c: &Ctx,
    xf: Xf,
    at: Pos2,
    col: Color32,
    glyph: &str,
    r: f32,
    alpha: f32,
    scale: f32,
) {
    if alpha <= 0.01 {
        return;
    }
    let dark = if col == green() {
        hex("#15502c")
    } else {
        hex("#7a2020")
    };
    let rr = r * scale;
    fcirc(
        c,
        xf,
        at,
        rr,
        col.gamma_multiply(alpha),
        dark.gamma_multiply(alpha),
        1.5 * scale,
    );
    c.dt(
        xf,
        Pos2::new(at.x, at.y + 0.0),
        Align2::CENTER_CENTER,
        glyph,
        14.5 * scale * r / 12.5,
        Color32::WHITE.gamma_multiply(alpha),
        None,
        false,
    );
}

/// `chevron`: the port kind on its stub. `side`: +1 right edge, -1 left.
fn chevron(c: &Ctx, xf: Xf, x: f32, y: f32, kind: &str, side: f32, col: Color32) {
    let tri = |dir: f32| {
        let pts = vec![
            xf.pt(x - 3.0 * dir, y - 3.6),
            xf.pt(x + 3.0 * dir, y),
            xf.pt(x - 3.0 * dir, y + 3.6),
        ];
        c.p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    };
    match kind {
        "in" => tri(-side),
        "out" => tri(side),
        "inout" => {
            let pts = vec![
                xf.pt(x, y - 3.6),
                xf.pt(x + 4.0, y),
                xf.pt(x, y + 3.6),
                xf.pt(x - 4.0, y),
            ];
            c.p.add(Shape::convex_polygon(
                pts,
                Color32::WHITE,
                Stroke::new(1.3 * xf.k, col),
            ));
        }
        _ => fcirc(c, xf, Pos2::new(x, y), 2.6, Color32::WHITE, col, 1.3),
    }
}

/// Word-wrapped lines (`wrapText`).
fn wrap(t: &str, max_w: f32, size: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut cur = String::new();
    for w in t.split(' ') {
        let cand = if cur.is_empty() {
            w.to_string()
        } else {
            format!("{cur} {w}")
        };
        if sans_w(&cand, size) > max_w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        } else {
            cur = cand;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[allow(clippy::too_many_arguments)]
fn wrap_text(
    c: &Ctx,
    xf: Xf,
    x: f32,
    y: f32,
    t: &str,
    max_w: f32,
    size: f32,
    col: Color32,
    lh: f32,
) -> usize {
    let lines = wrap(t, max_w, size);
    for (i, ln) in lines.iter().enumerate() {
        tx(c, xf, x, y + i as f32 * lh, 0, ln, size, col, None, false);
    }
    lines.len()
}

/// Draw order and hit tolerance for a leg in screen px.
const LEG_TOL: f32 = 6.0;

// ------------------------------------------------------------ orders

const BOX_ORDER: [&str; 6] = [
    "orders", "orders2", "workers", "workers2", "payment", "payment2",
];
const BOX_X: f32 = 300.0;
const BOX_W: f32 = 252.0;
const HEAD: f32 = 42.0;
const TOP: f32 = 78.0;

struct Anchor {
    x: f32,
    y: f32,
    side: f32,
    edge: f32,
    sub: usize,
    port: Value,
}

struct BoxL {
    sub: Value,
    y: f32,
    h: f32,
    tp: Vec<Value>,
    pp: Vec<Value>,
}

struct Class {
    event: bool,
    name: String,
    members: Vec<String>,
    links: Vec<Value>,
    terminals: Vec<String>,
    member_of: BTreeMap<String, String>,
    pos: Pos2,
    side: f32,
    label: R4,
    label_d: f32,
    lines: Vec<String>,
}

fn pkey(end: &Value) -> String {
    format!("{}|{}", s(&end["subnet"]), s(&end["port"]))
}

fn link_text(l: &Value) -> String {
    let g = match s(&l["kind"]) {
        "event" => "⊗",
        "data" => "▷",
        "guard" => "⊘",
        _ => "◆",
    };
    format!(
        "{}:{} {g} {}:{}",
        s(&l["from"]["subnet"]),
        s(&l["from"]["port"]),
        s(&l["to"]["subnet"]),
        s(&l["to"]["port"])
    )
}

/// "fused:a+b+c" -> ["fused:a+", "b+", "c"].
fn split_fused(name: &str) -> Vec<String> {
    let parts: Vec<&str> = name.split('+').collect();
    let n = parts.len();
    parts
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if i + 1 < n {
                format!("{p}+")
            } else {
                p.to_string()
            }
        })
        .collect()
}

fn leaks_of(c: &Class) -> Option<&Value> {
    c.links.iter().find(|l| l["leak"] == true)
}

/// A running fusion: pre-spiders (one per link), joined through shared legs,
/// slide into the class's one spider.
struct Group {
    terminals: Vec<String>,
    members: BTreeSet<String>,
    pos: Pos2,
    label: String,
}

struct Fusion {
    groups: Vec<Group>,
    grow: bool,
    p: f32,
}

const HOLD: f32 = 0.35;

impl Fusion {
    /// Progress of the slide, 0 while the pre-spiders hold.
    fn u(&self) -> f32 {
        ease(((self.p - HOLD) / (1.0 - HOLD)).clamp(0.0, 1.0))
    }
    fn owner_of_term(&self, k: &str, member: &str) -> usize {
        self.groups
            .iter()
            .position(|g| g.terminals.iter().any(|t| t == k))
            .or_else(|| self.groups.iter().position(|g| g.members.contains(member)))
            .unwrap_or(0)
    }
    fn body_alpha(&self) -> f32 {
        if self.grow {
            ((self.p - 0.55) / 0.45).clamp(0.0, 1.0)
        } else {
            ((self.p - 0.9) / 0.1).clamp(0.0, 1.0)
        }
    }
    fn body_scale(&self) -> f32 {
        if self.grow {
            0.2 + 0.8 * ease(((self.p - 0.55) / 0.45).clamp(0.0, 1.0))
        } else {
            1.0
        }
    }
}

pub(super) fn spiders_orders(c: &mut Ctx, area: Rect) {
    let Some(flat) = c.d.flat.clone() else { return };
    if flat["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refused this composition ({}): no classes to draw.",
                s(&flat["error"])
            ),
        );
        return;
    }
    let (grn, rd) = (green(), red());
    let mut subs = subnets(c);
    subs.sort_by(|a, b| {
        let rank = |x: &Value| {
            BOX_ORDER
                .iter()
                .position(|i| *i == s(&x["id"]))
                .unwrap_or(100)
        };
        rank(a).cmp(&rank(b)).then(s(&a["id"]).cmp(s(&b["id"])))
    });
    let ls = links(c);

    // ---- boxes and port anchors
    let nsub = subs.len();
    let pitch = if nsub > 4 {
        21.0
    } else if nsub > 3 {
        23.0
    } else {
        26.0
    };
    let gap = if nsub > 3 { 24.0 } else { 30.0 };
    let mut boxes: Vec<BoxL> = vec![];
    let mut anchors: BTreeMap<String, Anchor> = BTreeMap::new();
    let mut anchor_order: Vec<String> = vec![];
    let mut y = TOP;
    for (si, sub) in subs.iter().enumerate() {
        let ports = arr(&sub["ports"]);
        let tp: Vec<Value> = ports
            .iter()
            .filter(|p| s(&p["target"]) == "transition")
            .cloned()
            .collect();
        let pp: Vec<Value> = ports
            .iter()
            .filter(|p| s(&p["target"]) != "transition")
            .cloned()
            .collect();
        let n = tp.len().max(pp.len());
        let h = HEAD + n as f32 * pitch + 8.0;
        let y0 = y + HEAD + 4.0;
        for (i, p) in tp.iter().enumerate() {
            let k = format!("{}|{}", s(&sub["id"]), s(&p["id"]));
            anchors.insert(
                k.clone(),
                Anchor {
                    x: BOX_X + BOX_W + 17.0,
                    y: y0 + pitch * (i as f32 + 0.5 + (n - tp.len()) as f32 / 2.0),
                    side: 1.0,
                    edge: BOX_X + BOX_W,
                    sub: si,
                    port: p.clone(),
                },
            );
            anchor_order.push(k);
        }
        for (i, p) in pp.iter().enumerate() {
            let k = format!("{}|{}", s(&sub["id"]), s(&p["id"]));
            anchors.insert(
                k.clone(),
                Anchor {
                    x: BOX_X - 15.0,
                    y: y0 + pitch * (i as f32 + 0.5 + (n - pp.len()) as f32 / 2.0),
                    side: -1.0,
                    edge: BOX_X,
                    sub: si,
                    port: p.clone(),
                },
            );
            anchor_order.push(k);
        }
        boxes.push(BoxL {
            sub: sub.clone(),
            y,
            h,
            tp,
            pp,
        });
        y += h + gap;
    }
    let col_bottom = y - gap;

    // ---- classes, read from Flatten (never computed here)
    let member_of = |end: &Value, event: bool| -> Option<String> {
        let sub = subs.iter().find(|x| s(&x["id"]) == s(&end["subnet"]))?;
        let p = arr(&sub["ports"]).iter().find(|q| q["id"] == end["port"])?;
        Some(format!(
            "{}/{}",
            s(&sub["id"]),
            if event {
                s(&p["transition"])
            } else {
                s(&p["place"])
            }
        ))
    };
    let mk = |event: bool, name: &str, members: Vec<String>, cl: Vec<Value>| -> Class {
        let mut terminals: Vec<String> = vec![];
        for l in &cl {
            for end in [&l["from"], &l["to"]] {
                let k = pkey(end);
                if anchors.contains_key(&k) && !terminals.contains(&k) {
                    terminals.push(k);
                }
            }
        }
        let mut members = members;
        members.sort();
        let mut member_map = BTreeMap::new();
        for l in &cl {
            for end in [&l["from"], &l["to"]] {
                if let Some(m) = member_of(end, event) {
                    member_map.insert(pkey(end), m);
                }
            }
        }
        let lines = if event {
            split_fused(name)
        } else {
            vec![name.to_string()]
        };
        Class {
            event,
            name: name.to_string(),
            members,
            links: cl,
            terminals,
            member_of: member_map,
            pos: Pos2::ZERO,
            side: if event { 1.0 } else { -1.0 },
            label: R4 {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0,
            },
            label_d: 0.0,
            lines,
        }
    };
    let mut classes: Vec<Class> = vec![];
    let mut fused: Vec<(&String, &Value)> = flat["fused_groups"]
        .as_object()
        .into_iter()
        .flatten()
        .collect();
    fused.sort_by(|a, b| a.0.cmp(b.0));
    let mut wires: Vec<(&String, &Value)> =
        flat["wires"].as_object().into_iter().flatten().collect();
    wires.sort_by(|a, b| a.0.cmp(b.0));
    for (name, members) in &fused {
        let m = strs(members);
        let cl: Vec<Value> = ls
            .iter()
            .filter(|l| {
                s(&l["kind"]) == "event"
                    && member_of(&l["from"], true).is_some_and(|x| m.contains(&x))
            })
            .cloned()
            .collect();
        classes.push(mk(true, name, m, cl));
    }
    let n_event = classes.len();
    for (name, members) in &wires {
        let m = strs(members);
        let cl: Vec<Value> = ls
            .iter()
            .filter(|l| {
                matches!(s(&l["kind"]), "token" | "data")
                    && member_of(&l["from"], false).is_some_and(|x| m.contains(&x))
            })
            .cloned()
            .collect();
        classes.push(mk(false, name, m, cl));
    }
    let guards: Vec<Value> = ls
        .iter()
        .filter(|l| s(&l["kind"]) == "guard")
        .cloned()
        .collect();

    // ---- manifest (what the drawing below draws, one entry per class)
    for cl in &classes {
        c.scene.emit(
            "spider",
            &cl.name,
            json!({"colour": if cl.event { "event" } else { "token" }, "legs": cl.members}),
        );
    }
    for l in &ls {
        if l["leak"] == true {
            c.scene.emit("leak", s(&l["key"]), json!({}));
        }
    }

    // ---- spider lanes
    let anchor_r = BOX_X + BOX_W + 17.0;
    let anchor_l = BOX_X - 15.0;
    let ay = |k: &str| anchors[k].y;
    struct Item {
        idx: usize,
        yc: f32,
        lo: f32,
        hi: f32,
        span: f32,
        lane: usize,
    }
    let assign = |range: std::ops::Range<usize>| -> Vec<Item> {
        let mut items: Vec<Item> = range
            .filter(|i| !classes[*i].terminals.is_empty())
            .map(|i| {
                let cl = &classes[i];
                let ys: Vec<f32> = cl.terminals.iter().map(|k| ay(k)).collect();
                let yc = mean(&ys);
                let lh = cl.lines.len() as f32 * 13.0;
                let mn = ys.iter().cloned().fold(f32::MAX, f32::min);
                let mx = ys.iter().cloned().fold(f32::MIN, f32::max);
                Item {
                    idx: i,
                    yc,
                    lo: mn.min(yc - lh / 2.0) - 12.0,
                    hi: mx.max(yc + lh / 2.0) + 12.0,
                    span: mx - mn,
                    lane: 0,
                }
            })
            .collect();
        items.sort_by(|a, b| {
            a.span
                .total_cmp(&b.span)
                .then(a.yc.total_cmp(&b.yc))
                .then(classes[a.idx].name.cmp(&classes[b.idx].name))
        });
        let mut placed: Vec<(f32, f32, usize)> = vec![];
        for it in &mut items {
            let mut lane = 0;
            for p in &placed {
                if p.0 < it.hi && it.lo < p.1 {
                    lane = lane.max(p.2 + 1);
                }
            }
            it.lane = lane;
            placed.push((it.lo, it.hi, lane));
        }
        items
    };
    let tl = assign(0..n_event);
    let pl = assign(n_event..classes.len());
    const LANE_G: f32 = 190.0;
    const LANE_R: f32 = 120.0;
    for it in &tl {
        classes[it.idx].pos = Pos2::new(anchor_r + 118.0 + it.lane as f32 * LANE_G, it.yc);
    }
    for it in &pl {
        classes[it.idx].pos = Pos2::new(anchor_l - 92.0 - it.lane as f32 * LANE_R, it.yc);
    }
    // A leg that grazes another spider reads as joined to it: each spider, in
    // lane order, moves to the nearest spot clear of the others.
    let leg_samples = |cl: &Class, pos: Pos2| -> Vec<Pos2> {
        cl.terminals
            .iter()
            .flat_map(|k| {
                let a = &anchors[k];
                cubic_samples(Pos2::new(a.x, a.y), pos, 40)
            })
            .collect()
    };
    let mut offs: Vec<(f32, f32)> = vec![];
    for dx in [0.0, -16.0, 16.0, -32.0, 32.0, -48.0, 48.0, -64.0, 64.0] {
        for dy in [0.0, -12.0, 12.0, -24.0, 24.0] {
            offs.push((dx, dy));
        }
    }
    offs.sort_by(|a, b| (a.0.abs() + 1.5 * a.1.abs()).total_cmp(&(b.0.abs() + 1.5 * b.1.abs())));
    let nudge = |classes: &mut Vec<Class>, group: &[usize], side: f32| {
        for &ci in group {
            let mut best: Option<Pos2> = None;
            let mut bs = f32::MIN;
            for (dx, dy) in &offs {
                let p = Pos2::new(classes[ci].pos.x + dx, classes[ci].pos.y + dy);
                if if side > 0.0 {
                    p.x < anchor_r + 70.0
                } else {
                    p.x > anchor_l - 60.0
                } {
                    continue;
                }
                let mut d = f32::MAX;
                let mine = leg_samples(&classes[ci], p);
                for &oi in group {
                    if oi == ci {
                        continue;
                    }
                    let o = &classes[oi];
                    d = d.min(o.pos.distance(p) - 14.0);
                    for q in leg_samples(o, o.pos) {
                        d = d.min(q.distance(p));
                    }
                    for q in &mine {
                        d = d.min(q.distance(o.pos));
                    }
                }
                if d >= 26.0 {
                    best = Some(p);
                    break;
                }
                if d > bs {
                    bs = d;
                    best = Some(p);
                }
            }
            if let Some(b) = best {
                classes[ci].pos = b;
            }
        }
    };
    let tgroup: Vec<usize> = tl.iter().map(|i| i.idx).collect();
    let pgroup: Vec<usize> = pl.iter().map(|i| i.idx).collect();
    nudge(&mut classes, &tgroup, 1.0);
    nudge(&mut classes, &pgroup, -1.0);
    let max_lane_x = classes
        .iter()
        .filter(|x| x.event)
        .map(|x| x.pos.x)
        .fold(0.0, f32::max);
    let w = 1150f32.max(max_lane_x + 200.0);

    // guard routing: around the box column, through the gutter that is shorter
    struct GRoute {
        l: Value,
        a: Pos2,
        b: Pos2,
        gy: f32,
        top: bool,
        ch: f32,
    }
    let (mut n_top, mut n_bot) = (0usize, 0usize);
    let mut groutes: Vec<GRoute> = vec![];
    for (gi, l) in guards.iter().enumerate() {
        let (Some(a), Some(b)) = (anchors.get(&pkey(&l["from"])), anchors.get(&pkey(&l["to"])))
        else {
            continue;
        };
        let top = n_top < 1 && (a.y - TOP) + (b.y - TOP) <= (col_bottom - a.y) + (col_bottom - b.y);
        let gy = if top {
            let g = TOP - 14.0 - n_top as f32 * 18.0;
            n_top += 1;
            g
        } else {
            let g = col_bottom + 24.0 + n_bot as f32 * 20.0;
            n_bot += 1;
            g
        };
        groutes.push(GRoute {
            l: l.clone(),
            a: Pos2::new(a.x, a.y),
            b: Pos2::new(b.x, b.y),
            gy,
            top,
            ch: 24.0 + gi as f32 * 7.0,
        });
    }
    let after_guards = if n_bot > 0 {
        col_bottom + 24.0 + n_bot as f32 * 20.0 + 10.0
    } else {
        col_bottom + 10.0
    };

    // ---- label placement against obstacles (boxes, spiders, legs, guards)
    let mut obs = Obs::new(R4 {
        x: 8.0,
        y: 54.0,
        w: w - 16.0,
        h: 4000.0,
    });
    for b in &boxes {
        obs.rects.push(R4 {
            x: BOX_X - 24.0,
            y: b.y - 4.0,
            w: BOX_W + 48.0,
            h: b.h + 8.0,
        });
    }
    for cl in &classes {
        obs.rects.push(R4 {
            x: cl.pos.x - 15.0,
            y: cl.pos.y - 15.0,
            w: 30.0,
            h: 30.0,
        });
        for k in &cl.terminals {
            let a = &anchors[k];
            obs.pts
                .extend(cubic_samples(Pos2::new(a.x, a.y), cl.pos, 64));
        }
    }
    for r in &groutes {
        obs.rects.push(R4 {
            x: r.a.x.min(r.b.x) - 26.0,
            y: r.gy - 8.0,
            w: (r.a.x - r.b.x).abs() + 52.0,
            h: 16.0,
        });
        let lw = sans_w(
            &format!(
                "⊘ guard {}:{} → {}:{}: a read arc, not a spider",
                s(&r.l["from"]["subnet"]),
                s(&r.l["from"]["port"]),
                s(&r.l["to"]["subnet"]),
                s(&r.l["to"]["port"])
            ),
            11.0,
        ) + 24.0;
        obs.rects.push(R4 {
            x: (r.a.x + r.b.x) / 2.0 - lw / 2.0,
            y: r.gy - 10.0,
            w: lw,
            h: 20.0,
        });
        obs.rects.push(R4 {
            x: r.a.x + r.ch - 4.0,
            y: r.a.y.min(r.gy),
            w: 8.0,
            h: (r.a.y - r.gy).abs(),
        });
        obs.rects.push(R4 {
            x: r.b.x - r.ch - 4.0,
            y: r.b.y.min(r.gy),
            w: 8.0,
            h: (r.b.y - r.gy).abs(),
        });
    }
    let band_g = (BOX_X + BOX_W + 34.0, w - 10.0);
    let band_r = (10.0, BOX_X - 34.0);
    #[allow(clippy::needless_range_loop)]
    for ci in 0..classes.len() {
        if classes[ci].terminals.is_empty() {
            continue;
        }
        let lines = &classes[ci].lines;
        let bw = lines.iter().map(|t| mono_w(t, 11.0)).fold(0.0, f32::max) + 6.0;
        let bh = lines.len() as f32 * 13.0 + 3.0;
        let side = classes[ci].side;
        let pos = classes[ci].pos;
        let (r, d) = place_block(
            &obs,
            pos.x,
            pos.y,
            bw,
            bh,
            side,
            if side > 0.0 { band_g } else { band_r },
            "side",
        );
        classes[ci].label = r;
        classes[ci].label_d = d;
        obs.rects.push(r);
    }
    // the pool-leak annotation on every class holding a link that
    // instantiates pat:idiom-pool-release-saga-commit
    let mut leak_boxes: Vec<(usize, R4)> = vec![];
    let leak_lines = [
        "fires only on commit: on rollback the",
        "worker stays in in_use",
        "(pat:idiom-pool-release-saga-commit caveat)",
    ];
    #[allow(clippy::needless_range_loop)]
    for ci in 0..n_event {
        if leaks_of(&classes[ci]).is_none() || classes[ci].terminals.is_empty() {
            continue;
        }
        let bw = leak_lines
            .iter()
            .map(|t| sans_w(t, 11.0))
            .fold(0.0, f32::max)
            + 14.0;
        let bh = leak_lines.len() as f32 * 14.0 + 8.0;
        let pos = classes[ci].pos;
        let (r, _) = place_block(&obs, pos.x, pos.y, bw, bh, 1.0, band_g, "below");
        leak_boxes.push((ci, r));
        obs.rects.push(r);
    }
    let sp_bottom = classes
        .iter()
        .map(|x| x.label.y + x.label.h)
        .chain(leak_boxes.iter().map(|(_, r)| r.y + r.h))
        .fold(0.0, f32::max);
    let legend_y = after_guards.max(sp_bottom).max(col_bottom) + 26.0;
    let h_total = legend_y + 84.0 + 18.0;

    // ---- merge animation (a link was just added)
    let anim = c.anim(|e| match e {
        Event::LinkAdded { key, .. } => Some(key.clone()),
        _ => None,
    });
    let mut fusions: BTreeMap<usize, Fusion> = BTreeMap::new();
    let mut guard_fade: Option<(String, f32)> = None;
    if let Some((key, p)) = &anim {
        if let Some(l) = ls.iter().find(|l| s(&l["key"]) == key) {
            let kind = s(&l["kind"]);
            if kind == "guard" {
                guard_fade = Some((key.clone(), *p));
            } else if let Some(ci) = classes.iter().position(|cl| {
                cl.links.iter().any(|x| s(&x["key"]) == key) && !cl.terminals.is_empty()
            }) {
                let cl = &classes[ci];
                let n = cl.links.len();
                let spread = 62.0;
                let groups: Vec<Group> = cl
                    .links
                    .iter()
                    .enumerate()
                    .map(|(i, lk)| {
                        let terms: Vec<String> = [pkey(&lk["from"]), pkey(&lk["to"])]
                            .into_iter()
                            .filter(|k| anchors.contains_key(k))
                            .collect();
                        let ys: Vec<f32> = terms.iter().map(|k| ay(k)).collect();
                        Group {
                            members: terms
                                .iter()
                                .filter_map(|k| cl.member_of.get(k).cloned())
                                .collect(),
                            pos: Pos2::new(
                                cl.pos.x + (i as f32 - (n as f32 - 1.0) / 2.0) * spread,
                                mean(&ys),
                            ),
                            label: format!(
                                "{} {} {}",
                                s(&lk["from"]["port"]),
                                kind_glyph(s(&lk["kind"])),
                                s(&lk["to"]["port"])
                            ),
                            terminals: terms,
                        }
                    })
                    .collect();
                fusions.insert(
                    ci,
                    Fusion {
                        groups,
                        grow: n == 1,
                        p: *p,
                    },
                );
            }
        }
    }

    // ---- draw
    let xf = Xf::fit(area, w, h_total, 1.5);
    let area_d = xf.rect(0.0, 0.0, w, h_total);
    c.p.rect_filled(area_d, 0.0, PAPER);
    c.p.rect_filled(xf.rect(0.0, 0.0, BOX_X - 30.0, h_total), 0.0, BAND_R);
    c.p.rect_filled(
        xf.rect(
            BOX_X + BOX_W + 30.0,
            0.0,
            w - (BOX_X + BOX_W + 30.0),
            h_total,
        ),
        0.0,
        BAND_G,
    );
    // headings
    for (x, h, t1, t2, col) in [
        (
            16.0,
            0u8,
            "PLACE CLASSES · red",
            "token and data links: one union-find",
            rd,
        ),
        (
            BOX_X + BOX_W / 2.0,
            1,
            "INSTANCES",
            "opaque boxes, ⊗ down the page",
            INK,
        ),
        (
            BOX_X + BOX_W + 44.0,
            0,
            "TRANSITION CLASSES · green",
            "event links: a second union-find (compose_flatten.go:72-88)",
            grn,
        ),
    ] {
        let r1 = tx(c, xf, x, 24.0, h, t1, 11.0, col, None, false);
        let r2 = tx(c, xf, x, 38.0, h, t2, 10.5, MUTED, None, false);
        let hit = r1.union(r2);
        if t1.starts_with("PLACE") {
            c.hover(
                hit,
                info("place classes: token and data links")
                    .fact(
                        "union-find",
                        "one, over every place (compose_flatten.go:55-70)",
                    )
                    .fact(
                        "flat name",
                        "wire:<smallest member> (compose_flatten.go:201-209)",
                    )
                    .note("Red is ZX's other colour, borrowed for shape only.")
                    .cite(Cites::place_classes())
                    .cite(Cites::uf_doc())
                    .cite(Cites::flat_names())
                    .cite(Cites::token_dir())
                    .cite(Cites::schema())
                    .cite(bib("coecke-duncan")),
            );
        } else if t1.starts_with("TRANS") {
            c.hover(hit, info("transition classes: event links")
                .fact("union-find", "a second one, over every transition (compose_flatten.go:72-88)")
                .fact("flat name", "fused:<sorted members joined by +> (compose_flatten.go:211-227)")
                .fact("fires", "as one rendezvous: enabled only when every member is (compose_fuse.go:9-14)")
                .note("Green is ZX's Z colour, borrowed for shape only. Green never fuses with red: two separate union-finds.")
                .cite(Cites::trans_classes())
                .cite(Cites::fuse_doc())
                .cite(Cites::uf_doc())
                .cite(Cites::flat_names())
                .cite(Cites::road_spider())
                .cite(bib("coecke-duncan"))
                .cite(bib("bruni-et-al")));
        }
    }
    if n_event == classes.len() {
        let extra = subs.iter().any(|x| x["extra"] == true);
        let lines: &[&str] = if extra {
            &[
                "No place classes yet: no token or data",
                "link. Drag from a round port to a port",
                "of the same schema on another instance;",
                "go-pflow's rules decide (compose.go:",
                "667-674 for schemas).",
            ]
        } else {
            &[
                "No place classes. Token and data links",
                "need agreeing port schemas, and these",
                "patterns' place ports all differ",
                "(cq/13; compose.go:667-674). Add a",
                "second instance in the toolbar to",
                "link one (a red spider).",
            ]
        };
        for (i, ln) in lines.iter().enumerate() {
            tx(
                c,
                xf,
                16.0,
                TOP + 18.0 + i as f32 * 15.0,
                0,
                ln,
                11.0,
                MUTED,
                None,
                false,
            );
        }
    }
    if n_event == 0 {
        let lines = [
            "No transition classes: no event links.",
            "Drag from a square port to a square",
            "port of another instance.",
        ];
        for (i, ln) in lines.iter().enumerate() {
            tx(
                c,
                xf,
                BOX_X + BOX_W + 44.0,
                TOP + 18.0 + i as f32 * 15.0,
                0,
                ln,
                11.0,
                MUTED,
                None,
                false,
            );
        }
    }

    // guards (not spiders): under the legs
    for r in &groutes {
        let v = if r.top { -1.0 } else { 1.0 };
        let (ax, bx) = (r.a.x + r.ch, r.b.x - r.ch);
        let pts = [
            r.a,
            Pos2::new(ax, r.a.y),
            Pos2::new(ax, r.gy),
            Pos2::new(bx, r.gy),
            Pos2::new(bx, r.b.y),
            r.b,
        ];
        let _ = v;
        let path = rounded_pts(&pts, 10.0);
        let mut col = kind_color("guard");
        if let Some((k, p)) = &guard_fade {
            if *k == s(&r.l["key"]) {
                col = col.gamma_multiply(0.35 + 0.65 * p);
            }
        }
        stroke_pts(c, xf, &path, col, 1.8, Some((6.0, 4.0)));
        let mid_x = (r.a.x + r.b.x) / 2.0;
        let label = format!(
            "⊘ guard {}:{} → {}:{}: a read arc, not a spider",
            s(&r.l["from"]["subnet"]),
            s(&r.l["from"]["port"]),
            s(&r.l["to"]["subnet"]),
            s(&r.l["to"]["port"])
        );
        let lw = sans_w(&label, 11.0) + 16.0;
        frect(
            c,
            xf,
            R4 {
                x: mid_x - lw / 2.0,
                y: r.gy - 8.5,
                w: lw,
                h: 17.0,
            },
            8.5,
            Color32::from_rgb(0xff, 0xfa, 0xf0),
            col,
            1.0,
        );
        tx(c, xf, mid_x, r.gy + 4.0, 1, &label, 11.0, col, None, false);
        let hit_pts = scr(xf, &path);
        let i = link_info(&r.l).note("Not a spider: a guard lowers to a read arc on the gated transition, nothing fuses, and it sits outside the free-SMC fragment (ROADMAP Phase 6). W_RESTRICTIVE_LINK.");
        c.hover_poly(&hit_pts, LEG_TOL, i);
    }

    // legs
    let mut leg_polys: Vec<(Vec<Pos2>, Info)> = vec![];
    let mut spider_draw: Vec<usize> = vec![];
    for (ci, cl) in classes.iter().enumerate() {
        if cl.terminals.is_empty() {
            continue;
        }
        spider_draw.push(ci);
        let fz = fusions.get(&ci);
        for k in &cl.terminals {
            let a = &anchors[k];
            let ap = Pos2::new(a.x, a.y);
            let via: Vec<&Value> = cl
                .links
                .iter()
                .filter(|l| pkey(&l["from"]) == *k || pkey(&l["to"]) == *k)
                .collect();
            let kind = via.first().map_or("event", |l| s(&l["kind"]));
            let end = match fz {
                Some(f) if f.grow => lerp2(Pos2::new(a.x + 6.0 * a.side, a.y), cl.pos, ease(f.p)),
                Some(f) => {
                    let o = f.owner_of_term(k, cl.member_of.get(k).map_or("", String::as_str));
                    lerp2(f.groups[o].pos, cl.pos, f.u())
                }
                None => cl.pos,
            };
            let col = kind_color(kind);
            let (w_, dash) = leg_style(kind);
            let pts = curve_x(ap, end);
            stroke_pts(c, xf, &pts, col, w_, dash);
            let base = via
                .first()
                .map(|l| link_info(l))
                .unwrap_or_else(|| info("leg"));
            let mut i = base;
            i.title = format!(
                "leg {} of a {} spider",
                k.replace('|', ":"),
                if cl.event { "green" } else { "red" }
            );
            i = i
                .fact("member", cl.member_of.get(k).cloned().unwrap_or_default())
                .fact(
                    "links through this leg",
                    via.iter()
                        .map(|l| link_text(l))
                        .collect::<Vec<_>>()
                        .join("; "),
                )
                .fact("spider", cl.name.as_str());
            i.note = if cl.event {
                "An undirected leg: event links have no direction rule.".into()
            } else {
                "A leg into the place class; its style is its link kind's (RenderDOT).".into()
            };
            leg_polys.push((scr(xf, &pts), i));
        }
    }

    // boxes
    for (bi, b) in boxes.iter().enumerate() {
        let sub = &b.sub;
        let pat = &comp::ontology()["patterns"][s(&sub["pattern"])];
        let rect = R4 {
            x: BOX_X,
            y: b.y,
            w: BOX_W,
            h: b.h,
        };
        frect(c, xf, rect, 7.0, Color32::WHITE, hex("#c3c9d2"), 1.2);
        c.p.rect_filled(
            xf.rect(BOX_X + 0.6, b.y + 0.6, BOX_W - 1.2, HEAD - 0.6),
            egui::CornerRadius {
                nw: (7.0 * xf.k) as u8,
                ne: (7.0 * xf.k) as u8,
                sw: 0,
                se: 0,
            },
            pattern_tint(s(&sub["pattern"])),
        );
        stroke_pts(
            c,
            xf,
            &[
                Pos2::new(BOX_X, b.y + HEAD),
                Pos2::new(BOX_X + BOX_W, b.y + HEAD),
            ],
            hex("#dde1e6"),
            1.0,
            None,
        );
        tx(
            c,
            xf,
            BOX_X + 12.0,
            b.y + 18.0,
            0,
            s(&sub["id"]),
            13.5,
            INK,
            None,
            false,
        );
        tx(
            c,
            xf,
            BOX_X + 12.0,
            b.y + 33.0,
            0,
            &format!("pat:{} · {}", s(&sub["pattern"]), s(&sub["net_type"])),
            10.5,
            MUTED,
            None,
            false,
        );
        tx(
            c,
            xf,
            BOX_X + BOX_W - 10.0,
            b.y + 18.0,
            2,
            &format!(
                "{} places · {} transitions",
                arr(&sub["places"]).len(),
                arr(&sub["transitions"]).len()
            ),
            10.5,
            MUTED,
            None,
            false,
        );
        for p in &b.pp {
            let a = &anchors[&format!("{}|{}", s(&sub["id"]), s(&p["id"]))];
            tx(
                c,
                xf,
                BOX_X + 10.0,
                a.y + 3.8,
                0,
                s(&p["id"]),
                10.5,
                INK,
                None,
                true,
            );
        }
        for p in &b.tp {
            let a = &anchors[&format!("{}|{}", s(&sub["id"]), s(&p["id"]))];
            tx(
                c,
                xf,
                BOX_X + BOX_W - 10.0,
                a.y + 3.8,
                2,
                s(&p["id"]),
                10.5,
                INK,
                None,
                true,
            );
        }
        if sub["extra"] == true {
            tx(
                c,
                xf,
                BOX_X + BOX_W - 10.0,
                b.y + 33.0,
                2,
                "second instance",
                10.0,
                MUTED,
                None,
                false,
            );
        }
        let mut i = subnet_info(sub);
        i.note = format!(
            "{} drawn as an opaque box: its internal net is hidden; only its ports (the legs) take part in fusion.",
            s(&pat["prefLabel"])
        );
        c.hover(xf.rect(rect.x, rect.y, rect.w, rect.h), i);
        let _ = bi;
    }

    // legs hover after the boxes would hide them; ports are drawn on top
    for (pts, i) in leg_polys {
        stroke_nothing(c, &pts);
        c.hover_poly(&pts, LEG_TOL, i);
    }

    // ports: stub, direction chevron, shape, a hit area >= 20 px
    for k in &anchor_order {
        let a = &anchors[k];
        let p = &a.port;
        let sub = &subs[a.sub];
        let is_t = s(&p["target"]) == "transition";
        let col0 = proto_port_color(p);
        let x0 = a.edge.min(a.x) - if a.side < 0.0 { 10.0 } else { 0.0 };
        let x1 = a.edge.max(a.x) + if a.side > 0.0 { 10.0 } else { 0.0 };
        let hit = xf.rect(x0, a.y - 10.0, x1 - x0, 20.0);
        let kind_s = s(&p["kind"]).to_string();
        let z = c.port_zone(hit, s(&sub["id"]), p, |i| {
            i.fact(
                "in this view",
                if is_t {
                    "a leg on the right edge: an event link joins it to a green spider (a guard link starts here, as a wire)"
                } else {
                    "a leg on the left edge: a token or data link joins it to a red spider (a guard link ends here, as a wire)"
                },
            )
        });
        let col = if z.dim {
            col0.gamma_multiply(0.3)
        } else {
            col0
        };
        let stub = [Pos2::new(a.edge, a.y), Pos2::new(a.x, a.y)];
        stroke_pts(
            c,
            xf,
            &stub,
            col,
            1.6,
            if kind_s == "observe" {
                Some((2.0, 2.0))
            } else {
                None
            },
        );
        chevron(c, xf, (a.edge + a.x) / 2.0, a.y, &kind_s, a.side, col);
        let at = xf.pt(a.x, a.y);
        if is_t {
            let r = Rect::from_center_size(at, Vec2::splat(10.0 * xf.k));
            c.p.rect(
                r,
                0.0,
                col,
                Stroke::new(1.0 * xf.k, Color32::WHITE),
                StrokeKind::Middle,
            );
            c.zone_ring_rect(r.expand(4.0 * xf.k), &z);
        } else {
            let fill = if kind_s == "observe" {
                Color32::WHITE
            } else {
                col
            };
            c.p.circle(at, 4.2 * xf.k, fill, Stroke::new(1.6 * xf.k, col));
            c.zone_ring_circle(at, 9.0 * xf.k, &z);
        }
    }

    // spiders
    for &ci in &spider_draw {
        let cl = &classes[ci];
        let col = if cl.event { grn } else { rd };
        let fz = fusions.get(&ci);
        let glyph = if cl.event {
            kind_glyph("event")
        } else if cl.links.iter().any(|l| s(&l["kind"]) == "token") {
            kind_glyph("token")
        } else {
            kind_glyph("data")
        };
        let (alpha, scale) = fz.map_or((1.0, 1.0), |f| (f.body_alpha(), f.body_scale()));
        spider_glyph(c, xf, cl.pos, col, glyph, 12.5, alpha, scale);
        // label (with a leader line when it had to move away)
        let halo = if cl.event { BAND_G } else { BAND_R };
        if alpha > 0.01 {
            if cl.label_d > 0.0 {
                let q = nearest_on(cl.label, cl.pos.x, cl.pos.y);
                let dd = q.distance(cl.pos).max(1.0);
                let from = cl.pos + (q - cl.pos) * (14.0 / dd);
                c.dash_poly(
                    &[xf.pt(from.x, from.y), xf.pt(q.x, q.y)],
                    col.gamma_multiply(0.8 * alpha),
                    xf.k,
                    2.0 * xf.k,
                    3.0 * xf.k,
                );
            }
            let tcol = if cl.event {
                hex("#1b5e36")
            } else {
                hex("#8a2a2a")
            }
            .gamma_multiply(alpha);
            for (i, ln) in cl.lines.iter().enumerate() {
                tx(
                    c,
                    xf,
                    cl.label.x + 2.0,
                    cl.label.y + 10.5 + i as f32 * 13.0,
                    0,
                    ln,
                    11.0,
                    tcol,
                    Some(halo),
                    true,
                );
            }
        }
        c.hover(
            Rect::from_center_size(xf.pt(cl.pos.x, cl.pos.y), Vec2::splat(25.0 * xf.k)),
            spider_info(c, cl),
        );
    }

    // pre-spiders and shared legs of a running fusion
    for (&ci, f) in &fusions {
        let cl = &classes[ci];
        if f.grow {
            continue;
        }
        let col = if cl.event { grn } else { rd };
        let halo = if cl.event { BAND_G } else { BAND_R };
        let u = f.u();
        let owner_of_member = |m: &String| f.groups.iter().position(|g| g.members.contains(m));
        let mut seen: BTreeSet<(usize, usize)> = BTreeSet::new();
        for (gi, g) in f.groups.iter().enumerate() {
            for m in &g.members {
                let Some(o) = owner_of_member(m) else {
                    continue;
                };
                if o == gi || !seen.insert((o, gi)) {
                    continue;
                }
                let a_pos = lerp2(g.pos, cl.pos, u);
                let b_pos = lerp2(f.groups[o].pos, cl.pos, u);
                let alpha = if f.p < HOLD + (1.0 - HOLD) * 0.6 {
                    1.0
                } else {
                    1.0 - (f.p - (HOLD + (1.0 - HOLD) * 0.6)) / (1.0 - (HOLD + (1.0 - HOLD) * 0.6))
                }
                .clamp(0.0, 1.0);
                stroke_pts(
                    c,
                    xf,
                    &curve_y(a_pos, b_pos),
                    col.gamma_multiply(alpha),
                    2.2,
                    None,
                );
                let la = if f.p < HOLD {
                    1.0
                } else {
                    1.0 - (f.p - HOLD) / ((1.0 - HOLD) * 0.35)
                }
                .clamp(0.0, 1.0);
                if la > 0.01 {
                    let mx = (g.pos.x + f.groups[o].pos.x) / 2.0 + 8.0 * cl.side;
                    let my = (g.pos.y + f.groups[o].pos.y) / 2.0 + 4.0;
                    tx(
                        c,
                        xf,
                        mx,
                        my,
                        if cl.side > 0.0 { 0 } else { 2 },
                        &format!("shared leg: {m}"),
                        10.0,
                        col.gamma_multiply(la),
                        Some(halo.gamma_multiply(la)),
                        true,
                    );
                }
            }
        }
        for g in &f.groups {
            let at = lerp2(g.pos, cl.pos, u);
            let alpha = if f.p < 0.93 {
                1.0
            } else {
                (1.0 - (f.p - 0.93) / 0.07).clamp(0.0, 1.0)
            };
            let glyph = if cl.event {
                kind_glyph("event")
            } else {
                kind_glyph("token")
            };
            spider_glyph(c, xf, at, col, glyph, 12.5, alpha, 1.0);
            let la = if f.p < HOLD {
                1.0
            } else {
                1.0 - (f.p - HOLD) / ((1.0 - HOLD) * 0.3)
            }
            .clamp(0.0, 1.0);
            if la > 0.01 {
                tx(
                    c,
                    xf,
                    at.x + 17.0 * cl.side,
                    at.y + 4.0,
                    if cl.side > 0.0 { 0 } else { 2 },
                    &g.label,
                    10.5,
                    col.gamma_multiply(la),
                    Some(halo),
                    true,
                );
            }
        }
    }

    // the leak annotations
    for (ci, lb) in &leak_boxes {
        let cl = &classes[*ci];
        let sp = cl.pos;
        let q = nearest_on(*lb, sp.x, sp.y);
        let dd = q.distance(sp).max(1.0);
        let from = sp + (q - sp) * (14.0 / dd);
        c.dash_poly(
            &[xf.pt(from.x, from.y), xf.pt(q.x, q.y)],
            LEAK,
            1.3 * xf.k,
            3.0 * xf.k,
            3.0 * xf.k,
        );
        frect(c, xf, *lb, 5.0, hex("#fdf0ee"), LEAK, 1.1);
        for (i, ln) in leak_lines.iter().enumerate() {
            tx(
                c,
                xf,
                lb.x + 7.0,
                lb.y + 16.0 + i as f32 * 14.0,
                0,
                ln,
                11.0,
                LEAK,
                None,
                false,
            );
        }
        if let Some(l) = leaks_of(cl) {
            let idiom = comp::ontology()["idioms"]
                .as_array()
                .and_then(|a| a.iter().find(|x| x["key"] == l["idiom"]));
            let mut i = link_info(l);
            i.title = "pool-leak caveat on release ⊗ commit".into();
            if let Some(idm) = idiom {
                i = i.fact("caveat (verbatim)", s(&idm["caveat"]));
            }
            i.note = "Fusion only says release and commit fire as one. Nothing fires release on the rollback path, so a worker taken by acquire stays in in_use: the expected failure ROADMAP 4.4 records.".into();
            i.cites.insert(0, Cites::road_scenario());
            i.cites.insert(0, Cites::road_leak());
            i.cites.insert(0, Cites::leak());
            c.hover(xf.rect(lb.x, lb.y, lb.w, lb.h), i);
        }
    }

    orders_legend(c, xf, legend_y, w, &subs);
    // caption: what fused
    let multi: Vec<&Class> = classes.iter().filter(|x| x.links.len() > 1).collect();
    let caption = if !multi.is_empty() {
        multi
            .iter()
            .map(|x| {
                format!(
                    "{} links share members → one spider {}",
                    x.links.len(),
                    x.name
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    } else if !classes.is_empty() {
        "no two links share a member, so every spider has two legs".to_string()
    } else if !ls.is_empty() {
        "no event, token or data link: no spiders (a guard never forms one)".to_string()
    } else {
        "no links: no spiders".to_string()
    };
    tx(
        c,
        xf,
        16.0,
        h_total - 8.0,
        0,
        &caption,
        11.0,
        MUTED,
        None,
        false,
    );

    c.note("honesty", "Analogy of shape only: ZX spiders without phases or any quantum semantics. The one law used, fusion of same-colour spiders that share a leg, is what go-pflow's Flatten does with its union-finds (compose_fuse.go:9-20); green never fuses with red because Flatten keeps transition classes and place classes apart (compose_flatten.go:55-88).");
    c.note("analogy", "A red spider is red for its union-find, not its link kind: token and data links share one union-find, so each leg keeps its own RenderDOT style (data: dotted blue). A cycle of event links a↔b is one class (compose_fuse.go:19-20), drawn as one spider, never a loop.");
    c.note("na", "Guard links are not spiders: with the default condition \"> 0\" they lower to a read arc, and they sit outside the free-SMC fragment (ROADMAP Phase 6). They are drawn as an ochre ⊘ wire and nothing fuses.");
}

/// Hit-testing of legs only needs the polyline; nothing is drawn twice.
fn stroke_nothing(_c: &Ctx, _pts: &[Pos2]) {}

fn spider_info(c: &Ctx, cl: &Class) -> Info {
    let flat = c.d.flat.as_ref();
    let legs = cl
        .terminals
        .iter()
        .map(|k| k.replace('|', ":"))
        .collect::<Vec<_>>()
        .join(", ");
    let fused = cl
        .links
        .iter()
        .map(link_text)
        .collect::<Vec<_>>()
        .join("; ");
    if cl.event {
        let t = flat
            .map(|f| arr(&f["transitions"]).to_vec())
            .unwrap_or_default()
            .into_iter()
            .find(|t| s(&t["id"]) == cl.name);
        let mut i = info(format!(
            "green spider: one transition class of {}",
            cl.members.len()
        ))
        .fact("flat name", cl.name.as_str())
        .fact("members", cl.members.join(", "));
        if let Some(t) = &t {
            i = i.fact("flat description", s(&t["description"]));
        }
        i = i
            .fact("fused", "one rendezvous: pre/postset is the union of the members', so it fires only when every member could")
            .fact("links fused", fused)
            .fact("legs", legs)
            .fact(
                "representative",
                format!("{} (the union-find root, the smallest member; the flat name lists every member, sorted)", cl.members.first().cloned().unwrap_or_default()),
            );
        i.note = format!(
            "Analogy of shape: a ZX spider with no phase. {} It fires as one rendezvous (Bruni et al.), not as a linear map.",
            if cl.links.len() > 1 {
                format!("{} links share members, so Flatten's union-find puts them in one class; the drawing fuses their spiders, which is the only ZX law borrowed.", cl.links.len())
            } else {
                "One link: a two-legged spider.".to_string()
            }
        );
        i.cites = vec![
            Cites::trans_classes(),
            Cites::fuse_doc(),
            Cites::uf_doc(),
            Cites::uf_union(),
            Cites::flat_names(),
            bib("coecke-duncan"),
            bib("bruni-et-al"),
        ];
        i
    } else {
        let p = flat
            .map(|f| arr(&f["places"]).to_vec())
            .unwrap_or_default()
            .into_iter()
            .find(|p| s(&p["id"]) == cl.name);
        let mut i = info(format!(
            "red spider: one place class of {}",
            cl.members.len()
        ))
        .fact("flat name", cl.name.as_str())
        .fact("members", cl.members.join(", "));
        if let Some(p) = &p {
            i = i.fact("initial", p["initial"].to_string()).fact(
                "capacity",
                if p["capacity"].as_i64().unwrap_or(0) > 0 {
                    p["capacity"].to_string()
                } else {
                    "0 (unbounded)".into()
                },
            );
        }
        i = i
            .fact(
                "fused",
                "a wire: places fused by token/data links; initial markings summed",
            )
            .fact("links fused", fused)
            .fact("legs", legs)
            .fact(
                "representative",
                format!(
                    "{} (the union-find root, the smallest member; the wire is named after it)",
                    cl.members.first().cloned().unwrap_or_default()
                ),
            );
        i.note = "Analogy of shape: ZX's other colour. Red is the place-class union-find, which token and data links share; each leg keeps its link kind's RenderDOT style. A red spider never fuses with a green one.".into();
        i.cites = vec![
            Cites::place_classes(),
            Cites::uf_doc(),
            Cites::flat_names(),
            bib("coecke-duncan"),
            bib("baez-master"),
        ];
        i
    }
}

fn orders_legend(c: &mut Ctx, xf: Xf, y: f32, w: f32, subs: &[Value]) {
    let (grn, rd) = (green(), red());
    c.p.rect_filled(xf.rect(0.0, y - 16.0, w, 118.0), 0.0, PAPER);
    stroke_pts(
        c,
        xf,
        &[Pos2::new(16.0, y - 16.0), Pos2::new(w - 16.0, y - 16.0)],
        hex("#dde1e6"),
        1.0,
        None,
    );
    let schemas: BTreeSet<String> = subs
        .iter()
        .flat_map(|x| arr(&x["ports"]).iter().map(|p| s(&p["schema"]).to_string()))
        .filter(|x| !x.is_empty())
        .collect();
    let mut hover_rects: Vec<Rect> = vec![];
    // row 1: ports
    let mut cx = 16.0;
    let item = |c: &Ctx,
                cx: &mut f32,
                cy: f32,
                tx_off: f32,
                text: &str,
                muted: bool,
                mono: bool,
                gap: f32|
     -> Rect {
        let r = tx(
            c,
            xf,
            *cx + 8.0 + tx_off,
            cy + 4.0,
            0,
            text,
            11.0,
            if muted { MUTED } else { INK },
            None,
            mono,
        );
        *cx += 8.0
            + tx_off
            + if mono {
                mono_w(text, 11.0)
            } else {
                sans_w(text, 11.0)
            }
            + gap;
        r
    };
    let c0 = cx;
    let r = Rect::from_center_size(xf.pt(cx + 8.0, y), Vec2::splat(10.0 * xf.k));
    c.p.rect_filled(r, 0.0, hex("#3c4656"));
    item(c, &mut cx, y, 14.0, "transition port", false, false, 18.0);
    c.p.circle(
        xf.pt(cx + 8.0, y),
        4.2 * xf.k,
        Color32::WHITE,
        Stroke::new(1.4 * xf.k, hex("#888888")),
    );
    item(
        c,
        &mut cx,
        y,
        14.0,
        "place port, coloured by its schema:",
        false,
        false,
        8.0,
    );
    for sch in [
        "queue:items",
        "queue:payload",
        "resource",
        "account",
        "saga:outcome",
    ] {
        if !schemas.contains(sch) {
            continue;
        }
        c.p.circle_filled(xf.pt(cx + 8.0, y), 4.2 * xf.k, proto_schema_color(sch));
        item(c, &mut cx, y, 9.0, sch, false, true, 6.0);
    }
    hover_rects.push(Rect::from_min_max(xf.pt(c0, y - 8.0), xf.pt(cx, y + 8.0)));
    // row 2: port kinds
    let y2 = y + 21.0;
    tx(
        c,
        xf,
        24.0,
        y2 + 4.0,
        0,
        "arrow on a stub = the port's kind:",
        11.0,
        MUTED,
        None,
        false,
    );
    cx = 16.0 + 8.0 + sans_w("arrow on a stub = the port's kind:", 11.0) + 14.0;
    for (kind, text) in [
        ("in", "in (into the box)"),
        ("out", "out (away from it)"),
        ("inout", "inout"),
        ("observe", "observe (a tap)"),
    ] {
        let mx = cx + 8.0;
        let g = hex("#555555");
        let line = [Pos2::new(mx - 9.0, y2), Pos2::new(mx + 9.0, y2)];
        stroke_pts(
            c,
            xf,
            &line,
            g,
            1.5,
            if kind == "observe" {
                Some((2.0, 2.0))
            } else {
                None
            },
        );
        c.p.rect_filled(xf.rect(mx - 12.0, y2 - 3.0, 3.0, 6.0), 0.0, g);
        chevron(c, xf, mx, y2, kind, 1.0, g);
        item(c, &mut cx, y2, 16.0, text, false, false, 18.0);
    }
    // row 3: spiders
    let y3 = y + 44.0;
    cx = 16.0;
    spider_glyph(
        c,
        xf,
        Pos2::new(cx + 8.0, y3),
        grn,
        kind_glyph("event"),
        8.5,
        1.0,
        1.0,
    );
    let r1 = item(
        c,
        &mut cx,
        y3,
        14.0,
        "transition class: event links fuse transitions",
        false,
        false,
        18.0,
    );
    spider_glyph(
        c,
        xf,
        Pos2::new(cx + 8.0, y3),
        rd,
        kind_glyph("token"),
        8.5,
        1.0,
        1.0,
    );
    item(
        c,
        &mut cx,
        y3,
        14.0,
        "place class: token / data links fuse places",
        false,
        false,
        18.0,
    );
    stroke_pts(
        c,
        xf,
        &[Pos2::new(cx - 1.0, y3), Pos2::new(cx + 17.0, y3)],
        kind_color("guard"),
        2.0,
        Some((4.0, 3.0)),
    );
    item(
        c,
        &mut cx,
        y3,
        14.0,
        "⊘ guard: a read arc, never a spider",
        false,
        false,
        18.0,
    );
    let _ = r1;
    let note = "Green fuses only with green and red only with red: Flatten keeps two union-finds. A parallel to ZX's two colours, not an identity.";
    tx(c, xf, 24.0, y + 68.0, 0, note, 11.0, MUTED, None, false);
    let hit = Rect::from_min_max(xf.pt(16.0, y - 12.0), xf.pt(w - 16.0, y + 74.0));
    c.hover(
        hit,
        info("legend")
            .fact("link colours and glyphs", "go-pflow RenderDOT (composedot.go:28-33)")
            .fact("port colour", "the port schema (ROADMAP Phase 6: wire types are port schemas)")
            .fact("two colours", "two union-finds in Flatten: places by token/data links, transitions by event links")
            .cite(Cites::dot())
            .cite(Cites::place_classes())
            .cite(Cites::trans_classes()),
    );
    let _ = hover_rects;
}

// ============================================================ connect-4

impl<'a> Ctx<'a> {
    /// A plan slot's drop target, drawing left to the caller: lit where
    /// `plan.slot_candidates` says the piece may go; the drop runs
    /// `plan.add_piece`. Returns whether it is lit.
    fn slot_zone_info(&mut self, r: Rect, slot: &str, mut i: Info) -> bool {
        let mut lit = false;
        let mut v: Option<Value> = None;
        if let Some(Drag::Piece {
            cands: Some(cn), ..
        }) = self.drag
        {
            if let Some(x) = arr(&cn["slots"]).iter().find(|x| s(&x["slot"]) == slot) {
                lit = x["ok"] == true;
                v = Some(x.clone());
            }
        }
        if let Some(x) = &v {
            i = i.fact(
                if x["ok"] == true { "drop" } else { "refused" },
                s(&x["why"]).to_string() + if x["ok"] == true { "legal" } else { "" },
            );
        }
        self.hover(r, i);
        if self.released && self.pointer_in(r) {
            if let (Some(Drag::Piece { name, .. }), Some(x)) = (self.drag, &v) {
                if x["ok"] == true {
                    let n = name.clone();
                    self.command("plan.add_piece", json!({"piece": n, "slot": slot}));
                }
            }
        }
        lit
    }
}

fn slot_info(id: &str) -> Info {
    let mut i = info(format!("slot: {}", sp::slot(id).map_or(id, |x| x.label)));
    if let Some(sl) = sp::slot(id) {
        i = i.fact("accepts", sl.accepts).fact(
            "field",
            sl.field
                .unwrap_or("(designed in the ROADMAP, not a SearchPlan field)"),
        );
        i.note = if sl.designed {
            "A slot the design has and plan.rs does not: shown, never fillable.".into()
        } else {
            format!("SearchPlan field \"{}\".", sl.field.unwrap_or(""))
        };
    }
    match id {
        "drivers" => i.cites.push(Cites::plan_drivers()),
        "order" => {
            i.cites.push(Cites::chain_default());
            i.cites.push(Cites::chain_ties());
        }
        "filters" => {
            i.cites.push(Cites::plan_filters());
            i.cites.push(Cites::plan_check());
        }
        "heuristics" => i.cites.push(Cites::plan_check()),
        "memo" => i.cites.push(Cites::plan_memo()),
        _ => {}
    }
    i
}

fn piece_info(name: &str) -> Info {
    let Some(p) = sp::piece_info(name) else {
        return info(name);
    };
    let status = match p.status {
        "built" => format!("built (pflow-rs {})", sp::PLAN_RS_REV),
        "reserved" => "reserved: named only to be refused".to_string(),
        _ => {
            "PROPOSED in the ontology ROADMAP (approved 2026-10-27); plan.rs refuses it as unknown"
                .to_string()
        }
    };
    let mut i = info(format!("{} piece {name}", p.kind.to_lowercase()))
        .fact("kind", p.kind)
        .fact("status", status)
        .fact("contract (ROADMAP ct:*)", p.contract);
    if p.kind == "Order" {
        i = i.fact(
            "decisive",
            if p.decisive {
                "yes: a nonzero key settles the rank (top tier)"
            } else {
                "no"
            },
        );
    }
    if !p.reads.is_empty() {
        i = i.fact("reads", p.reads);
    }
    if p.needs_goals {
        i = i.fact("needs", "the net's goals (Connect-4 declares them)");
    }
    i.note = format!(
        "{} ns:SearchPiece and the ct:* contracts are PROPOSED vocabulary (ontology ROADMAP Phase 5); the names, kinds and refusals are plan.rs's.",
        p.doc
    );
    match p.kind {
        "Order" => i.cites.push(rs(
            "order.rs",
            116,
            123,
            "Piece::NAMES: the order pieces plan.rs takes",
        )),
        "Driver" => i
            .cites
            .push(rs("plan.rs", 99, 99, "DRIVERS: the drivers plan.rs takes")),
        "Memo" => i.cites.push(Cites::plan_memo()),
        _ => i.cites.push(Cites::plan_check()),
    }
    i
}

struct ChipSpec {
    slot: &'static str,
    name: String,
    label: String,
    x: f32,
    y: f32,
    w: f32,
    ghost: bool,
    index: Option<usize>,
    tag: Option<String>,
}

const CHIP_H: f32 = 26.0;

fn ordinal(i: usize) -> String {
    format!(
        "{}{}",
        i + 1,
        ["st", "nd", "rd"].get(i).copied().unwrap_or("th")
    )
}

struct LegDef {
    pts: Vec<Pos2>,
    color: Color32,
    dash: Option<(f32, f32)>,
    start_arrow: bool,
    end_arrow: bool,
    end_bar: bool,
    label: [String; 2],
    at: (f32, f32, u8),
    info: Info,
    key: &'static str,
}

fn lerp_pts(pts: &[Pos2], f: f32) -> Vec<Pos2> {
    let f = f.clamp(0.0, 1.0);
    if f >= 1.0 {
        return pts.to_vec();
    }
    let n = pts.len() - 1;
    let x = f * n as f32;
    let i = (x.floor() as usize).min(n - 1);
    let mut out: Vec<Pos2> = pts[..=i].to_vec();
    out.push(lerp2(pts[i], pts[i + 1], x - i as f32));
    out
}

pub(super) fn spiders_c4(c: &mut Ctx, area: Rect) {
    let (plan, order, _, _, _) = plan_lists(c);
    let has = |f: &str| plan.get(f).is_some_and(|v| !v.is_null());
    let memo = plan.get("memo").filter(|m| !m.is_null()).cloned();
    let mode = plan
        .get("mode")
        .and_then(Value::as_str)
        .unwrap_or("exact")
        .to_string();
    let grid = area.shrink2(Vec2::new(0.0, 0.0));
    let design = Rect::from_min_max(grid.left_top(), grid.right_bottom() - Vec2::new(0.0, 50.0));
    const W: f32 = 1150.0;
    const H: f32 = 700.0;
    let xf = Xf::fit(design, W, H, 1.5);
    c.p.rect_filled(xf.rect(0.0, 0.0, W, H), 0.0, PAPER);
    c.p.rect_filled(xf.rect(262.0, 0.0, 528.0, H), 0.0, BAND_T);
    let in_chain = |n: &str| order.iter().any(|x| x == n);
    let has_history = in_chain("history");

    // ---- manifest (unchanged): the junctions and the pieces that touch them
    let mut tt_legs: Vec<String> = vec![];
    if in_chain("tt_best") {
        tt_legs.push("tt_best".into());
    }
    if memo.is_some() {
        tt_legs.push("memo".into());
    }
    let hi_legs: Vec<String> = order.iter().filter(|x| *x == "history").cloned().collect();
    c.scene.emit("junction", "tt", json!({"legs": tt_legs}));
    c.scene
        .emit("junction", "history", json!({"legs": hi_legs}));

    // ---- headings
    for (x, h, t1, t2, col) in [
        (
            28.0,
            0u8,
            "THE FIXED SEARCH",
            "and the slots that wrap it",
            INK,
        ),
        (
            526.0,
            1,
            "SHARED TABLES · junctions, not spiders",
            "hollow dots: state several parts read or write; nothing fuses",
            TABLE,
        ),
        (
            806.0,
            0,
            "PIECES THAT TOUCH THEM",
            "a leg only while the piece is in the plan",
            INK,
        ),
    ] {
        tx(c, xf, x, 22.0, h, t1, 11.0, col, None, false);
        tx(c, xf, x, 36.0, h, t2, 10.5, MUTED, None, false);
    }

    // ---- slots
    let sl = |id: &str| -> Option<R4> {
        Some(match id {
            "drivers" => R4 {
                x: 28.0,
                y: 52.0,
                w: 220.0,
                h: 112.0,
            },
            "memo" => R4 {
                x: 806.0,
                y: 52.0,
                w: 316.0,
                h: 82.0,
            },
            "order" => R4 {
                x: 806.0,
                y: 166.0,
                w: 316.0,
                h: 262.0,
            },
            "filters" => R4 {
                x: 28.0,
                y: 460.0,
                w: 220.0,
                h: 96.0,
            },
            "heuristics" => R4 {
                x: 28.0,
                y: 572.0,
                w: 220.0,
                h: 96.0,
            },
            "bounds" => R4 {
                x: 806.0,
                y: 460.0,
                w: 316.0,
                h: 96.0,
            },
            "engine" => R4 {
                x: 806.0,
                y: 572.0,
                w: 316.0,
                h: 96.0,
            },
            _ => return None,
        })
    };
    let core = R4 {
        x: 28.0,
        y: 196.0,
        w: 220.0,
        h: 232.0,
    };
    let tt = Pos2::new(606.0, 236.0);
    let hi = Pos2::new(606.0, 382.0);
    let order_given = has("order");
    for slot in sp::SLOTS.iter() {
        let Some(r) = sl(slot.id) else { continue };
        let (title, sub) = match slot.id {
            "drivers" => ("drivers", "touch neither table".to_string()),
            "memo" => ("memo", "caps the transposition table".to_string()),
            "order" => (
                "order chain",
                if order_given {
                    "compared top first"
                } else {
                    "not given: the default chain"
                }
                .to_string(),
            ),
            "filters" => ("filters", String::new()),
            "heuristics" => ("heuristics", String::new()),
            "bounds" => ("bounds", "designed, not built".to_string()),
            _ => ("engine", "designed, not built".to_string()),
        };
        let lit = c.slot_zone_info(xf.rect(r.x, r.y, r.w, r.h), slot.id, slot_info(slot.id));
        let (stroke, sw) = if lit {
            (LIT, 3.0)
        } else if slot.designed {
            (hex("#b9bfc8"), 1.3)
        } else {
            (hex("#aab2bd"), 1.3)
        };
        if slot.designed && !lit {
            dashed_rect(c, xf, r, hex("#f4f5f7"), stroke, sw, (5.0, 4.0));
        } else {
            frect(
                c,
                xf,
                r,
                8.0,
                if slot.designed {
                    hex("#f4f5f7")
                } else {
                    Color32::WHITE
                },
                stroke,
                sw,
            );
        }
        tx(
            c,
            xf,
            r.x + 12.0,
            r.y + 19.0,
            0,
            title,
            12.0,
            if slot.designed { MUTED } else { INK },
            None,
            false,
        );
        if !sub.is_empty() {
            tx(
                c,
                xf,
                r.x + r.w - 12.0,
                r.y + 19.0,
                2,
                &sub,
                10.5,
                MUTED,
                None,
                false,
            );
        }
    }

    // ---- chips (positions first: the legs are placed around them)
    let mut chips: Vec<ChipSpec> = vec![];
    let chip_w = |label: &str, idx: bool| mono_w(label, 11.5) + if idx { 40.0 } else { 22.0 };
    let driver_label = |d: &Value| -> (String, String) {
        let name = driver_name(d);
        let label = match d {
            Value::Object(o) if name == "budget" => format!(
                "budget {{nodes: {}}}",
                o.get("budget")
                    .and_then(|b| b.get("nodes"))
                    .map_or(String::new(), |n| n.to_string())
            ),
            _ => name.clone(),
        };
        (name, label)
    };
    let drv_r = sl("drivers").unwrap();
    let drivers_raw: Vec<Value> = plan
        .get("drivers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let null_window = !plan.get("drivers").is_some_and(|d| !d.is_null())
        || drivers_raw.iter().any(|d| driver_name(d) == "null_window");
    if !has("drivers") {
        chips.push(ChipSpec {
            slot: "drivers",
            name: "null_window".into(),
            label: "null_window".into(),
            x: drv_r.x + 12.0,
            y: drv_r.y + 46.0,
            w: chip_w("null_window", false),
            ghost: true,
            index: None,
            tag: None,
        });
    } else {
        for (i, d) in drivers_raw.iter().enumerate() {
            let (name, label) = driver_label(d);
            chips.push(ChipSpec {
                slot: "drivers",
                w: chip_w(&label, true),
                name,
                label,
                x: drv_r.x + 12.0,
                y: drv_r.y + 46.0 + i as f32 * 32.0,
                ghost: false,
                index: Some(i),
                tag: None,
            });
        }
    }
    let memo_r = sl("memo").unwrap();
    if let Some(m) = &memo {
        let label = format!(
            "memo {{entries: {}}}",
            m.get("entries").map_or(String::new(), |n| n.to_string())
        );
        chips.push(ChipSpec {
            slot: "memo",
            name: "memo".into(),
            w: chip_w(&label, true),
            label,
            x: memo_r.x + 12.0,
            y: memo_r.y + 50.0,
            ghost: false,
            index: Some(0),
            tag: None,
        });
    }
    let ord_r = sl("order").unwrap();
    for (i, name) in order.iter().enumerate() {
        let p = sp::piece_info(name);
        chips.push(ChipSpec {
            slot: "order",
            name: name.clone(),
            label: name.clone(),
            x: ord_r.x + 16.0,
            y: ord_r.y + 50.0 + i as f32 * 44.0,
            w: chip_w(name, order_given),
            ghost: !order_given,
            index: if order_given { Some(i) } else { None },
            tag: Some(format!(
                "{} · {}{}",
                ordinal(i),
                if p.is_some_and(|x| x.decisive) {
                    "decisive · "
                } else {
                    ""
                },
                p.map_or("", |x| x.contract)
            )),
        });
    }
    let fl_r = sl("filters").unwrap();
    let filters_given = has("filters");
    let filter_names: Vec<String> = plan
        .get("filters")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(driver_name).collect())
        .unwrap_or_default();
    if !filters_given {
        for (i, n) in sp::field_or_default(&plan, "filters").iter().enumerate() {
            let n = s(n).to_string();
            chips.push(ChipSpec {
                slot: "filters",
                w: chip_w(&n, false),
                label: n.clone(),
                name: n,
                x: fl_r.x + 12.0,
                y: fl_r.y + 40.0 + i as f32 * 30.0,
                ghost: true,
                index: None,
                tag: None,
            });
        }
    } else {
        for (i, n) in filter_names.iter().enumerate() {
            chips.push(ChipSpec {
                slot: "filters",
                w: chip_w(n, true),
                label: n.clone(),
                name: n.clone(),
                x: fl_r.x + 12.0,
                y: fl_r.y + 40.0 + i as f32 * 30.0,
                ghost: false,
                index: Some(i),
                tag: None,
            });
        }
    }
    let he_r = sl("heuristics").unwrap();
    let heur: Vec<String> = plan
        .get("heuristics")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(driver_name).collect())
        .unwrap_or_default();
    for (i, n) in heur.iter().enumerate() {
        chips.push(ChipSpec {
            slot: "heuristics",
            w: chip_w(n, true),
            label: n.clone(),
            name: n.clone(),
            x: he_r.x + 12.0,
            y: he_r.y + 40.0 + i as f32 * 30.0,
            ghost: false,
            index: Some(i),
            tag: None,
        });
    }

    // ---- legs: core ↔ tables
    let cx_r = core.x + core.w;
    let ttp = |dx: f32, dy: f32| Pos2::new(tt.x + dx, tt.y + dy);
    let hip = |dx: f32, dy: f32| Pos2::new(hi.x + dx, hi.y + dy);
    let mut legs: Vec<LegDef> = vec![];
    legs.push(LegDef {
        pts: curve_x(Pos2::new(cx_r, core.y + 30.0), ttp(-11.0, -3.0)),
        color: TABLE,
        dash: None,
        start_arrow: true,
        end_arrow: false,
        end_bar: false,
        label: ["reads lo, hi, best".into(), "game.rs:1069-1079".into()],
        at: (cx_r + 14.0, core.y + 6.0, 0),
        info: info("the core reads the table")
            .fact(
                "reads",
                "lo and hi narrow the window or settle the node; best becomes Ctx.first",
            )
            .cite(Cites::tt_read())
            .cite(Cites::entry()),
        key: "core-tt-r",
    });
    legs.push(LegDef {
        pts: curve_x(Pos2::new(cx_r, core.y + 76.0), ttp(-11.0, 4.0)),
        color: TABLE,
        dash: None,
        start_arrow: false,
        end_arrow: true,
        end_bar: false,
        label: ["writes it through store".into(), "game.rs:1082-1085, 1168-1185".into()],
        at: (cx_r + 14.0, core.y + 100.0, 0),
        info: info("the core writes the table")
            .fact("writes", "store: a final position's score, then each search's bounds merged with what the table knew")
            .fact("space", "the table is bounded: past its entries a new position takes the place of a weaker one (memo.rs:15-24)")
            .cite(Cites::store())
            .cite(Cites::store_leaf())
            .cite(Cites::store_after())
            .cite(Cites::table_policy()),
        key: "core-tt-w",
    });
    legs.push(LegDef {
        pts: curve_x(Pos2::new(cx_r, core.y + 166.0), hip(-11.0, -3.0)),
        color: TABLE,
        dash: None,
        start_arrow: true,
        end_arrow: false,
        end_bar: false,
        label: ["lends &history in Ctx".into(), "game.rs:1090-1095".into()],
        at: (cx_r + 14.0, core.y + 140.0, 0),
        info: info("the core lends the history table to the pieces")
            .fact(
                "how",
                "Ctx { first, history: &self.history } is built per node",
            )
            .cite(Cites::ctx_build())
            .cite(Cites::ctx_doc()),
        key: "core-hi-r",
    });
    legs.push(LegDef {
        pts: curve_x(Pos2::new(cx_r, core.y + 210.0), hip(-11.0, 4.0)),
        color: TABLE,
        dash: if has_history { None } else { Some((4.0, 4.0)) },
        start_arrow: false,
        end_arrow: true,
        end_bar: false,
        label: [
            "reports a cutoff: chain.cutoff".into(),
            "game.rs:1164 → order.rs:277-281".into(),
        ],
        at: (cx_r + 14.0, core.y + 232.0, 0),
        info: info("the core reports a cutoff to the chain")
            .fact(
                "how",
                "chain.cutoff(&mut self.history, t) calls every piece's cutoff",
            )
            .fact(
                "who writes",
                if has_history {
                    "History::cutoff (history[t] += 1)"
                } else {
                    "nobody in this chain: Orderer::cutoff is a no-op by default"
                },
            )
            .cite(Cites::cutoff())
            .cite(Cites::chain_cutoff())
            .cite(Cites::hist_cutoff())
            .cite(Cites::cutoff_default()),
        key: "core-hi-w",
    });
    // piece legs: each label goes where it covers no other label, dot or leg
    let mut pobs = Obs::new(R4 {
        x: 262.0,
        y: 44.0,
        w: 540.0,
        h: H - 60.0,
    });
    for j in [tt, hi] {
        pobs.rects.push(R4 {
            x: j.x - 150.0,
            y: j.y - 48.0,
            w: 172.0,
            h: 64.0,
        });
    }
    for l in &legs {
        pobs.pts.extend(l.pts.iter().cloned());
        pobs.rects.push(R4 {
            x: l.at.0,
            y: l.at.1 - 11.0,
            w: 215.0,
            h: 28.0,
        });
    }
    let chip_at = |slot: &str, name: &str| chips.iter().find(|x| x.slot == slot && x.name == name);
    struct Pending {
        chip: (f32, f32, &'static str),
        j: Pos2,
        pts: Vec<Pos2>,
        label: [String; 2],
        color: Color32,
        dash: Option<(f32, f32)>,
        end_arrow: bool,
        start_arrow: bool,
        end_bar: bool,
        info: Info,
        key: &'static str,
    }
    let mut pending: Vec<Pending> = vec![];
    if let Some(ch) = chip_at("order", "tt_best") {
        pending.push(Pending {
            chip: (ch.x, ch.y, "order"),
            j: tt,
            pts: curve_x(Pos2::new(ch.x, ch.y), ttp(11.0, 2.0)),
            label: [
                "tt_best reads Ctx.first = e.best".into(),
                "game.rs:1078 · order.rs:52-58".into(),
            ],
            color: TABLE,
            dash: None,
            end_arrow: true,
            start_arrow: false,
            end_bar: false,
            info: info("tt_best reads the remembered best move")
                .fact("key", "the move equal to Ctx.first ranks first (decisive)")
                .fact(
                    "set from",
                    "first = e.best when the table holds the position",
                )
                .cite(Cites::tt_best_key())
                .cite(Cites::tt_read())
                .cite(Cites::ctx_doc()),
            key: "ttbest",
        });
    }
    if let Some(ch) = chip_at("order", "history") {
        pending.push(Pending {
            chip: (ch.x, ch.y, "order"),
            j: hi,
            pts: curve_x(Pos2::new(ch.x, ch.y), hip(11.0, 0.0)),
            label: [
                "History reads history[t]; its cutoff adds 1".into(),
                "order.rs:68-71, 73-75".into(),
            ],
            color: TABLE,
            dash: None,
            end_arrow: true,
            start_arrow: true,
            end_bar: false,
            info: info("History reads and writes the history table")
                .fact("reads", "key = cx.history[t]")
                .fact(
                    "writes",
                    "cutoff: history[t] += 1, the only piece that writes it",
                )
                .cite(Cites::hist_key())
                .cite(Cites::hist_cutoff())
                .cite(Cites::chain_cutoff()),
            key: "history",
        });
    }
    if let Some(ch) = chip_at("memo", "memo") {
        pending.push(Pending {
            chip: (ch.x, ch.y - 16.0, "memo"),
            j: tt,
            pts: curve_y(Pos2::new(ch.x + 20.0, ch.y + 13.0), ttp(4.0, -12.0)),
            label: ["memo caps it: with_memo".into(), "game.rs:633-642 · plan.rs:425-427".into()],
            color: proto_piece_color("Memo"),
            dash: Some((5.0, 3.0)),
            end_arrow: false,
            start_arrow: false,
            end_bar: true,
            info: info("memo caps the transposition table")
                .fact("how", "with_memo keeps the table to at most N entries, inside a search and across calls; past that a new position takes the place of a weaker one")
                .fact("contract", "value-preserving: forgetting never changes a value, only what it costs")
                .cite(Cites::with_memo())
                .cite(Cites::plan_memo())
                .cite(Cites::store())
                .cite(Cites::table_policy()),
            key: "memo",
        });
    }
    for p in &pending {
        pobs.pts.extend(p.pts.iter().cloned());
    }
    for p in pending {
        let w = sans_w(&p.label[0], 11.0).max(mono_w(&p.label[1], 10.0)) + 6.0;
        let h = 28.0;
        let xr = if p.chip.2 == "memo" {
            memo_r.x
        } else {
            ord_r.x
        } - 10.0;
        let mut cands: Vec<(R4, u8)> = vec![
            (
                R4 {
                    x: xr - w,
                    y: p.chip.1 + 12.0,
                    w,
                    h,
                },
                2,
            ),
            (
                R4 {
                    x: xr - w,
                    y: p.chip.1 - 12.0 - h,
                    w,
                    h,
                },
                2,
            ),
            (
                R4 {
                    x: p.j.x + 18.0,
                    y: p.j.y + 14.0,
                    w,
                    h,
                },
                0,
            ),
            (
                R4 {
                    x: p.j.x + 18.0,
                    y: p.j.y - 14.0 - h,
                    w,
                    h,
                },
                0,
            ),
        ];
        for k in 1..=8 {
            let k = k as f32;
            cands.push((
                R4 {
                    x: xr - w,
                    y: p.chip.1 + 12.0 + k * 22.0,
                    w,
                    h,
                },
                2,
            ));
            cands.push((
                R4 {
                    x: xr - w,
                    y: p.chip.1 - 12.0 - h - k * 22.0,
                    w,
                    h,
                },
                2,
            ));
            cands.push((
                R4 {
                    x: p.j.x + 18.0,
                    y: p.j.y + 14.0 + k * 22.0,
                    w,
                    h,
                },
                0,
            ));
        }
        let rects: Vec<R4> = cands.iter().map(|x| x.0).collect();
        let bi = pobs.best(&rects).unwrap_or(0);
        let (r, al) = cands[bi];
        pobs.rects.push(r);
        legs.push(LegDef {
            pts: p.pts,
            color: p.color,
            dash: p.dash,
            start_arrow: p.start_arrow,
            end_arrow: p.end_arrow,
            end_bar: p.end_bar,
            label: p.label,
            at: (if al == 2 { r.x + r.w } else { r.x }, r.y + 11.0, al),
            info: p.info,
            key: p.key,
        });
    }

    // ---- merge animation (a piece was just added)
    let anim = c.anim(|e| match e {
        Event::PieceAdded { name, slot } => Some((name.clone(), slot.clone())),
        _ => None,
    });
    let anim_chip: Option<(usize, f32)> = anim.as_ref().and_then(|((name, slot), p)| {
        chips
            .iter()
            .position(|x| x.slot == slot.as_str() && x.name == *name && !x.ghost)
            .or_else(|| chips.iter().rposition(|x| x.slot == slot.as_str()))
            .map(|i| (i, *p))
    });
    let anim_leg: Option<(&str, f32)> = anim.as_ref().and_then(|((name, _), p)| {
        let k = match name.as_str() {
            "tt_best" => "ttbest",
            "history" => "history",
            "memo" => "memo",
            _ => return None,
        };
        Some((k, *p))
    });

    // ---- legs and their labels
    let lab_halo = Some(BAND_T);
    for l in &legs {
        let f = match anim_leg {
            Some((k, p)) if k == l.key => ((p - 0.35) / 0.65).clamp(0.0, 1.0),
            _ => 1.0,
        };
        let shown = lerp_pts(&l.pts, ease(f));
        stroke_pts(c, xf, &shown, l.color, 2.0, l.dash);
        if f >= 1.0 {
            let n = l.pts.len();
            let (a0, a1) = (
                xf.pt(l.pts[0].x, l.pts[0].y),
                xf.pt(l.pts[n - 1].x, l.pts[n - 1].y),
            );
            if l.start_arrow {
                let q = xf.pt(l.pts[2].x, l.pts[2].y);
                c.arrow_head(q, a0, 11.0 * xf.k, l.color);
            }
            if l.end_arrow {
                let q = xf.pt(l.pts[n - 3].x, l.pts[n - 3].y);
                c.arrow_head(q, a1, 11.0 * xf.k, l.color);
            }
            if l.end_bar {
                let q = xf.pt(l.pts[n - 3].x, l.pts[n - 3].y);
                let d = (a1 - q).normalized();
                let nrm = Vec2::new(-d.y, d.x) * 8.0 * xf.k;
                c.poly(&[a1 - nrm, a1 + nrm], l.color, 4.0 * xf.k);
            }
        }
        let la = if anim_leg.is_some_and(|(k, _)| k == l.key) {
            f
        } else {
            1.0
        };
        if la > 0.01 {
            let (lx, ly, al) = l.at;
            tx(
                c,
                xf,
                lx,
                ly,
                al,
                &l.label[0],
                11.0,
                INK.gamma_multiply(la),
                lab_halo,
                false,
            );
            tx(
                c,
                xf,
                lx,
                ly + 13.0,
                al,
                &l.label[1],
                10.0,
                MUTED.gamma_multiply(la),
                lab_halo,
                true,
            );
        }
        c.hover_poly(&scr(xf, &l.pts), LEG_TOL, l.info.clone());
    }

    // ---- core
    let core_rect = xf.rect(core.x, core.y, core.w, core.h);
    frect(c, xf, core, 8.0, hex("#eef1f5"), hex("#8d96a3"), 1.4);
    tx(
        c,
        xf,
        core.x + 12.0,
        core.y + 22.0,
        0,
        "core",
        13.0,
        INK,
        None,
        false,
    );
    tx(
        c,
        xf,
        core.x + 50.0,
        core.y + 22.0,
        0,
        "Solver::search",
        11.5,
        INK,
        None,
        true,
    );
    wrap_text(
        c,
        xf,
        core.x + 12.0,
        core.y + 44.0,
        "Fail-soft alpha-beta (game.rs:1050-1179). Not a piece: the search is fixed and a plan chooses what swaps in (plan.rs:3-4).",
        core.w - 24.0,
        11.0,
        MUTED,
        14.0,
    );
    let (pl, tr, _) = net_counts(c);
    wrap_text(
        c,
        xf,
        core.x + 12.0,
        core.y + core.h - 30.0,
        &format!("net: connect-4, {pl} places, {tr} transitions"),
        core.w - 24.0,
        10.5,
        MUTED,
        14.0,
    );
    c.hover(
        core_rect,
        info("the core search (fixed, not a piece)")
            .fact("what", "fail-soft alpha-beta in the +1 side's view")
            .fact(
                "tables",
                "reads and writes the transposition table; lends the history table to the chain",
            )
            .fact(
                "in the plan",
                "no field: plan.rs fixes the search and a plan picks what swaps in",
            )
            .cite(Cites::search())
            .cite(Cites::plan_fixed())
            .cite(Cites::tt_read())
            .cite(Cites::store())
            .cite(Cites::ctx_build())
            .cite(Cites::cutoff()),
    );

    // ---- junctions (shared state, not spiders)
    let ttq = |m: &Option<Value>| {
        m.as_ref()
            .map(|x| x.get("entries").map_or(String::new(), |n| n.to_string()))
    };
    let tt_info = info("junction: the transposition table (shared state, not a spider)")
        .fact("entries", "Entry { lo, hi, best } per position key (memo.rs:53-60)")
        .fact(
            "read by",
            format!(
                "the core (bounds and best move){}",
                if in_chain("tt_best") { "; tt_best, through Ctx.first" } else { "" }
            ),
        )
        .fact("written by", "the core, through store")
        .fact(
            "capped by",
            match ttq(&memo) {
                Some(n) => format!("memo: {n} entries (with_memo)"),
                None => "Solver::MEMO = 2^23 entries by default; past that a new position takes the place of a weaker one (memo.rs:15-24)".into(),
            },
        )
        .fact(
            "shared",
            if null_window {
                "the yes/no searches null_window runs share it, and it persists across calls"
            } else {
                "null_window is off, so one search over the whole range uses it; it persists across calls"
            },
        )
        .note("A junction, not a ZX spider: shared state that several parts read or write. Nothing fuses and no spider law applies.")
        .cite(Cites::entry())
        .cite(Cites::fields())
        .cite(Cites::tt_shared())
        .cite(Cites::tt_read())
        .cite(Cites::store())
        .cite(Cites::with_memo())
        .cite(Cites::memo_default())
        .cite(Cites::table_policy());
    let tt_info = if null_window {
        tt_info
    } else {
        tt_info.cite(Cites::solve_one()).cite(Cites::plan_drivers())
    };
    let hi_info = info("junction: the history table (shared state, not a spider)")
        .fact("entries", "one u32 counter per transition (game.rs:619)")
        .fact(
            "read by",
            if has_history { "History, through Ctx.history" } else { "no piece in this chain" },
        )
        .fact(
            "written by",
            if has_history {
                "History::cutoff only (history[t] += 1)"
            } else {
                "nothing: every other piece's cutoff is a no-op"
            },
        )
        .fact("lent by", "the core: &history in Ctx, &mut history to chain.cutoff on a cutoff")
        .note("A junction, not a ZX spider. Without History in the chain the table is still allocated and lent, but nothing reads or writes it.")
        .cite(Cites::fields())
        .cite(Cites::hist_init())
        .cite(Cites::ctx_build())
        .cite(Cites::cutoff())
        .cite(Cites::chain_cutoff())
        .cite(Cites::hist_key())
        .cite(Cites::hist_cutoff())
        .cite(Cites::cutoff_default())
        .cite(Cites::ctx_doc());
    for (pos, title, sub, i) in [
        (
            tt,
            "transposition table",
            "Solver.tt · game.rs:566",
            tt_info,
        ),
        (hi, "history table", "Solver.history · game.rs:567", hi_info),
    ] {
        fcirc(c, xf, pos, 9.5, PAPER, TABLE, 3.0);
        tx(
            c,
            xf,
            pos.x - 14.0,
            pos.y - 34.0,
            2,
            title,
            12.5,
            TABLE,
            Some(BAND_T),
            false,
        );
        tx(
            c,
            xf,
            pos.x - 14.0,
            pos.y - 20.0,
            2,
            sub,
            10.5,
            MUTED,
            Some(BAND_T),
            true,
        );
        c.hover(
            Rect::from_center_size(xf.pt(pos.x, pos.y), Vec2::splat(36.0 * xf.k)),
            i,
        );
    }
    if !has_history {
        tx(
            c,
            xf,
            hi.x,
            hi.y + 30.0,
            1,
            "nothing in this chain reads",
            10.5,
            MUTED,
            None,
            false,
        );
        tx(
            c,
            xf,
            hi.x,
            hi.y + 43.0,
            1,
            "or writes it (order.rs:36-37)",
            10.5,
            MUTED,
            None,
            false,
        );
    }

    // ---- slot texts
    let why = if mode == "exact" {
        "exact mode refuses every one: it can change the value (plan.rs:385-392)"
    } else {
        "play mode: none is built in this release (plan.rs:385-392)"
    };
    if has("drivers") && drivers_raw.is_empty() {
        wrap_text(
            c,
            xf,
            drv_r.x + 12.0,
            drv_r.y + 46.0,
            "none, so null_window is off: one search over the whole reward range (plan.rs:29-30, game.rs:943-959)",
            drv_r.w - 24.0,
            11.0,
            MUTED,
            14.0,
        );
    } else if !has("drivers") {
        tx(
            c,
            xf,
            drv_r.x + 12.0,
            drv_r.y + 76.0,
            0,
            "default: on (plan.rs:29-30)",
            10.5,
            MUTED,
            None,
            false,
        );
    }
    if has("drivers") && drivers_raw.len() == 1 {
        wrap_text(
            c,
            xf,
            drv_r.x + 12.0,
            drv_r.y + 78.0,
            if null_window {
                "its yes/no searches share the core's table (game.rs:25-29)"
            } else {
                "null_window off: one search over the whole range (plan.rs:29-30; game.rs:943-959)"
            },
            drv_r.w - 24.0,
            10.5,
            MUTED,
            13.0,
        );
    }
    if memo.is_none() {
        wrap_text(
            c,
            xf,
            memo_r.x + 12.0,
            memo_r.y + 42.0,
            "none: the table keeps at most Solver::MEMO = 2^23 entries and forgets the weakest past that; a value never changes, only what it costs (game.rs:588-594, memo.rs:15-24)",
            memo_r.w - 24.0,
            10.5,
            MUTED,
            14.0,
        );
    }
    if order_given && order.is_empty() {
        wrap_text(
            c,
            xf,
            ord_r.x + 12.0,
            ord_r.y + 46.0,
            "empty: ties keep the net's own move order (order.rs:197-199)",
            ord_r.w - 24.0,
            11.0,
            MUTED,
            14.0,
        );
    }
    if !order_given {
        tx(
            c,
            xf,
            ord_r.x + 12.0,
            ord_r.y + ord_r.h - 12.0,
            0,
            "Chain::default (order.rs:205-216)",
            10.5,
            MUTED,
            None,
            false,
        );
    }
    if !filters_given {
        tx(
            c,
            xf,
            fl_r.x + 12.0,
            fl_r.y + fl_r.h - 12.0,
            0,
            "default: Filter::DEFAULT (plan.rs:41-43)",
            10.5,
            MUTED,
            None,
            false,
        );
    } else if filter_names.is_empty() {
        wrap_text(
            c,
            xf,
            fl_r.x + 12.0,
            fl_r.y + 42.0,
            "[]: no filter runs (plan.rs:41-43)",
            fl_r.w - 24.0,
            10.5,
            MUTED,
            14.0,
        );
    }
    if heur.is_empty() {
        wrap_text(
            c,
            xf,
            he_r.x + 12.0,
            he_r.y + 42.0,
            &format!("empty. {why}"),
            he_r.w - 24.0,
            10.5,
            MUTED,
            14.0,
        );
    }
    let bo = sl("bounds").unwrap();
    let en = sl("engine").unwrap();
    wrap_text(
        c,
        xf,
        bo.x + 12.0,
        bo.y + 42.0,
        &format!(
            "In the ROADMAP's design; not a SearchPlan field at {}, so nothing can be dropped here.",
            sp::PLAN_RS_REV
        ),
        bo.w - 24.0,
        10.5,
        MUTED,
        14.0,
    );
    wrap_text(
        c,
        xf,
        en.x + 12.0,
        en.y + 42.0,
        "game.rs picks BitModel or IndexedModel itself; no plan field chooses it.",
        en.w - 24.0,
        10.5,
        MUTED,
        14.0,
    );

    // ---- chips
    for (ci, ch) in chips.iter().enumerate() {
        let kind = sp::piece_info(&ch.name).map_or("", |p| p.kind);
        let col = proto_piece_color(kind);
        let (mut dy, mut alpha) = (0.0, 1.0);
        if let Some((i, p)) = anim_chip {
            if i == ci {
                if p < 0.6 {
                    let t = ease(p / 0.6);
                    dy = -46.0 + 50.0 * t;
                    alpha = t;
                } else {
                    dy = 4.0 * (1.0 - (p - 0.6) / 0.4);
                }
            }
        }
        let r = R4 {
            x: ch.x,
            y: ch.y - CHIP_H / 2.0 + dy,
            w: ch.w,
            h: CHIP_H,
        };
        let fill = if ch.ghost {
            hex("#fbfbfc")
        } else {
            Color32::WHITE
        };
        if ch.ghost {
            dashed_rect(c, xf, r, fill, col.gamma_multiply(alpha), 1.6, (4.0, 3.0));
        } else {
            frect(
                c,
                xf,
                r,
                5.0,
                fill.gamma_multiply(alpha),
                col.gamma_multiply(alpha),
                1.6,
            );
        }
        tx(
            c,
            xf,
            ch.x + 10.0,
            ch.y + dy + 4.2,
            0,
            &ch.label,
            11.5,
            col.gamma_multiply(alpha),
            None,
            true,
        );
        let face = xf.rect(r.x, r.y, r.w, r.h);
        c.hover(face, piece_info(&ch.name));
        if let Some(idx) = ch.index {
            let xr = xf.rect(ch.x + ch.w - 22.0, r.y + 3.0, 18.0, CHIP_H - 6.0);
            tx(
                c,
                xf,
                ch.x + ch.w - 13.0,
                ch.y + dy + 5.0,
                1,
                "×",
                13.0,
                LEAK.gamma_multiply(alpha),
                None,
                false,
            );
            c.hover(
                xr,
                info(format!("remove {} from {}", ch.name, ch.slot))
                    .fact("click", "plan.remove_piece"),
            );
            if c.is_clicked_in(xr) {
                c.command("plan.remove_piece", json!({"slot": ch.slot, "index": idx}));
            }
        }
        if let Some(t) = &ch.tag {
            tx(
                c,
                xf,
                ch.x + ch.w + 10.0,
                ch.y + 4.0,
                0,
                t,
                10.5,
                MUTED,
                None,
                false,
            );
        }
    }

    plan_warn(c);
    palette(c, area);
    c.note("na", "A search plan has no event links, so there are no transition classes and nothing fuses: no spider is drawn here. The nearest honest picture is the plan's two pieces of shared state, drawn as junctions (hollow dots, not ZX spiders) with a leg to each part of the plan that game.rs and order.rs show reading, writing or capping them. A piece's leg is drawn only when that piece is in the plan.");
    c.note("honesty", "The junctions are shared state, not spiders. A plan has no event links, so no spider law applies to it at all.");
}

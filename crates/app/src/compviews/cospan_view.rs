//! The cospan view (see `docs/composition-views-scene.md`): each instance
//! drawn as an open Petri net X → S ← Y with its real places, transitions
//! and arcs, legs from boundary feet to places, observe taps on top, event
//! ports on the bottom edge, the pushout as a glued place between two
//! instances, event and guard links as ghosts. After the prototype's
//! `js/views/cospan.js`, in its design coordinates.

use super::a_draw::*;
use super::*;

// --- geometry (the prototype's constants) ------------------------------------

const W0: f32 = 1150.0;
const SIDE: f32 = 40.0;
const COLX: f32 = 18.0;
const GAP: f32 = 96.0;
const SEAM: f32 = 140.0;
const FH: f32 = 400.0;
const BAND: f32 = 44.0;
const ROW_HEAD: f32 = 40.0;
const LANE: f32 = 130.0;
const HEAD: f32 = 52.0;
const FOOT: f32 = 154.0;
const MIN_SAGA: f32 = 480.0;
const FS: f32 = 8.0;

fn pref_w(pattern: &str) -> f32 {
    match pattern {
        "queue" | "resource-pool" => 180.0,
        "saga" => 516.0,
        _ => 200.0,
    }
}

fn frame_fill(pattern: &str) -> Color32 {
    match pattern {
        "queue" => hex("#f1f6fc"),
        "resource-pool" => hex("#fcf6ec"),
        "saga" => hex("#f6f2fc"),
        _ => hex("#f4f5f7"),
    }
}

/// The prototype's side(s) of a port: X (left foot), Y (right foot), top
/// (observe tap) or bottom (transition port).
fn port_sides(port: &Value) -> Vec<&'static str> {
    if s(&port["target"]) == "transition" {
        return vec!["bottom"];
    }
    match s(&port["kind"]) {
        "in" => vec!["X"],
        "out" => vec!["Y"],
        "inout" => vec!["X", "Y"],
        "observe" => vec!["top"],
        _ => vec!["X"],
    }
}

// --- the prototype's cites ------------------------------------------------------

fn road(from: u64, to: u64, note: &str) -> String {
    cite_of("ontologyRoadmap", "ROADMAP.md", from, to, note)
}
fn go(path: &str, from: u64, to: u64, note: &str) -> String {
    cite_of("goPflow", path, from, to, note)
}
fn onto(path: &str, from: u64, to: u64, note: &str) -> String {
    cite_of("ontology", path, from, to, note)
}

struct Cites;
impl Cites {
    fn cospan_row() -> String {
        road(399, 399, "Open Petri nets as cospans: composition is a pushout along boundary places; the exact semantics of token and data links")
    }
    fn boundaries_row() -> String {
        road(400, 400, "Nets with boundaries: the semantics of event links, which a cospan of places does not express")
    }
    fn model() -> String {
        road(405, 416, "Model composition: ⊗ when unlinked, a pushout for token/data, transition fusion for event, guard outside the free SMC; observe ports are side taps, inout ports are drawn on both sides")
    }
    fn tensor() -> String {
        road(
            407,
            407,
            "Unlinked subnets: the monoidal product ⊗ (disjoint union)",
        )
    }
    fn pushout() -> String {
        road(408, 409, "lk:token / lk:data: a pushout of open nets along a shared place; data links add the observer-does-not-consume restriction")
    }
    fn event() -> String {
        road(
            410,
            411,
            "lk:event: transition fusion; its semantics is Bruni et al.'s, not Baez–Master's",
        )
    }
    fn guard() -> String {
        road(412, 413, "lk:guard: a cross-subnet read or inhibitor arc, outside the free-SMC fragment; drawn, flagged, no functorial semantics")
    }
    fn sides() -> String {
        road(415, 416, "port direction gives domain/codomain; observe ports are side taps; inout ports are drawn on both sides")
    }
    fn place_classes() -> String {
        go(
            "metamodel/compose_flatten.go",
            55,
            70,
            "Flatten: token and data links union the two places into one class",
        )
    }
    fn wire_name() -> String {
        go(
            "metamodel/compose_flatten.go",
            201,
            209,
            "flatPlaceName: a fused class is named \"wire:\" + its canonical member",
        )
    }
    fn smallest() -> String {
        go(
            "metamodel/unionfind.go",
            5,
            6,
            "the canonical member of a class is its lexicographically smallest",
        )
    }
    fn initial_sum() -> String {
        go("metamodel/compose_flatten.go", 291, 292, "mergePlaces: the fused place's initial marking is the sum; its capacity is the tighter one")
    }
    fn direction() -> String {
        go("metamodel/compose.go", 676, 683, "the only port-kind checks on a token link: from is not an in-port, to is not an out-port (observe passes both)")
    }
    fn schema() -> String {
        go(
            "metamodel/compose.go",
            667,
            673,
            "port schema tags must agree when both are set (E_SCHEMA_MISMATCH)",
        )
    }
    fn cq13() -> String {
        onto("cq/13-linkable-ports.rq", 7, 7, "expect event candidates and no token candidates: the three patterns' place ports carry different schemas")
    }
    fn depth_port() -> String {
        onto(
            "ns/patterns.ttl",
            220,
            221,
            "queue-port-depth: place items, for a data or guard link (observe the backlog)",
        )
    }
    fn arcs() -> String {
        onto(
            "ns/core.ttl",
            27,
            37,
            "ns:InputArc (place → transition) and ns:OutputArc (transition → place)",
        )
    }
    fn laws() -> String {
        road(420, 427, "orderers: a partial monoid under chaining; drivers: endomorphisms composed around the core")
    }
    // pflow-rs, re-read at aacb7eb (the prototype's lines are c396d46's)
    fn solver() -> String {
        rs_cite("plan.rs", 396, 402, "SearchPlan::solver(&self, g: &Game): the plan builds a solver over a game; it is not glued to the net")
    }
    fn game() -> String {
        rs_cite("game.rs", 53, 56, "Game: a two-sided game on a net")
    }
    fn auto_win() -> String {
        format!(
            "pflow-rs crates/pflow-metamodel/tests/common/mod.rs:28 @{} (the test game declares every transition whose name contains \"win\" automatic)",
            sp::PLAN_RS_REV
        )
    }
    fn fixture() -> String {
        format!(
            "pflow-rs crates/pflow-metamodel/tests/fixtures/connect-4.json @{} (the Connect-4 net; examples/connect-4.json is byte-identical)",
            sp::PLAN_RS_REV
        )
    }
}

// --- small geometry ---------------------------------------------------------------

/// Liang–Barsky: does segment a-b pass through `r`?
fn seg_rect(a: Pos2, b: Pos2, r: Rect) -> bool {
    let (mut t0, mut t1) = (0.0f32, 1.0f32);
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let p = [-dx, dx, -dy, dy];
    let q = [a.x - r.min.x, r.max.x - a.x, a.y - r.min.y, r.max.y - a.y];
    for i in 0..4 {
        if p[i] == 0.0 {
            if q[i] < 0.0 {
                return false;
            }
            continue;
        }
        let t = q[i] / p[i];
        if p[i] < 0.0 {
            if t > t1 {
                return false;
            }
            t0 = t0.max(t);
        } else {
            if t < t0 {
                return false;
            }
            t1 = t1.min(t);
        }
    }
    true
}

fn text_rect(x: f32, y: f32, t: &str, size: f32, anchor: &str) -> Rect {
    let w = t.chars().count() as f32 * size * 0.63;
    let x0 = match anchor {
        "middle" => x - w / 2.0,
        "end" => x - w,
        _ => x,
    };
    Rect::from_min_size(
        Pos2::new(x0 - 1.5, y - size * 0.82),
        Vec2::new(w + 3.0, size + 1.5),
    )
}

fn clip(from: Pos2, to: Pos2, r_from: f32, r_to: f32, ft: bool, tt: bool) -> (Pos2, Pos2) {
    let d = to - from;
    let len = d.length().max(1e-6);
    let (ux, uy) = (d.x / len, d.y / len);
    let off = |trans: bool, r: f32| {
        if trans {
            r / ux.abs().max(uy.abs()).max(1e-6)
        } else {
            r
        }
    };
    let (a, b) = (off(ft, r_from), off(tt, r_to));
    (
        Pos2::new(from.x + ux * a, from.y + uy * a),
        Pos2::new(to.x - ux * b, to.y - uy * b),
    )
}

const PLACE_CANDS: [(f32, f32, &str); 8] = [
    (0.0, 18.0, "middle"),
    (0.0, -12.0, "middle"),
    (12.0, 3.0, "start"),
    (-12.0, 3.0, "end"),
    (7.0, 19.0, "start"),
    (-7.0, 19.0, "end"),
    (7.0, -11.0, "start"),
    (-7.0, -11.0, "end"),
];
const TRANS_CANDS: [(f32, f32, &str); 9] = [
    (0.0, -10.0, "middle"),
    (0.0, 18.5, "middle"),
    (10.0, 3.0, "start"),
    (-10.0, 3.0, "end"),
    (6.0, -10.0, "start"),
    (-6.0, -10.0, "end"),
    (6.0, 18.5, "start"),
    (-6.0, 18.5, "end"),
    (0.0, -17.0, "middle"),
];

struct LabelItem {
    key: String,
    at: Pos2,
    text: String,
    place: bool,
    above: bool,
    deg: usize,
    order: usize,
    bounds: Rect,
}

/// Greedy, deterministic label placement (`placeLabels`): each label takes
/// the first candidate with the fewest crossings.
fn place_labels(
    items: &mut [LabelItem],
    polys: &[Vec<Pos2>],
    rects: &[Rect],
    fixed: &[Rect],
) -> BTreeMap<String, (Pos2, &'static str)> {
    items.sort_by(|a, b| b.deg.cmp(&a.deg).then(a.order.cmp(&b.order)));
    let mut labels: Vec<Rect> = fixed.to_vec();
    let mut out = BTreeMap::new();
    for it in items.iter() {
        let base: Vec<(f32, f32, &'static str)> = if it.place {
            PLACE_CANDS.to_vec()
        } else {
            TRANS_CANDS.to_vec()
        };
        let cands: Vec<(f32, f32, &'static str)> = if it.above {
            let mut v = vec![base[1], base[0]];
            v.extend_from_slice(&base[2..]);
            v
        } else {
            base
        };
        let mut best: Option<(f32, Pos2, &'static str, Rect)> = None;
        for (i, (dx, dy, anchor)) in cands.iter().enumerate() {
            let (x, y) = (it.at.x + dx, it.at.y + dy);
            let r = text_rect(x, y, &it.text, FS, anchor);
            let mut score = i as f32 * 0.05;
            for poly in polys {
                if poly.windows(2).any(|w| seg_rect(w[0], w[1], r)) {
                    score += 1.0;
                }
            }
            for rr in rects {
                if r.intersects(*rr) {
                    score += 4.0;
                }
            }
            for rr in &labels {
                if r.intersects(*rr) {
                    score += 6.0;
                }
            }
            if !it.bounds.contains_rect(r) {
                score += 3.0;
            }
            if best.as_ref().is_none_or(|b| score < b.0 - 1e-6) {
                best = Some((score, Pos2::new(x, y), anchor, r));
            }
        }
        if let Some((_, p, a, r)) = best {
            labels.push(r);
            out.insert(it.key.clone(), (p, a));
        }
    }
    out
}

fn align_of(anchor: &str) -> Align2 {
    match anchor {
        "start" => Align2::LEFT_BOTTOM,
        "end" => Align2::RIGHT_BOTTOM,
        _ => Align2::CENTER_BOTTOM,
    }
}

// --- layouts of the three patterns (frame coordinates) ------------------------

struct Inst {
    id: String,
    sub: Value,
    pattern: String,
    extra: bool,
    w: f32,
    x0: f32,
    y0: f32,
    row: usize,
    nodes: BTreeMap<String, Pos2>,
    above: BTreeSet<String>,
    /// Saga account arcs routed down the corridor: `from>to` keys.
    routed: BTreeSet<String>,
}

const R_LANE_M: f32 = 44.0;
const R_LANE_C: f32 = 70.0;
const R0: f32 = 108.0;
const R1: f32 = 158.0;
const R2: f32 = 208.0;
const R3: f32 = 258.0;
const R4: f32 = 300.0;
const RB: f32 = 334.0;

fn has(sub: &Value, kind: &str, id: &str) -> bool {
    arr(&sub[kind]).iter().any(|x| s(&x["id"]) == id)
}

fn local_layout(
    sub: &Value,
    pattern: &str,
    w: f32,
) -> (BTreeMap<String, Pos2>, BTreeSet<String>, BTreeSet<String>) {
    let mut n: BTreeMap<String, Pos2> = BTreeMap::new();
    let mut above = BTreeSet::new();
    let mut routed = BTreeSet::new();
    let cx = w / 2.0;
    match pattern {
        "queue" => {
            n.insert("items".into(), Pos2::new(cx, 124.0));
            if has(sub, "places", "slots") {
                n.insert("slots".into(), Pos2::new(cx, 244.0));
            }
            n.insert("enqueue".into(), Pos2::new(cx - 52.0, 184.0));
            n.insert("dequeue".into(), Pos2::new(cx + 52.0, 184.0));
        }
        "resource-pool" => {
            n.insert("available".into(), Pos2::new(cx, 124.0));
            n.insert("in_use".into(), Pos2::new(cx, 244.0));
            n.insert("acquire".into(), Pos2::new(cx + 52.0, 184.0));
            n.insert("release".into(), Pos2::new(cx - 52.0, 184.0));
        }
        "saga" => {
            let steps: Vec<String> = arr(&sub["params"]["steps"])
                .iter()
                .map(|x| s(&x["id"]).to_string())
                .collect();
            let ns = steps.len();
            let ncol = 2 * ns + 3;
            let sp_ = (w - 68.0) / (ncol as f32 - 1.0);
            let col = |k: usize| 34.0 + k as f32 * sp_;
            for (i, st) in steps.iter().enumerate() {
                n.insert(format!("next_{st}"), Pos2::new(col(2 * i), R0));
                n.insert(format!("do_{st}"), Pos2::new(col(2 * i + 1), R0));
                n.insert(format!("fail_{st}"), Pos2::new(col(2 * i + 1), R1));
                n.insert(format!("undo_{st}"), Pos2::new(col(2 * i + 1), R3));
                n.insert(format!("{st}_pending"), Pos2::new(col(2 * i), R1));
                n.insert(format!("{st}_failed"), Pos2::new(col(2 * i + 1), R2));
                n.insert(format!("{st}_done"), Pos2::new(col(2 * i + 2), R2));
                n.insert(format!("{st}_compensated"), Pos2::new(col(2 * i + 1), R4));
                n.insert(format!("unwind_{st}"), Pos2::new(col(2 * i + 2), R3));
            }
            n.insert("committing".into(), Pos2::new(col(2 * ns), R0));
            n.insert("commit".into(), Pos2::new(col(2 * ns + 1), R0));
            n.insert("fail_commit".into(), Pos2::new(col(2 * ns + 1), R1));
            n.insert("completed".into(), Pos2::new(col(2 * ns + 2), R0));
            n.insert("rolled_back".into(), Pos2::new(col(0), RB));
            let acct_ids: Vec<String> = arr(&sub["places"])
                .iter()
                .map(|p| s(&p["id"]).to_string())
                .filter(|id| sub["params"]["accounts"].get(id).is_some())
                .collect();
            let touches = |a: &str, t: &str| {
                arr(&sub["arcs"]).iter().any(|x| {
                    (s(&x["from"]) == a && s(&x["to"]) == t)
                        || (s(&x["to"]) == a && s(&x["from"]) == t)
                })
            };
            for (ai, a) in acct_ids.iter().enumerate() {
                let j = steps
                    .iter()
                    .position(|st| touches(a, &format!("do_{st}")))
                    .unwrap_or(ns.saturating_sub(1));
                let left = ai % 2 == 0;
                let k = (ai / 2) as f32;
                let x = col(2 * j + 1)
                    + if left { -sp_ / 2.0 } else { sp_ / 2.0 }
                    + if left { -k } else { k } * sp_;
                n.insert(
                    a.clone(),
                    Pos2::new(x, if left { R_LANE_C } else { R_LANE_M }),
                );
                above.insert(a.clone());
                for st in &steps {
                    let t = format!("undo_{st}");
                    for arc in arr(&sub["arcs"]) {
                        let (f, to) = (s(&arc["from"]), s(&arc["to"]));
                        if (f == a && to == t) || (f == t && to == a) {
                            routed.insert(format!("{f}>{to}"));
                        }
                    }
                }
            }
        }
        _ => {}
    }
    // fallback: anything the pattern layout did not place
    let missing: Vec<String> = arr(&sub["places"])
        .iter()
        .chain(arr(&sub["transitions"]))
        .map(|x| s(&x["id"]).to_string())
        .filter(|id| !n.contains_key(id))
        .collect();
    let per = (((w - 40.0) / 60.0).floor() as usize).max(1);
    for (i, id) in missing.iter().enumerate() {
        n.insert(
            id.clone(),
            Pos2::new(
                30.0 + (i % per) as f32 * 60.0,
                300.0 + (i / per) as f32 * 30.0,
            ),
        );
    }
    (n, above, routed)
}

/// Instances glued by token/data links, in left-to-right order.
fn components(ids: &[String], glue: &[Value]) -> Vec<Vec<String>> {
    let mut parent: BTreeMap<String, String> = ids.iter().map(|i| (i.clone(), i.clone())).collect();
    fn find(p: &BTreeMap<String, String>, x: &str) -> String {
        let mut x = x.to_string();
        while p[&x] != x {
            x = p[&x].clone();
        }
        x
    }
    for l in glue {
        let (a, b) = (
            find(&parent, s(&l["from"]["subnet"])),
            find(&parent, s(&l["to"]["subnet"])),
        );
        if a != b {
            parent.insert(b, a);
        }
    }
    let mut groups: Vec<(String, Vec<String>)> = vec![];
    for i in ids {
        let r = find(&parent, i);
        match groups.iter_mut().find(|g| g.0 == r) {
            Some(g) => g.1.push(i.clone()),
            None => groups.push((r, vec![i.clone()])),
        }
    }
    groups
        .into_iter()
        .map(|(_, ids)| {
            let links: Vec<&Value> = glue
                .iter()
                .filter(|l| ids.iter().any(|i| i == s(&l["from"]["subnet"])))
                .collect();
            if links.is_empty() {
                return ids;
            }
            let mut order = vec![
                s(&links[0]["from"]["subnet"]).to_string(),
                s(&links[0]["to"]["subnet"]).to_string(),
            ];
            for _ in 0..ids.len() {
                for l in &links {
                    let (a, b) = (
                        s(&l["from"]["subnet"]).to_string(),
                        s(&l["to"]["subnet"]).to_string(),
                    );
                    if order.contains(&a) && !order.contains(&b) {
                        order.push(b);
                    } else if order.contains(&b) && !order.contains(&a) {
                        order.insert(0, a);
                    }
                }
            }
            for i in &ids {
                if !order.contains(i) {
                    order.push(i.clone());
                }
            }
            order
        })
        .collect()
}

struct Glued {
    name: String,
    members: Vec<String>,
    homes: Vec<(String, String, Pos2)>,
    p: Pos2,
    left: String,
    right: String,
    flat_place: Option<Value>,
    links: Vec<usize>,
}

#[derive(Clone)]
struct Pt {
    pos: Pos2,
    trans: bool,
    home: bool,
    glued: Option<usize>,
    hpos: Pos2,
}

struct Foot {
    port: Value,
    side: &'static str,
    cx: f32,
    y: f32,
    pts: Vec<Pos2>,
    glued: bool,
    label: Option<(Pos2, &'static str, bool)>,
    target: Pt,
}

struct Tap {
    port: Value,
    tx: f32,
    ty: f32,
    pts: Vec<Pos2>,
    target: Pt,
}

struct TPort {
    port: Value,
    px: f32,
    py: f32,
}

struct ArcG {
    arc: Value,
    f: Pt,
    t: Pt,
    pts: Vec<Pos2>,
    merged: bool,
}

#[derive(Default)]
struct Geo {
    arcs: Vec<ArcG>,
    feet: Vec<Foot>,
    taps: Vec<Tap>,
    tports: Vec<TPort>,
    polys: Vec<Vec<Pos2>>,
    fixed: Vec<Rect>,
}

struct Ghost {
    l: Value,
    a: (Pos2, &'static str),
    b: (Pos2, &'static str),
    span: f32,
}

/// Token dots in a place (`tokenDots`): up to three dots, else the number.
fn token_dots(c: &Ctx, xf: Xf, at: Pos2, n: i64, r: f32, scale: f32, alpha: f32) {
    if n <= 0 {
        return;
    }
    let col = INK.gamma_multiply(alpha);
    if n > 3 {
        c.dt(
            xf,
            Pos2::new(at.x, at.y + 3.2 * scale),
            Align2::CENTER_CENTER,
            &n.to_string(),
            9.0 * scale,
            col,
            None,
            false,
        );
        return;
    }
    let pts: &[(f32, f32)] = match n {
        1 => &[(0.0, 0.0)],
        2 => &[(-2.8, 0.0), (2.8, 0.0)],
        _ => &[(0.0, -2.8), (-2.6, 2.0), (2.6, 2.0)],
    };
    let dr = if r > 9.0 { 2.4 } else { 2.0 };
    for (dx, dy) in pts {
        c.p.circle_filled(
            xf.pt(at.x + dx * scale, at.y + dy * scale),
            (dr * scale * xf.k).max(1.2),
            col,
        );
    }
}

pub(super) fn cospan_orders(c: &mut Ctx, area: Rect) {
    let subs = subnets(c);
    let ls = port_links(c);
    let by_id: BTreeMap<String, Value> = subs
        .iter()
        .map(|x| (s(&x["id"]).to_string(), x.clone()))
        .collect();
    let flat = c.d.flat.clone().unwrap_or(Value::Null);
    let flat_ok = flat["ok"] == true;
    if !flat_ok && !ls.is_empty() {
        c.note(
            "warn",
            format!(
                "Flatten refused the links ({}); no pushout is drawn.",
                s(&flat["error"])
            ),
        );
    }
    let is_glue = |l: &Value| matches!(s(&l["kind"]), "token" | "data");
    let glue: Vec<Value> = if flat_ok {
        ls.iter().filter(|l| is_glue(l)).cloned().collect()
    } else {
        vec![]
    };
    let ghosts: Vec<Value> = ls
        .iter()
        .filter(|l| !glue.iter().any(|g| g["key"] == l["key"]))
        .cloned()
        .collect();

    // 1. instances, components, rows
    let ids: Vec<String> = subs.iter().map(|x| s(&x["id"]).to_string()).collect();
    let comps = components(&ids, &glue);
    let mut inst: BTreeMap<String, Inst> = BTreeMap::new();
    for sub in &subs {
        let pattern = s(&sub["pattern"]).to_string();
        inst.insert(
            s(&sub["id"]).to_string(),
            Inst {
                id: s(&sub["id"]).into(),
                sub: sub.clone(),
                w: pref_w(&pattern),
                pattern,
                extra: sub["extra"] == true,
                x0: 0.0,
                y0: 0.0,
                row: 0,
                nodes: BTreeMap::new(),
                above: BTreeSet::new(),
                routed: BTreeSet::new(),
            },
        );
    }
    let inner = W0 - 2.0 * SIDE;
    let comp_w = |inst: &BTreeMap<String, Inst>, ids: &[String]| {
        ids.iter().map(|i| inst[i].w).sum::<f32>() + SEAM * (ids.len() as f32 - 1.0)
    };
    for ids in &comps {
        let excess = comp_w(&inst, ids) - inner;
        let sagas: Vec<&String> = ids.iter().filter(|i| inst[*i].pattern == "saga").collect();
        if excess > 0.0 && !sagas.is_empty() {
            let cut = (excess / sagas.len() as f32).min(pref_w("saga") - MIN_SAGA);
            let sagas: Vec<String> = sagas.into_iter().cloned().collect();
            for i in sagas {
                inst.get_mut(&i).unwrap().w = pref_w("saga") - cut;
            }
        }
    }
    struct Row {
        comps: Vec<Vec<String>>,
        width: f32,
        y0: f32,
        x: f32,
    }
    let mut rows: Vec<Row> = vec![];
    for ids in &comps {
        let w = comp_w(&inst, ids);
        match rows.last_mut() {
            Some(r) if r.width + GAP + w <= inner => {
                r.comps.push(ids.clone());
                r.width += GAP + w;
            }
            _ => rows.push(Row {
                comps: vec![ids.clone()],
                width: w,
                y0: 0.0,
                x: 0.0,
            }),
        }
    }
    let w_total = rows.iter().map(|r| r.width + 2.0 * SIDE).fold(W0, f32::max);
    let row_pitch = ROW_HEAD + FH + LANE;
    let h_total = HEAD + rows.len().max(1) as f32 * row_pitch + FOOT;
    for (ri, row) in rows.iter_mut().enumerate() {
        let top = HEAD + ri as f32 * row_pitch;
        row.y0 = top + ROW_HEAD;
        let mut x = (w_total - row.width) / 2.0;
        row.x = x;
        for (ci, ids) in row.comps.iter().enumerate() {
            if ci > 0 {
                x += GAP;
            }
            for (k, id) in ids.iter().enumerate() {
                if k > 0 {
                    x += SEAM;
                }
                let i = inst.get_mut(id).unwrap();
                i.x0 = x;
                i.y0 = row.y0;
                i.row = ri;
                x += i.w;
            }
        }
    }
    for i in inst.values_mut() {
        let (n, a, r) = local_layout(&i.sub, &i.pattern, i.w);
        i.nodes = n;
        i.above = a;
        i.routed = r;
    }
    let comp_of =
        |id: &str| -> &Vec<String> { comps.iter().find(|c| c.iter().any(|x| x == id)).unwrap() };

    // 3. glued places: one per flat wire class spanning two instances
    let mut gl: Vec<Glued> = vec![];
    let mut merged: BTreeMap<String, usize> = BTreeMap::new();
    if flat_ok {
        for (name, members) in flat["wires"].as_object().into_iter().flatten() {
            let members = strs(members);
            let mut sids: Vec<String> = vec![];
            for m in &members {
                let sid = m.split('/').next().unwrap_or("").to_string();
                if inst.contains_key(&sid) && !sids.contains(&sid) {
                    sids.push(sid);
                }
            }
            if sids.len() < 2 {
                continue;
            }
            let comp = comp_of(&sids[0]);
            let left = sids
                .iter()
                .filter_map(|x| comp.iter().position(|y| y == x))
                .min()
                .unwrap_or(0);
            let a = &inst[&comp[left]];
            let b = comp.get(left + 1).map_or(a, |id| &inst[id]);
            let homes: Vec<(String, String, Pos2)> = members
                .iter()
                .filter_map(|m| {
                    let (sid, pid) = m.split_once('/')?;
                    let i = inst.get(sid)?;
                    let n = i.nodes.get(pid)?;
                    Some((
                        sid.to_string(),
                        pid.to_string(),
                        Pos2::new(i.x0 + n.x, i.y0 + n.y),
                    ))
                })
                .collect();
            let y = homes.iter().map(|h| h.2.y).sum::<f32>() / homes.len().max(1) as f32;
            let flat_place = arr(&flat["places"])
                .iter()
                .find(|p| s(&p["id"]) == name)
                .cloned();
            let idx = gl.len();
            for m in &members {
                merged.insert(m.clone(), idx);
            }
            gl.push(Glued {
                name: name.clone(),
                members,
                homes,
                p: Pos2::new((a.x0 + a.w + b.x0) / 2.0, y),
                left: a.id.clone(),
                right: b.id.clone(),
                flat_place,
                links: vec![],
            });
        }
        // spread glued places sharing a seam so two never sit within 64
        let mut by_seam: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        for (i, g) in gl.iter().enumerate() {
            by_seam.entry(g.left.clone()).or_default().push(i);
        }
        for list in by_seam.values_mut() {
            list.sort_by(|a, b| {
                gl[*a]
                    .p
                    .y
                    .total_cmp(&gl[*b].p.y)
                    .then(gl[*a].name.cmp(&gl[*b].name))
            });
            for k in 1..list.len() {
                let prev = gl[list[k - 1]].p.y;
                if gl[list[k]].p.y - prev < 64.0 {
                    gl[list[k]].p.y = prev + 64.0;
                }
            }
        }
        for (li, l) in ls.iter().enumerate() {
            if !is_glue(l) {
                continue;
            }
            let fs = &by_id[s(&l["from"]["subnet"])];
            let fp = arr(&fs["ports"])
                .iter()
                .find(|p| p["id"] == l["from"]["port"]);
            if let Some(fp) = fp {
                let key = format!("{}/{}", s(&l["from"]["subnet"]), s(&fp["place"]));
                if let Some(&gi) = merged.get(&key) {
                    gl[gi].links.push(li);
                }
            }
        }
    }
    let pos_of = |sid: &str, id: &str| -> Pt {
        let i = &inst[sid];
        let n = i.nodes.get(id).copied().unwrap_or(Pos2::ZERO);
        let hpos = Pos2::new(i.x0 + n.x, i.y0 + n.y);
        let trans = has(&i.sub, "transitions", id);
        match merged.get(&format!("{sid}/{id}")) {
            Some(&g) => Pt {
                pos: gl[g].p,
                trans,
                home: false,
                glued: Some(g),
                hpos,
            },
            None => Pt {
                pos: hpos,
                trans,
                home: true,
                glued: None,
                hpos,
            },
        }
    };
    // foot points glued at a seam are drawn there (half-rings), not on their column
    let mut glued_side: BTreeSet<String> = BTreeSet::new();
    for g in &gl {
        for &li in &g.links {
            let l = &ls[li];
            for end in [&l["from"], &l["to"]] {
                let sid = s(&end["subnet"]);
                let side = if sid == g.left {
                    Some("Y")
                } else if sid == g.right {
                    Some("X")
                } else {
                    None
                };
                let port = arr(&by_id[sid]["ports"])
                    .iter()
                    .find(|p| p["id"] == end["port"]);
                if let (Some(side), Some(port)) = (side, port) {
                    if port_sides(port).contains(&side) {
                        glued_side.insert(format!("{sid}|{}|{side}", s(&end["port"])));
                    }
                }
            }
        }
    }

    // 4. geometry
    let mut geo: BTreeMap<String, Geo> = BTreeMap::new();
    for sub in &subs {
        let sid = s(&sub["id"]);
        let i = &inst[sid];
        let (x0, y0, w) = (i.x0, i.y0, i.w);
        let mut g = Geo::default();
        for a in arr(&sub["arcs"]) {
            let (af, at) = (s(&a["from"]), s(&a["to"]));
            let f = pos_of(sid, af);
            let t = pos_of(sid, at);
            let route_key = format!("{af}>{at}");
            let mut pts: Vec<Pos2> = vec![];
            let mut routed = false;
            if i.routed.contains(&route_key) && f.home && t.home {
                // an account's arc to an undo transition runs down the free corridor
                let (acct, tr, from_t) =
                    if i.nodes.contains_key(af) && f.pos.y < t.pos.y && !f.trans {
                        (f.pos, t.pos, false)
                    } else {
                        (t.pos, f.pos, true)
                    };
                let sx = tr.x + if acct.x < tr.x { -3.5 } else { 3.5 };
                let knee = Pos2::new(acct.x, tr.y - 15.0);
                let top = Pos2::new(sx, tr.y - 6.5);
                let c1 = Pos2::new(acct.x, tr.y - 8.0);
                let c2 = Pos2::new(sx, tr.y - 13.0);
                if from_t {
                    pts = cubic_pts(top, c2, c1, knee, 6);
                    pts.push(Pos2::new(acct.x, acct.y + 9.0));
                } else {
                    pts.push(Pos2::new(acct.x, acct.y + 9.0));
                    pts.extend(cubic_pts(knee, c1, c2, top, 6));
                }
                routed = true;
            }
            if !routed {
                if !f.home || !t.home {
                    let (m, o, m_is_from) = if !f.home {
                        (&f, &t, true)
                    } else {
                        (&t, &f, false)
                    };
                    let cc = m.hpos;
                    let (dxo, dyo) = (cc.x - o.pos.x, cc.y - o.pos.y);
                    let lo = (dxo * dxo + dyo * dyo).sqrt().max(1e-6);
                    let r_o = if o.trans {
                        7.0 / (dxo / lo).abs().max((dyo / lo).abs()).max(1e-6)
                    } else {
                        9.0
                    };
                    let op = Pos2::new(o.pos.x + dxo / lo * r_o, o.pos.y + dyo / lo * r_o);
                    let (dx, dy) = (m.pos.x - cc.x, m.pos.y - cc.y);
                    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
                    let mp = Pos2::new(m.pos.x - dx / len * 12.0, m.pos.y - dy / len * 12.0);
                    pts = if m_is_from {
                        quad_pts(mp, cc, op, 10)
                    } else {
                        quad_pts(op, cc, mp, 10)
                    };
                } else {
                    let (a0, b0) = clip(
                        f.pos,
                        t.pos,
                        if f.trans { 6.5 } else { 8.5 },
                        if t.trans { 7.0 } else { 9.0 },
                        f.trans,
                        t.trans,
                    );
                    pts = vec![a0, b0];
                }
            }
            g.polys.push(pts.clone());
            let merged_end = !f.home || !t.home;
            g.arcs.push(ArcG {
                arc: a.clone(),
                f,
                t,
                pts,
                merged: merged_end,
            });
        }
        // feet
        for side in ["X", "Y"] {
            let cx = if side == "X" {
                x0 - COLX
            } else {
                x0 + w + COLX
            };
            let mut pts: Vec<(Value, f32, f32)> = arr(&sub["ports"])
                .iter()
                .filter(|p| port_sides(p).contains(&side))
                .map(|p| {
                    let hy = y0 + i.nodes.get(s(&p["place"])).map_or(0.0, |n| n.y);
                    (p.clone(), hy, hy)
                })
                .collect();
            pts.sort_by(|a, b| a.1.total_cmp(&b.1));
            for k in 1..pts.len() {
                if pts[k].2 - pts[k - 1].2 < 18.0 {
                    pts[k].2 = pts[k - 1].2 + 18.0;
                }
            }
            for (port, _, y) in pts {
                let key = format!("{sid}|{}|{side}", s(&port["id"]));
                let target = pos_of(sid, s(&port["place"]));
                let start = Pos2::new(cx + if side == "X" { 6.0 } else { -6.0 }, y);
                let lp = if !target.home {
                    let cc = Pos2::new(target.hpos.x, y);
                    let end = Pos2::new(
                        target.pos.x + if target.pos.x > cc.x { -15.0 } else { 15.0 },
                        target.pos.y,
                    );
                    quad_pts(start, cc, end, 10)
                } else {
                    vec![
                        start,
                        Pos2::new(
                            target.pos.x + if side == "X" { -10.0 } else { 10.0 },
                            target.pos.y,
                        ),
                    ]
                };
                let glued = glued_side.contains(&key);
                let label_w = text_w(s(&port["id"]), FS);
                let leg_len = ((if target.home {
                    target.pos.x
                } else if side == "X" {
                    x0 + w
                } else {
                    x0
                }) - cx)
                    .abs();
                let label = if leg_len >= label_w + 30.0 {
                    Some((
                        Pos2::new(if side == "X" { cx + 9.0 } else { cx - 9.0 }, y - 5.0),
                        if side == "X" { "start" } else { "end" },
                        true,
                    ))
                } else if s(&port["id"]) != s(&port["place"]) {
                    Some((Pos2::new(cx, y - 10.0), "middle", false))
                } else {
                    None
                };
                if !glued {
                    g.polys.push(lp.clone());
                    if let Some((p, a, _)) = &label {
                        g.fixed.push(text_rect(p.x, p.y, s(&port["id"]), FS, a));
                    }
                }
                g.feet.push(Foot {
                    port: port.clone(),
                    side: if side == "X" { "X" } else { "Y" },
                    cx,
                    y,
                    pts: lp,
                    glued,
                    label,
                    target,
                });
            }
        }
        // observe taps on the top edge
        for port in arr(&sub["ports"])
            .iter()
            .filter(|p| port_sides(p).contains(&"top"))
        {
            let home = i
                .nodes
                .get(s(&port["place"]))
                .copied()
                .unwrap_or(Pos2::ZERO);
            let home = Pos2::new(x0 + home.x, y0 + home.y);
            let target = pos_of(sid, s(&port["place"]));
            let (tx, ty) = (home.x, y0);
            let pts = if target.home {
                vec![Pos2::new(tx, ty + 5.0), Pos2::new(tx, target.pos.y - 10.0)]
            } else {
                let end = Pos2::new(target.pos.x, target.pos.y - 15.0);
                quad_pts(Pos2::new(tx, ty + 5.0), Pos2::new(tx, home.y), end, 10)
            };
            g.polys.push(pts.clone());
            g.taps.push(Tap {
                port: port.clone(),
                tx,
                ty,
                pts,
                target,
            });
        }
        // event ports on the bottom edge
        let mut tps: Vec<(usize, Value, f32)> = arr(&sub["ports"])
            .iter()
            .filter(|p| s(&p["target"]) == "transition")
            .enumerate()
            .map(|(k, p)| {
                (
                    k,
                    p.clone(),
                    i.nodes.get(s(&p["transition"])).map_or(0.0, |n| n.x),
                )
            })
            .collect();
        tps.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
        let spread = tps.windows(2).any(|w| w[1].2 - w[0].2 < 60.0);
        let n = tps.len();
        for (k, (_, port, tx)) in tps.into_iter().enumerate() {
            let px = x0
                + if spread {
                    30.0 + k as f32 * (w - 60.0) / (n.saturating_sub(1).max(1) as f32)
                } else {
                    tx
                };
            g.tports.push(TPort {
                port,
                px,
                py: y0 + FH,
            });
        }
        geo.insert(sid.to_string(), g);
    }

    // 5. node labels
    let mut label_at: BTreeMap<String, (Pos2, &'static str)> = BTreeMap::new();
    for sub in &subs {
        let sid = s(&sub["id"]);
        let i = &inst[sid];
        let g = &geo[sid];
        let mut rects = vec![];
        let nodes: Vec<(String, bool)> = arr(&sub["places"])
            .iter()
            .map(|p| (s(&p["id"]).to_string(), true))
            .chain(
                arr(&sub["transitions"])
                    .iter()
                    .map(|t| (s(&t["id"]).to_string(), false)),
            )
            .collect();
        for (id, _) in &nodes {
            let p = pos_of(sid, id);
            if p.home {
                rects.push(Rect::from_min_size(
                    Pos2::new(p.pos.x - 9.0, p.pos.y - 9.0),
                    Vec2::splat(18.0),
                ));
            }
        }
        let mut deg: BTreeMap<&str, usize> = BTreeMap::new();
        for a in arr(&sub["arcs"]) {
            *deg.entry(s(&a["from"])).or_default() += 1;
            *deg.entry(s(&a["to"])).or_default() += 1;
        }
        let bounds = Rect::from_min_size(
            Pos2::new(i.x0 + 2.0, i.y0 + 2.0),
            Vec2::new(i.w - 4.0, FH - BAND - 4.0),
        );
        let mut items: Vec<LabelItem> = nodes
            .iter()
            .enumerate()
            .filter(|(_, (id, _))| pos_of(sid, id).home)
            .map(|(order, (id, place))| LabelItem {
                key: format!("{sid}/{id}"),
                at: pos_of(sid, id).pos,
                text: id.clone(),
                place: *place,
                above: i.above.contains(id),
                deg: deg.get(id.as_str()).copied().unwrap_or(0),
                order,
                bounds,
            })
            .collect();
        label_at.extend(place_labels(&mut items, &g.polys, &rects, &g.fixed));
    }

    // --- drawing -----------------------------------------------------------------
    let xf = Xf::fit(area, w_total, h_total, 1.7);
    let k = xf.k;
    let wd = |x: f32| (x * k).max(0.7);
    let sp_ = |pts: &[Pos2]| -> Vec<Pos2> { pts.iter().map(|p| xf.pt(p.x, p.y)).collect() };
    let text_col = hex("#4a525c");
    let dtb = |c: &Ctx,
               at: Pos2,
               anchor: &str,
               t: &str,
               size: f32,
               col: Color32,
               halo: Color32,
               mono: bool| {
        c.dt(
            xf,
            Pos2::new(at.x, at.y + size * 0.2),
            align_of(anchor),
            t,
            size,
            col,
            Some(halo),
            mono,
        )
    };
    let leak_event = c.anim(|e| match e {
        Event::LinkAdded { key, kind } => Some((key.clone(), kind.clone())),
        _ => None,
    });

    let inst_event = c.anim(|e| match e {
        Event::InstanceAdded { id } => Some(id.clone()),
        _ => None,
    });
    let ptr_design = c
        .pointer
        .map(|p| Pos2::new((p.x - xf.origin.x) / k, (p.y - xf.origin.y) / k));

    // header
    c.dt(
        xf,
        Pos2::new(SIDE - 22.0, 20.0),
        Align2::LEFT_BOTTOM,
        "Each instance is an open Petri net: a cospan X → S ← Y of finite sets into its places S (Baez–Master).",
        12.5,
        INK,
        None,
        false,
    );
    c.dt(
        xf,
        Pos2::new(SIDE - 22.0, 37.0),
        Align2::LEFT_BOTTOM,
        "Feet X (left: in and inout ports) and Y (right: out and inout ports) map by legs (grey dashed) to places. Gluing Y of one net to X of the next is a pushout; in go-pflow that is lk:token and lk:data.",
        10.5,
        MUTED,
        None,
        false,
    );

    // frames, titles, feet columns, event bands
    for sub in &subs {
        let sid = s(&sub["id"]);
        let i = &inst[sid];
        let (x0, y0, w) = (i.x0, i.y0, i.w);
        let fr = xf.rect(x0, y0, w, FH);
        c.p.rect(
            fr,
            9.0 * k,
            frame_fill(&i.pattern),
            Stroke::new(wd(1.2), hex("#c5ccd5")),
            StrokeKind::Middle,
        );
        let halo = frame_fill(&i.pattern);
        let xs: Vec<&Value> = arr(&sub["ports"])
            .iter()
            .filter(|p| port_sides(p).contains(&"X"))
            .collect();
        let ys: Vec<&Value> = arr(&sub["ports"])
            .iter()
            .filter(|p| port_sides(p).contains(&"Y"))
            .collect();
        let taps: Vec<&Value> = arr(&sub["ports"])
            .iter()
            .filter(|p| port_sides(p).contains(&"top"))
            .collect();
        let tps: Vec<&Value> = arr(&sub["ports"])
            .iter()
            .filter(|p| s(&p["target"]) == "transition")
            .collect();
        let names = |v: &[&Value]| v.iter().map(|p| s(&p["id"])).collect::<Vec<_>>().join(", ");
        if c.pointer_in(fr) {
            let mut inf = subnet_info(sub);
            let mut facts = vec![
                (
                    "open net".to_string(),
                    format!(
                        "X = {{{}}} → S ({} places) ← Y = {{{}}}",
                        names(&xs),
                        arr(&sub["places"]).len(),
                        names(&ys)
                    ),
                ),
                ("legs".to_string(), {
                    let mut v: Vec<String> = vec![];
                    for p in xs.iter().chain(ys.iter()) {
                        let t = format!("{} ↦ {}", s(&p["id"]), s(&p["place"]));
                        if !v.contains(&t) {
                            v.push(t);
                        }
                    }
                    v.join("; ")
                }),
            ];
            if !taps.is_empty() {
                facts.push((
                    "taps (in neither foot)".into(),
                    taps.iter()
                        .map(|p| format!("{} ↦ {}", s(&p["id"]), s(&p["place"])))
                        .collect::<Vec<_>>()
                        .join("; "),
                ));
            }
            facts.push(("event ports (not legs)".into(), names(&tps)));
            facts.append(&mut inf.lines);
            inf.lines = facts;
            inf.note = "An open net in the Baez–Master sense: the feet X and Y are finite sets, the legs are functions into the places. Transition ports are not part of the cospan; event links compose them by fusion (nets with boundaries).".into();
            let mut cites = vec![Cites::cospan_row(), Cites::sides()];
            cites.append(&mut inf.cites);
            inf.cites = cites;
            c.hover_if(true, inf);
        }
        let pat_label = s(&comp::ontology()["patterns"][&i.pattern]["prefLabel"]).to_string();
        let pat_label = if pat_label.is_empty() {
            i.pattern.clone()
        } else {
            pat_label
        };
        dtb(
            c,
            Pos2::new(x0 - COLX, y0 - 25.0),
            "start",
            &i.id,
            12.5,
            INK,
            PAPER,
            false,
        );
        let nums = format!(
            "{} → {} ← {}",
            xs.len(),
            arr(&sub["places"]).len(),
            ys.len()
        );
        dtb(
            c,
            Pos2::new(x0 + w + COLX, y0 - 25.0),
            "end",
            &nums,
            9.5,
            MUTED,
            PAPER,
            true,
        );
        let room = w + 2.0 * COLX
            - (text_w(&i.id, 12.5) * 1.06 + 7.0)
            - (nums.chars().count() as f32 * 9.5 * 0.62 + 10.0);
        let pats = [
            if i.extra {
                format!("{pat_label} · second instance")
            } else {
                pat_label.clone()
            },
            if i.extra {
                format!("{pat_label} · extra")
            } else {
                pat_label.clone()
            },
            pat_label.clone(),
            String::new(),
        ];
        if let Some(t) = pats
            .iter()
            .find(|t| t.chars().count() as f32 * 10.0 * 0.6 <= room)
        {
            if !t.is_empty() {
                dtb(
                    c,
                    Pos2::new(x0 - COLX + text_w(&i.id, 12.5) * 1.06 + 7.0, y0 - 25.0),
                    "start",
                    t,
                    10.0,
                    MUTED,
                    PAPER,
                    false,
                );
            }
        }
        let band_top = y0 + FH - BAND;
        for (side, cx) in [("X", x0 - COLX), ("Y", x0 + w + COLX)] {
            c.dash_poly(
                &sp_(&[Pos2::new(cx, y0 + 14.0), Pos2::new(cx, band_top - 6.0)]),
                hex("#b9c0c8"),
                wd(1.0),
                wd(1.5),
                wd(3.0),
            );
            let r = c.dt(
                xf,
                Pos2::new(cx, y0 - 8.0),
                Align2::CENTER_BOTTOM,
                side,
                12.0,
                INK,
                Some(PAPER),
                false,
            );
            if c.pointer_in(r.expand(3.0)) {
                let pts = if side == "X" { &xs } else { &ys };
                let inf = info(format!(
                    "{} of {sid}",
                    if side == "X" { "left foot X" } else { "right foot Y" }
                ))
                .fact(
                    "points",
                    if pts.is_empty() { "∅".to_string() } else { names(pts) },
                )
                .note(if side == "X" {
                    "The boundary points the net can be glued along on its left: its in and inout place ports."
                } else {
                    "The boundary points the net can be glued along on its right: its out and inout place ports."
                })
                .cite(Cites::sides());
                c.hover_if(true, inf);
            }
        }
        c.dash_poly(
            &sp_(&[
                Pos2::new(x0 + 8.0, band_top),
                Pos2::new(x0 + w - 8.0, band_top),
            ]),
            hex("#c9cfd6"),
            wd(1.0),
            wd(3.0),
            wd(3.0),
        );
        if w > 320.0 {
            c.dt(
                xf,
                Pos2::new(x0 + w / 2.0, band_top + 12.0),
                Align2::CENTER_BOTTOM,
                "event ports: not part of a cospan (see Nets with boundaries)",
                7.5,
                MUTED,
                None,
                false,
            );
        } else {
            c.dt(
                xf,
                Pos2::new(x0 + w / 2.0, band_top + 11.0),
                Align2::CENTER_BOTTOM,
                "event ports: not part of a cospan",
                7.5,
                MUTED,
                None,
                false,
            );
            c.dt(
                xf,
                Pos2::new(x0 + w / 2.0, band_top + 20.0),
                Align2::CENTER_BOTTOM,
                "(see Nets with boundaries)",
                7.5,
                MUTED,
                None,
                false,
            );
        }
        let _ = halo;
    }

    // the animation state of a link-added token/data link
    let fade_for = |gi: usize| -> f32 {
        match &leak_event {
            Some(((key, kind), t))
                if matches!(kind.as_str(), "token" | "data")
                    && gl[gi].links.iter().any(|&li| s(&ls[li]["key"]) == key) =>
            {
                ((*t - 0.5) / 0.5).max(0.0)
            }
            _ => 1.0,
        }
    };

    // arcs
    for sub in &subs {
        let sid = s(&sub["id"]);
        let pattern_cites = comp::ontology()["patterns"][s(&sub["pattern"])]["cites"].clone();
        for ag in &geo[sid].arcs {
            let (f, t, a) = (&ag.f, &ag.t, &ag.arc);
            let gi = f.glued.or(t.glued);
            let alpha = gi.map_or(1.0, fade_for);
            let col = if ag.merged {
                kind_color("token").gamma_multiply(0.75)
            } else {
                ARC
            };
            let col = col.gamma_multiply(alpha);
            let pts = sp_(&ag.pts);
            c.poly(&pts, col, wd(1.0));
            if let [.., p, q] = pts.as_slice() {
                c.arrow_head(*p, *q, wd(5.5), col);
            }
            let weight = a["weight"].as_f64().unwrap_or(1.0);
            if weight > 1.0 {
                let mid = Pos2::new((f.pos.x + t.pos.x) / 2.0, (f.pos.y + t.pos.y) / 2.0);
                c.dt(
                    xf,
                    Pos2::new(mid.x + 4.0, mid.y - 3.0),
                    Align2::LEFT_BOTTOM,
                    &format!("{weight}"),
                    8.0,
                    text_col,
                    Some(frame_fill(&inst[sid].pattern)),
                    false,
                );
            }
            if c.near_poly(&pts, 4.0) {
                let input = !f.trans;
                let mut inf = info(format!(
                    "arc {sid}/{} → {sid}/{}",
                    s(&a["from"]),
                    s(&a["to"])
                ))
                .fact("weight", format!("{weight}"))
                .note(if input {
                    "Place to transition. Weight tokens are consumed when the transition fires."
                } else {
                    "Transition to place. Weight tokens are produced when the transition fires."
                })
                .cite(Cites::arcs());
                if ag.merged {
                    let g = &gl[gi.unwrap_or(0)];
                    inf = inf.fact(
                        "after the pushout",
                        format!(
                            "the place is {}: this arc now reads or writes the shared place",
                            g.name
                        ),
                    );
                }
                inf = inf.cites_of(&pattern_cites);
                c.hover_if(true, inf);
            }
        }
    }

    // legs (feet and taps)
    for sub in &subs {
        let sid = s(&sub["id"]);
        let g = &geo[sid];
        for ft in &g.feet {
            if ft.glued {
                continue;
            }
            let alpha = ft.target.glued.map_or(1.0, fade_for);
            let pts = sp_(&ft.pts);
            c.dash_poly(&pts, LEG.gamma_multiply(alpha), wd(1.0), wd(4.0), wd(3.0));
            if c.near_poly(&pts, 4.0) {
                let place = if ft.target.home {
                    format!("{sid}/{}", s(&ft.port["place"]))
                } else {
                    gl[ft.target.glued.unwrap_or(0)].name.clone()
                };
                let mut inf = foot_info(sub, &ft.port, ft.side);
                inf.title = format!("leg {sid}:{} ↦ {place}", s(&ft.port["id"]));
                inf.note = format!(
                    "A leg of the cospan: the function from the foot {} to the places sends this point to {}.",
                    ft.side,
                    if ft.target.home { s(&ft.port["place"]).to_string() } else { format!("the glued place {place}") }
                );
                c.hover_if(true, inf);
            }
        }
        for tp in &g.taps {
            let col = proto_port_color(&tp.port);
            let alpha = tp.target.glued.map_or(1.0, fade_for);
            let pts = sp_(&tp.pts);
            c.dash_poly(&pts, col.gamma_multiply(alpha), wd(1.3), wd(1.5), wd(3.0));
            if c.near_poly(&pts, 4.0) {
                c.hover_if(true, tap_info(sub, &tp.port));
            }
        }
    }

    // places and transitions (a glued place is drawn once, on its seam)
    let hot_tr: Option<(String, String)> = ptr_design.and_then(|p| {
        for sub in &subs {
            let sid = s(&sub["id"]);
            for tp in &geo[sid].tports {
                if Rect::from_center_size(Pos2::new(tp.px, tp.py), Vec2::splat(18.0)).contains(p) {
                    return Some((sid.to_string(), s(&tp.port["transition"]).to_string()));
                }
            }
            for t in arr(&sub["transitions"]) {
                let q = pos_of(sid, s(&t["id"]));
                if Rect::from_center_size(q.pos, Vec2::splat(14.0)).contains(p) {
                    return Some((sid.to_string(), s(&t["id"]).to_string()));
                }
            }
        }
        None
    });
    for sub in &subs {
        let sid = s(&sub["id"]);
        let fill = frame_fill(s(&sub["pattern"]));
        for pl in arr(&sub["places"]) {
            let pid = s(&pl["id"]);
            if merged.contains_key(&format!("{sid}/{pid}")) {
                continue;
            }
            let p = pos_of(sid, pid);
            let q = arr(&sub["ports"])
                .iter()
                .find(|q| s(&q["target"]) == "place" && s(&q["place"]) == pid);
            let stroke = q.map_or(hex("#8a929c"), proto_port_color);
            let ctr = xf.pt(p.pos.x, p.pos.y);
            c.p.circle(
                ctr,
                8.0 * k,
                Color32::WHITE,
                Stroke::new(wd(if pl["exported"] == true { 1.8 } else { 1.2 }), stroke),
            );
            token_dots(
                c,
                xf,
                p.pos,
                pl["initial"].as_i64().unwrap_or(0),
                8.0,
                1.0,
                1.0,
            );
            if let Some((lp, an)) = label_at.get(&format!("{sid}/{pid}")) {
                dtb(c, *lp, an, pid, FS, text_col, fill, false);
            }
            if ctr.distance(c.pointer.unwrap_or(Pos2::new(-1e5, -1e5))) <= 9.0 * k {
                c.hover_if(true, place_info(sub, pl));
            }
        }
        for t in arr(&sub["transitions"]) {
            let tid = s(&t["id"]);
            let p = pos_of(sid, tid);
            let exported = arr(&sub["ports"])
                .iter()
                .any(|q| s(&q["transition"]) == tid);
            let hot = hot_tr.as_ref().is_some_and(|h| h.0 == sid && h.1 == tid);
            let r = xf.rect(p.pos.x - 6.0, p.pos.y - 6.0, 12.0, 12.0);
            c.p.rect(
                r,
                1.5 * k,
                Color32::WHITE,
                Stroke::new(
                    if hot { wd(2.2) } else { wd(1.3) },
                    if hot { Color32::BLACK } else { NODE },
                ),
                StrokeKind::Middle,
            );
            if exported {
                c.p.rect_filled(xf.rect(p.pos.x - 6.0, p.pos.y + 3.5, 12.0, 2.5), 0.0, NODE);
            }
            if let Some((lp, an)) = label_at.get(&format!("{sid}/{tid}")) {
                dtb(c, *lp, an, tid, FS, text_col, fill, false);
            }
            if c.pointer_in(r.expand(2.0)) {
                let mut inf = info(format!("transition {sid}/{tid}"))
                    .fact("ports", {
                        let v: Vec<&str> = arr(&sub["ports"])
                            .iter()
                            .filter(|q| s(&q["transition"]) == tid)
                            .map(|q| s(&q["id"]))
                            .collect();
                        if v.is_empty() {
                            "none: inside the boundary".to_string()
                        } else {
                            v.join(", ")
                        }
                    })
                    .note(s(&t["description"]).to_string());
                if exported {
                    inf.note = "The dark bar marks a transition exposed as an event port (bottom edge). Hover the port to see the connection.".into();
                }
                c.hover_if(true, inf);
            }
        }
    }

    // ports
    let mut port_at: BTreeMap<(String, String, String), Pos2> = BTreeMap::new();
    for sub in &subs {
        let sid = s(&sub["id"]);
        let g = &geo[sid];
        let fill = frame_fill(s(&sub["pattern"]));
        for ft in &g.feet {
            if ft.glued {
                continue;
            }
            let col = proto_port_color(&ft.port);
            let at = Pos2::new(ft.cx, ft.y);
            port_at.insert((sid.into(), s(&ft.port["id"]).into(), ft.side.into()), at);
            let ctr = xf.pt(at.x, at.y);
            let hit = Rect::from_center_size(ctr, Vec2::splat((18.0 * k).max(14.0)));
            let z = c.port_zone(hit, sid, &ft.port, |_| {
                let mut i = foot_info(sub, &ft.port, ft.side);
                i.title = format!("port {sid}:{}", s(&ft.port["id"]));
                i
            });
            let col = if z.dim { col.gamma_multiply(0.3) } else { col };
            c.p.circle(ctr, 5.5 * k, col, Stroke::new(wd(1.5), Color32::WHITE));
            c.zone_ring_circle(ctr, 9.0 * k, &z);
            if let Some((lp, an, own)) = &ft.label {
                dtb(
                    c,
                    *lp,
                    an,
                    s(&ft.port["id"]),
                    FS,
                    col,
                    if *own { fill } else { PAPER },
                    false,
                );
            }
        }
        for tp in &g.taps {
            let col = proto_port_color(&tp.port);
            let at = Pos2::new(tp.tx, tp.ty);
            port_at.insert((sid.into(), s(&tp.port["id"]).into(), "top".into()), at);
            let ctr = xf.pt(at.x, at.y);
            let hit = Rect::from_center_size(ctr, Vec2::splat((18.0 * k).max(14.0)));
            let z = c.port_zone(hit, sid, &tp.port, |_| tap_info(sub, &tp.port));
            let col = if z.dim { col.gamma_multiply(0.3) } else { col };
            c.p.circle(ctr, 5.0 * k, Color32::WHITE, Stroke::new(wd(1.8), col));
            c.p.circle_filled(ctr, 1.6 * k, col);
            c.zone_ring_circle(ctr, 9.0 * k, &z);
            dtb(
                c,
                Pos2::new(tp.tx, tp.ty - 9.0),
                "middle",
                &format!("{} (observe)", s(&tp.port["id"])),
                FS,
                col,
                PAPER,
                false,
            );
        }
        for tp in &g.tports {
            let at = Pos2::new(tp.px, tp.py);
            port_at.insert((sid.into(), s(&tp.port["id"]).into(), "bottom".into()), at);
            let ctr = xf.pt(at.x, at.y);
            let hit = Rect::from_center_size(ctr, Vec2::splat((16.0 * k).max(14.0)));
            let z = c.port_zone(hit, sid, &tp.port, |mut i| {
                i.note = "A transition port is not a leg of a Baez–Master cospan, whose feet are sets of places. Event links compose it by transition fusion: nets with boundaries (Bruni et al.).".into();
                i.cites.push(Cites::boundaries_row());
                i.cites.push(Cites::event());
                i
            });
            let col = if z.dim {
                NODE.gamma_multiply(0.3)
            } else {
                NODE
            };
            let r = Rect::from_center_size(ctr, Vec2::splat(9.0 * k));
            c.p.rect(
                r,
                1.0 * k,
                col,
                Stroke::new(wd(1.2), Color32::WHITE),
                StrokeKind::Middle,
            );
            c.zone_ring_rect(r.expand(3.0), &z);
            dtb(
                c,
                Pos2::new(tp.px, tp.py - 8.0),
                "middle",
                s(&tp.port["id"]),
                7.5,
                NODE,
                fill,
                false,
            );
            // the connector to its transition while the port or the transition is hot
            let hot = hot_tr
                .as_ref()
                .is_some_and(|h| h.0 == sid && h.1 == s(&tp.port["transition"]));
            if hot {
                let t = pos_of(sid, s(&tp.port["transition"]));
                let pts = cubic_pts(
                    Pos2::new(tp.px, tp.py - 5.0),
                    Pos2::new(tp.px, tp.py - 60.0),
                    Pos2::new(t.pos.x, t.pos.y + 60.0),
                    Pos2::new(t.pos.x, t.pos.y + 7.0),
                    16,
                );
                c.dash_poly(&sp_(&pts), NODE, wd(1.2), wd(2.0), wd(2.5));
            }
        }
    }

    // a new instance fades in
    if let Some((id, t)) = &inst_event {
        if let Some(i) = inst.get(id) {
            let r = xf.rect(i.x0 - 40.0, i.y0 - 40.0, i.w + 80.0, FH + 60.0);
            c.p.rect_filled(r, 0.0, PAPER.gamma_multiply(1.0 - *t));
        }
    }

    // 6. seams: the pushout
    let mut seams_done: BTreeSet<String> = BTreeSet::new();
    for (gi, g) in gl.iter().enumerate() {
        let a = &inst[&g.left];
        let b = &inst[&g.right];
        let kinds: Vec<&str> = g.links.iter().map(|&li| s(&ls[li]["kind"])).collect();
        let lk = if kinds.contains(&"token") {
            "token"
        } else {
            "data"
        };
        let lc = kind_color(lk);
        if seams_done.insert(a.id.clone()) {
            let top = a.y0 + 10.0;
            let bot = a.y0 + FH - BAND - 6.0;
            let mut busy: Vec<(f32, f32)> = gl
                .iter()
                .filter(|q| q.left == a.id)
                .map(|q| {
                    (
                        q.p.y - 15.0 - 5.0 * q.links.len() as f32 - 6.0,
                        q.p.y + 15.0 + 5.0 * q.links.len() as f32 + 34.0,
                    )
                })
                .collect();
            busy.sort_by(|u, v| u.0.total_cmp(&v.0));
            let mut free: Vec<(f32, f32)> = vec![];
            let mut cur = top;
            for (u, v) in busy {
                if u > cur {
                    free.push((cur, u));
                }
                cur = cur.max(v);
            }
            if bot > cur {
                free.push((cur, bot));
            }
            free.sort_by(|u, v| (v.1 - v.0).total_cmp(&(u.1 - u.0)));
            let (f0, f1) = free.first().copied().unwrap_or((top, bot));
            let seam_links: Vec<&Value> = gl
                .iter()
                .filter(|q| q.left == a.id)
                .flat_map(|q| q.links.iter().map(|&li| &ls[li]))
                .collect();
            let back = seam_links
                .iter()
                .filter(|l| inst[s(&l["from"]["subnet"])].x0 > g.p.x)
                .count();
            let fwd = seam_links.len() - back;
            let line2 = if back == 0 {
                format!("Y of {} glued to X of {}", a.id, b.id)
            } else if fwd == 0 {
                format!("Y of {} glued to X of {}", b.id, a.id)
            } else {
                "glued both ways (a loop)".to_string()
            };
            let fits = text_w(&line2, 8.0) + 12.0 < f1 - f0;
            let line = sp_(&[Pos2::new(g.p.x, top), Pos2::new(g.p.x, bot)]);
            let dash = comp::ontology()["link_style"][lk]["dash"]
                .as_str()
                .unwrap_or("")
                .to_string();
            let al = fade_for(gi).max(0.0);
            let _ = al;
            if dash.is_empty() {
                c.poly(&line, lc.gamma_multiply(0.45), wd(1.2));
            } else {
                let nums: Vec<f32> = dash.split(' ').filter_map(|x| x.parse().ok()).collect();
                c.dash_poly(
                    &line,
                    lc.gamma_multiply(0.45),
                    wd(1.2),
                    wd(nums[0]),
                    wd(*nums.get(1).unwrap_or(&nums[0])),
                );
            }
            let mid = (f0 + f1) / 2.0;
            let glyph = kind_glyph(lk);
            rot_text(
                c,
                xf,
                Pos2::new(g.p.x - 8.0, mid),
                &format!("{glyph} pushout seam"),
                8.5,
                lc,
            );
            if fits {
                rot_text(c, xf, Pos2::new(g.p.x + 6.0, mid), &line2, 8.0, lc);
            }
            let hit = Rect::from_min_max(xf.pt(g.p.x - 3.0, top), xf.pt(g.p.x + 3.0, bot));
            if c.pointer_in(hit.expand2(Vec2::new(4.0, 0.0))) {
                let mut inf = info(format!("pushout seam: {} ; {}", a.id, b.id))
                    .fact("glued wires", gl.iter().filter(|q| q.left == a.id).map(|q| q.name.clone()).collect::<Vec<_>>().join(", "))
                    .fact(
                        "links",
                        seam_links
                            .iter()
                            .map(|l| format!("{}: {}:{} → {}:{}", s(&l["kind"]), s(&l["from"]["subnet"]), s(&l["from"]["port"]), s(&l["to"]["subnet"]), s(&l["to"]["port"])))
                            .collect::<Vec<_>>()
                            .join("; "),
                    )
                    .note("Gluing the right foot of one open net to the left foot of the next identifies the places the glued points map to: the pushout of the two place sets over the shared boundary.")
                    .cite(Cites::cospan_row())
                    .cite(Cites::pushout())
                    .cite(Cites::place_classes());
                if back > 0 && fwd > 0 {
                    inf.note = format!("Some links glue {a0}'s Y to {b0}'s X and others glue {b0}'s Y back to {a0}'s X. go-pflow allows both; the places are identified all the same (one class per wire), but the composite is no longer a plain sequence {a0} ; {b0}: it closes a loop. Drawn on one seam.", a0 = a.id, b0 = b.id);
                } else if back > 0 {
                    inf.note = format!("The link runs from {b0} to {a0}; the view keeps the first link's from side on the left.", a0 = a.id, b0 = b.id);
                }
                c.hover_if(true, inf);
            }
        }
        // the glued place
        let sums: Vec<i64> = g
            .homes
            .iter()
            .map(|(sid, pid, _)| {
                arr(&inst[sid].sub["places"])
                    .iter()
                    .find(|p| s(&p["id"]) == pid)
                    .and_then(|p| p["initial"].as_i64())
                    .unwrap_or(0)
            })
            .collect();
        let total = g
            .flat_place
            .as_ref()
            .and_then(|p| p["initial"].as_i64())
            .unwrap_or_else(|| sums.iter().sum());
        let first = g.links.first().map(|&li| &ls[li]);
        let schema_port = first.and_then(|l| {
            arr(&by_id[s(&l["from"]["subnet"])]["ports"])
                .iter()
                .find(|p| p["id"] == l["from"]["port"])
        });
        let sc = schema_port.map_or(hex("#888888"), proto_port_color);
        // the merge animation of this place
        let (scale, galpha, lab_alpha, fly) = match &leak_event {
            Some(((key, kind), t))
                if matches!(kind.as_str(), "token" | "data")
                    && g.links.iter().any(|&li| s(&ls[li]["key"]) == key) =>
            {
                let t = *t;
                let (sc_, al) = if t < 0.55 {
                    (0.2, 0.0)
                } else if t < 0.82 {
                    let u = (t - 0.55) / 0.27;
                    (lerp(0.2, 1.3, u), u)
                } else {
                    (lerp(1.3, 1.0, (t - 0.82) / 0.18), 1.0)
                };
                (
                    sc_,
                    al,
                    if t < 0.6 { 0.0 } else { (t - 0.6) / 0.4 },
                    Some(t),
                )
            }
            _ => (1.0, 1.0, 1.0, None),
        };
        // the two homes fly to the seam
        if let Some(t) = fly {
            let u = (t / 0.8).min(1.0);
            let fade = if t < 0.8 {
                1.0 - 0.1 * (t / 0.8)
            } else {
                0.9 * (1.0 - (t - 0.8) / 0.2)
            };
            for (sid, pid, home) in &g.homes {
                let at = lerp2(*home, g.p, u);
                let pat = &inst[sid].sub;
                let q = arr(&pat["ports"])
                    .iter()
                    .find(|q| s(&q["target"]) == "place" && s(&q["place"]) == pid.as_str());
                let ctr = xf.pt(at.x, at.y);
                c.p.circle(
                    ctr,
                    8.0 * k,
                    Color32::WHITE.gamma_multiply(fade),
                    Stroke::new(
                        wd(1.8),
                        q.map_or(hex("#8a929c"), proto_port_color)
                            .gamma_multiply(fade),
                    ),
                );
                let init = arr(&pat["places"])
                    .iter()
                    .find(|p| s(&p["id"]) == pid)
                    .and_then(|p| p["initial"].as_i64())
                    .unwrap_or(0);
                token_dots(c, xf, at, init, 8.0, 1.0, fade);
            }
        }
        let ctr = xf.pt(g.p.x, g.p.y);
        if galpha > 0.0 {
            c.p.circle(
                ctr,
                10.5 * k * scale,
                Color32::WHITE.gamma_multiply(galpha),
                Stroke::new(wd(2.2 * scale.min(1.0)), sc.gamma_multiply(galpha)),
            );
            token_dots(c, xf, g.p, total, 10.5, scale, galpha);
        }
        let ring_r = 15.0 + 5.0 * (g.links.len().saturating_sub(1)) as f32;
        if lab_alpha > 0.0 {
            c.dt(
                xf,
                Pos2::new(g.p.x, g.p.y + ring_r + 15.0),
                Align2::CENTER_BOTTOM,
                &g.name,
                7.6,
                INK.gamma_multiply(lab_alpha),
                Some(PAPER.gamma_multiply(lab_alpha)),
                true,
            );
            c.dt(
                xf,
                Pos2::new(g.p.x, g.p.y + ring_r + 25.0),
                Align2::CENTER_BOTTOM,
                &format!(
                    "initial {} = {total}",
                    sums.iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<_>>()
                        .join(" + ")
                ),
                7.5,
                MUTED.gamma_multiply(lab_alpha),
                Some(PAPER.gamma_multiply(lab_alpha)),
                false,
            );
        }
        if galpha > 0.5 && c.pointer.is_some_and(|p| p.distance(ctr) <= 11.0 * k) {
            let mut inf = info(format!("glued place {}", g.name));
            if let Some(fp) = &g.flat_place {
                inf = inf
                    .fact("members", g.members.join(", "))
                    .fact("initial", format!("{}", total))
                    .fact("capacity", {
                        let cap = fp["capacity"].as_i64().unwrap_or(0);
                        if cap > 0 {
                            cap.to_string()
                        } else {
                            "0 (unbounded)".into()
                        }
                    })
                    .fact(
                        "fused",
                        "a wire: places fused by token/data links; initial markings summed",
                    );
            }
            inf = inf
                .note(format!("The pushout: {} are one place in the composite, named as go-pflow's Flatten names a fused class (its smallest member). The marking ({} = {total}) and the capacity (the tighter one) are Flatten's: a Baez–Master open net carries neither, so the pushout itself says nothing about them.", g.members.join(" and "), sums.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" + ")))
                .cite(Cites::place_classes())
                .cite(Cites::wire_name())
                .cite(Cites::smallest())
                .cite(Cites::initial_sum())
                .cite(Cites::pushout());
            c.hover_if(true, inf);
        }
        // half-rings, one per glued foot point, each a port
        for (n, &li) in g.links.iter().enumerate() {
            let l = &ls[li];
            let r = 15.0 + n as f32 * 5.0;
            for (end_i, end) in [&l["from"], &l["to"]].into_iter().enumerate() {
                let sid = s(&end["subnet"]);
                let i = &inst[sid];
                let Some(port) = arr(&i.sub["ports"]).iter().find(|p| p["id"] == end["port"])
                else {
                    continue;
                };
                let on_left = i.x0 < g.p.x;
                // the foot point this half-ring grew from
                let sides = port_sides(port);
                let want = if on_left { ["Y", "X"] } else { ["X", "Y"] };
                let side = want
                    .iter()
                    .copied()
                    .find(|w| sides.contains(w))
                    .unwrap_or("top");
                let start = if side == "top" {
                    geo[sid]
                        .taps
                        .iter()
                        .find(|q| q.port["id"] == port["id"])
                        .map(|q| Pos2::new(q.tx, q.ty))
                } else {
                    geo[sid]
                        .feet
                        .iter()
                        .find(|q| q.port["id"] == port["id"] && q.side == side)
                        .map(|q| Pos2::new(q.cx, q.y))
                }
                .unwrap_or(Pos2::new(
                    if on_left {
                        i.x0 + i.w + COLX
                    } else {
                        i.x0 - COLX
                    },
                    i.y0 + i.nodes.get(s(&port["place"])).map_or(0.0, |n| n.y),
                ));
                let slide = match &leak_event {
                    Some(((key, kind), t))
                        if matches!(kind.as_str(), "token" | "data") && s(&l["key"]) == key =>
                    {
                        let _ = kind;
                        1.0 - *t
                    }
                    _ => 0.0,
                };
                let off = (start - g.p) * slide;
                let cc = g.p + off;
                let pts = half_ring_pts(Pos2::new(cc.x, cc.y), r, !on_left);
                let spts = sp_(&pts);
                let hit = Rect::from_min_max(
                    xf.pt(if on_left { cc.x - r - 6.0 } else { cc.x }, cc.y - r - 4.0),
                    xf.pt(if on_left { cc.x } else { cc.x + r + 6.0 }, cc.y + r + 4.0),
                );
                let z = c.port_zone(hit, sid, port, |_| {
                    let obs = s(&port["kind"]) == "observe";
                    let mut inf = info(format!("{sid}:{}, glued ({} link)", s(&port["id"]), s(&l["kind"])));
                    inf = inf.fact("link", s(&l["key"]));
                    inf.note = format!(
                        "This half-ring is the foot point {sid}:{}; the other half is {}. The pushout identifies them, so both map to {}. go-pflow keeps the port after linking, so it stays draggable.{}{}",
                        s(&port["id"]),
                        if end_i == 0 { format!("{}:{}", s(&l["to"]["subnet"]), s(&l["to"]["port"])) } else { format!("{}:{}", s(&l["from"]["subnet"]), s(&l["from"]["port"])) },
                        g.name,
                        if s(&l["kind"]) == "data" { " A data link is read-only on the observer side (E_DATALINK_CONSUMES): drawn dotted." } else { "" },
                        if obs { " This end is an observe port: validateLink allows a token link through it (only in/out are checked)." } else { "" },
                    );
                    inf.cites = vec![Cites::pushout(), Cites::place_classes()];
                    inf
                });
                let col = proto_port_color(port).gamma_multiply(if z.dim { 0.3 } else { 1.0 });
                if s(&l["kind"]) == "data" && end_i == 1 {
                    c.dash_poly(&spts, col, wd(3.2), wd(2.0), wd(3.0));
                } else {
                    c.poly(&spts, col, wd(3.2));
                }
                c.zone_ring_pts(&spts, &z, wd(3.2));
            }
            c.scene.emit(
                "link",
                s(&l["key"]),
                json!({"kind": s(&l["kind"]), "style": "pushout"}),
            );
        }
        let _ = gi;
    }

    // 7. ⊗ between components
    for row in &rows {
        for ci in 1..row.comps.len() {
            let prev = &inst[row.comps[ci - 1].last().unwrap()];
            let next = &inst[&row.comps[ci][0]];
            let x = (prev.x0 + prev.w + next.x0) / 2.0;
            let y = row.y0 + (FH - BAND) / 2.0;
            let ctr = xf.pt(x, y);
            c.p.circle(ctr, 14.0 * k, PAPER, Stroke::new(wd(1.0), hex("#c5ccd5")));
            c.dt(
                xf,
                Pos2::new(x, y + 1.0),
                Align2::CENTER_CENTER,
                "⊗",
                19.0,
                hex("#7a828c"),
                None,
                false,
            );
            if c.pointer.is_some_and(|p| p.distance(ctr) <= 14.0 * k) {
                let inf = info("monoidal product ⊗")
                    .fact("left", row.comps[ci - 1].join(" ; "))
                    .fact("right", row.comps[ci].join(" ; "))
                    .note("Side by side with no shared place: the disjoint union of the two open nets (feet and places). As a cospan composite this is all there is until a token or data link glues a foot. Event links between these instances do change the go-pflow composite (Flatten fuses the two transitions), but not by gluing places: transition fusion is not a cospan composite, so it is drawn as a ghost below the frames, not here.")
                    .cite(Cites::tensor())
                    .cite(Cites::event());
                c.hover_if(true, inf);
            }
        }
    }

    // 8. ghosts: event and guard links
    let ghost_info = |l: &Value| -> Info {
        let mut inf = link_info(l);
        if s(&l["kind"]) == "event" {
            inf.note = "Event link: transition fusion, not a pushout. A Baez–Master cospan has places on its feet, so fusing two transitions is not a cospan composite; it is the composition of nets with boundaries (Bruni et al.). Drawn as a ghost: real in go-pflow (it fuses the two transitions), not this convention's.".into();
            let mut cites = vec![Cites::event(), Cites::boundaries_row()];
            cites.append(&mut inf.cites);
            inf.cites = cites;
        } else {
            inf.note = if s(&l["kind"]) == "guard" {
                "Guard link: a cross-subnet read arc (W_RESTRICTIVE_LINK). Contextual: outside the free-SMC fragment, so no cospan or functorial semantics is claimed for it.".into()
            } else {
                "A token or data link that Flatten refused: no pushout is drawn, so it is shown as a ghost.".into()
            };
            let mut cites = vec![Cites::guard()];
            cites.append(&mut inf.cites);
            inf.cites = cites;
        }
        inf
    };
    let mut same_row: BTreeMap<usize, Vec<Ghost>> = BTreeMap::new();
    let mut cross_row: Vec<Ghost> = vec![];
    for l in &ghosts {
        let find = |end: &Value, prefer: &[&str]| -> Option<(Pos2, &'static str)> {
            for side in prefer {
                let key = (
                    s(&end["subnet"]).to_string(),
                    s(&end["port"]).to_string(),
                    side.to_string(),
                );
                if let Some(p) = port_at.get(&key) {
                    let name: &'static str = match *side {
                        "bottom" => "bottom",
                        "top" => "top",
                        "X" => "X",
                        _ => "Y",
                    };
                    return Some((*p, name));
                }
            }
            None
        };
        let a = find(&l["from"], &["bottom"]).or_else(|| find(&l["from"], &["Y", "X", "top"]));
        let b = if s(&l["kind"]) == "event" {
            find(&l["to"], &["bottom"])
        } else {
            find(&l["to"], &["top", "X", "Y"])
        };
        let (Some(a), Some(b)) = (a, b) else { continue };
        let (ra, rb) = (
            inst[s(&l["from"]["subnet"])].row,
            inst[s(&l["to"]["subnet"])].row,
        );
        let g = Ghost {
            l: l.clone(),
            a,
            b,
            span: (a.0.x - b.0.x).abs(),
        };
        if ra != rb {
            cross_row.push(g);
        } else {
            same_row.entry(ra).or_default().push(g);
        }
    }
    struct Stub {
        li: usize,
        end_i: usize,
        row: usize,
        gx: Option<f32>,
        sx: f32,
        lane: bool,
    }
    let mut stubs: Vec<Stub> = vec![];
    for (gi, g) in cross_row.iter().enumerate() {
        for (end_i, (p, end)) in [(g.a, &g.l["from"]), (g.b, &g.l["to"])]
            .into_iter()
            .enumerate()
        {
            let t = &inst[s(&end["subnet"])];
            let gx = if p.1 == "top" {
                Some(if p.0.x < t.x0 + t.w / 2.0 {
                    t.x0 - COLX - 13.0
                } else {
                    t.x0 + t.w + COLX + 13.0
                })
            } else {
                None
            };
            stubs.push(Stub {
                li: gi,
                end_i,
                row: t.row,
                gx,
                sx: gx.unwrap_or(p.0.x),
                lane: p.1 == "bottom" || p.1 == "top",
            });
        }
    }
    let mut stub_len: BTreeMap<(usize, usize), f32> = BTreeMap::new();
    for r in 0..rows.len() {
        let mut lane: Vec<&Stub> = stubs.iter().filter(|x| x.row == r && x.lane).collect();
        lane.sort_by(|u, v| {
            v.sx.total_cmp(&u.sx).then(
                cross_row[u.li].l["index"]
                    .as_i64()
                    .cmp(&cross_row[v.li].l["index"].as_i64()),
            )
        });
        for (j, x) in lane.iter().enumerate() {
            stub_len.insert((x.li, x.end_i), 30.0 + 22.0 * j as f32);
        }
        for (j, x) in stubs.iter().filter(|x| x.row == r && !x.lane).enumerate() {
            stub_len.insert((x.li, x.end_i), 30.0 + 14.0 * j as f32);
        }
    }
    let mut lane_deep: BTreeMap<usize, f32> = BTreeMap::new();
    let mut avoid: Vec<Vec<Rect>> = rows.iter().map(|_| vec![]).collect();
    let glyph_rect =
        |x: f32, y: f32| Rect::from_min_size(Pos2::new(x - 8.5, y - 8.5), Vec2::splat(17.0));
    for sub in &subs {
        let sid = s(&sub["id"]);
        let i = &inst[sid];
        avoid[i.row].push(Rect::from_min_size(
            Pos2::new(i.x0 - COLX - 4.0, i.y0 - 37.0),
            Vec2::new(i.w + 2.0 * COLX + 8.0, 15.0),
        ));
        avoid[i.row].push(Rect::from_min_size(
            Pos2::new(i.x0 - COLX - 8.0, i.y0 - 19.0),
            Vec2::new(16.0, 14.0),
        ));
        avoid[i.row].push(Rect::from_min_size(
            Pos2::new(i.x0 + i.w + COLX - 8.0, i.y0 - 19.0),
            Vec2::new(16.0, 14.0),
        ));
        for tp in &geo[sid].taps {
            avoid[i.row].push(text_rect(
                tp.tx,
                tp.ty - 9.0,
                &format!("{} (observe)", s(&tp.port["id"])),
                FS,
                "middle",
            ));
        }
    }
    let inside = |r: Rect| r.min.x >= 2.0 && r.max.x <= w_total - 2.0;
    // ghost anim for the link just added
    let ghost_anim = |l: &Value| -> Option<f32> {
        match &leak_event {
            Some(((key, kind), t))
                if !matches!(kind.as_str(), "token" | "data") && s(&l["key"]) == key =>
            {
                Some(*t)
            }
            _ => None,
        }
    };
    let draw_glyph = |c: &mut Ctx, l: &Value, x: f32, y: f32, anim: Option<f32>| {
        let col = kind_color(s(&l["kind"]));
        let (scale, alpha) = match anim {
            Some(t) => (
                if t < 0.7 {
                    1.7 * (t / 0.7)
                } else {
                    lerp(1.7, 1.0, (t - 0.7) / 0.3)
                },
                1.0,
            ),
            None => (1.0, 1.0),
        };
        let ctr = xf.pt(x, y);
        let leak = l["leak"] == true;
        c.p.circle(
            ctr,
            7.5 * k * scale,
            PAPER.gamma_multiply(alpha),
            Stroke::new(
                wd(if leak { 1.8 } else { 1.2 }),
                if leak { LEAK } else { col },
            ),
        );
        c.dt(
            xf,
            Pos2::new(x, y),
            Align2::CENTER_CENTER,
            kind_glyph(s(&l["kind"])),
            10.5 * scale,
            col,
            None,
            false,
        );
        ctr
    };
    let ghost_stroke = |l: &Value, anim: Option<f32>| -> (Color32, f32) {
        let col = kind_color(s(&l["kind"]));
        let a = 0.6 * anim.unwrap_or(1.0);
        (col.gamma_multiply(a), wd(1.5))
    };
    for g in &cross_row {
        let l = &g.l;
        let an = ghost_anim(l);
        let (col, w_) = ghost_stroke(l, an);
        let gi = cross_row.iter().position(|x| std::ptr::eq(x, g)).unwrap();
        let mut hit_poly: Vec<Vec<Pos2>> = vec![];
        let mut glyph_at: Vec<Pos2> = vec![];
        for (end_i, (p, end, other)) in [(g.a, &l["from"], &l["to"]), (g.b, &l["to"], &l["from"])]
            .into_iter()
            .enumerate()
        {
            let t = &inst[s(&end["subnet"])];
            let len = stub_len.get(&(gi, end_i)).copied().unwrap_or(30.0);
            let (pts, x2, y2) = if p.1 == "top" {
                let gx = stubs
                    .iter()
                    .find(|x| x.li == gi && x.end_i == end_i)
                    .and_then(|x| x.gx)
                    .unwrap_or(p.0.x);
                let y2 = t.y0 + FH + len;
                (
                    rounded_pts(
                        &[
                            Pos2::new(p.0.x, p.0.y - 5.5),
                            Pos2::new(p.0.x, t.y0 - 3.0),
                            Pos2::new(gx, t.y0 - 3.0),
                            Pos2::new(gx, y2),
                        ],
                        9.0,
                    ),
                    gx,
                    y2,
                )
            } else {
                let dir = match p.1 {
                    "bottom" => (0.0, 1.0),
                    "X" => (-1.0, 0.0),
                    _ => (1.0, 0.0),
                };
                let (x2, y2) = (p.0.x + dir.0 * len, p.0.y + dir.1 * len);
                (
                    vec![
                        Pos2::new(p.0.x + dir.0 * 6.0, p.0.y + dir.1 * 6.0),
                        Pos2::new(x2, y2),
                    ],
                    x2,
                    y2,
                )
            };
            let spts = sp_(&pts);
            if s(&l["kind"]) == "event" {
                c.dash_poly(&spts, col, w_, wd(5.0), wd(4.0));
            } else {
                c.dash_poly(&spts, col, w_, wd(6.0), wd(4.0));
            }
            hit_poly.push(spts);
            draw_glyph(c, l, x2, y2, an);
            glyph_at.push(Pos2::new(x2, y2));
            let (ri, rs) = (inst[s(&other["subnet"])].row, t.row);
            let where_ = if ri > rs { "row below" } else { "row above" };
            let label = format!(
                "{} link with {}:{} ({where_}){}",
                s(&l["kind"]),
                s(&other["subnet"]),
                s(&other["port"]),
                if l["leak"] == true {
                    ": pool leaks a worker on rollback"
                } else {
                    ""
                }
            );
            let away: Vec<(f32, f32, &str)> = if p.1 == "X" {
                vec![(-11.0, 3.2, "end"), (11.0, 3.2, "start")]
            } else {
                vec![(11.0, 3.2, "start"), (-11.0, 3.2, "end")]
            };
            let mut cands: Vec<(f32, f32, &str, Rect)> = away
                .into_iter()
                .chain([(0.0, 20.0, "middle")])
                .map(|(dx, dy, a)| {
                    (
                        x2 + dx,
                        y2 + dy,
                        a,
                        text_rect(x2 + dx, y2 + dy, &label, 8.5, a),
                    )
                })
                .collect();
            avoid[rs].push(glyph_rect(x2, y2));
            let pick = cands
                .iter()
                .position(|c_| inside(c_.3) && !avoid[rs].iter().any(|q| c_.3.intersects(*q)))
                .or_else(|| cands.iter().position(|c_| inside(c_.3)))
                .unwrap_or(0);
            let (px_, py_, pa, pr) = cands.swap_remove(pick);
            dtb(
                c,
                Pos2::new(px_, py_),
                pa,
                &label,
                8.5,
                if l["leak"] == true {
                    LEAK
                } else {
                    kind_color(s(&l["kind"]))
                },
                PAPER,
                false,
            );
            avoid[rs].push(pr);
            if p.1 == "bottom" || p.1 == "top" {
                let e = lane_deep.entry(rs).or_insert(rows[rs].y0 + FH + 40.0);
                *e = e.max(y2);
            }
        }
        for pts in &hit_poly {
            if c.near_poly(pts, 5.0) {
                c.hover_if(true, ghost_info(l));
            }
        }
        for ga in &glyph_at {
            if c.pointer
                .is_some_and(|p| p.distance(xf.pt(ga.x, ga.y)) <= 9.0 * k)
            {
                c.hover_if(true, ghost_info(l));
            }
        }
        c.scene.emit(
            "link",
            s(&l["key"]),
            json!({"kind": s(&l["kind"]), "style": style_of(s(&l["kind"]))}),
        );
    }
    for (r, list) in same_row.iter_mut() {
        list.sort_by(|p, q| {
            (s(&p.l["kind"]) == "guard")
                .cmp(&(s(&q.l["kind"]) == "guard"))
                .then(p.span.total_cmp(&q.span))
                .then(p.l["index"].as_i64().cmp(&q.l["index"].as_i64()))
        });
        let e = rows[*r].y0 + FH;
        let below = stubs
            .iter()
            .filter(|x| x.row == *r && x.lane)
            .filter_map(|x| stub_len.get(&(x.li, x.end_i)).copied())
            .fold(0.0f32, f32::max);
        let base = if below > 0.0 {
            (below + 26.0) / 0.75
        } else {
            36.0
        };
        struct Drawn {
            li: usize,
            pts: Vec<Pos2>,
            mx: f32,
            my: f32,
        }
        let mut drawn: Vec<Drawn> = vec![];
        for (kk, g) in list.iter().enumerate() {
            let (a, b) = (g.a, g.b);
            let dd = base + 17.0 * kk as f32;
            let (pts, mx, my);
            if s(&g.l["kind"]) == "event" {
                pts = cubic_pts(
                    Pos2::new(a.0.x, e + 6.0),
                    Pos2::new(a.0.x, e + 6.0 + dd),
                    Pos2::new(b.0.x, e + 6.0 + dd),
                    Pos2::new(b.0.x, b.0.y + 6.0),
                    24,
                );
                mx = (a.0.x + b.0.x) / 2.0;
                my = e + 6.0 + 0.75 * dd;
            } else {
                let t = &inst[s(&g.l["to"]["subnet"])];
                let left = t.x0 - COLX - 13.0;
                let right = t.x0 + t.w + COLX + 13.0;
                let gx = match b.1 {
                    "X" => left,
                    "Y" => right,
                    _ => {
                        if a.0.x < t.x0 + t.w / 2.0 {
                            left
                        } else {
                            right
                        }
                    }
                };
                let ly = e + 6.0 + 0.75 * dd;
                let corners = if b.1 == "top" {
                    vec![
                        Pos2::new(a.0.x, e + 6.0),
                        Pos2::new(a.0.x, ly),
                        Pos2::new(gx, ly),
                        Pos2::new(gx, t.y0 - 3.0),
                        Pos2::new(b.0.x, t.y0 - 3.0),
                        Pos2::new(b.0.x, b.0.y - 5.5),
                    ]
                } else {
                    vec![
                        Pos2::new(a.0.x, e + 6.0),
                        Pos2::new(a.0.x, ly),
                        Pos2::new(gx, ly),
                        Pos2::new(gx, b.0.y),
                        Pos2::new(b.0.x + if b.1 == "X" { -6.0 } else { 6.0 }, b.0.y),
                    ]
                };
                pts = rounded_pts(&corners, 9.0);
                mx = (a.0.x + gx) / 2.0;
                my = ly;
            }
            avoid[*r].push(glyph_rect(mx, my));
            drawn.push(Drawn {
                li: kk,
                pts,
                mx,
                my,
            });
        }
        for d in drawn {
            let l = list[d.li].l.clone();
            let an = ghost_anim(&l);
            let (col, w_) = ghost_stroke(&l, an);
            let spts = sp_(&d.pts);
            if s(&l["kind"]) == "event" {
                c.dash_poly(&spts, col, w_, wd(5.0), wd(4.0));
            } else {
                c.dash_poly(&spts, col, w_, wd(6.0), wd(4.0));
            }
            let ctr = draw_glyph(c, &l, d.mx, d.my, an);
            let mut deepest = d.my;
            let caption: Option<(String, Color32, f32)> = if s(&l["kind"]) == "guard" {
                Some((
                    "⊘ guard link: contextual, outside the free SMC".into(),
                    kind_color("guard"),
                    8.5,
                ))
            } else if l["leak"] == true {
                Some((
                    "pool leaks a worker on rollback (idiom caveat)".into(),
                    LEAK,
                    8.0,
                ))
            } else {
                None
            };
            if let Some((t, col, size)) = caption {
                let cands: Vec<(f32, Rect)> = [19.0f32, -12.0, 31.0, 43.0]
                    .iter()
                    .map(|dy| (d.my + dy, text_rect(d.mx, d.my + dy, &t, size, "middle")))
                    .collect();
                let pick = cands
                    .iter()
                    .find(|c_| !avoid[*r].iter().any(|q| c_.1.intersects(*q)))
                    .copied()
                    .unwrap_or(cands[0]);
                avoid[*r].push(pick.1);
                deepest = deepest.max(pick.0);
                dtb(
                    c,
                    Pos2::new(d.mx, pick.0),
                    "middle",
                    &t,
                    size,
                    col,
                    PAPER,
                    false,
                );
            }
            let e = lane_deep.entry(*r).or_insert(e + 40.0);
            *e = e.max(if deepest - 19.0 > d.my {
                deepest - 19.0
            } else {
                d.my
            });
            if c.near_poly(&spts, 5.0) || c.pointer.is_some_and(|p| p.distance(ctr) <= 9.0 * k) {
                c.hover_if(true, ghost_info(&l));
            }
            c.scene.emit(
                "link",
                s(&l["key"]),
                json!({"kind": s(&l["kind"]), "style": style_of(s(&l["kind"]))}),
            );
        }
    }
    for (r, row) in rows.iter().enumerate() {
        let has_event = ghosts.iter().any(|l| {
            s(&l["kind"]) == "event"
                && (inst[s(&l["from"]["subnet"])].row == r || inst[s(&l["to"]["subnet"])].row == r)
        });
        if !has_event {
            continue;
        }
        let e = row.y0 + FH;
        let y = (lane_deep.get(&r).copied().unwrap_or(e + 40.0) + 36.0).min(e + LANE - 6.0);
        dtb(c, Pos2::new(row.x - COLX, y), "start", "⊗ event link: transition fusion, not a pushout (Bruni et al.). Drawn as a ghost: real in go-pflow, not a cospan composite.", 9.5, kind_color("event"), PAPER, false);
    }

    // 9. footer: the composite and the legend
    let fy = h_total - FOOT + 26.0;
    let loops = |ids: &[String]| {
        glue.iter().any(|l| {
            let a = ids.iter().position(|x| x == s(&l["from"]["subnet"]));
            let b = ids.iter().position(|x| x == s(&l["to"]["subnet"]));
            matches!((a, b), (Some(a), Some(b)) if a > b)
        })
    };
    let comp_expr = rows
        .iter()
        .flat_map(|r| r.comps.iter())
        .map(|ids| {
            if ids.len() > 1 {
                format!(
                    "({}{})",
                    ids.join(" ; "),
                    if loops(ids) { ", glued both ways" } else { "" }
                )
            } else {
                ids[0].clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ⊗ ");
    let comp_title = format!("Cospan composite: {comp_expr}");
    let tr = c.dt(
        xf,
        Pos2::new(SIDE - 22.0, fy),
        Align2::LEFT_BOTTOM,
        &comp_title,
        12.5,
        INK,
        None,
        true,
    );
    let n_event = ghosts.iter().filter(|l| s(&l["kind"]) == "event").count();
    let n_guard = ghosts.len() - n_event;
    if !ghosts.is_empty() {
        let parts = [
            (n_event > 0).then(|| {
                format!(
                    "{n_event} event fusion{}",
                    if n_event == 1 { "" } else { "s" }
                )
            }),
            (n_guard > 0).then(|| {
                format!(
                    "{n_guard} guard read arc{}",
                    if n_guard == 1 { "" } else { "s" }
                )
            }),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        let sfx = format!(
            "+ {parts} ({})",
            if ghosts.len() == 1 {
                "the ghost; not a cospan composite"
            } else {
                "the ghosts; not cospan composites"
            }
        );
        let r = c.dt(
            xf,
            Pos2::new((tr.max.x - xf.origin.x) / k + 14.0, fy),
            Align2::LEFT_BOTTOM,
            &sfx,
            10.5,
            if n_event > 0 {
                kind_color("event")
            } else {
                kind_color("guard")
            },
            None,
            false,
        );
        if c.pointer_in(r) {
            c.hover_if(true, info("event and guard links in the composite")
                .note("go-pflow's composite is the cospan composite plus these: Flatten fuses each event-linked pair of transitions into one (the composition of nets with boundaries, Bruni et al.) and lowers each guard link to a read arc across subnets (contextual, outside the free SMC). Neither glues places, so neither is a cospan composite.")
                .cite(Cites::model()).cite(Cites::event()).cite(Cites::guard()).cite(Cites::boundaries_row()));
        }
    }
    let extras: Vec<String> = subs
        .iter()
        .filter(|x| x["extra"] == true)
        .map(|x| s(&x["id"]).to_string())
        .collect();
    if glue.is_empty() {
        c.dt(xf, Pos2::new(SIDE - 22.0, fy + 18.0), Align2::LEFT_BOTTOM,
            &format!("No token or data links: as a cospan composite this is only the monoidal product ⊗ (disjoint union); no pushout.{}{}",
                if n_event > 0 { " The event links fuse transitions on top." } else { "" },
                if n_guard > 0 { " Guard links add read arcs." } else { "" }),
            10.5, INK, None, false);
        c.dt(xf, Pos2::new(SIDE - 22.0, fy + 33.0), Align2::LEFT_BOTTOM,
            &format!("cq/13 finds no token candidates among queue, pool and saga: their place schemas differ (queue:items, resource, account, saga:outcome). {}",
                if extras.is_empty() { "Add a second instance to glue two feet." } else { "A second instance is here: a token link to its same-schema twin glues two feet." }),
            10.5, MUTED, None, false);
    } else {
        c.dt(xf, Pos2::new(SIDE - 22.0, fy + 18.0), Align2::LEFT_BOTTOM, "; is the cospan composite (the pushout along the glued foot: go-pflow lk:token / lk:data). ⊗ is the monoidal product (disjoint union).", 10.5, INK, None, false);
        c.dt(xf, Pos2::new(SIDE - 22.0, fy + 33.0), Align2::LEFT_BOTTOM, "The glued place is named and marked as go-pflow's Flatten does it: wire:<smallest member>, initial markings summed (Flatten's choice: an open net carries no marking).", 10.5, MUTED, None, false);
    }
    legend(c, xf, SIDE - 22.0, fy + 62.0);

    // manifest
    for sub in &subs {
        let sid = s(&sub["id"]);
        c.scene
            .emit("subnet", sid, json!({"pattern": s(&sub["pattern"])}));
        for p in arr(&sub["ports"]) {
            c.scene.emit(
                "port",
                &format!("{sid}:{}", s(&p["id"])),
                json!({"side": side_of(s(&p["kind"]), s(&p["target"])), "target": s(&p["target"])}),
            );
        }
    }
    let form = if glue.is_empty() { "tensor" } else { "pushout" };
    c.scene.emit("composite", "bundle", json!({"form": form}));

    if glue.is_empty() {
        c.note("honesty", format!("Only lk:token and lk:data are cospan composites: a Baez–Master open net has places on its feet, and go-pflow fuses the two places into one (Flatten's place classes). So as a cospan composite the instances are only {comp_expr}{}. Among the three default instances nothing can be glued: cq/13 finds no token candidates (the place schemas differ), and dragging from their place ports lights no place port (only transition ports, for a guard link). {} Event links fuse transitions (Bruni et al.) and guard links are contextual, outside the free SMC: both are drawn as ghosts.",
            if n_event > 0 { "; the event links fuse transitions on top of that" } else { "" },
            if extras.is_empty() { "Add + payment2, + orders2 or + workers2 and drag between same-schema place ports to see the pushout.".to_string() } else { format!("A second instance is on the page ({}): drag from one of its place ports to the same port of its twin (for example payment:account:customer to payment2:account:customer) to see the pushout.", extras.join(", ")) }));
    } else {
        c.note("honesty", "The pushout is drawn where a token or data link glues two feet: the two instances meet at a seam and the two places become one, named and marked as go-pflow's Flatten does (wire:<smallest member>, initial markings summed). go-pflow keeps every port after linking, so the glued points stay draggable (the two half-rings). Event links (transition fusion, Bruni et al.) and guard links (contextual, outside the free SMC) are ghosts, not composites.");
    }
    c.note("honesty", "A Baez–Master open net has places, transitions and arcs with multiplicities, and no capacities or markings. go-pflow places carry both (the queue's and the pool's places are bounded). The cospan reading covers the structure only: summing the markings and keeping the tighter capacity of a glued pair are Flatten's choices (compose_flatten.go:291-292), not part of the pushout.");
    c.note("honesty", "go-pflow keeps every port after a link, and a link may glue any compatible pair of ports, not only a whole foot onto a whole foot as in Baez–Master. The view draws each link as one glued pair of points; the unglued points stay on the composite's feet.");
    c.note("honesty", "Observe ports are drawn as taps in neither foot (the ROADMAP's reading). go-pflow's validateLink checks only in/out for a token link, so a token link through an observe port is allowed; the view draws it and says so.");
    c.note("honesty", "Layout is this view's: the positions of places and transitions inside a frame, and which instance goes left of which in a glued pair (the first link's from side), are drawing choices.");
    c.note("judgment", comp::PARAMS_NOTE);
}

fn style_of(kind: &str) -> &'static str {
    match kind {
        "token" | "data" => "pushout",
        "event" => "ghost",
        _ => "guard",
    }
}

/// Text rotated a quarter turn counter-clockwise, centred at `at`
/// (design coordinates): the seam's label.
fn rot_text(c: &Ctx, xf: Xf, at: Pos2, t: &str, size: f32, col: Color32) {
    let fs = (size * xf.k).max(7.0);
    let galley =
        c.p.layout_no_wrap(t.to_string(), FontId::proportional(fs), col);
    let ctr = xf.pt(at.x, at.y);
    let (w, h) = (galley.size().x, galley.size().y);
    let pos = Pos2::new(ctr.x - h / 2.0, ctr.y + w / 2.0);
    c.p.add(
        egui::epaint::TextShape::new(pos, galley, col).with_angle(-std::f32::consts::FRAC_PI_2),
    );
}

fn foot_info(sub: &Value, port: &Value, side: &str) -> Info {
    let sid = s(&sub["id"]);
    let mut i = info(format!("port {sid}:{}", s(&port["id"])))
        .fact("kind", s(&port["kind"]))
        .fact("target", "place")
        .fact("place", s(&port["place"]))
        .fact("schema", s(&port["schema"]))
        .fact("ontology", s(&port["pattern_port"]))
        .fact(
            "foot",
            if side == "X" {
                "X, the left foot (domain): in and inout ports"
            } else {
                "Y, the right foot (codomain): out and inout ports"
            },
        )
        .fact(
            "leg",
            format!("{} ↦ {sid}/{}", s(&port["id"]), s(&port["place"])),
        );
    if s(&port["kind"]) == "inout" {
        i = i.note("An inout port is in both feet, so it is drawn on both sides (ROADMAP Phase 6); both points are this one port.");
    }
    if let Some(pp) = port_concept_of(sub, s(&port["pattern_port"])) {
        if i.note.is_empty() {
            i = i.note(s(&pp["comment"]));
        }
        i = i.cites_of(&pp["cites"]);
    }
    i.cite(Cites::sides())
}

fn tap_info(sub: &Value, port: &Value) -> Info {
    let sid = s(&sub["id"]);
    let mut i = info(format!("port {sid}:{}", s(&port["id"])))
        .fact("kind", s(&port["kind"]))
        .fact("place", s(&port["place"]))
        .fact("schema", s(&port["schema"]))
        .fact("ontology", s(&port["pattern_port"]))
        .fact("drawn as", "a tap on the top edge, in neither foot: read-only by the ontology's reading (data or guard links)")
        .note("An observe port is a side tap (ROADMAP Phase 6), not a foot. Note: go-pflow's validateLink checks only in/out for token links, so a token link through this port is allowed; the ontology describes it for data or guard links.");
    if let Some(pp) = port_concept_of(sub, s(&port["pattern_port"])) {
        i = i.cites_of(&pp["cites"]);
    }
    i.cite(Cites::sides())
        .cite(Cites::depth_port())
        .cite(Cites::direction())
}

fn port_concept_of(sub: &Value, key: &str) -> Option<Value> {
    let pat = &comp::ontology()["patterns"][s(&sub["pattern"])];
    arr(&pat["ports"])
        .iter()
        .chain(arr(&pat["portTemplates"]))
        .find(|p| s(&p["key"]) == key)
        .cloned()
}

fn place_info(sub: &Value, pl: &Value) -> Info {
    let sid = s(&sub["id"]);
    let pid = s(&pl["id"]);
    let ports: Vec<&str> = arr(&sub["ports"])
        .iter()
        .filter(|q| s(&q["place"]) == pid)
        .map(|q| s(&q["id"]))
        .collect();
    let cap = pl["capacity"].as_i64().unwrap_or(0);
    let mut i = info(format!("place {sid}/{pid}"))
        .fact("initial", pl["initial"].to_string())
        .fact(
            "capacity",
            if cap > 0 {
                cap.to_string()
            } else {
                "0 (unbounded)".into()
            },
        )
        .fact(
            "exported",
            if pl["exported"] == true {
                format!(
                    "yes (ports: {})",
                    if ports.is_empty() {
                        "none".into()
                    } else {
                        ports.join(", ")
                    }
                )
            } else {
                "no: inside the boundary".into()
            },
        )
        .fact(
            "in this drawing",
            if pl["exported"] == true {
                "a leg target: a foot point maps here"
            } else {
                "interior: no foot point maps here"
            },
        )
        .note(s(&pl["description"]).to_string());
    i = i.cites_of(&comp::ontology()["patterns"][s(&sub["pattern"])]["cites"]);
    i
}

/// The legend and the wire-type key under the composite.
fn legend(c: &mut Ctx, xf: Xf, x0: f32, ly: f32) {
    let k = xf.k;
    let mut lx = x0;
    let mut item = |c: &mut Ctx, draw: &dyn Fn(&Ctx, f32, f32), label: &str, i: Info| {
        draw(c, lx, ly);
        let r = c.dt(
            xf,
            Pos2::new(lx + 18.0, ly + 3.5),
            Align2::LEFT_BOTTOM,
            label,
            9.5,
            INK,
            None,
            false,
        );
        let hit = Rect::from_min_max(
            xf.pt(lx - 2.0, ly - 8.0),
            pos2_max(r.max, xf.pt(lx + 18.0, ly + 8.0)),
        );
        if c.pointer_in(hit) {
            c.hover_if(true, i);
        }
        lx += 18.0 + text_w(label, 9.5) + 20.0;
    };
    type LegendItem<'x> = (&'x str, Box<dyn Fn(&Ctx, f32, f32)>, Info);
    let items: Vec<LegendItem> = vec![
        (
            "foot point (X or Y), by schema",
            Box::new(move |c: &Ctx, x, y| {
                c.p.circle(xf.pt(x + 6.0, y), 5.0 * k, proto_schema_color("queue:items"), Stroke::new((1.5 * k).max(0.7), Color32::WHITE));
            }),
            info("foot point").note("A boundary point of the cospan: a place port, colored by its schema (the wire type).").cite(Cites::sides()),
        ),
        (
            "leg",
            Box::new(move |c: &Ctx, x, y| {
                c.dash_poly(&[xf.pt(x, y), xf.pt(x + 13.0, y)], LEG, (1.0 * k).max(0.7), 4.0 * k, 3.0 * k);
            }),
            info("leg").note("The function from a foot to the places: each point maps to the place its port names."),
        ),
        (
            "observe tap (top edge)",
            Box::new(move |c: &Ctx, x, y| {
                let col = proto_schema_color("queue:items");
                let ctr = xf.pt(x + 6.0, y);
                c.p.circle(ctr, 5.0 * k, Color32::WHITE, Stroke::new((1.8 * k).max(0.7), col));
                c.p.circle_filled(ctr, 1.6 * k, col);
            }),
            info("observe tap").note("An observe port: a side tap, in neither foot.").cite(Cites::sides()).cite(Cites::depth_port()),
        ),
        (
            "event port (bottom edge): not part of a cospan",
            Box::new(move |c: &Ctx, x, y| {
                c.p.rect_filled(xf.rect(x + 1.5, y - 4.5, 9.0, 9.0), 1.0, NODE);
            }),
            info("event port").note("A transition port. Not a leg: event links compose these by fusion (nets with boundaries).").cite(Cites::boundaries_row()),
        ),
        (
            "glued place (pushout); half-rings: the glued points",
            Box::new(move |c: &Ctx, x, y| {
                let col = proto_schema_color("account");
                let ctr = xf.pt(x + 6.0, y);
                c.p.circle(ctr, 6.0 * k, Color32::WHITE, Stroke::new((1.8 * k).max(0.7), col));
                c.poly(&half_ring_pts(ctr, 9.0 * k, false), col, (2.5 * k).max(0.7));
            }),
            info("glued place").note("Two places identified by a token or data link: the pushout. The two half-rings are the two glued foot points.").cite(Cites::pushout()).cite(Cites::place_classes()).cite(Cites::wire_name()).cite(Cites::smallest()),
        ),
    ];
    for (label, draw, i) in items {
        item(c, &*draw, label, i);
    }
    let ly2 = ly + 22.0;
    let mut sx = x0;
    c.dt(
        xf,
        Pos2::new(sx, ly2 + 3.5),
        Align2::LEFT_BOTTOM,
        "Wire types (port schemas):",
        9.5,
        MUTED,
        None,
        false,
    );
    sx += text_w("Wire types (port schemas):", 9.5) + 12.0;
    for sch in ["queue:items", "resource", "account", "saga:outcome"] {
        c.p.circle(
            xf.pt(sx + 5.0, ly2),
            5.0 * k,
            proto_schema_color(sch),
            Stroke::new((1.5 * k).max(0.7), Color32::WHITE),
        );
        let r = c.dt(
            xf,
            Pos2::new(sx + 14.0, ly2 + 3.5),
            Align2::LEFT_BOTTOM,
            sch,
            9.5,
            INK,
            None,
            true,
        );
        let hit = Rect::from_min_max(
            xf.pt(sx, ly2 - 7.0),
            pos2_max(r.max, xf.pt(sx + 14.0, ly2 + 7.0)),
        );
        if c.pointer_in(hit) {
            c.hover_if(true, info(format!("wire type {sch}")).note("A port schema. A token or data link between two ports that both carry one needs them to agree (E_SCHEMA_MISMATCH otherwise).").cite(Cites::sides()).cite(Cites::schema()).cite(Cites::cq13()));
        }
        sx += 14.0 + text_w(sch, 9.5) + 18.0;
    }
    c.dt(
        xf,
        Pos2::new(sx + 10.0, ly2 + 3.5),
        Align2::LEFT_BOTTOM,
        "Numbers by each title: |X| → |S| ← |Y| (taps are in neither foot).",
        9.5,
        MUTED,
        None,
        false,
    );
}

fn pos2_max(a: Pos2, b: Pos2) -> Pos2 {
    Pos2::new(a.x.max(b.x), a.y.max(b.y))
}

// ------------------------------------------------------------ connect-4

pub(super) fn cospan_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, ar) = net_counts(c);
    let plan =
        c.d.plan
            .as_ref()
            .map(|p| p["plan"].clone())
            .unwrap_or(Value::Null);
    let w = 1150.0f32;
    let h = 800.0f32;
    let xf = Xf::fit(area, w, h, 1.7);
    let k = xf.k;
    let wd = |x: f32| (x * k).max(0.7);
    let sp_ = |pts: &[Pos2]| -> Vec<Pos2> { pts.iter().map(|p| xf.pt(p.x, p.y)).collect() };
    c.dt(xf, Pos2::new(18.0, 20.0), Align2::LEFT_BOTTOM, "Connect-4 as an open net: the closed net ∅ → N ← ∅ (both feet empty), the net a search plan runs over.", 12.5, INK, None, false);
    c.dt(xf, Pos2::new(18.0, 37.0), Align2::LEFT_BOTTOM, "Any Petri net is an open net with both feet empty. Nothing on its boundary can be glued: no place is exposed. Places as dots, transitions as squares, at the fixture's own x/y.", 10.5, MUTED, None, false);
    let (fx, fy, fw, fh) = (140.0f32, 92.0f32, 870.0f32, 452.0f32);
    let frame = xf.rect(fx, fy, fw, fh);
    c.p.rect(
        frame,
        10.0 * k,
        hex("#f4f6f8"),
        Stroke::new(wd(1.2), hex("#c5ccd5")),
        StrokeKind::Middle,
    );
    let halo = hex("#f4f6f8");
    c.dt(
        xf,
        Pos2::new(fx, fy - 12.0),
        Align2::LEFT_BOTTOM,
        "N: connect-4 (7x6)",
        12.5,
        INK,
        Some(PAPER),
        false,
    );
    c.dt(
        xf,
        Pos2::new(fx + fw, fy - 12.0),
        Align2::RIGHT_BOTTOM,
        &format!("0 → {pl} ← 0   ·   {pl} places · {tr} transitions · {ar} arcs"),
        9.5,
        MUTED,
        Some(PAPER),
        true,
    );
    for (side, cx) in [("X", fx - 30.0), ("Y", fx + fw + 30.0)] {
        c.dash_poly(
            &sp_(&[Pos2::new(cx, fy + 16.0), Pos2::new(cx, fy + fh - 16.0)]),
            hex("#b9c0c8"),
            wd(1.0),
            wd(1.5),
            wd(3.0),
        );
        c.dt(
            xf,
            Pos2::new(cx, fy + 2.0),
            Align2::CENTER_BOTTOM,
            side,
            12.0,
            INK,
            Some(PAPER),
            false,
        );
        let ctr = xf.pt(cx, fy + fh / 2.0);
        c.p.circle(ctr, 13.0 * k, PAPER, Stroke::new(wd(1.0), hex("#c5ccd5")));
        c.dt(
            xf,
            Pos2::new(cx, fy + fh / 2.0 + 1.0),
            Align2::CENTER_CENTER,
            "∅",
            15.0,
            hex("#7a828c"),
            None,
            false,
        );
        if c.pointer.is_some_and(|p| p.distance(ctr) <= 13.0 * k) {
            c.hover_if(true, info(format!("{} foot: ∅", if side == "X" { "left" } else { "right" }))
                .note("The fixture declares no ports: every place is interior, so this foot is the empty set. A closed net is an open net with empty feet, ∅ → N ← ∅. Gluing along ∅ is only the disjoint union: there is nothing to identify.")
                .cite(Cites::fixture()).cite(Cites::cospan_row()));
        }
    }
    // the miniature at the fixture's own coordinates
    if let Some(doc) = c.d.c4.as_ref() {
        let xs = doc
            .places
            .iter()
            .map(|p| p.x)
            .chain(doc.transitions.iter().map(|t| t.x));
        let ys = doc
            .places
            .iter()
            .map(|p| p.y)
            .chain(doc.transitions.iter().map(|t| t.y));
        let (min_x, max_x) = xs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
        let (min_y, max_y) = ys.fold((f64::MAX, f64::MIN), |(a, b), y| (a.min(y), b.max(y)));
        let (pad_t, pad_b, pad_x) = (44.0f32, 44.0f32, 30.0f32);
        let sc = ((fw - 2.0 * pad_x) / (max_x - min_x).max(1.0) as f32)
            .min((fh - pad_t - pad_b) / (max_y - min_y).max(1.0) as f32);
        let ox = fx + (fw - (max_x - min_x) as f32 * sc) / 2.0;
        let oy = fy + pad_t;
        let at =
            |x: f64, y: f64| Pos2::new(ox + (x - min_x) as f32 * sc, oy + (y - min_y) as f32 * sc);
        let pos: BTreeMap<&str, Pos2> = doc
            .places
            .iter()
            .map(|p| (p.id.as_str(), at(p.x, p.y)))
            .chain(
                doc.transitions
                    .iter()
                    .map(|t| (t.id.as_str(), at(t.x, t.y))),
            )
            .collect();
        let arc_col = Color32::from_rgba_unmultiplied(0x5b, 0x64, 0x70, 26);
        let mut inh = 0;
        for a in &doc.arcs {
            if a.inhibit {
                inh += 1;
            }
            if let (Some(p), Some(q)) = (pos.get(a.source.as_str()), pos.get(a.target.as_str())) {
                c.line(xf.pt(p.x, p.y), xf.pt(q.x, q.y), arc_col, wd(0.5));
            }
        }
        let mut deg: BTreeMap<&str, usize> = BTreeMap::new();
        for a in &doc.arcs {
            *deg.entry(a.source.as_str()).or_default() += 1;
            *deg.entry(a.target.as_str()).or_default() += 1;
        }
        let ptr = c.pointer;
        let mut hover_node: Option<Info> = None;
        for t in &doc.transitions {
            let p = pos[t.id.as_str()];
            let drop = t.id.contains("_play_");
            let r = Rect::from_center_size(xf.pt(p.x, p.y), Vec2::splat((4.0 * k).max(2.5)));
            c.p.rect_filled(r, 0.0, if drop { NODE } else { hex("#7f5aa8") });
            if ptr.is_some_and(|q| r.expand(2.0).contains(q)) {
                let mut i = info(format!("transition {}", t.id))
                    .fact("kind", if drop { "a drop (a move)" } else { "a win detector: automatic in pflow-rs's test game (fires as soon as it is enabled)" })
                    .fact("arcs", deg.get(t.id.as_str()).copied().unwrap_or(0).to_string())
                    .fact("fixture x, y", format!("{}, {}", t.x, t.y))
                    .cite(Cites::fixture());
                if !drop {
                    i = i.cite(Cites::auto_win());
                }
                hover_node = Some(i);
            }
        }
        for p in &doc.places {
            let q = pos[p.id.as_str()];
            let init = p.initial.first().copied().unwrap_or(0.0);
            let ctr = xf.pt(q.x, q.y);
            c.p.circle(
                ctr,
                2.7 * k,
                if init > 0.0 { INK } else { Color32::WHITE },
                Stroke::new(wd(0.8), hex("#4a525c")),
            );
            if ptr.is_some_and(|m| m.distance(ctr) <= 3.5 * k + 2.0) {
                hover_node = Some(
                    info(format!("place {}", p.id))
                        .fact("initial", format!("{init}"))
                        .fact(
                            "arcs",
                            deg.get(p.id.as_str()).copied().unwrap_or(0).to_string(),
                        )
                        .fact("fixture x, y", format!("{}, {}", p.x, p.y))
                        .fact("on a foot", "no: the net is closed")
                        .cite(Cites::fixture()),
                );
            }
        }
        // group labels by the fixture's own names
        let groups: [(&str, &str); 3] = [("open_", "open_c_r"), ("x_", "x_c_r"), ("o_", "o_c_r")];
        for (pre, label) in groups {
            let ns: Vec<Pos2> = doc
                .places
                .iter()
                .filter(|p| {
                    p.id.starts_with(pre)
                        && p.id[pre.len()..].starts_with(|ch: char| ch.is_ascii_digit())
                })
                .map(|p| pos[p.id.as_str()])
                .collect();
            if ns.is_empty() {
                continue;
            }
            let cx = (ns.iter().map(|p| p.x).fold(f32::MAX, f32::min)
                + ns.iter().map(|p| p.x).fold(f32::MIN, f32::max))
                / 2.0;
            let top = ns.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            c.dt(
                xf,
                Pos2::new(cx, top - 9.0),
                Align2::CENTER_BOTTOM,
                &format!("{label} · {} places", ns.len()),
                8.5,
                MUTED,
                Some(halo),
                false,
            );
        }
        for id in ["x_turn", "o_turn", "game_active", "win_x", "win_o"] {
            if let Some(p) = pos.get(id) {
                c.dt(
                    xf,
                    Pos2::new(p.x, p.y - 7.0),
                    Align2::CENTER_BOTTOM,
                    id,
                    8.0,
                    INK,
                    Some(halo),
                    false,
                );
            }
        }
        let plays: Vec<Pos2> = doc
            .transitions
            .iter()
            .filter(|t| t.id.contains("_play_"))
            .map(|t| pos[t.id.as_str()])
            .collect();
        if !plays.is_empty() {
            let mx = plays.iter().map(|p| p.x).fold(f32::MIN, f32::max);
            let my = plays.iter().map(|p| p.y).fold(f32::MAX, f32::min);
            c.dt(
                xf,
                Pos2::new(mx, my - 9.0),
                Align2::RIGHT_BOTTOM,
                &format!("x_play_c_r, o_play_c_r: {} drops (moves)", plays.len()),
                8.5,
                MUTED,
                Some(halo),
                false,
            );
        }
        let mut win_kinds: BTreeMap<String, Vec<Pos2>> = BTreeMap::new();
        let mut n_wins = 0;
        for t in doc.transitions.iter().filter(|t| !t.id.contains("_play_")) {
            n_wins += 1;
            let kind =
                t.id.split("_win_")
                    .nth(1)
                    .and_then(|r| r.split('_').next())
                    .map(str::to_string)
                    .unwrap_or_else(|| "other".into());
            win_kinds.entry(kind).or_default().push(pos[t.id.as_str()]);
        }
        for (kind, vs) in &win_kinds {
            let cx = (vs.iter().map(|p| p.x).fold(f32::MAX, f32::min)
                + vs.iter().map(|p| p.x).fold(f32::MIN, f32::max))
                / 2.0;
            let by = vs.iter().map(|p| p.y).fold(f32::MIN, f32::max);
            c.dt(
                xf,
                Pos2::new(cx, by + 14.0),
                Align2::CENTER_BOTTOM,
                &format!("win_{kind} ({})", vs.len()),
                8.5,
                MUTED,
                Some(halo),
                false,
            );
        }
        c.dt(xf, Pos2::new(fx + fw / 2.0, fy + fh - 9.0), Align2::CENTER_BOTTOM, &format!("x_win_*, o_win_*: {n_wins} win detectors (the fixture: catalytic, consuming the opponent's turn token and game_active)"), 8.5, MUTED, Some(halo), false);
        // the frame and the texture: hover entries (later ones win)
        if c.pointer_in(frame) {
            c.hover_if(true, info("N: connect-4, a closed net")
                .fact("places", pl.to_string())
                .fact("transitions", tr.to_string())
                .fact("arcs", ar.to_string())
                .fact("inhibitor arcs", inh.to_string())
                .fact("as an open net", "∅ → N ← ∅: both feet empty")
                .note("The Connect-4 game net as a closed net: no ports, so both feet are empty (Baez–Master: an open net with empty feet). Arcs are drawn faint, as a texture: every arc of the fixture, between its own coordinates.")
                .cite(Cites::fixture()).cite(Cites::game()).cite(Cites::arcs()));
        }
        if let Some(i) = hover_node {
            c.hover_if(true, i);
        }
    }
    // the plan runs over N
    let (py, ph, px0, pw) = (598.0f32, 112.0f32, 40.0f32, w - 80.0);
    let ax = fx + fw / 2.0;
    let blue = hex("#2f6fb3");
    let arrow = [Pos2::new(ax, py - 4.0), Pos2::new(ax, fy + fh + 6.0)];
    c.dash_poly(&sp_(&arrow), blue, wd(1.4), wd(5.0), wd(4.0));
    c.arrow_head(
        xf.pt(arrow[0].x, arrow[0].y),
        xf.pt(arrow[1].x, arrow[1].y),
        wd(7.0),
        blue,
    );
    c.dt(xf, Pos2::new(ax + 10.0, fy + fh + 30.0), Align2::LEFT_BOTTOM, "runs over N: SearchPlan::solver(&Game) builds a solver for a game on this net. Not glued to it: the plan has no places.", 9.5, blue, Some(PAPER), false);
    let is_example =
        c.d.plan
            .as_ref()
            .is_some_and(|p| p["plan"] == sp::canonical(&sp::connect4_exact()));
    let pbox = xf.rect(px0, py, pw, ph);
    c.p.rect(
        pbox,
        9.0 * k,
        Color32::WHITE,
        Stroke::new(wd(1.2), hex("#9fb6d3")),
        StrokeKind::Middle,
    );
    let perr =
        c.d.plan
            .as_ref()
            .and_then(|p| p["check"]["error"].as_str().map(String::from));
    if c.pointer_in(pbox) {
        let plan_json =
            c.d.plan
                .as_ref()
                .map_or(String::new(), |p| s(&p["json"]).to_string());
        let i = info("the search plan")
            .fact("plan JSON", plan_json)
            .fact(
                "is plan:connect4-exact",
                if is_example {
                    "yes (the plan.rs module-doc example)"
                } else {
                    "no: changed on this page"
                },
            )
            .fact(
                "check",
                match &perr {
                    Some(e) => format!("refused: {e}"),
                    None => "passes (SearchPlan::check and solver)".to_string(),
                },
            )
            .note("A SearchPlan: the pieces of the game search, picked by name. Its slots are filled by pieces; plan.rs's check decides what fits. It composes by chaining, wrapping and filling, which is the operad view's subject. The laws named in the caption (a partial monoid of order pieces, drivers composed around the core) are the ontology ROADMAP's reading of plan.rs, not a theorem checked here.")
            .cite(Cites::solver())
            .cite(Cites::laws());
        c.hover_if(true, i);
    }
    c.dt(
        xf,
        Pos2::new(px0 + 12.0, py + 17.0),
        Align2::LEFT_BOTTOM,
        &format!(
            "Solver for {}{} · mode: {}",
            if is_example {
                "plan:connect4-exact"
            } else {
                "a changed plan (not plan:connect4-exact)"
            },
            if perr.is_some() { " (refused)" } else { "" },
            plan["mode"].as_str().unwrap_or("exact")
        ),
        10.5,
        INK,
        Some(Color32::WHITE),
        false,
    );
    c.dt(
        xf,
        Pos2::new(px0 + pw - 12.0, py + 17.0),
        Align2::RIGHT_BOTTOM,
        "drag a piece from the palette into a slot",
        9.0,
        MUTED,
        Some(Color32::WHITE),
        false,
    );
    c.dt(xf, Pos2::new(px0 + 12.0, py + ph - 9.0), Align2::LEFT_BOTTOM, "Slots take pieces by kind: the order chain is a partial monoid under chaining, drivers wrap the search, the memo fills one slot (ROADMAP). Bounds and engine are designed, not built.", 8.5, MUTED, Some(Color32::WHITE), false);
    // slots, scaled to fit the strip
    let piece_label = |e: &Value| -> (String, String) {
        match e {
            Value::String(n) => (n.clone(), n.clone()),
            Value::Object(o) => {
                let (n, a) = o
                    .iter()
                    .next()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .unwrap_or_default();
                let t = if let Some(x) = a.get("nodes") {
                    format!("{n} nodes {x}")
                } else {
                    format!("{n} {a}")
                };
                (n, t)
            }
            other => (other.to_string(), other.to_string()),
        }
    };
    let chip_w = |t: &str| t.chars().count() as f32 * 9.0 * 0.62 + 14.0;
    struct Cell {
        slot: &'static sp::Slot,
        entries: Option<Vec<(String, String)>>,
        content: Option<String>,
        label: String,
        w: f32,
    }
    let mut cells: Vec<Cell> = vec![];
    for slot in sp::SLOTS.iter() {
        let entries: Option<Vec<(String, String)>> = if slot.designed {
            None
        } else {
            let f = slot.field.unwrap_or("");
            let v = plan.get(f);
            Some(match (f, v) {
                ("memo", Some(m)) if !m.is_null() => {
                    vec![("memo".into(), format!("memo entries {}", m["entries"]))]
                }
                ("memo", _) => vec![],
                (_, Some(Value::Array(a))) => a.iter().map(&piece_label).collect(),
                _ => vec![],
            })
        };
        let absent = !slot.designed
            && slot
                .field
                .is_some_and(|f| f != "memo" && plan.get(f).is_none());
        let content: Option<String> = if slot.designed {
            Some("not built".into())
        } else if absent && slot.id == "drivers" {
            Some("default: null_window".into())
        } else if absent && slot.id == "order" {
            Some("default: tt_best, immediate_win, history".into())
        } else if entries.as_ref().is_some_and(|e| e.is_empty()) {
            Some(if slot.id == "order" {
                "[] (identity)".into()
            } else {
                "empty".into()
            })
        } else {
            None
        };
        let nw_off = slot.id == "drivers"
            && plan["drivers"]
                .as_array()
                .is_some_and(|d| !d.iter().any(|x| driver_name(x) == "null_window"));
        let label = if nw_off {
            format!("{} · null_window off", slot.label)
        } else {
            slot.label.to_string()
        };
        let wv = (text_w(&label, 8.5) + 18.0).max(match (&content, &entries) {
            (Some(t), _) => t.chars().count() as f32 * 8.5 * 0.6 + 20.0,
            (None, Some(e)) => e.iter().fold(11.0, |a, (_, t)| a + chip_w(t) + 5.0),
            _ => 40.0,
        });
        cells.push(Cell {
            slot,
            entries,
            content,
            label,
            w: wv,
        });
    }
    let total_w = cells.iter().map(|x| x.w).sum::<f32>() + 8.0 * (cells.len() as f32 - 1.0);
    let fit = ((pw - 24.0) / total_w).min(1.0);
    let mut cx = px0 + (pw - total_w * fit) / 2.0;
    let cy = py + 28.0;
    let chh = 56.0;
    let piece_event = c.anim(|e| match e {
        Event::PieceAdded { name, slot } => Some((name.clone(), slot.clone())),
        _ => None,
    });
    for cell in &cells {
        let wv = cell.w * fit;
        let r = xf.rect(cx, cy, wv, chh * fit.max(0.6));
        let title = &cell.label;
        let lit = c.slot_zone(r, cell.slot.id);
        c.p.rect_filled(
            r,
            6.0 * k,
            if cell.slot.designed {
                hex("#f6f7f9")
            } else {
                hex("#f3f7fc")
            },
        );
        if cell.slot.designed {
            c.p.extend(Shape::dashed_line(
                &[
                    r.left_top(),
                    r.right_top(),
                    r.right_bottom(),
                    r.left_bottom(),
                    r.left_top(),
                ],
                Stroke::new(wd(1.2), hex("#c9cfd6")),
                4.0 * k,
                3.0 * k,
            ));
        } else {
            c.p.rect_stroke(
                r,
                6.0 * k,
                Stroke::new(wd(1.2), hex("#9fb6d3")),
                StrokeKind::Middle,
            );
        }
        if lit == Some(true) {
            c.p.ctx().request_repaint();
            let tm = c.p.ctx().input(|i| i.time) as f32;
            c.p.rect_stroke(
                r,
                6.0 * k,
                Stroke::new(3.0 + 1.5 * (tm * 7.0).sin(), LIT),
                StrokeKind::Middle,
            );
        }
        c.dt(
            xf,
            Pos2::new(cx + 8.0 * fit, cy + 13.0),
            Align2::LEFT_BOTTOM,
            title,
            8.5 * fit.max(0.8),
            MUTED,
            None,
            false,
        );
        if let Some(t) = &cell.content {
            c.dt(
                xf,
                Pos2::new(cx + 8.0 * fit, cy + 36.0),
                Align2::LEFT_BOTTOM,
                t,
                8.5 * fit.max(0.8),
                hex("#8a929c"),
                None,
                false,
            );
        } else if let Some(es) = &cell.entries {
            let mut qx = cx + 6.0 * fit;
            let n = es.len();
            for (i, (name, text)) in es.iter().enumerate() {
                let pw_ = chip_w(text) * fit;
                let col = proto_piece_color(sp::piece_info(name).map_or("", |p| p.kind));
                let is_new = piece_event.as_ref().is_some_and(|((pn, ps), _)| {
                    pn == name && ps == cell.slot.id && (cell.slot.id == "memo" || i == n - 1)
                });
                let (dy, alpha) = match &piece_event {
                    Some((_, t)) if is_new => (lerp(-46.0, 0.0, *t) * fit, *t),
                    _ => (0.0, 1.0),
                };
                let chip = xf.rect(qx, cy + 24.0 + dy / fit.max(0.01) * fit, pw_, 20.0 * fit);
                c.p.rect(
                    chip,
                    4.0 * k,
                    Color32::WHITE.gamma_multiply(alpha),
                    Stroke::new(wd(1.4), col.gamma_multiply(alpha)),
                    StrokeKind::Middle,
                );
                c.p.text(
                    chip.center(),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::monospace((9.0 * k * fit).max(7.0) * 0.94),
                    col.gamma_multiply(alpha),
                );
                if c.pointer_in(chip) {
                    let ip = sp::piece_info(name);
                    let mut inf = info(format!("piece {name}"));
                    if let Some(p) = ip {
                        inf = inf
                            .fact("kind", p.kind)
                            .fact("status", p.status)
                            .fact("contract", p.contract)
                            .note(p.doc)
                            .cite(format!(
                                "pflow-rs crates/pflow-metamodel/src/plan.rs @{}",
                                sp::PLAN_RS_REV
                            ));
                    }
                    c.hover_if(true, inf);
                    if c.is_clicked_in(chip) {
                        c.command(
                            "plan.remove_piece",
                            json!({"slot": cell.slot.id, "index": i}),
                        );
                    }
                }
                qx += (chip_w(text) + 5.0) * fit;
            }
        }
        cx += (cell.w + 8.0) * fit;
    }
    // footer
    c.dt(xf, Pos2::new(18.0, h - 52.0), Align2::LEFT_BOTTOM, "Composite: none. N is closed (∅ → N ← ∅), and a plan is not a net, so there is no pushout to draw here.", 11.5, INK, Some(PAPER), false);
    c.dt(xf, Pos2::new(18.0, h - 35.0), Align2::LEFT_BOTTOM, "The orders example has open nets; switch to it to see feet, legs and (with a second instance) a pushout. The plan's own composition is the operad view (tab 3).", 10.0, MUTED, Some(PAPER), false);
    c.scene
        .emit("net", "connect-4", json!({"places": pl, "transitions": tr}));
    c.scene.emit("note", "closed", json!({}));
    plan_warn(c);
    c.note("na", "Does not apply as drawn: a SearchPlan is not an open net. It composes searches by chaining order pieces, wrapping drivers around the search and filling slots, and there is no place on its boundary to glue. The nearest honest picture is the operad view (tab 3). The plan runs over this closed net (SearchPlan::solver takes the game), it does not compose with it: below, the net is drawn as ∅ → N ← ∅ with the plan beside it.");
}

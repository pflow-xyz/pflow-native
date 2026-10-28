//! The incidence view (see `docs/composition-views-scene.md`): C = post − pre
//! (places × transitions) of a P/T net, and how composition acts on it. The
//! orders example draws the block-diagonal C of the active instances next to
//! the C of the flat net, event classes and wires as combs, and y·C per
//! declared invariant; the connect4 example draws the Connect-4 net's matrix.
//! Port of the prototype's `js/views/incidence.js`. C itself is core's
//! (`bundle.incidence`); the page adds only how it is drawn and the pre/post
//! behind each cell.

use std::collections::HashMap;

use super::d_draw::*;
use super::pflow_rs_cites::pflow_rs;
use super::*;

fn c_merge_arcs() -> String {
    go("metamodel/compose_flatten.go", 335, 411, "mergeArcs: duplicates merge; under MergeSum weights add, which preserves each component's P-invariants (337-341)")
}
fn c_merge_sum() -> String {
    go(
        "metamodel/compose.go",
        200,
        202,
        "MergeSum: each component still consumes what it declared",
    )
}
fn c_guard() -> String {
    go("metamodel/compose_flatten.go", 424, 482, "applyGuardLinks: structural lowering adds a read (or inhibitor) arc, expression lowering a guard on the transition; neither moves tokens")
}
fn c_weight_go() -> String {
    go("petri/net.go", 98, 109, "GetWeightSum: an arc's weight is the sum over its token colors (1 when empty); IncidenceMatrix uses it")
}
fn c_firing() -> String {
    go(
        "metamodel/firing.go",
        79,
        120,
        "Inputs/Outputs leave out read and inhibitor arcs; Tests holds them",
    )
}
fn c_fire() -> String {
    go(
        "metamodel/firing.go",
        198,
        212,
        "Fire: subtract inputs, add outputs; read and inhibitor arcs move nothing",
    )
}
fn c_inc_go() -> String {
    go(
        "reachability/invariants.go",
        91,
        151,
        "IncidenceMatrix: C[p][t] = output − input; inhibitor arcs skipped",
    )
}
fn c_structural() -> String {
    go(
        "docs/MODEL-CORRECTNESS.md",
        113,
        113,
        "method \"structural\": y*C = 0 on the incidence matrix",
    )
}
fn c_road42() -> String {
    onto("ontologyRoadmap", "ROADMAP.md", 243, 245, "Phase 4.2 (a planned deliverable): compare with pflow-jl glue on token links only; exit is the same incidence matrix up to place renaming")
}
fn c_road6() -> String {
    onto("ontologyRoadmap", "ROADMAP.md", 405, 413, "Phase 6: ⊗ is disjoint union; token/data glue places; event fuses transitions; guard is outside the free SMC")
}

const NEG: Color32 = Color32::from_rgb(0xc8, 0x55, 0x3d);
const NEG2: Color32 = Color32::from_rgb(0x8f, 0x2a, 0x1b);
const POS: Color32 = Color32::from_rgb(0x3a, 0x76, 0xb4);
const POS2: Color32 = Color32::from_rgb(0x1d, 0x4b, 0x80);
const GRID: Color32 = Color32::from_rgb(0xe5, 0xe8, 0xec);
const INKC: Color32 = Color32::from_rgb(0x2b, 0x2f, 0x36);
const MUTEDC: Color32 = Color32::from_rgb(0x6a, 0x71, 0x7c);
const CLASS_COLORS: [&str; 6] = [
    "#227744", "#7b4cc2", "#b26a00", "#a8457f", "#178f7f", "#566573",
];
const MONO: f32 = 6.05;

fn cell_fill(v: i64) -> Color32 {
    match v {
        v if v <= -2 => NEG2,
        v if v < 0 => NEG,
        v if v >= 2 => POS2,
        v if v > 0 => POS,
        _ => Color32::TRANSPARENT,
    }
}

fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// What one matrix entry is made of.
#[derive(Clone, Default)]
struct Ent {
    pre: i64,
    post: i64,
    /// Read / inhibitor arcs: (type, weight, guard link).
    tests: Vec<(String, i64)>,
}

type Extra<'a> = Option<&'a dyn Fn(&Ctx, Pos2)>;

type EntMap = HashMap<(String, String), Ent>;

fn is_token(p: &Value) -> bool {
    s(&p["kind"]) != "data"
}

fn ro(a: &Value) -> bool {
    matches!(s(&a["type"]), "read" | "inhibitor")
}

fn build_ents(
    arcs: impl Iterator<Item = (String, String, i64, String)>,
    places: &BTreeSet<String>,
    trans: &BTreeSet<String>,
    token: &BTreeSet<String>,
) -> EntMap {
    let mut m: EntMap = HashMap::new();
    for (from, to, w, ty) in arcs {
        let w = if w == 0 { 1 } else { w };
        let readonly = ty == "read" || ty == "inhibitor";
        if places.contains(&from) && trans.contains(&to) {
            let e = m.entry((from.clone(), to.clone())).or_default();
            if readonly {
                e.tests.push((ty, w));
            } else if token.contains(&from) {
                e.pre += w;
            }
        } else if trans.contains(&from) && places.contains(&to) && !readonly && token.contains(&to)
        {
            m.entry((to, from)).or_default().post += w;
        }
    }
    m
}

#[derive(Clone)]
struct Cls {
    name: String,
    members: Vec<String>,
    tag: String,
    color: Color32,
    links: Vec<Value>,
}

#[derive(Clone)]
struct Wire {
    name: String,
    members: Vec<String>,
    kind: String,
    links: Vec<Value>,
    tag: String,
    color: Color32,
}

fn link_text(l: &Value) -> String {
    format!(
        "{}:{} ⊗ {}:{}{}",
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
}

fn class_info(cl: &Cls) -> Info {
    info(format!("event class {}: {} transitions fuse", cl.tag, cl.members.len()))
        .fact("what it is", "Transitions joined by event links (union-find over the links; the class is named by its sorted members).")
        .fact("flat transition", cl.name.clone())
        .fact("members", cl.members.join(", "))
        .fact("links", if cl.links.is_empty() { "none".to_string() } else { cl.links.iter().map(link_text).collect::<Vec<_>>().join("; ") })
        .fact("on C", format!("the flat column is the sum of the {} member columns", cl.members.len()))
        .fact("P-invariants", "a sum of columns keeps y·C = 0 for every y that was 0 on each member column (compose_flatten.go:337-341)")
        .fact("sources", "Bruni, Melgratti, Montanari, Sobociński (2013)")
        .cite(c_merge_arcs())
        .cite(c_merge_sum())
        .cite(c_road6())
}

fn wire_info(w: &Wire) -> Info {
    info(format!("wire {}: {} places fuse", w.tag, w.members.len()))
        .fact("what it is", "Places joined by token or data links (union-find; named \"wire:\" + the smallest member).")
        .fact("flat place", w.name.clone())
        .fact("members", w.members.join(", "))
        .fact(
            "links",
            w.links
                .iter()
                .map(|l| format!("{} {}:{} → {}:{}", s(&l["kind"]), s(&l["from"]["subnet"]), s(&l["from"]["port"]), s(&l["to"]["subnet"]), s(&l["to"]["port"])))
                .collect::<Vec<_>>()
                .join("; "),
        )
        .fact("on C", format!("the flat row is the sum of the {} member rows", w.members.len()))
        .fact("initial marking", "the members' initial markings add (mergePlaces)")
        .note(format!(
            "Place gluing is the pushout of open nets along the shared place (Baez–Master). ROADMAP 4.2 plans to compare the token-link part of this matrix with pflow-jl glue, up to place renaming{}; that comparison is not run here.",
            if w.kind == "data" { " (this wire is a data link, which 4.2 leaves out)" } else { "" }
        ))
        .cite(c_merge_arcs())
        .cite(c_road42())
        .cite(c_road6())
}

/// The pattern's invariant concept for a constraint (the prototype's `invariantInfo`).
fn invariant_concept(c: &Ctx, sid: &str, local: &str) -> Option<Value> {
    let sub = subnets(c).into_iter().find(|x| s(&x["id"]) == sid)?;
    let pat = &comp::ontology()["patterns"][s(&sub["pattern"])];
    arr(&pat["invariants"])
        .iter()
        .find(|i| {
            let id = s(&i["id"]);
            local == id
                || local.starts_with(&id.replace("<step>", ""))
                || (id.contains('<') && local.starts_with(id.split('<').next().unwrap_or("")))
        })
        .cloned()
}

fn fmt(v: i64) -> String {
    fmt_c(v)
}

/// A matrix cell: the colour of its value, the value when there is room, a
/// ring for pre = post, a diamond for a read arc.
#[allow(clippy::too_many_arguments)]
fn cell(c: &Ctx, r: Rect, v: i64, e: &Ent, show_val: bool, guard: Color32, a: f32) {
    let inner = r.shrink(0.5);
    if v != 0 {
        c.p.rect_filled(inner, 0.0, cell_fill(v).gamma_multiply(a));
        if show_val {
            c.text(
                r.center(),
                Align2::CENTER_CENTER,
                &fmt(v),
                9.0 * (r.height() / 13.0).max(0.8),
                Color32::WHITE.gamma_multiply(a),
            );
        }
    }
    if v == 0 && e.pre != 0 && e.post != 0 {
        c.p.circle_stroke(
            r.center(),
            r.width().min(r.height()) / 4.0,
            Stroke::new(1.2, MUTEDC.gamma_multiply(a)),
        );
    }
    if !e.tests.is_empty() {
        let rad = if v != 0 {
            (r.width().min(r.height()) / 5.0).max(2.6)
        } else {
            r.width().min(r.height()) / 2.0 - 2.0
        };
        let ctr = if v != 0 {
            Pos2::new(r.right() - rad - 0.5, r.top() + rad + 0.5)
        } else {
            r.center()
        };
        let pts = vec![
            ctr + Vec2::new(0.0, -rad),
            ctr + Vec2::new(rad, 0.0),
            ctr + Vec2::new(0.0, rad),
            ctr + Vec2::new(-rad, 0.0),
        ];
        c.p.add(Shape::convex_polygon(
            pts,
            Color32::WHITE.gamma_multiply(a),
            Stroke::new(1.3, guard.gamma_multiply(a)),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
fn grid(c: &Ctx, xf: &Xf, x0: f32, y0: f32, n: usize, m: usize, cw: f32, rh: f32) {
    let st = Stroke::new(0.6, GRID);
    for i in 0..=m {
        c.p.line_segment(
            [
                xf.pt(x0, y0 + i as f32 * rh),
                xf.pt(x0 + n as f32 * cw, y0 + i as f32 * rh),
            ],
            st,
        );
    }
    for j in 0..=n {
        c.p.line_segment(
            [
                xf.pt(x0 + j as f32 * cw, y0),
                xf.pt(x0 + j as f32 * cw, y0 + m as f32 * rh),
            ],
            st,
        );
    }
}

fn brackets(c: &Ctx, xf: &Xf, x: f32, y: f32, w: f32, h: f32) {
    let (s_, o) = (5.0, 3.0);
    let st = Stroke::new(1.3, INKC);
    for pts in [
        vec![
            (x - o + s_, y - 1.0),
            (x - o, y - 1.0),
            (x - o, y + h + 1.0),
            (x - o + s_, y + h + 1.0),
        ],
        vec![
            (x + w + o - s_, y - 1.0),
            (x + w + o, y - 1.0),
            (x + w + o, y + h + 1.0),
            (x + w + o - s_, y + h + 1.0),
        ],
    ] {
        c.p.add(Shape::line(
            pts.into_iter().map(|p| xf.pt(p.0, p.1)).collect(),
            st,
        ));
    }
}

/// Text rotated −60° about its start (the prototype's column headers).
fn rot_text(c: &Ctx, at: Pos2, t: &[(String, Color32)], size: f32) -> f32 {
    let font = FontId::monospace(size);
    let mut job = egui::text::LayoutJob::default();
    for (txt, col) in t {
        job.append(txt, 0.0, egui::text::TextFormat::simple(font.clone(), *col));
    }
    let g = c.p.layout_job(job);
    let w = g.size().x;
    let at = at + Vec2::new(size * 0.45, 0.0);
    c.p.add(
        egui::epaint::TextShape::new(at, g, INKC)
            .with_angle_and_anchor(-std::f32::consts::FRAC_PI_3, Align2::LEFT_BOTTOM),
    );
    w
}

pub(super) fn incidence_orders(c: &mut Ctx, area: Rect) {
    let (Some(inc), Some(flat)) = (c.d.incidence.clone(), c.d.flat.clone()) else {
        return;
    };
    if inc["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refused the composition ({}), so there is no flat matrix.",
                s(&inc["error"])
            ),
        );
        return;
    }
    let links = links(c);
    let mut subs = subnets(c);
    subs.sort_by(|a, b| s(&a["id"]).cmp(s(&b["id"]))); // Flatten's order (compose_flatten.go sortedSubnets)
    let guard_col = kind_color("guard");

    // Left: the disjoint union, block by block.
    struct Blk {
        sid: String,
        pattern: String,
        r0: usize,
        r1: usize,
        c0: usize,
        c1: usize,
    }
    let mut rows_l: Vec<(String, String, Value)> = vec![]; // (qualified id, subnet id, place)
    let mut cols_l: Vec<(String, String)> = vec![];
    let mut blocks: Vec<Blk> = vec![];
    let mut arcs_l: Vec<(String, String, i64, String)> = vec![];
    let mut tok_l: BTreeSet<String> = BTreeSet::new();
    let core_blocks: BTreeMap<String, Value> = arr(&inc["blocks"])
        .iter()
        .map(|b| (s(&b["subnet"]).to_string(), b.clone()))
        .collect();
    for sb in &subs {
        let sid = s(&sb["id"]).to_string();
        let b = Blk {
            sid: sid.clone(),
            pattern: s(&sb["pattern"]).to_string(),
            r0: rows_l.len(),
            r1: 0,
            c0: cols_l.len(),
            c1: 0,
        };
        for p in arr(&sb["places"]) {
            let id = format!("{sid}/{}", s(&p["id"]));
            if is_token(p) {
                tok_l.insert(id.clone());
            }
            rows_l.push((id, sid.clone(), p.clone()));
        }
        for t in arr(&sb["transitions"]) {
            cols_l.push((format!("{sid}/{}", s(&t["id"])), sid.clone()));
        }
        for a in arr(&sb["arcs"]) {
            arcs_l.push((
                format!("{sid}/{}", s(&a["from"])),
                format!("{sid}/{}", s(&a["to"])),
                a["weight"].as_i64().unwrap_or(1),
                s(&a["type"]).to_string(),
            ));
        }
        blocks.push(Blk {
            r1: rows_l.len(),
            c1: cols_l.len(),
            ..b
        });
    }
    let pl_l: BTreeSet<String> = rows_l.iter().map(|r| r.0.clone()).collect();
    let tr_l: BTreeSet<String> = cols_l.iter().map(|r| r.0.clone()).collect();
    let ent_l = build_ents(arcs_l.into_iter(), &pl_l, &tr_l, &tok_l);
    // C of a block entry is core's (bundle.incidence blocks); 0 off the diagonal
    let c_left = |ri: usize, ci: usize| -> i64 {
        let (r, cc) = (&rows_l[ri], &cols_l[ci]);
        if r.1 != cc.1 {
            return 0;
        }
        let Some(b) = core_blocks.get(&r.1) else {
            return 0;
        };
        let pi = arr(&b["rows"])
            .iter()
            .position(|x| s(x) == r.0.trim_start_matches(&format!("{}/", r.1)));
        let ti = arr(&b["cols"])
            .iter()
            .position(|x| s(x) == cc.0.trim_start_matches(&format!("{}/", cc.1)));
        match (pi, ti) {
            (Some(p), Some(t)) => b["matrix"][p][t].as_i64().unwrap_or(0),
            _ => 0,
        }
    };
    let row_ix_l: HashMap<&str, usize> = rows_l
        .iter()
        .enumerate()
        .map(|(i, r)| (r.0.as_str(), i))
        .collect();
    let col_ix_l: HashMap<&str, usize> = cols_l
        .iter()
        .enumerate()
        .map(|(i, r)| (r.0.as_str(), i))
        .collect();

    // Event classes and wires, from Flatten's map.
    let by_sub: BTreeMap<&str, &Value> = subs.iter().map(|x| (s(&x["id"]), x)).collect();
    let port_member = |r: &Value, place: bool| -> Option<String> {
        let sb = by_sub.get(s(&r["subnet"]))?;
        let p = arr(&sb["ports"])
            .iter()
            .find(|q| s(&q["id"]) == s(&r["port"]))?;
        let x = s(&p[if place { "place" } else { "transition" }]);
        (!x.is_empty()).then(|| format!("{}/{x}", s(&sb["id"])))
    };
    let classes: Vec<Cls> = flat["fused_groups"]
        .as_object()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, (name, ms))| {
            let members = strs(ms);
            Cls {
                name: name.clone(),
                tag: format!("⊗{}", alpha(i)),
                color: hex(CLASS_COLORS[i % CLASS_COLORS.len()]),
                links: links
                    .iter()
                    .filter(|l| {
                        s(&l["kind"]) == "event"
                            && port_member(&l["from"], false).is_some_and(|m| members.contains(&m))
                    })
                    .cloned()
                    .collect(),
                members,
            }
        })
        .collect();
    let wires: Vec<Wire> = flat["wires"]
        .as_object()
        .into_iter()
        .flatten()
        .enumerate()
        .map(|(i, (name, ms))| {
            let members = strs(ms);
            let ls: Vec<Value> = links
                .iter()
                .filter(|l| {
                    matches!(s(&l["kind"]), "token" | "data")
                        && [&l["from"], &l["to"]]
                            .iter()
                            .any(|r| port_member(r, true).is_some_and(|m| members.contains(&m)))
                })
                .cloned()
                .collect();
            let kind = if ls.iter().any(|l| s(&l["kind"]) == "token") {
                "token"
            } else {
                "data"
            };
            Wire {
                name: name.clone(),
                members,
                kind: kind.into(),
                links: ls,
                tag: format!("{}{}", kind_glyph(kind), i + 1),
                color: kind_color(kind),
            }
        })
        .collect();
    let mut class_of: HashMap<String, usize> = HashMap::new();
    for (i, cl) in classes.iter().enumerate() {
        for m in &cl.members {
            class_of.insert(m.clone(), i);
        }
        class_of.insert(cl.name.clone(), i);
    }
    let mut wire_of: HashMap<String, usize> = HashMap::new();
    for (i, w) in wires.iter().enumerate() {
        for m in &w.members {
            wire_of.insert(m.clone(), i);
        }
        wire_of.insert(w.name.clone(), i);
    }
    let guards = links.iter().filter(|l| s(&l["kind"]) == "guard").count();

    // Right: the flat net.
    let fplaces = arr(&flat["places"]);
    let ftrans = arr(&flat["transitions"]);
    let c_flat = arr(&inc["matrix"]);
    let (n_r, m_r) = (ftrans.len(), fplaces.len());
    let pl_r: BTreeSet<String> = fplaces.iter().map(|p| s(&p["id"]).to_string()).collect();
    let tr_r: BTreeSet<String> = ftrans.iter().map(|p| s(&p["id"]).to_string()).collect();
    let tok_r: BTreeSet<String> = fplaces
        .iter()
        .filter(|p| is_token(p))
        .map(|p| s(&p["id"]).to_string())
        .collect();
    let ent_r = build_ents(
        arr(&flat["arcs"]).iter().map(|a| {
            (
                s(&a["from"]).to_string(),
                s(&a["to"]).to_string(),
                a["weight"].as_i64().unwrap_or(1),
                s(&a["type"]).to_string(),
            )
        }),
        &pl_r,
        &tr_r,
        &tok_r,
    );
    let cv_r = |i: usize, j: usize| c_flat[i][j].as_i64().unwrap_or(0);
    let _ = ro;

    // Mechanical check: every flat entry is the sum of its member entries on the left.
    let (mut checked, mut agree) = (0usize, 0usize);
    for (i, p) in fplaces.iter().enumerate() {
        for (j, t) in ftrans.iter().enumerate() {
            let mut sum = 0;
            for pm in strs(&p["members"]) {
                for tm in strs(&t["members"]) {
                    if let (Some(a), Some(b)) =
                        (row_ix_l.get(pm.as_str()), col_ix_l.get(tm.as_str()))
                    {
                        sum += c_left(*a, *b);
                    }
                }
            }
            checked += 1;
            if sum == cv_r(i, j) {
                agree += 1;
            }
        }
    }
    let member_sum = |i: usize, j: usize| -> i64 {
        let mut sum = 0;
        for pm in strs(&fplaces[i]["members"]) {
            for tm in strs(&ftrans[j]["members"]) {
                if let (Some(a), Some(b)) = (row_ix_l.get(pm.as_str()), col_ix_l.get(tm.as_str())) {
                    sum += c_left(*a, *b);
                }
            }
        }
        sum
    };

    // layout (depends only on the active instances, so a link keeps every cell put)
    let (m_, gap, verd) = (14.0f32, 104.0f32, 172.0f32);
    let max_len = |it: &mut dyn Iterator<Item = usize>| it.max().unwrap_or(0) as f32;
    let label_wl = (max_len(&mut rows_l.iter().map(|r| r.0.chars().count())) * MONO).ceil() + 12.0;
    let label_wr = label_wl + (5.0 * MONO).ceil();
    let (n_l, m_l) = (cols_l.len(), rows_l.len());
    let cw = (((1150.0 - (2.0 * m_ + label_wl + gap + label_wr + verd))
        / (2.0 * n_l.max(1) as f32))
        .floor())
    .clamp(11.0, 24.0);
    let rh = (cw * 0.78).round().clamp(12.0, 17.0);
    let show_val = cw >= 15.0;
    let col_font = if cw >= 15.0 { 10.0 } else { 9.0 };
    let col_label_h =
        (max_len(&mut cols_l.iter().map(|r| r.0.chars().count())) * MONO * 0.866).ceil() + 10.0;
    let x0_l = m_ + label_wl;
    let x_mr = x0_l + n_l as f32 * cw;
    let x0_r = x_mr + gap + label_wr;
    let x_region_r = x_mr + gap;
    // pattern legend (flows onto a second line if it must)
    let mut patterns: Vec<(String, String, Vec<String>)> = vec![];
    for sb in &subs {
        let key = s(&sb["pattern"]).to_string();
        let label = comp::ontology()["patterns"][&key]["prefLabel"]
            .as_str()
            .unwrap_or(&key)
            .to_string();
        match patterns.iter_mut().find(|p| p.0 == key) {
            Some(p) => p.2.push(s(&sb["id"]).to_string()),
            None => patterns.push((key, label, vec![s(&sb["id"]).to_string()])),
        }
    }
    let legend_items: Vec<f32> = patterns
        .iter()
        .map(|p| {
            16.0 + p.1.chars().count() as f32 * 6.3
                + 6.0
                + p.2
                    .iter()
                    .map(|s| s.chars().count() as f32 * 6.1 + 8.0)
                    .sum::<f32>()
                + 14.0
        })
        .collect();
    let mut legend_lines: Vec<Vec<usize>> = vec![vec![]];
    let mut lx = 0.0;
    for (i, w) in legend_items.iter().enumerate() {
        if lx + w > x_mr - m_ && !legend_lines.last().unwrap().is_empty() {
            legend_lines.push(vec![]);
            lx = 0.0;
        }
        legend_lines.last_mut().unwrap().push(i);
        lx += w;
    }
    let y_leg0 = 50.0;
    let y_lab = y_leg0 + legend_lines.len() as f32 * 16.0 + col_label_h;
    let y_m = y_lab + 6.0;
    let (y_mbl, y_mbr) = (y_m + m_l as f32 * rh, y_m + m_r as f32 * rh);
    let comb_step = if classes.is_empty() { 0.0 } else { 12.0 };
    let y_comb_end = y_mbl
        + if classes.is_empty() {
            0.0
        } else {
            10.0 + classes.len() as f32 * comb_step
        };
    let y_inv0 = y_mbr + 26.0;
    let inv_row_h = rh + 3.0;
    let invs = arr(&inc["invariants"]);
    let y_inv_end = y_inv0 + 8.0 + invs.len() as f32 * inv_row_h;
    let legend_bottom = 2
        + classes.len()
        + usize::from(!classes.is_empty())
        + wires.len()
        + usize::from(guards > 0);
    let y_bot = y_comb_end.max(y_inv_end) + 26.0;
    let w_total = (x0_r + n_r.max(1) as f32 * cw + verd + m_).max(900.0);
    let h_total = y_bot + legend_bottom as f32 * 16.0 + 8.0;
    let mut xf = Xf::fit(area, w_total, h_total, 1.6);
    xf.origin.x += ((area.width() - w_total * xf.k) / 2.0).max(0.0);
    let k = xf.k;

    // the merge animation: what was there before this link, from the previous revision
    #[derive(Clone)]
    struct Mo {
        t: f32,
        kind: String,
        link: Value,
    }
    let mo = c
        .anim(|e| match e {
            Event::LinkAdded { key, kind } => Some((key.clone(), kind.clone())),
            _ => None,
        })
        .and_then(|((key, kind), t)| {
            let l = links.iter().find(|l| s(&l["key"]) == key)?.clone();
            Some(Mo { t, kind, link: l })
        });
    let inst_added = c.anim(|e| match e {
        Event::InstanceAdded { id } => Some(id.clone()),
        _ => None,
    });
    let key_of = |ms: &Value| -> String {
        let mut v = strs(ms);
        v.sort();
        v.join("|")
    };
    // per-column / per-row slide offsets and fades of the flat matrix
    let mut col_off = vec![0.0f32; n_r];
    let mut row_off = vec![0.0f32; m_r];
    let mut col_al = vec![1.0f32; n_r];
    let mut row_al = vec![1.0f32; m_r];
    struct Ghost {
        // the old member's entries, drawn in the new fused column / row
        cells: Vec<(usize, usize, i64)>,
        from_idx: usize,
        to_idx: usize,
        by_col: bool,
    }
    let mut ghosts: Vec<Ghost> = vec![];
    let mut badge: Option<(Pos2, Color32)> = None;
    let mut comb_draw: Option<(bool, String)> = None;
    if let (Some(mo), Some((pf, pi))) = (&mo, c.d.prev.clone()) {
        if mo.kind != "guard" && pf["ok"] == true {
            let by_col = mo.kind == "event";
            let (now, before) = if by_col {
                (ftrans, arr(&pf["transitions"]))
            } else {
                (fplaces, arr(&pf["places"]))
            };
            let prev_ix: HashMap<String, usize> = before
                .iter()
                .enumerate()
                .map(|(i, x)| (key_of(&x["members"]), i))
                .collect();
            let prev_m = arr(&pi["matrix"]);
            let now_row: HashMap<&str, usize> = fplaces
                .iter()
                .enumerate()
                .map(|(i, p)| (s(&p["id"]), i))
                .collect();
            let now_col: HashMap<&str, usize> = ftrans
                .iter()
                .enumerate()
                .map(|(i, p)| (s(&p["id"]), i))
                .collect();
            let sl = (1.0 - (mo.t / 0.62).clamp(0.0, 1.0)).max(0.0);
            for (j, x) in now.iter().enumerate() {
                let key = key_of(&x["members"]);
                if let Some(&i) = prev_ix.get(&key) {
                    let d = (i as f32 - j as f32) * if by_col { cw } else { rh } * sl;
                    if by_col {
                        col_off[j] = d;
                    } else {
                        row_off[j] = d;
                    }
                    continue;
                }
                let a = ((mo.t - 0.58) / 0.42).clamp(0.0, 1.0);
                if by_col {
                    col_al[j] = a;
                } else {
                    row_al[j] = a;
                }
                let xm = strs(&x["members"]);
                for (i, b) in before.iter().enumerate() {
                    if !strs(&b["members"]).iter().all(|m| xm.contains(m)) {
                        continue;
                    }
                    let mut cells = vec![];
                    if by_col {
                        for (pidx, p) in arr(&pf["places"]).iter().enumerate() {
                            let v = prev_m[pidx][i].as_i64().unwrap_or(0);
                            if let (true, Some(r)) = (v != 0, now_row.get(s(&p["id"]))) {
                                cells.push((*r, j, v));
                            }
                        }
                    } else {
                        for (tidx, t) in arr(&pf["transitions"]).iter().enumerate() {
                            let v = prev_m[i][tidx].as_i64().unwrap_or(0);
                            if let (true, Some(cc)) = (v != 0, now_col.get(s(&t["id"]))) {
                                cells.push((j, *cc, v));
                            }
                        }
                    }
                    ghosts.push(Ghost {
                        cells,
                        from_idx: i,
                        to_idx: j,
                        by_col,
                    });
                }
                let name = s(&x["id"]);
                let color = if by_col {
                    class_of.get(name).map(|i| classes[*i].color)
                } else {
                    wire_of.get(name).map(|i| wires[*i].color)
                }
                .unwrap_or(hex("#227744"));
                let p = if by_col {
                    xf.pt(x0_r + j as f32 * cw + cw / 2.0, y_m + m_r as f32 * rh / 2.0)
                } else {
                    xf.pt(x0_r + n_r as f32 * cw / 2.0, y_m + j as f32 * rh + rh / 2.0)
                };
                badge = Some((p, color));
                comb_draw = Some((by_col, name.to_string()));
            }
        }
    }
    let _ = (&comb_draw, inst_added.as_ref());

    // hovered cell of a matrix, for the crosshair
    let hover_cell = |x0: f32, n: usize, m: usize, c: &Ctx| -> Option<(usize, usize)> {
        let p = c.pointer_pos()?;
        let (fx, fy) = ((p.x - xf.origin.x) / k - x0, (p.y - xf.origin.y) / k - y_m);
        if fx < 0.0 || fy < 0.0 {
            return None;
        }
        let (j, i) = ((fx / cw) as usize, (fy / rh) as usize);
        (j < n && i < m).then_some((i, j))
    };
    let hl_l = hover_cell(x0_l, n_l, m_l, c);
    let hl_r = hover_cell(x0_r, n_r, m_r, c);
    // a hovered invariant lights the rows it names
    let mut y_rows: Vec<usize> = vec![];

    // notes
    if guards > 0 {
        c.note("honesty", format!("{} to a read arc (◇ on the right): it gates firing but moves nothing, so C does not change (applyGuardLinks, firing.go Inputs/Outputs).", if guards == 1 { "A guard link lowers".to_string() } else { format!("{guards} guard links lower") }));
    }
    c.note("na", "No ports here: an incidence matrix has no boundary. Make links in a view that draws ports (Cospans, Spiders, …); this view recomputes both matrices from the links. P-invariant checks below are read from bundle.incidence (y·C and y·M₀ over the flat net).");

    // titles
    let ids: Vec<&str> = subs.iter().map(|x| s(&x["id"])).collect();
    c.text(
        xf.pt(m_, 18.0),
        Align2::LEFT_CENTER,
        &format!("C of {}", ids.join(" ⊗ ")),
        13.0 * k,
        INKC,
    );
    c.text(
        xf.pt(m_, 34.0),
        Align2::LEFT_CENTER,
        &format!("block-diagonal: ⊗ is a disjoint union · {m_l} places × {n_l} transitions"),
        11.0 * k,
        MUTEDC,
    );
    c.text(
        xf.pt(x_region_r, 18.0),
        Align2::LEFT_CENTER,
        &format!(
            "C of the flat net (Flatten, {} link{})",
            links.len(),
            if links.len() == 1 { "" } else { "s" }
        ),
        13.0 * k,
        INKC,
    );
    c.text(
        xf.pt(x_region_r, 34.0),
        Align2::LEFT_CENTER,
        &format!(
            "{m_r} places × {n_r} transitions · event links add columns, token/data links add rows"
        ),
        11.0 * k,
        MUTEDC,
    );
    // pattern legend
    for (li, line) in legend_lines.iter().enumerate() {
        let (mut x, y) = (m_, y_leg0 + li as f32 * 16.0);
        for &pi in line {
            let (key, label, sids) = &patterns[pi];
            c.p.rect(
                Rect::from_min_size(xf.pt(x, y - 9.0), Vec2::new(12.0, 11.0) * k),
                2.0 * k,
                pattern_tint_p(key),
                Stroke::new(1.0, hex("#c3cbd5")),
                StrokeKind::Middle,
            );
            c.text(
                xf.pt(x + 16.0, y - 3.5),
                Align2::LEFT_CENTER,
                label,
                11.0 * k,
                MUTEDC,
            );
            x += 16.0 + label.chars().count() as f32 * 6.3 + 6.0;
            for sid in sids {
                let r = c.mono(xf.pt(x, y - 3.5), Align2::LEFT_CENTER, sid, 10.0 * k, INKC);
                if let Some(sb) = by_sub.get(sid.as_str()) {
                    c.hover(r, subnet_info(sb));
                }
                x += sid.chars().count() as f32 * 6.1 + 8.0;
            }
            x += 14.0;
        }
    }

    // ---- left matrix
    for b in &blocks {
        let r = Rect::from_min_size(
            xf.pt(x0_l + b.c0 as f32 * cw, y_m + b.r0 as f32 * rh),
            Vec2::new((b.c1 - b.c0) as f32 * cw, (b.r1 - b.r0) as f32 * rh) * k,
        );
        let al = match &inst_added {
            Some((id, t)) if *id == b.sid => *t,
            _ => 1.0,
        };
        c.p.rect(
            r,
            0.0,
            pattern_tint_p(&b.pattern).gamma_multiply(al),
            Stroke::new(0.8, hex("#c9d1db")),
            StrokeKind::Middle,
        );
    }
    for cl in &classes {
        for m in &cl.members {
            if let Some(j) = col_ix_l.get(m.as_str()) {
                c.p.rect_filled(
                    Rect::from_min_size(
                        xf.pt(x0_l + *j as f32 * cw, y_m),
                        Vec2::new(cw, m_l as f32 * rh) * k,
                    ),
                    0.0,
                    cl.color.gamma_multiply(0.1),
                );
            }
        }
    }
    for w in &wires {
        for m in &w.members {
            if let Some(i) = row_ix_l.get(m.as_str()) {
                c.p.rect_filled(
                    Rect::from_min_size(
                        xf.pt(x0_l, y_m + *i as f32 * rh),
                        Vec2::new(n_l as f32 * cw, rh) * k,
                    ),
                    0.0,
                    w.color.gamma_multiply(0.1),
                );
            }
        }
    }
    grid(c, &xf, x0_l, y_m, n_l, m_l, cw, rh);
    let none = Ent::default();
    for (i, r) in rows_l.iter().enumerate() {
        for (j, cc) in cols_l.iter().enumerate() {
            let e = ent_l.get(&(r.0.clone(), cc.0.clone())).unwrap_or(&none);
            let v = c_left(i, j);
            let rect = Rect::from_min_size(
                xf.pt(x0_l + j as f32 * cw, y_m + i as f32 * rh),
                Vec2::new(cw, rh) * k,
            );
            cell(c, rect, v, e, show_val, guard_col, 1.0);
            if hl_l == Some((i, j)) {
                let (sr, sc) = (&by_sub[r.1.as_str()], r.1 == cc.1);
                let w = wire_of.get(&r.0).map(|i| &wires[*i]);
                let kc = class_of.get(&cc.0).map(|i| &classes[*i]);
                let mut inf = info(format!("C[{}, {}] = {}", r.0, cc.0, fmt(v)))
                    .fact("place", r.0.clone())
                    .fact("transition", cc.0.clone())
                    .fact(
                        "pre (arc place → transition)",
                        if e.pre != 0 {
                            e.pre.to_string()
                        } else {
                            "none".into()
                        },
                    )
                    .fact(
                        "post (arc transition → place)",
                        if e.post != 0 {
                            e.post.to_string()
                        } else {
                            "none".into()
                        },
                    )
                    .fact("C = post − pre", fmt(v))
                    .fact(
                        "block",
                        if sc {
                            format!(
                                "{} (a {})",
                                r.1,
                                comp::ontology()["patterns"][s(&sr["pattern"])]["prefLabel"]
                                    .as_str()
                                    .unwrap_or("")
                            )
                        } else {
                            format!(
                                "off the diagonal: {} × {} is 0, because ⊗ is a disjoint union",
                                r.1, cc.1
                            )
                        },
                    );
                if v == 0 && e.pre != 0 && e.post != 0 {
                    inf = inf.fact(
                        "self-loop",
                        "pre = post: the loop is in the net but C records 0",
                    );
                }
                if let Some(w) = w {
                    inf = inf.fact("row fuses into", format!("{} ({})", w.name, w.tag));
                }
                if let Some(kc) = kc {
                    inf = inf.fact("column fuses into", format!("{} ({})", kc.name, kc.tag));
                }
                inf = inf.note("Computed from the instance's arcs (the subnets are pflow-compose's constructors); C is bundle.incidence's block.").cite(c_inc_go()).cite(c_firing());
                c.hover(rect, inf);
            }
        }
    }
    brackets(c, &xf, x0_l, y_m, n_l as f32 * cw, m_l as f32 * rh);
    // crosshair
    if let Some((i, j)) = hl_l {
        let st = Stroke::new(1.2, INKC);
        c.p.rect_stroke(
            Rect::from_min_size(
                xf.pt(x0_l, y_m + i as f32 * rh),
                Vec2::new(n_l as f32 * cw, rh) * k,
            ),
            0.0,
            st,
            StrokeKind::Middle,
        );
        c.p.rect_stroke(
            Rect::from_min_size(
                xf.pt(x0_l + j as f32 * cw, y_m),
                Vec2::new(cw, m_l as f32 * rh) * k,
            ),
            0.0,
            st,
            StrokeKind::Middle,
        );
    }
    // row headers (left)
    for (i, r) in rows_l.iter().enumerate() {
        let w = wire_of.get(&r.0).map(|i| &wires[*i]);
        let local = r.0.trim_start_matches(&format!("{}/", r.1)).to_string();
        let hl = hl_l.is_some_and(|h| h.0 == i);
        let y = y_m + i as f32 * rh + rh / 2.0;
        let loc_w = local.chars().count() as f32 * MONO;
        let pre = format!("{}/", r.1);
        let loc = c.mono(
            xf.pt(x0_l - 7.0, y),
            Align2::RIGHT_CENTER,
            &local,
            col_font * k,
            if let Some(w) = w {
                w.color
            } else if hl {
                Color32::BLACK
            } else {
                INKC
            },
        );
        let pr = c.mono(
            Pos2::new(loc.left(), loc.center().y),
            Align2::RIGHT_CENTER,
            &pre,
            col_font * k,
            MUTEDC,
        );
        let _ = loc_w;
        let nz = (0..n_l).filter(|j| c_left(i, *j) != 0).count();
        let mut inf = info(format!("place {}", r.0))
            .fact("concept", "ns:Place")
            .fact("initial", r.2["initial"].to_string())
            .fact(
                "row of C",
                format!("{nz} nonzero entries (all inside the {} block)", r.1),
            );
        if let Some(w) = w {
            inf = inf.fact(
                "fuses into",
                format!(
                    "{} ({}): its row is the sum of {} member rows",
                    w.name,
                    w.tag,
                    w.members.len()
                ),
            );
        }
        if !is_token(&r.2) {
            inf = inf.fact(
                "kind",
                "data place: no token arcs count, so its row is 0 (firing.go tokenPlace)",
            );
        }
        c.hover(pr.union(loc), inf);
    }
    // column headers (left), rotated
    for (j, cc) in cols_l.iter().enumerate() {
        let kc = class_of.get(&cc.0).map(|i| &classes[*i]);
        let local = cc.0.trim_start_matches(&format!("{}/", cc.1)).to_string();
        let at = xf.pt(x0_l + j as f32 * cw + cw / 2.0 + 3.0, y_lab);
        let hl = hl_l.is_some_and(|h| h.1 == j);
        let w = rot_text(
            c,
            at,
            &[
                (format!("{}/", cc.1), MUTEDC),
                (
                    local,
                    if let Some(kc) = kc {
                        kc.color
                    } else if hl {
                        Color32::BLACK
                    } else {
                        INKC
                    },
                ),
            ],
            col_font * k,
        );
        let end = at + Vec2::new(w * 0.5, -w * 0.866);
        let nz = (0..m_l).filter(|i| c_left(*i, j) != 0).count();
        let mut inf = info(format!("transition {}", cc.0))
            .fact("concept", "ns:Transition")
            .fact(
                "column of C",
                format!("{nz} nonzero entries (all inside the {} block)", cc.1),
            );
        if let Some(kc) = kc {
            inf = inf.fact(
                "fuses into",
                format!(
                    "{} = {}: its column is the sum of {} member columns (MergeSum)",
                    kc.tag,
                    kc.name,
                    kc.members.len()
                ),
            );
        }
        c.hover_seg(at, end, 5.0 * k.max(0.8), inf.cite(c_merge_arcs()));
    }
    // event-class combs under the left matrix
    for (ci, cl) in classes.iter().enumerate() {
        let mut js: Vec<usize> = cl
            .members
            .iter()
            .filter_map(|m| col_ix_l.get(m.as_str()).copied())
            .collect();
        js.sort();
        if js.is_empty() {
            continue;
        }
        let y = y_mbl + 10.0 + ci as f32 * comb_step;
        let xs: Vec<f32> = js
            .iter()
            .map(|j| x0_l + *j as f32 * cw + cw / 2.0)
            .collect();
        let frac = match &comb_draw {
            Some((true, name)) if *name == cl.name => mo
                .as_ref()
                .map_or(1.0, |m| ease((m.t / 0.9).clamp(0.0, 1.0))),
            _ => 1.0,
        };
        let st = Stroke::new(1.6 * k.max(0.8), cl.color);
        let xend = xs[0] + (xs[xs.len() - 1] - xs[0]) * frac;
        c.p.line_segment([xf.pt(xs[0], y), xf.pt(xend, y)], st);
        for x in &xs {
            c.p.line_segment([xf.pt(*x, y_mbl + 1.0), xf.pt(*x, y)], st);
            c.p.circle_filled(xf.pt(*x, y), 2.2 * k, cl.color);
        }
        let r = c.text(
            xf.pt(xs[0] - 6.0, y),
            Align2::RIGHT_CENTER,
            &cl.tag,
            10.0 * k,
            cl.color,
        );
        c.hover(
            Rect::from_min_max(
                xf.pt(xs[0] - 30.0, y - 5.0),
                xf.pt(xs[xs.len() - 1] + 4.0, y + 5.0),
            )
            .union(r),
            class_info(cl),
        );
    }
    // wire combs at the right of the left matrix
    let wire_step = if wires.is_empty() {
        0.0
    } else {
        12f32.min(44.0 / wires.len() as f32)
    };
    for (wi, w) in wires.iter().enumerate() {
        let mut is_: Vec<usize> = w
            .members
            .iter()
            .filter_map(|m| row_ix_l.get(m.as_str()).copied())
            .collect();
        is_.sort();
        if is_.is_empty() {
            continue;
        }
        let x = x_mr + 10.0 + wi as f32 * wire_step;
        let ys: Vec<f32> = is_
            .iter()
            .map(|i| y_m + *i as f32 * rh + rh / 2.0)
            .collect();
        let frac = match &comb_draw {
            Some((false, name)) if *name == w.name => mo
                .as_ref()
                .map_or(1.0, |m| ease((m.t / 0.9).clamp(0.0, 1.0))),
            _ => 1.0,
        };
        let st = Stroke::new(1.6 * k.max(0.8), w.color);
        let yend = ys[0] + (ys[ys.len() - 1] - ys[0]) * frac;
        c.p.line_segment([xf.pt(x, ys[0]), xf.pt(x, yend)], st);
        for y in &ys {
            c.p.line_segment([xf.pt(x_mr + 1.0, *y), xf.pt(x, *y)], st);
            c.p.circle_filled(xf.pt(x, *y), 2.2 * k, w.color);
        }
        let r = c.text(
            xf.pt(x, ys[0] - 5.0),
            Align2::CENTER_CENTER,
            &w.tag,
            10.0 * k,
            w.color,
        );
        c.hover(
            Rect::from_min_max(
                xf.pt(x - 4.0, ys[0] - 10.0),
                xf.pt(x + 4.0, ys[ys.len() - 1] + 3.0),
            )
            .union(r),
            wire_info(w),
        );
    }
    // Flatten arrow
    {
        let ay = y_m + m_l.min(m_r) as f32 * rh / 2.0;
        let (ax0, ax1) = (x_mr + 56.0, x_mr + gap - 4.0);
        c.line(
            xf.pt(ax0, ay),
            xf.pt(ax1 - 6.0, ay),
            MUTEDC,
            1.4 * k.max(0.8),
        );
        c.p.add(Shape::convex_polygon(
            vec![
                xf.pt(ax1, ay),
                xf.pt(ax1 - 7.0, ay - 4.0),
                xf.pt(ax1 - 7.0, ay + 4.0),
            ],
            MUTEDC,
            Stroke::NONE,
        ));
        c.text(
            xf.pt((ax0 + ax1) / 2.0, ay - 7.0),
            Align2::CENTER_CENTER,
            "Flatten",
            10.0 * k,
            MUTEDC,
        );
        c.text(
            xf.pt((ax0 + ax1) / 2.0, ay + 15.0),
            Align2::CENTER_CENTER,
            "Σ cols, Σ rows",
            8.5 * k,
            MUTEDC,
        );
        c.hover(
            Rect::from_min_max(xf.pt(ax0 - 2.0, ay - 20.0), xf.pt(ax1 + 2.0, ay + 20.0)),
            info("Flatten: from the disjoint union to the flat net")
                .fact("what it is", "go-pflow Bundle.Flatten, ported by pflow-compose: namespace every element, fuse places joined by token/data links and transitions joined by event links, merge duplicate arcs (MergeSum), lower guard links.")
                .fact("on C", "event classes add columns, wires add rows, guard links leave C alone")
                .fact("here", format!("{m_l} × {n_l} → {m_r} × {n_r}; {} event class{}, {} wire{}", classes.len(), if classes.len() == 1 { "" } else { "es" }, wires.len(), if wires.len() == 1 { "" } else { "s" }))
                .cite(go("metamodel/compose_flatten.go", 15, 27, "Flatten pipeline"))
                .cite(go("metamodel/compose_flatten.go", 335, 412, "mergeArcs: MergeSum"))
                .cite(c_guard()),
        );
    }

    // ---- right matrix
    for (j, t) in ftrans.iter().enumerate() {
        if let Some(ci) = class_of.get(s(&t["id"])) {
            c.p.rect_filled(
                Rect::from_min_size(
                    xf.pt(x0_r + j as f32 * cw + col_off[j], y_m),
                    Vec2::new(cw, m_r as f32 * rh) * k,
                ),
                0.0,
                classes[*ci].color.gamma_multiply(0.1 * col_al[j]),
            );
        }
    }
    for (i, p) in fplaces.iter().enumerate() {
        if let Some(wi) = wire_of.get(s(&p["id"])) {
            c.p.rect_filled(
                Rect::from_min_size(
                    xf.pt(x0_r, y_m + i as f32 * rh + row_off[i]),
                    Vec2::new(n_r as f32 * cw, rh) * k,
                ),
                0.0,
                wires[*wi].color.gamma_multiply(0.1 * row_al[i]),
            );
        }
    }
    // the rows a hovered invariant names
    let inv_hover: Option<usize> = c.pointer_pos().and_then(|p| {
        let (fx, fy) = ((p.x - xf.origin.x) / k, (p.y - xf.origin.y) / k);
        if fx < x_region_r || fx > x0_r + n_r as f32 * cw + verd {
            return None;
        }
        let i = ((fy - (y_inv0 + 4.0)) / inv_row_h).floor();
        (i >= 0.0 && (i as usize) < invs.len() && fy >= y_inv0 + 4.0).then_some(i as usize)
    });
    if let Some(ii) = inv_hover {
        for (p, _) in invs[ii]["y"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|e| Some((s(&e[0]).to_string(), e[1].as_i64()?)))
        {
            if let Some(i) = fplaces.iter().position(|x| s(&x["id"]) == p) {
                y_rows.push(i);
            }
        }
    }
    for i in &y_rows {
        c.p.rect_filled(
            Rect::from_min_size(
                xf.pt(x0_r - 2.0, y_m + *i as f32 * rh),
                Vec2::new(n_r as f32 * cw + 4.0, rh) * k,
            ),
            0.0,
            hex("#f2c94c").gamma_multiply(0.32),
        );
    }
    grid(c, &xf, x0_r, y_m, n_r, m_r, cw, rh);
    for (i, p) in fplaces.iter().enumerate() {
        for (j, t) in ftrans.iter().enumerate() {
            let e = ent_r
                .get(&(s(&p["id"]).to_string(), s(&t["id"]).to_string()))
                .unwrap_or(&none);
            let v = cv_r(i, j);
            let a = col_al[j] * row_al[i];
            if a <= 0.01 {
                continue;
            }
            let rect = Rect::from_min_size(
                xf.pt(
                    x0_r + j as f32 * cw + col_off[j],
                    y_m + i as f32 * rh + row_off[i],
                ),
                Vec2::new(cw, rh) * k,
            );
            cell(c, rect, v, e, show_val, guard_col, a);
            if hl_r == Some((i, j)) {
                let (pm, tm) = (strs(&p["members"]), strs(&t["members"]));
                let n = pm.len() * tm.len();
                let sum = member_sum(i, j);
                let w = wire_of.get(s(&p["id"])).map(|i| &wires[*i]);
                let kc = class_of.get(s(&t["id"])).map(|i| &classes[*i]);
                let _ = (w, kc);
                let mut inf = info(format!("C[{}, {}] = {}", s(&p["id"]), s(&t["id"]), fmt(v)))
                    .fact("place", s(&p["id"]))
                    .fact("transition", s(&t["id"]))
                    .fact("pre / post", format!("{} / {}", e.pre, e.post))
                    .fact("C = post − pre", fmt(v));
                if pm.len() > 1 {
                    inf = inf.fact("place members", pm.join(", "));
                }
                if tm.len() > 1 {
                    inf = inf.fact("transition members", tm.join(", "));
                }
                if n > 1 {
                    inf = inf.fact(
                        "sum of the member entries on the left",
                        format!(
                            "{} over {n} entries {}",
                            fmt(sum),
                            if sum == v {
                                "= this entry ✓"
                            } else {
                                "≠ this entry ✗"
                            }
                        ),
                    );
                }
                if !e.tests.is_empty() {
                    inf = inf.fact(
                        "read / inhibitor arcs",
                        format!(
                            "{}: move nothing, so they are not in C",
                            e.tests
                                .iter()
                                .map(|(ty, w)| format!("{ty} {w}"))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    );
                }
                if v == 0 && e.pre != 0 && e.post != 0 {
                    inf = inf.fact("self-loop", "pre = post: C records 0");
                }
                inf = inf.note("C of the flat net (bundle.incidence). Under MergeSum the arcs of fused members add, so this entry is the sum of the member entries.").cite(c_merge_arcs()).cite(c_firing());
                if !e.tests.is_empty() {
                    inf = inf.cite(c_guard());
                }
                c.hover(rect, inf);
            }
        }
    }
    brackets(c, &xf, x0_r, y_m, n_r as f32 * cw, m_r as f32 * rh);
    if let Some((i, j)) = hl_r {
        let st = Stroke::new(1.2, INKC);
        c.p.rect_stroke(
            Rect::from_min_size(
                xf.pt(x0_r, y_m + i as f32 * rh),
                Vec2::new(n_r as f32 * cw, rh) * k,
            ),
            0.0,
            st,
            StrokeKind::Middle,
        );
        c.p.rect_stroke(
            Rect::from_min_size(
                xf.pt(x0_r + j as f32 * cw, y_m),
                Vec2::new(cw, m_r as f32 * rh) * k,
            ),
            0.0,
            st,
            StrokeKind::Middle,
        );
    }
    // the old member columns (rows) slide together, then fade as the sum appears
    if let Some(mo) = &mo {
        for g in &ghosts {
            let s_t = (mo.t / 0.62).clamp(0.0, 1.0);
            let al = if mo.t > 0.62 {
                0.95 * (1.0 - (mo.t - 0.62) / 0.38)
            } else {
                0.95
            };
            let d = (g.from_idx as f32 - g.to_idx as f32)
                * if g.by_col { cw } else { rh }
                * (1.0 - s_t);
            for (r, cc, v) in &g.cells {
                let (x, y) = if g.by_col {
                    (x0_r + *cc as f32 * cw + d, y_m + *r as f32 * rh)
                } else {
                    (x0_r + *cc as f32 * cw, y_m + *r as f32 * rh + d)
                };
                let rect = Rect::from_min_size(xf.pt(x, y), Vec2::new(cw, rh) * k);
                cell(c, rect, *v, &none, show_val, guard_col, al);
            }
        }
        if let Some((p, color)) = badge {
            let f = if mo.t < 0.36 {
                0.0
            } else if mo.t < 0.52 {
                0.2 + 1.05 * ((mo.t - 0.36) / 0.16)
            } else if mo.t < 0.62 {
                1.25 - 0.25 * ((mo.t - 0.52) / 0.1)
            } else {
                1.0
            };
            let al = if mo.t > 0.85 {
                1.0 - (mo.t - 0.85) / 0.15
            } else {
                1.0
            };
            if f > 0.0 {
                c.p.circle(
                    p,
                    9.0 * k * f,
                    Color32::WHITE.gamma_multiply(al),
                    Stroke::new(1.8 * k.max(0.8), color.gamma_multiply(al)),
                );
                c.text(
                    p,
                    Align2::CENTER_CENTER,
                    "Σ",
                    12.0 * k * f,
                    color.gamma_multiply(al),
                );
            }
        }
        if mo.kind == "guard" {
            let ft = port_member(&mo.link["from"], false);
            let fp = port_member(&mo.link["to"], true);
            let t_ix =
                ft.and_then(|m| ftrans.iter().position(|t| strs(&t["members"]).contains(&m)));
            let p_ix = fp.and_then(|m| {
                fplaces
                    .iter()
                    .position(|p| strs(&p["members"]).contains(&m))
            });
            if let (Some(i), Some(j)) = (p_ix, t_ix) {
                let ctr = xf.pt(
                    x0_r + j as f32 * cw + cw / 2.0,
                    y_m + i as f32 * rh + rh / 2.0,
                );
                let rad = if mo.t < 0.5 {
                    22.0 - 13.0 * (mo.t / 0.5)
                } else {
                    9.0
                };
                let al = if mo.t > 0.7 {
                    1.0 - (mo.t - 0.7) / 0.3
                } else {
                    (mo.t / 0.25).min(1.0)
                };
                c.p.circle_stroke(
                    ctr,
                    rad * k,
                    Stroke::new(2.0 * k.max(0.8), guard_col.gamma_multiply(al)),
                );
                c.text(
                    xf.pt(
                        x0_r + n_r as f32 * cw + 10.0,
                        y_m + i as f32 * rh + rh / 2.0,
                    ),
                    Align2::LEFT_CENTER,
                    "◇ read arc added: C unchanged",
                    10.0 * k,
                    guard_col.gamma_multiply(al),
                );
            }
        }
    }
    // flat row headers
    for (i, p) in fplaces.iter().enumerate() {
        let id = s(&p["id"]);
        let w = wire_of.get(id).map(|i| &wires[*i]);
        let hl = hl_r.is_some_and(|h| h.0 == i);
        let y = y_m + i as f32 * rh + rh / 2.0 + row_off[i];
        let at = xf.pt(x0_r - 7.0, y);
        let slash = id.rfind('/').map_or(0, |x| x + 1);
        let r = if let Some(w) = w {
            c.mono(
                at,
                Align2::RIGHT_CENTER,
                id,
                col_font * k,
                w.color.gamma_multiply(row_al[i]),
            )
        } else {
            let loc = c.mono(
                at,
                Align2::RIGHT_CENTER,
                &id[slash..],
                col_font * k,
                if hl { Color32::BLACK } else { INKC }.gamma_multiply(row_al[i]),
            );
            let pr = c.mono(
                Pos2::new(loc.left(), loc.center().y),
                Align2::RIGHT_CENTER,
                &id[..slash],
                col_font * k,
                MUTEDC.gamma_multiply(row_al[i]),
            );
            pr.union(loc)
        };
        let nz = (0..n_r).filter(|j| cv_r(i, *j) != 0).count();
        let mut inf = info(format!("flat place {id}"))
            .fact("concept", "ns:Place")
            .fact("members", strs(&p["members"]).join(", "))
            .fact("initial", p["initial"].to_string());
        if let Some(w) = w {
            inf = inf.fact(
                "row of C",
                format!(
                    "{}: the sum of the {} member rows on the left (place fusion)",
                    w.tag,
                    w.members.len()
                ),
            );
        }
        inf = inf
            .fact("nonzero entries", nz.to_string())
            .cites_of(&json!([]));
        inf.cites = vec![c_merge_arcs()];
        c.hover(r, inf);
    }
    // flat column headers
    for (j, t) in ftrans.iter().enumerate() {
        let id = s(&t["id"]);
        let kc = class_of.get(id).map(|i| &classes[*i]);
        let hl = hl_r.is_some_and(|h| h.1 == j);
        let at = xf.pt(x0_r + j as f32 * cw + cw / 2.0 + 3.0 + col_off[j], y_lab);
        let al = col_al[j];
        let w = if let Some(kc) = kc {
            rot_text(
                c,
                at,
                &[(
                    format!("{} fused ({})", kc.tag, kc.members.len()),
                    kc.color.gamma_multiply(al),
                )],
                col_font * k,
            )
        } else {
            let slash = id.rfind('/').map_or(0, |x| x + 1);
            rot_text(
                c,
                at,
                &[
                    (id[..slash].to_string(), MUTEDC.gamma_multiply(al)),
                    (
                        id[slash..].to_string(),
                        if hl { Color32::BLACK } else { INKC }.gamma_multiply(al),
                    ),
                ],
                col_font * k,
            )
        };
        let end = at + Vec2::new(w * 0.5, -w * 0.866);
        let nz = (0..m_r).filter(|i| cv_r(*i, j) != 0).count();
        let mut inf = info(format!("flat transition {id}"))
            .fact("concept", "ns:Transition")
            .fact("members", strs(&t["members"]).join(", "));
        if let Some(kc) = kc {
            inf = inf.fact(
                "column of C",
                format!(
                    "{}: the sum of the {} member columns on the left (MergeSum)",
                    kc.tag,
                    kc.members.len()
                ),
            );
        }
        inf = inf.fact("nonzero entries", nz.to_string());
        inf.cites = vec![c_merge_arcs()];
        c.hover_seg(at, end, 5.0 * k.max(0.8), inf);
    }

    // ---- y·C strips under the flat matrix
    let hd = c.text(
        xf.pt(x_region_r, y_inv0 - 6.0),
        Align2::LEFT_CENTER,
        "P-invariants: y·C for each declared Σ tokens(p) == n",
        10.0 * k,
        INKC,
    );
    c.text(
        Pos2::new(hd.right() + 8.0 * k, hd.center().y),
        Align2::LEFT_CENTER,
        "read from bundle.incidence (y·C and y·M₀ over the flat net)",
        10.0 * k,
        MUTEDC,
    );
    for (kx, iv) in invs.iter().enumerate() {
        let y = y_inv0 + 4.0 + kx as f32 * inv_row_h;
        let hov = inv_hover == Some(kx);
        if hov {
            c.p.rect_filled(
                Rect::from_min_size(
                    xf.pt(x_region_r, y - 1.0),
                    Vec2::new(x0_r + n_r as f32 * cw + verd - x_region_r - 4.0, rh + 2.0) * k,
                ),
                3.0 * k,
                hex("#eef2f7"),
            );
        }
        let id = s(&iv["id"]);
        c.mono(
            xf.pt(x0_r - 7.0, y + rh / 2.0),
            Align2::RIGHT_CENTER,
            id,
            10.0 * k,
            INKC,
        );
        let parsed = iv["holds"] != Value::Null || iv["y_c"].is_array();
        let sid = id.split('/').next().unwrap_or("");
        let local = id.split_once('/').map_or(id, |x| x.1);
        let mut inf = info(format!("y·C for {id}")).fact("expression", s(&iv["expr"]));
        if let Some(concept) = invariant_concept(c, sid, local) {
            inf = inf
                .fact(
                    "concept",
                    format!("ns:PatternInvariant ({})", s(&concept["key"])),
                )
                .fact("kind", s(&concept["kind"]))
                .fact("as the ontology states it", s(&concept["expr"]));
            inf = inf.cites_of(&concept["cites"]);
        }
        inf = inf.fact("status here", "checked by linear algebra on the flat net (bundle.incidence); go-pflow verify and sim_verify are the engines of record");
        if parsed && iv["y_c"].is_array() {
            let y_c: Vec<i64> = arr(&iv["y_c"])
                .iter()
                .map(|x| x.as_i64().unwrap_or(0))
                .collect();
            c.p.rect(
                Rect::from_min_size(xf.pt(x0_r, y), Vec2::new(n_r as f32 * cw, rh) * k),
                0.0,
                Color32::WHITE,
                Stroke::new(0.8, hex("#d5dae0")),
                StrokeKind::Middle,
            );
            for (j, v) in y_c.iter().enumerate() {
                if *v == 0 {
                    continue;
                }
                let r = Rect::from_min_size(
                    xf.pt(x0_r + j as f32 * cw + 0.5, y + 0.5),
                    Vec2::new(cw - 1.0, rh - 1.0) * k,
                );
                c.p.rect_filled(r, 0.0, cell_fill(*v));
                if show_val {
                    c.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        &fmt(*v),
                        9.0 * k,
                        Color32::WHITE,
                    );
                }
            }
            let n = iv["n"].as_i64().unwrap_or(0);
            let ym0 = iv["y_m0"].as_i64().unwrap_or(0);
            let bad = y_c.iter().filter(|v| **v != 0).count();
            let (ok_c, ok_m) = (bad == 0, ym0 == n);
            let tx = x0_r + n_r as f32 * cw + 8.0;
            let ok_col = hex("#1e7a46");
            let bad_col = hex("#c0392b");
            let a = c.text(
                xf.pt(tx, y + rh / 2.0),
                Align2::LEFT_CENTER,
                &if ok_c {
                    "y·C = 0 ✓".to_string()
                } else {
                    format!("y·C ≠ 0 at {bad} ✗")
                },
                11.0 * k,
                if ok_c { ok_col } else { bad_col },
            );
            let b = c.text(
                Pos2::new(a.right() + 8.0 * k, a.center().y),
                Align2::LEFT_CENTER,
                "y·M₀ =",
                11.0 * k,
                MUTEDC,
            );
            c.text(
                Pos2::new(b.right() + 3.0 * k, a.center().y),
                Align2::LEFT_CENTER,
                &if ok_m {
                    format!("{ym0} ✓")
                } else {
                    format!("{ym0} ≠ {n} ✗")
                },
                11.0 * k,
                if ok_m { ok_col } else { bad_col },
            );
            let coeffs: Vec<String> = arr(&iv["y"])
                .iter()
                .map(|e| {
                    if e[1].as_i64() == Some(1) {
                        s(&e[0]).to_string()
                    } else {
                        format!("{}·{}", e[1], s(&e[0]))
                    }
                })
                .collect();
            inf = inf.fact("y (from the expression)", coeffs.join(" + "));
            let cols: Vec<String> = y_c
                .iter()
                .enumerate()
                .filter(|(_, v)| **v != 0)
                .map(|(j, v)| {
                    let terms: Vec<String> = arr(&iv["y"])
                        .iter()
                        .filter_map(|e| {
                            let pi = fplaces.iter().position(|p| s(&p["id"]) == s(&e[0]))?;
                            let x = cv_r(pi, j);
                            (x != 0).then(|| {
                                format!(
                                    "{}{}",
                                    if e[1].as_i64() == Some(1) {
                                        String::new()
                                    } else {
                                        format!("{}·", e[1])
                                    },
                                    fmt(x)
                                )
                            })
                        })
                        .collect();
                    format!("{}: {} = {}", s(&ftrans[j]["id"]), terms.join(" "), fmt(*v))
                })
                .collect();
            inf = inf
                .fact("y·C", if cols.is_empty() { format!("0 in all {n_r} columns") } else { cols.join("; ") })
                .fact("y·M₀", format!("{ym0} (declared {n})"))
                .fact(
                    "verdict",
                    if bad > 0 {
                        format!("not a P-invariant of the flat net: {bad} transition{} change the sum{}", if bad == 1 { "" } else { "s" }, if arr(&iv["y"]).iter().any(|e| s(&e[0]).starts_with("wire:")) { " (a fused place's row also carries the other instance's arcs)" } else { "" })
                    } else if ok_m {
                        "holds in every reachable marking of this flat net (y·C = 0 and y·M₀ = n)".to_string()
                    } else {
                        "y is still a P-invariant, but y·M₀ differs from the declared constant, so the constraint is false from the start".to_string()
                    },
                );
            if arr(&iv["y"]).iter().any(|e| e[1].as_i64().unwrap_or(0) > 1) {
                inf = inf.fact(
                    "note",
                    "after fusion two terms name one place, so its coefficient is the count",
                );
            }
        } else {
            c.text(
                xf.pt(x0_r, y + rh / 2.0),
                Align2::LEFT_CENTER,
                "not a plain sum of tokens(…) == n: not checked here",
                10.0 * k,
                MUTEDC,
            );
            inf = inf.fact("checked", "no: the expression is not a plain sum of tokens(…) == n, or names a place the net lacks");
        }
        inf = inf.note("From bundle.incidence: y·C over the matrix above and y·M₀ over its initial marking. It is not a run of go-pflow verify; capacities, read arcs and inhibitors only restrict firing, so they cannot break a P-invariant.").cite(c_structural()).cite(c_merge_arcs()).cite(c_fire());
        let rr = Rect::from_min_max(
            xf.pt(x_region_r, y - 1.0),
            xf.pt(x0_r + n_r as f32 * cw + verd - 4.0, y + rh + 1.0),
        );
        c.hover(rr, inf);
        let holds = iv["holds"] == true;
        c.scene.emit("invariant", id, json!({"holds": holds}));
    }

    // ---- bottom legend
    let mut y = y_bot;
    let mut x = m_;
    let mut sw = |c: &mut Ctx, fill: Color32, label: &str, extra: Extra| {
        let r = Rect::from_min_size(xf.pt(x, y - 9.0), Vec2::new(14.0, 11.0) * k);
        c.p.rect(
            r,
            0.0,
            fill,
            if fill == Color32::TRANSPARENT {
                Stroke::new(1.0, hex("#c3cbd5"))
            } else {
                Stroke::NONE
            },
            StrokeKind::Middle,
        );
        if let Some(f) = extra {
            f(c, xf.pt(x + 7.0, y - 3.5));
        }
        c.text(
            xf.pt(x + 19.0, y - 3.5),
            Align2::LEFT_CENTER,
            label,
            10.5 * k,
            MUTEDC,
        );
        x += 19.0 + label.chars().count() as f32 * 5.7 + 16.0;
    };
    sw(c, NEG, "−w: t consumes w from p (pre)", None);
    sw(c, POS, "+w: t produces w into p (post)", None);
    sw(c, Color32::TRANSPARENT, "blank: 0", None);
    sw(
        c,
        Color32::TRANSPARENT,
        "read arc (guard link): moves nothing, not in C",
        Some(&|c, p| {
            let pts = vec![
                p + Vec2::new(0.0, -4.0) * k,
                p + Vec2::new(4.0, 0.0) * k,
                p + Vec2::new(0.0, 4.0) * k,
                p + Vec2::new(-4.0, 0.0) * k,
            ];
            c.p.add(Shape::convex_polygon(
                pts,
                Color32::TRANSPARENT,
                Stroke::new(1.4, guard_col),
            ));
        }),
    );
    sw(
        c,
        Color32::TRANSPARENT,
        "pre = post: C is 0",
        Some(&|c, p| {
            c.p.circle_stroke(p, 3.0 * k, Stroke::new(1.2, MUTEDC));
        }),
    );
    y += 16.0;
    for cl in &classes {
        let a = c.text(
            xf.pt(m_, y),
            Align2::LEFT_CENTER,
            &cl.tag,
            10.0 * k,
            cl.color,
        );
        let b = c.mono(
            Pos2::new(a.right() + 6.0 * k, a.center().y),
            Align2::LEFT_CENTER,
            &cl.name,
            10.0 * k,
            INKC,
        );
        let d = c.text(Pos2::new(b.right() + 6.0 * k, a.center().y), Align2::LEFT_CENTER, &format!("column = sum of {} member columns (event links; MergeSum, compose_flatten.go:335-411)", cl.members.len()), 10.5 * k, MUTEDC);
        c.hover(a.union(d), class_info(cl));
        y += 16.0;
    }
    if !classes.is_empty() {
        c.text(xf.pt(m_, y), Align2::LEFT_CENTER, &format!("{}: event classes, tagged with RenderDOT's event-link glyph ⊗; not the monoidal product ⊗ in the left title", classes.iter().map(|c| c.tag.clone()).collect::<Vec<_>>().join(", ")), 10.5 * k, MUTEDC);
        y += 16.0;
    }
    for w in &wires {
        let a = c.text(xf.pt(m_, y), Align2::LEFT_CENTER, &w.tag, 10.0 * k, w.color);
        let b = c.mono(
            Pos2::new(a.right() + 6.0 * k, a.center().y),
            Align2::LEFT_CENTER,
            &w.name,
            10.0 * k,
            INKC,
        );
        let d = c.text(Pos2::new(b.right() + 6.0 * k, a.center().y), Align2::LEFT_CENTER, &format!("row = sum of {} member rows ({} link{}: place fusion, a pushout along the shared place)", w.members.len(), w.kind, if w.links.len() == 1 { "" } else { "s" }), 10.5 * k, MUTEDC);
        c.hover(a.union(d), wire_info(w));
        y += 16.0;
    }
    if guards > 0 {
        c.text(xf.pt(m_, y), Align2::LEFT_CENTER, &format!("{guards} guard link{}: read arc{} drawn as ◇ on the flat matrix; C is the same with or without {}", if guards == 1 { "" } else { "s" }, if guards == 1 { "" } else { "s" }, if guards == 1 { "it" } else { "them" }), 10.5 * k, MUTEDC);
        y += 16.0;
    }
    let a = c.text(
        xf.pt(m_, y),
        Align2::LEFT_CENTER,
        &format!("{agree}/{checked}"),
        10.5 * k,
        if agree == checked {
            hex("#1e7a46")
        } else {
            hex("#c0392b")
        },
    );
    c.text(Pos2::new(a.right() + 5.0 * k, a.center().y), Align2::LEFT_CENTER, "flat entries equal the sum of their member entries on the left (checked here: composition acts on C by adding rows and columns)", 10.5 * k, MUTEDC);

    // scene manifest
    for b in arr(&inc["blocks"]) {
        c.scene.emit(
            "block",
            s(&b["subnet"]),
            json!({"rows": arr(&b["rows"]).len(), "cols": arr(&b["cols"]).len()}),
        );
    }
    c.scene.emit(
        "matrix",
        "flat",
        json!({"rows": arr(&inc["rows"]).len(), "cols": arr(&inc["cols"]).len()}),
    );
}

// ------------------------------------------------------------ connect4

pub(super) fn incidence_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, _) = net_counts(c);
    c.scene
        .emit("matrix", "connect-4", json!({"rows": pl, "cols": tr}));
    c.note("na", "A SearchPlan has no incidence matrix: it is a composite of search pieces (plan.rs), not a P/T net, and its pieces have no places or transitions to put in one. Drawn instead, as the nearest honest thing: the incidence matrix of the Connect-4 net the plan searches (examples/connect-4.json, the pflow-rs fixture). Nothing here is a plan slot; pieces are placed in the plan views.");
    let (Some(doc), Ok(model)) = (c.d.c4.as_ref(), sp::connect4_model()) else {
        return;
    };
    let cmat = comp::incidence(model);
    let prow: HashMap<&str, usize> = model
        .places
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.as_str(), i))
        .collect();
    let tcol: HashMap<&str, usize> = model
        .transitions
        .iter()
        .enumerate()
        .map(|(i, t)| (t.id.as_str(), i))
        .collect();
    // pre, post, inhibitor weight per (place, transition), from the fixture's arcs
    let pset: BTreeSet<&str> = doc.places.iter().map(|p| p.id.as_str()).collect();
    let tset: BTreeSet<&str> = doc.transitions.iter().map(|t| t.id.as_str()).collect();
    let mut ents: HashMap<(usize, usize), (i64, i64, i64)> = HashMap::new();
    let (mut n_inhib, mut n_other) = (0usize, 0usize);
    for a in &doc.arcs {
        let w = if a.weight.is_empty() {
            1
        } else {
            a.weight.iter().sum::<f64>() as i64
        };
        let (p, t) = if pset.contains(a.source.as_str()) {
            (&a.source, &a.target)
        } else {
            (&a.target, &a.source)
        };
        if a.inhibit {
            n_inhib += 1;
            if let (Some(pi), Some(ti)) = (prow.get(p.as_str()), tcol.get(t.as_str())) {
                ents.entry((*pi, *ti)).or_default().2 = w;
            }
            continue;
        }
        if pset.contains(a.source.as_str()) && tset.contains(a.target.as_str()) {
            if let (Some(pi), Some(ti)) = (prow.get(a.source.as_str()), tcol.get(a.target.as_str()))
            {
                ents.entry((*pi, *ti)).or_default().0 += w;
            }
        } else if tset.contains(a.source.as_str()) && pset.contains(a.target.as_str()) {
            if let (Some(pi), Some(ti)) = (prow.get(a.target.as_str()), tcol.get(a.source.as_str()))
            {
                ents.entry((*pi, *ti)).or_default().1 += w;
            }
        } else {
            n_other += 1;
        }
    }
    let at = |p: &str, t: &str| -> (i64, i64, i64, i64) {
        let (pi, ti) = (prow[p], tcol[t]);
        let e = ents.get(&(pi, ti)).copied().unwrap_or_default();
        (e.0, e.1, e.2, cmat[pi][ti])
    };
    // groups by ID prefix (this page's grouping; the fixture has none)
    let cell_re = |id: &str, pre: &str| -> Option<(usize, usize)> {
        let r = id.strip_prefix(pre)?;
        let mut it = r.split('_');
        let (a, b) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?);
        it.next().is_none().then_some((a, b))
    };
    struct G {
        label: &'static str,
        what: &'static str,
        items: Vec<String>,
        subs: Vec<(String, Vec<String>)>,
        start: usize,
        a0: f32,
        a1: f32,
    }
    let sub_of = |id: &str| -> String {
        for pre in ["open_", "x_", "o_"] {
            if let Some((c_, _)) = cell_re(id, pre) {
                return format!("c{c_}");
            }
        }
        for pre in ["x_play_", "o_play_"] {
            if let Some((c_, _)) = cell_re(id, pre) {
                return format!("c{c_}");
            }
        }
        for pre in ["x_win_", "o_win_"] {
            if let Some(r) = id.strip_prefix(pre) {
                return r.split('_').next().unwrap_or("").to_string();
            }
        }
        String::new()
    };
    let mut row_groups: Vec<G> = [
        ("open_c_r", "", "open_"),
        ("x_c_r", "", "x_"),
        ("o_c_r", "", "o_"),
        ("other", "", ""),
    ]
    .iter()
    .map(|(l, w, _)| G {
        label: l,
        what: w,
        items: vec![],
        subs: vec![],
        start: 0,
        a0: 0.0,
        a1: 0.0,
    })
    .collect();
    for p in &doc.places {
        let i = ["open_", "x_", "o_"]
            .iter()
            .position(|pre| cell_re(&p.id, pre).is_some())
            .unwrap_or(3);
        row_groups[i].items.push(p.id.clone());
    }
    let mut col_groups: Vec<G> = [
        ("x_play_c_r", "X drops"),
        ("o_play_c_r", "O drops"),
        ("x_win_*", "X win detectors"),
        ("o_win_*", "O win detectors"),
        ("other", "other transitions"),
    ]
    .iter()
    .map(|(l, w)| G {
        label: l,
        what: w,
        items: vec![],
        subs: vec![],
        start: 0,
        a0: 0.0,
        a1: 0.0,
    })
    .collect();
    for t in &doc.transitions {
        let i = if cell_re(&t.id, "x_play_").is_some() {
            0
        } else if cell_re(&t.id, "o_play_").is_some() {
            1
        } else if t.id.starts_with("x_win_") {
            2
        } else if t.id.starts_with("o_win_") {
            3
        } else {
            4
        };
        col_groups[i].items.push(t.id.clone());
    }
    for g in row_groups.iter_mut().chain(col_groups.iter_mut()) {
        let mut order: Vec<String> = vec![];
        for id in &g.items {
            let s_ = sub_of(id);
            if !order.contains(&s_) {
                order.push(s_);
            }
        }
        g.items
            .sort_by_key(|id| order.iter().position(|o| *o == sub_of(id)).unwrap_or(0));
        g.subs = order
            .iter()
            .map(|o| {
                (
                    o.clone(),
                    g.items
                        .iter()
                        .filter(|id| sub_of(id) == *o)
                        .cloned()
                        .collect(),
                )
            })
            .collect();
    }
    let mut rg: Vec<G> = row_groups
        .into_iter()
        .filter(|g| !g.items.is_empty())
        .collect();
    let mut cg: Vec<G> = col_groups
        .into_iter()
        .filter(|g| !g.items.is_empty())
        .collect();
    // layout
    let (m_, ax, gg, cw, rh) = (12.0f32, 112.0f32, 3.0f32, 4.5f32, 4.5f32);
    let (x0, y0) = (m_ + ax, 86.0f32);
    let mut rows: Vec<(String, f32, usize)> = vec![]; // (id, y, group)
    let mut cols: Vec<(String, f32, usize)> = vec![];
    let mut x = x0;
    for (gi, g) in cg.iter_mut().enumerate() {
        if gi > 0 {
            x += gg;
        }
        g.a0 = x;
        g.start = cols.len();
        for t in &g.items {
            cols.push((t.clone(), x, gi));
            x += cw;
        }
        g.a1 = x;
    }
    let mut y = y0;
    for (gi, g) in rg.iter_mut().enumerate() {
        if gi > 0 {
            y += gg;
        }
        g.a0 = y;
        g.start = rows.len();
        for p in &g.items {
            rows.push((p.clone(), y, gi));
            y += rh;
        }
        g.a1 = y;
    }
    let (x_end, y_end) = (x, y);
    let w_total = 1150f32.max(x_end + m_);
    let h_total = y_end + 104.0;
    let xf = Xf::fit(area, w_total, h_total, 1.8);
    let k = xf.k;
    // counts
    let stats = |ps: &[String], ts: &[String]| -> (usize, usize, usize, usize) {
        let (mut nz, mut neg, mut pos, mut cat) = (0, 0, 0, 0);
        for p in ps {
            for t in ts {
                let (pre, post, _, v) = at(p, t);
                if v != 0 {
                    nz += 1;
                    if v < 0 {
                        neg += 1
                    } else {
                        pos += 1
                    }
                } else if pre != 0 && post != 0 {
                    cat += 1;
                }
            }
        }
        (nz, neg, pos, cat)
    };
    let all_p: Vec<String> = doc.places.iter().map(|p| p.id.clone()).collect();
    let all_t: Vec<String> = doc.transitions.iter().map(|t| t.id.clone()).collect();
    let (nz, neg, pos, cat) = stats(&all_p, &all_t);
    c.text(
        xf.pt(m_, 18.0),
        Align2::LEFT_CENTER,
        &format!(
            "C of the Connect-4 net: {} places × {} transitions",
            all_p.len(),
            all_t.len()
        ),
        13.0 * k,
        INKC,
    );
    let name = doc
        .extra
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("connect-4")
        .to_string();
    c.text(xf.pt(m_, 34.0), Align2::LEFT_CENTER, &format!("{name} · examples/connect-4.json (pflow-rs fixture) · grouped by ID prefix on this page"), 11.0 * k, MUTEDC);
    // block frames and sub-group separators
    for r in &rg {
        for cc in &cg {
            c.p.rect(
                Rect::from_min_max(xf.pt(cc.a0, r.a0), xf.pt(cc.a1, r.a1)),
                0.0,
                Color32::WHITE,
                Stroke::new(0.7, hex("#dde1e6")),
                StrokeKind::Middle,
            );
        }
    }
    let sep = Stroke::new(0.6, hex("#eef0f3"));
    for cc in &cg {
        let mut xx = cc.a0;
        for (si, (_, items)) in cc.subs.iter().enumerate() {
            if si > 0 {
                c.p.line_segment([xf.pt(xx, y0), xf.pt(xx, y_end)], sep);
            }
            xx += items.len() as f32 * cw;
        }
    }
    for r in &rg {
        let mut yy = r.a0;
        for (si, (_, items)) in r.subs.iter().enumerate() {
            if si > 0 {
                c.p.line_segment([xf.pt(x0, yy), xf.pt(x_end, yy)], sep);
            }
            yy += items.len() as f32 * rh;
        }
    }
    // cells
    let guard_col = kind_color("guard");
    for (p, py, _) in &rows {
        for (t, tx, _) in &cols {
            let (pre, post, inhib, v) = at(p, t);
            if v == 0 && pre == 0 && post == 0 && inhib == 0 {
                continue;
            }
            let r = Rect::from_min_size(
                xf.pt(*tx + 0.3, *py + 0.3),
                Vec2::new(cw - 0.6, rh - 0.6) * k,
            );
            if v != 0 {
                c.p.rect_filled(r, 0.0, cell_fill(v));
            } else if pre != 0 && post != 0 {
                c.p.circle_filled(
                    xf.pt(*tx + cw / 2.0, *py + rh / 2.0),
                    1.15 * k,
                    hex("#9aa1ab"),
                );
            }
            if inhib != 0 {
                let (a, b) = (
                    xf.pt(*tx + 0.6, *py + 0.6),
                    xf.pt(*tx + cw - 0.6, *py + rh - 0.6),
                );
                c.p.line_segment([a, b], Stroke::new(0.9, guard_col));
                c.p.line_segment(
                    [Pos2::new(b.x, a.y), Pos2::new(a.x, b.y)],
                    Stroke::new(0.9, guard_col),
                );
            }
        }
    }
    // axes: group labels and sub-group ticks
    let fixture = pflow_rs("incidence.fixture");
    let desc = doc
        .extra
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let group_info = |title: String, ids: &[String], ps: &[String], ts: &[String]| -> Info {
        let (nz, neg, pos, cat) = stats(ps, ts);
        info(title)
            .fact("count", ids.len().to_string())
            .fact("IDs", if ids.len() <= 8 { ids.join(", ") } else { format!("{} … {}", ids[..3].join(", "), ids[ids.len() - 1]) })
            .fact("nonzero entries", format!("{nz} ({neg} × −1, {pos} × +1)"))
            .fact("pre = post pairs (C = 0)", cat.to_string())
            .fact("description", desc.clone())
            .note("Grouped by ID prefix on this page; the fixture has no groups. Counts computed here from examples/connect-4.json.")
            .cite(fixture.clone())
            .cite(c_inc_go())
    };
    for g in &rg {
        let mid = (g.a0 + g.a1) / 2.0;
        let tall = g.a1 - g.a0 > 26.0;
        let r = c.mono(
            xf.pt(m_, if tall { mid - 2.0 } else { mid + 3.5 - 3.5 }),
            Align2::LEFT_CENTER,
            g.label,
            10.0 * k,
            INKC,
        );
        c.hover(
            r.expand2(Vec2::new(0.0, 2.0)),
            group_info(
                format!("places {} ({})", g.label, g.items.len()),
                &g.items,
                &g.items,
                &all_t,
            ),
        );
        if tall {
            c.text(
                xf.pt(m_, mid + 11.0),
                Align2::LEFT_CENTER,
                &format!("{} places", g.items.len()),
                9.5 * k,
                MUTEDC,
            );
        } else {
            c.text(
                Pos2::new(r.right() + 6.0 * k, r.center().y),
                Align2::LEFT_CENTER,
                &format!("({})", g.items.len()),
                9.5 * k,
                MUTEDC,
            );
        }
        if g.subs.len() > 1 {
            let mut yy = g.a0;
            for (key, items) in &g.subs {
                let ym = yy + items.len() as f32 * rh / 2.0;
                let t = c.text(
                    xf.pt(x0 - 4.0, ym),
                    Align2::RIGHT_CENTER,
                    key,
                    7.5 * k,
                    MUTEDC,
                );
                c.hover(
                    t.expand2(Vec2::new(0.0, 1.0)),
                    group_info(
                        format!(
                            "places {}, column {} ({})",
                            g.label,
                            key.trim_start_matches('c'),
                            items.len()
                        ),
                        items,
                        items,
                        &all_t,
                    ),
                );
                yy += items.len() as f32 * rh;
            }
        }
    }
    for g in &cg {
        let mid = (g.a0 + g.a1) / 2.0;
        let a = c.mono(
            xf.pt(mid, y0 - 26.0),
            Align2::CENTER_CENTER,
            g.label,
            10.0 * k,
            INKC,
        );
        let b = c.text(
            Pos2::new(a.right() + 5.0 * k, a.center().y),
            Align2::LEFT_CENTER,
            &format!("{} {}", g.items.len(), g.what),
            9.5 * k,
            MUTEDC,
        );
        c.hover(
            a.union(b),
            group_info(
                format!("transitions {}: {} ({})", g.label, g.what, g.items.len()),
                &g.items,
                &all_p,
                &g.items,
            ),
        );
        let st = Stroke::new(1.0, hex("#b5bcc5"));
        c.p.add(Shape::line(
            vec![
                xf.pt(g.a0 + 1.0, y0 - 18.0),
                xf.pt(g.a0 + 1.0, y0 - 15.0),
                xf.pt(g.a1 - 1.0, y0 - 15.0),
                xf.pt(g.a1 - 1.0, y0 - 18.0),
            ],
            st,
        ));
        if g.subs.len() > 1 {
            let mut xx = g.a0;
            for (key, items) in &g.subs {
                let sm = xx + items.len() as f32 * cw / 2.0;
                let is_dir = !(key.starts_with('c')
                    && key[1..].chars().all(|ch| ch.is_ascii_digit())
                    && key.len() > 1);
                let label = if is_dir {
                    format!("{key} {}", items.len())
                } else {
                    key.clone()
                };
                let t = c.text(
                    xf.pt(sm, y0 - 5.0),
                    Align2::CENTER_CENTER,
                    &label,
                    7.5 * k,
                    MUTEDC,
                );
                c.hover(
                    t.expand2(Vec2::new(1.0, 1.0)),
                    group_info(
                        format!(
                            "transitions {}, {} ({})",
                            g.label,
                            if is_dir {
                                format!("direction {key}")
                            } else {
                                format!("column {}", &key[1..])
                            },
                            items.len()
                        ),
                        items,
                        &all_p,
                        items,
                    ),
                );
                xx += items.len() as f32 * cw;
            }
        }
    }
    // hover: the cell under the pointer, with its row and column lit and a readout
    let mut readout = ("hover a cell".to_string(), MUTEDC);
    if let Some(p) = c.pointer_pos() {
        let (fx, fy) = ((p.x - xf.origin.x) / k, (p.y - xf.origin.y) / k);
        let col = cg
            .iter()
            .find(|g| fx >= g.a0 && fx < g.a1)
            .and_then(|g| cols.get(g.start + ((fx - g.a0) / cw) as usize));
        let row = rg
            .iter()
            .find(|g| fy >= g.a0 && fy < g.a1)
            .and_then(|g| rows.get(g.start + ((fy - g.a0) / rh) as usize));
        if let (Some(col), Some(row)) = (col, row) {
            let (pre, post, inhib, v) = at(&row.0, &col.0);
            c.p.rect_filled(
                Rect::from_min_max(xf.pt(x0, row.1), xf.pt(x_end, row.1 + rh)),
                0.0,
                INKC.gamma_multiply(0.07),
            );
            c.p.rect_filled(
                Rect::from_min_max(xf.pt(col.1, y0), xf.pt(col.1 + cw, y_end)),
                0.0,
                INKC.gamma_multiply(0.07),
            );
            c.p.rect_stroke(
                Rect::from_min_size(xf.pt(col.1, row.1), Vec2::new(cw, rh) * k),
                0.0,
                Stroke::new(0.9, INKC),
                StrokeKind::Middle,
            );
            readout = (
                format!(
                    "{} × {}: C = {} (pre {pre}, post {post}){}",
                    row.0,
                    col.0,
                    fmt(v),
                    if v == 0 && pre != 0 && post != 0 {
                        ", a catalytic read"
                    } else {
                        ""
                    }
                ),
                if v < 0 {
                    NEG
                } else if v > 0 {
                    POS
                } else {
                    INKC
                },
            );
            let init = doc
                .place(&row.0)
                .and_then(|p| p.initial.first().copied())
                .unwrap_or(0.0);
            let mut inf = info(format!("C[{}, {}] = {}", row.0, col.0, fmt(v)))
                .fact("place", format!("{} (initial {init})", row.0))
                .fact("transition", col.0.clone())
                .fact(
                    "pre (arc place → transition)",
                    if pre != 0 {
                        pre.to_string()
                    } else {
                        "none".into()
                    },
                )
                .fact(
                    "post (arc transition → place)",
                    if post != 0 {
                        post.to_string()
                    } else {
                        "none".into()
                    },
                )
                .fact("C = post − pre", fmt(v));
            if v == 0 && pre != 0 && post != 0 {
                inf = inf.fact("pre = post", "the transition takes the token and puts it back (a catalytic read): in the net, not in C");
            }
            if inhib != 0 {
                inf = inf.fact(
                    "inhibitTransition arc",
                    format!("weight {inhib}: excluded from C"),
                );
            }
            inf = inf
                .note("Computed from examples/connect-4.json as go-pflow IncidenceMatrix does: each arc's weight is the sum of its weight vector (GetWeightSum; this fixture has one token color and every weight is [1]), and inhibitTransition arcs are left out. pflow-rs incidence_matrix applies the same rule to its metamodel Model; the entry is core's (plan.net's model).")
                .cite(c_inc_go())
                .cite(c_weight_go())
                .cite(pflow_rs("incidence.incRs"))
                .cite(pflow_rs("incidence.inhibRs"))
                .cite(pflow_rs("incidence.rawArc"))
                .cite(pflow_rs("incidence.rawWeight"))
                .cite(pflow_rs("incidence.weightRs"))
                .cite(fixture.clone());
            c.hover(
                Rect::from_min_size(xf.pt(col.1, row.1), Vec2::new(cw, rh) * k),
                inf,
            );
        }
    }
    c.text(
        xf.pt(x_end, 34.0),
        Align2::RIGHT_CENTER,
        &readout.0,
        10.0 * k,
        readout.1,
    );
    // legend and computed summary
    let (mut lx, mut ly) = (m_, y_end + 26.0);
    let mut sw = |c: &mut Ctx, label: String, draw: &dyn Fn(&Ctx, Pos2)| {
        draw(c, xf.pt(lx, ly - 9.0));
        c.text(
            xf.pt(lx + 18.0, ly - 3.5),
            Align2::LEFT_CENTER,
            &label,
            10.5 * k,
            MUTEDC,
        );
        lx += 18.0 + label.chars().count() as f32 * 5.7 + 18.0;
    };
    let swatch = |c: &Ctx, p: Pos2, fill: Color32| {
        c.p.rect(
            Rect::from_min_size(p, Vec2::new(12.0, 11.0) * k),
            0.0,
            fill,
            if fill == Color32::WHITE {
                Stroke::new(1.0, hex("#c3cbd5"))
            } else {
                Stroke::NONE
            },
            StrokeKind::Middle,
        );
    };
    sw(c, "−1: consumes (pre)".into(), &|c, p| swatch(c, p, NEG));
    sw(c, "+1: produces (post)".into(), &|c, p| swatch(c, p, POS));
    sw(c, "blank: 0".into(), &|c, p| swatch(c, p, Color32::WHITE));
    sw(
        c,
        "pre = post = 1 (a win detector reads a cell): in the net, C = 0".into(),
        &|c, p| {
            swatch(c, p, Color32::WHITE);
            c.p.circle_filled(p + Vec2::new(6.0, 5.5) * k, 1.8 * k, hex("#9aa1ab"));
        },
    );
    sw(
        c,
        format!("inhibitTransition arc: left out of C ({n_inhib} here)"),
        &|c, p| {
            c.p.line_segment(
                [p + Vec2::new(1.0, 1.0) * k, p + Vec2::new(11.0, 10.0) * k],
                Stroke::new(1.4, guard_col),
            );
            c.p.line_segment(
                [p + Vec2::new(11.0, 1.0) * k, p + Vec2::new(1.0, 10.0) * k],
                Stroke::new(1.4, guard_col),
            );
        },
    );
    ly += 18.0;
    c.text(xf.pt(m_, ly - 3.5), Align2::LEFT_CENTER, &format!("{} entries · {nz} nonzero ({neg} × −1, {pos} × +1) · {cat} pre = post pairs that C cannot see · {n_inhib} inhibitTransition arcs · {} arcs{} · C is core's, the pre/post behind it computed here", thousands(all_p.len() * all_t.len()), doc.arcs.len(), if n_other > 0 { format!(" ({n_other} not place↔transition, ignored)") } else { String::new() }), 10.5 * k, MUTEDC);
    ly += 16.0;
    c.text(xf.pt(m_, ly - 3.5), Align2::LEFT_CENTER, "The fixture declares no invariants, so no y·C check is drawn. Rows and columns are grouped by prefix here; go-pflow and pflow-rs sort them by ID (a permutation of the same C).", 10.5 * k, MUTEDC);
    let _ = (&mut rg, &mut cg);
}

//! The boundaries view (see `docs/composition-views-scene.md`): nets with
//! boundaries (Sobociński 2010; Bruni, Melgratti, Montanari, Sobociński 2013).
//! Each instance is a net N : k → l whose transition ports sit on an outer
//! boundary or face one shared boundary; each event class (Flatten's fused
//! group) is one boundary port, and the composite transition is the card
//! beyond it. Port of the prototype's `js/views/boundaries.js`.

use super::d_bnd_layout::{self as bl, *};
use super::d_draw::*;
use super::pflow_rs_cites::pflow_rs;
use super::*;

fn c_rendezvous() -> String {
    go("metamodel/compose_fuse.go", 11, 14, "an event class becomes one transition: pre/postset is the union, guard the conjunction; a rendezvous, enabled only when every participant is")
}
fn c_classes() -> String {
    go(
        "metamodel/compose_fuse.go",
        16,
        20,
        "fusion is over equivalence classes, not pairs, so it is associative",
    )
}
fn c_every_member() -> String {
    go("metamodel/compose_fuse.go", 166, 171, "\"A rendezvous is enabled only when every participant is\", so an unrepresentable guard on any member marks the fused transition")
}
fn c_trans_classes() -> String {
    go(
        "metamodel/compose_flatten.go",
        72,
        88,
        "Flatten: transition classes, merged by EventLink (union-find)",
    )
}
fn c_union_find() -> String {
    go(
        "metamodel/unionfind.go",
        5,
        14,
        "the smallest member names a class, so link order does not matter",
    )
}
fn c_flat_name() -> String {
    go(
        "metamodel/compose_flatten.go",
        211,
        227,
        "flatTransitionName: \"fused:\" + sorted members joined by \"+\"",
    )
}
fn c_why_not() -> String {
    go(
        "metamodel/firing.go",
        142,
        196,
        "EnabledWhyNot: why a transition cannot fire at a marking",
    )
}
fn c_guard_lower() -> String {
    go("metamodel/compose_flatten.go", 424, 468, "applyGuardLinks: a structural guard becomes a read (or inhibitor) arc on the gated flat transition")
}
fn c_model_no_ports() -> String {
    go(
        "metamodel/schema.go",
        23,
        97,
        "Model, what Flatten returns: places, transitions, arcs, constraints, ...; no ports",
    )
}
fn c_subnet_ports() -> String {
    go(
        "metamodel/compose.go",
        104,
        116,
        "Subnet: a model plus its boundary (Ports)",
    )
}
fn c_lk_event() -> String {
    onto(
        "ontology",
        "ns/patterns.ttl",
        171,
        174,
        "lk:event: fuses two transitions; no port-direction rule",
    )
}
fn c_leak() -> String {
    onto(
        "ontology",
        "ns/patterns.ttl",
        493,
        498,
        "pat:idiom-pool-release-saga-commit: the pool-leak caveat",
    )
}
fn c_road_row() -> String {
    onto("ontologyRoadmap", "ROADMAP.md", 400, 400, "nets with boundaries: the semantics of event links, which a cospan of places does not express")
}
fn c_road_event() -> String {
    onto(
        "ontologyRoadmap",
        "ROADMAP.md",
        410,
        411,
        "lk:event is transition fusion; its semantics is Bruni et al.'s, not Baez–Master's",
    )
}
fn c_road_monoidal() -> String {
    onto(
        "ontologyRoadmap",
        "ROADMAP.md",
        407,
        407,
        "unlinked subnets: the monoidal product (disjoint union)",
    )
}
fn c_road_cospan() -> String {
    onto(
        "ontologyRoadmap",
        "ROADMAP.md",
        408,
        409,
        "lk:token / lk:data: a pushout along a shared place (a cospan composite)",
    )
}
fn c_road_guard() -> String {
    onto(
        "ontologyRoadmap",
        "ROADMAP.md",
        412,
        413,
        "lk:guard: a cross-subnet read arc, outside the free-SMC fragment",
    )
}
fn c_fixture() -> String {
    pflow_rs("boundaries.fixture")
}

const PAPER_DEF: &str = "Def. 3.1 / 4.1: a net with boundaries N : m → n wires each transition to ports on the left boundary m and the right boundary n";
const PAPER_SYNC: &str = "Def. 3.2 and 3.4 (C/E), Def. 4.3 and 4.6 (P/T): composing M : l → m with N : m → n, the composite's transitions are the minimal synchronisations (U, V) with U• = •V; their pre- and postsets are the sums of the members' (defined just before Def. 3.4 and Def. 4.6)";
const PAPER_SOLO: &str = "p. 12: a transition wired to no port of the shared boundary is its own minimal synchronisation ({t}, ∅)";
const PAPER_CLOSED: &str = "Def. 4.1 with m = n = 0: a P/T net with no ports is a net with boundaries N : 0 → 0 (Remark 3.6 says so for C/E nets, and that composing two of them is their disjoint union)";
const PAPER_NO_INTERACTION: &str =
    "Remark 3.11: a net N : 0 → 0 has no boundary, so there is no way of interacting with it";
const PAPER_CONTENTION: &str = "Def. 3.1 (iii)-(iv): two transitions wired to one port on the same side are in contention (cannot fire together); Def. 4.3: a P/T synchronisation needs U• = •V";

fn ev_col() -> Color32 {
    kind_color("event")
}

/// Event classes by union-find over the event links (go-pflow's rule), for
/// the layout of the state before a link was added.
fn classes_without(subs: &[Value], links: &[Value]) -> Vec<(String, Vec<String>)> {
    let member = |r: &Value| -> Option<String> {
        let sub = subs.iter().find(|x| s(&x["id"]) == s(&r["subnet"]))?;
        let p = arr(&sub["ports"])
            .iter()
            .find(|p| s(&p["id"]) == s(&r["port"]))?;
        let t = s(&p["transition"]);
        (!t.is_empty()).then(|| format!("{}/{t}", s(&sub["id"])))
    };
    let mut parent: BTreeMap<String, String> = BTreeMap::new();
    fn find(p: &mut BTreeMap<String, String>, x: &str) -> String {
        let q = p
            .entry(x.to_string())
            .or_insert_with(|| x.to_string())
            .clone();
        if q == x {
            return q;
        }
        let r = find(p, &q);
        p.insert(x.to_string(), r.clone());
        r
    }
    for l in links.iter().filter(|l| s(&l["kind"]) == "event") {
        if let (Some(a), Some(b)) = (member(&l["from"]), member(&l["to"])) {
            let (ra, rb) = (find(&mut parent, &a), find(&mut parent, &b));
            if ra != rb {
                parent.insert(rb, ra);
            }
        }
    }
    let keys: Vec<String> = parent.keys().cloned().collect();
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for k in keys {
        let r = find(&mut parent, &k);
        groups.entry(r).or_default().push(k);
    }
    groups
        .into_values()
        .filter(|g| g.len() > 1)
        .map(|mut g| {
            g.sort();
            (format!("fused:{}", g.join("+")), g)
        })
        .collect()
}

fn poly_hover(c: &mut Ctx, xf: &Xf, pts: &[(f32, f32)], tol: f32, i: Info) {
    let Some(p) = c.pointer_pos() else { return };
    let hit = pts
        .windows(2)
        .any(|w| dist_to_seg(p, xf.pt(w[0].0, w[0].1), xf.pt(w[1].0, w[1].1)) <= tol);
    if hit {
        c.hover(Rect::from_center_size(p, Vec2::splat(1.0)), i);
    }
}

fn dashed_poly(c: &Ctx, xf: &Xf, pts: &[(f32, f32)], col: Color32, w: f32, dash: (f32, f32)) {
    let v: Vec<Pos2> = pts.iter().map(|p| xf.pt(p.0, p.1)).collect();
    c.p.extend(Shape::dashed_line(
        &v,
        Stroke::new(w, col),
        dash.0 * xf.k,
        dash.1 * xf.k,
    ));
}

fn token_dots(c: &Ctx, at: Pos2, n: i64, k: f32) {
    let dot = |dx: f32, dy: f32, r: f32| {
        c.p.circle_filled(at + Vec2::new(dx, dy) * k, r * k, INK);
    };
    match n {
        0 => {}
        1 => dot(0.0, 0.0, 1.9),
        2 => {
            dot(-2.0, 0.0, 1.6);
            dot(2.0, 0.0, 1.6);
        }
        3 => {
            dot(0.0, -2.0, 1.5);
            dot(-2.1, 1.5, 1.5);
            dot(2.1, 1.5, 1.5);
        }
        _ => {
            c.text(at, Align2::CENTER_CENTER, &n.to_string(), 7.5 * k, INK);
        }
    }
}

fn transition_port(c: &Ctx, xf: &Xf, at: (f32, f32), side: char, kind: &str, a: f32) {
    let p = xf.pt(at.0, at.1);
    let r = Rect::from_center_size(p, Vec2::splat(11.0 * xf.k));
    c.p.rect(
        r,
        1.5 * xf.k,
        TRANSITION_PORT.gamma_multiply(a),
        Stroke::new(1.0, Color32::WHITE.gamma_multiply(a)),
        StrokeKind::Middle,
    );
    let inward = if side == 'L' { 1.0 } else { -1.0 };
    let dir = match kind {
        "in" => inward,
        "out" => -inward,
        _ => 0.0,
    };
    let k = xf.k;
    if dir != 0.0 {
        let tri = vec![
            p + Vec2::new(-2.2 * dir, -3.2) * k,
            p + Vec2::new(2.6 * dir, 0.0) * k,
            p + Vec2::new(-2.2 * dir, 3.2) * k,
        ];
        c.p.add(Shape::convex_polygon(tri, Color32::WHITE, Stroke::NONE));
    } else {
        c.p.circle_filled(p, 1.8 * k, Color32::WHITE);
    }
}

fn place_port(c: &Ctx, xf: &Xf, at: (f32, f32), kind: &str, col: Color32) {
    let p = xf.pt(at.0, at.1);
    let k = xf.k;
    c.p.circle(
        p,
        5.0 * k,
        if kind == "observe" {
            Color32::WHITE
        } else {
            col
        },
        Stroke::new(1.6 * k.max(0.8), col),
    );
    let tri = |dir: f32| {
        let o = p + Vec2::new(-11.0 * k, 0.0);
        c.p.add(Shape::convex_polygon(
            vec![
                o + Vec2::new(-2.4 * dir, -3.0) * k,
                o + Vec2::new(2.6 * dir, 0.0) * k,
                o + Vec2::new(-2.4 * dir, 3.0) * k,
            ],
            col,
            Stroke::NONE,
        ));
    };
    match kind {
        "in" => tri(1.0),
        "out" => tri(-1.0),
        "inout" => {
            let o = p + Vec2::new(-12.0 * k, 0.0);
            c.p.add(Shape::convex_polygon(
                vec![
                    o + Vec2::new(-5.0, 0.0) * k,
                    o + Vec2::new(0.0, -3.0) * k,
                    o + Vec2::new(0.0, 3.0) * k,
                ],
                col,
                Stroke::NONE,
            ));
            c.p.add(Shape::convex_polygon(
                vec![
                    o + Vec2::new(4.0, 0.0) * k,
                    o + Vec2::new(-1.0, -3.0) * k,
                    o + Vec2::new(-1.0, 3.0) * k,
                ],
                col,
                Stroke::NONE,
            ));
        }
        _ => {}
    }
}

/// Offsets (design units) that put an element back where it was before the
/// link that just merged, easing to zero.
struct Shift {
    t: f32,
    prev: Lay,
    link: Value,
    absorbed: Vec<usize>,
    new_class: Option<String>,
    added_instance: Option<String>,
}

impl Shift {
    fn k(&self) -> f32 {
        1.0 - self.t
    }
    fn port(&self, lay: &Lay, key: &str) -> Vec2 {
        match (lay.port_ix.get(key), self.prev.port_ix.get(key)) {
            (Some(n), Some(p)) => {
                let (n, p) = (&lay.ports[*n], &self.prev.ports[*p]);
                Vec2::new(p.x - n.x, p.y - n.y) * self.k()
            }
            _ => Vec2::ZERO,
        }
    }
    fn rect(&self, lay: &Lay, member: &str) -> f32 {
        match (lay.rows.get(member), self.prev.rows.get(member)) {
            (Some(n), Some(p)) => {
                (self.prev.boxes[p.0].rows[p.1].rx - lay.boxes[n.0].rows[n.1].rx) * self.k()
            }
            _ => 0.0,
        }
    }
    fn class_dy(&self, c: &Class) -> f32 {
        if Some(&c.name) == self.new_class.as_ref() || self.is_merged(c) {
            return match self
                .absorbed
                .iter()
                .map(|i| &self.prev.classes[*i])
                .max_by_key(|p| p.members.len())
            {
                Some(big) => (big.top - c.top) * self.k(),
                None => 0.0,
            };
        }
        match self.prev.classes.iter().find(|p| p.name == c.name) {
            Some(p) => (p.top - c.top) * self.k(),
            None => 0.0,
        }
    }
    fn is_merged(&self, c: &Class) -> bool {
        !self.absorbed.is_empty() && self.new_class.as_deref() == Some(c.name.as_str())
    }
}

fn shift_for(
    c: &Ctx,
    lay: &Lay,
    subs: &[Value],
    links: &[Value],
    flat: &Value,
    alone: &Value,
) -> Option<Shift> {
    let (ev, t) = c.anim(|e| match e {
        Event::LinkAdded { key, kind } => Some((Some((key.clone(), kind.clone())), None)),
        Event::InstanceAdded { id } => Some((None, Some(id.clone()))),
        _ => None,
    })?;
    let (link_ev, inst) = ev;
    if let Some(id) = inst {
        return Some(Shift {
            t,
            prev: layout_for(subs, links, flat, alone),
            link: Value::Null,
            absorbed: vec![],
            new_class: None,
            added_instance: Some(id),
        });
    }
    let (key, kind) = link_ev?;
    let l = links.iter().find(|l| s(&l["key"]) == key)?.clone();
    if kind != "event" {
        return Some(Shift {
            t,
            prev: layout_for(subs, links, flat, alone),
            link: l,
            absorbed: vec![],
            new_class: None,
            added_instance: None,
        });
    }
    let prev_links: Vec<Value> = links
        .iter()
        .filter(|x| s(&x["key"]) != key)
        .cloned()
        .collect();
    let prev = layout_for_links(subs, &prev_links, flat, alone);
    // the class that holds the new link, and the old ones it swallowed
    let member = |r: &Value| -> Option<String> {
        let sub = subs.iter().find(|x| s(&x["id"]) == s(&r["subnet"]))?;
        let p = arr(&sub["ports"])
            .iter()
            .find(|p| s(&p["id"]) == s(&r["port"]))?;
        Some(format!("{}/{}", s(&sub["id"]), s(&p["transition"])))
    };
    let m0 = member(&l["from"])?;
    let now = lay.classes.iter().find(|c| c.members.contains(&m0))?;
    let absorbed: Vec<usize> = prev
        .classes
        .iter()
        .enumerate()
        .filter(|(_, pc)| pc.members.iter().all(|m| now.members.contains(m)))
        .map(|(i, _)| i)
        .collect();
    Some(Shift {
        t,
        prev,
        link: l,
        absorbed,
        new_class: Some(now.name.clone()),
        added_instance: None,
    })
}

fn layout_for(subs: &[Value], links: &[Value], flat: &Value, alone: &Value) -> Lay {
    let fused: Vec<(String, Vec<String>)> = flat["fused_groups"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (k.clone(), strs(v)))
        .collect();
    layout(&Input {
        subs,
        links,
        fused,
        flat,
        alone,
    })
}

fn layout_for_links(subs: &[Value], links: &[Value], flat: &Value, alone: &Value) -> Lay {
    let fused = classes_without(subs, links);
    layout(&Input {
        subs,
        links,
        fused,
        flat,
        alone,
    })
}

pub(super) fn boundaries_orders(c: &mut Ctx, area: Rect) {
    let Some(flat) = c.d.flat.clone() else { return };
    if flat["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refused the links ({}); nothing to synchronise.",
                s(&flat["error"])
            ),
        );
        return;
    }
    let alone = c.d.boundaries.clone().unwrap_or(Value::Null);
    let subs = subnets(c);
    let links = links(c);
    let lay = layout_for(&subs, &links, &flat, &alone);
    let xf = {
        let mut xf = Xf::fit(area, W, lay.height, 1.5);
        xf.origin.x += ((area.width() - W * xf.k) / 2.0).max(0.0);
        xf
    };
    let shift = shift_for(c, &lay, &subs, &links, &flat, &alone);
    let k = xf.k;
    let by_id: BTreeMap<&str, &Value> = subs.iter().map(|x| (s(&x["id"]), x)).collect();
    let ev = ev_col();
    let model_places: BTreeMap<&str, &Value> = arr(&flat["places"])
        .iter()
        .map(|p| (s(&p["id"]), p))
        .collect();
    let mark = |id: &str| {
        model_places
            .get(id)
            .and_then(|p| p["initial"].as_i64())
            .unwrap_or(0)
    };

    // legend
    let ly = xf.pt(0.0, 18.0).y;
    let mut lx = 12.0;
    let mut lg = |c: &mut Ctx, draw: &dyn Fn(&Ctx, f32), text: &str, w: f32| {
        draw(c, lx + 6.0);
        c.text(
            Pos2::new(xf.pt(lx + 16.0, 0.0).x, ly + 4.0 * k),
            Align2::LEFT_CENTER,
            text,
            10.5 * k,
            MUTED,
        );
        lx += w;
    };
    lg(
        c,
        &|c, x| transition_port(c, &xf, (x, 18.0), 'L', "in", 1.0),
        "transition port (▸ in / ◂ out)",
        190.0,
    );
    lg(
        c,
        &|c, x| {
            c.p.circle_filled(xf.pt(x, 18.0), 4.5 * k, schema_color_p("resource"));
        },
        "place port (by schema; not this view's subject)",
        270.0,
    );
    lg(
        c,
        &|c, x| {
            let p = xf.pt(x, 18.0);
            c.p.circle(
                p,
                7.0 * k,
                Color32::WHITE,
                Stroke::new(1.6 * k.max(0.8), ev),
            );
            c.text(p, Align2::CENTER_CENTER, "⊗", 11.0 * k, ev);
        },
        "boundary port = one event class",
        210.0,
    );
    lg(
        c,
        &|c, x| {
            let p = xf.pt(x, 18.0);
            c.line(
                p + Vec2::new(-6.0, 0.0) * k,
                p + Vec2::new(6.0, 0.0) * k,
                ev,
                2.0 * k.max(0.8),
            )
        },
        "event wire (RenderDOT green)",
        180.0,
    );
    lg(
        c,
        &|c, x| {
            let p = xf.pt(x, 18.0);
            c.p.extend(Shape::dashed_line(
                &[p + Vec2::new(-6.0, 0.0) * k, p + Vec2::new(6.0, 0.0) * k],
                Stroke::new(1.4 * k.max(0.8), kind_color("token").gamma_multiply(0.6)),
                3.0 * k,
                3.0 * k,
            ));
        },
        "token / data / guard: Cospans view",
        200.0,
    );
    c.hover(
        Rect::from_min_max(xf.pt(0.0, 6.0), xf.pt(W, 30.0)),
        info("Legend: nets with boundaries")
            .fact("what it shows", "Transitions are wired to ports on a boundary; nets that share a boundary compose by synchronising the transitions wired to the same port.")
            .fact("paper", PAPER_DEF)
            .fact("composition", PAPER_SYNC)
            .fact("colors", "transition ports in the transition color, place ports by schema; event wires use RenderDOT's event color")
            .fact("sources", "Bruni, Melgratti, Montanari, Sobociński, LMCS 9(3:16) 2013 (arXiv 1307.0204); Sobociński, CONCUR 2010")
            .cite(c_lk_event()),
    );
    // headers
    for (x, al, t) in [
        (8.0, Align2::LEFT_CENTER, "outer boundary"),
        (
            BOX_X + 6.0,
            Align2::LEFT_CENTER,
            "instances: nets with boundaries N : k → l",
        ),
        (SHARED_X, Align2::CENTER_CENTER, "shared boundary"),
        (
            CARD_X + 26.0,
            Align2::LEFT_CENTER,
            "composite transitions (Flatten's fused groups)",
        ),
    ] {
        c.text(xf.pt(x, 50.0), al, t, 10.5 * k, MUTED);
    }

    // boundaries (dashed verticals)
    let n_outer = lay
        .ports
        .iter()
        .filter(|q| s(&q.port["target"]) == "transition" && q.side == 'L')
        .count();
    let (o0, o1) = (
        xf.pt(OUTER_X, TOP - 6.0),
        xf.pt(OUTER_X, lay.boxes_bottom + 6.0),
    );
    c.p.extend(Shape::dashed_line(
        &[o0, o1],
        Stroke::new(1.4 * k.max(0.8), hex("#9aa1aa")),
        5.0 * k,
        4.0 * k,
    ));
    c.hover_seg(o0, o1, 6.0, info(format!("outer boundary of the composite: {n_outer} transition ports"))
        .fact("what it is", "The transition ports no event link uses. Each such transition is its own minimal synchronisation ({t}, ∅); read as a net with boundaries, it stays on the composite's outer boundary.")
        .fact("paper", PAPER_SOLO)
        .fact("flat name", "a singleton class keeps its qualified id, e.g. orders/enqueue (compose_flatten.go:211-215)")
        .fact("go-pflow", "Flatten returns a Model, which has no ports: the boundary belongs to a Subnet. This edge is the paper's reading, not something Flatten emits.")
        .cite(c_flat_name()).cite(c_trans_classes()).cite(c_model_no_ports()).cite(c_subnet_ports()));
    let shared_bottom = lay
        .classes
        .iter()
        .map(|c| c.top + c.h)
        .fold(lay.boxes_bottom, f32::max)
        + 6.0;
    let (s0, s1) = (xf.pt(SHARED_X, TOP - 6.0), xf.pt(SHARED_X, shared_bottom));
    c.p.extend(Shape::dashed_line(
        &[s0, s1],
        Stroke::new(1.4 * k.max(0.8), ev.gamma_multiply(0.7)),
        5.0 * k,
        4.0 * k,
    ));
    c.hover_seg(s0, s1, 6.0, link_kind_info("event")
        .fact("shared boundary", format!("{} port{}, one per event class", lay.classes.len(), if lay.classes.len() == 1 { "" } else { "s" }))
        .fact("paper", PAPER_SYNC)
        .note("In the paper the shared boundary lies between two nets, one on each side, and is hidden in their composite. go-pflow fuses n subnets at once, so every instance faces it from one side here.")
        .cite(c_road_row()).cite(c_road_event()).cite(c_trans_classes()));
    if lay.classes.is_empty() {
        let r = Rect::from_min_size(
            xf.pt(SHARED_X - 120.0, TOP + 80.0),
            Vec2::new(240.0, 54.0) * k,
        );
        c.p.rect(
            r,
            6.0 * k,
            Color32::WHITE,
            Stroke::new(1.0, hex("#cfd5dc")),
            StrokeKind::Middle,
        );
        c.text(
            xf.pt(SHARED_X, TOP + 101.0),
            Align2::CENTER_CENTER,
            "no event links: nothing synchronises",
            11.5 * k,
            INK,
        );
        c.text(
            xf.pt(SHARED_X, TOP + 118.0),
            Align2::CENTER_CENTER,
            "every transition is its own ({t}, ∅): disjoint union",
            10.0 * k,
            MUTED,
        );
        c.hover(r, info("an empty shared boundary")
            .fact("what it is", "With no event links every transition is a minimal synchronisation on its own, and the composite is the disjoint union of the instances.")
            .fact("paper", PAPER_SOLO)
            .fact("ROADMAP", "unlinked subnets: the monoidal product (disjoint union)")
            .cite(c_road_monoidal()));
    }

    // instance boxes
    for (bi, b) in lay.boxes.iter().enumerate() {
        let sub = &b.sub;
        let sid = s(&sub["id"]);
        let (xa, al) = match &shift {
            Some(sh) if sh.added_instance.as_deref() == Some(sid) => (-40.0 * sh.k(), sh.t),
            _ => (0.0, 1.0),
        };
        let r = Rect::from_min_size(xf.pt(BOX_X + xa, b.y), Vec2::new(BOX_W, b.h) * k);
        let pattern = s(&sub["pattern"]);
        c.p.rect(
            r,
            6.0 * k,
            Color32::WHITE.gamma_multiply(al),
            Stroke::new(1.3 * k.max(0.8), hex("#333333").gamma_multiply(al)),
            StrokeKind::Middle,
        );
        let head = Rect::from_min_size(
            r.left_top() + Vec2::new(1.0, 1.0),
            Vec2::new(r.width() - 2.0, HEAD * k - 1.0),
        );
        c.p.rect_filled(head, 5.0 * k, pattern_tint_p(pattern).gamma_multiply(al));
        c.line(
            Pos2::new(r.left(), head.bottom()),
            Pos2::new(r.right(), head.bottom()),
            hex("#d5d9df"),
            1.0,
        );
        let n_l = b
            .rows
            .iter()
            .flat_map(|r| &r.ports)
            .filter(|i| lay.ports[**i].side == 'L')
            .count();
        let n_r = b
            .rows
            .iter()
            .flat_map(|r| &r.ports)
            .filter(|i| lay.ports[**i].side == 'R')
            .count();
        let label = comp::ontology()["patterns"][pattern]["prefLabel"]
            .as_str()
            .unwrap_or(pattern)
            .to_string();
        let t0 = xf.pt(BOX_X + xa + 10.0, b.y + 14.0);
        let wr = c.text(
            t0,
            Align2::LEFT_CENTER,
            sid,
            12.5 * k,
            INK.gamma_multiply(al),
        );
        c.text(
            Pos2::new(wr.right() + 6.0 * k, t0.y),
            Align2::LEFT_CENTER,
            &format!(
                "{label}{}",
                if sub["extra"] == true {
                    " · added"
                } else {
                    ""
                }
            ),
            11.0 * k,
            MUTED.gamma_multiply(al),
        );
        c.mono(
            xf.pt(BOX_X + xa + BOX_W - 10.0, b.y + 14.0),
            Align2::RIGHT_CENTER,
            &format!("{sid} : {n_l} → {n_r}"),
            11.5 * k,
            INK.gamma_multiply(al),
        );
        c.hover(r, subnet_info(sub)
            .fact("as a net with boundaries", format!("{sid} : {n_l} → {n_r}: {n_l} outer (unsynchronised) and {n_r} shared (synchronised) transition ports"))
            .fact("which edge", "Which ports face the shared boundary is this drawing's choice (the event links decide it); the pattern fixes the ports, not their sides.")
            .fact("paper", PAPER_DEF));
        // transitions: one row each, wired to the outer (left) or shared (right) edge
        for row in &b.rows {
            let rx = row.rx + shift.as_ref().map_or(0.0, |sh| sh.rect(&lay, &row.member));
            for pi in &row.ports {
                let q = &lay.ports[*pi];
                let (x1, x2) = if q.side == 'L' {
                    (BOX_X + 6.0, rx)
                } else {
                    (rx + RECT_W, BOX_R - 6.0)
                };
                let (p1, p2) = (xf.pt(x1, row.cy), xf.pt(x2, row.cy));
                let col = if row.linked {
                    ev.gamma_multiply(0.8)
                } else {
                    hex("#9aa1aa")
                };
                c.p.extend(Shape::dashed_line(
                    &[p1, p2],
                    Stroke::new((if row.linked { 1.3 } else { 1.0 }) * k.max(0.8), col),
                    3.0 * k,
                    2.5 * k,
                ));
                let pid = s(&q.port["id"]);
                let lbl = if pid == row.tid {
                    s(&q.port["kind"]).to_string()
                } else {
                    format!("{pid} · {}", s(&q.port["kind"]))
                };
                if q.side == 'L' {
                    c.mono(
                        xf.pt(rx + RECT_W + 8.0, row.cy),
                        Align2::LEFT_CENTER,
                        &lbl,
                        9.5 * k,
                        MUTED,
                    );
                } else {
                    c.mono(
                        xf.pt(rx - 8.0, row.cy),
                        Align2::RIGHT_CENTER,
                        &lbl,
                        9.5 * k,
                        MUTED,
                    );
                }
            }
            let rr = Rect::from_min_size(
                xf.pt(rx, row.cy - RECT_H / 2.0),
                Vec2::new(RECT_W, RECT_H) * k,
            );
            c.p.rect(
                rr,
                2.0 * k,
                if row.linked {
                    hex("#e8f3ec")
                } else {
                    Color32::WHITE
                },
                Stroke::new(1.2 * k.max(0.8), if row.linked { ev } else { INK }),
                StrokeKind::Middle,
            );
            c.mono(rr.center(), Align2::CENTER_CENTER, &row.tid, 10.5 * k, INK);
            let arcs = arr(&sub["arcs"]);
            let fmtw = |a: &Value, key: &str| {
                let w = a["weight"].as_i64().unwrap_or(1);
                format!(
                    "{}{}",
                    s(&a[key]),
                    if w > 1 {
                        format!("×{w}")
                    } else {
                        String::new()
                    }
                )
            };
            let pre: Vec<String> = arcs
                .iter()
                .filter(|a| s(&a["to"]) == row.tid)
                .map(|a| fmtw(a, "from"))
                .collect();
            let post: Vec<String> = arcs
                .iter()
                .filter(|a| s(&a["from"]) == row.tid)
                .map(|a| fmtw(a, "to"))
                .collect();
            let mut i = info(format!("transition {sid}/{}", row.tid))
                .fact("concept", "ns:Transition")
                .fact(
                    "ports",
                    row.ports
                        .iter()
                        .map(|i| s(&lay.ports[*i].port["id"]).to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                )
                .fact(
                    "preset (in this instance)",
                    if pre.is_empty() {
                        "∅".into()
                    } else {
                        pre.join(", ")
                    },
                )
                .fact(
                    "postset (in this instance)",
                    if post.is_empty() {
                        "∅".into()
                    } else {
                        post.join(", ")
                    },
                )
                .fact(
                    "synchronised",
                    match &row.cls {
                        Some(cl) => format!("yes: in {cl}"),
                        None => {
                            "no: its own minimal synchronisation ({t}, ∅) on the outer boundary"
                                .into()
                        }
                    },
                )
                .note(
                    "Arcs inside an instance are not drawn in this view; the Flat view draws them.",
                );
            if row.cls.is_some() {
                i = i.cite(c_trans_classes());
            }
            c.hover(rr, i);
        }
        // place ports (faint rows) and the places with their marking
        for pi in &b.pports {
            let q = &lay.ports[*pi];
            let pid = s(&q.port["id"]);
            let t = format!(
                "{pid} ({}) → {} · {}",
                s(&q.port["kind"]),
                s(&q.port["place"]),
                s(&q.port["schema"])
            );
            let at = xf.pt(BOX_X + 12.0, q.y);
            let w = c
                .mono(at, Align2::LEFT_CENTER, &t, 9.5 * k, MUTED)
                .expand2(Vec2::new(0.0, 3.0 * k));
            c.hover(w, info(format!("port {sid}:{pid}"))
                .fact("kind", s(&q.port["kind"]))
                .fact("place", s(&q.port["place"]))
                .fact("schema", s(&q.port["schema"]))
                .note("Place ports are not this convention's subject: token and data links glue places (a cospan; see the Cospans view).")
                .cite(c_road_cospan()));
        }
        if let Some(pi) = b.pports.first() {
            let yy = lay.ports[*pi].y - PROW / 2.0 - 2.0;
            c.line(
                xf.pt(BOX_X + 10.0, yy),
                xf.pt(BOX_R - 10.0, yy),
                hex("#eceef1"),
                1.0,
            );
        }
        for (pl, cx, cy) in &b.chips {
            let exported = pl["exported"] == true;
            let at = xf.pt(*cx + 5.5, *cy);
            c.p.circle(
                at,
                5.5 * k,
                Color32::WHITE,
                Stroke::new(
                    (if exported { 1.2 } else { 0.9 }) * k.max(0.8),
                    if exported { INK } else { hex("#a3a9b1") },
                ),
            );
            token_dots(c, at, pl["initial"].as_i64().unwrap_or(0), k);
            let w = c.mono(
                xf.pt(*cx + 14.0, *cy),
                Align2::LEFT_CENTER,
                s(&pl["id"]),
                9.0 * k,
                if exported { INK } else { MUTED },
            );
            c.hover(w.union(Rect::from_center_size(at, Vec2::splat(11.0 * k))), info(format!("place {sid}/{}", s(&pl["id"])))
                .fact("initial", pl["initial"].to_string())
                .fact("exported", if exported { "yes" } else { "no: inside the boundary" })
                .note("Places are listed with their initial marking; this convention is about how transitions meet a boundary."));
        }
        let _ = bi;
    }

    // ghosts: token / data / guard links (faint pointers to the cospan view)
    for g in &lay.ghosts {
        let kind = s(&g.l["kind"]);
        let col = kind_color(kind);
        let fresh = shift
            .as_ref()
            .is_some_and(|sh| s(&sh.link["key"]) == s(&g.l["key"]));
        let (frac, al) = if fresh {
            let t = shift.as_ref().unwrap().t;
            (t, 0.55)
        } else {
            (1.0, 0.55)
        };
        let pts = partial(&g.pts, frac);
        dashed_poly(
            c,
            &xf,
            &pts,
            col.gamma_multiply(al),
            1.4 * k.max(0.8),
            if kind == "data" {
                (1.5, 3.0)
            } else {
                (5.0, 3.0)
            },
        );
        let m = xf.pt(g.lane, g.my);
        if frac >= 1.0 {
            c.p.circle(
                m,
                6.5 * k,
                Color32::WHITE,
                Stroke::new(1.2 * k.max(0.8), col.gamma_multiply(al)),
            );
            c.text(
                m,
                Align2::CENTER_CENTER,
                kind_glyph(kind),
                8.5 * k,
                col.gamma_multiply(al),
            );
        }
        let why = if kind == "guard" {
            "A guard link lowers to a read arc on the gated transition: restrictive, outside the free-SMC fragment, and no synchronisation. Not this convention's subject; see the Cospans and Flat views.".to_string()
        } else {
            format!("A {kind} link glues two places (a pushout of open nets, Baez–Master), not transitions. Not this convention's subject: see the Cospans view.")
        };
        let i = link_info(&g.l).note(why).cite(if kind == "guard" {
            c_road_guard()
        } else {
            c_road_cospan()
        });
        poly_hover(c, &xf, &g.pts, 5.0, i);
    }

    // stubs to the outer boundary, then the transition and place ports themselves
    for q in &lay.ports {
        if s(&q.port["target"]) == "transition" && q.side == 'L' {
            let (a, b) = (xf.pt(OUTER_X, q.y), xf.pt(BOX_X - 6.0, q.y));
            c.line(a, b, hex("#b5bbc3"), 1.0);
            c.p.circle_filled(a, 2.2 * k, hex("#9aa1aa"));
        }
    }
    for q in &lay.ports {
        let off = shift
            .as_ref()
            .map_or(Vec2::ZERO, |sh| sh.port(&lay, &q.key));
        let (x, y) = (q.x + off.x, q.y + off.y);
        let trans = s(&q.port["target"]) == "transition";
        let sub = by_id.get(q.sid.as_str());
        let added = shift
            .as_ref()
            .is_some_and(|sh| sh.added_instance.as_deref() == Some(q.sid.as_str()));
        // the real port: hover, drag, drop target (and its lit ring while dragging)
        let hit_at = xf.pt(x, y);
        if let Some(_sub) = sub {
            c.port(hit_at, &q.sid, &q.port, false, true);
        }
        if added {
            continue;
        }
        if trans {
            transition_port(c, &xf, (x, y), q.side, s(&q.port["kind"]), 1.0);
        } else {
            place_port(
                c,
                &xf,
                (x, y),
                s(&q.port["kind"]),
                schema_color_p(s(&q.port["schema"])),
            );
        }
    }

    // event wires, boundary ports and composite cards
    let flat_t: BTreeMap<&str, &Value> = arr(&flat["transitions"])
        .iter()
        .map(|t| (s(&t["id"]), t))
        .collect();
    for cl in &lay.classes {
        let dy = shift.as_ref().map_or(0.0, |sh| sh.class_dy(cl));
        let merged_new = shift
            .as_ref()
            .is_some_and(|sh| sh.new_class.as_deref() == Some(cl.name.as_str()));
        let brand_new = merged_new && shift.as_ref().is_some_and(|sh| sh.absorbed.is_empty());
        let t = shift.as_ref().map_or(1.0, |sh| sh.t);
        let prev_members: BTreeSet<&String> = shift
            .as_ref()
            .filter(|_| merged_new)
            .map(|sh| {
                sh.absorbed
                    .iter()
                    .flat_map(|i| &sh.prev.classes[*i].members)
                    .collect()
            })
            .unwrap_or_default();
        let by = cl.by + dy;
        for (m, pi) in &cl.mports {
            let q = &lay.ports[*pi];
            let off = shift
                .as_ref()
                .map_or(Vec2::ZERO, |sh| sh.port(&lay, &q.key));
            let a = xf.pt(BOX_R + 6.0 + off.x, q.y + off.y);
            let b = xf.pt(SHARED_X - BPORT_R - 1.0, by);
            let new_wire = merged_new && !prev_members.contains(m);
            let frac = if new_wire {
                ((t - 0.35) / 0.55).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let al = if merged_new && !new_wire && t < 1.0 {
                ((t - 0.5) / 0.4).clamp(0.0, 1.0).max(0.25)
            } else {
                1.0
            };
            let pts = bezier(a, b, 18);
            let shown = partial_pts(&pts, frac);
            for w in shown.windows(2) {
                c.line(w[0], w[1], ev.gamma_multiply(al), 1.8 * k.max(0.8));
            }
            let via: Vec<String> = cl
                .clinks
                .iter()
                .filter(|l| {
                    [&l["from"], &l["to"]].iter().any(|r| {
                        by_id.get(s(&r["subnet"])).is_some_and(|sb| {
                            arr(&sb["ports"]).iter().any(|p| {
                                s(&p["id"]) == s(&r["port"])
                                    && format!("{}/{}", s(&r["subnet"]), s(&p["transition"])) == *m
                            })
                        })
                    })
                })
                .map(|l| {
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
                })
                .collect();
            let i = link_kind_info("event")
                .fact("this wire", format!("{m} is wired to boundary port {}", cl.letter))
                .fact("what it is", "A member of the event class: it fires only together with every other transition wired to this port.")
                .fact("via links", via.join("; "))
                .cite(c_trans_classes());
            if let Some(p) = c.pointer_pos() {
                if pts.windows(2).any(|w| dist_to_seg(p, w[0], w[1]) <= 5.0) {
                    c.hover(Rect::from_center_size(p, Vec2::splat(1.0)), i);
                }
            }
        }
        // card
        let fl = flat_t.get(cl.name.as_str()).copied();
        let card_al = if brand_new { t } else { 1.0 };
        draw_card(
            c, &xf, &lay, cl, dy, card_al, merged_new, t, fl, &by_id, &mark, &flat,
        );
        // boundary port
        let p = xf.pt(SHARED_X, by);
        let pop = if brand_new {
            if t < 0.35 {
                0.0
            } else {
                1.0 + 0.35
                    * (1.0 - ((t - 0.7).abs() / 0.35).min(1.0))
                    * (if t < 0.7 { 1.0 } else { 0.0 })
            }
        } else if merged_new {
            1.0 + 0.4 * (1.0 - ((t - 0.7).abs() / 0.2).min(1.0))
        } else {
            1.0
        };
        if pop > 0.0 {
            let r = BPORT_R * pop;
            c.p.circle(p, r * k, Color32::WHITE, Stroke::new(2.0 * k.max(0.8), ev));
            c.text(p, Align2::CENTER_CENTER, "⊗", 15.0 * k * pop, ev);
            c.text(
                p + Vec2::new(-(BPORT_R + 5.0) * k, -(BPORT_R + 2.0) * k),
                Align2::RIGHT_BOTTOM,
                &cl.letter.to_string(),
                10.5 * k,
                ev,
            );
        }
        // the old boundary ports travel to the new one and fuse into it
        if let Some(sh) = &shift {
            if merged_new {
                for i in &sh.absorbed {
                    let pc = &sh.prev.classes[*i];
                    let f = ease(((sh.t) / 0.85).clamp(0.0, 1.0));
                    let y = pc.by + (cl.by - pc.by) * f;
                    let al = if sh.t > 0.85 {
                        1.0 - (sh.t - 0.85) / 0.15
                    } else {
                        0.9
                    };
                    let g = xf.pt(SHARED_X, y);
                    if pc.name != cl.name {
                        c.p.circle(
                            g,
                            BPORT_R * k,
                            Color32::WHITE.gamma_multiply(al),
                            Stroke::new(2.0 * k.max(0.8), ev.gamma_multiply(al)),
                        );
                        c.text(
                            g,
                            Align2::CENTER_CENTER,
                            "⊗",
                            15.0 * k,
                            ev.gamma_multiply(al),
                        );
                    }
                }
            }
        }
        if let Some(fl) = fl {
            let mut i = flat_transition_info(fl)
                .fact("boundary port", format!("{}: one event class, {} members", cl.letter, cl.members.len()))
                .fact("links (binary)", format!("{}, joined by union-find into one class", cl.clinks.len()))
                .note(format!("One port of the shared boundary per go-pflow event class. In the paper, transitions wired to one port from the same side are alternatives, not partners ({PAPER_CONTENTION}); this port stands for go-pflow's n-ary rendezvous."));
            i.cites
                .extend([c_trans_classes(), c_union_find(), c_rendezvous()]);
            c.hover(
                Rect::from_center_size(p, Vec2::splat((BPORT_R + 4.0) * 2.0 * k)),
                i,
            );
        }
        c.scene
            .emit("boundary", &cl.name, json!({"legs": cl.members}));
        c.scene
            .emit("composite", &cl.name, json!({"members": cl.members}));
    }

    // bottom summary
    let n_sync: usize = lay.classes.iter().map(|c| c.members.len()).sum();
    let n_all: usize = subs.iter().map(|x| arr(&x["transitions"]).len()).sum();
    let n_ported = lay
        .boxes
        .iter()
        .flat_map(|b| &b.rows)
        .filter(|r| r.cls.is_none() && !r.ports.is_empty())
        .count();
    let n_internal = n_all - n_sync - n_ported;
    let n_flat_t = arr(&flat["transitions"]).len();
    let sum = format!(
        "composite: {n_all} transitions in {} instances → {n_flat_t} flat transitions: {n_sync} synchronised into {} composite{}; {} unsynchronised, {n_ported} of them on the outer boundary ({n_outer} port{}){}.",
        subs.len(),
        lay.classes.len(),
        if lay.classes.len() == 1 { "" } else { "s" },
        n_all - n_sync,
        if n_outer == 1 { "" } else { "s" },
        if n_internal > 0 { format!(", {n_internal} internal (no port)") } else { String::new() },
    );
    let r = c.text(
        xf.pt(12.0, lay.height - 18.0),
        Align2::LEFT_CENTER,
        &sum,
        11.0 * k,
        MUTED,
    );
    c.hover(r, info("the composite")
        .fact("what it is", "Flatten's transitions: one per event class, plus every unsynchronised transition under its qualified id.")
        .fact("flat net", format!("{} places, {n_flat_t} transitions", arr(&flat["places"]).len()))
        .fact("paper", PAPER_SOLO)
        .cite(c_trans_classes()).cite(c_flat_name()));

    c.note("honesty", "Bruni et al. compose two nets along one boundary; go-pflow's event classes are n-ary (union-find over binary links). Each class is drawn as one boundary port, and the composite transitions are exactly Flatten's fused groups. Unlinked transition ports sit on the outer boundary, unsynchronised. Places and token/data/guard links are drawn faint: they belong to the Cospans view.");
    c.note("honesty", "In the paper, two transitions wired to one port from the same side are alternatives, not partners (Def. 3.1 (iii)-(iv); a P/T synchronisation needs U• = •V, Def. 4.3). So the single port per class is go-pflow's rendezvous written in the paper's notation, not the paper's port semantics. \"Enabled on its own\" on a card runs the same EnabledWhyNot on one member's own arcs over the flat places (bundle.boundaries); \"enabled at the initial marking\" is a check at one marking, not a proof about any other.");
    let _ = (
        c_every_member(),
        c_classes(),
        c_guard_lower(),
        c_why_not(),
        c_leak(),
    );
}

/// A cubic from `a` to `b` with horizontal tangents, as 19 points.
fn bezier(a: Pos2, b: Pos2, n: usize) -> Vec<Pos2> {
    let dx = (b.x - a.x).abs().max(40.0) * 0.5;
    let (c1, c2) = (a + Vec2::new(dx, 0.0), b - Vec2::new(dx, 0.0));
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            Pos2::new(
                u * u * u * a.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * b.x,
                u * u * u * a.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * b.y,
            )
        })
        .collect()
}

/// The first `frac` of a polyline (by point count).
fn partial_pts(pts: &[Pos2], frac: f32) -> Vec<Pos2> {
    if frac >= 1.0 {
        return pts.to_vec();
    }
    let n = ((pts.len() - 1) as f32 * frac).floor() as usize;
    let mut v: Vec<Pos2> = pts[..=n.min(pts.len() - 1)].to_vec();
    if n + 1 < pts.len() {
        let r = (pts.len() - 1) as f32 * frac - n as f32;
        v.push(pts[n] + (pts[n + 1] - pts[n]) * r);
    }
    v
}

fn partial(pts: &[(f32, f32)], frac: f32) -> Vec<(f32, f32)> {
    if frac >= 1.0 {
        return pts.to_vec();
    }
    let total: f32 = pts
        .windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum();
    let mut left = total * frac;
    let mut out = vec![pts[0]];
    for w in pts.windows(2) {
        let l = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        if left >= l {
            out.push(w[1]);
            left -= l;
        } else {
            let r = if l > 0.0 { left / l } else { 0.0 };
            out.push((
                w[0].0 + (w[1].0 - w[0].0) * r,
                w[0].1 + (w[1].1 - w[0].1) * r,
            ));
            break;
        }
    }
    out
}

fn link_kind_info(kind: &str) -> Info {
    let lk = &comp::ontology()["link_kinds"][kind];
    info(format!("{kind} link"))
        .fact(
            "from / to",
            format!("{} → {}", s(&lk["fromTarget"]), s(&lk["toTarget"])),
        )
        .note(s(&lk["comment"]))
        .cites_of(&lk["cites"])
}

fn flat_transition_info(t: &Value) -> Info {
    let members = strs(&t["members"]);
    let mut i = info(format!("flat transition {}", s(&t["id"])))
        .fact("concept", "ns:Transition")
        .fact("members", members.join(", "));
    if members.len() > 1 {
        i = i.fact("fused", "one rendezvous: pre/postset is the union of the members', so it fires only when every member could");
    }
    i.note = s(&t["description"]).to_string();
    i.cites = flatten_cites_b();
    i
}

fn flatten_cites_b() -> Vec<String> {
    vec![
        go("metamodel/compose_flatten.go", 15, 27, "Flatten pipeline"),
        go(
            "metamodel/unionfind.go",
            1,
            60,
            "the smallest member names the class",
        ),
        go(
            "metamodel/compose_flatten.go",
            201,
            227,
            "flat names: wire:… and fused:…",
        ),
        go(
            "metamodel/compose_flatten.go",
            252,
            333,
            "mergePlaces: initial sums, capacity is the tightest",
        ),
        go(
            "metamodel/compose_flatten.go",
            335,
            412,
            "mergeArcs: MergeSum",
        ),
        go("metamodel/compose_fuse.go", 84, 192, "fuseClass"),
    ]
}

#[allow(clippy::too_many_arguments)]
fn draw_card(
    c: &mut Ctx,
    xf: &Xf,
    lay: &Lay,
    cl: &Class,
    dy: f32,
    al: f32,
    merged: bool,
    t: f32,
    fl: Option<&Value>,
    by_id: &BTreeMap<&str, &Value>,
    mark: &dyn Fn(&str) -> i64,
    flat: &Value,
) {
    let k = xf.k;
    let ev = ev_col();
    let (cx, top) = (CARD_X, cl.top + dy);
    let by = cl.by + dy;
    c.line(
        xf.pt(SHARED_X + BPORT_R, by),
        xf.pt(cx, by),
        ev.gamma_multiply(al),
        1.4 * k.max(0.8),
    );
    let r = Rect::from_min_size(xf.pt(cx, top), Vec2::new(CARD_W, cl.h) * k);
    // a freshly merged card flashes green and fades to white
    let flash = if merged {
        (1.0 - ((t - 0.3) / 0.7).clamp(0.0, 1.0)).max(0.0)
    } else {
        0.0
    };
    let bg = lerp_col(Color32::WHITE, hex("#d9efe1"), flash);
    c.p.rect(
        r,
        6.0 * k,
        bg.gamma_multiply(al),
        Stroke::new(1.3 * k.max(0.8), hex("#b9d8c4").gamma_multiply(al)),
        StrokeKind::Middle,
    );
    let saved = c.p.clone();
    // header
    let h0 = xf.pt(cx + 12.0, top + 18.0);
    let a = c.text(
        h0,
        Align2::LEFT_CENTER,
        &format!("{} ", cl.letter),
        11.5 * k,
        ev.gamma_multiply(al),
    );
    let b = c.text(
        Pos2::new(a.right(), h0.y),
        Align2::LEFT_CENTER,
        "composite transition",
        11.5 * k,
        INK.gamma_multiply(al),
    );
    c.text(
        Pos2::new(b.right() + 4.0, h0.y),
        Align2::LEFT_CENTER,
        &format!("· {} members, one firing", cl.members.len()),
        10.0 * k,
        MUTED.gamma_multiply(al),
    );
    let (verdict, vcol) = if cl.why.is_empty() {
        ("✓ enabled at the initial marking", ev)
    } else {
        ("✗ not enabled at the initial marking", MUTED)
    };
    c.text(
        xf.pt(cx + CARD_W - 12.0, top + 18.0),
        Align2::RIGHT_CENTER,
        verdict,
        10.0 * k,
        vcol.gamma_multiply(al),
    );
    if let Some(fl) = fl {
        let hr = Rect::from_min_size(xf.pt(cx, top), Vec2::new(CARD_W, 26.0) * k);
        c.hover(
            hr,
            flat_transition_info(fl)
                .fact(
                    "at the initial marking",
                    if cl.why.is_empty() {
                        "enabled".to_string()
                    } else {
                        format!("not enabled: {}", cl.why)
                    },
                )
                .fact(
                    "computed by",
                    "pflow-metamodel's enabled_why_not on the flat net (bundle.boundaries)",
                )
                .cite(c_why_not()),
        );
    }
    // flat name, one member per line, each with its own enablement
    let mut yy = top + 24.0;
    for (i, m) in cl.members.iter().enumerate() {
        yy += 13.0;
        let (ok, lines) = &cl.alone[i];
        let dot = xf.pt(cx + 15.0, yy - 3.5);
        c.p.circle(
            dot,
            3.2 * k,
            if *ok { ev } else { Color32::WHITE },
            Stroke::new(1.2 * k.max(0.8), if *ok { ev } else { hex("#9aa1aa") }),
        );
        c.mono(
            xf.pt(cx + 24.0, yy - 3.5),
            Align2::LEFT_CENTER,
            &format!("{}{m}", if i == 0 { "fused:" } else { "     +" }),
            10.5 * k,
            INK.gamma_multiply(al),
        );
        for (j, ln) in lines.iter().enumerate() {
            c.text(
                xf.pt(cl.status_x, yy - 3.5 + j as f32 * 13.0),
                Align2::LEFT_CENTER,
                ln,
                9.5 * k,
                MUTED.gamma_multiply(al),
            );
        }
        let rr = Rect::from_min_size(
            xf.pt(cx + 8.0, yy - 11.0),
            Vec2::new(
                CARD_W - 16.0,
                (13.0 + (lines.len() as f32 - 1.0) * 13.0).max(13.0),
            ) * k,
        );
        let (sid, tid) = m.split_once('/').unwrap_or(("", m));
        let mut inf = info(format!("transition {m}"))
            .fact("on its own, at the initial marking", if *ok { "enabled".to_string() } else { format!("not enabled: {}", lines.join(" ").trim_start_matches("alone: ")) })
            .fact("computed by", "pflow-metamodel's enabled_why_not on this member's own arcs (and any guard gating it) over the flat places and the flat initial marking (bundle.boundaries)")
            .fact("in the composite", "fires only with every other member")
            .note("The flat name is go-pflow's: \"fused:\" + the sorted members joined by \"+\" (there is no explicit link ID on this page).");
        if let Some(sb) = by_id.get(sid) {
            inf = inf.fact("subnet", s(&sb["id"])).fact("transition", tid);
        }
        inf = inf.cite(c_flat_name()).cite(c_why_not());
        c.hover(rr, inf);
        yy += (lines.len() as f32 - 1.0) * 13.0;
    }
    yy += 4.0;
    if !cl.why.is_empty() {
        yy += 13.0;
        for (j, ln) in cl.why_lines.iter().enumerate() {
            c.text(
                xf.pt(cx + 24.0, yy - 3.5 + j as f32 * 13.0),
                Align2::LEFT_CENTER,
                &ln.replace('@', &cl.letter.to_string()),
                9.5 * k,
                MUTED.gamma_multiply(al),
            );
        }
        let hh = 13.0 * cl.why_lines.len() as f32;
        c.hover(
            Rect::from_min_size(
                xf.pt(cx + 20.0, yy - 11.0),
                Vec2::new(CARD_W - 30.0, hh) * k,
            ),
            info("why the composite cannot fire yet")
                .fact("reason", cl.why.clone())
                .fact("marking", "the flat net's initial marking")
                .fact(
                    "shown as",
                    format!("the flat name is shortened to {} on the card", cl.letter),
                )
                .cite(c_why_not()),
        );
        yy += (cl.why_lines.len() as f32 - 1.0) * 13.0;
    }
    // the rendezvous rule
    yy += 16.0;
    c.text(
        xf.pt(cx + 12.0, yy - 3.0),
        Align2::LEFT_CENTER,
        "RENDEZVOUS",
        9.0 * k,
        MUTED.gamma_multiply(al),
    );
    c.mono(
        xf.pt(cx + 12.0 + 70.0, yy - 3.0),
        Align2::LEFT_CENTER,
        "go-pflow compose_fuse.go:11-14",
        9.0 * k,
        MUTED.gamma_multiply(al),
    );
    yy += 13.0;
    c.text(
        xf.pt(cx + 12.0, yy - 3.5),
        Align2::LEFT_CENTER,
        "fires only when every member is enabled; pre/postset = union",
        10.5 * k,
        INK.gamma_multiply(al),
    );
    c.hover(Rect::from_min_size(xf.pt(cx + 8.0, yy - 24.0), Vec2::new(CARD_W - 16.0, 30.0) * k), info("the rendezvous rule (go-pflow)")
        .fact("what it says", "The transitions in an EventLink class become one transition whose pre/postset is the union of theirs and whose guard is their conjunction: a rendezvous, so it is enabled only when every participant is.")
        .fact("paper", PAPER_SYNC)
        .cite(c_rendezvous()).cite(c_classes()).cite(c_every_member()));
    // the binary links behind the class
    yy += 13.0;
    c.text(
        xf.pt(cx + 12.0, yy - 3.5),
        Align2::LEFT_CENTER,
        &format!(
            "from {} binary event link{}, joined by union-find (link order does not matter):",
            cl.clinks.len(),
            if cl.clinks.len() == 1 { "" } else { "s" }
        ),
        9.5 * k,
        MUTED.gamma_multiply(al),
    );
    for l in &cl.clinks {
        yy += 12.0;
        let t = format!(
            "{} {}:{} — {}:{}",
            kind_glyph("event"),
            s(&l["from"]["subnet"]),
            end_name(&l["from"]),
            s(&l["to"]["subnet"]),
            end_name(&l["to"])
        );
        let w = c.mono(
            xf.pt(cx + 24.0, yy - 3.0),
            Align2::LEFT_CENTER,
            &t,
            9.5 * k,
            INK.gamma_multiply(al),
        );
        if !s(&l["idiom"]).is_empty() {
            c.mono(
                Pos2::new(w.right() + 8.0 * k, w.center().y),
                Align2::LEFT_CENTER,
                &format!("pat:{}", s(&l["idiom"])),
                9.5 * k,
                MUTED.gamma_multiply(al),
            );
        }
        c.hover(
            w.expand2(Vec2::new(0.0, 2.0)),
            link_info(l).cite(c_union_find()),
        );
    }
    yy += 6.0;
    // pre/post diagram: the union of the members' arcs into one bar
    let bar_x = cx + CARD_W / 2.0 - 6.0;
    let (pre_x, post_x) = (bar_x - 52.0, bar_x + 12.0 + 52.0);
    yy += 14.0;
    c.text(
        xf.pt(pre_x + 6.0, yy - 2.0),
        Align2::RIGHT_CENTER,
        "preset ∪",
        9.5 * k,
        MUTED.gamma_multiply(al),
    );
    c.text(
        xf.pt(post_x - 6.0, yy - 2.0),
        Align2::LEFT_CENTER,
        "∪ postset",
        9.5 * k,
        MUTED.gamma_multiply(al),
    );
    let rows_top = yy + 4.0;
    let bar_h = cl.nrows as f32 * 18.0;
    let left: Vec<(&Value, bool)> = cl
        .pre
        .iter()
        .map(|a| (a, false))
        .chain(cl.reads.iter().map(|a| (a, true)))
        .collect();
    let off = |n: usize| (cl.nrows - n) as f32 * 18.0 / 2.0;
    let place_circle =
        |c: &mut Ctx, px: f32, py: f32, pid: &str, anchor: Align2, arc: &Value, read: bool| {
            let at = xf.pt(px, py);
            c.p.circle(
                at,
                6.0 * k,
                Color32::WHITE,
                Stroke::new(
                    1.2 * k.max(0.8),
                    if read { kind_color("guard") } else { INK },
                ),
            );
            token_dots(c, at, mark(pid), k);
            let dx = if anchor == Align2::RIGHT_CENTER {
                -10.0
            } else {
                10.0
            };
            let w = c.mono(
                xf.pt(px + dx, py),
                anchor,
                pid,
                10.0 * k,
                INK.gamma_multiply(al),
            );
            let ph = arr(&flat["places"])
                .iter()
                .find(|p| s(&p["id"]) == pid)
                .cloned()
                .unwrap_or(Value::Null);
            let w2 = arr(&arc["weight"]).len();
            let _ = w2;
            c.hover(
                w.union(Rect::from_center_size(at, Vec2::splat(12.0 * k))),
                info(format!("flat place {pid}"))
                    .fact("initial", ph["initial"].to_string())
                    .fact(
                        "arc",
                        format!(
                            "{} → {}, weight {}{}",
                            s(&arc["from"]),
                            s(&arc["to"]),
                            arc["weight"].as_i64().unwrap_or(1),
                            if read {
                                " (read arc: moves nothing)"
                            } else {
                                ""
                            }
                        ),
                    )
                    .cites_of(&json!([])),
            );
        };
    for (i, (a, read)) in left.iter().enumerate() {
        let py = rows_top + off(left.len()) + i as f32 * 18.0 + 9.0;
        let (p0, p1) = (xf.pt(pre_x + 6.0, py), xf.pt(bar_x - 1.0, py));
        let col = if *read {
            kind_color("guard")
        } else {
            hex("#6b7280")
        };
        if *read {
            c.p.extend(Shape::dashed_line(
                &[p0, p1],
                Stroke::new(1.2 * k.max(0.8), col),
                4.0 * k,
                3.0 * k,
            ));
        } else {
            c.line(p0, p1, col, 1.2 * k.max(0.8));
        }
        c.arrow(p1 - Vec2::new(0.5, 0.0), p1, col, 0.1, 6.0 * k);
        let w = a["weight"].as_i64().unwrap_or(1);
        if w > 1 {
            c.text(
                xf.pt((pre_x + bar_x) / 2.0, py - 3.0),
                Align2::CENTER_CENTER,
                &format!("×{w}"),
                9.0 * k,
                INK,
            );
        }
        c.hover_seg(p0, p1, 4.0, info(format!("{} {} → {}", if *read { "read arc" } else { "input arc" }, s(&a["from"]), s(&a["to"])))
            .fact("weight", w.to_string())
            .fact(if *read { "from" } else { "merge" }, if *read { "a guard link with the default condition \"> 0\", lowered to a read arc of weight 1 on the gated flat transition: it tests, moves nothing" } else { "duplicate arcs are summed (MergeSum)" })
            .cite(if *read { c_guard_lower() } else { flatten_cites_b()[4].clone() }));
        place_circle(c, pre_x, py, s(&a["from"]), Align2::RIGHT_CENTER, a, *read);
    }
    for (i, a) in cl.post.iter().enumerate() {
        let py = rows_top + off(cl.post.len()) + i as f32 * 18.0 + 9.0;
        let (p0, p1) = (xf.pt(bar_x + 12.0, py), xf.pt(post_x - 7.0, py));
        c.arrow(p0, p1, hex("#6b7280"), 1.2 * k.max(0.8), 6.0 * k);
        let w = a["weight"].as_i64().unwrap_or(1);
        if w > 1 {
            c.text(
                xf.pt((post_x + bar_x + 12.0) / 2.0, py - 3.0),
                Align2::CENTER_CENTER,
                &format!("×{w}"),
                9.0 * k,
                INK,
            );
        }
        c.hover_seg(
            p0,
            p1,
            4.0,
            info(format!("output arc {} → {}", s(&a["from"]), s(&a["to"])))
                .fact("weight", w.to_string())
                .cite(flatten_cites_b()[4].clone()),
        );
        place_circle(c, post_x, py, s(&a["to"]), Align2::LEFT_CENTER, a, false);
    }
    let bar = Rect::from_min_size(xf.pt(bar_x, rows_top), Vec2::new(12.0, bar_h) * k);
    c.p.rect(
        bar,
        2.0 * k,
        if cl.why.is_empty() {
            hex("#e8f3ec")
        } else {
            hex("#f4f5f7")
        },
        Stroke::new(1.6 * k.max(0.8), ev),
        StrokeKind::Middle,
    );
    if let Some(fl) = fl {
        c.hover(bar, flat_transition_info(fl)
            .fact("preset (union)", if left.is_empty() { "∅".to_string() } else { left.iter().map(|(a, _)| s(&a["from"]).to_string()).collect::<Vec<_>>().join(", ") })
            .fact("postset (union)", if cl.post.is_empty() { "∅".to_string() } else { cl.post.iter().map(|a| s(&a["to"]).to_string()).collect::<Vec<_>>().join(", ") })
            .note("Its pre- and postsets are the members' together; in the paper the composite transition's are the sums of the synchronised transitions' (Def. 4.6).")
            .cite(c_rendezvous()));
    }
    yy = rows_top + bar_h + 8.0;
    // the pool-leak caveat, on the class whose link carries it
    if let Some((link, caveat)) = &cl.leak {
        let hh = 18.0 + cl.leak_lines.len() as f32 * 12.0;
        let lr = Rect::from_min_size(xf.pt(cx + 10.0, yy), Vec2::new(CARD_W - 20.0, hh) * k);
        c.p.rect(
            lr,
            4.0 * k,
            hex("#fdf0ee"),
            Stroke::new(0.9, LEAK),
            StrokeKind::Middle,
        );
        let w = c.text(
            xf.pt(cx + 18.0, yy + 8.0),
            Align2::LEFT_CENTER,
            "⚠ pool leak: release fires only with commit",
            10.0 * k,
            LEAK,
        );
        c.text(
            Pos2::new(w.right() + 6.0, w.center().y),
            Align2::LEFT_CENTER,
            "(idiom caveat, patterns.ttl:496)",
            9.5 * k,
            MUTED,
        );
        for (i, t) in cl.leak_lines.iter().enumerate() {
            c.text(
                xf.pt(cx + 18.0, yy + 20.0 + i as f32 * 12.0),
                Align2::LEFT_CENTER,
                t,
                9.5 * k,
                hex("#8a2d22"),
            );
        }
        let _ = caveat;
        c.hover(lr, link_info(link)
            .fact("caveat", "the pool-leak caveat (idiom, assistant judgment)")
            .note("A rendezvous with commit only: on the rollback path no port fires, so workers/in_use keeps the unit. The refund scenario of the Flat view is expected to fail on exactly this.")
            .cite(c_leak()));
    }
    c.p = saved;
    let _ = lay;
}

fn lerp_col(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub(super) fn boundaries_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, ar) = net_counts(c);
    c.scene.emit("na", "boundaries", json!({}));
    c.note("na", "Does not apply: a search plan composes pieces into slots (plan.rs's check decides); nothing synchronises transitions across a boundary. And the Connect-4 net is closed: it has no ports, so as a net with boundaries it is N : 0 → 0 (Bruni et al. Def. 4.1 with m = n = 0; Remark 3.6 for C/E nets), with nothing to compose along. The honest nearest drawing is that closed box; the plan is drawn in the Operad and Lens views.");
    let (w, h) = (1150.0, 430.0);
    let mut xf = Xf::fit(area, w, h, 1.4);
    xf.origin.x += ((area.width() - w * xf.k) / 2.0).max(0.0);
    let k = xf.k;
    let drops =
        c.d.c4
            .as_ref()
            .map(|d| {
                d.transitions
                    .iter()
                    .filter(|t| t.id.contains("_play_"))
                    .count()
            })
            .unwrap_or(0);
    let name =
        c.d.c4
            .as_ref()
            .and_then(|d| d.extra.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("connect-4")
            .to_string();
    let desc =
        c.d.c4
            .as_ref()
            .and_then(|d| d.extra.get("description"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
    let (bx, by, bw, bh) = (395.0, 70.0, 360.0, 165.0);
    for (x, side) in [(bx - 110.0, "left"), (bx + bw + 110.0, "right")] {
        let (a, b) = (xf.pt(x, by + 10.0), xf.pt(x, by + bh - 10.0));
        c.p.extend(Shape::dashed_line(
            &[a, b],
            Stroke::new(1.4 * k.max(0.8), hex("#9aa1aa")),
            5.0 * k,
            4.0 * k,
        ));
        let z = xf.pt(x, by + bh / 2.0 + 6.0);
        c.p.rect_filled(
            Rect::from_center_size(z, Vec2::new(24.0, 26.0) * k),
            0.0,
            PAPER,
        );
        c.mono(z, Align2::CENTER_CENTER, "0", 22.0 * k, INK);
        c.text(
            xf.pt(x, by + bh + 8.0),
            Align2::CENTER_TOP,
            &format!("{side} boundary: no ports"),
            10.5 * k,
            MUTED,
        );
        c.hover(Rect::from_min_max(xf.pt(x - 14.0, by), xf.pt(x + 14.0, by + bh)), info(format!("the {side} boundary: 0 ports"))
            .fact("what it is", "The Connect-4 net declares no ports, so neither boundary has a port to synchronise on.")
            .fact("paper", PAPER_CLOSED)
            .fact("consequence", PAPER_NO_INTERACTION)
            .cite(c_fixture()));
    }
    let r = Rect::from_min_size(xf.pt(bx, by), Vec2::new(bw, bh) * k);
    c.p.rect(
        r,
        6.0 * k,
        Color32::WHITE,
        Stroke::new(1.3 * k.max(0.8), hex("#333333")),
        StrokeKind::Middle,
    );
    c.p.rect_filled(
        Rect::from_min_size(
            r.left_top() + Vec2::splat(1.0),
            Vec2::new(r.width() - 2.0, 30.0 * k - 1.0),
        ),
        5.0 * k,
        hex("#eef0f3"),
    );
    c.line(
        xf.pt(bx, by + 30.0),
        xf.pt(bx + bw, by + 30.0),
        hex("#d5d9df"),
        1.0,
    );
    c.text(
        xf.pt(bx + 12.0, by + 15.0),
        Align2::LEFT_CENTER,
        &name,
        13.0 * k,
        INK,
    );
    c.mono(
        xf.pt(bx + bw - 12.0, by + 15.0),
        Align2::RIGHT_CENTER,
        "N : 0 → 0",
        12.0 * k,
        INK,
    );
    let rows = [
        (pl.to_string(), "places".to_string()),
        (
            tr.to_string(),
            format!(
                "transitions: {drops} drops (x_/o_play_*), {} others",
                tr - drops as i64
            ),
        ),
        (ar.to_string(), "arcs".to_string()),
        (
            "0".to_string(),
            "ports: nothing is exposed to a boundary".to_string(),
        ),
    ];
    for (i, (n, t)) in rows.iter().enumerate() {
        let y = by + 55.0 + i as f32 * 26.0;
        c.mono(xf.pt(bx + 70.0, y), Align2::RIGHT_CENTER, n, 17.0 * k, INK);
        c.text(xf.pt(bx + 80.0, y), Align2::LEFT_CENTER, t, 11.5 * k, MUTED);
    }
    c.mono(
        xf.pt(bx + bw / 2.0, by + bh + 22.0),
        Align2::CENTER_CENTER,
        &format!(
            "pflow-rs crates/pflow-metamodel/tests/fixtures/connect-4.json @{} (MIT)",
            sp::PLAN_RS_REV
        ),
        9.5 * k,
        MUTED,
    );
    c.hover(r, info(format!("{name}: a closed net"))
        .fact("description", desc)
        .fact("places / transitions / arcs", format!("{pl} / {tr} / {ar}"))
        .fact("drops (names with _play_)", drops.to_string())
        .fact("other transitions", format!("{} (the fixture's description: catalytic win detectors)", tr - drops as i64))
        .fact("ports", "0: the fixture declares none")
        .fact("as a net with boundaries", "N : 0 → 0 (Def. 4.1, m = n = 0)")
        .note("Composing it with another closed net would only be their disjoint union (Remark 3.6, for C/E nets): there is no transition synchronisation to draw. The fixture is a plain P/T net (no inhibitor arcs, no capacities), so Def. 4.1 applies as stated.")
        .cite(c_fixture()));
    // what the plan is, and where it is drawn
    let ty = by + bh + 58.0;
    let plan_json =
        c.d.plan
            .as_ref()
            .map(|p| s(&p["json"]).to_string())
            .unwrap_or_default();
    let canon = sp::canonical(&sp::connect4_exact()).to_string();
    let is_example = plan_json == canon;
    let err =
        c.d.plan
            .as_ref()
            .and_then(|p| p["check"]["error"].as_str().map(String::from));
    let line1 = format!(
        "The search plan {}fills slots with pieces; it has no transitions to synchronise.",
        if is_example {
            "plan:connect4-exact "
        } else {
            "(edited from plan:connect4-exact) "
        }
    );
    c.text(
        xf.pt(w / 2.0, ty),
        Align2::CENTER_CENTER,
        &line1,
        12.0 * k,
        INK,
    );
    let second = match &err {
        Some(e) => format!("The current plan is refused: {e}"),
        None => format!("Current plan: {plan_json}"),
    };
    let max = ((w - 40.0) / (10.5 * 0.56)) as usize;
    let lines: Vec<String> = if second.chars().count() > max {
        match second[..max.min(second.len())].rfind(',') {
            Some(cut) => vec![second[..=cut].to_string(), second[cut + 1..].to_string()],
            None => bl::wrap(&second, max),
        }
    } else {
        vec![second]
    };
    for (i, l) in lines.iter().enumerate() {
        c.text(
            xf.pt(w / 2.0, ty + 18.0 + i as f32 * 14.0),
            Align2::CENTER_CENTER,
            l,
            10.5 * k,
            MUTED,
        );
    }
    c.hover(Rect::from_min_size(xf.pt(40.0, ty - 12.0), Vec2::new(w - 80.0, 50.0) * k), info("the search plan (not a net with boundaries)")
        .fact("what it is", "SearchPlan JSON, checked by plan.rs SearchPlan::check: order pieces, drivers, filters, heuristics and memo go into slots. No piece is a transition, and no slot is a boundary port.")
        .fact("plan", plan_json.clone())
        .fact("check", err.unwrap_or_else(|| "accepted".into()))
        .cite(pflow_rs("plan.connect4Exact")));
    // pointers to the views that draw the plan
    for (x, view, label, sub) in [
        (
            w / 2.0 - 262.0,
            "operad",
            "Operad view",
            "pieces filling a solver template's slots",
        ),
        (
            w / 2.0 + 12.0,
            "lens",
            "Lens view",
            "drivers around the search core",
        ),
    ] {
        let g = Rect::from_min_size(xf.pt(x, ty + 40.0), Vec2::new(250.0, 46.0) * k);
        c.p.rect(
            g,
            8.0 * k,
            Color32::WHITE,
            Stroke::new(1.3 * k.max(0.8), kind_color("data")),
            StrokeKind::Middle,
        );
        c.text(
            xf.pt(x + 16.0, ty + 59.0),
            Align2::LEFT_CENTER,
            &format!("{label} →"),
            12.5 * k,
            kind_color("data"),
        );
        c.text(
            xf.pt(x + 16.0, ty + 76.0),
            Align2::LEFT_CENTER,
            sub,
            10.5 * k,
            MUTED,
        );
        c.hand_if(g);
        c.hover(g, info(format!("go to the {label}")).fact("why there", if view == "operad" { "a plan is an outer box with typed slots filled by pieces (operadic composition; ROADMAP Phase 6)" } else { "drivers wrap the core, play forward and value back (an analogy, labelled there)" }));
        if c.click_in(g) {
            c.command("view.composition", json!({"view": view}));
        }
    }
}

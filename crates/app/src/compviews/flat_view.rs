//! The flat view (see `docs/composition-views-scene.md`): the one place/
//! transition net go-pflow's Flatten produces, drawn the way RenderFlatDOT
//! marks it, with a token game (`bundle.fire`) and the frozen scenarios
//! stepped one at a time (`bundle.scenario`, `bundle.step`). Port of the
//! prototype's `js/views/flat.js`.

use super::d_draw::*;
use super::d_flat_layout::{self as lay, Anchor, Lay, Node};
use super::*;

fn flatten_cites() -> Vec<String> {
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

fn c_flat_dot() -> String {
    go(
        "metamodel/composedot.go",
        59,
        115,
        "RenderFlatDOT: the flattened model, fused elements marked",
    )
}
fn c_dot_doc() -> String {
    go(
        "metamodel/composedot.go",
        9,
        19,
        "a place is a circle, a transition a box; fused elements are double-bordered",
    )
}
fn c_wire_peri() -> String {
    go(
        "metamodel/composedot.go",
        80,
        83,
        "a wire (places fused into one slot): peripheries=2",
    )
}
fn c_fused_peri() -> String {
    go(
        "metamodel/composedot.go",
        97,
        100,
        "several transitions that now fire as one: peripheries=2",
    )
}
fn c_arc_attrs() -> String {
    go(
        "metamodel/composedot.go",
        243,
        263,
        "arcAttrs: inhibitor = circle head, read = dashed and undirected, weight > 1 labelled",
    )
}
fn c_guard_lower() -> String {
    go(
        "metamodel/compose_flatten.go",
        424,
        482,
        "applyGuardLinks: a guard link becomes a read (or inhibitor) arc",
    )
}
fn c_constraints() -> String {
    go(
        "metamodel/compose_flatten.go",
        484,
        504,
        "rewriteConstraints: qualified IDs, place references rewritten",
    )
}
fn c_initial() -> String {
    go(
        "metamodel/firing.go",
        49,
        58,
        "InitialMarking: token places only",
    )
}
fn c_why_not() -> String {
    go(
        "metamodel/firing.go",
        142,
        196,
        "EnabledWhyNot: the refusal wording shown here",
    )
}
fn c_fire() -> String {
    go(
        "metamodel/firing.go",
        198,
        212,
        "Fire: read and inhibitor arcs move nothing",
    )
}
fn c_leak() -> String {
    onto(
        "ontology",
        "ns/patterns.ttl",
        493,
        497,
        "pat:idiom-pool-release-saga-commit; its ns:caveat (line 496): \"Fires only on commit…\"",
    )
}
fn c_pool_inv() -> String {
    onto(
        "ontology",
        "ns/patterns.ttl",
        272,
        275,
        "pat:resource-pool-inv-conservation",
    )
}
fn c_arc_classes() -> String {
    onto(
        "ontology",
        "ns/core.ttl",
        25,
        51,
        "ns:Arc and its subclasses InputArc, OutputArc, ReadArc, InhibitorArc",
    )
}
fn c_faith() -> String {
    onto(
        "ontologyRoadmap",
        "ROADMAP.md",
        247,
        295,
        "faithfulness checks: vacuity, scenario, semantic",
    )
}
fn c_exit41() -> String {
    onto(
        "ontologyRoadmap",
        "ROADMAP.md",
        240,
        242,
        "4.1 exit: \"a failed conservation check on the rollback path\"",
    )
}
fn c_worked() -> String {
    onto(
        "ontologyRoadmap",
        "ROADMAP.md",
        276,
        280,
        "worked expectation: the rollback scenario fails, the worker never returns",
    )
}
fn c_d44() -> String {
    onto(
        "ontologyRoadmap",
        "ROADMAP.md",
        289,
        291,
        "4.4: the rollback scenario fails with the pool-leak evidence",
    )
}

fn with_cites(mut i: Info, cs: Vec<String>) -> Info {
    i.cites.extend(cs);
    i
}

const LEAK_WORD: &str = "Fires only on commit. If the saga rolls back, no port fires, the worker stays in in_use and the pool leaks a unit. Releasing on both outcomes is not expressible with the declared ports.";

/// The flat element `member` ("subnet/local") ended up in.
fn flat_of(flat: &Value, member: &str) -> Option<String> {
    let (sn, loc) = member.split_once('/')?;
    flat["transition_map"][sn][loc]
        .as_str()
        .or_else(|| flat["place_map"][sn][loc].as_str())
        .map(str::to_string)
}

fn marking_of(game: &Value, id: &str) -> i64 {
    game["marking"][id].as_i64().unwrap_or(0)
}

fn is_enabled(game: &Value, id: &str) -> bool {
    arr(&game["enabled"]).iter().any(|e| s(e) == id)
}

fn why_not(game: &Value, id: &str) -> String {
    arr(&game["transitions"])
        .iter()
        .find(|t| s(&t["id"]) == id)
        .map(|t| s(&t["why"]).to_string())
        .unwrap_or_default()
}

/// Where a straight arc from `n` towards (tx, ty) leaves `n`'s boundary.
fn boundary(n: &Node, tx: f32, ty: f32, pad: f32) -> Pos2 {
    let (dx, dy) = (tx - n.x, ty - n.y);
    let len = dx.hypot(dy).max(1e-6);
    if n.place {
        let r = n.r + pad;
        return Pos2::new(n.x + dx / len * r, n.y + dy / len * r);
    }
    let (hw, hh) = (n.w / 2.0 + pad, n.h / 2.0 + pad);
    let k = (hw / dx.abs().max(1e-9)).min(hh / dy.abs().max(1e-9));
    Pos2::new(n.x + dx * k, n.y + dy * k)
}

/// The consuming inputs, outputs and tests of flat transition `t`, as
/// firing.go's Inputs/Outputs/Tests (token places only).
fn io_arcs(flat: &Value, t: &str) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let token: BTreeSet<&str> = arr(&flat["places"])
        .iter()
        .filter(|p| s(&p["kind"]) != "data")
        .map(|p| s(&p["id"]))
        .collect();
    let ro = |a: &Value| matches!(s(&a["type"]), "read" | "inhibitor");
    let (mut i, mut o, mut te) = (vec![], vec![], vec![]);
    for a in arr(&flat["arcs"]) {
        if s(&a["to"]) == t && token.contains(s(&a["from"])) {
            if ro(a) {
                te.push(a.clone());
            } else {
                i.push(a.clone());
            }
        } else if s(&a["from"]) == t && token.contains(s(&a["to"])) && !ro(a) {
            o.push(a.clone());
        }
    }
    (i, o, te)
}

/// The member arcs each flat arc came from (mergeArcs retargets them onto
/// flat ids and merges duplicates).
fn arc_sources(c: &Ctx, flat: &Value) -> BTreeMap<String, Vec<String>> {
    let mut m: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for sub in subnets(c) {
        let sid = s(&sub["id"]).to_string();
        let fl = |x: &str| -> String {
            flat["place_map"][&sid][x]
                .as_str()
                .or_else(|| flat["transition_map"][&sid][x].as_str())
                .map(str::to_string)
                .unwrap_or(format!("{sid}/{x}"))
        };
        for a in arr(&sub["arcs"]) {
            let k = format!(
                "{}\u{0}{}\u{0}{}",
                fl(s(&a["from"])),
                fl(s(&a["to"])),
                s(&a["type"])
            );
            let w = a["weight"].as_i64().unwrap_or(1);
            m.entry(k).or_default().push(format!(
                "{sid}/{} → {sid}/{}{}",
                s(&a["from"]),
                s(&a["to"]),
                if w > 1 {
                    format!(" ×{w}")
                } else {
                    String::new()
                }
            ));
        }
    }
    m
}

// ------------------------------------------------------------ drawing the net

struct Net<'a> {
    xf: Xf,
    lay: &'a Lay,
    flat: &'a Value,
    game: &'a Value,
    /// The hovered flat node, from the pointer's position this frame.
    hot: Option<usize>,
    /// Blocked transition (a scenario stopped at it).
    blocked: Option<String>,
    sources: BTreeMap<String, Vec<String>>,
}

fn node_rect(xf: &Xf, n: &Node) -> Rect {
    if n.place {
        Rect::from_center_size(xf.pt(n.x, n.y), Vec2::splat(xf.len(n.r * 2.0)))
    } else {
        Rect::from_center_size(xf.pt(n.x, n.y), Vec2::new(xf.len(n.w), xf.len(n.h)))
    }
}

fn sw(xf: &Xf, w: f32) -> f32 {
    (w * xf.k).max(0.9)
}

fn draw_clusters(c: &mut Ctx, net: &Net, subs: &[Value]) {
    let xf = net.xf;
    for cl in &net.lay.clusters {
        let r = xf.rect(cl.rect.x, cl.rect.y, cl.rect.w, cl.rect.h);
        let just = c
            .anim(|e| match e {
                Event::InstanceAdded { id } if *id == cl.sid => Some(()),
                _ => None,
            })
            .map(|(_, t)| t);
        let a = just.unwrap_or(1.0);
        c.p.rect(
            r,
            xf.len(14.0),
            pattern_tint_p(&cl.pattern).gamma_multiply(0.5 * a),
            Stroke::new(1.0, hex("#d3d9e0").gamma_multiply(a)),
            StrokeKind::Middle,
        );
        let label = comp::ontology()["patterns"][&cl.pattern]["prefLabel"]
            .as_str()
            .unwrap_or(&cl.pattern)
            .to_string();
        let t0 = xf.pt(cl.rect.x + 12.0, cl.rect.y + 11.0);
        let id = format!("{}/", cl.sid);
        let wr = c.mono(t0, Align2::LEFT_CENTER, &id, 11.5 * xf.k, INK);
        c.text(
            Pos2::new(wr.right() + 6.0 * xf.k, t0.y),
            Align2::LEFT_CENTER,
            &format!(
                "{label}{}",
                if cl.extra { " (second instance)" } else { "" }
            ),
            11.5 * xf.k,
            MUTED,
        );
        if let Some(sub) = subs.iter().find(|x| s(&x["id"]) == cl.sid) {
            let mut i = subnet_info(sub);
            i.note = format!(
                "A cluster here only groups the flat elements whose ID starts with \"{}/\". In the flat net there is no subnet any more: RenderFlatDOT draws it without clusters (composedot.go:62-115), and Flatten removes the ports.",
                cl.sid
            );
            i.cites.push(c_flat_dot());
            c.hover(r, i);
        }
    }
}

fn arc_info(net: &Net, a: &Value) -> Info {
    let (from, to) = (s(&a["from"]), s(&a["to"]));
    let ty = s(&a["type"]);
    let place_first = arr(&net.flat["places"]).iter().any(|p| s(&p["id"]) == from);
    let (cls, label) = match ty {
        "read" => ("ReadArc", "read arc"),
        "inhibitor" => ("InhibitorArc", "inhibitor arc"),
        _ if place_first => ("InputArc", "arc"),
        _ => ("OutputArc", "arc"),
    };
    let key = format!("{from}\u{0}{to}\u{0}{ty}");
    let src = net.sources.get(&key).cloned().unwrap_or_default();
    let mut i = info(format!("{label} {from} → {to}"))
        .fact("concept", format!("ns:{cls}"))
        .fact("weight", a["weight"].as_i64().unwrap_or(1).to_string())
        .fact(
            "type",
            if ty.is_empty() || ty == "normal" {
                "normal (consumes or produces)".to_string()
            } else {
                ty.to_string()
            },
        );
    i = i.fact(
        "from member arcs",
        if src.is_empty() {
            "none: added by a guard link".to_string()
        } else {
            src.join("; ")
        },
    );
    if src.len() > 1 {
        i = i.fact(
            "merged",
            format!(
                "{} member arcs onto one pair of flat IDs: MergeSum adds the weights",
                src.len()
            ),
        );
    }
    i = i.note(if ty == "read" && src.is_empty() {
        "A guard link, lowered: its default condition \"> 0\" becomes a read arc of weight 1 from the observed place to the gated transition. It moves no token."
    } else if ty == "read" {
        "A read arc: the transition needs the weight on the place and moves nothing."
    } else {
        ""
    });
    let mut cs = vec![c_arc_classes(), flatten_cites()[4].clone()];
    if ty == "read" && src.is_empty() {
        cs.push(c_guard_lower());
    }
    cs.push(c_arc_attrs());
    with_cites(i, cs)
}

fn node_info(net: &Net, n: &Node, el: &Value) -> Info {
    let flat = net.flat;
    let id = &n.id;
    let mut i = info(format!(
        "flat {} {id}",
        if n.place { "place" } else { "transition" }
    ))
    .fact(
        "concept",
        if n.place { "ns:Place" } else { "ns:Transition" },
    );
    i.note = s(&el["description"]).to_string();
    i = i.fact("members", n.members.join(", "));
    if n.place {
        let arcs = arr(&flat["arcs"]);
        let pre: Vec<String> = arcs
            .iter()
            .filter(|a| s(&a["to"]) == *id)
            .map(|a| {
                format!(
                    "{}{}",
                    s(&a["from"]),
                    if s(&a["type"]).is_empty() || s(&a["type"]) == "normal" {
                        String::new()
                    } else {
                        format!(" ({})", s(&a["type"]))
                    }
                )
            })
            .collect();
        let post: Vec<String> = arcs
            .iter()
            .filter(|a| s(&a["from"]) == *id)
            .map(|a| {
                format!(
                    "{}{}",
                    s(&a["to"]),
                    if s(&a["type"]).is_empty() || s(&a["type"]) == "normal" {
                        String::new()
                    } else {
                        format!(" ({})", s(&a["type"]))
                    }
                )
            })
            .collect();
        i = i.fact("initial", el["initial"].to_string()).fact(
            "capacity",
            if el["capacity"].as_i64().unwrap_or(0) == 0 {
                "0 (unbounded)".to_string()
            } else {
                el["capacity"].to_string()
            },
        );
        if n.fused {
            i = i.fact(
                "fused",
                "a wire: places fused by token/data links; initial markings summed",
            );
        }
        i = i
            .fact(
                "tokens now",
                if n.data_place {
                    "a data place: no marking (firing.go)".to_string()
                } else {
                    marking_of(net.game, id).to_string()
                },
            )
            .fact(
                "filled by",
                if pre.is_empty() {
                    "nothing".into()
                } else {
                    pre.join(", ")
                },
            )
            .fact(
                "read or consumed by",
                if post.is_empty() {
                    "nothing".into()
                } else {
                    post.join(", ")
                },
            );
        i.note = if n.fused {
            "A wire: places fused by token or data links into one slot. RenderFlatDOT double-borders it (peripheries=2)."
        } else {
            "One member, so the flat ID is the member's qualified ID."
        }
        .to_string();
        let mut cs = flatten_cites();
        cs.push(if n.fused { c_wire_peri() } else { c_flat_dot() });
        cs.push(c_initial());
        with_cites(i, cs)
    } else {
        let (inp, out, tests) = io_arcs(flat, id);
        let w = |a: &Value, k: &str| {
            let w = a["weight"].as_i64().unwrap_or(1);
            format!(
                "{}{}",
                s(&a[k]),
                if w > 1 {
                    format!(" ×{w}")
                } else {
                    String::new()
                }
            )
        };
        if n.fused {
            i = i.fact("fused", "one rendezvous: pre/postset is the union of the members', so it fires only when every member could");
        }
        i = i
            .fact(
                "consumes",
                if inp.is_empty() {
                    "nothing".into()
                } else {
                    inp.iter()
                        .map(|a| w(a, "from"))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            )
            .fact(
                "produces",
                if out.is_empty() {
                    "nothing".into()
                } else {
                    out.iter()
                        .map(|a| w(a, "to"))
                        .collect::<Vec<_>>()
                        .join(", ")
                },
            );
        if !tests.is_empty() {
            i = i.fact(
                "reads (read arcs)",
                tests
                    .iter()
                    .map(|a| {
                        format!(
                            "{} ≥ {}{}",
                            s(&a["from"]),
                            a["weight"].as_i64().unwrap_or(1),
                            if s(&a["type"]) == "read" {
                                ""
                            } else {
                                " (inhibitor)"
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", "),
            );
        }
        let why = why_not(net.game, id);
        i = i.fact(
            "at this marking",
            if is_enabled(net.game, id) {
                "enabled: click to fire".to_string()
            } else {
                format!("not enabled: {why}")
            },
        );
        i.note = if n.fused {
            "Several member transitions that now fire as one (event links). RenderFlatDOT double-borders it (peripheries=2). It fires only when every member could."
        } else {
            "One member, so the flat ID is the member's qualified ID."
        }
        .to_string();
        let mut cs = flatten_cites();
        cs.push(if n.fused {
            c_fused_peri()
        } else {
            c_flat_dot()
        });
        cs.push(c_why_not());
        cs.push(c_fire());
        with_cites(i, cs)
    }
}

fn draw_arcs(c: &mut Ctx, net: &Net, fade: &BTreeMap<String, f32>) {
    let xf = net.xf;
    for a in arr(&net.flat["arcs"]) {
        let (Some(na), Some(nb)) = (net.lay.node(s(&a["from"])), net.lay.node(s(&a["to"]))) else {
            continue;
        };
        let ty = s(&a["type"]);
        let ro = ty == "read" || ty == "inhibitor";
        let p0 = xf_pt(&xf, boundary(na, nb.x, nb.y, 1.0));
        let p1 = xf_pt(&xf, boundary(nb, na.x, na.y, if ro { 1.0 } else { 2.0 }));
        let hi_node = net
            .hot
            .is_some_and(|h| net.lay.nodes[h].id == na.id || net.lay.nodes[h].id == nb.id);
        let hot_arc = !hi_node
            && net.hot.is_none()
            && c.pointer_pos()
                .is_some_and(|p| dist_to_seg(p, p0, p1) <= 5.0);
        let hi = hi_node || hot_arc;
        let alpha = fade
            .get(&na.id)
            .or_else(|| fade.get(&nb.id))
            .copied()
            .unwrap_or(1.0);
        let wgt = a["weight"].as_i64().unwrap_or(1);
        match ty {
            "read" => {
                let col = READ.gamma_multiply(alpha);
                c.p.extend(Shape::dashed_line(
                    &[p0, p1],
                    Stroke::new(sw(&xf, if hi { 2.0 } else { 1.5 }), col),
                    5.0 * xf.k,
                    3.5 * xf.k,
                ));
                c.p.circle_filled(p1, (2.6 * xf.k).max(2.0), col);
            }
            "inhibitor" => {
                let col = INHIB.gamma_multiply(alpha);
                c.line(p0, p1, col, sw(&xf, 1.25));
                c.p.circle(
                    p1,
                    (3.4 * xf.k).max(2.5),
                    Color32::WHITE,
                    Stroke::new(sw(&xf, 1.4), col),
                );
            }
            _ => {
                let col = if hi { INK } else { ARC }.gamma_multiply(alpha);
                c.arrow(
                    p0,
                    p1,
                    col,
                    sw(&xf, if hi { 1.8 } else { 1.25 }),
                    (7.0 * xf.k).max(5.0),
                );
            }
        }
        if wgt > 1 {
            let m = p0 + (p1 - p0) * 0.5;
            let r = Rect::from_center_size(m, Vec2::new(16.0, 15.0) * xf.k.max(0.8));
            c.p.rect(
                r,
                r.height() / 2.0,
                Color32::WHITE,
                Stroke::new(0.8, ARC),
                StrokeKind::Middle,
            );
            c.text(
                r.center(),
                Align2::CENTER_CENTER,
                &wgt.to_string(),
                10.0 * xf.k.max(0.85),
                INK,
            );
        }
        c.hover_seg(p0, p1, 5.0, arc_info(net, a));
    }
}

fn xf_pt(xf: &Xf, p: Pos2) -> Pos2 {
    xf.pt(p.x, p.y)
}

fn draw_label(c: &Ctx, xf: &Xf, n: &Node) {
    let (b, anchor, tx) = lay::label_box(n, n.lab);
    let al = |a: Anchor| match a {
        Anchor::Start => Align2::LEFT_CENTER,
        Anchor::Middle => Align2::CENTER_CENTER,
        Anchor::End => Align2::RIGHT_CENTER,
    };
    let x = xf.pt(tx, 0.0).x;
    c.halo(
        Pos2::new(x, xf.pt(0.0, b.y0 + 6.0).y),
        al(anchor),
        &n.lines[0],
        lay::FS_PRE * xf.k,
        MUTED,
        true,
    );
    c.halo(
        Pos2::new(x, xf.pt(0.0, b.y0 + 19.0).y),
        al(anchor),
        &n.lines[1],
        lay::FS_LOC * xf.k,
        INK,
        true,
    );
}

fn draw_nodes(c: &mut Ctx, net: &Net, appear: &BTreeMap<String, f32>) {
    let xf = net.xf;
    let places = arr(&net.flat["places"]);
    let trans = arr(&net.flat["transitions"]);
    for (ni, n) in net.lay.nodes.iter().enumerate() {
        let el = if n.place {
            places.iter().find(|p| s(&p["id"]) == n.id)
        } else {
            trans.iter().find(|p| s(&p["id"]) == n.id)
        }
        .cloned()
        .unwrap_or(Value::Null);
        // a fused element that is merging in appears when its members arrive
        let a = appear.get(&n.id).copied().unwrap_or(1.0);
        let at = xf.pt(n.x, n.y);
        let r = node_rect(&xf, n);
        if a > 0.0 {
            if n.place {
                let rad = xf.len(n.r);
                let col = INK.gamma_multiply(a);
                if n.data_place {
                    let rr = Rect::from_center_size(at, Vec2::new(rad * 2.0, rad * 1.6));
                    c.p.rect(
                        rr,
                        2.0,
                        Color32::WHITE.gamma_multiply(a),
                        Stroke::new(sw(&xf, 1.3), col),
                        StrokeKind::Middle,
                    );
                } else {
                    c.p.circle(
                        at,
                        rad,
                        Color32::WHITE.gamma_multiply(a),
                        Stroke::new(sw(&xf, 1.4), col),
                    );
                    if n.fused {
                        c.p.circle(
                            at,
                            rad - xf.len(3.5),
                            Color32::WHITE.gamma_multiply(a),
                            Stroke::new(sw(&xf, 1.3), col),
                        );
                    }
                    c.tokens(
                        at,
                        marking_of(net.game, &n.id),
                        if n.fused { rad - xf.len(3.5) } else { rad },
                    );
                }
                draw_label(c, &xf, n);
            } else {
                let en = is_enabled(net.game, &n.id);
                let blocked = net.blocked.as_deref() == Some(n.id.as_str());
                let (fill, stroke) = if en {
                    (EN_FILL, Stroke::new(sw(&xf, 2.0), EN))
                } else if blocked {
                    (hex("#fbecea"), Stroke::new(sw(&xf, 2.2), BLOCKED))
                } else {
                    (PAPER, Stroke::new(sw(&xf, 1.3), INK))
                };
                let (fill, stroke) = (
                    fill.gamma_multiply(a),
                    Stroke::new(stroke.width, stroke.color.gamma_multiply(a)),
                );
                if n.fused {
                    c.p.rect(
                        r,
                        xf.len(7.0),
                        Color32::WHITE.gamma_multiply(a),
                        Stroke::new(sw(&xf, 1.3), INK.gamma_multiply(a)),
                        StrokeKind::Middle,
                    );
                    c.p.rect(
                        r.shrink(xf.len(3.5)),
                        xf.len(5.0),
                        fill,
                        stroke,
                        StrokeKind::Middle,
                    );
                    let k = n.lines.len() as f32;
                    for (i, l) in n.lines.iter().enumerate() {
                        let y = -((k - 1.0) * 13.0) / 2.0 + i as f32 * 13.0;
                        c.mono(
                            xf.pt(n.x, n.y + y),
                            Align2::CENTER_CENTER,
                            l,
                            lay::FS_FUSED * xf.k,
                            INK.gamma_multiply(a),
                        );
                    }
                } else {
                    c.p.rect(r, xf.len(6.0), fill, stroke, StrokeKind::Middle);
                    c.mono(
                        xf.pt(n.x, n.y - 6.0),
                        Align2::CENTER_CENTER,
                        &n.lines[0],
                        lay::FS_PRE * xf.k,
                        MUTED.gamma_multiply(a),
                    );
                    c.mono(
                        xf.pt(n.x, n.y + 6.0),
                        Align2::CENTER_CENTER,
                        &n.lines[1],
                        lay::FS_LOC * xf.k,
                        INK.gamma_multiply(a),
                    );
                }
                if en {
                    c.hand_if(r);
                    if c.click_in(r) {
                        c.command("bundle.fire", json!({"transition": n.id}));
                    }
                }
            }
        }
        let hit = if n.place { r } else { r.expand(2.0) };
        c.hover(hit, node_info(net, n, &el));
        // scene manifest: collected while drawing, in the resting state
        if n.place {
            c.scene.emit(
                "place",
                &n.id,
                json!({"initial": el["initial"].as_i64().unwrap_or(0), "fused": n.fused}),
            );
        } else {
            c.scene.emit("transition", &n.id, json!({"fused": n.fused}));
        }
        let _ = ni;
    }
}

impl<'a> Ctx<'a> {
    pub fn hand_if(&self, r: Rect) {
        if self.pointer_in(r) {
            self.hand();
        }
    }
}

fn draw_legend(c: &mut Ctx, xf: &Xf, l: &lay::R) {
    let r = xf.rect(l.x, l.y, l.w, l.h);
    c.p.rect(
        r,
        xf.len(10.0),
        Color32::WHITE,
        Stroke::new(1.0, hex("#dde1e6")),
        StrokeKind::Middle,
    );
    let rows = [
        "place, with its tokens",
        "transition",
        "enabled: click to fire",
        "fused (peripheries=2)",
        "arc; weight > 1 written on it",
        "read arc (guard link, lowered)",
        "cluster: one subnet prefix (page)",
    ];
    for (i, text) in rows.iter().enumerate() {
        let y = 18.0 + i as f32 * 21.5;
        let g = xf.pt(l.x + 22.0, l.y + y);
        let k = xf.k;
        match i {
            0 => {
                c.p.circle(g, 8.0 * k, Color32::WHITE, Stroke::new(sw(xf, 1.3), INK));
                c.p.circle_filled(g, 2.4 * k, INK);
            }
            1 => {
                c.p.rect(
                    Rect::from_center_size(g, Vec2::new(22.0, 14.0) * k),
                    4.0 * k,
                    PAPER,
                    Stroke::new(sw(xf, 1.3), INK),
                    StrokeKind::Middle,
                );
            }
            2 => {
                c.p.rect(
                    Rect::from_center_size(g, Vec2::new(22.0, 14.0) * k),
                    4.0 * k,
                    EN_FILL,
                    Stroke::new(sw(xf, 2.0), EN),
                    StrokeKind::Middle,
                );
            }
            3 => {
                c.double_rect(
                    Rect::from_center_size(g, Vec2::new(24.0, 16.0) * k),
                    5.0 * k,
                    PAPER,
                    Stroke::new(sw(xf, 1.2), INK),
                    3.0 * k,
                );
            }
            4 => {
                c.arrow(
                    g - Vec2::new(14.0, 0.0) * k,
                    g + Vec2::new(13.0, 0.0) * k,
                    ARC,
                    sw(xf, 1.3),
                    6.0 * k,
                );
            }
            5 => {
                c.p.extend(Shape::dashed_line(
                    &[g - Vec2::new(14.0, 0.0) * k, g + Vec2::new(11.0, 0.0) * k],
                    Stroke::new(sw(xf, 1.5), READ),
                    5.0 * k,
                    3.5 * k,
                ));
                c.p.circle_filled(g + Vec2::new(12.0, 0.0) * k, 2.6 * k, READ);
            }
            _ => {
                c.p.rect(
                    Rect::from_center_size(g, Vec2::new(24.0, 14.0) * k),
                    5.0 * k,
                    hex("#f1ebfa"),
                    Stroke::new(1.0, hex("#d3d9e0")),
                    StrokeKind::Middle,
                );
            }
        }
        c.text(
            xf.pt(l.x + 44.0, l.y + y),
            Align2::LEFT_CENTER,
            text,
            10.5 * k,
            INK,
        );
    }
    c.text(
        xf.pt(l.x + 12.0, l.y + l.h - 8.0),
        Align2::LEFT_CENTER,
        "shapes: go-pflow RenderFlatDOT",
        9.5 * xf.k,
        MUTED,
    );
    c.hover(
        r,
        info("How the flat net is drawn")
            .fact("a place", "a circle; a transition a rounded box; a wire or fused transition is double-bordered (peripheries=2)")
            .fact("arcs", "an inhibitor arc has a circle head, a read arc is dashed and undirected, a weight above 1 is the arc's label")
            .fact("the dot on a read arc", "this page's mark for the transition end (RenderFlatDOT draws the read arc with no head)")
            .fact("clusters", "this page's grouping by member subnet; RenderFlatDOT draws none")
            .fact("green", "enabled at the current marking (firing.go EnabledWhyNot returns nil)")
            .note("RenderFlatDOT: a place is a circle, a transition a rounded box, a wire or fused transition is double-bordered (peripheries=2). arcAttrs: an inhibitor arc has a circle head, a read arc is dashed and undirected, a weight above 1 is the arc's label.")
            .cite(c_dot_doc())
            .cite(c_flat_dot())
            .cite(c_wire_peri())
            .cite(c_fused_peri())
            .cite(c_arc_attrs())
            .cite(c_why_not()),
    );
}

// ------------------------------------------------------------ the merge animation

/// What a link that was just added fused, and how far along the animation is:
/// the flat ids that grow in, and the member ghosts that slide together.
struct Merge {
    t: f32,
    kind: String,
    targets: Vec<String>,
    /// A guard link's read arc (place id, transition id) drawn in.
    guard: Option<(String, String)>,
}

fn merge_of(c: &Ctx, flat: &Value) -> Option<Merge> {
    let ((key, kind), t) = c.anim(|e| match e {
        Event::LinkAdded { key, kind } => Some((key.clone(), kind.clone())),
        _ => None,
    })?;
    let st = c.d.state.as_ref()?;
    let l = arr(&st["links"]).iter().find(|l| s(&l["key"]) == key)?;
    let subs = subnets(c);
    let end = |e: &Value| -> Option<(String, Value)> {
        let sub = subs.iter().find(|x| s(&x["id"]) == s(&e["subnet"]))?;
        let p = arr(&sub["ports"])
            .iter()
            .find(|p| s(&p["id"]) == s(&e["port"]))?;
        Some((s(&sub["id"]).to_string(), p.clone()))
    };
    let (a, b) = (end(&l["from"])?, end(&l["to"])?);
    let tid = |x: &(String, Value)| flat_of(flat, &format!("{}/{}", x.0, s(&x.1["transition"])));
    let pid = |x: &(String, Value)| flat_of(flat, &format!("{}/{}", x.0, s(&x.1["place"])));
    let mut m = Merge {
        t,
        kind: kind.clone(),
        targets: vec![],
        guard: None,
    };
    match kind.as_str() {
        "event" => m.targets.extend(tid(&a)),
        "token" | "data" => m.targets.extend(pid(&a)),
        "guard" => {
            // from a transition port to a place port: a read arc
            let (tr, pl) = if s(&a.1["target"]) == "transition" {
                (&a, &b)
            } else {
                (&b, &a)
            };
            m.guard = Some((pid(pl)?, tid(tr)?));
        }
        _ => {}
    }
    Some(m)
}

fn draw_merge(c: &mut Ctx, net: &Net, m: &Merge) {
    let xf = net.xf;
    let t = m.t;
    if let Some((p, tr)) = &m.guard {
        if let (Some(a), Some(b)) = (net.lay.node(p), net.lay.node(tr)) {
            let (pa, pb) = (xf.pt(a.x, a.y), xf.pt(b.x, b.y));
            let f = (t / 0.8).min(1.0);
            let end = pa + (pb - pa) * f;
            let al = if t > 0.8 { 1.0 - (t - 0.8) / 0.2 } else { 1.0 };
            c.line(pa, end, READ.gamma_multiply(0.8 * al), sw(&xf, 3.0));
        }
        return;
    }
    let col = kind_color(if m.kind == "event" { "event" } else { &m.kind });
    for id in &m.targets {
        let Some(n) = net.lay.node(id) else { continue };
        if !n.fused {
            continue;
        }
        for mem in &n.members {
            let Some(h) = net.lay.home.get(mem) else {
                continue;
            };
            // hold at home, slide together, hand over to the fused element
            let s_t = ((t - 0.12) / 0.6).clamp(0.0, 1.0);
            let k = ease(s_t);
            let x = h.0 + (n.x - h.0) * k;
            let y = h.1 + (n.y - h.1) * k;
            let al = if t > 0.72 {
                1.0 - (t - 0.72) / 0.28
            } else {
                1.0
            };
            let (pre, loc) = mem.split_once('/').unwrap_or(("", mem));
            let at = xf.pt(x, y);
            if n.place {
                c.p.circle(
                    at,
                    xf.len(lay::PR),
                    Color32::WHITE.gamma_multiply(al),
                    Stroke::new(sw(&xf, 1.8), col.gamma_multiply(al)),
                );
                c.mono(
                    at + Vec2::new(0.0, xf.len(lay::PR + 12.0)),
                    Align2::CENTER_CENTER,
                    &format!("{pre}/{loc}"),
                    9.5 * xf.k,
                    INK.gamma_multiply(al),
                );
            } else {
                let w = lay::mw(loc, 9.5) + 18.0;
                let r = Rect::from_center_size(at, Vec2::new(w, 28.0) * xf.k);
                c.p.rect(
                    r,
                    xf.len(6.0),
                    Color32::WHITE.gamma_multiply(al),
                    Stroke::new(sw(&xf, 1.8), col.gamma_multiply(al)),
                    StrokeKind::Middle,
                );
                c.mono(
                    at,
                    Align2::CENTER_CENTER,
                    loc,
                    9.5 * xf.k,
                    INK.gamma_multiply(al),
                );
            }
        }
        // the fused element rings in as the members arrive
        if t > 0.62 {
            let f = ((t - 0.62) / 0.38).clamp(0.0, 1.0);
            let at = xf.pt(n.x, n.y);
            let rad = if n.place {
                xf.len(n.r)
            } else {
                xf.len(n.w.max(n.h) / 2.0)
            };
            c.p.circle_stroke(
                at,
                rad * (1.0 + 0.5 * f),
                Stroke::new(sw(&xf, 2.0), col.gamma_multiply(1.0 - f)),
            );
        }
    }
}

// ------------------------------------------------------------ the control bar

fn chip(c: &mut Ctx, x: &mut f32, y: f32, label: &str, on: bool, enabled: bool) -> Rect {
    let w = c.text_w(label, 12.0, false) + 18.0;
    let r = Rect::from_min_size(Pos2::new(*x, y), Vec2::new(w, 22.0));
    let (fill, fg) = if on {
        (INK, Color32::WHITE)
    } else if enabled {
        (Color32::WHITE, INK)
    } else {
        (hex("#f1f3f6"), MUTED.gamma_multiply(0.7))
    };
    c.p.rect(
        r,
        6.0,
        fill,
        Stroke::new(1.0, hex("#d3d9e0")),
        StrokeKind::Middle,
    );
    c.text(r.center(), Align2::CENTER_CENTER, label, 12.0, fg);
    if enabled {
        c.hand_if(r);
    }
    *x += w + 6.0;
    r
}

fn controls(c: &mut Ctx, area: Rect, flat: &Value, game: &Value) -> f32 {
    let y = area.top();
    let mut x = area.left();
    let sc = &game["scenario"];
    let active = s(&sc["id"]).to_string();
    let done = sc["done"] == true;
    let r = c.text(
        Pos2::new(x, y + 11.0),
        Align2::LEFT_CENTER,
        "Token game:",
        12.0,
        MUTED,
    );
    x = r.right() + 8.0;
    let r = chip(c, &mut x, y, "↺ Reset marking", false, true);
    c.hover(
        r,
        info("Reset marking")
            .fact("command", "bundle.reset_marking")
            .note("Back to the flat net's initial marking: nothing fired, no scenario selected.")
            .cite(c_initial()),
    );
    if c.click_in(r) {
        c.command("bundle.reset_marking", json!({}));
    }
    let can_step = !active.is_empty() && !done;
    let r = chip(c, &mut x, y, "Step ▶", false, can_step);
    c.hover(
        r,
        info("Step the selected scenario")
            .fact("command", "bundle.step")
            .note("Fires the scenario's next member, resolved to the flat transition that contains it, if it is enabled; the first step that is not enabled stops the script."),
    );
    if can_step && c.click_in(r) {
        c.command("bundle.step", json!({}));
    }
    let r = chip(c, &mut x, y, "Run all ▶▶", false, can_step);
    c.hover(
        r,
        info("Run the rest of the scenario").fact("command", "bundle.step {all: true}"),
    );
    if can_step && c.click_in(r) {
        c.command("bundle.step", json!({"all": true}));
    }
    x += 8.0;
    let r = c.text(
        Pos2::new(x, y + 11.0),
        Align2::LEFT_CENTER,
        "Scenarios (ROADMAP 4.4):",
        12.0,
        MUTED,
    );
    x = r.right() + 8.0;
    for run in c.d.runs.clone() {
        let id = s(&run["id"]).to_string();
        let r = chip(c, &mut x, y, &format!("▶ {id}"), id == active, true);
        if c.click_in(r) {
            c.command("bundle.scenario", json!({"id": id}));
        }
        let pass = arr(&run["expect"]).iter().all(|e| e["pass"] == true);
        let stopped = s(&run["stopped_step"]).to_string();
        let mut i = info(format!("scenario {id}: {}", s(&run["title"])));
        for st in arr(&run["steps"]) {
            i = i.fact(
                s(&st["member"]),
                if st["enabled"] == true {
                    s(&st["flat"]).to_string()
                } else {
                    format!("refused: {}", s(&st["why"]))
                },
            );
        }
        for e in arr(&run["expect"]) {
            i = i.fact(
                &format!("{} == {}", s(&e["place"]), e["n"]),
                format!(
                    "{} ({})",
                    e["actual"],
                    if e["pass"] == true { "PASS" } else { "FAIL" }
                ),
            );
        }
        i = i
            .fact("click", "select it: the token game goes back to the initial marking, then Step fires it")
            .note("The whole script run from the initial marking through pflow-metamodel's enabled/fire on the flat net (bundle.run). Judgment, frozen: the scripts and their predicates.");
        c.hover(r, i);
        c.scene.emit(
            "scenario",
            &id,
            json!({"pass": pass, "stopped_step": stopped}),
        );
    }
    // status, right-aligned
    let n_en = arr(&game["enabled"]).len();
    let n_all = arr(&flat["transitions"]).len();
    let mut st = format!(
        "{n_en} of {n_all} enabled · fired {}",
        arr(&game["fired"]).len()
    );
    if done {
        let exp = arr(&sc["expect"]);
        let f = exp.iter().filter(|e| e["pass"] != true).count();
        st.push_str(&format!(" · {active}: {} PASS", exp.len() - f));
        if f > 0 {
            st.push_str(&format!(" {f} FAIL"));
        }
        if sc["completed"] != true {
            st.push_str(&format!(
                " STOPPED AT {}",
                sc["stopped_at"].as_i64().unwrap_or(0) + 1
            ));
        }
    }
    c.text(
        Pos2::new(area.right(), y + 11.0),
        Align2::RIGHT_CENTER,
        &st,
        12.0,
        MUTED,
    );
    24.0
}

// ------------------------------------------------------------ the cards

fn card_frame(c: &Ctx, r: Rect) {
    c.p.rect(
        r,
        8.0,
        Color32::WHITE,
        Stroke::new(1.0, hex("#dde1e6")),
        StrokeKind::Middle,
    );
}

/// Whether the links are the page default (event links compared either way
/// round) and no second instance is added.
fn is_default_composition(c: &Ctx) -> bool {
    let key = |kind: &str, from: &Value, to: &Value| -> String {
        let a = format!("{}|{}", s(&from["subnet"]), s(&from["port"]));
        let b = format!("{}|{}", s(&to["subnet"]), s(&to["port"]));
        if kind == "event" {
            let mut v = [a, b];
            v.sort();
            format!("event:{}~{}", v[0], v[1])
        } else {
            format!("{kind}:{a}->{b}")
        }
    };
    let Some(st) = &c.d.state else { return false };
    let mut now: Vec<String> = arr(&st["links"])
        .iter()
        .map(|l| key(s(&l["kind"]), &l["from"], &l["to"]))
        .collect();
    let mut def: Vec<String> = comp::default_links()
        .iter()
        .map(|l| {
            let j = comp::link_json(l);
            key(s(&j["kind"]), &j["from"], &j["to"])
        })
        .collect();
    now.sort();
    def.sort();
    now == def && !arr(&st["subnets"]).iter().any(|x| x["extra"] == true)
}

/// Why "the worker returns to the pool" failed, read off this flat net and
/// this run: which flat transition holds workers/release, what it is fused
/// with, and whether the run fired it. The idiom's caveat is quoted only when
/// its situation is the one on screen.
fn leak_why(flat: &Value, game: &Value) -> Option<(String, bool, String, String)> {
    let sc = &game["scenario"];
    let acq = flat_of(flat, "workers/acquire")?;
    let rel = flat_of(flat, "workers/release")?;
    let fired: BTreeSet<&str> = arr(&game["fired"]).iter().map(s).collect();
    if !fired.contains(acq.as_str()) || fired.contains(rel.as_str()) || sc["completed"] != true {
        return None;
    }
    let partners: Vec<String> = strs(&flat["fused_groups"][&rel])
        .into_iter()
        .filter(|m| m != "workers/release")
        .collect();
    let rolled = flat_of(flat, "payment/rolled_back")
        .map(|p| marking_of(game, &p) > 0)
        .unwrap_or(false);
    let idiom_case = partners.len() == 1 && partners[0] == "payment/commit" && rolled;
    let mut t = String::from("Why the worker does not return. ");
    if acq == "workers/acquire" {
        t.push_str("workers/acquire fired, ");
    } else {
        t.push_str(&format!("workers/acquire fired as part of {acq}, "));
    }
    if !partners.is_empty() {
        t.push_str(&format!(
            "but workers/release fires only as part of {rel} (fused with {} by event links), and this run never fires it",
            partners.join(", ")
        ));
    } else {
        t.push_str("but workers/release is fused with nothing (no event link reaches workers:release here), and the script never fires it");
    }
    t.push_str(", so the token stays in workers/in_use: the pool leaks a unit.");
    if idiom_case {
        t.push_str(" That is the idiom's own caveat: \"Fires only on commit. If the saga rolls back, no port fires, the worker stays in in_use and the pool leaks a unit.\" (patterns.ttl:496). ROADMAP Phase 4 expects exactly this failure (ROADMAP.md:276-280, 289-291).");
    }
    Some((t, idiom_case, acq, rel))
}

fn scenario_card(c: &mut Ctx, r: Rect, flat: &Value, game: &Value) {
    card_frame(c, r);
    let saved = c.p.clone();
    c.p = saved.with_clip_rect(r.shrink(1.0));
    let pad = 10.0;
    let w = r.width() - 2.0 * pad;
    let mut y = r.top() + 8.0;
    let x = r.left() + pad;
    let sc = &game["scenario"];
    if sc.is_null() {
        c.text(
            Pos2::new(x, y),
            Align2::LEFT_TOP,
            "Scenario checks",
            12.5,
            INK,
        );
        y += 20.0;
        y += c.wrapped(Pos2::new(x, y), "A scenario starts from the initial marking, fires its script one step at a time (each member transition resolved to the flat transition that contains it), stops at the first step that is not enabled, then checks its expected marking predicates.", 11.0, INK, w, false) + 4.0;
        c.wrapped(Pos2::new(x, y), "The scripts and predicates are judgment, written from the description and then frozen (ROADMAP 4.4). Whether a step is enabled, and what the marking becomes, is go-pflow's firing rule, run by pflow-metamodel.", 11.0, MUTED, w, false);
        c.p = saved;
        return;
    }
    c.text(
        Pos2::new(x, y),
        Align2::LEFT_TOP,
        &format!("{}: {}", s(&sc["id"]), s(&sc["title"])),
        12.5,
        INK,
    );
    y += 20.0;
    // steps on the left, the verdict on the right when the card is wide
    let wide = w > 620.0;
    let (sw_, rx) = if wide {
        (w * 0.46, x + w * 0.5)
    } else {
        (w, x)
    };
    let right_top = y;
    for (i, st) in arr(&sc["steps"]).iter().enumerate() {
        let ok = st["enabled"] == true;
        let (lab, fg, bg) = if ok {
            ("FIRED", EN, hex("#e3f3e8"))
        } else {
            ("STOP", hex("#9a5d0a"), hex("#fdf0dc"))
        };
        let nr = c.text(
            Pos2::new(x, y),
            Align2::LEFT_TOP,
            &format!("{}.", i + 1),
            11.0,
            MUTED,
        );
        let b = c.badge(Pos2::new(nr.right() + 4.0, y - 1.0), lab, fg, bg, 10.0);
        let member = s(&st["member"]);
        let flat_id = s(&st["flat"]);
        let line = if !flat_id.is_empty() && flat_id != member {
            format!("{member} → {flat_id}")
        } else {
            member.to_string()
        };
        let h = c.wrapped(
            Pos2::new(b.right() + 6.0, y),
            &line,
            10.0,
            INK,
            sw_ - (b.right() - x) - 6.0,
            true,
        );
        y += h.max(14.0) + 2.0;
        if !ok {
            let h = c.wrapped(
                Pos2::new(x + 14.0, y),
                &format!("not enabled: {}", s(&st["why"])),
                10.0,
                hex("#9a5d0a"),
                sw_ - 14.0,
                false,
            );
            y += h + 2.0;
        }
    }
    let left = (x, sw_, y);
    if wide {
        y = right_top;
    }
    let (x, w) = (rx, if wide { w * 0.5 } else { w });
    if sc["done"] == true {
        let n_steps = arr(&sc["script"]).len();
        let msg = if sc["completed"] == true {
            format!("All {n_steps} steps fired.")
        } else {
            format!(
                "Stopped at step {} of {n_steps}.",
                sc["stopped_at"].as_i64().unwrap_or(0) + 1
            )
        };
        c.text(Pos2::new(x, y), Align2::LEFT_TOP, &msg, 11.0, MUTED);
        y += 16.0;
        for e in arr(&sc["expect"]) {
            let pass = e["pass"] == true;
            let (lab, fg, bg) = if pass {
                ("PASS", EN, hex("#e3f3e8"))
            } else {
                ("FAIL", hex("#b2342a"), hex("#fbe4e1"))
            };
            let b = c.badge(Pos2::new(x, y - 1.0), lab, fg, bg, 10.0);
            let txt = format!(
                "{}  ({} {} {}; is {})",
                s(&e["says"]),
                if s(&e["flat"]).is_empty() {
                    s(&e["place"])
                } else {
                    s(&e["flat"])
                },
                s(&e["op"]),
                e["n"],
                e["actual"]
            );
            let h = c.wrapped(
                Pos2::new(b.right() + 6.0, y),
                &txt,
                10.5,
                INK,
                w - (b.right() - x) - 6.0,
                false,
            );
            let row = Rect::from_min_size(Pos2::new(x, y - 1.0), Vec2::new(w, h.max(14.0) + 2.0));
            c.hover(
                row,
                info(format!("expected: {}", s(&e["says"])))
                    .fact("predicate", format!("{} {} {}", s(&e["place"]), s(&e["op"]), e["n"]))
                    .fact("flat place", if s(&e["flat"]).is_empty() { "(not in the flat net)" } else { s(&e["flat"]) })
                    .fact("at the end of the run", e["actual"].to_string())
                    .fact("verdict", if pass { "PASS" } else { "FAIL" })
                    .note("A scenario predicate: judgment, written from the description and frozen (the prototype's js/model/orders.js). ROADMAP 4.3 proposes recording such checks as ns:FaithfulnessCheck receipts; that class is not in the ontology yet.")
                    .cite(c_faith())
                    .cite(c_d44()),
            );
            y += h.max(14.0) + 3.0;
        }
        let leak_fail = arr(&sc["expect"])
            .iter()
            .any(|e| e["pass"] != true && s(&e["place"]) == "workers/available");
        if leak_fail {
            if let Some((t, idiom_case, acq, rel)) = leak_why(flat, game) {
                let h = c.wrapped_h(&t, 10.5, w - 12.0, false);
                let rr = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h + 10.0));
                c.p.rect_filled(rr, 4.0, hex("#fbf4e6"));
                c.wrapped(
                    rr.left_top() + Vec2::new(6.0, 5.0),
                    &t,
                    10.5,
                    INK,
                    w - 12.0,
                    false,
                );
                let i = if idiom_case {
                    info("the pool-leak caveat")
                        .fact(
                            "concept",
                            "ns:CompositionIdiom (pat:idiom-pool-release-saga-commit)",
                        )
                        .fact("workers/release is in", rel)
                        .fact("fired in this run", "no")
                        .note(LEAK_WORD)
                        .cite(c_leak())
                        .cite(c_worked())
                        .cite(c_d44())
                } else {
                    info("why the worker does not return")
                        .fact("workers/acquire is in", acq)
                        .fact("workers/release is in", rel)
                        .fact("fired in this run", "acquire yes, release no")
                        .note("Read off this flat net: the flat transition that holds workers/release never fired in this run. The idiom caveat (patterns.ttl:496) is quoted only when workers/release is fused with payment/commit alone and the saga rolled back; that is not the situation in this run.")
                        .cite(c_why_not())
                        .cite(c_fire())
                };
                c.hover(rr, i);
                y += h + 14.0;
            }
        }
        let note = s(&sc["note"]);
        if !note.is_empty() {
            let t = if is_default_composition(c) {
                format!("Scenario note (orders.js): {note}")
            } else {
                format!("Scenario note (orders.js, written for the three-idiom composition; this composition differs, so read the run above instead): {note}")
            };
            if wide {
                c.wrapped(
                    Pos2::new(left.0, left.2 + 6.0),
                    &t,
                    10.0,
                    MUTED,
                    left.1,
                    false,
                );
            } else {
                c.wrapped(Pos2::new(x, y), &t, 10.0, MUTED, w, false);
            }
        }
    } else {
        c.text(
            Pos2::new(x, y),
            Align2::LEFT_TOP,
            "press Step ▶ to fire the next member",
            11.0,
            MUTED,
        );
    }
    c.p = saved;
}

fn constraints_card(c: &mut Ctx, r: Rect, flat: &Value, game: &Value) {
    card_frame(c, r);
    let saved = c.p.clone();
    c.p = saved.with_clip_rect(r.shrink(1.0));
    let (x, w) = (r.left() + 10.0, r.width() - 20.0);
    let mut y = r.top() + 8.0;
    c.text(
        Pos2::new(x, y),
        Align2::LEFT_TOP,
        "Declared constraints, at this marking",
        12.5,
        INK,
    );
    y += 20.0;
    for k in arr(&game["constraints"]) {
        let ev = k["evaluated"] == true;
        let (lab, fg, bg) = if !ev {
            ("NOT EVALUATED", hex("#9a5d0a"), hex("#fdf0dc"))
        } else if k["holds"] == true {
            ("HOLDS", EN, hex("#e3f3e8"))
        } else {
            ("VIOLATED", hex("#b2342a"), hex("#fbe4e1"))
        };
        let b = c.badge(Pos2::new(x, y - 1.0), lab, fg, bg, 10.0);
        let id = s(&k["id"]);
        let ir = c.mono(
            Pos2::new(b.right() + 6.0, y),
            Align2::LEFT_TOP,
            id,
            10.5,
            INK,
        );
        let expr = if ev {
            format!("{} {} {}", s(&k["lhs"]), s(&k["op"]), k["rhs"])
        } else {
            s(&k["expr"]).to_string()
        };
        c.mono(
            Pos2::new(ir.right() + 8.0, y),
            Align2::LEFT_TOP,
            &expr,
            10.5,
            MUTED,
        );
        let row = Rect::from_min_max(Pos2::new(x, y - 1.0), Pos2::new(x + w, y + 15.0));
        let local = id.split_once('/').map_or(id, |x| x.1);
        let sub = id.split_once('/').map_or("", |x| x.0);
        let mut i = info(format!("invariant {id}"))
            .fact("flat expression", s(&k["expr"]))
            .fact(
                "at this marking",
                if ev {
                    format!(
                        "{} {} {}: {}",
                        s(&k["lhs"]),
                        s(&k["op"]),
                        k["rhs"],
                        if k["holds"] == true { "holds" } else { "violated" }
                    )
                } else {
                    "not evaluated (expression form not handled here)".into()
                },
            )
            .fact("status", "declared by the constructor, not proved here (sim_verify / go-pflow verify decide); the Incidence view checks y·C = 0 and y·M₀ = n")
            .fact("local id", local)
            .fact("subnet", sub);
        i = i.cite(c_constraints());
        c.hover(row, i);
        y += 17.0;
    }
    // the leak, and why conservation does not catch it
    let in_use = flat_of(flat, "workers/in_use");
    let rolled = flat_of(flat, "payment/rolled_back");
    let leaking = match (in_use, rolled) {
        (Some(a), Some(b)) => marking_of(game, &a) > 0 && marking_of(game, &b) > 0,
        _ => false,
    };
    let pool = arr(&game["constraints"])
        .iter()
        .find(|k| s(&k["id"]) == "workers/pool_conservation");
    if let (true, Some(pool)) = (leaking, pool) {
        let v = if pool["evaluated"] == true {
            format!("{} == {}", s(&pool["lhs"]), pool["rhs"])
        } else {
            s(&pool["expr"]).to_string()
        };
        let t = format!("Conservation is not liveness. The saga has rolled back with a worker still in workers/in_use, and pool_conservation still holds ({v}): a stuck token keeps the sum. That is the gap ROADMAP Phase 4's faithfulness checks are for: Validate and verify say what a net satisfies, not whether it is the system described, so frozen scenarios are run as well (ROADMAP.md:247-295). Its 4.1 exit expects the leak to show as \"a failed conservation check on the rollback path\" (lines 240-242); on this net the declared conservation constraint holds, and what fails is the scenario predicate.");
        let h = c.wrapped_h(&t, 10.0, w - 12.0, false);
        let rr = Rect::from_min_size(Pos2::new(x, y + 2.0), Vec2::new(w, h + 10.0));
        c.p.rect_filled(rr, 4.0, hex("#fbf4e6"));
        c.wrapped(
            rr.left_top() + Vec2::new(6.0, 5.0),
            &t,
            10.0,
            INK,
            w - 12.0,
            false,
        );
        c.hover(
            rr,
            info("conservation is not liveness")
                .fact(
                    "concept",
                    "ns:PatternInvariant (pat:resource-pool-inv-conservation)",
                )
                .fact(
                    "tokens(\"available\") + tokens(\"in_use\") == capacity",
                    format!("{v}: holds"),
                )
                .fact(
                    "what fails instead",
                    "the scenario predicate \"the worker returns to the pool\"",
                )
                .cite(c_pool_inv())
                .cite(c_faith())
                .cite(c_exit41())
                .cite(c_worked()),
        );
        y += h + 16.0;
    }
    c.wrapped(Pos2::new(x, y + 2.0), "The constructors declare these; Flatten carries them over with qualified IDs and rewritten place references. Holding at one marking is not a proof (go-pflow verify or sim_verify decide that).", 10.0, MUTED, w, false);
    c.p = saved;
}

// ------------------------------------------------------------ the view

pub(super) fn flat_orders(c: &mut Ctx, area: Rect) {
    let Some(flat) = c.d.flat.clone() else { return };
    if flat["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refuses this composition, so there is no flat net to draw: {}",
                s(&flat["error"])
            ),
        );
        return;
    }
    let game = c.d.game.clone().unwrap_or(Value::Null);
    let subs = subnets(c);
    let layout = lay::layout(&subs, &flat);
    let bar_h = controls(c, area, &flat, &game) + 6.0;
    let cards_h = (area.height() * 0.29).clamp(150.0, 250.0);
    let net_area = Rect::from_min_max(
        area.left_top() + Vec2::new(0.0, bar_h),
        Pos2::new(area.right(), area.bottom() - cards_h - 8.0),
    );
    let mut xf = Xf::fit(net_area, layout.w, layout.h, 1.4);
    xf.origin.x += ((net_area.width() - layout.w * xf.k) / 2.0).max(0.0);
    // the pointer's node, before anything is drawn (an arc lights with its ends)
    let hot = c.pointer_pos().and_then(|p| {
        layout.nodes.iter().position(|n| {
            node_rect(&xf, n)
                .expand(if n.place { 0.0 } else { 2.0 })
                .contains(p)
        })
    });
    let blocked = {
        let sc = &game["scenario"];
        if sc["done"] == true && sc["completed"] != true {
            arr(&sc["steps"])
                .last()
                .and_then(|st| st["flat"].as_str().map(str::to_string))
        } else {
            None
        }
    };
    let net = Net {
        xf,
        lay: &layout,
        flat: &flat,
        game: &game,
        hot,
        blocked,
        sources: arc_sources(c, &flat),
    };
    // the merge animation: the fused element appears only as its members arrive
    let merge = merge_of(c, &flat);
    let mut appear: BTreeMap<String, f32> = BTreeMap::new();
    let mut fade: BTreeMap<String, f32> = BTreeMap::new();
    if let Some(m) = &merge {
        for id in &m.targets {
            let a = ((m.t - 0.6) / 0.3).clamp(0.0, 1.0);
            appear.insert(id.clone(), if a > 0.0 { 1.0 } else { 0.0 });
            fade.insert(id.clone(), a);
        }
    }
    draw_clusters(c, &net, &subs);
    draw_arcs(c, &net, &fade);
    draw_nodes(c, &net, &appear);
    draw_legend(c, &xf, &layout.legend);
    if let Some(m) = &merge {
        draw_merge(c, &net, m);
    }

    // cards
    let cards = Rect::from_min_max(
        Pos2::new(area.left(), area.bottom() - cards_h),
        area.right_bottom(),
    );
    let w1 = cards.width() * 0.58;
    scenario_card(
        c,
        Rect::from_min_size(cards.left_top(), Vec2::new(w1 - 6.0, cards.height())),
        &flat,
        &game,
    );
    constraints_card(
        c,
        Rect::from_min_max(
            cards.left_top() + Vec2::new(w1 + 6.0, 0.0),
            cards.right_bottom(),
        ),
        &flat,
        &game,
    );

    c.note("na", "No ports are drawn here: Flatten removes the boundary, and a flat net has only places, transitions and arcs. Links are made in the pattern-level views; this view shows the one net they produce, under go-pflow's flat IDs.");
    c.note("judgment", "The scenarios and their expected predicates are judgment, written from the description and frozen (ROADMAP Phase 4). Whether a step is enabled, and the marking it leaves, is go-pflow's firing rule as pflow-metamodel runs it.");
    c.note("honesty", "The flat IDs, arcs, weights, initial markings and constraints are Flatten's (pflow-compose). The positions and the clusters are this page's layout: RenderFlatDOT draws no clusters, and in the flat net an element belongs to a subnet only through its ID. A read arc (a guard link, lowered) is dashed and undirected, as RenderFlatDOT draws it; the small dot that marks its transition end is this page's. Constraints are evaluated at the current marking only: holding at one marking proves no invariant, and an invariant that holds is not liveness.");
}

pub(super) fn flat_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, ar) = net_counts(c);
    let r = Rect::from_min_size(
        area.left_top(),
        Vec2::new(area.width(), area.height() - 30.0),
    );
    draw_c4_net(c, r);
    c.hover(
        r,
        info("connect-4")
            .fact("places", pl.to_string())
            .fact("transitions", tr.to_string())
            .fact("arcs", ar.to_string()),
    );
    c.scene.emit(
        "net",
        "connect-4",
        json!({"places": pl, "transitions": tr, "arcs": ar}),
    );
    c.note("na", "A search plan is not a net, so Flatten has nothing to flatten and no plan slot is drawn here. The net the plan searches (pflow-rs's fixture connect-4.json) is drawn at its own coordinates.");
}

// ------------------------------------------------------------ 6 boundaries

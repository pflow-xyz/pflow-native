//! The operad view (see `docs/composition-views-scene.md`).

use super::*;

pub(super) fn operad_orders(c: &mut Ctx, area: Rect) {
    let outer = Rect::from_min_size(area.left_top(), Vec2::new(area.width(), 330.0));
    c.p.rect(
        outer,
        12.0,
        Color32::from_rgb(0xf7, 0xf8, 0xfa),
        Stroke::new(1.6, INK),
        StrokeKind::Middle,
    );
    c.text(
        outer.left_top() + Vec2::new(10.0, 8.0),
        Align2::LEFT_TOP,
        "the bundle (outer box)",
        12.0,
        MUTED,
    );
    let inner = outer.shrink2(Vec2::new(60.0, 50.0));
    let laid = draw_boxes(c, inner, 200.0, true);
    let ls = links(c);
    let mut used: BTreeSet<String> = BTreeSet::new();
    for l in &ls {
        used.insert(format!(
            "{}:{}",
            s(&l["from"]["subnet"]),
            s(&l["from"]["port"])
        ));
        used.insert(format!("{}:{}", s(&l["to"]["subnet"]), s(&l["to"]["port"])));
        let (Some(a), Some(b)) = (
            anchor(&laid, &l["from"], outer.center()),
            anchor(&laid, &l["to"], outer.center()),
        ) else {
            continue;
        };
        let j = Pos2::new((a.x + b.x) / 2.0, outer.bottom() - 22.0);
        let col = kind_color(s(&l["kind"]));
        c.line(a, j, col, 1.6);
        c.line(b, j, col, 1.6);
        c.p.circle_filled(j, 5.0, col);
        c.hover(Rect::from_center_size(j, Vec2::splat(12.0)), link_info(l).note("A junction: an undirected cable soldered onto both ports (Spivak's wiring diagrams are cospans)."));
        c.scene
            .emit("junction", s(&l["key"]), json!({"kind": s(&l["kind"])}));
    }
    for l in &laid {
        c.scene.emit("hole", &l.id, json!({}));
        for (p, anchors) in &l.ports {
            let key = format!("{}:{}", l.id, s(&p["id"]));
            if !used.contains(&key) {
                if let Some(a) = anchors.first() {
                    let ex = Pos2::new(a.x, outer.top());
                    c.dashed(*a, ex, Color32::from_gray(200), 0.8);
                    c.p.circle_stroke(ex, 3.0, Stroke::new(1.0, MUTED));
                }
                c.scene.emit("exposed", &key, json!({}));
            }
        }
    }
    if ls.iter().any(|l| s(&l["kind"]) == "guard") {
        c.note("na", "Guard links are not cables: a guard restricts one subnet on another's place (W_RESTRICTIVE_LINK) and sits outside the free-SMC fragment (ROADMAP Phase 6). It is drawn and flagged; no operadic composite is claimed.");
    }
    c.note("judgment", "Exposed ports: the outer box's own ports are every port no link uses, drawn faintly on its boundary. That choice of interface is this view's reading; a go-pflow Bundle declares subnets, links and constraints but no ports of its own.");
    c.note("honesty", "Spivak's operad of wiring diagrams (arXiv:1305.0297, Ex. 2.1.7) is undirected: a wiring diagram is a cospan, every port is soldered onto a cable, and filling a hole is a pushout.");
}

pub(super) fn operad_c4(c: &mut Ctx, area: Rect) {
    let (plan, order, drivers, filters, heur) = plan_lists(c);
    let names: Vec<String> = drivers.iter().map(driver_name).collect();
    // frames: budget outermost, then null_window (game.rs's nesting)
    let mut frame = Rect::from_min_size(
        area.left_top(),
        Vec2::new(area.width(), area.height() - 60.0),
    );
    let mut nest: Vec<String> = vec![];
    for d in ["budget", "null_window"] {
        if names.iter().any(|n| n == d) {
            nest.push(d.into());
        }
    }
    for n in names
        .iter()
        .filter(|n| *n != "budget" && *n != "null_window")
    {
        nest.push(n.clone());
    }
    for d in &nest {
        c.p.rect(
            frame,
            10.0,
            Color32::TRANSPARENT,
            Stroke::new(1.5, Color32::from_rgb(0x17, 0x8f, 0x7f)),
            StrokeKind::Middle,
        );
        c.text(
            frame.left_top() + Vec2::new(8.0, 4.0),
            Align2::LEFT_TOP,
            &format!("driver {d}"),
            11.5,
            Color32::from_rgb(0x17, 0x8f, 0x7f),
        );
        c.hover(Rect::from_min_size(frame.left_top(), Vec2::new(frame.width(), 18.0)), info(format!("driver {d}")).note("Drivers wrap the core (plan.rs reads them into flags; their JSON order does not change the nesting)."));
        c.scene.emit("frame", d, json!({}));
        frame = frame.shrink(22.0);
    }
    let core = frame;
    c.boxr(core, Color32::from_rgb(0xf7, 0xf8, 0xfa), INK, 1.4);
    c.text(
        core.left_top() + Vec2::new(8.0, 4.0),
        Align2::LEFT_TOP,
        "core: typed holes",
        12.0,
        INK,
    );
    let slots = sp::SLOTS;
    let w = (core.width() - 30.0) / 4.0;
    for (i, sl) in slots.iter().enumerate() {
        let (row, col) = (i / 4, i % 4);
        let r = Rect::from_min_size(
            core.left_top()
                + Vec2::new(
                    10.0 + col as f32 * (w + 4.0),
                    28.0 + row as f32 * ((core.height() - 40.0) / 2.0),
                ),
            Vec2::new(w - 4.0, (core.height() - 50.0) / 2.0),
        );
        c.slot(
            r,
            sl.id,
            &format!(
                "{} ({}){}",
                sl.label,
                sl.accepts,
                if sl.designed { " · designed" } else { "" }
            ),
        );
        if sl.id != "drivers" {
            // drivers are frames (above), not a core hole; the drop
            // target stays
            c.scene.emit("hole", sl.id, json!({"accepts": sl.accepts}));
        }
        let entries: Vec<String> = match sl.id {
            "order" => order.clone(),
            "filters" => filters.clone(),
            "heuristics" => heur.clone(),
            "memo" if plan.get("memo").is_some_and(|m| !m.is_null()) => vec!["memo".into()],
            _ => vec![],
        };
        for (j, e) in entries.iter().enumerate() {
            let ch = c.chip(
                r.left_top() + Vec2::new(8.0, 22.0 + j as f32 * 22.0),
                e,
                false,
            );
            if c.clicked && c.pointer.is_some_and(|p| ch.contains(p)) && c.drag.is_none() {
                c.command("plan.remove_piece", json!({"slot": sl.id, "index": j}));
            }
            c.scene.emit("piece", &format!("{}/{e}", sl.id), json!({}));
        }
    }
    palette(c, area);
    plan_warn(c);
    c.note("honesty", "Holes are typed by piece kind, not by wires: a piece has no wires of its own, so this is read as an operad in the sense of Fong-Spivak (Rough Def. 6.91: types, operations, substitution), not as Spivak's cospan operad of wiring diagrams.");
}

// ------------------------------------------------------------ 4 string

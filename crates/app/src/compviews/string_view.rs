//! The string view (see `docs/composition-views-scene.md`).

use super::*;

pub(super) fn string_orders(c: &mut Ctx, area: Rect) {
    // instances stacked (⊗), wires leave each box on the right
    let subs = subnets(c);
    let n = subs.len().max(1) as f32;
    let h = ((area.height() - 40.0) / n - 14.0).clamp(60.0, 140.0);
    let bx = area.left() + 200.0;
    let mut wire_end: BTreeMap<String, Pos2> = BTreeMap::new();
    for (i, sub) in subs.iter().enumerate() {
        let id = s(&sub["id"]);
        let r = Rect::from_min_size(
            Pos2::new(bx, area.top() + i as f32 * (h + 14.0)),
            Vec2::new(170.0, h),
        );
        c.boxr(r, pattern_tint(s(&sub["pattern"])), INK, 1.3);
        c.text(r.center(), Align2::CENTER_CENTER, id, 13.0, INK);
        c.hover(r, subnet_info(sub));
        c.scene.emit("box", id, json!({}));
        let ports = arr(&sub["ports"]);
        for (j, p) in ports.iter().enumerate() {
            let y = r.top() + (j as f32 + 1.0) * h / (ports.len() as f32 + 1.0);
            let a = Pos2::new(r.right(), y);
            let b = Pos2::new(r.right() + 140.0, y);
            let sch = s(&p["schema"]);
            let col = if sch.is_empty() {
                TRANS
            } else {
                schema_color(sch)
            };
            c.line(a, b, col, 1.6);
            let key = format!("{id}:{}", s(&p["id"]));
            c.mono(
                b + Vec2::new(4.0, 0.0),
                Align2::LEFT_CENTER,
                &format!(
                    "{} {}",
                    s(&p["id"]),
                    if sch.is_empty() {
                        "(transition)".into()
                    } else {
                        format!(": {sch}")
                    }
                ),
                10.0,
                MUTED,
            );
            c.port(a, id, p, false, true);
            if s(&p["target"]) == "place" {
                c.scene.emit("wire", &key, json!({"schema": sch}));
            }
            wire_end.insert(key, b);
        }
    }
    let ls = links(c);
    for (i, l) in ls.iter().enumerate() {
        let k = s(&l["kind"]);
        let fa = wire_end.get(&format!(
            "{}:{}",
            s(&l["from"]["subnet"]),
            s(&l["from"]["port"])
        ));
        let ta = wire_end.get(&format!(
            "{}:{}",
            s(&l["to"]["subnet"]),
            s(&l["to"]["port"])
        ));
        if let (Some(a), Some(b)) = (fa, ta) {
            let x = area.left() + 620.0 + i as f32 * 26.0;
            let col = kind_color(k);
            c.line(*a + Vec2::new(150.0, 0.0), Pos2::new(x, a.y), col, 1.4);
            c.line(*b + Vec2::new(150.0, 0.0), Pos2::new(x, b.y), col, 1.4);
            c.line(Pos2::new(x, a.y), Pos2::new(x, b.y), col, 1.4);
            let m = Pos2::new(x, (a.y + b.y) / 2.0);
            let r = c.text(m, Align2::CENTER_CENTER, kind_glyph(k), 15.0, col);
            c.hover(r.expand(3.0), link_info(l));
            c.scene.emit("fusion", s(&l["key"]), json!({}));
        }
    }
    c.note("honesty", "Pattern level: instances side by side (⊗, drawn stacked), schema-typed wires, fusion with RenderDOT's glyphs. The transition-level drawing (pflow-xyz diagram-viewer.js) is not ported here; the flattened net is in the Flat net view.");
}

pub(super) fn string_c4(c: &mut Ctx, area: Rect) {
    let (_, order, drivers, _, _) = plan_lists(c);
    let names: Vec<String> = drivers.iter().map(driver_name).collect();
    let mut frame = Rect::from_min_size(
        area.left_top() + Vec2::new(60.0, 10.0),
        Vec2::new(area.width() - 120.0, area.height() - 90.0),
    );
    for d in ["budget", "null_window"]
        .iter()
        .filter(|d| names.iter().any(|n| n == *d))
    {
        c.p.rect(
            frame,
            8.0,
            Color32::TRANSPARENT,
            Stroke::new(1.4, Color32::from_rgb(0x17, 0x8f, 0x7f)),
            StrokeKind::Middle,
        );
        c.text(
            frame.left_top() + Vec2::new(8.0, 4.0),
            Align2::LEFT_TOP,
            &format!("frame: {d}"),
            11.0,
            Color32::from_rgb(0x17, 0x8f, 0x7f),
        );
        c.scene.emit("frame", d, json!({}));
        frame = frame.shrink(26.0);
    }
    let core = Rect::from_center_size(frame.center(), Vec2::new(220.0, 70.0));
    c.boxr(core, Color32::WHITE, INK, 1.6);
    c.text(
        core.center(),
        Align2::CENTER_CENTER,
        "core: Position → Value⊥",
        13.0,
        INK,
    );
    c.scene.emit("box", "core", json!({}));
    c.line(
        Pos2::new(frame.left(), core.center().y),
        core.left_center(),
        INK,
        1.8,
    );
    c.line(
        core.right_center(),
        Pos2::new(frame.right(), core.center().y),
        INK,
        1.8,
    );
    // the order chain on the parameter wire
    let chain_r = Rect::from_min_size(
        Pos2::new(core.left(), frame.top() + 24.0),
        Vec2::new(core.width(), 24.0 * order.len().max(1) as f32 + 10.0),
    );
    c.slot(
        chain_r.expand2(Vec2::new(60.0, 4.0)),
        "order",
        "order chain",
    );
    for (i, p) in order.iter().enumerate() {
        c.chip(
            chain_r.left_top() + Vec2::new(10.0, 6.0 + i as f32 * 24.0),
            p,
            false,
        );
        c.scene.emit("chain", p, json!({"index": i}));
    }
    c.line(
        chain_r.center_bottom(),
        core.center_top(),
        Color32::from_rgb(0x2f, 0x6f, 0xb3),
        1.4,
    );
    let mut back = vec![
        ("recursion", "recursion: search calls itself on each child"),
        (
            "tt",
            "the transposition table: what a search found, read back",
        ),
        (
            "cutoff",
            "chain.cutoff: a cutoff reported back to the pieces",
        ),
    ];
    if names.iter().any(|n| n == "null_window") {
        back.insert(
            0,
            (
                "nullwindow",
                "null-window loop: yes/no searches, one per gap between rewards",
            ),
        );
    }
    for (i, (id, t)) in back.iter().enumerate() {
        let y = core.bottom() + 20.0 + i as f32 * 16.0;
        c.dashed(
            Pos2::new(core.right(), y),
            Pos2::new(core.left(), y),
            MUTED,
            1.0,
        );
        let r = c.text(
            Pos2::new(core.right() + 8.0, y),
            Align2::LEFT_CENTER,
            t,
            10.5,
            MUTED,
        );
        c.hover(
            r,
            info(format!("back edge: {id}"))
                .cite("pflow-rs crates/pflow-metamodel/src/game.rs (Solver::search)"),
        );
        c.scene.emit("backedge", id, json!({}));
    }
    palette(c, area);
    plan_warn(c);
    c.note("honesty", "The core is fixed; a plan chooses the order chain on its parameter wire and the drivers drawn as frames. Feedback (null-window loop, recursion, the transposition table, cutoffs) is drawn as dashed back edges: a string diagram of a recursive search is a sketch, not a term.");
}

// ------------------------------------------------------------ 5 flat

//! The spiders view (see `docs/composition-views-scene.md`).

use super::*;

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
    // member boxes on the left, one spider per class on the right
    let mut y = area.top() + 10.0;
    let x0 = area.left();
    let sx = area.left() + 330.0;
    let ls = links(c);
    let mut classes: Vec<(String, &str, Vec<String>)> = vec![];
    for (k, v) in flat["fused_groups"].as_object().into_iter().flatten() {
        classes.push((k.clone(), "event", strs(v)));
    }
    for (k, v) in flat["wires"].as_object().into_iter().flatten() {
        classes.push((k.clone(), "token", strs(v)));
    }
    if classes.is_empty() {
        c.text(
            Pos2::new(x0, y),
            Align2::LEFT_TOP,
            "No event or token links: no class has two members, so nothing fuses.",
            13.0,
            MUTED,
        );
    }
    for (id, colour, legs) in &classes {
        let n = legs.len() as f32;
        let h = n * 30.0 + 10.0;
        let center = Pos2::new(sx, y + h / 2.0);
        let col = kind_color(colour);
        for (i, m) in legs.iter().enumerate() {
            let my = y + 20.0 + i as f32 * 30.0;
            let r = Rect::from_min_size(Pos2::new(x0, my - 11.0), Vec2::new(220.0, 22.0));
            c.boxr(r, Color32::from_rgb(0xfd, 0xe2, 0xc4), MUTED, 1.0);
            c.mono(r.center(), Align2::CENTER_CENTER, m, 11.0, INK);
            c.curve(r.right_center(), center, col, 2.0);
            c.hover(r, info(format!("leg: {m}")).fact("class", id.as_str()).note("A member: it fires as the class (go-pflow's union-find, compose_fuse.go:9-20)."));
        }
        let dot = Rect::from_center_size(center, Vec2::splat(20.0));
        c.p.circle_filled(center, 9.0, col);
        c.text(
            center + Vec2::new(16.0, 0.0),
            Align2::LEFT_CENTER,
            id,
            11.5,
            col,
        );
        c.hover(
            dot,
            info(format!("spider {id}"))
                .fact("colour", *colour)
                .fact("legs", legs.join(", "))
                .note(if *colour == "event" {
                    "One event class = one spider: its members fuse into one flat transition."
                } else {
                    "A token wire: its places fuse into one flat place."
                }),
        );
        c.scene
            .emit("spider", id, json!({"colour": colour, "legs": legs}));
        y += h + 24.0;
    }
    for l in &ls {
        if l["leak"] == true {
            let r = c.text(
                Pos2::new(x0, y + 6.0),
                Align2::LEFT_TOP,
                &format!(
                    "leak: {} fires only with commit; on rollback the worker never comes back",
                    s(&l["key"])
                ),
                12.0,
                LEAK,
            );
            c.hover(r, link_info(l));
            c.scene.emit("leak", s(&l["key"]), json!({}));
            y += 22.0;
        }
    }
    c.note("analogy", "Analogy of shape only: ZX spiders without phases or any quantum semantics. The one law used, fusion of same-colour spiders that share a leg, is what go-pflow's Flatten does with its union-finds (compose_fuse.go:9-20).");
}

/// The plan's lists as the solver reads them (absent fields are defaults).

pub(super) fn spiders_c4(c: &mut Ctx, area: Rect) {
    let (plan, order, _, _, _) = plan_lists(c);
    let memo = plan.get("memo").is_some_and(|m| !m.is_null());
    let core = Rect::from_min_size(
        area.left_top() + Vec2::new(area.width() / 2.0 - 120.0, 20.0),
        Vec2::new(240.0, 70.0),
    );
    c.boxr(core, Color32::from_rgb(0xee, 0xf1, 0xf5), MUTED, 1.4);
    c.text(
        core.center(),
        Align2::CENTER_CENTER,
        "core · Solver::search",
        13.0,
        INK,
    );
    c.hover(
        core,
        info("the core search (fixed, not a piece)").fact(
            "tables",
            "reads and writes the transposition table; lends the history table to the chain",
        ),
    );
    let mut tt_legs: Vec<String> = vec![];
    if order.iter().any(|x| x == "tt_best") {
        tt_legs.push("tt_best".into());
    }
    if memo {
        tt_legs.push("memo".into());
    }
    let hi_legs: Vec<String> = order.iter().filter(|x| *x == "history").cloned().collect();
    for (k, (id, title, legs)) in [
        ("tt", "transposition table", &tt_legs),
        ("history", "history table", &hi_legs),
    ]
    .iter()
    .enumerate()
    {
        let center = Pos2::new(
            area.left() + area.width() * (0.3 + 0.4 * k as f32),
            core.bottom() + 120.0,
        );
        c.p.circle_stroke(center, 12.0, Stroke::new(2.0, INK));
        c.text(
            center + Vec2::new(0.0, 18.0),
            Align2::CENTER_TOP,
            title,
            12.0,
            INK,
        );
        c.line(
            center - Vec2::new(0.0, 12.0),
            core.center_bottom(),
            MUTED,
            1.0,
        );
        for (i, leg) in legs.iter().enumerate() {
            let at = center + Vec2::new(-60.0 + i as f32 * 120.0, 90.0);
            let r = c.chip(at - Vec2::new(30.0, 0.0), leg, false);
            c.line(center + Vec2::new(0.0, 12.0), r.center_top(), INK, 1.4);
        }
        c.hover(Rect::from_center_size(center, Vec2::splat(26.0)), info(format!("junction: the {title} (shared state, not a spider)")).fact("legs", legs.join(", ")).note("A junction, not a ZX spider: shared state that several parts read or write. Nothing fuses and no spider law applies."));
        c.scene.emit("junction", id, json!({"legs": legs}));
    }
    plan_warn(c);
    c.note("na", "A search plan has no event links, so there are no transition classes and nothing fuses: no spider is drawn here. The nearest honest picture is the plan's two pieces of shared state, drawn as junctions (hollow dots, not ZX spiders) with a leg to each piece in the plan that reads, writes or caps them.");
}

// ------------------------------------------------------------ 3 operad

//! The boundaries view (see `docs/composition-views-scene.md`).

use super::*;

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
    let groups: Vec<(String, Vec<String>)> = flat["fused_groups"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (k.clone(), strs(v)))
        .collect();
    let bx = area.left() + area.width() * 0.45;
    c.dashed(
        Pos2::new(bx, area.top()),
        Pos2::new(bx, area.bottom() - 30.0),
        kind_color("event"),
        1.5,
    );
    c.text(
        Pos2::new(bx, area.top()),
        Align2::CENTER_BOTTOM,
        "shared boundary",
        11.0,
        MUTED,
    );
    let mut y = area.top() + 20.0;
    for (i, (id, members)) in groups.iter().enumerate() {
        let letter = (b'A' + i as u8) as char;
        let port = Pos2::new(bx, y + members.len() as f32 * 13.0);
        for (j, m) in members.iter().enumerate() {
            let my = y + j as f32 * 26.0 + 10.0;
            let r = Rect::from_min_size(Pos2::new(area.left(), my - 10.0), Vec2::new(230.0, 20.0));
            c.boxr(r, Color32::WHITE, MUTED, 1.0);
            c.mono(r.center(), Align2::CENTER_CENTER, m, 10.5, INK);
            c.line(r.right_center(), port, kind_color("event"), 1.4);
            c.hover(r, info(format!("{m} is wired to boundary port {letter}")));
        }
        c.p.rect_filled(
            Rect::from_center_size(port, Vec2::splat(12.0)),
            1.0,
            kind_color("event"),
        );
        c.text(
            port + Vec2::new(10.0, -10.0),
            Align2::LEFT_BOTTOM,
            &letter.to_string(),
            12.0,
            INK,
        );
        c.hover(
            Rect::from_center_size(port, Vec2::splat(14.0)),
            info(format!(
                "boundary port {letter}: one event class, {} members",
                members.len()
            ))
            .fact("class", id.as_str()),
        );
        c.scene.emit("boundary", id, json!({"legs": members}));
        let card = Rect::from_min_size(
            Pos2::new(bx + 40.0, port.y - 16.0),
            Vec2::new(area.right() - bx - 50.0, 32.0),
        );
        c.boxr(
            card,
            Color32::from_rgb(0xee, 0xf6, 0xf0),
            kind_color("event"),
            1.0,
        );
        c.mono(
            card.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            id,
            10.5,
            INK,
        );
        c.line(port, card.left_center(), kind_color("event"), 1.0);
        c.hover(card, info(format!("composite {id}")).fact("members", members.join(", ")).note("The rendezvous rule: the class becomes one transition whose pre/postset is the union of its members'; it fires only when every member can."));
        c.scene.emit("composite", id, json!({"members": members}));
        y += members.len() as f32 * 26.0 + 30.0;
    }
    if groups.is_empty() {
        c.text(
            Pos2::new(bx + 20.0, area.top() + 20.0),
            Align2::LEFT_TOP,
            "an empty shared boundary: every transition is its own minimal synchronisation",
            12.0,
            MUTED,
        );
    }
    c.note("honesty", "Honesty: Bruni et al. compose two nets along one boundary; go-pflow's event classes are n-ary (union-find over binary links). Each class is drawn as one boundary port, and the composite transitions are exactly Flatten's fused groups.");
}

pub(super) fn boundaries_c4(c: &mut Ctx, area: Rect) {
    let r = Rect::from_min_size(
        area.left_top() + Vec2::new(area.width() * 0.25, 40.0),
        Vec2::new(area.width() * 0.5, area.height() * 0.6),
    );
    c.boxr(r, Color32::WHITE, MUTED, 1.0);
    draw_c4_net(c, r.shrink(10.0));
    c.text(
        r.center_bottom() + Vec2::new(0.0, 10.0),
        Align2::CENTER_TOP,
        "connect-4: a closed net, 0 boundary ports on either side",
        12.5,
        INK,
    );
    c.scene.emit("na", "boundaries", json!({}));
    c.note("na", "Does not apply: a search plan composes pieces into slots (plan.rs's check decides); nothing synchronises transitions across a boundary. And the Connect-4 net is closed: it has no ports, so as a net with boundaries it is one with both boundaries empty.");
}

// ------------------------------------------------------------ 7 lens

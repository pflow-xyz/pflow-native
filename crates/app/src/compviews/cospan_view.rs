//! The cospan view (see `docs/composition-views-scene.md`).

use super::*;

pub(super) fn cospan_orders(c: &mut Ctx, area: Rect) {
    let laid = draw_boxes(c, area, 210.0, true);
    for l in &laid {
        c.scene.emit("subnet", &l.id, json!({"pattern": l.pattern}));
        for (p, _) in &l.ports {
            c.scene.emit(
                "port",
                &format!("{}:{}", l.id, s(&p["id"])),
                json!({"side": side_of(s(&p["kind"]), s(&p["target"])), "target": s(&p["target"])}),
            );
        }
    }
    let ls = links(c);
    let mut glued = 0;
    for l in &ls {
        if draw_link(c, &laid, l).is_some() {
            let k = s(&l["kind"]);
            let style = match k {
                "token" | "data" => {
                    glued += 1;
                    "pushout"
                }
                "event" => "ghost",
                _ => "guard",
            };
            c.scene
                .emit("link", s(&l["key"]), json!({"kind": k, "style": style}));
        }
    }
    let form = if glued > 0 { "pushout" } else { "tensor" };
    let y = laid
        .iter()
        .map(|l| l.rect.bottom())
        .fold(area.top(), f32::max)
        + 110.0;
    let r = c.text(
        Pos2::new(area.left(), y),
        Align2::LEFT_TOP,
        &if glued > 0 {
            format!("composite: a pushout along {glued} glued boundary place(s) (Flatten's place classes)")
        } else {
            "composite: ⊗ only (disjoint union): no token or data link glues two feet; event links are dashed ghosts (transition fusion, not a pushout)".into()
        },
        13.0,
        INK,
    );
    c.scene.emit("composite", "bundle", json!({"form": form}));
    c.hover(
        r,
        info("the composite").fact("form", form).note(
            "Baez–Master: an open net is a cospan X → S ← Y; composing glues boundaries (a pushout).",
        ),
    );
    if glued == 0 {
        c.note(
            "na",
            "Only lk:token and lk:data are cospan composites: a Baez–Master open net has places on its feet, and go-pflow fuses the two places into one (Flatten's place classes). So as a cospan composite the instances are only ⊗ (disjoint union); the event links are drawn as dashed ghosts.",
        );
    } else {
        c.note(
            "na",
            "The pushout is drawn where a token or data link glues two feet: the two places become one, named and marked as go-pflow's Flatten does (wire:<smallest member>, initial markings summed).",
        );
    }
    c.note("judgment", comp::PARAMS_NOTE);
}

pub(super) fn cospan_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, _) = net_counts(c);
    let net = Rect::from_min_size(
        area.left_top() + Vec2::new(180.0, 30.0),
        Vec2::new(area.width() - 360.0, area.height() - 120.0),
    );
    c.boxr(net.expand(8.0), Color32::WHITE, MUTED, 1.0);
    draw_c4_net(c, net);
    c.text(
        Pos2::new(area.left() + 80.0, net.center().y),
        Align2::CENTER_CENTER,
        "∅",
        30.0,
        INK,
    );
    c.text(
        Pos2::new(area.right() - 80.0, net.center().y),
        Align2::CENTER_CENTER,
        "∅",
        30.0,
        INK,
    );
    c.text(
        Pos2::new(area.center().x, net.bottom() + 24.0),
        Align2::CENTER_TOP,
        &format!("connect-4: a closed net ∅ → N ← ∅ ({pl} places, {tr} transitions)"),
        13.0,
        INK,
    );
    c.scene
        .emit("net", "connect-4", json!({"places": pl, "transitions": tr}));
    c.scene.emit("note", "closed", json!({}));
    c.hover(
        net,
        info("connect-4 (pflow-rs tests/fixtures/connect-4.json, MIT)")
            .fact("places", pl.to_string())
            .fact("transitions", tr.to_string())
            .note("No ports: as an open net its feet are empty."),
    );
    c.note("na", "Does not apply as drawn: a SearchPlan is not an open net. It composes searches by chaining order pieces, wrapping drivers around the search and filling slots, and there is no place on its boundary to glue. The nearest honest picture is the net the plan searches, drawn as a closed net ∅ → N ← ∅.");
}

// ------------------------------------------------------------ 2 spiders

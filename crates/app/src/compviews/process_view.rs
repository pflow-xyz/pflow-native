//! The process view (see `docs/composition-views-scene.md`).

use super::*;

pub(super) fn process_orders(c: &mut Ctx, area: Rect) {
    let mut y = area.top();
    for r in c.d.runs.clone() {
        let id = s(&r["id"]).to_string();
        c.text(
            Pos2::new(area.left(), y),
            Align2::LEFT_TOP,
            &format!("{id}: {}", s(&r["title"])),
            12.0,
            INK,
        );
        y += 18.0;
        let mut x = area.left();
        let mut fired: Vec<String> = vec![];
        for st in arr(&r["steps"]) {
            let ok = st["enabled"] == true;
            let label = if ok {
                s(&st["flat"]).to_string()
            } else {
                s(&st["member"]).to_string()
            };
            let w = (label.len() as f32 * 6.0 + 12.0).min(330.0);
            let b = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 24.0));
            c.boxr(
                b,
                if ok {
                    Color32::WHITE
                } else {
                    Color32::from_rgb(0xfb, 0xe9, 0xe7)
                },
                if ok { INK } else { LEAK },
                1.0,
            );
            c.mono(b.center(), Align2::CENTER_CENTER, &label, 9.5, INK);
            c.hover(
                b,
                info(format!("step {}", s(&st["member"])))
                    .fact("flat", s(&st["flat"]))
                    .fact("why not", s(&st["why"])),
            );
            if ok {
                fired.push(label.clone());
            }
            if x > area.left() {
                c.line(Pos2::new(x - 8.0, y + 12.0), b.left_center(), MUTED, 1.0);
            }
            x = b.right() + 8.0;
        }
        let leak = arr(&r["expect"])
            .iter()
            .any(|e| s(&e["place"]) == "workers/available" && e["pass"] != true);
        if leak {
            c.text(
                Pos2::new(x + 6.0, y + 12.0),
                Align2::LEFT_CENTER,
                "a worker never comes back",
                11.0,
                LEAK,
            );
        }
        let stopped = s(&r["stopped_step"]).to_string();
        fired.sort();
        c.scene
            .emit("run", &id, json!({"fired": fired, "stopped_step": stopped}));
        y += 40.0;
    }
    c.note("honesty", "Individual tokens are a choice here: the net itself does not order tokens. The collective-token reading (Petri nets generate commutative monoidal categories) identifies runs that differ only in which token was used; Baez, Genovese, Master and Shulman treat both readings (Σ-nets).");
}

pub(super) fn process_c4(c: &mut Ctx, area: Rect) {
    let op = c.d.opening.clone();
    let mut x = area.left();
    for st in arr(&op["steps"]) {
        let b = Rect::from_min_size(Pos2::new(x, area.top() + 20.0), Vec2::new(120.0, 30.0));
        let ok = st["enabled"] == true;
        c.boxr(b, Color32::WHITE, if ok { INK } else { LEAK }, 1.0);
        c.mono(
            b.center(),
            Align2::CENTER_CENTER,
            s(&st["transition"]),
            10.5,
            INK,
        );
        c.hover(
            b,
            info(format!("drop {}", s(&st["transition"])))
                .fact("enabled", ok.to_string())
                .fact("why not", s(&st["why"])),
        );
        x = b.right() + 14.0;
    }
    let fired = op["fired"].as_i64().unwrap_or(0);
    c.scene.emit("run", "opening", json!({"fired": fired}));
    c.note("na", "No search is run and the search plan is not drawn here, so the plan pieces have no slot to snap into in this view. The plan (plan.rs) computes one value over all plays from a position; one play is one process. This is a fixed four-drop opening, each drop checked enabled by the firing rule (plan.opening).");
}

// ------------------------------------------------------------ 10 laws

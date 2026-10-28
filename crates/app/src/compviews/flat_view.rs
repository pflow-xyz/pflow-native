//! The flat view (see `docs/composition-views-scene.md`).

use super::*;

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
    let places = arr(&flat["places"]);
    let trans = arr(&flat["transitions"]);
    let cols = 6usize;
    let net = Rect::from_min_size(
        area.left_top(),
        Vec2::new(area.width() * 0.68, area.height() - 20.0),
    );
    let pos_p: BTreeMap<String, Pos2> = places
        .iter()
        .enumerate()
        .map(|(i, p)| {
            (
                s(&p["id"]).to_string(),
                Pos2::new(
                    net.left()
                        + 40.0
                        + (i % cols) as f32 * (net.width() - 80.0) / (cols - 1) as f32,
                    net.top() + 20.0 + (i / cols) as f32 * 64.0,
                ),
            )
        })
        .collect();
    let ty = net.top() + 20.0 + (places.len().div_ceil(cols)) as f32 * 64.0 + 30.0;
    let pos_t: BTreeMap<String, Pos2> = trans
        .iter()
        .enumerate()
        .map(|(i, t)| {
            (
                s(&t["id"]).to_string(),
                Pos2::new(
                    net.left() + 40.0 + (i % 4) as f32 * (net.width() - 80.0) / 3.0,
                    ty + (i / 4) as f32 * 56.0,
                ),
            )
        })
        .collect();
    for a in arr(&flat["arcs"]) {
        let (f, t) = (s(&a["from"]), s(&a["to"]));
        if let (Some(p), Some(q)) = (pos_p.get(f).or(pos_t.get(f)), pos_t.get(t).or(pos_p.get(t))) {
            let col = if s(&a["type"]) == "read" {
                kind_color("guard")
            } else {
                Color32::from_gray(170)
            };
            c.line(*p, *q, col, 0.8);
        }
    }
    for p in places {
        let id = s(&p["id"]);
        let at = pos_p[id];
        let fused = arr(&p["members"]).len() > 1;
        c.p.circle_filled(at, 11.0, Color32::WHITE);
        c.p.circle_stroke(at, 11.0, Stroke::new(1.2, INK));
        if fused {
            c.p.circle_stroke(at, 14.0, Stroke::new(1.0, INK));
        }
        let n = p["initial"].as_i64().unwrap_or(0);
        if n > 0 {
            c.text(at, Align2::CENTER_CENTER, &n.to_string(), 11.0, INK);
        }
        c.mono(
            at + Vec2::new(0.0, 16.0),
            Align2::CENTER_TOP,
            id,
            8.5,
            MUTED,
        );
        c.hover(
            Rect::from_center_size(at, Vec2::splat(24.0)),
            info(format!("place {id}"))
                .fact("initial", n.to_string())
                .fact("capacity", p["capacity"].to_string())
                .fact("members", strs(&p["members"]).join(", "))
                .note(s(&p["description"])),
        );
        c.scene
            .emit("place", id, json!({"initial": n, "fused": fused}));
    }
    for t in trans {
        let id = s(&t["id"]);
        let at = pos_t[id];
        let fused = t["fused"] == true;
        let r = Rect::from_center_size(at, Vec2::new(18.0, 26.0));
        c.p.rect_filled(r, 1.0, TRANS);
        if fused {
            c.p.rect_stroke(
                r.expand(3.0),
                1.0,
                Stroke::new(1.0, INK),
                StrokeKind::Middle,
            );
        }
        c.mono(
            at + Vec2::new(0.0, 18.0),
            Align2::CENTER_TOP,
            id,
            8.5,
            MUTED,
        );
        let en = arr(&flat["initial_enablement"])
            .iter()
            .find(|e| s(&e["transition"]) == id)
            .cloned()
            .unwrap_or(Value::Null);
        c.hover(
            r.expand(3.0),
            info(format!("transition {id}"))
                .fact("members", strs(&t["members"]).join(", "))
                .fact(
                    "enabled at the initial marking",
                    if en["enabled"] == true {
                        "yes".to_string()
                    } else {
                        s(&en["why"]).to_string()
                    },
                )
                .note(s(&t["description"])),
        );
        c.scene.emit("transition", id, json!({"fused": fused}));
    }
    // scenarios
    let mut y = area.top();
    let x = net.right() + 20.0;
    c.text(
        Pos2::new(x, y),
        Align2::LEFT_TOP,
        "scenarios (judgment, frozen):",
        12.0,
        INK,
    );
    y += 20.0;
    for r in c.d.runs.clone() {
        let pass = arr(&r["expect"]).iter().all(|e| e["pass"] == true);
        let stopped = s(&r["stopped_step"]).to_string();
        let line = format!(
            "{} {} — {}{}",
            if pass { "PASS" } else { "FAIL" },
            s(&r["id"]),
            s(&r["title"]),
            if stopped.is_empty() {
                String::new()
            } else {
                format!(" (stops at {stopped})")
            }
        );
        let rr = c.text(
            Pos2::new(x, y),
            Align2::LEFT_TOP,
            &line,
            11.5,
            if pass {
                Color32::from_rgb(0x2e, 0x8b, 0x57)
            } else {
                LEAK
            },
        );
        let mut i = info(format!("scenario {}", s(&r["id"])));
        for st in arr(&r["steps"]) {
            i = i.fact(
                s(&st["member"]),
                if st["enabled"] == true {
                    s(&st["flat"]).to_string()
                } else {
                    format!("refused: {}", s(&st["why"]))
                },
            );
        }
        for e in arr(&r["expect"]) {
            i = i.fact(
                &format!("{} == {}", s(&e["place"]), e["n"]),
                format!(
                    "{} ({})",
                    e["actual"],
                    if e["pass"] == true { "PASS" } else { "FAIL" }
                ),
            );
        }
        c.hover(
            rr,
            i.note("Fired through pflow-metamodel's enabled/fire on the flat net (bundle.run)."),
        );
        c.scene.emit(
            "scenario",
            s(&r["id"]),
            json!({"pass": pass, "stopped_step": stopped}),
        );
        y += 18.0;
    }
    c.note("na", "No ports are drawn here: Flatten removes the boundary, and a flat net has only places, transitions and arcs. Links are made in the pattern-level views; this view shows the one net they produce, under go-pflow's flat IDs.");
    c.note("judgment", "The scenarios and their expected predicates are judgment, written from the description and frozen (ROADMAP Phase 4).");
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

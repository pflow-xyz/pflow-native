//! The lens view (see `docs/composition-views-scene.md`).

use super::*;

pub(super) fn lens_orders(c: &mut Ctx, area: Rect, mode: &str) {
    let Some(rep) = c.d.lens.clone() else { return };
    let a = &rep["analysis"];
    // the mode toggle (a registry command)
    let t1 = Rect::from_min_size(area.left_top(), Vec2::new(90.0, 22.0));
    let t2 = Rect::from_min_size(
        area.left_top() + Vec2::new(96.0, 0.0),
        Vec2::new(150.0, 22.0),
    );
    c.button(
        t1,
        "strict",
        mode == "strict",
        "view.composition",
        json!({"lens": "strict"}),
    );
    c.button(
        t2,
        "assume a backward pass",
        mode == "assume",
        "view.composition",
        json!({"lens": "assume"}),
    );
    let holds = a["holds"] == true;
    let legal_token = a["legal"]["token"].as_i64().unwrap_or(0);
    let legal_data = a["legal"]["data"].as_i64().unwrap_or(0);
    let glued = a["glued"].as_i64().unwrap_or(0);
    let mut y = area.top() + 36.0;
    let v = c.text(Pos2::new(area.left(), y), Align2::LEFT_TOP, &format!("strict lens test: {} — {} boundary glued (legal: {legal_token} token, {legal_data} data links)", if holds { "a lens composite" } else { "not a lens composite" }, glued), 13.5, if holds { Color32::from_rgb(0x2e, 0x8b, 0x57) } else { LEAK });
    c.hover(v, info("the strict test").fact("holds", holds.to_string()).fact("glued", glued.to_string()).fact("no backward pass", strs(&a["no_backward"]).join(", ")).note("Lens composition is sequential: it needs a boundary to compose along, a backward map in every part, and the parts' backward maps to fire together as the forward pass does."));
    c.scene.emit("verdict", "strict", json!({"holds": holds, "glued": glued, "legal_token": legal_token, "legal_data": legal_data}));
    y += 28.0;
    let assume = mode == "assume";
    let colw = area.width() / 3.0 - 10.0;
    // pairs (the fallback's lenses)
    let x0 = area.left();
    c.text(
        Pos2::new(x0, y),
        Align2::LEFT_TOP,
        "backward pairs (found mechanically)",
        12.0,
        INK,
    );
    let mut py = y + 20.0;
    for p in arr(&a["pairs"]) {
        let id = format!("{}/{}->{}", s(&p["subnet"]), s(&p["t"]), s(&p["u"]));
        let r = Rect::from_min_size(
            Pos2::new(x0, py),
            Vec2::new(colw, if assume { 34.0 } else { 18.0 }),
        );
        if assume {
            c.boxr(r, Color32::from_rgb(0xf1, 0xea, 0xf7), MUTED, 1.0);
            c.mono(
                r.left_top() + Vec2::new(6.0, 3.0),
                Align2::LEFT_TOP,
                &format!("→ {} (forward)", s(&p["t"])),
                10.5,
                INK,
            );
            c.mono(
                r.left_top() + Vec2::new(6.0, 18.0),
                Align2::LEFT_TOP,
                &format!("← {} (backward)", s(&p["u"])),
                10.5,
                MUTED,
            );
        } else {
            c.mono(r.left_top(), Align2::LEFT_TOP, &id, 10.5, INK);
        }
        c.hover(
            r,
            info(format!("pair {id}"))
                .fact("residuals", strs(&p["residuals"]).join(", "))
                .fact("resources", strs(&p["resources"]).join(", "))
                .fact("exact inverse", p["exact"].to_string()),
        );
        c.scene.emit("pair", &id, json!({}));
        py = r.bottom() + 4.0;
    }
    // steps
    let x1 = area.left() + colw + 20.0;
    c.text(
        Pos2::new(x1, y),
        Align2::LEFT_TOP,
        "forward pass of the commit run, step by step",
        12.0,
        INK,
    );
    let mut sy = y + 20.0;
    for st in arr(&a["steps"]) {
        let id = s(&st["flat"]);
        let composes = st["composes"] == true;
        let split = st["split"] == true;
        let dead = st["dead_forward"]["constraint"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let r = Rect::from_min_size(Pos2::new(x1, sy), Vec2::new(colw, 36.0));
        c.boxr(
            r,
            Color32::WHITE,
            if composes {
                Color32::from_rgb(0x2e, 0x8b, 0x57)
            } else {
                LEAK
            },
            if composes { 1.0 } else { 2.0 },
        );
        c.mono(
            r.left_top() + Vec2::new(6.0, 3.0),
            Align2::LEFT_TOP,
            id,
            10.0,
            INK,
        );
        c.text(
            r.left_top() + Vec2::new(6.0, 19.0),
            Align2::LEFT_TOP,
            &format!(
                "{}{}{}",
                if composes {
                    "composes"
                } else {
                    "does not compose"
                },
                if split { " · backward maps split" } else { "" },
                if dead.is_empty() {
                    String::new()
                } else {
                    format!(" · dead by {dead}")
                }
            ),
            10.5,
            MUTED,
        );
        let back: Vec<String> = arr(&st["back"])
            .iter()
            .map(|b| s(&b["u"]).to_string())
            .collect();
        c.hover(
            r,
            info(format!("step {id}"))
                .fact("members", strs(&st["members"]).join(", "))
                .fact("backward maps", back.join(", "))
                .fact("composes", composes.to_string()),
        );
        c.scene.emit(
            "step",
            id,
            json!({"composes": composes, "split": split, "dead_forward": dead}),
        );
        sy = r.bottom() + 6.0;
    }
    // fixes
    let x2 = area.left() + 2.0 * (colw + 20.0);
    c.text(
        Pos2::new(x2, y),
        Align2::LEFT_TOP,
        "one-link repairs, each re-run",
        12.0,
        INK,
    );
    let mut fy = y + 20.0;
    for f in arr(&rep["fixes"]) {
        let id = format!("{}:{}", s(&f["how"]), s(&f["u"]));
        let fh = f["result"]["holds"] == true;
        let r = c.mono(
            Pos2::new(x2, fy),
            Align2::LEFT_TOP,
            &format!(
                "{id} ⊗ {} → {}",
                s(&f["target"]),
                if f["legal"] != true {
                    "refused"
                } else if fh {
                    "a lens"
                } else {
                    "not a lens"
                }
            ),
            10.5,
            INK,
        );
        let mut i = info(format!("repair {id}"))
            .fact("link", f["link"].to_string())
            .fact("legal", f["legal"].to_string());
        for run in arr(&f["result"]["runs"]) {
            i = i.fact(
                s(&run["id"]),
                if s(&run["stopped_step"]).is_empty() {
                    format!(
                        "{} predicates pass",
                        arr(&run["expect"])
                            .iter()
                            .filter(|e| e["pass"] == true)
                            .count()
                    )
                } else {
                    format!("stops at {}", s(&run["stopped_step"]))
                },
            );
        }
        c.hover(r, i);
        c.scene.emit("fix", &id, json!({"holds": fh}));
        fy += 18.0;
    }
    if assume {
        c.note("judgment", "Assumption (fallback): the pairs drawn as lenses were found mechanically (a residual only t produces and only u consumes; u returns what t moved on resource places), and the forward pass is the \"commit\" scenario's run (judgment).");
    } else {
        c.note("na", "Strict test: the composition as composed, no assumption. Switch to the fallback to see the pairs that do have a backward pass drawn as lenses.");
    }
    c.note("analogy", "Open games / lenses (Ghani, Hedges, Winschel, Zahn): forward play, backward values. Used here as a test of shape on the flat net, not as game theory.");
}

pub(super) fn lens_c4(c: &mut Ctx, area: Rect) {
    let (plan, order, drivers, _, _) = plan_lists(c);
    let names: Vec<String> = drivers.iter().map(driver_name).collect();
    let mut y = area.top();
    for d in &names {
        let r = Rect::from_min_size(
            Pos2::new(area.left(), y),
            Vec2::new(area.width() * 0.6, 26.0),
        );
        c.boxr(
            r,
            Color32::from_rgb(0xe8, 0xf5, 0xf2),
            Color32::from_rgb(0x17, 0x8f, 0x7f),
            1.2,
        );
        c.text(
            r.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            &format!("driver {d}: positions forward, value back"),
            11.5,
            INK,
        );
        c.hover(
            r,
            info(format!("driver {d}"))
                .note("A frame around the lenses below (drivers wrap the core)."),
        );
        c.scene.emit("frame", d, json!({}));
        y += 32.0;
    }
    for p in &order {
        let r = Rect::from_min_size(
            Pos2::new(area.left() + 30.0, y),
            Vec2::new(area.width() * 0.55, 30.0),
        );
        c.boxr(r, Color32::WHITE, Color32::from_rgb(0x2f, 0x6f, 0xb3), 1.2);
        c.mono(
            r.left_center() + Vec2::new(8.0, 0.0),
            Align2::LEFT_CENTER,
            &format!("{p}: ↓ position · ↑ value"),
            11.0,
            INK,
        );
        c.hover(
            r,
            info(format!("lens {p}")).note(
                "An order piece changes how soon a cutoff comes, never the value (order.rs:1-7).",
            ),
        );
        c.scene.emit("lens", p, json!({}));
        y += 36.0;
    }
    let memo = plan.get("memo").is_some_and(|m| !m.is_null());
    let r = c.text(
        Pos2::new(area.left(), y + 8.0),
        Align2::LEFT_TOP,
        &if memo {
            format!(
                "memo: {} entries (the transposition table, shared by every lens above)",
                plan["memo"]["entries"]
            )
        } else {
            "memo: none (the table is unbounded within one search)".into()
        },
        11.5,
        MUTED,
    );
    c.hover(r, info("memo").fact("present", memo.to_string()));
    c.scene.emit("memo", "memo", json!({"present": memo}));
    plan_warn(c);
    c.note("analogy", "Analogy of shape only. Open games (Ghani, Hedges, Winschel, Zahn) characterise equilibria: a best-response condition on strategy profiles. This plan computes a value: minimax with alpha-beta under a node budget. The lens drawing borrows only the shape (positions forward down the tree, values back up); no equilibrium is computed or checked.");
}

// ------------------------------------------------------------ 8 incidence

//! The incidence view (see `docs/composition-views-scene.md`).

use super::*;

pub(super) fn heat(c: &mut Ctx, r: Rect, m: &[Value], max_cell: f32) -> Rect {
    let rows = m.len().max(1);
    let cols = m.first().map_or(1, |x| arr(x).len().max(1));
    let cell = (r.width() / cols as f32)
        .min(r.height() / rows as f32)
        .min(max_cell);
    for (i, row) in m.iter().enumerate() {
        for (j, v) in arr(row).iter().enumerate() {
            let x = v.as_i64().unwrap_or(0);
            if x == 0 {
                continue;
            }
            let col = if x > 0 {
                Color32::from_rgb(0x2e, 0x8b, 0x57)
            } else {
                LEAK
            };
            c.p.rect_filled(
                Rect::from_min_size(
                    r.left_top() + Vec2::new(j as f32 * cell, i as f32 * cell),
                    Vec2::splat(cell.max(1.0)),
                ),
                0.0,
                col,
            );
        }
    }
    let used = Rect::from_min_size(
        r.left_top(),
        Vec2::new(cols as f32 * cell, rows as f32 * cell),
    );
    c.p.rect_stroke(used, 0.0, Stroke::new(0.8, MUTED), StrokeKind::Middle);
    used
}

pub(super) fn incidence_orders(c: &mut Ctx, area: Rect) {
    let Some(inc) = c.d.incidence.clone() else {
        return;
    };
    if inc["ok"] != true {
        c.note(
            "warn",
            format!(
                "Flatten refused the composition ({}), so there is no flat matrix.",
                s(&inc["error"])
            ),
        );
        return;
    }
    // disjoint union (block diagonal) beside the flat matrix
    let left = Rect::from_min_size(
        area.left_top() + Vec2::new(0.0, 20.0),
        Vec2::new(area.width() * 0.45, area.height() * 0.55),
    );
    c.text(
        area.left_top(),
        Align2::LEFT_TOP,
        "⊗: the subnets' blocks (block diagonal)",
        12.0,
        INK,
    );
    let mut off = left.left_top();
    for b in arr(&inc["blocks"]) {
        let rows = arr(&b["rows"]).len();
        let cols = arr(&b["cols"]).len();
        let used = heat(
            c,
            Rect::from_min_size(off, Vec2::new(cols as f32 * 10.0, rows as f32 * 10.0)),
            arr(&b["matrix"]),
            10.0,
        );
        c.hover(
            used,
            info(format!("block {}", s(&b["subnet"])))
                .fact("rows (places)", rows.to_string())
                .fact("cols (transitions)", cols.to_string()),
        );
        c.scene.emit(
            "block",
            s(&b["subnet"]),
            json!({"rows": rows, "cols": cols}),
        );
        off = used.right_bottom();
    }
    let right = Rect::from_min_size(
        Pos2::new(area.left() + area.width() * 0.5, area.top() + 20.0),
        Vec2::new(area.width() * 0.48, area.height() * 0.55),
    );
    c.text(
        right.left_top() - Vec2::new(0.0, 20.0),
        Align2::LEFT_TOP,
        "the flat net's C = post − pre (event fusion sums columns, token fusion sums rows)",
        12.0,
        INK,
    );
    let (rows, cols) = (arr(&inc["rows"]).len(), arr(&inc["cols"]).len());
    let used = heat(c, right, arr(&inc["matrix"]), 12.0);
    c.hover(
        used,
        info("the flat matrix")
            .fact("rows", rows.to_string())
            .fact("cols", cols.to_string()),
    );
    c.scene
        .emit("matrix", "flat", json!({"rows": rows, "cols": cols}));
    let mut y = area.top() + area.height() * 0.62;
    c.text(
        Pos2::new(area.left(), y),
        Align2::LEFT_TOP,
        "declared invariants, checked as y·C = 0:",
        12.0,
        INK,
    );
    y += 20.0;
    for k in arr(&inc["invariants"]) {
        let holds = k["holds"] == true;
        let r = c.mono(
            Pos2::new(area.left(), y),
            Align2::LEFT_TOP,
            &format!(
                "{} {} : {}",
                if holds { "✓" } else { "✗" },
                s(&k["id"]),
                s(&k["expr"])
            ),
            10.5,
            if holds { INK } else { LEAK },
        );
        c.hover(
            r,
            info(format!("invariant {}", s(&k["id"])))
                .fact("holds", holds.to_string())
                .note(
                    "Computed by bundle.incidence: y has a 1 on each place the expression names.",
                ),
        );
        c.scene
            .emit("invariant", s(&k["id"]), json!({"holds": holds}));
        y += 16.0;
    }
    c.note("na", "No ports here: an incidence matrix has no boundary. Make links in a view that draws ports (Cospans, Spiders, …); this view recomputes both matrices from the links.");
}

pub(super) fn incidence_c4(c: &mut Ctx, area: Rect) {
    let (pl, tr, _) = net_counts(c);
    let m = sp::connect4_model()
        .map(comp::incidence)
        .unwrap_or_default();
    let mv: Vec<Value> = m.iter().map(|r| json!(r)).collect();
    let used = heat(
        c,
        Rect::from_min_size(
            area.left_top(),
            Vec2::new(area.width(), area.height() - 30.0),
        ),
        &mv,
        6.0,
    );
    c.hover(
        used,
        info("connect-4's incidence matrix")
            .fact("rows (places)", pl.to_string())
            .fact("cols (transitions)", tr.to_string()),
    );
    c.scene
        .emit("matrix", "connect-4", json!({"rows": pl, "cols": tr}));
    c.note("na", "A SearchPlan has no incidence matrix: it is a composite of search pieces (plan.rs), not a P/T net, and its pieces have no places or transitions to put in one. Drawn instead, as the nearest honest thing: the incidence matrix of the net the plan searches.");
}

// ------------------------------------------------------------ 9 process

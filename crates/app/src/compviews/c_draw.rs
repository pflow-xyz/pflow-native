//! Drawing helpers of the lens, process and laws views: the prototype's SVG
//! primitives (text with a halo, arrows, dashed and rounded outlines,
//! flattened Beziers) as egui painter calls in design coordinates (`Xf`), and
//! hit testing along a path so a wire is inspectable where a rect is not
//! enough.

#![allow(clippy::too_many_arguments)]

use super::*;

pub(super) const PAPER: Color32 = Color32::from_rgb(0xfa, 0xfa, 0xfa);
/// The prototype's theme (`js/theme.js`): ink, muted, leak.
pub(super) const P_INK: Color32 = Color32::from_rgb(0x2b, 0x2f, 0x36);
pub(super) const P_MUTED: Color32 = Color32::from_rgb(0x6a, 0x71, 0x7c);
pub(super) const P_LEAK: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);

/// `SEARCH_TYPE_COLOR` and `PIECE_KIND_COLOR` of `js/theme.js`.
pub(super) fn type_color(t: &str) -> Color32 {
    match t {
        "Position" => hex("#2f6fb3"),
        "Value" | "Value⊥" => hex("#b0413e"),
        "MoveList" => hex("#c47a12"),
        "Window" => hex("#7b4cc2"),
        "Table" => hex("#178f7f"),
        "Cutoff" => hex("#8a6d3b"),
        _ => hex("#777777"),
    }
}

pub(super) fn kind_color_of(k: &str) -> Color32 {
    match k {
        "Order" => hex("#c47a12"),
        "Driver" => hex("#2f6fb3"),
        "Memo" => hex("#178f7f"),
        "Filter" => hex("#b0413e"),
        "Heuristic" => hex("#a8457f"),
        _ => hex("#888888"),
    }
}

/// A font size in design units, scaled, and never below legibility.
pub(super) fn fsz(xf: &Xf, size: f32) -> f32 {
    (size * xf.k).max(7.5)
}

pub(super) fn dtext(
    c: &Ctx,
    xf: &Xf,
    x: f32,
    y: f32,
    al: Align2,
    t: &str,
    size: f32,
    col: Color32,
) -> Rect {
    c.p.text(xf.pt(x, y), al, t, FontId::proportional(fsz(xf, size)), col)
}

pub(super) fn dmono(
    c: &Ctx,
    xf: &Xf,
    x: f32,
    y: f32,
    al: Align2,
    t: &str,
    size: f32,
    col: Color32,
) -> Rect {
    c.p.text(xf.pt(x, y), al, t, FontId::monospace(fsz(xf, size)), col)
}

/// Bold by a second pass half a pixel over (egui ships one weight).
pub(super) fn dbold(
    c: &Ctx,
    xf: &Xf,
    x: f32,
    y: f32,
    al: Align2,
    t: &str,
    size: f32,
    col: Color32,
    mono: bool,
) -> Rect {
    let f = if mono {
        FontId::monospace(fsz(xf, size))
    } else {
        FontId::proportional(fsz(xf, size))
    };
    let p = xf.pt(x, y);
    c.p.text(p + Vec2::new(0.45, 0.0), al, t, f.clone(), col);
    c.p.text(p, al, t, f, col)
}

/// Text with a paper-coloured halo (the prototype's `paint-order: stroke`).
pub(super) fn halo(
    c: &Ctx,
    xf: &Xf,
    x: f32,
    y: f32,
    al: Align2,
    t: &str,
    size: f32,
    col: Color32,
    bg: Color32,
    mono: bool,
) -> Rect {
    let f = if mono {
        FontId::monospace(fsz(xf, size))
    } else {
        FontId::proportional(fsz(xf, size))
    };
    let p = xf.pt(x, y);
    for d in [(-1.3, 0.0), (1.3, 0.0), (0.0, -1.3), (0.0, 1.3)] {
        c.p.text(p + Vec2::new(d.0, d.1), al, t, f.clone(), bg);
    }
    c.p.text(p, al, t, f, col)
}

/// Several lines of text, `lh` apart (design units), the first at `y`.
pub(super) fn dlines(
    c: &Ctx,
    xf: &Xf,
    x: f32,
    y: f32,
    al: Align2,
    lines: &[String],
    size: f32,
    lh: f32,
    col: Color32,
    mono: bool,
) -> Rect {
    let mut r = Rect::NOTHING;
    for (i, l) in lines.iter().enumerate() {
        let q = if mono {
            dmono(c, xf, x, y + i as f32 * lh, al, l, size, col)
        } else {
            dtext(c, xf, x, y + i as f32 * lh, al, l, size, col)
        };
        r = r.union(q);
    }
    r
}

/// The width a text takes at `size` (design units), measured by egui.
pub(super) fn tw(c: &Ctx, xf: &Xf, t: &str, size: f32, mono: bool) -> f32 {
    let f = if mono {
        FontId::monospace(fsz(xf, size))
    } else {
        FontId::proportional(fsz(xf, size))
    };
    let w =
        c.p.layout_no_wrap(t.to_string(), f, Color32::WHITE)
            .size()
            .x;
    w / xf.k.max(0.01)
}

/// A cubic Bezier as points (20 samples).
pub(super) fn cubic(a: Pos2, b: Pos2, c: Pos2, d: Pos2) -> Vec<Pos2> {
    (0..=20)
        .map(|i| {
            let t = i as f32 / 20.0;
            let u = 1.0 - t;
            let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            Pos2::new(
                w[0] * a.x + w[1] * b.x + w[2] * c.x + w[3] * d.x,
                w[0] * a.y + w[1] * b.y + w[2] * c.y + w[3] * d.y,
            )
        })
        .collect()
}

/// A design-space cubic into screen points.
pub(super) fn dcubic(xf: &Xf, p: [(f32, f32); 4]) -> Vec<Pos2> {
    cubic(
        xf.pt(p[0].0, p[0].1),
        xf.pt(p[1].0, p[1].1),
        xf.pt(p[2].0, p[2].1),
        xf.pt(p[3].0, p[3].1),
    )
}

pub(super) fn poly(c: &Ctx, pts: &[Pos2], col: Color32, w: f32) {
    if pts.len() >= 2 {
        c.p.add(Shape::line(pts.to_vec(), Stroke::new(w, col)));
    }
}

pub(super) fn dpoly(c: &Ctx, pts: &[Pos2], col: Color32, w: f32, dash: (f32, f32)) {
    if pts.len() >= 2 {
        c.p.extend(Shape::dashed_line(pts, Stroke::new(w, col), dash.0, dash.1));
    }
}

/// An arrow head at `tip`, pointing along `from -> tip`.
pub(super) fn arrow(c: &Ctx, from: Pos2, tip: Pos2, col: Color32, size: f32) {
    let d = tip - from;
    let l = d.length();
    if l < 0.01 {
        return;
    }
    let u = d / l;
    let n = Vec2::new(-u.y, u.x);
    let back = tip - u * size;
    c.p.add(Shape::convex_polygon(
        vec![tip, back + n * (size * 0.5), back - n * (size * 0.5)],
        col,
        Stroke::NONE,
    ));
}

/// A line with an optional arrow head and dash, through screen points.
pub(super) fn wire(c: &Ctx, pts: &[Pos2], col: Color32, w: f32, dash: bool, head: bool, k: f32) {
    if dash {
        dpoly(c, pts, col, w, (5.0 * k.max(0.7), 4.0 * k.max(0.7)));
    } else {
        poly(c, pts, col, w);
    }
    if head && pts.len() >= 2 {
        let tip = pts[pts.len() - 1];
        let mut from = pts[pts.len() - 2];
        for p in pts.iter().rev().skip(1) {
            if p.distance(tip) > 3.0 {
                from = *p;
                break;
            }
        }
        arrow(c, from, tip, col, 8.5 * k.max(0.7));
    }
}

/// Distance from `p` to the segment `a b`.
pub(super) fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-6 {
        return p.distance(a);
    }
    let t = (((p - a).dot(ab)) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Inspector hover on a path (the wire's hit area of the prototype).
pub(super) fn hover_path(c: &mut Ctx, pts: &[Pos2], tol: f32, i: Info) {
    let Some(p) = c.pointer else { return };
    if pts.windows(2).any(|s| seg_dist(p, s[0], s[1]) <= tol) {
        let r = Rect::from_center_size(p, Vec2::splat(1.0));
        c.hover(r, i);
    }
}

/// A rounded outline as points (for a dashed rounded box).
pub(super) fn round_outline(r: Rect, rad: f32) -> Vec<Pos2> {
    let rad = rad.min(r.width() / 2.0).min(r.height() / 2.0);
    let mut pts = vec![];
    let corners = [
        (r.right() - rad, r.top() + rad, -90.0f32),
        (r.right() - rad, r.bottom() - rad, 0.0),
        (r.left() + rad, r.bottom() - rad, 90.0),
        (r.left() + rad, r.top() + rad, 180.0),
    ];
    for (cx, cy, a0) in corners {
        for i in 0..=5 {
            let a = (a0 + 90.0 * i as f32 / 5.0).to_radians();
            pts.push(Pos2::new(cx + rad * a.cos(), cy + rad * a.sin()));
        }
    }
    pts.push(pts[0]);
    pts
}

/// A box: fill, then a solid or dashed rounded outline.
pub(super) fn dbox(
    c: &Ctx,
    xf: &Xf,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    rad: f32,
    fill: Color32,
    stroke: Color32,
    sw: f32,
    dashed: bool,
) -> Rect {
    let r = xf.rect(x, y, w, h);
    let rr = (rad * xf.k).max(1.0);
    if dashed {
        c.p.rect_filled(r, rr, fill);
        dpoly(
            c,
            &round_outline(r, rr),
            stroke,
            sw,
            (4.0 * xf.k, 3.0 * xf.k),
        );
    } else {
        c.p.rect(r, rr, fill, Stroke::new(sw, stroke), StrokeKind::Middle);
    }
    r
}

/// A drop target for a plan slot, without the standard box `Ctx::slot`
/// draws: lit by `plan.slot_candidates` while a piece is dragged, and the
/// drop runs `plan.add_piece`. Returns whether it is lit.
pub(super) fn slot_target(c: &mut Ctx, r: Rect, slot: &str) -> bool {
    let mut lit = false;
    let mut v: Option<Value> = None;
    if let Some(Drag::Piece {
        cands: Some(cs), ..
    }) = c.drag
    {
        if let Some(x) = arr(&cs["slots"]).iter().find(|x| s(&x["slot"]) == slot) {
            lit = x["ok"] == true;
            v = Some(x.clone());
        }
    }
    if lit {
        c.p.rect_stroke(
            r.expand(2.0),
            7.0,
            Stroke::new(3.0, LIT),
            StrokeKind::Middle,
        );
    }
    if c.released && c.pointer.is_some_and(|p| r.contains(p)) {
        if let (Some(Drag::Piece { name, .. }), Some(x)) = (c.drag, &v) {
            if x["ok"] == true {
                let n = name.clone();
                c.command("plan.add_piece", json!({"piece": n, "slot": slot}));
            }
        }
    }
    lit
}

/// The `slot` facts of `Ctx::slot` for the inspector (accepts, field, and
/// the verdict of a drag in progress).
pub(super) fn slot_info(c: &Ctx, slot: &str) -> Info {
    let mut i = info(format!("slot {slot}"));
    if let Some(sl) = sp::slot(slot) {
        i = i.fact("accepts", sl.accepts).fact(
            "field",
            sl.field
                .unwrap_or("(designed in the ROADMAP, not a SearchPlan field)"),
        );
    }
    if let Some(Drag::Piece {
        cands: Some(cs), ..
    }) = c.drag
    {
        if let Some(x) = arr(&cs["slots"]).iter().find(|x| s(&x["slot"]) == slot) {
            i = i.fact(
                if x["ok"] == true { "drop" } else { "refused" },
                s(&x["why"]).to_string() + if x["ok"] == true { "legal" } else { "" },
            );
        }
    }
    i
}

/// A small round remove button (×): a click removes the piece. Returns the
/// hit rect.
pub(super) fn remove_x(c: &mut Ctx, xf: &Xf, x: f32, y: f32, slot: &str, index: usize, what: &str) {
    let ctr = xf.pt(x, y);
    let r = Rect::from_center_size(ctr, Vec2::splat(16.0 * xf.k.max(0.8)));
    let hot = c.pointer.is_some_and(|p| r.contains(p));
    c.p.circle(
        ctr,
        6.5 * xf.k.max(0.8),
        if hot { P_LEAK } else { Color32::WHITE },
        Stroke::new(1.0, if hot { P_LEAK } else { hex("#b9bec6") }),
    );
    c.p.text(
        ctr,
        Align2::CENTER_CENTER,
        "×",
        FontId::proportional(fsz(xf, 11.0)),
        if hot { Color32::WHITE } else { P_MUTED },
    );
    c.hover(
        r,
        info(format!("remove {what}")).note(
            "Click to remove it (plan.remove_piece; the plan must still pass pflow-rs's check).",
        ),
    );
    if c.clicked && hot && c.drag.is_none() {
        c.command("plan.remove_piece", json!({"slot": slot, "index": index}));
    }
}

/// Formats an integer with thousands separators (`1,000,000`).
pub(super) fn fmt_n(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// A pflow-rs cite at the pinned revision: `pflow-rs <path>:<from>-<to> @aacb7eb (note)`.
pub(super) fn rs_cite(path: &str, from: u32, to: u32, note: &str) -> String {
    let lines = if to != 0 && to != from {
        format!(":{from}-{to}")
    } else if from != 0 {
        format!(":{from}")
    } else {
        String::new()
    };
    let note = if note.is_empty() {
        String::new()
    } else {
        format!(" ({note})")
    };
    format!(
        "pflow-rs crates/pflow-metamodel/{path}{lines} @{}{note}",
        sp::PLAN_RS_REV
    )
}

/// Wraps `t` at word boundaries to about `max_chars` per line.
pub(super) fn wrap_words(t: &str, max_chars: usize) -> Vec<String> {
    let mut lines = vec![];
    let mut cur = String::new();
    for w in t.split_whitespace() {
        if cur.is_empty() {
            cur = w.to_string();
        } else if cur.chars().count() + 1 + w.chars().count() <= max_chars {
            cur.push(' ');
            cur.push_str(w);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// A paragraph wrapped to `w` pixels at `pos`, with an optional bold lead-in
/// the first line starts after. Returns the rect it took.
pub(super) fn para(
    c: &Ctx,
    pos: Pos2,
    w: f32,
    text: &str,
    size: f32,
    col: Color32,
    lead: Option<(&str, Color32)>,
    mono: bool,
) -> Rect {
    let font = if mono {
        FontId::monospace(size)
    } else {
        FontId::proportional(size)
    };
    let mut lead_w = 0.0;
    if let Some((l, lc)) = lead {
        let g = c.p.layout_no_wrap(l.to_string(), font.clone(), lc);
        lead_w = g.size().x + 3.0;
        c.p.text(
            pos + Vec2::new(0.45, 0.0),
            Align2::LEFT_TOP,
            l,
            font.clone(),
            lc,
        );
        c.p.text(pos, Align2::LEFT_TOP, l, font.clone(), lc);
    }
    let mut job = egui::text::LayoutJob::simple(text.to_string(), font, col, w.max(40.0));
    if let Some(sec) = job.sections.first_mut() {
        sec.leading_space = lead_w;
    }
    let galley = c.p.layout_job(job);
    let r = Rect::from_min_size(pos, galley.size().max(Vec2::new(lead_w, 0.0)));
    c.p.galley(pos, galley, col);
    r
}

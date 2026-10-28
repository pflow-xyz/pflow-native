//! Colours and small drawing helpers of the operad view, from the
//! prototype's `js/theme.js` (the native `mod.rs` colours differ from them;
//! that discrepancy is reported to the lead, not edited here).
//!
//! `proto_*` names are kept so they can be de-duplicated after merging.

use super::a_draw::*;
use super::*;

pub(super) const FAINT: Color32 = Color32::from_rgb(0x9a, 0xa3, 0xad);
pub(super) const SLATE: Color32 = Color32::from_rgb(0x3c, 0x46, 0x56);
pub(super) const OP_MUTED: Color32 = Color32::from_rgb(0x6a, 0x71, 0x7c);
pub(super) const OP_INK: Color32 = Color32::from_rgb(0x2b, 0x2f, 0x36);
pub(super) const OP_LEAK: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);

/// Horizontal anchor of design-space text.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Al {
    L,
    M,
    R,
}

/// Text options.
#[derive(Clone, Copy)]
pub(super) struct Tx {
    pub size: f32,
    pub color: Color32,
    pub mono: bool,
    pub bold: bool,
    pub halo: Option<Color32>,
}

pub(super) fn tx(size: f32, color: Color32) -> Tx {
    Tx {
        size,
        color,
        mono: false,
        bold: false,
        halo: None,
    }
}

impl Tx {
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn halo(mut self, c: Color32) -> Self {
        self.halo = Some(c);
        self
    }
}

/// Estimated width of `t` in design units (the prototype measures real
/// text; this is deterministic and a little generous).
pub(super) fn tw(t: &str, size: f32, mono: bool) -> f32 {
    t.chars().count() as f32 * size * if mono { 0.61 } else { 0.56 }
}

/// Greedy word wrap to `max_w` design units.
pub(super) fn wrap(t: &str, max_w: f32, size: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut cur = String::new();
    for w in t.split(' ') {
        let cand = if cur.is_empty() {
            w.to_string()
        } else {
            format!("{cur} {w}")
        };
        if !cur.is_empty() && tw(&cand, size, false) > max_w {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        } else {
            cur = cand;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Thousands separators, as the prototype's `fmtN`.
pub(super) fn fmt_n(n: i64) -> String {
    let t = n.to_string();
    let mut out = String::new();
    for (i, ch) in t.chars().enumerate() {
        if i > 0 && (t.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

pub(super) fn ext_note(mut i: Info, n: &str) -> Info {
    if n.is_empty() {
        return i;
    }
    i.note = if i.note.is_empty() {
        n.to_string()
    } else {
        format!("{} {n}", i.note)
    };
    i
}

impl<'a> Ctx<'a> {
    /// Design-space text; `y` is the baseline-ish line the prototype uses.
    pub(super) fn txt(&self, xf: Xf, x: f32, y: f32, al: Al, t: &str, o: Tx) -> Rect {
        let a = match al {
            Al::L => Align2::LEFT_CENTER,
            Al::M => Align2::CENTER_CENTER,
            Al::R => Align2::RIGHT_CENTER,
        };
        let at = Pos2::new(x, y - o.size * 0.34);
        let r = self.dt(xf, at, a, t, o.size, o.color, o.halo, o.mono);
        if o.bold {
            self.dt(
                xf,
                Pos2::new(x + 0.4, at.y),
                a,
                t,
                o.size,
                o.color,
                None,
                o.mono,
            );
        }
        r
    }

    /// Text with subscripts: `(part, is_subscript)`; returns the right edge
    /// in design units.
    pub(super) fn rich(&self, xf: Xf, x: f32, y: f32, parts: &[(String, bool)], o: Tx) -> f32 {
        let mut cx = x;
        for (t, sub) in parts {
            let (sz, dy) = if *sub {
                (o.size * 0.74, o.size * 0.3)
            } else {
                (o.size, 0.0)
            };
            let r = self.txt(xf, cx, y + dy, Al::L, t, Tx { size: sz, ..o });
            cx += r.width() / xf.k.max(0.1) + 0.5;
        }
        cx
    }

    /// A rounded rectangle in design space, filled and/or stroked, solid
    /// or dashed.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn rrect(
        &self,
        xf: Xf,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        fill: Option<Color32>,
        stroke: Option<(Color32, f32)>,
        dash: Option<(f32, f32)>,
    ) {
        let rect = xf.rect(x, y, w, h);
        if let Some(f) = fill {
            self.p.rect_filled(rect, xf.len(r), f);
        }
        if let Some((col, sw)) = stroke {
            match dash {
                None => {
                    self.p
                        .rect_stroke(rect, xf.len(r), Stroke::new(sw, col), StrokeKind::Middle);
                }
                Some((d, g)) => {
                    let pts = rrect_outline(rect, xf.len(r));
                    self.dash_poly(&pts, col, sw, xf.len(d), xf.len(g));
                }
            }
        }
    }

    /// Diagonal hatching inside `rect` (the prototype's `opv-hatch`).
    pub(super) fn hatch(&self, xf: Xf, x: f32, y: f32, w: f32, h: f32) {
        let rect = xf.rect(x, y, w, h);
        let pp = self.p.with_clip_rect(rect);
        let step = xf.len(11.3);
        let col = hex("#e3e5e9");
        let mut k = -rect.height();
        while k < rect.width() {
            pp.line_segment(
                [
                    Pos2::new(rect.left() + k, rect.bottom()),
                    Pos2::new(rect.left() + k + rect.height(), rect.top()),
                ],
                Stroke::new(xf.len(3.0), col),
            );
            k += step;
        }
    }
}

/// A rounded rectangle's outline as a closed polyline.
pub(super) fn rrect_outline(r: Rect, rad: f32) -> Vec<Pos2> {
    let rad = rad.min(r.width() / 2.0).min(r.height() / 2.0);
    let (l, t, rt, b) = (r.left(), r.top(), r.right(), r.bottom());
    let mut v = vec![];
    for (c, a0) in [
        (Pos2::new(rt - rad, t + rad), -90.0_f32),
        (Pos2::new(rt - rad, b - rad), 0.0),
        (Pos2::new(l + rad, b - rad), 90.0),
        (Pos2::new(l + rad, t + rad), 180.0),
    ] {
        for i in 0..=5 {
            let a = (a0 + 90.0 * i as f32 / 5.0).to_radians();
            v.push(Pos2::new(c.x + rad * a.cos(), c.y + rad * a.sin()));
        }
    }
    v.push(v[0]);
    v
}

//! Drawing helpers shared by the flat, boundaries and incidence views: text
//! with a halo or wrapped to a width, arrows, token dots, badges, hover over a
//! segment, the prototype's colours (`js/theme.js`) and cite labels.

use super::*;

pub const PAPER: Color32 = Color32::from_rgb(0xfa, 0xfa, 0xfa);
pub const ARC: Color32 = Color32::from_rgb(0x86, 0x8d, 0x97);
pub const EN: Color32 = Color32::from_rgb(0x1e, 0x7a, 0x46);
pub const EN_FILL: Color32 = Color32::from_rgb(0xe6, 0xf4, 0xea);
pub const FIRE: Color32 = Color32::from_rgb(0xc4, 0x7a, 0x12);
pub const READ: Color32 = Color32::from_rgb(0x33, 0x66, 0xaa);
pub const INHIB: Color32 = Color32::from_rgb(0xaa, 0x33, 0x33);
pub const PRE: Color32 = Color32::from_rgb(0x2f, 0x6f, 0xb3);
pub const POST: Color32 = Color32::from_rgb(0xc4, 0x7a, 0x12);
pub const BLOCKED: Color32 = Color32::from_rgb(0xb2, 0x34, 0x2a);

/// The prototype's pattern tints (`theme.js` PATTERN_TINT).
pub fn pattern_tint_p(pattern: &str) -> Color32 {
    match pattern {
        "queue" => hex("#e8f0fa"),
        "resource-pool" => hex("#fbf0e0"),
        "saga" => hex("#f1ebfa"),
        _ => hex("#eef0f3"),
    }
}

/// The prototype's schema colours (`theme.js` SCHEMA_COLOR).
pub fn schema_color_p(schema: &str) -> Color32 {
    match schema {
        "queue:items" => hex("#2f6fb3"),
        "queue:payload" => hex("#6b7f99"),
        "resource" => hex("#c47a12"),
        "account" => hex("#7b4cc2"),
        "saga:outcome" => hex("#178f7f"),
        _ => hex("#777777"),
    }
}

pub const TRANSITION_PORT: Color32 = Color32::from_rgb(0x3c, 0x46, 0x56);

/// "go-pflow metamodel/firing.go:142-196 @69257b3 (note)": a go-pflow cite
/// at the revision the ontology records (checked against it).
pub fn go(path: &str, from: u32, to: u32, note: &str) -> String {
    let rev = comp::ontology()["repos"]["goPflow"]["rev"]
        .as_str()
        .unwrap_or("");
    let lines = if to != from {
        format!("{from}-{to}")
    } else {
        from.to_string()
    };
    format!("go-pflow {path}:{lines} @{rev} ({note})")
}

/// A pflow-rs cite at the pinned revision (`aacb7eb`), lines re-checked there.
pub fn rs(path: &str, from: u32, to: u32, note: &str) -> String {
    let lines = if to != from {
        format!("{from}-{to}")
    } else {
        from.to_string()
    };
    format!("pflow-rs {path}:{lines} @{} ({note})", sp::PLAN_RS_REV)
}

/// An ontology cite (`ns/patterns.ttl`, ROADMAP.md) at its recorded revision.
pub fn onto(repo: &str, path: &str, from: u32, to: u32, note: &str) -> String {
    let name = comp::ontology()["repos"][repo]["name"]
        .as_str()
        .unwrap_or("ontology-pflow-xyz");
    let rev = comp::ontology()["repos"][repo]["rev"]
        .as_str()
        .unwrap_or("");
    let lines = if to != from {
        format!("{from}-{to}")
    } else {
        from.to_string()
    };
    format!("{name} {path}:{lines} @{rev} ({note})")
}

pub fn dist_to_seg(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-6 {
        return p.distance(a);
    }
    let t = (((p - a).dot(ab)) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

impl<'a> Ctx<'a> {
    pub fn pointer_pos(&self) -> Option<Pos2> {
        self.pointer
    }

    pub fn is_clicked(&self) -> bool {
        self.clicked
    }

    /// Text with a halo of the page colour (the prototype's `paint-order:
    /// stroke`), so a label stays readable over arcs.
    pub fn halo(
        &self,
        pos: Pos2,
        align: Align2,
        t: &str,
        size: f32,
        c: Color32,
        mono: bool,
    ) -> Rect {
        let font = if mono {
            FontId::monospace(size)
        } else {
            FontId::proportional(size)
        };
        let d = (size * 0.16).max(1.0);
        for (dx, dy) in [
            (-d, 0.0),
            (d, 0.0),
            (0.0, -d),
            (0.0, d),
            (-d, -d),
            (d, d),
            (-d, d),
            (d, -d),
        ] {
            self.p
                .text(pos + Vec2::new(dx, dy), align, t, font.clone(), PAPER);
        }
        self.p.text(pos, align, t, font, c)
    }

    /// Width of `t` set at `size` (monospace when `mono`).
    pub fn text_w(&self, t: &str, size: f32, mono: bool) -> f32 {
        let font = if mono {
            FontId::monospace(size)
        } else {
            FontId::proportional(size)
        };
        self.p
            .layout_no_wrap(t.to_string(), font, Color32::BLACK)
            .size()
            .x
    }

    /// Text wrapped to `width`; returns the height it took.
    pub fn wrapped(
        &self,
        pos: Pos2,
        t: &str,
        size: f32,
        c: Color32,
        width: f32,
        mono: bool,
    ) -> f32 {
        let font = if mono {
            FontId::monospace(size)
        } else {
            FontId::proportional(size)
        };
        let g = self.p.layout(t.to_string(), font, c, width.max(20.0));
        let h = g.size().y;
        self.p.galley(pos, g, c);
        h
    }

    /// The height `t` would take wrapped to `width`.
    pub fn wrapped_h(&self, t: &str, size: f32, width: f32, mono: bool) -> f32 {
        let font = if mono {
            FontId::monospace(size)
        } else {
            FontId::proportional(size)
        };
        self.p
            .layout(t.to_string(), font, Color32::BLACK, width.max(20.0))
            .size()
            .y
    }

    /// A small rounded badge (PASS / FAIL / FIRED …); returns its rect.
    pub fn badge(&self, pos: Pos2, t: &str, fg: Color32, bg: Color32, size: f32) -> Rect {
        let w = self.text_w(t, size, false) + 8.0;
        let r = Rect::from_min_size(pos, Vec2::new(w, size + 4.0));
        self.p.rect_filled(r, 3.0, bg);
        self.p.text(
            r.center(),
            Align2::CENTER_CENTER,
            t,
            FontId::proportional(size),
            fg,
        );
        r
    }

    /// A line with a filled arrowhead at `b`.
    pub fn arrow(&self, a: Pos2, b: Pos2, col: Color32, w: f32, head: f32) {
        self.line(a, b, col, w);
        let d = (b - a).normalized();
        if !d.x.is_finite() {
            return;
        }
        let n = Vec2::new(-d.y, d.x);
        self.p.add(Shape::convex_polygon(
            vec![
                b,
                b - d * head + n * head * 0.4,
                b - d * head - n * head * 0.4,
            ],
            col,
            Stroke::NONE,
        ));
    }

    /// Hover over a segment (within `tol` px of it).
    pub fn hover_seg(&mut self, a: Pos2, b: Pos2, tol: f32, i: Info) {
        if self.pointer.is_some_and(|p| dist_to_seg(p, a, b) <= tol) {
            if self.clicked {
                self.clicked_info = Some(i.clone());
            }
            self.hovered = Some(i);
        }
    }

    /// Hover anywhere in a circle.
    pub fn hover_circle(&mut self, at: Pos2, r: f32, i: Info) {
        if self.pointer.is_some_and(|p| p.distance(at) <= r) {
            if self.clicked {
                self.clicked_info = Some(i.clone());
            }
            self.hovered = Some(i);
        }
    }

    pub fn pointer_in(&self, r: Rect) -> bool {
        self.pointer.is_some_and(|p| r.contains(p))
    }

    /// Whether the click this frame landed in `r`.
    pub fn click_in(&self, r: Rect) -> bool {
        self.clicked && self.pointer_in(r)
    }

    pub fn hand(&self) {
        self.p.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    /// Tokens inside a place of radius `r`: dots up to four, else the count.
    pub fn tokens(&self, at: Pos2, n: i64, r: f32) {
        if n <= 0 {
            return;
        }
        let d = r * 0.24;
        let dot_r = (r * 0.2).max(2.2);
        let dot = |x: f32, y: f32| {
            self.p.circle_filled(at + Vec2::new(x, y), dot_r, INK);
        };
        match n {
            1 => dot(0.0, 0.0),
            2 => {
                dot(-d * 1.4, 0.0);
                dot(d * 1.4, 0.0);
            }
            3 => {
                dot(0.0, -d * 1.3);
                dot(-d * 1.4, d * 0.9);
                dot(d * 1.4, d * 0.9);
            }
            4 => {
                dot(-d * 1.3, -d * 1.3);
                dot(d * 1.3, -d * 1.3);
                dot(-d * 1.3, d * 1.3);
                dot(d * 1.3, d * 1.3);
            }
            _ => {
                self.p.text(
                    at,
                    Align2::CENTER_CENTER,
                    &n.to_string(),
                    FontId::proportional((r * 0.85).max(9.0)),
                    INK,
                );
            }
        }
    }

    /// A rounded rect outline drawn as the prototype's double border
    /// (`peripheries=2`) when `fused`.
    pub fn double_rect(&self, r: Rect, rad: f32, fill: Color32, stroke: Stroke, inner: f32) {
        self.p
            .rect(r, rad, Color32::WHITE, stroke, StrokeKind::Middle);
        self.p.rect(
            r.shrink(inner),
            (rad - 2.0).max(1.0),
            fill,
            stroke,
            StrokeKind::Middle,
        );
    }
}

pub fn fmt_c(v: i64) -> String {
    match v {
        v if v > 0 => format!("+{v}"),
        v if v < 0 => format!("−{}", -v),
        _ => "0".into(),
    }
}

/// Whether `id` is a flat id of a fused transition/wire, "fused:a+b" → ["a","b"].
pub fn alpha(i: usize) -> char {
    (b'A' + (i % 26) as u8) as char
}

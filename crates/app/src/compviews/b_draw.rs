//! Drawing primitives of the string view: the prototype's SVG elements
//! (rounded rects, dashed paths, cubics, ortho paths with rounded corners,
//! text with a paper halo) on an egui [`Painter`], in design coordinates
//! mapped by [`Xf`]. Nothing here knows about the model.

use super::*;

/// diagram-viewer.js's visual constants (dv:8-16, 433-526), as the prototype's `DV`.
pub(super) mod dv {
    use super::*;
    pub const BOX_H: f32 = 40.0;
    pub const RX: f32 = 6.0;
    pub const SW: f32 = 1.5;
    pub const PORT_R: f32 = 4.0;
    pub const LABEL: f32 = 10.0;
    pub const STROKE: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
    pub const PORT: Color32 = Color32::from_rgb(0x66, 0x66, 0x66);
    pub const WIRE: Color32 = Color32::from_rgb(0x88, 0x88, 0x88);
    pub const LABEL_FILL: Color32 = Color32::from_rgb(0x66, 0x66, 0x66);
    pub const BACK: Color32 = Color32::from_rgb(0xcc, 0x44, 0x44);
    pub const PAPER: Color32 = Color32::from_rgb(0xfa, 0xfa, 0xfa);
    pub const BACK_DASH: (f32, f32) = (6.0, 3.0);
}

/// The prototype's theme.js: wire colours by schema, search type, piece kind.
pub(super) mod theme {
    use super::*;
    pub const TRANSITION_PORT: Color32 = Color32::from_rgb(0x3c, 0x46, 0x56);
    pub const PAPER_INK: Color32 = Color32::from_rgb(0x2b, 0x2f, 0x36);
    pub const MUTED: Color32 = Color32::from_rgb(0x6a, 0x71, 0x7c);
    pub const LEAK: Color32 = Color32::from_rgb(0xc0, 0x39, 0x2b);

    pub fn schema(s: &str) -> Color32 {
        match s {
            "queue:items" => hex("#2f6fb3"),
            "queue:payload" => hex("#6b7f99"),
            "resource" => hex("#c47a12"),
            "account" => hex("#7b4cc2"),
            "saga:outcome" => hex("#178f7f"),
            _ => hex("#777777"),
        }
    }
    /// A port: a transition port is one neutral colour, a place port its schema's.
    pub fn port(is_transition: bool, schema_name: &str) -> Color32 {
        if is_transition {
            TRANSITION_PORT
        } else {
            schema(schema_name)
        }
    }
    pub fn pattern_tint(p: &str) -> Color32 {
        match p {
            "queue" => hex("#e8f0fa"),
            "resource-pool" => hex("#fbf0e0"),
            "saga" => hex("#f1ebfa"),
            _ => hex("#f3f4f6"),
        }
    }
    pub fn search_type(t: &str) -> Color32 {
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
    pub fn piece_kind(k: &str) -> Color32 {
        match k {
            "Order" => hex("#c47a12"),
            "Driver" => hex("#2f6fb3"),
            "Memo" => hex("#178f7f"),
            "Filter" => hex("#b0413e"),
            "Heuristic" => hex("#a8457f"),
            _ => hex("#888888"),
        }
    }
}

/// A painter and the design→window transform.
#[derive(Clone)]
pub(super) struct D {
    pub p: egui::Painter,
    pub xf: Xf,
}

pub(super) type Pt = (f32, f32);

impl D {
    pub fn pt(&self, p: Pt) -> Pos2 {
        self.xf.pt(p.0, p.1)
    }
    pub fn k(&self) -> f32 {
        self.xf.k
    }
    /// Text width in design units (the window scale divided out), for layouts.
    pub fn tw(&self, t: &str, size: f32) -> f32 {
        // measured at the design size: the layout must not depend on the zoom
        self.p
            .layout_no_wrap(t.to_string(), FontId::proportional(size), Color32::BLACK)
            .size()
            .x
    }
    pub fn twm(&self, t: &str, size: f32) -> f32 {
        self.p
            .layout_no_wrap(t.to_string(), FontId::monospace(size), Color32::BLACK)
            .size()
            .x
    }

    pub fn rect_r(&self, x: f32, y: f32, w: f32, h: f32) -> Rect {
        self.xf.rect(x, y, w, h)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rect(
        &self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        rx: f32,
        fill: Color32,
        stroke: Color32,
        sw: f32,
        dash: Option<(f32, f32)>,
    ) {
        let r = self.xf.rect(x, y, w, h);
        let k = self.xf.k;
        let st = if stroke == Color32::TRANSPARENT {
            Stroke::NONE
        } else {
            Stroke::new((sw * k).max(0.6), stroke)
        };
        match dash {
            None => {
                self.p.rect(r, rx * k, fill, st, StrokeKind::Middle);
            }
            Some(d) => {
                self.p.rect_filled(r, rx * k, fill);
                // a dashed rounded rect: the outline as a polyline
                let mut pts = vec![];
                let rr = (rx * k).min(r.width() / 2.0).min(r.height() / 2.0);
                let corner = |c: Pos2, a0: f32, pts: &mut Vec<Pos2>| {
                    for i in 0..=6 {
                        let a = a0 + std::f32::consts::FRAC_PI_2 * i as f32 / 6.0;
                        pts.push(c + Vec2::new(a.cos(), a.sin()) * rr);
                    }
                };
                use std::f32::consts::PI;
                corner(Pos2::new(r.right() - rr, r.top() + rr), -PI / 2.0, &mut pts);
                corner(Pos2::new(r.right() - rr, r.bottom() - rr), 0.0, &mut pts);
                corner(
                    Pos2::new(r.left() + rr, r.bottom() - rr),
                    PI / 2.0,
                    &mut pts,
                );
                corner(Pos2::new(r.left() + rr, r.top() + rr), PI, &mut pts);
                pts.push(pts[0]);
                self.p
                    .extend(Shape::dashed_line(&pts, st, d.0 * k, d.1 * k));
            }
        }
    }

    pub fn line(&self, a: Pt, b: Pt, color: Color32, w: f32) {
        self.p.line_segment(
            [self.pt(a), self.pt(b)],
            Stroke::new((w * self.xf.k).max(0.6), color),
        );
    }

    pub fn path(&self, pts: &[Pt], color: Color32, w: f32, dash: Option<(f32, f32)>) {
        if pts.len() < 2 {
            return;
        }
        let v: Vec<Pos2> = pts.iter().map(|p| self.pt(*p)).collect();
        let st = Stroke::new((w * self.xf.k).max(0.6), color);
        match dash {
            Some((a, b)) => {
                self.p
                    .extend(Shape::dashed_line(&v, st, a * self.xf.k, b * self.xf.k));
            }
            None => {
                self.p.add(Shape::line(v, st));
            }
        }
    }

    pub fn cubic(&self, c: [Pt; 4], color: Color32, w: f32, dash: Option<(f32, f32)>) {
        self.path(&flatten(c, 20), color, w, dash);
    }

    /// An arrowhead (dv:434, "0 0,8 3,0 6", refX 8) at the end of a path coming along `dir`.
    pub fn arrow(&self, tip: Pt, from: Pt, color: Color32) {
        let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
        let l = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (ux, uy) = (dx / l, dy / l);
        let (nx, ny) = (-uy, ux);
        let b = (tip.0 - ux * 8.0, tip.1 - uy * 8.0);
        let pts = vec![
            self.pt(tip),
            self.pt((b.0 + nx * 3.0, b.1 + ny * 3.0)),
            self.pt((b.0 - nx * 3.0, b.1 - ny * 3.0)),
        ];
        self.p.add(Shape::convex_polygon(pts, color, Stroke::NONE));
    }

    pub fn dot(&self, c: Pt, r: f32, fill: Color32) {
        self.p.circle_filled(self.pt(c), r * self.xf.k, fill);
    }
    pub fn ring(&self, c: Pt, r: f32, stroke: Color32, w: f32) {
        self.p.circle_stroke(
            self.pt(c),
            r * self.xf.k,
            Stroke::new(w * self.xf.k, stroke),
        );
    }
    pub fn disc(&self, c: Pt, r: f32, fill: Color32, stroke: Color32, w: f32) {
        self.p.circle(
            self.pt(c),
            r * self.xf.k,
            fill,
            Stroke::new(w * self.xf.k, stroke),
        );
    }

    /// Text at a design point; `size` is in design units.
    pub fn text(
        &self,
        at: Pt,
        align: Align2,
        t: &str,
        size: f32,
        color: Color32,
        mono: bool,
    ) -> Rect {
        let f = if mono {
            FontId::monospace(size * self.xf.k)
        } else {
            FontId::proportional(size * self.xf.k)
        };
        self.p.text(self.pt(at), align, t, f, color)
    }

    /// Text with a paper halo (SVG `paint-order: stroke`).
    pub fn halo(&self, at: Pt, align: Align2, t: &str, size: f32, color: Color32) -> Rect {
        let f = FontId::proportional(size * self.xf.k);
        let o = 1.4 * self.xf.k;
        for (dx, dy) in [(o, 0.0), (-o, 0.0), (0.0, o), (0.0, -o)] {
            self.p.text(
                self.pt(at) + Vec2::new(dx, dy),
                align,
                t,
                f.clone(),
                dv::PAPER,
            );
        }
        self.p.text(self.pt(at), align, t, f, color)
    }
}

pub(super) fn flatten(c: [Pt; 4], n: usize) -> Vec<Pt> {
    (0..=n)
        .map(|i| bez(c, i as f32 / n as f32))
        .collect::<Vec<_>>()
}

/// A point on a cubic.
pub(super) fn bez(c: [Pt; 4], t: f32) -> Pt {
    let u = 1.0 - t;
    let f = |a: f32, b: f32, cc: f32, d: f32| {
        u * u * u * a + 3.0 * u * u * t * b + 3.0 * u * t * t * cc + t * t * t * d
    };
    (
        f(c[0].0, c[1].0, c[2].0, c[3].0),
        f(c[0].1, c[1].1, c[2].1, c[3].1),
    )
}

/// The prototype's `ortho`: an axis-aligned path with rounded corners,
/// flattened to a polyline.
pub(super) fn ortho(points: &[Pt], r: f32) -> Vec<Pt> {
    let mut pts: Vec<Pt> = vec![];
    for (i, p) in points.iter().enumerate() {
        if i == 0
            || ((p.0 - pts[pts.len() - 1].0).powi(2) + (p.1 - pts[pts.len() - 1].1).powi(2)).sqrt()
                > 0.01
        {
            pts.push(*p);
        }
    }
    if pts.len() < 3 {
        return pts;
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let (p0, p1, p2) = (pts[i - 1], pts[i], pts[i + 1]);
        let l1 = ((p1.0 - p0.0).powi(2) + (p1.1 - p0.1).powi(2)).sqrt();
        let l2 = ((p2.0 - p1.0).powi(2) + (p2.1 - p1.1).powi(2)).sqrt();
        let rr = r.min(l1 / 2.0).min(l2 / 2.0);
        let a = (
            p1.0 - (p1.0 - p0.0) / l1 * rr,
            p1.1 - (p1.1 - p0.1) / l1 * rr,
        );
        let b = (
            p1.0 + (p2.0 - p1.0) / l2 * rr,
            p1.1 + (p2.1 - p1.1) / l2 * rr,
        );
        out.push(a);
        for j in 1..6 {
            let t = j as f32 / 6.0;
            let u = 1.0 - t;
            out.push((
                u * u * a.0 + 2.0 * u * t * p1.0 + t * t * b.0,
                u * u * a.1 + 2.0 * u * t * p1.1 + t * t * b.1,
            ));
        }
        out.push(b);
    }
    out.push(pts[pts.len() - 1]);
    out
}

/// The prototype's `cubicBetween`: a cubic leaving `p` and arriving at `q`
/// along their side normals.
pub(super) fn cubic_between(p: Pt, ps: &str, q: Pt, qs: &str, kmin: f32) -> [Pt; 4] {
    let norm = |s: &str| match s {
        "top" => (0.0, -1.0),
        "bottom" => (0.0, 1.0),
        "left" => (-1.0, 0.0),
        _ => (1.0, 0.0),
    };
    let dist = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
    let k = kmin.max(150.0_f32.min(dist / 2.2));
    let (a, b) = (norm(ps), norm(qs));
    [
        p,
        (p.0 + a.0 * k, p.1 + a.1 * k),
        (q.0 + b.0 * k, q.1 + b.1 * k),
        q,
    ]
}

/// Distance from `p` to a polyline (window space).
pub(super) fn dist_to_path(p: Pos2, pts: &[Pos2]) -> f32 {
    let mut best = f32::MAX;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let l2 = ab.length_sq().max(1e-6);
        let t = (((p - a).dot(ab)) / l2).clamp(0.0, 1.0);
        best = best.min((a + ab * t - p).length());
    }
    best
}

/// RenderDOT's glyph for a link kind (composedot.go:28-33), drawn as
/// geometry in a small circle, as the prototype's `glyph`.
pub(super) fn glyph(d: &D, kind: &str, at: Pt, r: f32, scale: f32) {
    let col = kind_color(kind);
    let r = r * scale;
    d.disc(at, r, Color32::WHITE, col, 1.8);
    let k = r * 0.7;
    let l = |a: Pt, b: Pt| d.line((at.0 + a.0, at.1 + a.1), (at.0 + b.0, at.1 + b.1), col, 1.6);
    match kind {
        "event" => {
            l((-k, -k), (k, k));
            l((-k, k), (k, -k));
        }
        "guard" => l((-k, k), (k, -k)),
        "token" => {
            let q = r * 0.55;
            let pts = vec![
                d.pt((at.0, at.1 - q)),
                d.pt((at.0 + q, at.1)),
                d.pt((at.0, at.1 + q)),
                d.pt((at.0 - q, at.1)),
            ];
            d.p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
        }
        _ => {
            let pts = vec![
                d.pt((at.0 - r * 0.35, at.1 - r * 0.5)),
                d.pt((at.0 + r * 0.55, at.1)),
                d.pt((at.0 - r * 0.35, at.1 + r * 0.5)),
            ];
            d.p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
        }
    }
}

/// Greedy word wrap to `max_w` design units.
pub(super) fn wrap(d: &D, text: &str, max_w: f32, size: f32) -> Vec<String> {
    let mut lines = vec![];
    let mut cur = String::new();
    for w in text.split_whitespace() {
        let t = if cur.is_empty() {
            w.to_string()
        } else {
            format!("{cur} {w}")
        };
        if !cur.is_empty() && d.tw(&t, size) > max_w {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
        } else {
            cur = t;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// "1,234" (the prototype's `fmt`).
pub(super) fn fmt_n(n: f64) -> String {
    let s = format!("{}", n.round() as i64);
    let (neg, digits) = s
        .strip_prefix('-')
        .map_or((false, s.as_str()), |d| (true, d));
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if neg {
        format!("-{out}")
    } else {
        out
    }
}

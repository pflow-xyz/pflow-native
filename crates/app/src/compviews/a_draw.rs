//! Drawing helpers shared by the cospan, spiders and operad views: text in
//! design coordinates with a halo, sampled curves, polyline hit-testing,
//! a port drop-zone that leaves the drawing to the caller, and cite strings
//! pinned to a revision.
//!
//! Additive to `mod.rs`: nothing here changes an existing item.

use super::*;

/// The prototype's paper colour (`diagram-viewer.js` background).
pub(super) const PAPER: Color32 = Color32::from_rgb(0xfa, 0xfa, 0xfb);
/// Glyph colours of the prototype's cospan view.
pub(super) const LEG: Color32 = Color32::from_rgb(0x8e, 0x97, 0xa2);
pub(super) const ARC: Color32 = Color32::from_rgb(0x6d, 0x75, 0x80);
pub(super) const NODE: Color32 = Color32::from_rgb(0x3c, 0x46, 0x56);

/// The prototype's text-width estimate (`tw`): wide enough that layout
/// never underestimates a label, and deterministic (no font metrics).
pub(super) fn text_w(s: &str, size: f32) -> f32 {
    s.chars().count() as f32 * size * 0.58
}

pub(super) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(super) fn lerp2(a: Pos2, b: Pos2, t: f32) -> Pos2 {
    Pos2::new(lerp(a.x, b.x, t), lerp(a.y, b.y, t))
}

/// A quadratic Bezier, sampled (`qPts`).
pub(super) fn quad_pts(p0: Pos2, c: Pos2, p1: Pos2, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            Pos2::new(
                u * u * p0.x + 2.0 * u * t * c.x + t * t * p1.x,
                u * u * p0.y + 2.0 * u * t * c.y + t * t * p1.y,
            )
        })
        .collect()
}

/// A cubic Bezier, sampled (`cPts`).
pub(super) fn cubic_pts(p0: Pos2, c1: Pos2, c2: Pos2, p1: Pos2, n: usize) -> Vec<Pos2> {
    (0..=n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let u = 1.0 - t;
            Pos2::new(
                u * u * u * p0.x
                    + 3.0 * u * u * t * c1.x
                    + 3.0 * u * t * t * c2.x
                    + t * t * t * p1.x,
                u * u * u * p0.y
                    + 3.0 * u * u * t * c1.y
                    + 3.0 * u * t * t * c2.y
                    + t * t * t * p1.y,
            )
        })
        .collect()
}

/// A polyline with rounded corners (`roundedPath`), sampled.
pub(super) fn rounded_pts(pts: &[Pos2], r: f32) -> Vec<Pos2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut out = vec![pts[0]];
    for i in 1..pts.len() - 1 {
        let (p, c, n) = (pts[i - 1], pts[i], pts[i + 1]);
        let l1 = p.distance(c).max(1e-3);
        let l2 = c.distance(n).max(1e-3);
        let (k1, k2) = (r.min(l1 / 2.0), r.min(l2 / 2.0));
        let a = c - (c - p) / l1 * k1;
        let b = c + (n - c) / l2 * k2;
        out.extend(quad_pts(a, c, b, 6));
    }
    out.push(*pts.last().unwrap());
    out
}

/// Half a circle about `c`, top to bottom, on the right (`right`) or left.
pub(super) fn half_ring_pts(c: Pos2, r: f32, right: bool) -> Vec<Pos2> {
    (0..=14)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 14.0;
            Pos2::new(
                c.x + if right { 1.0 } else { -1.0 } * r * a.cos(),
                c.y + r * a.sin(),
            )
        })
        .collect()
}

/// Distance from `p` to the segment `a`-`b`.
pub(super) fn dist_seg(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let l2 = ab.length_sq();
    if l2 < 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / l2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// "go-pflow metamodel/compose.go:667-673 @69257b3 (note)": a cite from the
/// ontology's repos (the revision it records).
pub(super) fn cite_of(repo: &str, path: &str, from: u64, to: u64, note: &str) -> String {
    cite_label(&json!({"repo": repo, "path": path, "from": from, "to": to, "note": note}))
}

/// A pflow-rs cite pinned to the rev this app builds against (`aacb7eb`),
/// under `crates/pflow-metamodel/src/`.
pub(super) fn rs_cite(path: &str, from: u64, to: u64, note: &str) -> String {
    let lines = if to != 0 && to != from {
        format!(":{from}-{to}")
    } else {
        format!(":{from}")
    };
    let note = if note.is_empty() {
        String::new()
    } else {
        format!(" ({note})")
    };
    format!(
        "pflow-rs crates/pflow-metamodel/src/{path}{lines} @{}{note}",
        sp::PLAN_RS_REV
    )
}

/// What a port drop-zone found out while the drag was in progress.
pub(super) struct PortZone {
    /// The ring colour: the dragged port's own (ink), or the link kind a
    /// legal drop would make.
    pub ring: Option<Color32>,
    /// A port drag is in progress and this port is not a candidate.
    pub dim: bool,
}

impl<'a> Ctx<'a> {
    /// Text in design coordinates: size scaled by the transform (never
    /// below 7 px), with an optional halo in the background colour so it
    /// reads over lines (the prototype's `paint-order: stroke`).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn dt(
        &self,
        xf: Xf,
        at: Pos2,
        align: Align2,
        t: &str,
        size: f32,
        color: Color32,
        halo: Option<Color32>,
        mono: bool,
    ) -> Rect {
        let fs = (size * xf.k).max(7.0);
        let font = if mono {
            FontId::monospace(fs * 0.94)
        } else {
            FontId::proportional(fs)
        };
        let pos = xf.pt(at.x, at.y);
        if let Some(h) = halo {
            for d in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                self.p
                    .text(pos + Vec2::new(d.0, d.1) * 1.1, align, t, font.clone(), h);
            }
        }
        self.p.text(pos, align, t, font, color)
    }

    /// A polyline in screen space.
    pub(super) fn poly(&self, pts: &[Pos2], color: Color32, w: f32) {
        self.p.add(Shape::line(pts.to_vec(), Stroke::new(w, color)));
    }

    /// A dashed (or dotted) polyline in screen space.
    pub(super) fn dash_poly(&self, pts: &[Pos2], color: Color32, w: f32, dash: f32, gap: f32) {
        self.p
            .extend(Shape::dashed_line(pts, Stroke::new(w, color), dash, gap));
    }

    /// A filled arrow head at `to`, pointing away from `from`.
    pub(super) fn arrow_head(&self, from: Pos2, to: Pos2, size: f32, color: Color32) {
        let d = (to - from).normalized();
        if !d.x.is_finite() {
            return;
        }
        let n = Vec2::new(-d.y, d.x);
        let tip = to;
        let base = to - d * size;
        self.p.add(Shape::convex_polygon(
            vec![tip, base + n * size * 0.4, base - n * size * 0.4],
            color,
            Stroke::NONE,
        ));
    }

    /// Hover (and click-to-pin) when the pointer is inside `hit`.
    pub(super) fn hover_if(&mut self, hit: bool, i: Info) {
        if hit {
            if self.clicked {
                self.clicked_info = Some(i.clone());
            }
            self.hovered = Some(i);
        }
    }

    #[allow(dead_code)]
    /// Hover on a polyline: the pointer within `tol` px of any segment.
    pub(super) fn hover_poly(&mut self, pts: &[Pos2], tol: f32, i: Info) {
        let hit = self
            .pointer
            .is_some_and(|p| pts.windows(2).any(|w| dist_seg(p, w[0], w[1]) <= tol));
        self.hover_if(hit, i);
    }

    /// Is the pointer within `tol` px of the polyline?
    pub(super) fn near_poly(&self, pts: &[Pos2], tol: f32) -> bool {
        self.pointer
            .is_some_and(|p| pts.windows(2).any(|w| dist_seg(p, w[0], w[1]) <= tol))
    }

    pub(super) fn pointer_in(&self, r: Rect) -> bool {
        self.pointer.is_some_and(|p| r.contains(p))
    }

    pub(super) fn is_clicked_in(&self, r: Rect) -> bool {
        self.clicked && self.drag.is_none() && self.pointer_in(r)
    }

    /// A port's drag-and-drop zone, with the drawing left to the caller:
    /// registers the port as a drag source and as a drop target (lit only
    /// where `bundle.candidates` says the link is legal), and the inspector
    /// entry (the verdict while a port is dragged). `extra` adds the view's
    /// own facts to the resting entry. `hit` should be at least 12 px wide.
    pub(super) fn port_zone(
        &mut self,
        hit: Rect,
        subnet: &str,
        port: &Value,
        extra: impl FnOnce(Info) -> Info,
    ) -> PortZone {
        let id = s(&port["id"]);
        let key = format!("{subnet}:{id}");
        let trans = s(&port["target"]) == "transition";
        let mut ring: Option<Color32> = None;
        let mut verdict: Option<Value> = None;
        if let Some(Drag::Port {
            subnet: fs,
            port: fp,
            cands,
        }) = self.drag
        {
            if fs == subnet && fp == id {
                ring = Some(INK);
            } else if let Some(c) = cands
                .as_ref()
                .and_then(|c| arr(&c["candidates"]).iter().find(|x| s(&x["key"]) == key))
            {
                if c["ok"] == true {
                    ring = Some(kind_color(s(&c["link"]["kind"])));
                }
                verdict = Some(c.clone());
            }
        }
        let dim = self.drag.is_some_and(|d| matches!(d, Drag::Port { .. })) && ring.is_none();
        let pat = s(&port["pattern_port"]);
        let mut i = info(format!("port {key}"))
            .fact("kind", s(&port["kind"]))
            .fact("target", if trans { "transition" } else { "place" })
            .fact(
                "element",
                if trans {
                    s(&port["transition"])
                } else {
                    s(&port["place"])
                },
            )
            .fact("schema", s(&port["schema"]))
            .fact("ontology", pat);
        if let Some(pp) = self.port_concept(subnet, pat) {
            i = i.note(s(&pp["comment"])).cites_of(&pp["cites"]);
        }
        i = extra(i);
        if let Some(v) = &verdict {
            let ok = v["ok"] == true;
            i = info(if ok {
                format!("{key}: legal ({} link)", s(&v["link"]["kind"]))
            } else {
                format!("{key}: refused")
            });
            for e in arr(&v["verdict"]["errors"]) {
                i = i.fact(s(&e["code"]), s(&e["message"]));
                if s(&e["rule"]) == "page" {
                    i = i.cite("page rule (pflow-native composition.rs)");
                }
            }
            for p in arr(&v["verdict"]["passed"]) {
                i = i.fact("passed", s(&p["text"])).cite(cite_label(&p["cite"]));
            }
        }
        self.hover(hit, i);
        if self.drag_started && self.press_origin.is_some_and(|p| hit.contains(p)) {
            self.start_drag = Some(Drag::Port {
                subnet: subnet.into(),
                port: id.into(),
                cands: None,
            });
        }
        if self.released && self.pointer.is_some_and(|p| hit.contains(p)) {
            if let Some(v) = verdict.filter(|v| v["ok"] == true) {
                let l = &v["link"];
                self.command(
                    "bundle.add_link",
                    json!({"kind": l["kind"], "from": l["from"], "to": l["to"]}),
                );
            }
        }
        PortZone { ring, dim }
    }

    /// The ring a lit port wears: a pulsing stroke in the link kind's colour
    /// (the prototype's `snap-legal`), or the source's black ring.
    pub(super) fn zone_ring_circle(&self, at: Pos2, r: f32, z: &PortZone) {
        if let Some(col) = z.ring {
            let t = self.p.ctx().input(|i| i.time) as f32;
            let w = if col == INK {
                2.5
            } else {
                self.p.ctx().request_repaint();
                3.0 + 1.5 * (t * 7.0).sin()
            };
            self.p.circle_stroke(at, r, Stroke::new(w, col));
        }
    }

    pub(super) fn zone_ring_rect(&self, r: Rect, z: &PortZone) {
        if let Some(col) = z.ring {
            let t = self.p.ctx().input(|i| i.time) as f32;
            let w = if col == INK {
                2.5
            } else {
                self.p.ctx().request_repaint();
                3.0 + 1.5 * (t * 7.0).sin()
            };
            self.p
                .rect_stroke(r, 3.0, Stroke::new(w, col), StrokeKind::Middle);
        }
    }

    pub(super) fn zone_ring_pts(&self, pts: &[Pos2], z: &PortZone, w0: f32) {
        if let Some(col) = z.ring {
            self.p
                .add(Shape::line(pts.to_vec(), Stroke::new(w0 + 2.5, col)));
        }
    }

    /// A plan slot's drop zone with the drawing left to the caller: `Some(ok)`
    /// while a piece is dragged and `plan.slot_candidates` has a verdict for
    /// the slot (`ok` = legal: light it), the inspector entry, and the drop
    /// (`plan.add_piece`) on release over a legal slot.
    pub(super) fn slot_zone(&mut self, r: Rect, slot: &str) -> Option<bool> {
        let mut v: Option<Value> = None;
        if let Some(Drag::Piece { cands: Some(c), .. }) = self.drag {
            v = arr(&c["slots"])
                .iter()
                .find(|x| s(&x["slot"]) == slot)
                .cloned();
        }
        if self.pointer_in(r) {
            let mut i = info(format!("slot {slot}"));
            if let Some(sl) = sp::slot(slot) {
                i = i.fact("accepts", sl.accepts).fact(
                    "field",
                    sl.field
                        .unwrap_or("(designed in the ROADMAP, not a SearchPlan field)"),
                );
            }
            if let Some(x) = &v {
                let ok = x["ok"] == true;
                i = i.fact(
                    if ok { "drop" } else { "refused" },
                    s(&x["why"]).to_string() + if ok { "legal" } else { "" },
                );
            }
            self.hover_if(true, i);
        }
        if self.released && self.pointer_in(r) {
            if let (Some(Drag::Piece { name, .. }), Some(x)) = (self.drag, &v) {
                if x["ok"] == true {
                    let n = name.clone();
                    self.command("plan.add_piece", json!({"piece": n, "slot": slot}));
                }
            }
        }
        v.map(|x| x["ok"] == true)
    }
}

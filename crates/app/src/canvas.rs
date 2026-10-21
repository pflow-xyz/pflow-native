//! The net view: draws the document and marking from the session, and turns
//! pointer gestures into registry commands. In-flight gestures (a drag, an
//! arc being drawn, a box select) are the only state kept here, and a drag
//! commits a single `net.move_by` on release.

use eframe::egui::{
    self, Align2, Color32, FontId, PointerButton, Pos2, Rect, Sense, Shape, Stroke, StrokeKind,
    Vec2,
};
use pflow_native_core::export::{token_rgb, PLACE_R, TRANSITION_H, TRANSITION_W};
use pflow_native_core::{NodeKind, Session, Tool};
use serde_json::{json, Value};

use crate::App;

#[derive(Default)]
pub struct CanvasState {
    /// Nodes being dragged and the world-space offset so far.
    drag: Option<(Vec<String>, Vec2)>,
    panning: bool,
    /// Source node of an arc being drawn.
    arc_from: Option<String>,
    /// Box-select corner (world) while shift-dragging empty space.
    box_from: Option<Pos2>,
    /// Arc picked in the select tool (for the inspector).
    pub selected_arc: Option<(String, String)>,
    last_size: Vec2,
}

impl CanvasState {
    /// Escape: drop any half-made arc, drag or box select.
    pub fn cancel(&mut self) {
        self.arc_from = None;
        self.drag = None;
        self.box_from = None;
    }
}

struct Xf {
    center: Pos2,
    cx: f32,
    cy: f32,
    zoom: f32,
}

impl Xf {
    fn new(rect: Rect, s: &Session) -> Self {
        Xf {
            center: rect.center(),
            cx: s.view.cx as f32,
            cy: s.view.cy as f32,
            zoom: s.view.zoom as f32,
        }
    }
    fn to_screen(&self, x: f64, y: f64) -> Pos2 {
        self.center
            + Vec2::new(
                (x as f32 - self.cx) * self.zoom,
                (y as f32 - self.cy) * self.zoom,
            )
    }
    fn to_world(&self, p: Pos2) -> Pos2 {
        let d = (p - self.center) / self.zoom;
        Pos2::new(self.cx + d.x, self.cy + d.y)
    }
}

enum Hit {
    Node(String),
    Arc(String, String),
}

fn hit_test(s: &Session, w: Pos2, zoom: f32) -> Option<Hit> {
    // Nodes first, topmost (last drawn) wins.
    for t in s.doc.transitions.iter().rev() {
        if (w.x - t.x as f32).abs() <= TRANSITION_W as f32 / 2.0 + 2.0
            && (w.y - t.y as f32).abs() <= TRANSITION_H as f32 / 2.0 + 2.0
        {
            return Some(Hit::Node(t.id.clone()));
        }
    }
    for p in s.doc.places.iter().rev() {
        if w.distance(Pos2::new(p.x as f32, p.y as f32)) <= PLACE_R as f32 + 2.0 {
            return Some(Hit::Node(p.id.clone()));
        }
    }
    let tol = 6.0 / zoom;
    for a in &s.doc.arcs {
        let (Some(p0), Some(p1)) = (s.doc.position(&a.source), s.doc.position(&a.target)) else {
            continue;
        };
        let (a0, a1) = (
            Pos2::new(p0.0 as f32, p0.1 as f32),
            Pos2::new(p1.0 as f32, p1.1 as f32),
        );
        let bend = arc_bend(s, &a.source, &a.target);
        let mid = curve_mid(a0, a1, bend);
        if seg_dist(w, a0, mid) < tol || seg_dist(w, mid, a1) < tol {
            return Some(Hit::Arc(a.source.clone(), a.target.clone()));
        }
    }
    None
}

fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Two arcs between the same pair (one each way) are drawn as gentle
/// curves so both stay visible.
fn arc_bend(s: &Session, src: &str, dst: &str) -> f32 {
    if s.doc.arc_index(dst, src).is_some() {
        18.0
    } else {
        0.0
    }
}

fn curve_mid(a: Pos2, b: Pos2, bend: f32) -> Pos2 {
    let d = b - a;
    let n = Vec2::new(-d.y, d.x).normalized();
    a + d * 0.5 + n * bend
}

pub fn ui(app: &mut App, ui: &mut egui::Ui) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    if (rect.size() - app.canvas.last_size).length() > 0.5 {
        app.canvas.last_size = rect.size();
        let first = app.sess.view.width == 1000.0 && app.sess.view.height == 700.0;
        app.run(
            "view.resize",
            json!({ "width": rect.width(), "height": rect.height() }),
        );
        if first {
            app.run("view.fit", Value::Null);
        }
    }
    let dark = ui.visuals().dark_mode;
    let bg = if dark {
        Color32::from_gray(24)
    } else {
        Color32::from_gray(250)
    };
    painter.rect_filled(rect, 0.0, bg);

    input(app, ui, &resp, rect);
    draw(app, &painter, rect, dark, resp.hover_pos());
}

fn input(app: &mut App, ui: &egui::Ui, resp: &egui::Response, rect: Rect) {
    let xf = Xf::new(rect, &app.sess);
    let (shift, middle, space, scroll, zoom_delta, origin) = ui.input(|i| {
        (
            i.modifiers.shift,
            i.pointer.button_down(PointerButton::Middle),
            i.key_down(egui::Key::Space),
            i.smooth_scroll_delta(),
            i.zoom_delta(),
            i.pointer.press_origin(),
        )
    });

    // Wheel pans; pinch or Cmd/Ctrl+wheel zooms around the pointer.
    if resp.hovered() {
        if zoom_delta != 1.0 {
            if let Some(p) = resp.hover_pos() {
                let w = xf.to_world(p);
                app.run(
                    "view.zoom",
                    json!({ "factor": zoom_delta, "x": w.x, "y": w.y }),
                );
            }
        } else if scroll != Vec2::ZERO {
            app.run("view.pan", json!({ "dx": scroll.x, "dy": scroll.y }));
        }
    }

    let pos = resp.interact_pointer_pos().or(resp.hover_pos());
    let world = pos.map(|p| xf.to_world(p));
    let hit = world.and_then(|w| hit_test(&app.sess, w, xf.zoom));
    let tool = app.sess.tool;

    // Panning: middle button, space+drag, or dragging empty space in Select.
    if resp.drag_started() {
        // Hit-test where the press began, not where the drag was detected.
        let w0 = origin.map(|p| xf.to_world(p)).or(world);
        let hit = w0.and_then(|w| hit_test(&app.sess, w, xf.zoom));
        let empty = hit.is_none();
        if middle || space || (empty && tool != Tool::Arc && !shift) {
            app.canvas.panning = true;
        } else if empty && shift {
            app.canvas.box_from = w0;
        } else if let (Tool::Select, Some(Hit::Node(id))) = (tool, &hit) {
            if !app.sess.selection.contains(id) {
                app.run("edit.select", json!({ "ids": [id] }));
            }
            app.canvas.drag = Some((app.sess.selection.clone(), Vec2::ZERO));
        }
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        if app.canvas.panning {
            app.run("view.pan", json!({ "dx": d.x, "dy": d.y }));
        } else if let Some((_, off)) = &mut app.canvas.drag {
            *off += d / xf.zoom;
        }
    }
    if resp.drag_stopped() {
        app.canvas.panning = false;
        if let Some((ids, off)) = app.canvas.drag.take() {
            if off.length() > 0.5 {
                app.queue(
                    "net.move_by",
                    json!({ "ids": ids, "dx": off.x, "dy": off.y }),
                );
            }
        }
        if let (Some(a), Some(b)) = (app.canvas.box_from.take(), world) {
            let r = Rect::from_two_pos(a, b);
            let ids: Vec<String> = app
                .sess
                .doc
                .places
                .iter()
                .map(|p| (&p.id, p.x, p.y))
                .chain(app.sess.doc.transitions.iter().map(|t| (&t.id, t.x, t.y)))
                .filter(|(_, x, y)| r.contains(Pos2::new(*x as f32, *y as f32)))
                .map(|(id, _, _)| id.clone())
                .collect();
            app.queue("edit.select", json!({ "ids": ids }));
        }
    }

    let secondary = resp.secondary_clicked();
    if resp.clicked() || secondary {
        let w = world.unwrap_or_default();
        match (tool, &hit) {
            (Tool::Select, Some(Hit::Node(id))) => {
                app.canvas.selected_arc = None;
                app.queue("edit.select", json!({ "ids": [id], "add": shift }));
            }
            (Tool::Select, Some(Hit::Arc(a, b))) => {
                app.canvas.selected_arc = Some((a.clone(), b.clone()));
                if !app.sess.selection.is_empty() {
                    app.queue("edit.select_none", Value::Null);
                }
            }
            (Tool::Select, None) => {
                app.canvas.selected_arc = None;
                if !app.sess.selection.is_empty() {
                    app.queue("edit.select_none", Value::Null);
                }
            }
            (Tool::Place, None) => app.queue("net.add_place", json!({ "x": w.x, "y": w.y })),
            (Tool::Transition, None) => {
                app.queue("net.add_transition", json!({ "x": w.x, "y": w.y }))
            }
            (Tool::Arc, Some(Hit::Node(id))) => match app.canvas.arc_from.take() {
                None => app.canvas.arc_from = Some(id.clone()),
                Some(src) if src == *id => {}
                Some(src) => app.queue(
                    "net.add_arc",
                    json!({ "source": src, "target": id, "inhibit": secondary }),
                ),
            },
            (Tool::Arc, None) => app.canvas.arc_from = None,
            (Tool::Token, Some(Hit::Node(id))) if app.sess.doc.place(id).is_some() => {
                let delta = if secondary { -1 } else { 1 };
                app.queue("net.add_tokens", json!({ "id": id, "delta": delta }));
            }
            (Tool::Delete, Some(Hit::Node(id))) => app.queue("edit.delete", json!({ "ids": [id] })),
            (Tool::Delete, Some(Hit::Arc(a, b))) => {
                app.queue("net.delete_arc", json!({ "source": a, "target": b }))
            }
            (Tool::Fire, Some(Hit::Node(id))) if app.sess.doc.transition(id).is_some() => {
                app.queue("sim.fire", json!({ "id": id }))
            }
            _ => {}
        }
    }
    if resp.double_clicked() && tool == Tool::Select {
        if let Some(Hit::Node(id)) = &hit {
            if app.sess.doc.transition(id).is_some() {
                app.queue("sim.fire", json!({ "id": id }));
            } else {
                app.inspector.focus_label = true;
            }
        }
    }
}

fn draw(app: &mut App, painter: &egui::Painter, rect: Rect, dark: bool, hover: Option<Pos2>) {
    let enabled = app.sess.enabled_transitions();
    let s = &app.sess;
    let xf = Xf::new(rect, s);
    let z = xf.zoom;
    let fg = if dark {
        Color32::from_gray(220)
    } else {
        Color32::from_gray(40)
    };
    let muted = if dark {
        Color32::from_gray(150)
    } else {
        Color32::from_gray(90)
    };
    let node_bg = if dark {
        Color32::from_gray(40)
    } else {
        Color32::WHITE
    };
    let sel = Color32::from_rgb(59, 130, 246);
    let font = FontId::proportional((12.0 * z).clamp(8.0, 20.0));

    // Grid dots every 50 world units.
    if z > 0.35 {
        let step = 50.0;
        let w0 = xf.to_world(rect.min);
        let w1 = xf.to_world(rect.max);
        let grid = if dark {
            Color32::from_gray(45)
        } else {
            Color32::from_gray(222)
        };
        let mut gx = (w0.x / step).floor() * step;
        while gx <= w1.x {
            let mut gy = (w0.y / step).floor() * step;
            while gy <= w1.y {
                painter.circle_filled(xf.to_screen(gx as f64, gy as f64), 1.2, grid);
                gy += step;
            }
            gx += step;
        }
    }

    // Live drag offset.
    let offset = |id: &str| -> Vec2 {
        match &app.canvas.drag {
            Some((ids, off)) if ids.iter().any(|x| x == id) => *off,
            _ => Vec2::ZERO,
        }
    };
    let pos = |id: &str| -> Option<Pos2> {
        s.doc.position(id).map(|(x, y)| {
            let o = offset(id);
            xf.to_screen(x + o.x as f64, y + o.y as f64)
        })
    };

    for a in &s.doc.arcs {
        let (Some(p0), Some(p1)) = (pos(&a.source), pos(&a.target)) else {
            continue;
        };
        let picked =
            app.canvas.selected_arc.as_ref() == Some(&(a.source.clone(), a.target.clone()));
        // Color by the token colors the weight uses (single color: plain).
        let color = if picked {
            sel
        } else if s.doc.colors() > 1 {
            let i = a.weight.iter().position(|w| *w > 0.0).unwrap_or(0);
            let (r, g, b) = token_rgb(s.doc.token.get(i).map_or("", |t| t), i);
            Color32::from_rgb(r, g, b)
        } else {
            muted
        };
        let stroke = Stroke::new(if picked { 2.5 } else { 1.6 }, color);
        let bend = arc_bend(s, &a.source, &a.target) * z;
        let mid = curve_mid(p0, p1, bend);
        // Trim ends to the node outlines.
        let start = trim(p0, mid, node_radius(s, &a.source, mid - p0) * z);
        let end = trim(p1, mid, node_radius(s, &a.target, p1 - mid) * z);
        let pts = if bend == 0.0 {
            vec![start, end]
        } else {
            (0..=16)
                .map(|i| {
                    let t = i as f32 / 16.0;
                    let q0 = start.lerp(mid, t);
                    let q1 = mid.lerp(end, t);
                    q0.lerp(q1, t)
                })
                .collect()
        };
        let n = pts.len();
        let dir = (pts[n - 1] - pts[n - 2]).normalized();
        painter.add(Shape::line(pts.clone(), stroke));
        let tip = pts[n - 1];
        if a.inhibit {
            painter.circle(tip - dir * 4.5 * z, 4.5 * z, node_bg, stroke);
        } else {
            let back = tip - dir * 10.0 * z;
            let nrm = Vec2::new(-dir.y, dir.x) * 5.0 * z;
            painter.add(Shape::convex_polygon(
                vec![tip, back + nrm, back - nrm],
                color,
                Stroke::NONE,
            ));
        }
        let w = a.weight_sum();
        if w != 1.0 {
            let m = pts[n / 2];
            let txt = if w.fract() == 0.0 {
                format!("{}", w as i64)
            } else {
                format!("{w}")
            };
            let r = painter.text(m, Align2::CENTER_CENTER, &txt, font.clone(), fg);
            painter.rect_filled(r.expand(2.0), 3.0, node_bg.gamma_multiply(0.85));
            painter.text(m, Align2::CENTER_CENTER, txt, font.clone(), fg);
        }
    }

    // Arc being drawn.
    if let (Some(src), Some(h)) = (&app.canvas.arc_from, hover) {
        if let Some(p) = pos(src) {
            painter.line_segment([p, h], Stroke::new(1.5, sel));
        }
    }

    for p in &s.doc.places {
        let c = pos(&p.id).unwrap();
        let selected = s.selection.contains(&p.id);
        let tokens = s.marking.get(&p.id).cloned().unwrap_or_default();
        let total: i64 = tokens.iter().sum();
        let cap: f64 = p.capacity.iter().sum();
        let full = cap > 0.0 && total as f64 >= cap;
        let stroke = if selected {
            Stroke::new(3.0, sel)
        } else if full {
            Stroke::new(2.0, Color32::from_rgb(220, 60, 60))
        } else {
            Stroke::new(2.0, fg)
        };
        let r = PLACE_R as f32 * z;
        painter.circle(c, r, node_bg, stroke);
        draw_tokens(painter, c, r, &tokens, &s.doc.token, fg, z);
        let label = p.label.as_deref().unwrap_or(&p.id);
        painter.text(
            c - Vec2::new(0.0, r + 4.0),
            Align2::CENTER_BOTTOM,
            label,
            font.clone(),
            fg,
        );
    }

    for t in &s.doc.transitions {
        let c = pos(&t.id).unwrap();
        let selected = s.selection.contains(&t.id);
        let on = enabled.contains(&t.id);
        let fill = if on {
            Color32::from_rgb(98, 250, 117)
        } else if dark {
            Color32::from_gray(60)
        } else {
            Color32::from_gray(235)
        };
        let r = Rect::from_center_size(
            c,
            Vec2::new(TRANSITION_W as f32 * z, TRANSITION_H as f32 * z),
        );
        let stroke = if selected {
            Stroke::new(3.0, sel)
        } else {
            Stroke::new(2.0, fg)
        };
        painter.rect(r, 2.0 * z, fill, stroke, StrokeKind::Middle);
        let label = t.label.as_deref().unwrap_or(&t.id);
        painter.text(
            r.center_top() - Vec2::new(0.0, 4.0),
            Align2::CENTER_BOTTOM,
            label,
            font.clone(),
            fg,
        );
        if let Some(rate) = s.rates.get(&t.id) {
            painter.text(
                r.center_bottom() + Vec2::new(0.0, 3.0),
                Align2::CENTER_TOP,
                format!("k={rate}"),
                FontId::proportional((10.0 * z).clamp(7.0, 16.0)),
                muted,
            );
        }
    }

    // Box select rubber band.
    if let (Some(a), Some(h)) = (app.canvas.box_from, hover) {
        let r = Rect::from_two_pos(xf.to_screen(a.x as f64, a.y as f64), h);
        painter.rect(
            r,
            0.0,
            sel.gamma_multiply(0.08),
            Stroke::new(1.0, sel),
            StrokeKind::Middle,
        );
    }

    if s.doc.places.is_empty() && s.doc.transitions.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Empty net — press 2 and click to add a place, 3 for a transition, 4 to connect",
            FontId::proportional(15.0),
            muted,
        );
    }
}

fn node_radius(s: &Session, id: &str, dir: Vec2) -> f32 {
    match s.doc.kind_of(id) {
        Some(NodeKind::Place) => PLACE_R as f32,
        _ => {
            let d = dir.normalized();
            (TRANSITION_W as f32 / 2.0) / d.x.abs().max(d.y.abs()).max(1e-3)
        }
    }
}

fn trim(from: Pos2, toward: Pos2, r: f32) -> Pos2 {
    from + (toward - from).normalized() * r
}

fn draw_tokens(
    painter: &egui::Painter,
    c: Pos2,
    r: f32,
    tokens: &[i64],
    names: &[String],
    fg: Color32,
    z: f32,
) {
    let total: i64 = tokens.iter().sum();
    if tokens.len() > 1 {
        // Colored net: one count per non-empty color, in the color.
        let parts: Vec<(usize, i64)> = tokens
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, n)| *n > 0)
            .collect();
        let k = parts.len() as f32;
        for (j, (i, n)) in parts.iter().enumerate() {
            let (cr, cg, cb) = token_rgb(names.get(*i).map_or("", |t| t), *i);
            let dy = (j as f32 - (k - 1.0) / 2.0) * 13.0 * z;
            painter.text(
                c + Vec2::new(0.0, dy),
                Align2::CENTER_CENTER,
                n.to_string(),
                FontId::proportional((13.0 * z).clamp(8.0, 22.0)),
                Color32::from_rgb(cr, cg, cb),
            );
        }
        return;
    }
    match total {
        0 => {}
        1..=5 => {
            let dot = 3.5 * z;
            let offs: &[(f32, f32)] = match total {
                1 => &[(0.0, 0.0)],
                2 => &[(-0.35, 0.0), (0.35, 0.0)],
                3 => &[(0.0, -0.35), (-0.35, 0.3), (0.35, 0.3)],
                4 => &[(-0.3, -0.3), (0.3, -0.3), (-0.3, 0.3), (0.3, 0.3)],
                _ => &[
                    (-0.35, -0.35),
                    (0.35, -0.35),
                    (0.0, 0.0),
                    (-0.35, 0.35),
                    (0.35, 0.35),
                ],
            };
            for (dx, dy) in offs {
                painter.circle_filled(c + Vec2::new(dx * r, dy * r), dot, fg);
            }
        }
        n => {
            painter.text(
                c,
                Align2::CENTER_CENTER,
                n.to_string(),
                FontId::proportional((14.0 * z).clamp(8.0, 24.0)),
                fg,
            );
        }
    }
}

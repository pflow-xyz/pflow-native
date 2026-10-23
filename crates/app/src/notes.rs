//! Notes (`note.*`): sticky notes an agent (or the user) pins on a node, a
//! result, a canvas point or the net. The canvas draws node, point and net
//! notes; results and the Inspector show theirs as cards with a delete
//! button, which queues `note.delete` like every other action.

use eframe::egui::{self, Color32, FontId, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2};
use pflow_native_core::{Note, NoteTarget};
use serde_json::json;

use crate::App;

const FILL: Color32 = Color32::from_rgb(254, 240, 138);
const EDGE: Color32 = Color32::from_rgb(202, 138, 4);
const WRAP: f32 = 240.0;

fn label(n: &Note) -> String {
    if n.author == "agent" {
        format!("🗒 {}", n.text)
    } else {
        format!("🗒 {} — {}", n.text, n.author)
    }
}

/// One note as a card, with a button that deletes it.
pub fn card(app: &mut App, ui: &mut egui::Ui, n: &Note) {
    egui::Frame::new()
        .fill(FILL)
        .stroke(Stroke::new(1.0, EDGE))
        .corner_radius(4.0)
        .inner_margin(6.0)
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.add(
                    egui::Label::new(egui::RichText::new(label(n)).color(Color32::BLACK)).wrap(),
                );
                if ui
                    .add(
                        egui::Button::new(egui::RichText::new("×").color(Color32::BLACK).strong())
                            .fill(Color32::TRANSPARENT)
                            .small(),
                    )
                    .on_hover_text("delete this note")
                    .clicked()
                {
                    app.queue("note.delete", json!({ "id": n.id }));
                }
            });
        });
}

/// Cards for every note whose target satisfies `pick`.
pub fn cards(app: &mut App, ui: &mut egui::Ui, pick: impl Fn(&NoteTarget) -> bool) {
    let notes: Vec<Note> = app
        .sess
        .notes
        .iter()
        .filter(|n| pick(&n.target))
        .cloned()
        .collect();
    for n in &notes {
        card(app, ui, n);
    }
}

/// A sticky note on the canvas, above and to the right of `anchor`, with a
/// leader line back to it.
fn sticky(painter: &Painter, anchor: Pos2, offset: Vec2, text: String) {
    let g = painter.layout(text, FontId::proportional(12.0), Color32::BLACK, WRAP);
    let min = anchor + offset - Vec2::new(0.0, g.size().y);
    let r = Rect::from_min_size(min, g.size()).expand(6.0);
    painter.line_segment([anchor, r.left_bottom()], Stroke::new(1.0, EDGE));
    painter.rect(r, 4.0, FILL, Stroke::new(1.0, EDGE), StrokeKind::Middle);
    painter.galley(min, g, Color32::BLACK);
}

/// Draws node, point and net notes. `node` maps a node id to its screen
/// position; `point` maps world coordinates to the screen.
pub fn draw(
    painter: &Painter,
    rect: Rect,
    notes: &[Note],
    zoom: f32,
    node: impl Fn(&str) -> Option<Pos2>,
    point: impl Fn(f64, f64) -> Pos2,
) {
    let off = Vec2::new(18.0, -18.0) * zoom.clamp(0.5, 2.0);
    let mut corner = rect.right_top() + Vec2::new(-10.0, 10.0);
    for n in notes {
        match &n.target {
            NoteTarget::Node(id) => {
                if let Some(p) = node(id) {
                    sticky(painter, p, off, label(n));
                }
            }
            NoteTarget::Point { x, y } => sticky(painter, point(*x, *y), Vec2::ZERO, label(n)),
            NoteTarget::Net => {
                let g = painter.layout(label(n), FontId::proportional(12.0), Color32::BLACK, WRAP);
                let r =
                    Rect::from_min_size(corner - Vec2::new(g.size().x, 0.0), g.size()).expand(6.0);
                painter.rect(r, 4.0, FILL, Stroke::new(1.0, EDGE), StrokeKind::Middle);
                painter.galley(r.min + Vec2::splat(6.0), g, Color32::BLACK);
                corner.y = r.max.y + 8.0;
            }
            NoteTarget::Artifact(_) => {}
        }
    }
}

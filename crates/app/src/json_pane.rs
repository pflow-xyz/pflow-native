//! The JSON-LD source pane (pflow.xyz's editor pane): the document as text,
//! synced both ways. Canvas edits refresh the text unless the user is
//! mid-edit; a text edit that parses is applied as `file.load_json
//! {live: true}` after a short pause, so typing is not one undo step per
//! keystroke.

use eframe::egui;
use serde_json::{json, Value};

use crate::App;

const DEBOUNCE_S: f64 = 0.6;

#[derive(Default)]
pub struct JsonPaneState {
    text: String,
    /// Session revision the text was generated from.
    synced: Option<u64>,
    /// Time of the last keystroke not yet applied.
    pending_since: Option<f64>,
    error: Option<String>,
}

pub fn ui(app: &mut App, ui: &mut egui::Ui) {
    let now = ui.input(|i| i.time);
    let st = &mut app.json;
    if st.pending_since.is_none() && st.synced != Some(app.sess.revision) {
        st.text = app.sess.doc.to_json_pretty();
        st.synced = Some(app.sess.revision);
        st.error = None;
    }
    ui.horizontal(|ui| {
        ui.strong("JSON-LD");
        if ui
            .small_button("⟲ revert")
            .on_hover_text("discard text edits")
            .clicked()
        {
            app.json.pending_since = None;
            app.json.synced = None;
        }
        if ui.small_button("copy").clicked() {
            ui.ctx().copy_text(app.json.text.clone());
        }
        if ui.small_button("✖").on_hover_text("close (⌘J)").clicked() {
            app.queue("view.toggle_json", Value::Null);
        }
    });
    if let Some(e) = &app.json.error {
        ui.colored_label(egui::Color32::from_rgb(220, 60, 60), e);
    }
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let r = ui.add(
                egui::TextEdit::multiline(&mut app.json.text)
                    .code_editor()
                    .desired_width(f32::INFINITY)
                    .desired_rows(40),
            );
            if r.changed() {
                app.json.pending_since = Some(now);
            }
        });
    if let Some(t) = app.json.pending_since {
        if now - t >= DEBOUNCE_S {
            app.json.pending_since = None;
            match serde_json::from_str::<Value>(&app.json.text) {
                Ok(v) => {
                    if v != app.sess.doc.to_value() {
                        app.queue("file.load_json", json!({ "json": v, "live": true }));
                    }
                    app.json.error = None;
                }
                Err(e) => {
                    // Keep the user's text; show why it is not applied.
                    app.json.error = Some(format!("not applied: {e}"));
                    app.json.pending_since = None;
                    app.json.synced = Some(app.sess.revision);
                }
            }
        } else {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

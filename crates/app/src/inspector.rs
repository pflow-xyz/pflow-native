//! Properties of the selected place, transition or arc. Text fields buffer
//! edits and commit one `net.set_*` command on Enter / focus loss, so a
//! typed value is one undo step, not one per keystroke.

use eframe::egui;
use serde_json::{json, Value};

use crate::App;

#[derive(Default)]
pub struct InspectorState {
    /// (what the buffers belong to, revision they were loaded at)
    key: Option<(String, u64)>,
    label: String,
    initial: String,
    capacity: String,
    weight: String,
    id: String,
    pub focus_label: bool,
}

fn fmt_vec(v: &[f64]) -> String {
    v.iter()
        .map(|f| {
            if f.fract() == 0.0 {
                format!("{}", *f as i64)
            } else {
                f.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_vec(s: &str) -> Result<Vec<f64>, String> {
    s.split(',')
        .map(|p| {
            p.trim()
                .parse::<f64>()
                .map_err(|_| format!("{p:?} is not a number"))
        })
        .collect()
}

/// A single-line edit that reports `true` once, when the user commits.
fn commit_edit(ui: &mut egui::Ui, buf: &mut String, focus: bool) -> bool {
    let r = ui.add(egui::TextEdit::singleline(buf).desired_width(f32::INFINITY));
    if focus {
        r.request_focus();
    }
    r.lost_focus()
}

pub fn ui(app: &mut App, ui: &mut egui::Ui) {
    ui.heading("Inspector");
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        let sel = app.sess.selection.clone();
        let arc = app.canvas.selected_arc.clone();
        match (sel.as_slice(), arc) {
            ([id], _) if app.sess.doc.place(id).is_some() => place(app, ui, id),
            ([id], _) if app.sess.doc.transition(id).is_some() => transition(app, ui, id),
            ([], Some((a, b))) if app.sess.doc.arc_index(&a, &b).is_some() => {
                arc_ui(app, ui, &a, &b)
            }
            ([], _) => document(app, ui),
            (many, _) => {
                ui.label(format!("{} nodes selected", many.len()));
                if ui.button("Delete selection").clicked() {
                    app.queue("edit.delete", Value::Null);
                }
            }
        }
    });
}

fn sync(app: &mut App, key: &str, load: impl FnOnce(&mut InspectorState)) {
    let k = (key.to_string(), app.sess.revision);
    if app.inspector.key.as_ref() != Some(&k) {
        load(&mut app.inspector);
        app.inspector.key = Some(k);
    }
}

fn place(app: &mut App, ui: &mut egui::Ui, id: &str) {
    let p = app.sess.doc.place(id).unwrap().clone();
    sync(app, id, |st| {
        st.id = p.id.clone();
        st.label = p.label.clone().unwrap_or_default();
        st.initial = fmt_vec(&p.initial);
        st.capacity = fmt_vec(&p.capacity);
    });
    ui.strong("Place");
    id_field(app, ui, id);
    ui.label("Label");
    let focus = std::mem::take(&mut app.inspector.focus_label);
    if commit_edit(ui, &mut app.inspector.label, focus) {
        app.queue(
            "net.set_label",
            json!({ "id": id, "label": app.inspector.label }),
        );
    }
    let colors = app.sess.doc.colors();
    let hint = if colors > 1 {
        format!(" ({colors} colors, comma-separated)")
    } else {
        String::new()
    };
    ui.label(format!("Initial tokens{hint}"));
    if commit_edit(ui, &mut app.inspector.initial, false) {
        match parse_vec(&app.inspector.initial) {
            Ok(v) => app.queue("net.set_place", json!({ "id": id, "initial": v })),
            Err(e) => app.status_err(e),
        }
    }
    ui.label("Capacity (0 = unbounded)");
    if commit_edit(ui, &mut app.inspector.capacity, false) {
        match parse_vec(&app.inspector.capacity) {
            Ok(v) => app.queue("net.set_place", json!({ "id": id, "capacity": v })),
            Err(e) => app.status_err(e),
        }
    }
    ui.horizontal(|ui| {
        if ui.button("+1").clicked() {
            app.queue("net.add_tokens", json!({ "id": id, "delta": 1 }));
        }
        if ui.button("−1").clicked() {
            app.queue("net.add_tokens", json!({ "id": id, "delta": -1 }));
        }
    });
    ui.separator();
    let now = app.sess.marking.get(id).cloned().unwrap_or_default();
    ui.label(format!("Current marking: {now:?}"));
    ui.label(format!("Position: ({}, {})", p.x, p.y));
    if ui.button("Delete place").clicked() {
        app.queue("edit.delete", json!({ "ids": [id] }));
    }
}

fn transition(app: &mut App, ui: &mut egui::Ui, id: &str) {
    let t = app.sess.doc.transition(id).unwrap().clone();
    sync(app, id, |st| {
        st.id = t.id.clone();
        st.label = t.label.clone().unwrap_or_default();
    });
    ui.strong("Transition");
    id_field(app, ui, id);
    ui.label("Label");
    let focus = std::mem::take(&mut app.inspector.focus_label);
    if commit_edit(ui, &mut app.inspector.label, focus) {
        app.queue(
            "net.set_label",
            json!({ "id": id, "label": app.inspector.label }),
        );
    }
    ui.label("Rate (ODE, session-only)");
    let mut rate = app.sess.rates.get(id).copied().unwrap_or(1.0);
    let r = ui.add(
        egui::DragValue::new(&mut rate)
            .speed(0.01)
            .range(0.0..=f64::MAX),
    );
    if r.changed() {
        app.queue("sim.set_rate", json!({ "id": id, "rate": rate }));
    }
    ui.separator();
    let enabled = app.sess.enabled_transitions().contains(&id.to_string());
    ui.label(if enabled {
        "Enabled ✔"
    } else {
        "Not enabled"
    });
    if ui
        .add_enabled(enabled, egui::Button::new("▶ Fire"))
        .clicked()
    {
        app.queue("sim.fire", json!({ "id": id }));
    }
    ui.label(format!("Position: ({}, {})", t.x, t.y));
    if ui.button("Delete transition").clicked() {
        app.queue("edit.delete", json!({ "ids": [id] }));
    }
}

fn arc_ui(app: &mut App, ui: &mut egui::Ui, a: &str, b: &str) {
    let i = app.sess.doc.arc_index(a, b).unwrap();
    let arc = app.sess.doc.arcs[i].clone();
    sync(app, &format!("{a}->{b}"), |st| {
        st.weight = fmt_vec(&arc.weight)
    });
    ui.strong(format!("Arc  {a} → {b}"));
    ui.label("Weight");
    if commit_edit(ui, &mut app.inspector.weight, false) {
        match parse_vec(&app.inspector.weight) {
            Ok(v) => app.queue(
                "net.set_arc",
                json!({ "source": a, "target": b, "weight": v }),
            ),
            Err(e) => app.status_err(e),
        }
    }
    let input_side = app.sess.doc.place(a).is_some();
    let mut inh = arc.inhibit;
    let what = if input_side {
        "Inhibitor (blocks when tokens ≥ weight)"
    } else {
        "Read arc (requires tokens ≥ weight, moves none)"
    };
    if ui.checkbox(&mut inh, what).changed() {
        app.queue(
            "net.set_arc",
            json!({ "source": a, "target": b, "inhibit": inh }),
        );
    }
    if ui.button("Delete arc").clicked() {
        app.queue("net.delete_arc", json!({ "source": a, "target": b }));
        app.canvas.selected_arc = None;
    }
}

/// The node id, renamable (`net.rename`).
fn id_field(app: &mut App, ui: &mut egui::Ui, id: &str) {
    ui.label("Id");
    if commit_edit(ui, &mut app.inspector.id, false) && app.inspector.id != id {
        app.queue(
            "net.rename",
            json!({ "id": id, "new_id": app.inspector.id.trim() }),
        );
    }
}

fn document(app: &mut App, ui: &mut egui::Ui) {
    let d = &app.sess.doc;
    ui.strong("Document");
    if let Some(n) = d.extra.get("name").and_then(Value::as_str) {
        ui.label(n);
    }
    ui.label(format!(
        "{} places, {} transitions, {} arcs",
        d.places.len(),
        d.transitions.len(),
        d.arcs.len()
    ));
    if let Some(desc) = d.extra.get("description").and_then(Value::as_str) {
        ui.weak(desc);
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Token colors:");
        for (i, t) in d.token.iter().enumerate() {
            let (r, g, b) = pflow_native_core::export::token_rgb(t, i);
            ui.colored_label(
                egui::Color32::from_rgb(r, g, b),
                t.rsplit('/').next().unwrap_or(t),
            );
        }
        if d.token.is_empty() {
            ui.weak("(single)");
        }
    });
    ui.horizontal(|ui| {
        if ui.button("Details…").clicked() {
            app.invoke("net.set_details");
        }
        if ui.button("Colors…").clicked() {
            app.invoke("net.set_colors");
        }
    });
    if app.sess.registry().find("net.layout").is_some() {
        ui.label("Auto layout:");
        ui.horizontal_wrapped(|ui| {
            for name in pflow_native_core::layout::Algorithm::ALL.map(|a| a.name()) {
                if ui.button(name).clicked() {
                    app.queue("net.layout", json!({ "algorithm": name }));
                }
            }
        });
    }
    ui.separator();
    ui.label("Select a node or arc to edit it.");
    ui.label("Tip: every action is a registry command: ⌘K opens the palette; the Console tab runs any command with JSON args.");
}

impl App {
    pub(crate) fn status_err(&mut self, e: String) {
        self.set_status(Err(e));
    }
}

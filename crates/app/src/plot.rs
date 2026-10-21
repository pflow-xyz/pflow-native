//! The ODE quick controls shown above the results (t_end, solver, run).

use eframe::egui;
use serde_json::{json, Value};

use crate::App;

pub struct PlotState {
    pub t_end: f64,
    pub method: String,
    /// Artifact whose settings the controls were last synced from.
    seen: Option<String>,
}

impl Default for PlotState {
    fn default() -> Self {
        Self {
            t_end: 10.0,
            method: "tsit5".into(),
            seen: None,
        }
    }
}

impl PlotState {
    pub fn args(&self) -> Value {
        json!({ "t_end": self.t_end, "method": self.method })
    }
}

pub fn controls(app: &mut App, ui: &mut egui::Ui) {
    // Follow ODE runs made elsewhere (console, control port, macros).
    if let Some(a) = app
        .sess
        .latest("timeseries")
        .filter(|a| a.source.command == "sim.ode")
    {
        if app.plot.seen.as_deref() != Some(a.id.as_str()) {
            if let Some(t) = a.source.args.get("t_end").and_then(Value::as_f64) {
                app.plot.t_end = t;
            }
            if let Some(m) = a.source.args.get("method").and_then(Value::as_str) {
                app.plot.method = m.to_string();
            }
            app.plot.seen = Some(a.id.clone());
        }
    }
    ui.horizontal(|ui| {
        ui.label("ODE t_end");
        ui.add(
            egui::DragValue::new(&mut app.plot.t_end)
                .speed(0.5)
                .range(0.01..=1.0e6),
        );
        egui::ComboBox::from_id_salt("method")
            .selected_text(app.plot.method.clone())
            .show_ui(ui, |ui| {
                for m in pflow_native_core::sim::SOLVERS {
                    ui.selectable_value(&mut app.plot.method, m.to_string(), *m);
                }
            });
        if ui.button("▶ Run ODE").clicked() {
            let args = app.plot.args();
            app.queue("sim.ode", args);
        }
    });
}

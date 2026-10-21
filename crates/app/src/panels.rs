//! Bottom dock: Plot (time series), Run (token game), Console (journal,
//! command line). The console's command line runs any registry command
//! with JSON args, the same thing the CLI's `exec` does.

use eframe::egui;
use serde_json::{json, Value};

use crate::{results, App, BottomTab};

#[derive(Default)]
pub struct ConsoleState {
    line: String,
    pub model_name: String,
    log: Vec<(String, Result<String, String>)>,
    steps: u32,
    seed: u32,
}

impl ConsoleState {
    pub fn record(&mut self, id: &str, args: &Value, r: &pflow_native_core::Result<Value>) {
        // View plumbing would drown the log.
        if matches!(id, "view.pan" | "view.zoom" | "view.resize") {
            return;
        }
        let call = if args.is_null() {
            id.to_string()
        } else {
            format!("{id} {args}")
        };
        let out = match r {
            Ok(v) => {
                let s = v.to_string();
                Ok(if s.len() > 300 {
                    format!("{}…", &s[..s.floor_char_boundary(300)])
                } else {
                    s
                })
            }
            Err(e) => Err(e.to_string()),
        };
        self.log.push((call, out));
        if self.log.len() > 500 {
            let _ = self.log.remove(0);
        }
    }
}

pub fn bottom(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.bottom, BottomTab::Plot, "📈 Results");
        ui.selectable_value(&mut app.bottom, BottomTab::Workbench, "🔬 Workbench");
        ui.selectable_value(&mut app.bottom, BottomTab::Run, "▶ Run");
        ui.selectable_value(&mut app.bottom, BottomTab::Models, "🗂 Models");
        ui.selectable_value(&mut app.bottom, BottomTab::Console, "⌨ Console");
    });
    ui.separator();
    match app.bottom {
        BottomTab::Plot => results::ui(app, ui),
        BottomTab::Workbench => workbench(app, ui),
        BottomTab::Run => run(app, ui),
        BottomTab::Models => models(app, ui),
        BottomTab::Console => console(app, ui),
    }
}

fn run(app: &mut App, ui: &mut egui::Ui) {
    let enabled = app.sess.enabled_transitions();
    ui.horizontal(|ui| {
        ui.label(format!("Fired: {}", app.sess.fired));
        if ui.button("↺ Reset").clicked() {
            app.queue("sim.reset", Value::Null);
        }
        ui.separator();
        if app.console.steps == 0 {
            app.console.steps = 10;
            app.console.seed = 1;
        }
        ui.add(
            egui::DragValue::new(&mut app.console.steps)
                .range(1..=10_000)
                .prefix("steps "),
        );
        ui.add(egui::DragValue::new(&mut app.console.seed).prefix("seed "));
        if ui.button("Random run").clicked() {
            app.queue(
                "sim.run",
                json!({ "steps": app.console.steps, "seed": app.console.seed }),
            );
        }
    });
    ui.separator();
    if enabled.is_empty() {
        ui.label("No transition is enabled (deadlock).");
    } else {
        ui.label(format!("Fireable ({}):", enabled.len()));
    }
    egui::ScrollArea::vertical()
        .id_salt("fireable")
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for t in &enabled {
                    let label = app
                        .sess
                        .doc
                        .transition(t)
                        .and_then(|t| t.label.clone())
                        .unwrap_or_else(|| t.clone());
                    if ui.button(format!("▶ {label}")).clicked() {
                        app.queue("sim.fire", json!({ "id": t }));
                    }
                    if ui.small_button("⌖").on_hover_text("center on it").clicked() {
                        app.queue("view.center", json!({ "id": t }));
                    }
                }
            });
            ui.separator();
            egui::Grid::new("marking").striped(true).show(ui, |ui| {
                ui.strong("place");
                ui.strong("tokens");
                ui.strong("initial");
                ui.end_row();
                for p in &app.sess.doc.places {
                    ui.label(p.label.as_deref().unwrap_or(&p.id));
                    let now = app.sess.marking.get(&p.id).cloned().unwrap_or_default();
                    ui.label(fmt_tokens(&now));
                    let init: Vec<i64> = p.initial.iter().map(|f| *f as i64).collect();
                    ui.label(fmt_tokens(&init));
                    ui.end_row();
                }
            });
        });
}

fn fmt_tokens(v: &[i64]) -> String {
    if v.len() == 1 {
        v[0].to_string()
    } else {
        format!("{v:?}")
    }
}

fn console(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("›");
        let r = ui.add(
            egui::TextEdit::singleline(&mut app.console.line)
                .hint_text(r#"command.id {"json":"args"}   e.g. net.add_place {"x":100,"y":100}"#)
                .desired_width(f32::INFINITY)
                .font(egui::TextStyle::Monospace),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            let line = std::mem::take(&mut app.console.line);
            let line = line.trim();
            let (id, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            match if rest.trim().is_empty() {
                Ok(Value::Null)
            } else {
                serde_json::from_str(rest)
            } {
                Ok(args) => app.queue(id, args),
                Err(e) => app.status_err(format!("args are not JSON: {e}")),
            }
            r.request_focus();
        }
    });
    ui.separator();
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .id_salt("console")
        .show(ui, |ui| {
            for (call, out) in &app.console.log {
                ui.monospace(format!("› {call}"));
                match out {
                    Ok(s) => ui.monospace(s),
                    Err(e) => ui.colored_label(egui::Color32::from_rgb(220, 60, 60), e),
                };
            }
        });
}

/// Every simulation / analysis command, grouped by menu, as buttons. Each
/// opens its argument form, prefilled from the selection; results land in
/// the Results tab.
fn workbench(app: &mut App, ui: &mut egui::Ui) {
    let reg = app.sess.registry();
    let groups = [
        "Simulate",
        "Workbench",
        "Analyze",
        "Data",
        "Compose",
        "Systems",
        "Store",
        "Share",
        "Remote",
        "Provenance",
    ];
    egui::ScrollArea::vertical().show(ui, |ui| {
        for g in groups {
            let cmds: Vec<_> = reg.commands().iter().filter(|c| c.menu == g).collect();
            if cmds.is_empty() {
                continue;
            }
            ui.strong(g);
            ui.horizontal_wrapped(|ui| {
                for c in cmds {
                    let r = ui.button(c.label).on_hover_text(format!("{}\n{}", c.help, c.id));
                    if r.clicked() {
                        app.invoke(c.id);
                    }
                }
            });
            ui.add_space(6.0);
        }
        ui.weak("Tip: select a transition or place first — forms prefill transition/place arguments from the selection.");
    });
}

/// The model library: the nets kept beside the open one (systems bind
/// them, diffs compare them).
fn models(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("Save the open net as");
        ui.add(egui::TextEdit::singleline(&mut app.console.model_name).desired_width(160.0));
        let name = app.console.model_name.trim().to_string();
        let exists = app.sess.models.contains_key(&name);
        let label = if exists { "Overwrite" } else { "Save" };
        if ui
            .add_enabled(!name.is_empty(), egui::Button::new(label))
            .clicked()
        {
            app.queue("model.save", json!({ "name": name, "overwrite": exists }));
        }
        ui.separator();
        if ui.button("Import…").clicked() {
            app.invoke("model.import");
        }
    });
    ui.separator();
    if app.sess.models.is_empty() {
        ui.weak("The library is empty. Save the open net, or import a file or example.");
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("models").striped(true).show(ui, |ui| {
            for h in ["name", "places", "transitions", "arcs", ""] {
                ui.strong(h);
            }
            ui.end_row();
            let rows: Vec<(String, usize, usize, usize)> = app
                .sess
                .models
                .iter()
                .map(|(n, d)| (n.clone(), d.places.len(), d.transitions.len(), d.arcs.len()))
                .collect();
            for (name, p, t, a) in rows {
                ui.label(&name);
                ui.label(p.to_string());
                ui.label(t.to_string());
                ui.label(a.to_string());
                ui.horizontal(|ui| {
                    if ui.small_button("open").clicked() {
                        app.queue("model.open", json!({ "name": name }));
                    }
                    if ui.small_button("delete").clicked() {
                        app.queue("model.delete", json!({ "name": name }));
                    }
                });
                ui.end_row();
            }
        });
    });
}

//! The Results tab: every stored artifact in a list, the selected one in
//! the view for its kind, export in each format it supports. The newest
//! result is selected automatically, so running a command shows its output.

use std::collections::HashMap;

use eframe::egui;
use pflow_native_core::{Artifact, ArtifactData};
use serde_json::{json, Value};

use crate::views::{self, ViewState};
use crate::App;

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Chart,
    Phase,
    Table,
}

#[derive(Default)]
pub struct ResultsState {
    pub selected: Option<String>,
    /// Artifact ids already shown in the list.
    known: std::collections::HashSet<String>,
    mode: Mode,
    views: HashMap<String, ViewState>,
}

pub fn ui(app: &mut App, ui: &mut egui::Ui) {
    // Follow new results. A command that stores several (a chart plus its
    // table or report) gets its chart selected.
    let fresh: Vec<&Artifact> = app
        .sess
        .artifacts
        .iter()
        .filter(|a| !app.results.known.contains(&a.id))
        .collect();
    if !fresh.is_empty() {
        let pick = fresh
            .iter()
            .find(|a| {
                matches!(
                    a.data,
                    ArtifactData::TimeSeries { .. }
                        | ArtifactData::Heatmap { .. }
                        | ArtifactData::Scatter { .. }
                        | ArtifactData::Graph { .. }
                        | ArtifactData::Sets { .. }
                        | ArtifactData::Trace { .. }
                        | ArtifactData::Bars { .. }
                        | ArtifactData::Drawing { .. }
                )
            })
            .or(fresh.last())
            .map(|a| a.id.clone());
        app.results.known.extend(fresh.iter().map(|a| a.id.clone()));
        app.results.selected = pick;
        app.results.mode = Mode::Chart;
    }
    if app
        .results
        .selected
        .as_ref()
        .is_some_and(|id| app.sess.artifact(id).is_none())
    {
        app.results.selected = app.sess.artifacts.last().map(|a| a.id.clone());
    }

    crate::plot::controls(app, ui);
    ui.separator();
    if app.sess.artifacts.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label("No results yet. Run the ODE (⌘R), or anything in the Workbench tab.");
        });
        return;
    }
    egui::Panel::left("results-list")
        .resizable(true)
        .default_size(210.0)
        .show(ui, |ui| list(app, ui));
    let Some(art) = app
        .results
        .selected
        .as_ref()
        .and_then(|id| app.sess.artifact(id))
        .cloned()
    else {
        return;
    };
    egui::CentralPanel::default().show(ui, |ui| viewer(app, ui, &art));
}

fn list(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.strong(format!("Results ({})", app.sess.artifacts.len()));
        if ui.small_button("clear").clicked() {
            app.queue("artifact.clear", Value::Null);
        }
    });
    egui::ScrollArea::vertical().show(ui, |ui| {
        for a in app.sess.artifacts.iter().rev() {
            let sel = app.results.selected.as_deref() == Some(a.id.as_str());
            let text = format!("{}  {}", kind_icon(&a.data), a.title);
            let r = ui.selectable_label(sel, text).on_hover_text(format!(
                "{} · {} · from {}",
                a.id,
                a.data.kind(),
                a.source.command
            ));
            if r.clicked() {
                app.results.selected = Some(a.id.clone());
                app.results.mode = Mode::Chart;
            }
        }
    });
}

fn kind_icon(d: &ArtifactData) -> &'static str {
    match d {
        ArtifactData::TimeSeries { .. } => "📈",
        ArtifactData::Heatmap { .. } => "▦",
        ArtifactData::Matrix { .. } => "⊞",
        ArtifactData::Graph { .. } => "☍",
        ArtifactData::Trace { .. } => "⇢",
        ArtifactData::Table { .. } => "☰",
        ArtifactData::Scatter { .. } => "⁘",
        ArtifactData::Sets { .. } => "◎",
        ArtifactData::Bars { .. } => "▤",
        ArtifactData::Drawing { .. } => "✏",
        ArtifactData::Report(_) => "📄",
    }
}

fn viewer(app: &mut App, ui: &mut egui::Ui, art: &Artifact) {
    ui.horizontal(|ui| {
        ui.strong(&art.title);
        ui.weak(format!("{} · {}", art.id, art.source.command));
        if let ArtifactData::TimeSeries { .. } = art.data {
            ui.separator();
            ui.selectable_value(&mut app.results.mode, Mode::Chart, "Lines");
            ui.selectable_value(&mut app.results.mode, Mode::Phase, "Phase");
            ui.selectable_value(&mut app.results.mode, Mode::Table, "Δ Table");
        }
        ui.separator();
        for f in art.data.formats() {
            if ui
                .small_button(f.to_uppercase())
                .on_hover_text(format!("export {f}"))
                .clicked()
            {
                app.export_artifact_as(&art.id, f);
            }
        }
        if ui
            .small_button("delete")
            .on_hover_text("delete this result")
            .clicked()
        {
            app.queue("artifact.delete", json!({ "id": art.id }));
        }
    });
    let st = app.results.views.entry(art.id.clone()).or_default();
    match (&art.data, app.results.mode) {
        (ArtifactData::TimeSeries { series, .. }, Mode::Phase) => views::phase(ui, st, series),
        (ArtifactData::TimeSeries { series, .. }, Mode::Table) => {
            let columns = ["series", "initial", "final", "Δ", "min", "max"]
                .map(String::from)
                .to_vec();
            let rows: Vec<Vec<Value>> = series
                .iter()
                .map(|s| {
                    let name = match &s.group {
                        Some(g) => format!("{} · {g}", s.name),
                        None => s.name.clone(),
                    };
                    let first = s.values.first().copied().unwrap_or(0.0);
                    let last = s.values.last().copied().unwrap_or(0.0);
                    let (lo, hi) = s
                        .values
                        .iter()
                        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
                            (a.min(v), b.max(v))
                        });
                    vec![
                        json!(name),
                        json!(first),
                        json!(last),
                        json!(last - first),
                        json!(lo),
                        json!(hi),
                    ]
                })
                .collect();
            views::table(ui, &columns, &rows);
        }
        (ArtifactData::TimeSeries { x_label, x, series }, _) => {
            views::timeseries(ui, st, x_label, x, series)
        }
        (
            ArtifactData::Heatmap {
                x_label,
                y_label,
                xs,
                ys,
                z,
            },
            _,
        ) => views::heatmap(ui, x_label, y_label, xs, ys, z),
        (
            ArtifactData::Scatter {
                x_label,
                y_label,
                points,
                highlight,
            },
            _,
        ) => views::scatter(ui, x_label, y_label, points, highlight),
        (ArtifactData::Table { columns, rows }, _) => views::table(ui, columns, rows),
        (ArtifactData::Matrix { rows, cols, values }, _) => views::matrix(ui, rows, cols, values),
        (ArtifactData::Report(v), _) => views::report(ui, v),
        (ArtifactData::Sets { sets }, _) => {
            if let Some((cmd, args)) = views::sets(ui, st, &art.id, sets) {
                app.queue(cmd, args);
            }
        }
        (ArtifactData::Trace { steps }, _) => {
            if let Some((cmd, args)) = views::trace(ui, st, &art.id, steps) {
                app.queue(cmd, args);
            }
        }
        (ArtifactData::Graph { nodes, edges }, _) => {
            if let Some((cmd, args)) = views::graph(ui, st, &art.id, nodes, edges) {
                app.queue(cmd, args);
            }
        }
        (ArtifactData::Bars { value_label, bars }, _) => views::bars(ui, value_label, bars),
        (
            ArtifactData::Drawing {
                width,
                height,
                shapes,
            },
            _,
        ) => views::drawing(ui, *width, *height, shapes),
    }
}

//! The time-series view: renders the newest `timeseries` artifact (the ODE
//! today; SSA bands and scenario overlays later) with a hover readout and
//! clickable legend.

use std::collections::BTreeSet;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use pflow_native_core::export::PALETTE;
use pflow_native_core::{Artifact, ArtifactData};
use serde_json::{json, Value};

use crate::App;

pub struct PlotState {
    pub t_end: f64,
    pub method: String,
    hidden: BTreeSet<String>,
}

impl Default for PlotState {
    fn default() -> Self {
        Self {
            t_end: 10.0,
            method: "tsit5".into(),
            hidden: BTreeSet::new(),
        }
    }
}

impl PlotState {
    pub fn args(&self) -> Value {
        json!({ "t_end": self.t_end, "method": self.method })
    }
}

pub fn ui(app: &mut App, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label("t_end");
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
        ui.separator();
        if ui.button("CSV…").clicked() {
            app.export_artifact("csv");
        }
        if ui.button("SVG…").clicked() {
            app.export_artifact("svg");
        }
    });
    let Some(art) = app.sess.latest("timeseries").cloned() else {
        ui.centered_and_justified(|ui| {
            ui.label("No results yet. Run the ODE (Cmd+R) to integrate the mass-action dynamics from the current marking.");
        });
        return;
    };
    draw(&mut app.plot, ui, &art);
}

fn draw(st: &mut PlotState, ui: &mut egui::Ui, art: &Artifact) {
    let ArtifactData::TimeSeries { x_label, x, series } = &art.data else {
        return;
    };
    let dark = ui.visuals().dark_mode;
    let fg = if dark {
        Color32::from_gray(210)
    } else {
        Color32::from_gray(50)
    };
    let grid = if dark {
        Color32::from_gray(55)
    } else {
        Color32::from_gray(225)
    };
    let legend_w = 150.0;
    let avail = ui.available_size();
    let (resp, painter) = ui.allocate_painter(avail, Sense::hover());
    let full = resp.rect;
    let plot = Rect::from_min_max(
        full.min + Vec2::new(54.0, 22.0),
        full.max - Vec2::new(legend_w + 10.0, 26.0),
    );
    if plot.width() < 40.0 || plot.height() < 30.0 {
        return;
    }
    let visible: Vec<(usize, &pflow_native_core::artifact::Series)> = series
        .iter()
        .enumerate()
        .filter(|(_, s)| !st.hidden.contains(&s.name))
        .collect();
    let xmin = x.first().copied().unwrap_or(0.0);
    let xmax = x.last().copied().unwrap_or(1.0).max(xmin + 1e-9);
    let ymax = visible
        .iter()
        .flat_map(|(_, s)| s.values.iter().copied())
        .fold(0.0_f64, f64::max)
        .max(1e-9)
        * 1.05;
    let sx = |v: f64| plot.left() + ((v - xmin) / (xmax - xmin)) as f32 * plot.width();
    let sy = |v: f64| plot.bottom() - (v / ymax) as f32 * plot.height();
    let small = FontId::proportional(11.0);

    painter.text(
        full.left_top() + Vec2::new(54.0, 3.0),
        Align2::LEFT_TOP,
        &art.title,
        FontId::proportional(12.0),
        fg,
    );
    for i in 0..=4 {
        let fy = i as f64 / 4.0;
        let y = sy(ymax * fy);
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            Stroke::new(1.0, grid),
        );
        painter.text(
            Pos2::new(plot.left() - 4.0, y),
            Align2::RIGHT_CENTER,
            fmt(ymax * fy),
            small.clone(),
            fg,
        );
        let xv = xmin + (xmax - xmin) * fy;
        let px = sx(xv);
        painter.line_segment(
            [Pos2::new(px, plot.top()), Pos2::new(px, plot.bottom())],
            Stroke::new(1.0, grid),
        );
        painter.text(
            Pos2::new(px, plot.bottom() + 3.0),
            Align2::CENTER_TOP,
            fmt(xv),
            small.clone(),
            fg,
        );
    }
    painter.text(
        Pos2::new(plot.center().x, full.bottom() - 2.0),
        Align2::CENTER_BOTTOM,
        x_label,
        small.clone(),
        fg,
    );

    for (i, s) in &visible {
        let (r, g, b) = PALETTE[i % PALETTE.len()];
        let pts: Vec<Pos2> = x
            .iter()
            .zip(&s.values)
            .map(|(&a, &v)| Pos2::new(sx(a), sy(v)))
            .collect();
        painter.add(Shape::line(
            pts,
            Stroke::new(2.0, Color32::from_rgb(r, g, b)),
        ));
    }

    // Hover readout.
    if let Some(h) = resp.hover_pos().filter(|p| plot.contains(*p)) {
        let xv = xmin + ((h.x - plot.left()) / plot.width()) as f64 * (xmax - xmin);
        let idx = x
            .partition_point(|&a| a < xv)
            .min(x.len().saturating_sub(1));
        painter.line_segment(
            [
                Pos2::new(sx(x[idx]), plot.top()),
                Pos2::new(sx(x[idx]), plot.bottom()),
            ],
            Stroke::new(1.0, fg.gamma_multiply(0.5)),
        );
        let mut text = format!("{x_label} = {}", fmt(x[idx]));
        for (_, s) in &visible {
            text.push_str(&format!("\n{} = {}", s.name, fmt(s.values[idx])));
        }
        let anchor = if h.x > plot.center().x {
            Align2::RIGHT_TOP
        } else {
            Align2::LEFT_TOP
        };
        let galley_pos = h + Vec2::new(if h.x > plot.center().x { -10.0 } else { 10.0 }, 0.0);
        let r = painter.text(galley_pos, anchor, &text, small.clone(), fg);
        painter.rect_filled(
            r.expand(4.0),
            4.0,
            if dark {
                Color32::from_black_alpha(200)
            } else {
                Color32::from_white_alpha(230)
            },
        );
        painter.text(galley_pos, anchor, &text, small.clone(), fg);
    }

    // Legend: click to toggle a series.
    let mut ly = plot.top();
    let lx = plot.right() + 14.0;
    for (i, s) in series.iter().enumerate() {
        let (r, g, b) = PALETTE[i % PALETTE.len()];
        let hidden = st.hidden.contains(&s.name);
        let c = if hidden {
            Color32::from_gray(140)
        } else {
            Color32::from_rgb(r, g, b)
        };
        let row = Rect::from_min_size(Pos2::new(lx, ly), Vec2::new(legend_w - 8.0, 16.0));
        let resp = ui.interact(row, egui::Id::unique(("pflow-legend", i)), Sense::click());
        if resp.clicked() {
            if hidden {
                st.hidden.remove(&s.name);
            } else {
                st.hidden.insert(s.name.clone());
            }
        }
        painter.line_segment(
            [Pos2::new(lx, ly + 8.0), Pos2::new(lx + 16.0, ly + 8.0)],
            Stroke::new(3.0, c),
        );
        let last = s.values.last().copied().unwrap_or(0.0);
        painter.text(
            Pos2::new(lx + 22.0, ly + 8.0),
            Align2::LEFT_CENTER,
            format!("{}  {}", s.name, fmt(last)),
            small.clone(),
            if hidden { c } else { fg },
        );
        ly += 18.0;
        if ly > full.bottom() - 16.0 {
            break;
        }
    }
}

fn fmt(v: f64) -> String {
    let a = v.abs();
    if a == 0.0 {
        "0".into()
    } else if !(1e-3..1e5).contains(&a) {
        format!("{v:.2e}")
    } else if a >= 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.3}")
    }
}

//! Renderers for artifacts, one per kind (plus the phase plot, a second
//! way to look at a time series). Pure drawing: they read an artifact and
//! per-view UI state, and never change the session.

use std::collections::BTreeSet;

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2,
};
use pflow_native_core::artifact::Series;
use pflow_native_core::export::{heat_rgb, styles, PALETTE};
use serde_json::Value;

/// Per-artifact viewing state.
#[derive(Default)]
pub struct ViewState {
    pub hidden: BTreeSet<String>,
    pub phase_x: usize,
    pub phase_y: usize,
    /// Graph view: node positions (world) from the layered layout, pan, zoom.
    graph: Option<Vec<Pos2>>,
    pan: Vec2,
    zoom: f32,
    /// Trace view: current step and autoplay.
    pub step: usize,
    playing: bool,
    last_tick: f64,
    pub set: Option<usize>,
}

/// A command a view asks the app to run (views never change the session).
pub type Action = Option<(&'static str, Value)>;

/// A JSON number: integers as integers, the rest through [`fmt`].
pub fn fmt_json(n: &serde_json::Number) -> String {
    match (n.as_i64(), n.as_u64()) {
        (Some(i), _) => i.to_string(),
        (_, Some(u)) => u.to_string(),
        _ => n.as_f64().map(fmt).unwrap_or_else(|| n.to_string()),
    }
}

pub fn fmt(v: f64) -> String {
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

fn palette(i: usize) -> Color32 {
    let (r, g, b) = PALETTE[i % PALETTE.len()];
    Color32::from_rgb(r, g, b)
}

fn colors(ui: &egui::Ui) -> (Color32, Color32) {
    if ui.visuals().dark_mode {
        (Color32::from_gray(210), Color32::from_gray(55))
    } else {
        (Color32::from_gray(50), Color32::from_gray(225))
    }
}

struct Axes {
    plot: Rect,
    x: (f64, f64),
    y: (f64, f64),
}

impl Axes {
    fn sx(&self, v: f64) -> f32 {
        self.plot.left() + ((v - self.x.0) / (self.x.1 - self.x.0)) as f32 * self.plot.width()
    }
    fn sy(&self, v: f64) -> f32 {
        self.plot.bottom() - ((v - self.y.0) / (self.y.1 - self.y.0)) as f32 * self.plot.height()
    }
    fn to_point(&self, x: f64, y: f64) -> Pos2 {
        Pos2::new(self.sx(x), self.sy(y))
    }
    fn draw(&self, p: &egui::Painter, ui: &egui::Ui, x_label: &str, y_label: &str) {
        let (fg, grid) = colors(ui);
        let small = FontId::proportional(11.0);
        for i in 0..=4 {
            let f = i as f64 / 4.0;
            let yv = self.y.0 + (self.y.1 - self.y.0) * f;
            let y = self.sy(yv);
            p.line_segment(
                [
                    Pos2::new(self.plot.left(), y),
                    Pos2::new(self.plot.right(), y),
                ],
                Stroke::new(1.0, grid),
            );
            p.text(
                Pos2::new(self.plot.left() - 4.0, y),
                Align2::RIGHT_CENTER,
                fmt(yv),
                small.clone(),
                fg,
            );
            let xv = self.x.0 + (self.x.1 - self.x.0) * f;
            let x = self.sx(xv);
            p.line_segment(
                [
                    Pos2::new(x, self.plot.top()),
                    Pos2::new(x, self.plot.bottom()),
                ],
                Stroke::new(1.0, grid),
            );
            p.text(
                Pos2::new(x, self.plot.bottom() + 3.0),
                Align2::CENTER_TOP,
                fmt(xv),
                small.clone(),
                fg,
            );
        }
        p.text(
            Pos2::new(self.plot.center().x, self.plot.bottom() + 16.0),
            Align2::CENTER_TOP,
            x_label,
            small.clone(),
            fg,
        );
        if !y_label.is_empty() {
            p.text(
                Pos2::new(self.plot.left(), self.plot.top() - 4.0),
                Align2::LEFT_BOTTOM,
                y_label,
                small,
                fg,
            );
        }
    }
}

fn range(it: impl Iterator<Item = f64>) -> (f64, f64) {
    let (a, b) = it
        .filter(|v| v.is_finite())
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
            (a.min(v), b.max(v))
        });
    if !a.is_finite() {
        (0.0, 1.0)
    } else if b - a < 1e-12 {
        (a - 0.5, b + 0.5)
    } else {
        (a, b)
    }
}

fn dashed(painter: &egui::Painter, pts: &[Pos2], stroke: Stroke, dash: usize) {
    match dash % 4 {
        0 => {
            painter.add(Shape::line(pts.to_vec(), stroke));
        }
        k => {
            let (d, g) = [(0.0, 0.0), (7.0, 4.0), (2.0, 3.0), (12.0, 4.0)][k];
            painter.extend(Shape::dashed_line(pts, stroke, d, g));
        }
    }
}

/// Clickable legend rows on the right edge; returns its width.
fn legend(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    at: Pos2,
    rows: &[(String, Color32, usize, String)],
    st: &mut ViewState,
    bottom: f32,
) {
    let (fg, _) = colors(ui);
    let small = FontId::proportional(11.0);
    let mut y = at.y;
    for (i, (key, c, dash, text)) in rows.iter().enumerate() {
        let hidden = st.hidden.contains(key);
        let row = Rect::from_min_size(Pos2::new(at.x, y), Vec2::new(150.0, 16.0));
        if ui
            .interact(row, egui::Id::unique(("legend", key, i)), Sense::click())
            .clicked()
        {
            if hidden {
                st.hidden.remove(key);
            } else {
                st.hidden.insert(key.clone());
            }
        }
        let c = if hidden { Color32::from_gray(140) } else { *c };
        dashed(
            painter,
            &[Pos2::new(at.x, y + 8.0), Pos2::new(at.x + 18.0, y + 8.0)],
            Stroke::new(3.0, c),
            *dash,
        );
        painter.text(
            Pos2::new(at.x + 24.0, y + 8.0),
            Align2::LEFT_CENTER,
            text,
            small.clone(),
            if hidden { c } else { fg },
        );
        y += 17.0;
        if y > bottom - 14.0 {
            break;
        }
    }
}

const LEGEND_W: f32 = 170.0;

fn plot_area(ui: &mut egui::Ui, legend: bool) -> (egui::Response, egui::Painter, Rect) {
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
    let full = resp.rect;
    let plot = Rect::from_min_max(
        full.min + Vec2::new(58.0, 18.0),
        full.max - Vec2::new(if legend { LEGEND_W } else { 16.0 }, 34.0),
    );
    (resp, painter, plot)
}

pub fn timeseries(
    ui: &mut egui::Ui,
    st: &mut ViewState,
    x_label: &str,
    x: &[f64],
    series: &[Series],
) {
    let (resp, painter, plot) = plot_area(ui, true);
    if plot.width() < 40.0 || plot.height() < 30.0 || x.is_empty() {
        return;
    }
    let (fg, _) = colors(ui);
    let sty = styles(series);
    let key = |s: &Series| match &s.group {
        Some(g) => format!("{}·{}", s.name, g),
        None => s.name.clone(),
    };
    let visible: Vec<usize> = (0..series.len())
        .filter(|&k| !st.hidden.contains(&key(&series[k])))
        .collect();
    let yr = range(visible.iter().flat_map(|&k| {
        let s = &series[k];
        s.values.iter().enumerate().flat_map(move |(i, &v)| {
            let b = s.band.as_ref().map_or(0.0, |b| b[i]);
            [v - b, v + b]
        })
    }));
    // Counts never go negative: a ± band must not drag the axis below 0.
    let nonneg = visible
        .iter()
        .all(|&k| series[k].values.iter().all(|v| *v >= 0.0));
    let lo = if nonneg { 0.0 } else { yr.0.min(0.0) };
    let yr = (lo, yr.1 * 1.04);
    let ax = Axes {
        plot,
        x: range(x.iter().copied()),
        y: yr,
    };
    ax.draw(&painter, ui, x_label, "");
    for &k in &visible {
        let s = &series[k];
        let c = palette(sty[k].0);
        if let Some(band) = &s.band {
            // Band as vertical quads between consecutive samples.
            for i in 1..x.len() {
                let quad = vec![
                    ax.to_point(x[i - 1], (s.values[i - 1] - band[i - 1]).max(ax.y.0)),
                    ax.to_point(x[i], (s.values[i] - band[i]).max(ax.y.0)),
                    ax.to_point(x[i], s.values[i] + band[i]),
                    ax.to_point(x[i - 1], s.values[i - 1] + band[i - 1]),
                ];
                painter.add(Shape::convex_polygon(
                    quad,
                    c.gamma_multiply(0.18),
                    Stroke::NONE,
                ));
            }
        }
        let pts: Vec<Pos2> = x
            .iter()
            .zip(&s.values)
            .map(|(&a, &v)| ax.to_point(a, v))
            .collect();
        dashed(&painter, &pts, Stroke::new(2.0, c), sty[k].1);
    }
    if let Some(h) = resp.hover_pos().filter(|p| plot.contains(*p)) {
        let xv = ax.x.0 + ((h.x - plot.left()) / plot.width()) as f64 * (ax.x.1 - ax.x.0);
        let idx = x.partition_point(|&a| a < xv).min(x.len() - 1);
        painter.line_segment(
            [
                Pos2::new(ax.sx(x[idx]), plot.top()),
                Pos2::new(ax.sx(x[idx]), plot.bottom()),
            ],
            Stroke::new(1.0, fg.gamma_multiply(0.5)),
        );
        let mut text = format!("{x_label} = {}", fmt(x[idx]));
        for &k in visible.iter().take(16) {
            let s = &series[k];
            let band = s
                .band
                .as_ref()
                .map(|b| format!(" ± {}", fmt(b[idx])))
                .unwrap_or_default();
            text.push_str(&format!("\n{} = {}{band}", key(s), fmt(s.values[idx])));
        }
        tooltip(&painter, ui, h, plot, &text);
    }
    let rows: Vec<(String, Color32, usize, String)> = series
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let last = s.values.last().copied().unwrap_or(0.0);
            (
                key(s),
                palette(sty[k].0),
                sty[k].1,
                format!("{}  {}", key(s), fmt(last)),
            )
        })
        .collect();
    legend(
        ui,
        &painter,
        Pos2::new(plot.right() + 12.0, plot.top()),
        &rows,
        st,
        plot.bottom() + 30.0,
    );
}

fn tooltip(painter: &egui::Painter, ui: &egui::Ui, h: Pos2, plot: Rect, text: &str) {
    let (fg, _) = colors(ui);
    let small = FontId::proportional(11.0);
    let right = h.x > plot.center().x;
    let anchor = if right {
        Align2::RIGHT_TOP
    } else {
        Align2::LEFT_TOP
    };
    let pos = h + Vec2::new(if right { -10.0 } else { 10.0 }, 0.0);
    let r = painter.text(pos, anchor, text, small.clone(), fg);
    let bg = if ui.visuals().dark_mode {
        Color32::from_black_alpha(210)
    } else {
        Color32::from_white_alpha(235)
    };
    painter.rect_filled(r.expand(4.0), 4.0, bg);
    painter.text(pos, anchor, text, small, fg);
}

/// Two series of a time series against each other (pflow.xyz Phase Plot).
pub fn phase(ui: &mut egui::Ui, st: &mut ViewState, series: &[Series]) {
    if series.len() < 2 {
        ui.label("A phase plot needs at least two series.");
        return;
    }
    let names: Vec<String> = series
        .iter()
        .map(|s| match &s.group {
            Some(g) => format!("{}·{}", s.name, g),
            None => s.name.clone(),
        })
        .collect();
    st.phase_x = st.phase_x.min(series.len() - 1);
    st.phase_y = if st.phase_y == st.phase_x || st.phase_y >= series.len() {
        (st.phase_x + 1) % series.len()
    } else {
        st.phase_y
    };
    ui.horizontal(|ui| {
        egui::ComboBox::from_label("x")
            .selected_text(&names[st.phase_x])
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut st.phase_x, i, n);
                }
            });
        egui::ComboBox::from_label("y")
            .selected_text(&names[st.phase_y])
            .show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut st.phase_y, i, n);
                }
            });
    });
    let (sx, sy) = (&series[st.phase_x], &series[st.phase_y]);
    let (resp, painter, plot) = plot_area(ui, false);
    if plot.width() < 40.0 || plot.height() < 30.0 {
        return;
    }
    let ax = Axes {
        plot,
        x: range(sx.values.iter().copied()),
        y: range(sy.values.iter().copied()),
    };
    ax.draw(&painter, ui, &names[st.phase_x], &names[st.phase_y]);
    let pts: Vec<Pos2> = sx
        .values
        .iter()
        .zip(&sy.values)
        .map(|(&a, &b)| ax.to_point(a, b))
        .collect();
    let n = pts.len();
    // Fade from start to end so direction reads.
    for i in 1..n {
        let t = i as f32 / n as f32;
        let c = palette(0).gamma_multiply(0.35 + 0.65 * t);
        painter.line_segment([pts[i - 1], pts[i]], Stroke::new(2.0, c));
    }
    if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
        painter.circle_filled(*a, 4.0, palette(2));
        painter.circle_filled(*b, 4.0, palette(1));
    }
    if let Some(h) = resp.hover_pos().filter(|p| plot.contains(*p)) {
        if let Some((i, _)) = pts
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.distance(h).total_cmp(&b.1.distance(h)))
        {
            let text = format!(
                "{} = {}\n{} = {}",
                names[st.phase_x],
                fmt(sx.values[i]),
                names[st.phase_y],
                fmt(sy.values[i])
            );
            tooltip(&painter, ui, h, plot, &text);
        }
    }
}

pub fn heatmap(
    ui: &mut egui::Ui,
    x_label: &str,
    y_label: &str,
    xs: &[f64],
    ys: &[f64],
    z: &[Vec<f64>],
) {
    let (resp, painter, plot) = plot_area(ui, true);
    if plot.width() < 40.0 || plot.height() < 30.0 || xs.is_empty() || ys.is_empty() {
        return;
    }
    let (fg, _) = colors(ui);
    let (zmin, zmax) = range(z.iter().flatten().copied());
    let (nx, ny) = (xs.len(), ys.len());
    let (cw, ch) = (plot.width() / nx as f32, plot.height() / ny as f32);
    for (iy, row) in z.iter().enumerate() {
        for (ix, &v) in row.iter().enumerate() {
            let (r, g, b) = heat_rgb((v - zmin) / (zmax - zmin));
            let cell = Rect::from_min_size(
                Pos2::new(
                    plot.left() + ix as f32 * cw,
                    plot.bottom() - (iy + 1) as f32 * ch,
                ),
                Vec2::new(cw + 0.5, ch + 0.5),
            );
            painter.rect_filled(cell, 0.0, Color32::from_rgb(r, g, b));
        }
    }
    // Axes over cell centers.
    let small = FontId::proportional(11.0);
    for (k, idx) in [0, nx / 2, nx - 1].into_iter().enumerate() {
        let x = plot.left() + (idx as f32 + 0.5) * cw;
        let align = [Align2::LEFT_TOP, Align2::CENTER_TOP, Align2::RIGHT_TOP][k];
        painter.text(
            Pos2::new(x, plot.bottom() + 3.0),
            align,
            fmt(xs[idx]),
            small.clone(),
            fg,
        );
    }
    for idx in [0, ny / 2, ny - 1] {
        let y = plot.bottom() - (idx as f32 + 0.5) * ch;
        painter.text(
            Pos2::new(plot.left() - 4.0, y),
            Align2::RIGHT_CENTER,
            fmt(ys[idx]),
            small.clone(),
            fg,
        );
    }
    painter.text(
        Pos2::new(plot.center().x, plot.bottom() + 16.0),
        Align2::CENTER_TOP,
        x_label,
        small.clone(),
        fg,
    );
    painter.text(
        Pos2::new(plot.left(), plot.top() - 4.0),
        Align2::LEFT_BOTTOM,
        y_label,
        small.clone(),
        fg,
    );
    // Color bar.
    let bar = Rect::from_min_size(
        Pos2::new(plot.right() + 14.0, plot.top()),
        Vec2::new(14.0, plot.height()),
    );
    for i in 0..40 {
        let t = i as f64 / 39.0;
        let (r, g, b) = heat_rgb(t);
        let h = bar.height() / 40.0;
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(bar.left(), bar.bottom() - (i + 1) as f32 * h),
                Vec2::new(bar.width(), h + 0.5),
            ),
            0.0,
            Color32::from_rgb(r, g, b),
        );
    }
    painter.text(
        bar.right_top() + Vec2::new(4.0, 0.0),
        Align2::LEFT_TOP,
        fmt(zmax),
        small.clone(),
        fg,
    );
    painter.text(
        bar.right_bottom() + Vec2::new(4.0, 0.0),
        Align2::LEFT_BOTTOM,
        fmt(zmin),
        small.clone(),
        fg,
    );
    if let Some(h) = resp.hover_pos().filter(|p| plot.contains(*p)) {
        let ix = (((h.x - plot.left()) / cw) as usize).min(nx - 1);
        let iy = (((plot.bottom() - h.y) / ch) as usize).min(ny - 1);
        let cell = Rect::from_min_size(
            Pos2::new(
                plot.left() + ix as f32 * cw,
                plot.bottom() - (iy + 1) as f32 * ch,
            ),
            Vec2::new(cw, ch),
        );
        painter.rect_stroke(
            cell,
            0.0,
            Stroke::new(1.5, Color32::WHITE),
            StrokeKind::Middle,
        );
        let text = format!(
            "{x_label} = {}\n{y_label} = {}\nvalue = {}",
            fmt(xs[ix]),
            fmt(ys[iy]),
            fmt(z[iy][ix])
        );
        tooltip(&painter, ui, h, plot, &text);
    }
}

pub fn scatter(
    ui: &mut egui::Ui,
    x_label: &str,
    y_label: &str,
    points: &[(f64, f64)],
    highlight: &[usize],
) {
    let (resp, painter, plot) = plot_area(ui, false);
    if plot.width() < 40.0 || plot.height() < 30.0 {
        return;
    }
    let ax = Axes {
        plot,
        x: range(points.iter().map(|p| p.0)),
        y: range(points.iter().map(|p| p.1)),
    };
    ax.draw(&painter, ui, x_label, y_label);
    for (i, &(x, y)) in points.iter().enumerate() {
        if !highlight.contains(&i) {
            painter.circle_filled(ax.to_point(x, y), 2.5, Color32::from_gray(150));
        }
    }
    let mut front: Vec<(f64, f64)> = highlight
        .iter()
        .filter_map(|&i| points.get(i).copied())
        .collect();
    front.sort_by(|a, b| a.0.total_cmp(&b.0));
    let red = palette(1);
    let pts: Vec<Pos2> = front.iter().map(|&(x, y)| ax.to_point(x, y)).collect();
    if pts.len() > 1 {
        painter.add(Shape::line(pts.clone(), Stroke::new(1.5, red)));
    }
    for p in &pts {
        painter.circle_filled(*p, 4.0, red);
    }
    if let Some(h) = resp.hover_pos().filter(|p| plot.contains(*p)) {
        if let Some((i, p)) = points.iter().enumerate().min_by(|a, b| {
            ax.to_point(a.1 .0, a.1 .1)
                .distance(h)
                .total_cmp(&ax.to_point(b.1 .0, b.1 .1).distance(h))
        }) {
            let star = if highlight.contains(&i) {
                " (Pareto)"
            } else {
                ""
            };
            let text = format!(
                "#{i}{star}\n{x_label} = {}\n{y_label} = {}",
                fmt(p.0),
                fmt(p.1)
            );
            tooltip(&painter, ui, h, plot, &text);
        }
    }
}

pub fn table(ui: &mut egui::Ui, columns: &[String], rows: &[Vec<Value>]) {
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("artifact-table")
                .striped(true)
                .show(ui, |ui| {
                    for c in columns {
                        ui.strong(c);
                    }
                    ui.end_row();
                    for row in rows.iter().take(2000) {
                        for v in row {
                            match v {
                                Value::Number(n) => ui.monospace(fmt_json(n)),
                                Value::String(s) => ui.label(s),
                                Value::Bool(b) => ui.label(if *b { "✔" } else { "" }),
                                Value::Null => ui.weak("—"),
                                other => ui.monospace(other.to_string()),
                            };
                        }
                        ui.end_row();
                    }
                });
            if rows.len() > 2000 {
                ui.weak(format!(
                    "… {} more rows (export CSV for all)",
                    rows.len() - 2000
                ));
            }
        });
}

/// A matrix as a table with row labels.
pub fn matrix(ui: &mut egui::Ui, rows: &[String], cols: &[String], values: &[Vec<f64>]) {
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("artifact-matrix")
                .striped(true)
                .show(ui, |ui| {
                    ui.label("");
                    for c in cols {
                        ui.strong(c);
                    }
                    ui.end_row();
                    for (r, row) in rows.iter().zip(values) {
                        ui.strong(r);
                        for v in row {
                            if *v == 0.0 {
                                ui.weak("·");
                            } else {
                                ui.monospace(fmt(*v));
                            }
                        }
                        ui.end_row();
                    }
                });
        });
}

pub fn report(ui: &mut egui::Ui, v: &Value) {
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            json_tree(ui, None, v, 0);
        });
}

fn json_tree(ui: &mut egui::Ui, key: Option<&str>, v: &Value, depth: usize) {
    let label = |k: Option<&str>| k.map(|k| format!("{k}: ")).unwrap_or_default();
    match v {
        Value::Object(m) => {
            let head = format!("{}{{{}}}", label(key), m.len());
            egui::CollapsingHeader::new(head)
                .id_salt((key, depth, m.len()))
                .default_open(depth < 2)
                .show(ui, |ui| {
                    for (k, v) in m {
                        json_tree(ui, Some(k), v, depth + 1);
                    }
                });
        }
        Value::Array(a) => {
            let head = format!("{}[{}]", label(key), a.len());
            egui::CollapsingHeader::new(head)
                .id_salt((key, depth, a.len()))
                .default_open(depth < 2 && a.len() <= 20)
                .show(ui, |ui| {
                    for (i, v) in a.iter().enumerate() {
                        json_tree(ui, Some(&i.to_string()), v, depth + 1);
                    }
                });
        }
        Value::Number(n) => {
            ui.monospace(format!("{}{}", label(key), fmt_json(n)));
        }
        other => {
            ui.monospace(format!("{}{}", label(key), other));
        }
    }
}

// ---------------------------------------------------------------- sets

pub fn sets(
    ui: &mut egui::Ui,
    st: &mut ViewState,
    id: &str,
    sets: &[pflow_native_core::artifact::NodeSet],
) -> Action {
    let mut act = None;
    ui.horizontal(|ui| {
        ui.weak("Click a set to highlight it on the net.");
        if ui.small_button("clear highlight").clicked() {
            st.set = None;
            act = Some(("view.clear_highlight", Value::Null));
        }
    });
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (i, set) in sets.iter().enumerate() {
                let sel = st.set == Some(i);
                let r = ui.selectable_label(sel, egui::RichText::new(&set.label).strong());
                if r.clicked() {
                    st.set = Some(i);
                    act = Some((
                        "artifact.show_set",
                        serde_json::json!({ "id": id, "index": i }),
                    ));
                }
                ui.horizontal_wrapped(|ui| {
                    ui.add_space(12.0);
                    for n in &set.nodes {
                        match set.weights.get(n) {
                            Some(w) if *w != 1.0 => ui.monospace(format!("{}·{n}", fmt(*w))),
                            _ => ui.monospace(n),
                        };
                    }
                    if !set.note.is_empty() {
                        ui.weak(&set.note);
                    }
                });
            }
        });
    act
}

// ---------------------------------------------------------------- trace

pub fn trace(
    ui: &mut egui::Ui,
    st: &mut ViewState,
    id: &str,
    steps: &[pflow_native_core::artifact::TraceStep],
) -> Action {
    let mut act = None;
    if steps.is_empty() {
        ui.label("Empty trace.");
        return None;
    }
    let last = steps.len() - 1;
    st.step = st.step.min(last);
    let go = |st: &mut ViewState, k: usize| {
        st.step = k.min(last);
        Some(("sim.goto", serde_json::json!({ "id": id, "step": st.step })))
    };
    ui.horizontal(|ui| {
        if ui.button("⏮").on_hover_text("first").clicked() {
            act = go(st, 0);
        }
        if ui.button("◀").on_hover_text("previous").clicked() {
            act = go(st, st.step.saturating_sub(1));
        }
        let label = if st.playing { "⏸ pause" } else { "▶ play" };
        if ui.button(label).clicked() {
            st.playing = !st.playing;
            if st.playing && st.step == last {
                act = go(st, 0);
            }
        }
        if ui.button("▶|").on_hover_text("next").clicked() {
            act = go(st, st.step + 1);
        }
        if ui.button("⏭").on_hover_text("last").clicked() {
            act = go(st, last);
        }
        ui.label(format!("step {} / {}", st.step, last));
    });
    if st.playing {
        let now = ui.input(|i| i.time);
        if now - st.last_tick > 0.7 {
            st.last_tick = now;
            if st.step >= last {
                st.playing = false;
            } else {
                act = go(st, st.step + 1);
            }
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(100));
    }
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Grid::new("trace").striped(true).show(ui, |ui| {
                ui.strong("#");
                ui.strong("fired");
                ui.strong("marking after");
                ui.end_row();
                for (i, s) in steps.iter().enumerate() {
                    let sel = i == st.step;
                    if ui.selectable_label(sel, i.to_string()).clicked() {
                        act = go(st, i);
                    }
                    ui.monospace(if s.transition.is_empty() {
                        "(start)"
                    } else {
                        &s.transition
                    });
                    let mk: Vec<String> = s
                        .marking
                        .iter()
                        .filter(|(_, v)| v.iter().any(|x| *x != 0))
                        .map(|(p, v)| {
                            if v.len() == 1 {
                                format!("{p}={}", v[0])
                            } else {
                                format!("{p}={v:?}")
                            }
                        })
                        .collect();
                    ui.label(mk.join("  "));
                    ui.end_row();
                }
            });
        });
    act
}

// ---------------------------------------------------------------- bars

pub fn bars(ui: &mut egui::Ui, value_label: &str, bars: &[pflow_native_core::artifact::Bar]) {
    let (fg, grid) = colors(ui);
    let n = bars.len().max(1);
    let row_h = 22.0_f32;
    let height = (n as f32 * row_h + 40.0).max(ui.available_height());
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let (resp, painter) =
                ui.allocate_painter(Vec2::new(ui.available_width(), height), Sense::hover());
            let full = resp.rect;
            let name_w = 150.0;
            let plot = Rect::from_min_max(
                full.min + Vec2::new(name_w, 6.0),
                full.max - Vec2::new(70.0, 24.0),
            );
            let vals = bars
                .iter()
                .flat_map(|b| [Some(b.value), b.lo, b.hi])
                .flatten();
            let has_range = bars.iter().any(|b| b.lo.is_some() || b.hi.is_some());
            let (mut lo, mut hi) = range(vals);
            if !has_range {
                lo = lo.min(0.0);
                hi = hi.max(0.0);
            }
            let ax = Axes {
                plot,
                x: (lo, hi),
                y: (0.0, 1.0),
            };
            let small = FontId::proportional(11.0);
            for i in 0..=4 {
                let xv = lo + (hi - lo) * i as f64 / 4.0;
                let x = ax.sx(xv);
                painter.line_segment(
                    [Pos2::new(x, plot.top()), Pos2::new(x, plot.bottom())],
                    Stroke::new(1.0, grid),
                );
                painter.text(
                    Pos2::new(x, plot.bottom() + 3.0),
                    Align2::CENTER_TOP,
                    fmt(xv),
                    small.clone(),
                    fg,
                );
            }
            painter.text(
                Pos2::new(plot.center().x, plot.bottom() + 15.0),
                Align2::CENTER_TOP,
                value_label,
                small.clone(),
                fg,
            );
            for (i, b) in bars.iter().enumerate() {
                let y = plot.top() + i as f32 * row_h + row_h / 2.0;
                painter.text(
                    Pos2::new(plot.left() - 8.0, y),
                    Align2::RIGHT_CENTER,
                    &b.name,
                    small.clone(),
                    fg,
                );
                if has_range {
                    let base = ax.sx(b.value);
                    if let Some(l) = b.lo {
                        let r = Rect::from_x_y_ranges(
                            ax.sx(l).min(base)..=ax.sx(l).max(base),
                            (y - 7.0)..=(y + 7.0),
                        );
                        painter.rect_filled(r, 2.0, palette(0));
                    }
                    if let Some(h) = b.hi {
                        let r = Rect::from_x_y_ranges(
                            ax.sx(h).min(base)..=ax.sx(h).max(base),
                            (y - 7.0)..=(y + 7.0),
                        );
                        painter.rect_filled(r, 2.0, palette(1));
                    }
                    painter.line_segment(
                        [Pos2::new(base, y - 9.0), Pos2::new(base, y + 9.0)],
                        Stroke::new(1.5, fg),
                    );
                    let text = format!(
                        "{} … {}",
                        b.lo.map(fmt).unwrap_or_default(),
                        b.hi.map(fmt).unwrap_or_default()
                    );
                    painter.text(
                        Pos2::new(plot.right() + 6.0, y),
                        Align2::LEFT_CENTER,
                        text,
                        small.clone(),
                        fg,
                    );
                } else {
                    let x0 = ax.sx(0.0);
                    let x1 = ax.sx(b.value);
                    let r = Rect::from_x_y_ranges(x0.min(x1)..=x0.max(x1), (y - 7.0)..=(y + 7.0));
                    painter.rect_filled(r, 2.0, palette(0));
                    painter.text(
                        Pos2::new(plot.right() + 6.0, y),
                        Align2::LEFT_CENTER,
                        fmt(b.value),
                        small.clone(),
                        fg,
                    );
                }
            }
        });
}

// ---------------------------------------------------------------- drawing

fn css(c: &str) -> Option<Color32> {
    let h = c.trim().strip_prefix('#')?;
    let v = u32::from_str_radix(h, 16).ok()?;
    match h.len() {
        6 => Some(Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)),
        3 => {
            let (r, g, b) = ((v >> 8) & 15, (v >> 4) & 15, v & 15);
            Some(Color32::from_rgb(
                (r * 17) as u8,
                (g * 17) as u8,
                (b * 17) as u8,
            ))
        }
        _ => None,
    }
    .or(match c {
        "black" => Some(Color32::BLACK),
        "white" => Some(Color32::WHITE),
        _ => None,
    })
}

pub fn drawing(
    ui: &mut egui::Ui,
    width: f64,
    height: f64,
    shapes: &[pflow_native_core::artifact::DrawShape],
) {
    use pflow_native_core::artifact::DrawShape as D;
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
    let r = resp.rect.shrink(6.0);
    // Drawings are authored on white; keep that so their colors read.
    let k = (r.width() as f64 / width.max(1.0)).min(r.height() as f64 / height.max(1.0)) as f32;
    let origin = r.center() - Vec2::new(width as f32 * k, height as f32 * k) / 2.0;
    let page = Rect::from_min_size(origin, Vec2::new(width as f32 * k, height as f32 * k));
    painter.rect_filled(page, 4.0, Color32::WHITE);
    let p = |x: f64, y: f64| origin + Vec2::new(x as f32 * k, y as f32 * k);
    let ink = |c: &str| css(c).unwrap_or(Color32::from_gray(60));
    for sh in shapes {
        match sh {
            D::Line {
                points,
                color,
                width,
                dashed,
            } => {
                let pts: Vec<Pos2> = points.iter().map(|&(x, y)| p(x, y)).collect();
                let stroke = Stroke::new((*width as f32 * k).max(0.6), ink(color));
                if *dashed {
                    painter.extend(Shape::dashed_line(&pts, stroke, 6.0 * k, 4.0 * k));
                } else {
                    painter.add(Shape::line(pts, stroke));
                }
            }
            D::Rect {
                x,
                y,
                w,
                h,
                fill,
                stroke,
                radius,
            } => {
                let rr = Rect::from_min_size(p(*x, *y), Vec2::new(*w as f32 * k, *h as f32 * k));
                let fill = fill
                    .as_deref()
                    .and_then(css)
                    .unwrap_or(Color32::TRANSPARENT);
                let st = stroke
                    .as_deref()
                    .and_then(css)
                    .map(|c| Stroke::new(1.0, c))
                    .unwrap_or(Stroke::NONE);
                painter.rect(rr, *radius as f32 * k, fill, st, StrokeKind::Middle);
            }
            D::Circle {
                x,
                y,
                r: rad,
                fill,
                stroke,
            } => {
                let fill = fill
                    .as_deref()
                    .and_then(css)
                    .unwrap_or(Color32::TRANSPARENT);
                let st = stroke
                    .as_deref()
                    .and_then(css)
                    .map(|c| Stroke::new(1.0, c))
                    .unwrap_or(Stroke::NONE);
                painter.circle(p(*x, *y), *rad as f32 * k, fill, st);
            }
            D::Text {
                x,
                y,
                text,
                size,
                anchor,
                color,
            } => {
                // SVG text sits on its baseline; egui anchors boxes.
                let align = match anchor.as_str() {
                    "start" => Align2::LEFT_BOTTOM,
                    "end" => Align2::RIGHT_BOTTOM,
                    _ => Align2::CENTER_BOTTOM,
                };
                let fs = (*size as f32 * k).max(6.0);
                painter.text(
                    p(*x, *y) + Vec2::new(0.0, fs * 0.25),
                    align,
                    text,
                    FontId::proportional(fs),
                    ink(color),
                );
            }
        }
    }
}

// ---------------------------------------------------------------- graph

/// Layered layout (levels left to right): BFS depth from the initial node (data.initial, else the
/// first), unreached nodes on a final layer, two barycenter passes.
fn layer_graph(
    nodes: &[pflow_native_core::artifact::GraphNode],
    edges: &[pflow_native_core::artifact::GraphEdge],
) -> Vec<Pos2> {
    use std::collections::{HashMap, VecDeque};
    let idx: HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut out: Vec<Vec<usize>> = vec![vec![]; nodes.len()];
    let mut inn: Vec<Vec<usize>> = vec![vec![]; nodes.len()];
    for e in edges {
        if let (Some(&a), Some(&b)) = (idx.get(e.source.as_str()), idx.get(e.target.as_str())) {
            out[a].push(b);
            inn[b].push(a);
        }
    }
    let start = nodes
        .iter()
        .position(|n| n.data.get("initial").and_then(Value::as_bool) == Some(true))
        .unwrap_or(0);
    let mut depth = vec![usize::MAX; nodes.len()];
    let mut q = VecDeque::new();
    if !nodes.is_empty() {
        depth[start] = 0;
        q.push_back(start);
    }
    while let Some(u) = q.pop_front() {
        for &v in &out[u] {
            if depth[v] == usize::MAX {
                depth[v] = depth[u] + 1;
                q.push_back(v);
            }
        }
    }
    let maxd = depth
        .iter()
        .filter(|d| **d != usize::MAX)
        .max()
        .copied()
        .unwrap_or(0);
    for d in depth.iter_mut() {
        if *d == usize::MAX {
            *d = maxd + 1;
        }
    }
    let nl = depth.iter().max().map_or(0, |m| m + 1);
    let mut layers: Vec<Vec<usize>> = vec![vec![]; nl];
    for (i, &d) in depth.iter().enumerate() {
        layers[d].push(i);
    }
    let mut order = vec![0.0_f32; nodes.len()];
    for l in &layers {
        for (k, &i) in l.iter().enumerate() {
            order[i] = k as f32;
        }
    }
    for _ in 0..2 {
        for l in layers.iter_mut().skip(1) {
            let bc = |i: usize| {
                let ps: Vec<f32> = inn[i].iter().map(|&p| order[p]).collect();
                if ps.is_empty() {
                    order[i]
                } else {
                    ps.iter().sum::<f32>() / ps.len() as f32
                }
            };
            l.sort_by(|&a, &b| bc(a).total_cmp(&bc(b)));
            for (k, &i) in l.iter().enumerate() {
                order[i] = k as f32;
            }
        }
    }
    let mut pos = vec![Pos2::ZERO; nodes.len()];
    for (d, l) in layers.iter().enumerate() {
        let w = l.len() as f32;
        for (k, &i) in l.iter().enumerate() {
            // Levels run left to right: the results panel is wide and short.
            pos[i] = Pos2::new(d as f32 * 190.0, (k as f32 - (w - 1.0) / 2.0) * 44.0);
        }
    }
    pos
}

pub fn graph(
    ui: &mut egui::Ui,
    st: &mut ViewState,
    id: &str,
    nodes: &[pflow_native_core::artifact::GraphNode],
    edges: &[pflow_native_core::artifact::GraphEdge],
) -> Action {
    let mut act = None;
    if nodes.is_empty() {
        ui.label("Empty graph.");
        return None;
    }
    if st.graph.as_ref().is_none_or(|p| p.len() != nodes.len()) {
        st.graph = Some(layer_graph(nodes, edges));
        st.zoom = 0.0;
    }
    let pos = st.graph.clone().unwrap();
    let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = resp.rect;
    let (fg, _) = colors(ui);
    let dark = ui.visuals().dark_mode;
    if st.zoom == 0.0 {
        // Fit.
        let (x0, x1) = range(pos.iter().map(|p| p.x as f64));
        let (y0, y1) = range(pos.iter().map(|p| p.y as f64));
        let w = (x1 - x0) as f32 + 200.0;
        let h = (y1 - y0) as f32 + 80.0;
        st.zoom = (rect.width() / w).min(rect.height() / h).clamp(0.05, 1.5);
        st.pan = -Vec2::new((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0);
    }
    if resp.dragged() {
        st.pan += resp.drag_delta() / st.zoom;
    }
    if resp.hovered() {
        let (scroll, zd) = ui.input(|i| (i.smooth_scroll_delta(), i.zoom_delta()));
        if zd != 1.0 {
            st.zoom = (st.zoom * zd).clamp(0.03, 4.0);
        } else if scroll.y != 0.0 {
            st.zoom = (st.zoom * (1.0 + scroll.y * 0.002)).clamp(0.03, 4.0);
        }
    }
    let z = st.zoom;
    let to = |p: Pos2| rect.center() + (p.to_vec2() + st.pan) * z;
    let painter = painter.with_clip_rect(rect);
    let small = FontId::proportional((11.0 * z).clamp(6.0, 14.0));
    let idx: std::collections::HashMap<&str, usize> = nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let box_size = |n: &pflow_native_core::artifact::GraphNode| {
        Vec2::new(
            (n.label.chars().count() as f32 * 6.5 + 16.0).clamp(40.0, 160.0),
            26.0,
        ) * z
    };
    let edge_col = if dark {
        Color32::from_gray(120)
    } else {
        Color32::from_gray(150)
    };
    for e in edges {
        let (Some(&a), Some(&b)) = (idx.get(e.source.as_str()), idx.get(e.target.as_str())) else {
            continue;
        };
        let (pa, pb) = (to(pos[a]), to(pos[b]));
        if !rect.expand(200.0).contains(pa) && !rect.expand(200.0).contains(pb) {
            continue;
        }
        if a == b {
            painter.circle_stroke(
                pa + Vec2::new(box_size(&nodes[a]).x / 2.0, -8.0 * z),
                8.0 * z,
                Stroke::new(1.0, edge_col),
            );
            continue;
        }
        let d = (pb - pa).normalized();
        let tip = pb - d * 14.0 * z;
        painter.line_segment([pa + d * 14.0 * z, tip], Stroke::new(1.0, edge_col));
        let nrm = Vec2::new(-d.y, d.x) * 4.0 * z;
        painter.add(Shape::convex_polygon(
            vec![tip, tip - d * 8.0 * z + nrm, tip - d * 8.0 * z - nrm],
            edge_col,
            Stroke::NONE,
        ));
        if z > 0.5 && edges.len() < 600 {
            painter.text(
                pa.lerp(pb, 0.5),
                Align2::CENTER_CENTER,
                &e.label,
                small.clone(),
                fg.gamma_multiply(0.75),
            );
        }
    }
    let hover = resp.hover_pos();
    let mut hovered = None;
    for (i, n) in nodes.iter().enumerate() {
        let c = to(pos[i]);
        if !rect.expand(100.0).contains(c) {
            continue;
        }
        let r = Rect::from_center_size(c, box_size(n));
        let initial = n.data.get("initial").and_then(Value::as_bool) == Some(true);
        let deadlock = n.data.get("deadlock").and_then(Value::as_bool) == Some(true);
        let fill = if deadlock {
            Color32::from_rgb(220, 70, 70)
        } else if dark {
            Color32::from_gray(45)
        } else {
            Color32::WHITE
        };
        let stroke = if initial {
            Stroke::new(2.5, Color32::from_rgb(16, 185, 129))
        } else {
            Stroke::new(1.0, fg.gamma_multiply(0.6))
        };
        painter.rect(r, 5.0 * z, fill, stroke, StrokeKind::Middle);
        if z > 0.25 {
            painter.text(
                c,
                Align2::CENTER_CENTER,
                &n.label,
                small.clone(),
                if deadlock { Color32::WHITE } else { fg },
            );
        }
        if hover.is_some_and(|h| r.contains(h)) {
            hovered = Some(i);
        }
    }
    if let Some(i) = hovered {
        let n = &nodes[i];
        let mut text = format!("{}\n{}", n.id, n.label);
        if !n.data.is_null() {
            text.push_str(&format!("\n{}", n.data));
        }
        text.push_str("\nclick: load this state onto the net");
        tooltip(&painter, ui, hover.unwrap(), rect, &text);
        if resp.clicked() {
            act = Some(("sim.goto", serde_json::json!({ "id": id, "node": n.id })));
        }
    }
    painter.text(
        rect.left_bottom() + Vec2::new(6.0, -4.0),
        Align2::LEFT_BOTTOM,
        format!(
            "{} states, {} edges · drag to pan, scroll to zoom",
            nodes.len(),
            edges.len()
        ),
        FontId::proportional(11.0),
        fg.gamma_multiply(0.7),
    );
    act
}

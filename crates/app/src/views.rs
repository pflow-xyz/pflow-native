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
}

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

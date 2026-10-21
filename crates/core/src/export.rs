//! Exports with no UI dependency, so a headless agent gets the same picture
//! the app shows: the net as SVG, artifacts as JSON / CSV / SVG.

use std::fmt::Write as _;

use serde_json::Value;

use crate::artifact::{Artifact, ArtifactData};
use crate::doc::Document;
use crate::sim::Marking;
use crate::{Error, Result};

pub const PLACE_R: f64 = 20.0;
pub const TRANSITION_W: f64 = 30.0;
pub const TRANSITION_H: f64 = 30.0;

/// Line-chart palette, shared with the app so exports match the screen.
pub const PALETTE: &[(u8, u8, u8)] = &[
    (0x3b, 0x82, 0xf6),
    (0xef, 0x44, 0x44),
    (0x10, 0xb9, 0x81),
    (0xf5, 0x9e, 0x0b),
    (0x8b, 0x5c, 0xf6),
    (0xec, 0x48, 0x99),
    (0x06, 0xb6, 0xd4),
    (0x84, 0xcc, 0x16),
];

/// The display color of token color `i` named `name` (a pflow.xyz token
/// URL like `https://pflow.xyz/tokens/red`, a CSS name, or `#rrggbb`).
pub fn token_rgb(name: &str, i: usize) -> (u8, u8, u8) {
    let short = name.rsplit('/').next().unwrap_or(name).to_ascii_lowercase();
    if let Some(hex) = short.strip_prefix('#').filter(|h| h.len() == 6) {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            return ((v >> 16) as u8, (v >> 8) as u8, v as u8);
        }
    }
    match short.as_str() {
        "black" => (0x55, 0x55, 0x55),
        "red" => (0xef, 0x44, 0x44),
        "blue" => (0x3b, 0x82, 0xf6),
        "green" => (0x10, 0xb9, 0x81),
        "brown" => (0xa1, 0x62, 0x07),
        "orange" => (0xf9, 0x73, 0x16),
        "yellow" => (0xea, 0xb3, 0x08),
        "purple" => (0x8b, 0x5c, 0xf6),
        "pink" => (0xec, 0x48, 0x99),
        "cyan" => (0x06, 0xb6, 0xd4),
        "gray" | "grey" => (0x9c, 0xa3, 0xaf),
        _ => PALETTE[i % PALETTE.len()],
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The net, with the given marking and enabled transitions highlighted.
pub fn net_svg(doc: &Document, marking: &Marking, enabled: &[String]) -> String {
    let (x0, y0, x1, y1) = doc.bounds().unwrap_or((0.0, 0.0, 200.0, 200.0));
    let pad = 60.0;
    let (vx, vy, vw, vh) = (x0 - pad, y0 - pad, x1 - x0 + 2.0 * pad, y1 - y0 + 2.0 * pad);
    let mut s = String::new();
    let _ = write!(
        s,
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vx} {vy} {vw} {vh}" width="{vw}" height="{vh}" font-family="sans-serif" font-size="12">"#
    );
    s.push_str(r##"<defs><marker id="arrow" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="#555"/></marker><marker id="inhibit" viewBox="0 0 10 10" refX="10" refY="5" markerWidth="8" markerHeight="8"><circle cx="5" cy="5" r="4" fill="white" stroke="#555" stroke-width="1.5"/></marker></defs>"##);
    let _ = write!(
        s,
        r#"<rect x="{vx}" y="{vy}" width="{vw}" height="{vh}" fill="white"/>"#
    );
    for a in &doc.arcs {
        let (Some(p0), Some(p1)) = (doc.position(&a.source), doc.position(&a.target)) else {
            continue;
        };
        let (from, to) = clip_segment(doc, &a.source, p0, &a.target, p1);
        let marker = if a.inhibit { "inhibit" } else { "arrow" };
        let _ = write!(
            s,
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#555" stroke-width="1.5" marker-end="url(#{marker})"/>"##,
            from.0, from.1, to.0, to.1
        );
        let w = a.weight_sum();
        if w != 1.0 {
            let (mx, my) = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
            let _ = write!(
                s,
                r##"<text x="{mx:.1}" y="{:.1}" text-anchor="middle" fill="#333">{}</text>"##,
                my - 4.0,
                fmt_num(w)
            );
        }
    }
    for p in &doc.places {
        let tokens: i64 = marking.get(&p.id).map(|v| v.iter().sum()).unwrap_or(0);
        let _ = write!(
            s,
            r##"<circle cx="{}" cy="{}" r="{PLACE_R}" fill="white" stroke="#333" stroke-width="2"/>"##,
            p.x, p.y
        );
        if tokens == 1 {
            let _ = write!(
                s,
                r##"<circle cx="{}" cy="{}" r="4" fill="#111"/>"##,
                p.x, p.y
            );
        } else if tokens > 1 {
            let _ = write!(
                s,
                r##"<text x="{}" y="{}" text-anchor="middle" font-size="14" font-weight="bold">{tokens}</text>"##,
                p.x,
                p.y + 5.0
            );
        }
        let label = p.label.as_deref().unwrap_or(&p.id);
        let _ = write!(
            s,
            r##"<text x="{}" y="{}" text-anchor="middle" fill="#333">{}</text>"##,
            p.x,
            p.y - PLACE_R - 6.0,
            esc(label)
        );
    }
    for t in &doc.transitions {
        let fill = if enabled.contains(&t.id) {
            "#62fa75"
        } else {
            "#ffffff"
        };
        let _ = write!(
            s,
            r##"<rect x="{}" y="{}" width="{TRANSITION_W}" height="{TRANSITION_H}" fill="{fill}" stroke="#333" stroke-width="2"/>"##,
            t.x - TRANSITION_W / 2.0,
            t.y - TRANSITION_H / 2.0
        );
        let label = t.label.as_deref().unwrap_or(&t.id);
        let _ = write!(
            s,
            r##"<text x="{}" y="{}" text-anchor="middle" fill="#333">{}</text>"##,
            t.x,
            t.y - TRANSITION_H / 2.0 - 6.0,
            esc(label)
        );
    }
    s.push_str("</svg>\n");
    s
}

/// Shortens a center-to-center segment so it starts and ends on the node
/// outlines.
pub fn clip_segment(
    doc: &Document,
    src: &str,
    p0: (f64, f64),
    dst: &str,
    p1: (f64, f64),
) -> ((f64, f64), (f64, f64)) {
    let (dx, dy) = (p1.0 - p0.0, p1.1 - p0.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-9);
    let (ux, uy) = (dx / len, dy / len);
    let r = |id: &str| -> f64 {
        match doc.kind_of(id) {
            Some(crate::doc::NodeKind::Place) => PLACE_R,
            _ => {
                // Distance from a square's center to its edge along (ux, uy).
                let h = TRANSITION_W / 2.0;
                h / ux.abs().max(uy.abs()).max(1e-9)
            }
        }
    };
    let (r0, r1) = (r(src), r(dst));
    (
        (p0.0 + ux * r0, p0.1 + uy * r0),
        (p1.0 - ux * r1, p1.1 - uy * r1),
    )
}

fn fmt_num(f: f64) -> String {
    if f.fract() == 0.0 {
        format!("{}", f as i64)
    } else {
        format!("{f:.4}")
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Writes `artifact` in `format` ("json", "csv", "svg").
pub fn artifact(a: &Artifact, format: &str) -> Result<String> {
    if format == "json" {
        return Ok(serde_json::to_string_pretty(a).expect("serializable"));
    }
    if !a.data.formats().contains(&format) {
        return Err(Error::BadArgs(format!(
            "{} artifacts export as {:?}, not {format:?}",
            a.data.kind(),
            a.data.formats()
        )));
    }
    match (&a.data, format) {
        (ArtifactData::TimeSeries { x_label, x, series }, "csv") => {
            let mut s = csv_field(x_label);
            for se in series {
                let name = match &se.group {
                    Some(g) => format!("{g}:{}", se.name),
                    None => se.name.clone(),
                };
                s.push(',');
                s.push_str(&csv_field(&name));
                if se.band.is_some() {
                    s.push(',');
                    s.push_str(&csv_field(&format!("{name}_sd")));
                }
            }
            s.push('\n');
            for (i, xv) in x.iter().enumerate() {
                s.push_str(&xv.to_string());
                for se in series {
                    s.push(',');
                    s.push_str(&se.values.get(i).map(|v| v.to_string()).unwrap_or_default());
                    if let Some(b) = &se.band {
                        s.push(',');
                        s.push_str(&b.get(i).map(|v| v.to_string()).unwrap_or_default());
                    }
                }
                s.push('\n');
            }
            Ok(s)
        }
        (ArtifactData::TimeSeries { x_label, x, series }, "svg") => {
            Ok(timeseries_svg(&a.title, x_label, x, series))
        }
        (
            ArtifactData::Heatmap {
                x_label,
                y_label,
                xs,
                ys,
                z,
            },
            "svg",
        ) => Ok(heatmap_svg(&a.title, x_label, y_label, xs, ys, z)),
        (
            ArtifactData::Scatter {
                x_label,
                y_label,
                points,
                highlight,
            },
            "svg",
        ) => Ok(scatter_svg(&a.title, x_label, y_label, points, highlight)),
        (ArtifactData::Matrix { rows, cols, values }, "csv") => {
            let mut s = String::new();
            for c in cols {
                s.push(',');
                s.push_str(&csv_field(c));
            }
            s.push('\n');
            for (r, row) in rows.iter().zip(values) {
                s.push_str(&csv_field(r));
                for v in row {
                    s.push(',');
                    s.push_str(&v.to_string());
                }
                s.push('\n');
            }
            Ok(s)
        }
        (ArtifactData::Table { columns, rows }, "csv") => {
            let mut s = columns
                .iter()
                .map(|c| csv_field(c))
                .collect::<Vec<_>>()
                .join(",");
            s.push('\n');
            for row in rows {
                let cells: Vec<String> = row
                    .iter()
                    .map(|v| match v {
                        Value::String(t) => csv_field(t),
                        other => csv_field(&other.to_string()),
                    })
                    .collect();
                s.push_str(&cells.join(","));
                s.push('\n');
            }
            Ok(s)
        }
        (ArtifactData::Heatmap { xs, ys, z, .. }, "csv") => {
            let mut s = String::from("y\\x");
            for x in xs {
                let _ = write!(s, ",{x}");
            }
            s.push('\n');
            for (y, row) in ys.iter().zip(z) {
                s.push_str(&y.to_string());
                for v in row {
                    let _ = write!(s, ",{v}");
                }
                s.push('\n');
            }
            Ok(s)
        }
        (
            ArtifactData::Scatter {
                x_label,
                y_label,
                points,
                highlight,
            },
            "csv",
        ) => {
            let mut s = format!("{},{},highlight\n", csv_field(x_label), csv_field(y_label));
            for (i, (x, y)) in points.iter().enumerate() {
                let _ = writeln!(s, "{x},{y},{}", highlight.contains(&i));
            }
            Ok(s)
        }
        (ArtifactData::Trace { steps }, "csv") => {
            let mut s = String::from("step,transition,marking\n");
            for (i, st) in steps.iter().enumerate() {
                let mk = serde_json::to_string(&st.marking).expect("serializable");
                let _ = writeln!(s, "{i},{},{}", csv_field(&st.transition), csv_field(&mk));
            }
            Ok(s)
        }
        _ => unreachable!("formats() lists only handled pairs"),
    }
}

/// Sequential color scale (viridis-like) for t in [0, 1]; shared with the
/// app so heatmap exports match the screen.
pub fn heat_rgb(t: f64) -> (u8, u8, u8) {
    const STOPS: [(f64, f64, f64); 5] = [
        (68.0, 1.0, 84.0),
        (59.0, 82.0, 139.0),
        (33.0, 145.0, 140.0),
        (94.0, 201.0, 98.0),
        (253.0, 231.0, 37.0),
    ];
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let f = t * (STOPS.len() - 1) as f64;
    let i = (f.floor() as usize).min(STOPS.len() - 2);
    let u = f - i as f64;
    let (a, b) = (STOPS[i], STOPS[i + 1]);
    let mix = |x: f64, y: f64| (x + (y - x) * u).round() as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// Color index per distinct series name, in first-seen order.
pub fn name_index(series: &[crate::artifact::Series]) -> Vec<usize> {
    let mut names: Vec<&str> = vec![];
    series
        .iter()
        .map(|s| match names.iter().position(|n| *n == s.name) {
            Some(i) => i,
            None => {
                names.push(&s.name);
                names.len() - 1
            }
        })
        .collect()
}

/// Dash index per distinct group (None = solid), in first-seen order.
pub fn group_index(series: &[crate::artifact::Series]) -> Vec<usize> {
    let mut groups: Vec<&str> = vec![];
    series
        .iter()
        .map(|s| match &s.group {
            None => 0,
            Some(g) => match groups.iter().position(|x| *x == g) {
                Some(i) => i,
                None => {
                    groups.push(g);
                    groups.len() - 1
                }
            },
        })
        .collect()
}

const DASHES: &[&str] = &["", "6 3", "2 3", "10 3 2 3", "1 2"];

struct Frame {
    h: f64,
    ml: f64,
    mt: f64,
    pw: f64,
    ph: f64,
}

fn svg_frame(title: &str, legend: bool) -> (String, Frame) {
    let (w, h) = (720.0, 400.0);
    let (ml, mr, mt, mb) = (56.0, if legend { 150.0 } else { 20.0 }, 32.0, 40.0);
    let f = Frame {
        h,
        ml,
        mt,
        pw: w - ml - mr,
        ph: h - mt - mb,
    };
    let mut s = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" font-family="sans-serif" font-size="11">"#
    );
    let _ = write!(s, r#"<rect width="{w}" height="{h}" fill="white"/>"#);
    let _ = write!(
        s,
        r#"<text x="{ml}" y="20" font-size="13" font-weight="bold">{}</text>"#,
        esc(title)
    );
    (s, f)
}

fn svg_axes(s: &mut String, f: &Frame, x: (f64, f64), y: (f64, f64), x_label: &str, y_label: &str) {
    let _ = write!(
        s,
        r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="#999"/>"##,
        f.ml, f.mt, f.pw, f.ph
    );
    for i in 0..=4 {
        let fy = i as f64 / 4.0;
        let yv = y.0 + (y.1 - y.0) * fy;
        let _ = write!(
            s,
            r##"<text x="{}" y="{:.1}" text-anchor="end" fill="#555">{}</text>"##,
            f.ml - 4.0,
            f.mt + f.ph - fy * f.ph + 4.0,
            short_num(yv)
        );
        let xv = x.0 + (x.1 - x.0) * fy;
        let _ = write!(
            s,
            r##"<text x="{:.1}" y="{}" text-anchor="middle" fill="#555">{}</text>"##,
            f.ml + fy * f.pw,
            f.mt + f.ph + 14.0,
            short_num(xv)
        );
    }
    let _ = write!(
        s,
        r##"<text x="{:.1}" y="{}" text-anchor="middle" fill="#333">{}</text>"##,
        f.ml + f.pw / 2.0,
        f.h - 6.0,
        esc(x_label)
    );
    if !y_label.is_empty() {
        let _ = write!(
            s,
            r##"<text x="12" y="{:.1}" text-anchor="middle" fill="#333" transform="rotate(-90 12 {:.1})">{}</text>"##,
            f.mt + f.ph / 2.0,
            f.mt + f.ph / 2.0,
            esc(y_label)
        );
    }
}

fn short_num(v: f64) -> String {
    let a = v.abs();
    if a != 0.0 && !(1e-3..1e5).contains(&a) {
        format!("{v:.2e}")
    } else {
        format!("{v:.3}")
    }
}

fn timeseries_svg(
    title: &str,
    x_label: &str,
    x: &[f64],
    series: &[crate::artifact::Series],
) -> String {
    let (mut s, f) = svg_frame(title, true);
    let xmin = x.first().copied().unwrap_or(0.0);
    let xmax = x.last().copied().unwrap_or(1.0).max(xmin + 1e-9);
    let ymax = series
        .iter()
        .flat_map(|se| {
            se.values
                .iter()
                .enumerate()
                .map(move |(i, v)| v + se.band.as_ref().map_or(0.0, |b| b[i]))
        })
        .fold(0.0_f64, f64::max)
        .max(1e-9);
    let ymin = series
        .iter()
        .flat_map(|se| se.values.iter().copied())
        .fold(0.0_f64, f64::min);
    let sx = |v: f64| f.ml + (v - xmin) / (xmax - xmin) * f.pw;
    let sy = |v: f64| f.mt + f.ph - (v - ymin) / (ymax - ymin) * f.ph;
    svg_axes(&mut s, &f, (xmin, xmax), (ymin, ymax), x_label, "");
    let ni = name_index(series);
    let gi = group_index(series);
    for (k, se) in series.iter().enumerate() {
        let (r, g, b) = PALETTE[ni[k] % PALETTE.len()];
        if let Some(band) = &se.band {
            let upper: Vec<String> = x
                .iter()
                .zip(&se.values)
                .zip(band)
                .map(|((&xv, &yv), &bv)| format!("{:.2},{:.2}", sx(xv), sy(yv + bv)))
                .collect();
            let lower: Vec<String> = x
                .iter()
                .zip(&se.values)
                .zip(band)
                .rev()
                .map(|((&xv, &yv), &bv)| format!("{:.2},{:.2}", sx(xv), sy(yv - bv)))
                .collect();
            let _ = write!(
                s,
                r#"<polygon fill="rgb({r},{g},{b})" fill-opacity="0.18" stroke="none" points="{} {}"/>"#,
                upper.join(" "),
                lower.join(" ")
            );
        }
        let pts: Vec<String> = x
            .iter()
            .zip(&se.values)
            .map(|(&xv, &yv)| format!("{:.2},{:.2}", sx(xv), sy(yv)))
            .collect();
        let dash = DASHES[gi[k] % DASHES.len()];
        let _ = write!(
            s,
            r#"<polyline fill="none" stroke="rgb({r},{g},{b})" stroke-width="2" stroke-dasharray="{dash}" points="{}"/>"#,
            pts.join(" ")
        );
        let label = match &se.group {
            Some(gr) => format!("{} · {}", se.name, gr),
            None => se.name.clone(),
        };
        let ly = f.mt + 12.0 + k as f64 * 15.0;
        if ly < f.h - 10.0 {
            let lx = f.ml + f.pw + 10.0;
            let _ = write!(
                s,
                r#"<line x1="{lx}" y1="{ly}" x2="{}" y2="{ly}" stroke="rgb({r},{g},{b})" stroke-width="3" stroke-dasharray="{dash}"/><text x="{}" y="{}">{}</text>"#,
                lx + 16.0,
                lx + 20.0,
                ly + 4.0,
                esc(&label)
            );
        }
    }
    s.push_str("</svg>\n");
    s
}

fn heatmap_svg(
    title: &str,
    x_label: &str,
    y_label: &str,
    xs: &[f64],
    ys: &[f64],
    z: &[Vec<f64>],
) -> String {
    let (mut s, f) = svg_frame(title, true);
    let (zmin, zmax) = z
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
            (a.min(v), b.max(v))
        });
    let span = (zmax - zmin).max(1e-12);
    let (nx, ny) = (xs.len().max(1), ys.len().max(1));
    let (cw, ch) = (f.pw / nx as f64, f.ph / ny as f64);
    for (iy, row) in z.iter().enumerate() {
        for (ix, &v) in row.iter().enumerate() {
            let (r, g, b) = heat_rgb((v - zmin) / span);
            let _ = write!(
                s,
                r#"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="rgb({r},{g},{b})"><title>{x_label}={} {y_label}={} → {}</title></rect>"#,
                f.ml + ix as f64 * cw,
                f.mt + f.ph - (iy + 1) as f64 * ch,
                cw + 0.3,
                ch + 0.3,
                xs[ix],
                ys[iy],
                v
            );
        }
    }
    let xr = (
        xs.first().copied().unwrap_or(0.0),
        xs.last().copied().unwrap_or(1.0),
    );
    let yr = (
        ys.first().copied().unwrap_or(0.0),
        ys.last().copied().unwrap_or(1.0),
    );
    svg_axes(&mut s, &f, xr, yr, x_label, y_label);
    // Color bar.
    let bx = f.ml + f.pw + 20.0;
    for i in 0..50 {
        let t = i as f64 / 49.0;
        let (r, g, b) = heat_rgb(t);
        let _ = write!(
            s,
            r#"<rect x="{bx}" y="{:.2}" width="16" height="{:.2}" fill="rgb({r},{g},{b})"/>"#,
            f.mt + f.ph - (i + 1) as f64 * f.ph / 50.0,
            f.ph / 50.0 + 0.3
        );
    }
    let _ = write!(
        s,
        r#"<text x="{}" y="{}">{}</text>"#,
        bx + 20.0,
        f.mt + 8.0,
        short_num(zmax)
    );
    let _ = write!(
        s,
        r#"<text x="{}" y="{}">{}</text>"#,
        bx + 20.0,
        f.mt + f.ph,
        short_num(zmin)
    );
    s.push_str("</svg>\n");
    s
}

fn scatter_svg(
    title: &str,
    x_label: &str,
    y_label: &str,
    points: &[(f64, f64)],
    highlight: &[usize],
) -> String {
    let (mut s, f) = svg_frame(title, false);
    let fold = |sel: fn(&(f64, f64)) -> f64| {
        points
            .iter()
            .map(sel)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), v| {
                (a.min(v), b.max(v))
            })
    };
    let (x0, x1) = fold(|p| p.0);
    let (y0, y1) = fold(|p| p.1);
    let (x0, x1) = if points.is_empty() {
        (0.0, 1.0)
    } else {
        (x0, x1.max(x0 + 1e-9))
    };
    let (y0, y1) = if points.is_empty() {
        (0.0, 1.0)
    } else {
        (y0, y1.max(y0 + 1e-9))
    };
    svg_axes(&mut s, &f, (x0, x1), (y0, y1), x_label, y_label);
    let sx = |v: f64| f.ml + (v - x0) / (x1 - x0) * f.pw;
    let sy = |v: f64| f.mt + f.ph - (v - y0) / (y1 - y0) * f.ph;
    for (i, &(x, y)) in points.iter().enumerate() {
        if !highlight.contains(&i) {
            let _ = write!(
                s,
                r##"<circle cx="{:.2}" cy="{:.2}" r="2.5" fill="#9ca3af"/>"##,
                sx(x),
                sy(y)
            );
        }
    }
    let mut front: Vec<(f64, f64)> = highlight
        .iter()
        .filter_map(|&i| points.get(i).copied())
        .collect();
    front.sort_by(|a, b| a.0.total_cmp(&b.0));
    if front.len() > 1 {
        let pts: Vec<String> = front
            .iter()
            .map(|&(x, y)| format!("{:.2},{:.2}", sx(x), sy(y)))
            .collect();
        let _ = write!(
            s,
            r##"<polyline fill="none" stroke="#ef4444" stroke-width="1.5" points="{}"/>"##,
            pts.join(" ")
        );
    }
    for &(x, y) in &front {
        let _ = write!(
            s,
            r##"<circle cx="{:.2}" cy="{:.2}" r="4" fill="#ef4444"/>"##,
            sx(x),
            sy(y)
        );
    }
    s.push_str("</svg>\n");
    s
}

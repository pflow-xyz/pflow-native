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
                s.push(',');
                s.push_str(&csv_field(&se.name));
            }
            s.push('\n');
            for (i, xv) in x.iter().enumerate() {
                s.push_str(&xv.to_string());
                for se in series {
                    s.push(',');
                    s.push_str(&se.values.get(i).map(|v| v.to_string()).unwrap_or_default());
                }
                s.push('\n');
            }
            Ok(s)
        }
        (ArtifactData::TimeSeries { x_label, x, series }, "svg") => {
            Ok(timeseries_svg(&a.title, x_label, x, series))
        }
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

fn timeseries_svg(
    title: &str,
    x_label: &str,
    x: &[f64],
    series: &[crate::artifact::Series],
) -> String {
    let (w, h) = (720.0, 400.0);
    let (ml, mr, mt, mb) = (56.0, 140.0, 32.0, 40.0);
    let (pw, ph) = (w - ml - mr, h - mt - mb);
    let xmin = x.first().copied().unwrap_or(0.0);
    let xmax = x.last().copied().unwrap_or(1.0).max(xmin + 1e-9);
    let ymax = series
        .iter()
        .flat_map(|s| s.values.iter().copied())
        .fold(0.0_f64, f64::max)
        .max(1e-9);
    let sx = |v: f64| ml + (v - xmin) / (xmax - xmin) * pw;
    let sy = |v: f64| mt + ph - v / ymax * ph;
    let mut s = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}" font-family="sans-serif" font-size="11">"#
    );
    let _ = write!(s, r#"<rect width="{w}" height="{h}" fill="white"/>"#);
    let _ = write!(
        s,
        r#"<text x="{ml}" y="20" font-size="13" font-weight="bold">{}</text>"#,
        esc(title)
    );
    let _ = write!(
        s,
        r##"<rect x="{ml}" y="{mt}" width="{pw}" height="{ph}" fill="none" stroke="#999"/>"##
    );
    for i in 0..=4 {
        let v = ymax * i as f64 / 4.0;
        let _ = write!(
            s,
            r##"<text x="{}" y="{:.1}" text-anchor="end" fill="#555">{:.3}</text>"##,
            ml - 4.0,
            sy(v) + 4.0,
            v
        );
        let xv = xmin + (xmax - xmin) * i as f64 / 4.0;
        let _ = write!(
            s,
            r##"<text x="{:.1}" y="{}" text-anchor="middle" fill="#555">{:.3}</text>"##,
            sx(xv),
            mt + ph + 14.0,
            xv
        );
    }
    let _ = write!(
        s,
        r##"<text x="{:.1}" y="{}" text-anchor="middle" fill="#333">{}</text>"##,
        ml + pw / 2.0,
        h - 6.0,
        esc(x_label)
    );
    for (i, se) in series.iter().enumerate() {
        let (r, g, b) = PALETTE[i % PALETTE.len()];
        let pts: Vec<String> = x
            .iter()
            .zip(&se.values)
            .map(|(&xv, &yv)| format!("{:.2},{:.2}", sx(xv), sy(yv)))
            .collect();
        let _ = write!(
            s,
            r#"<polyline fill="none" stroke="rgb({r},{g},{b})" stroke-width="2" points="{}"/>"#,
            pts.join(" ")
        );
        let ly = mt + 12.0 + i as f64 * 16.0;
        let _ = write!(
            s,
            r#"<line x1="{}" y1="{ly}" x2="{}" y2="{ly}" stroke="rgb({r},{g},{b})" stroke-width="3"/><text x="{}" y="{}">{}</text>"#,
            w - mr + 10.0,
            w - mr + 26.0,
            w - mr + 30.0,
            ly + 4.0,
            esc(&se.name)
        );
    }
    s.push_str("</svg>\n");
    s
}

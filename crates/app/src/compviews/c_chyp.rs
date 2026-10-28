//! Chyp diagrams for the Laws view: the geometry of one diagram of an
//! export (`crates/app/assets/chyp/*.json`), after the prototype's
//! `js/views/laws.js` (`fromChyp`, `fitBox`, `normalize`) and
//! `js/layout/hypergraph.js` (`wirePath`). Pure and egui-free.
//!
//! Coordinates are Chyp's own (`chyp.layout.convex_layout`, exported by
//! `chyp_headless` from Chyp 3d28483, Apache-2.0, Copyright (C) 2022-2023
//! Aleks Kissinger): box centres, layers, wire heights, and the routes wires
//! take through Chyp's hidden identity boxes. Only box widths are widened
//! here so the labels fit; ports stay on the box edges. The port offsets
//! (k/(n-1) - 0.5 units, chyp/gui/graphscene.py:151-170) and the box sizes
//! (1.1 x 0.8 or 1.6 units, graphscene.py:29-31, 60-66) are Chyp's. No Chyp
//! code is in this file.

use serde_json::Value;

pub const BOX_FONT: f32 = 11.0;
pub const DEFAULT_FILL: &str = "#e3edf7";

#[derive(Clone, Debug)]
pub struct DBox {
    pub id: String,
    pub label: String,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
    pub cy: f32,
    pub size: f32,
    pub bg: String,
    pub fg: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub ins: Vec<(f32, f32)>,
    pub outs: Vec<(f32, f32)>,
    /// Chyp's own centre and box size in units, for the inspector.
    pub chyp: Option<(f64, f64, f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct DWire {
    pub id: String,
    pub ty: Option<String>,
    pub pts: Vec<(f32, f32)>,
    /// Hidden layers the route passes (Chyp's identity boxes).
    pub route_len: usize,
}

#[derive(Clone, Debug)]
pub struct Dot {
    pub x: f32,
    pub y: f32,
    pub vertex: String,
}

#[derive(Clone, Debug)]
pub struct Dgm {
    pub boxes: Vec<DBox>,
    pub wires: Vec<DWire>,
    pub in_dots: Vec<Dot>,
    pub out_dots: Vec<Dot>,
    pub w: f32,
    pub h: f32,
    pub xl: Option<f32>,
    pub xr: Option<f32>,
    #[allow(dead_code)]
    pub scale: f32,
}

/// k-th of n ports on a box edge: offset from the centre, in units.
pub fn port_off(k: usize, n: usize) -> f32 {
    if n > 1 {
        k as f32 / (n as f32 - 1.0) - 0.5
    } else {
        0.0
    }
}

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

/// Widens a box so its label fits: free sides first (no ports there), else
/// both, capped. Returns (grow left, grow right, font size, overflow).
fn fit_box(
    label_w: &dyn Fn(f32) -> f32,
    n_in: usize,
    n_out: usize,
    base_w: f32,
    s: f32,
) -> (f32, f32, f32, bool) {
    let mut size = BOX_FONT;
    let need = |sz: f32| label_w(sz) + 10.0;
    let mut grow = (need(size) - base_w).max(0.0);
    let (mut gl, mut gr, mut overflow) = (0.0, 0.0, false);
    if grow > 0.0 {
        if n_in == 0 && n_out > 0 {
            gl = grow;
        } else if n_out == 0 && n_in > 0 {
            gr = grow;
        } else {
            let cap = (2.3 * s - base_w).max(0.0);
            while grow > cap && size > 9.0 {
                size -= 0.5;
                grow = (need(size) - base_w).max(0.0);
            }
            if grow > cap {
                overflow = true;
                grow = cap;
            }
            gl = grow / 2.0;
            gr = grow / 2.0;
        }
    }
    (gl, gr, size, overflow)
}

/// A Chyp graph (an export's `graphs[]` entry) at `s` pixels per Chyp unit,
/// in its own coordinates. `tw(text, size)` measures a label in pixels.
pub fn from_chyp(g: &Value, s: f32, styles: &Value, tw: &dyn Fn(&str, f32) -> f32) -> Dgm {
    let verts = g["vertices"].as_array().cloned().unwrap_or_default();
    let edges = g["edges"].as_array().cloned().unwrap_or_default();
    let mut boxes: Vec<DBox> = vec![];
    for e in &edges {
        let label = e["label"].as_str().unwrap_or("").to_string();
        let (inputs, outputs) = (strs(&e["inputs"]), strs(&e["outputs"]));
        let base_w = e["w"].as_f64().unwrap_or(1.1) as f32 * s;
        let h = e["h"].as_f64().unwrap_or(0.8) as f32 * s;
        let (gl, gr, size, _) =
            fit_box(&|sz| tw(&label, sz), inputs.len(), outputs.len(), base_w, s);
        let (cx, cy) = (f(&e["x"]) * s, f(&e["y"]) * s);
        let (left, right) = (cx - base_w / 2.0 - gl, cx + base_w / 2.0 + gr);
        let st = &styles[&label];
        boxes.push(DBox {
            id: e["id"].as_str().unwrap_or("").into(),
            ins: (0..inputs.len())
                .map(|k| (left, cy + port_off(k, inputs.len()) * s))
                .collect(),
            outs: (0..outputs.len())
                .map(|k| (right, cy + port_off(k, outputs.len()) * s))
                .collect(),
            label,
            left,
            right,
            top: cy - h / 2.0,
            bottom: cy + h / 2.0,
            cy,
            size,
            bg: st["bg"].as_str().unwrap_or(DEFAULT_FILL).into(),
            fg: st["fg"].as_str().unwrap_or("#111111").into(),
            inputs,
            outputs,
            chyp: Some((
                e["x"].as_f64().unwrap_or(0.0),
                e["y"].as_f64().unwrap_or(0.0),
                e["w"].as_f64().unwrap_or(1.1),
                e["h"].as_f64().unwrap_or(0.8),
            )),
        });
    }
    let mut prod: std::collections::BTreeMap<String, (f32, f32)> = Default::default();
    let mut cons: std::collections::BTreeMap<String, (f32, f32)> = Default::default();
    for b in &boxes {
        for (k, v) in b.outputs.iter().enumerate() {
            prod.insert(v.clone(), b.outs[k]);
        }
        for (k, v) in b.inputs.iter().enumerate() {
            cons.insert(v.clone(), b.ins[k]);
        }
    }
    let wires: Vec<DWire> = verts
        .iter()
        .map(|v| {
            let id = v["id"].as_str().unwrap_or("").to_string();
            let vp = (f(&v["x"]) * s, f(&v["y"]) * s);
            let mut pts: Vec<(f32, f32)> = vec![];
            if let Some(p) = prod.get(&id) {
                pts.push(*p);
                if vp.0 > p.0 + 0.5 {
                    pts.push(vp);
                }
            } else {
                pts.push(vp);
            }
            let route = v["route"].as_array().cloned().unwrap_or_default();
            for r in &route {
                pts.push((f(&r[0]) * s, f(&r[1]) * s));
            }
            if let Some(c) = cons.get(&id) {
                pts.push(*c);
            }
            DWire {
                id,
                ty: v["type"].as_str().map(str::to_string),
                pts,
                route_len: route.len(),
            }
        })
        .collect();
    let by_id = |id: &str| wires.iter().find(|w| w.id == id);
    let in_dots: Vec<Dot> = strs(&g["inputs"])
        .iter()
        .filter_map(|v| by_id(v).map(|w| (v, w.pts[0])))
        .map(|(v, p)| Dot {
            x: p.0,
            y: p.1,
            vertex: v.clone(),
        })
        .collect();
    let out_dots: Vec<Dot> = strs(&g["outputs"])
        .iter()
        .filter_map(|v| by_id(v).map(|w| (v, *w.pts.last().expect("a wire has points"))))
        .map(|(v, p)| Dot {
            x: p.0,
            y: p.1,
            vertex: v.clone(),
        })
        .collect();
    normalize(Dgm {
        boxes,
        wires,
        in_dots,
        out_dots,
        w: 0.0,
        h: 0.0,
        xl: None,
        xr: None,
        scale: s,
    })
}

/// Shifts to the origin with a margin; computes width, height, boundary x's.
pub fn normalize(mut d: Dgm) -> Dgm {
    let pad = 8.0;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    let mut take = |x: f32, y: f32| {
        x0 = x0.min(x);
        x1 = x1.max(x);
        y0 = y0.min(y);
        y1 = y1.max(y);
    };
    for b in &d.boxes {
        take(b.left, b.top);
        take(b.right, b.bottom);
    }
    for w in &d.wires {
        for p in &w.pts {
            take(p.0, p.1);
        }
    }
    if !x0.is_finite() {
        (x0, y0, x1, y1) = (0.0, 0.0, 40.0, 0.0);
    }
    if y1 - y0 < 14.0 {
        let m = (y0 + y1) / 2.0;
        y0 = m - 7.0;
        y1 = m + 7.0;
    }
    let (dx, dy) = (pad - x0, pad - y0);
    for b in &mut d.boxes {
        b.left += dx;
        b.right += dx;
        b.top += dy;
        b.bottom += dy;
        b.cy += dy;
        for p in b.ins.iter_mut().chain(b.outs.iter_mut()) {
            p.0 += dx;
            p.1 += dy;
        }
    }
    for w in &mut d.wires {
        for p in &mut w.pts {
            p.0 += dx;
            p.1 += dy;
        }
    }
    for p in d.in_dots.iter_mut().chain(d.out_dots.iter_mut()) {
        p.x += dx;
        p.y += dy;
    }
    d.w = x1 - x0 + 2.0 * pad;
    d.h = y1 - y0 + 2.0 * pad;
    d.xl = d.in_dots.iter().map(|p| p.x).reduce(f32::min);
    d.xr = d.out_dots.iter().map(|p| p.x).reduce(f32::max);
    d
}

/// A wire polyline flattened: level runs are straight, steps are Chyp's
/// tentacle cubic (graphscene.py:172-176: horizontal tangents, control
/// points `k`·dx in; `js/layout/hypergraph.js` `wirePath`).
pub fn wire_points(pts: &[(f32, f32)], k: f32) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = vec![];
    for (i, p) in pts.iter().enumerate() {
        if i == 0 {
            out.push(*p);
            continue;
        }
        let a = pts[i - 1];
        if (a.1 - p.1).abs() < 1e-6 {
            out.push(*p);
            continue;
        }
        let dx = (p.0 - a.0).abs() * k;
        let (c1, c2) = ((a.0 + dx, a.1), (p.0 - dx, p.1));
        for j in 1..=16 {
            let t = j as f32 / 16.0;
            let u = 1.0 - t;
            out.push((
                u * u * u * a.0 + 3.0 * u * u * t * c1.0 + 3.0 * u * t * t * c2.0 + t * t * t * p.0,
                u * u * u * a.1 + 3.0 * u * u * t * c1.1 + 3.0 * u * t * t * c2.1 + t * t * t * p.1,
            ));
        }
    }
    out
}

use crate::compviews::c_hypergraph::{HGraph, Layout};

/// A diagram laid out by `c_hypergraph` (a term Chyp has no coordinates
/// for), in the same shape as an exported one. `label(id)` and `style(label)`
/// give a box its text and fill; every wire is of type `ty`.
pub fn from_layout(
    l: &Layout,
    g: &HGraph,
    label: &dyn Fn(&str) -> String,
    styles: &Value,
    ty: Option<&str>,
) -> Dgm {
    let boxes: Vec<DBox> = g
        .boxes
        .iter()
        .filter_map(|b| {
            let lb = l.boxes.get(&b.id)?;
            let text = label(&b.id);
            let st = &styles[&text];
            Some(DBox {
                id: b.id.clone(),
                label: text,
                left: lb.in_x as f32,
                right: lb.out_x as f32,
                top: (lb.y - lb.h / 2.0) as f32,
                bottom: (lb.y + lb.h / 2.0) as f32,
                cy: lb.y as f32,
                size: BOX_FONT,
                bg: st["bg"].as_str().unwrap_or(DEFAULT_FILL).into(),
                fg: st["fg"].as_str().unwrap_or("#111111").into(),
                inputs: b.inputs.clone(),
                outputs: b.outputs.clone(),
                ins: lb
                    .in_y
                    .iter()
                    .map(|y| (lb.in_x as f32, *y as f32))
                    .collect(),
                outs: lb
                    .out_y
                    .iter()
                    .map(|y| (lb.out_x as f32, *y as f32))
                    .collect(),
                chyp: None,
            })
        })
        .collect();
    let mut wires = vec![];
    for v in &l.vertices {
        let conv = |p: &Vec<crate::compviews::c_hypergraph::Pt>| -> Vec<(f32, f32)> {
            p.iter().map(|q| (q.x as f32, q.y as f32)).collect()
        };
        if let Some(r) = &v.route {
            wires.push(DWire {
                id: v.id.clone(),
                ty: ty.map(str::to_string),
                pts: conv(r),
                route_len: 0,
            });
        } else {
            for p in &v.paths {
                wires.push(DWire {
                    id: v.id.clone(),
                    ty: ty.map(str::to_string),
                    pts: conv(p),
                    route_len: 0,
                });
            }
        }
    }
    let dots = |v: &Vec<(crate::compviews::c_hypergraph::Pt, String)>| -> Vec<Dot> {
        v.iter()
            .map(|(p, id)| Dot {
                x: p.x as f32,
                y: p.y as f32,
                vertex: id.clone(),
            })
            .collect()
    };
    normalize(Dgm {
        boxes,
        wires,
        in_dots: dots(&l.inputs),
        out_dots: dots(&l.outputs),
        w: 0.0,
        h: 0.0,
        xl: None,
        xr: None,
        scale: 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tw(t: &str, size: f32) -> f32 {
        t.chars().count() as f32 * size * 0.6
    }

    #[test]
    fn a_two_box_diagram_keeps_chyps_layers_and_ports() {
        // the export's first graph of sync-spiders frobL: n * id ; id * m
        let g = json!({
            "vertices": [
                {"id":"v0","x":-3.0,"y":-0.5},
                {"id":"v1","x":-0.8,"y":-1.0,"route":[[1.5,-1.0],[3.0,-1.0]]},
                {"id":"v2","x":-0.8,"y":0.0},
                {"id":"v3","x":-3.0,"y":1.0,"route":[[-1.5,1.0],[-0.8,1.0]]},
                {"id":"v7","x":3.0,"y":0.5}],
            "edges": [
                {"id":"e0","label":"n","inputs":["v0"],"outputs":["v1","v2"],"x":-1.5,"y":-0.5,"w":1.1,"h":1.6},
                {"id":"e1","label":"m","inputs":["v2","v3"],"outputs":["v7"],"x":1.5,"y":0.5,"w":1.1,"h":1.6}],
            "inputs": ["v0","v3"], "outputs": ["v1","v7"]});
        let d = from_chyp(
            &g,
            30.0,
            &json!({"n": {"bg": "#000000", "fg": "#ffffff"}}),
            &tw,
        );
        assert_eq!(d.boxes.len(), 2);
        assert_eq!(d.wires.len(), 5);
        // e0's two outputs sit k/(n-1) - 0.5 units from the centre: half a unit up and down
        let b = &d.boxes[0];
        assert!((b.outs[0].1 - (b.cy - 15.0)).abs() < 1e-3);
        assert!((b.outs[1].1 - (b.cy + 15.0)).abs() < 1e-3);
        // e0 sits left of e1, as Chyp's layers say
        assert!(d.boxes[0].right < d.boxes[1].left);
        // the boundary wires start at x of v0 and v3
        assert_eq!(d.in_dots.len(), 2);
        assert_eq!(d.out_dots.len(), 2);
        assert!(d.xl.expect("inputs") < d.boxes[0].left);
        // v2 joins e0's second output to e1's first input: a step, so a cubic
        let v2 = d.wires.iter().find(|w| w.id == "v2").expect("v2");
        assert!(!v2.pts.is_empty());
        // a step is Chyp's tentacle cubic, with horizontal tangents at both ends
        let pts = wire_points(&[(0.0, 0.0), (100.0, 50.0)], 0.4);
        assert!(pts.len() > 2);
        assert!((pts[1].1 - pts[0].1).abs() < 1.0);
        // a level run stays straight
        assert_eq!(wire_points(&[(0.0, 5.0), (100.0, 5.0)], 0.4).len(), 2);
    }

    #[test]
    fn a_long_label_widens_a_box_on_its_free_side() {
        let g = json!({"vertices": [{"id":"a","x":0.0,"y":0.0}], "edges": [
            {"id":"e","label":"immediate_win","inputs":[],"outputs":["a"],"x":0.0,"y":0.0,"w":1.1,"h":0.8}],
            "inputs": [], "outputs": ["a"]});
        let d = from_chyp(&g, 30.0, &json!({}), &tw);
        let b = &d.boxes[0];
        assert!(b.right - b.left > 1.1 * 30.0);
        // the output port stays on the right edge
        assert!((b.outs[0].0 - b.right).abs() < 1e-3);
    }
}

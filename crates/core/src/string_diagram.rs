//! String diagrams: a port of pflow.xyz's String Diagram view
//! (`pflow-xyz/public/diagram-viewer.js`, "dv:" below, at 6241e4b) and of
//! its companion net layout (`_applyStringDiagramLayout`,
//! `public/petri-view.js` pv:6381–6601 at bff33a9).
//!
//! The view reads a Petri net as a monoidal string diagram:
//!
//! - every transition is a box (a morphism), labeled with its id;
//! - every place is a wire type: an output port on the bottom of each box
//!   that produces into it, an input port on top of each box that consumes
//!   from it, and a wire from every producer port to every consumer port;
//! - places nobody produces enter from the top edge, places nobody consumes
//!   leave at the bottom (boundary wires);
//! - wires that would run upward (DFS back edges of the transition graph)
//!   are routed around the right side, dashed red, with an arrowhead.
//!
//! [`translate`] is `translatePetriNet` (dv:383) and returns the same boxes,
//! wires and size; the tests check them against values captured by running
//! the JS under node on the same files. [`drawing`] is `generateSVG`
//! (dv:419), emitting the same elements in the same order as
//! [`DrawShape`]s. Like the JS, the view shows no marking.
//!
//! What a [`DrawShape`] cannot say is approximated, see [`drawing`].

use std::collections::{HashMap, HashSet};

use serde::Serialize;

use crate::artifact::{ArtifactData, DrawShape};
use crate::doc::Document;

// ── Layout constants (dv:8–16) ─────────────────────────────────────
pub const BOX_HEIGHT: f64 = 40.0;
pub const BOX_MIN_WIDTH: f64 = 80.0;
pub const PORT_SPACING: f64 = 24.0;
pub const LAYER_GAP: f64 = 80.0;
pub const BOX_H_GAP: f64 = 40.0;
pub const PORT_RADIUS: f64 = 4.0;
pub const PADDING: f64 = 30.0;
pub const FONT_SIZE: f64 = 12.0;
pub const LABEL_FONT_SIZE: f64 = 10.0;

// ── Colors used by generateSVG (dv:419–543), as 6-digit hex ────────
const BACKGROUND: &str = "#fafafa";
const WIRE: &str = "#888888";
const PORT: &str = "#666666";
const LABEL: &str = "#666666";
const ALERT: &str = "#cc4444"; // "#c44": inhibitors, weights, back edges
const BOX_FILL: &str = "#ffffff";
const BOX_INK: &str = "#333333";
/// Wire stroke width (`stroke-width="1.5"` throughout dv:455–537).
const WIRE_WIDTH: f64 = 1.5;
/// Points per flattened cubic Bézier (a [`DrawShape::Line`] is a polyline).
const BEZIER_SEGMENTS: usize = 16;
/// `dominant-baseline="central"` (dv:508) as a baseline shift, in em: a
/// [`DrawShape::Text`] `y` is the alphabetic baseline.
const CENTRAL_DY: f64 = 0.35;

/// One typed port of a box.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Port {
    /// The place this port carries.
    pub wire: String,
    /// The arc's first-color weight (`arc.weight[0]`), if it has one.
    pub weight: Option<f64>,
    /// An inhibitor input (always false on outputs, dv:43).
    pub inhibit: bool,
}

/// A transition drawn as a box (dv:47).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DiagramBox {
    pub id: String,
    /// The transition id, as in the JS (dv:49), not its display label.
    pub label: String,
    /// Inputs (top) and outputs (bottom), in arc order.
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub layer: usize,
    pub pos_in_layer: usize,
    pub x: f64,
    pub y: f64,
    pub width: f64,
}

/// A wire from a producer's output port to a consumer's input port (dv:325).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Wire {
    pub wire: String,
    pub from: (f64, f64),
    pub to: (f64, f64),
    /// Producer → consumer is a back edge: routed around the right side.
    pub back: bool,
    /// The consumer's arc is an inhibitor.
    pub inhibit: bool,
    /// The producer's arc weight (first color).
    pub weight: Option<f64>,
}

/// `translatePetriNet`'s result (dv:407).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Diagram {
    pub boxes: Vec<DiagramBox>,
    pub wires: Vec<Wire>,
    /// Places consumed but never produced: they enter from the top.
    pub boundary_inputs: Vec<String>,
    /// Places produced but never consumed: they leave at the bottom.
    pub boundary_outputs: Vec<String>,
    /// Back edges of the transition graph, in discovery order.
    pub back_edges: Vec<(String, String)>,
    pub width: f64,
    pub height: f64,
}

/// The transition graph mediated by places (dv:65, pv:6388), with its
/// cycles broken, layered and crossing-minimized: the phases the view and
/// the net layout share. Indices are into `transitions` / `places`, both
/// in JS `Object.keys` order.
struct Dag<'a> {
    places: Vec<&'a str>,
    transitions: Vec<&'a str>,
    /// Per place: transitions producing into it / consuming from it, one
    /// entry per arc.
    producers: Vec<Vec<usize>>,
    consumers: Vec<Vec<usize>>,
    back: HashSet<(usize, usize)>,
    back_order: Vec<(usize, usize)>,
    level: Vec<usize>,
    /// Transitions per layer, in barycenter order.
    layers: Vec<Vec<usize>>,
}

impl<'a> Dag<'a> {
    fn new(doc: &'a Document) -> Self {
        let places = js_key_order(doc.places.iter().map(|p| p.id.as_str()));
        let transitions = js_key_order(doc.transitions.iter().map(|t| t.id.as_str()));
        let p_index: HashMap<&str, usize> =
            places.iter().enumerate().map(|(i, &p)| (p, i)).collect();
        let t_index: HashMap<&str, usize> = transitions
            .iter()
            .enumerate()
            .map(|(i, &t)| (t, i))
            .collect();
        let (np, nt) = (places.len(), transitions.len());

        // Producers and consumers per place, one entry per arc (dv:78).
        let mut producers = vec![vec![]; np];
        let mut consumers = vec![vec![]; np];
        for arc in &doc.arcs {
            if let (Some(&t), Some(&p)) = (
                t_index.get(arc.source.as_str()),
                p_index.get(arc.target.as_str()),
            ) {
                producers[p].push(t);
            }
            if let (Some(&p), Some(&t)) = (
                p_index.get(arc.source.as_str()),
                t_index.get(arc.target.as_str()),
            ) {
                consumers[p].push(t);
            }
        }

        // T1 → T2 whenever T1 produces into a place T2 consumes (dv:94).
        let mut outgoing = vec![vec![]; nt];
        let mut incoming = vec![vec![]; nt];
        for p in 0..np {
            for &src in &producers[p] {
                for &tgt in &consumers[p] {
                    if src != tgt {
                        outgoing[src].push(tgt);
                        incoming[tgt].push(src);
                    }
                }
            }
        }

        // DFS cycle breaking (dv:109): an edge to a node still on the
        // stack is a back edge. The JS recurses; this explicit stack
        // visits in the same order.
        let mut visited = vec![false; nt];
        let mut on_stack = vec![false; nt];
        let mut back = HashSet::new();
        let mut back_order = vec![];
        for root in 0..nt {
            if visited[root] {
                continue;
            }
            visited[root] = true;
            on_stack[root] = true;
            let mut stack = vec![(root, 0usize)];
            while let Some(top) = stack.last_mut() {
                let (u, next) = *top;
                if let Some(&t) = outgoing[u].get(next) {
                    top.1 += 1;
                    if !visited[t] {
                        visited[t] = true;
                        on_stack[t] = true;
                        stack.push((t, 0));
                    } else if on_stack[t] && back.insert((u, t)) {
                        back_order.push((u, t));
                    }
                } else {
                    on_stack[u] = false;
                    stack.pop();
                }
            }
        }

        // Longest-path layering over forward edges (dv:131); anything
        // unreached lands on layer 0.
        let mut lv = vec![-1i64; nt];
        for v in 0..nt {
            if incoming[v].iter().all(|&s| back.contains(&(s, v))) {
                lv[v] = 0;
            }
        }
        let mut changed = true;
        while changed {
            changed = false;
            for u in 0..nt {
                if lv[u] < 0 {
                    continue;
                }
                for &t in &outgoing[u] {
                    if back.contains(&(u, t)) {
                        continue;
                    }
                    if lv[u] + 1 > lv[t] {
                        lv[t] = lv[u] + 1;
                        changed = true;
                    }
                }
            }
        }
        let level: Vec<usize> = lv.iter().map(|&l| l.max(0) as usize).collect();

        // Barycenter crossing minimization (dv:172): 4 passes of a down
        // sweep then an up sweep; a node with no neighbor in the adjacent
        // layer keeps its index. Sorts are stable, as `Array.sort` is.
        let max_layer = level.iter().copied().max().unwrap_or(0);
        let mut layers: Vec<Vec<usize>> = vec![vec![]; max_layer + 1];
        for v in 0..nt {
            layers[level[v]].push(v);
        }
        let mut pos_of = vec![0.0f64; nt];
        for layer in &layers {
            for (i, &v) in layer.iter().enumerate() {
                pos_of[v] = i as f64;
            }
        }
        let mut bary = vec![0.0f64; nt];
        let barycenter = |v: usize,
                          nbrs: &[usize],
                          edge: &dyn Fn(usize) -> (usize, usize),
                          want: usize,
                          pos_of: &[f64]| {
            let (mut sum, mut count) = (0.0, 0usize);
            for &w in nbrs {
                if back.contains(&edge(w)) || level[w] != want {
                    continue;
                }
                sum += pos_of[w];
                count += 1;
            }
            if count > 0 {
                sum / count as f64
            } else {
                pos_of[v]
            }
        };
        for _pass in 0..4 {
            for (lvl, layer) in layers.iter_mut().enumerate().skip(1) {
                for &v in layer.iter() {
                    bary[v] = barycenter(v, &incoming[v], &|s| (s, v), lvl - 1, &pos_of);
                }
                sort_layer(layer, &bary, &mut pos_of);
            }
            for (lvl, layer) in layers.iter_mut().enumerate().take(max_layer).rev() {
                for &v in layer.iter() {
                    bary[v] = barycenter(v, &outgoing[v], &|t| (v, t), lvl + 1, &pos_of);
                }
                sort_layer(layer, &bary, &mut pos_of);
            }
        }

        Dag {
            places,
            transitions,
            producers,
            consumers,
            back,
            back_order,
            level,
            layers,
        }
    }
}

fn sort_layer(layer: &mut [usize], bary: &[f64], pos_of: &mut [f64]) {
    layer.sort_by(|&a, &b| bary[a].total_cmp(&bary[b]));
    for (i, &v) in layer.iter().enumerate() {
        pos_of[v] = i as f64;
    }
}

/// `translatePetriNet` (dv:383): boxes, wires, boundaries and size. A net
/// with no transitions gives an empty diagram of size 0 × 0 (dv:387).
pub fn translate(doc: &Document) -> Diagram {
    let dag = Dag::new(doc);
    if dag.transitions.is_empty() {
        return Diagram {
            boxes: vec![],
            wires: vec![],
            boundary_inputs: vec![],
            boundary_outputs: vec![],
            back_edges: vec![],
            width: 0.0,
            height: 0.0,
        };
    }
    let is_place: HashSet<&str> = dag.places.iter().copied().collect();

    // Phase 1, buildBoxes (dv:20): ports in arc order.
    let mut boxes: Vec<DiagramBox> = dag
        .transitions
        .iter()
        .enumerate()
        .map(|(i, &tid)| {
            let mut inputs = vec![];
            let mut outputs = vec![];
            for arc in &doc.arcs {
                if arc.target == tid && is_place.contains(arc.source.as_str()) {
                    inputs.push(Port {
                        wire: arc.source.clone(),
                        weight: arc.weight.first().copied(),
                        inhibit: arc.inhibit,
                    });
                }
                if arc.source == tid && is_place.contains(arc.target.as_str()) {
                    outputs.push(Port {
                        wire: arc.target.clone(),
                        weight: arc.weight.first().copied(),
                        inhibit: false,
                    });
                }
            }
            DiagramBox {
                id: tid.to_string(),
                label: tid.to_string(),
                inputs,
                outputs,
                layer: dag.level[i],
                pos_in_layer: 0,
                x: 0.0,
                y: 0.0,
                width: 0.0,
            }
        })
        .collect();
    for layer in &dag.layers {
        for (i, &v) in layer.iter().enumerate() {
            boxes[v].pos_in_layer = i;
        }
    }

    // Phase 6, computePositions (dv:236): width from the label (UTF-16
    // length, as `String.length`) and port counts; each layer is packed
    // left to right from PADDING (the JS computes a total width but does
    // not center on it).
    for b in &mut boxes {
        let label_width = b.label.encode_utf16().count() as f64 * (FONT_SIZE * 0.65) + 20.0;
        let inputs_width = b.inputs.len() as f64 * PORT_SPACING;
        let outputs_width = b.outputs.len() as f64 * PORT_SPACING;
        b.width = BOX_MIN_WIDTH
            .max(label_width)
            .max(inputs_width)
            .max(outputs_width);
    }
    for (lvl, layer) in dag.layers.iter().enumerate() {
        let mut x_cursor = PADDING;
        let y = PADDING + lvl as f64 * (BOX_HEIGHT + LAYER_GAP);
        for &v in layer {
            boxes[v].x = x_cursor;
            boxes[v].y = y;
            x_cursor += boxes[v].width + BOX_H_GAP;
        }
    }

    // routeWires (dv:272): port positions keyed by place, the last port
    // for a place winning as in the JS object maps.
    let mut out_pos: Vec<HashMap<&str, (f64, f64)>> = vec![HashMap::new(); boxes.len()];
    let mut in_pos: Vec<HashMap<&str, (f64, f64)>> = vec![HashMap::new(); boxes.len()];
    for (v, b) in boxes.iter().enumerate() {
        for (i, p) in b.outputs.iter().enumerate() {
            out_pos[v].insert(
                p.wire.as_str(),
                (port_x(b, i, b.outputs.len()), b.y + BOX_HEIGHT),
            );
        }
        for (i, p) in b.inputs.iter().enumerate() {
            in_pos[v].insert(p.wire.as_str(), (port_x(b, i, b.inputs.len()), b.y));
        }
    }
    let t_index: HashMap<&str, usize> = dag
        .transitions
        .iter()
        .enumerate()
        .map(|(i, &t)| (t, i))
        .collect();
    let mut wires = vec![];
    for &pid in &dag.places {
        let producers: Vec<_> = doc
            .arcs
            .iter()
            .filter(|a| a.target == pid)
            .filter_map(|a| t_index.get(a.source.as_str()).map(|&t| (t, a)))
            .collect();
        let consumers: Vec<_> = doc
            .arcs
            .iter()
            .filter(|a| a.source == pid)
            .filter_map(|a| t_index.get(a.target.as_str()).map(|&t| (t, a)))
            .collect();
        for &(prod, prod_arc) in &producers {
            for &(cons, cons_arc) in &consumers {
                if let (Some(&from), Some(&to)) = (out_pos[prod].get(pid), in_pos[cons].get(pid)) {
                    wires.push(Wire {
                        wire: pid.to_string(),
                        from,
                        to,
                        back: dag.back.contains(&(prod, cons)),
                        inhibit: cons_arc.inhibit,
                        weight: prod_arc.weight.first().copied(),
                    });
                }
            }
        }
    }
    // dv:341 also flags crossing wires, but generateSVG never reads the
    // flag, so it is not computed here.

    // findBoundaries (dv:360).
    let mut boundary_inputs = vec![];
    let mut boundary_outputs = vec![];
    for (p, &pid) in dag.places.iter().enumerate() {
        let produced = !dag.producers[p].is_empty();
        let consumed = !dag.consumers[p].is_empty();
        if consumed && !produced {
            boundary_inputs.push(pid.to_string());
        }
        if produced && !consumed {
            boundary_outputs.push(pid.to_string());
        }
    }

    // Size (dv:401).
    let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
    for b in &boxes {
        max_x = max_x.max(b.x + b.width);
        max_y = max_y.max(b.y + BOX_HEIGHT);
    }
    let width = max_x + PADDING;
    let height = max_y
        + PADDING
        + if boundary_outputs.is_empty() {
            0.0
        } else {
            LAYER_GAP
        };

    let name = |v: usize| dag.transitions[v].to_string();
    Diagram {
        boxes,
        wires,
        boundary_inputs,
        boundary_outputs,
        back_edges: dag
            .back_order
            .iter()
            .map(|&(a, b)| (name(a), name(b)))
            .collect(),
        width,
        height,
    }
}

/// `getPortX` (dv:265): ports centered on the box, PORT_SPACING apart.
pub fn port_x(b: &DiagramBox, index: usize, count: usize) -> f64 {
    if count == 0 {
        return b.x + b.width / 2.0;
    }
    let total_span = (count as f64 - 1.0) * PORT_SPACING;
    let start_x = b.x + (b.width - total_span) / 2.0;
    start_x + index as f64 * PORT_SPACING
}

/// `generateSVG` (dv:419) as a [`ArtifactData::Drawing`]: the same
/// elements in the same order and coordinates, at the same size
/// (`max(width, 200)` × `max(height, 100)`).
///
/// Approximations, where a [`DrawShape`] has no equivalent:
/// - Cubic Bézier wires are flattened to [`BEZIER_SEGMENTS`]-segment
///   polylines (straight ones to their two end points).
/// - The back-edge arrowhead marker (dv:435, a filled triangle scaled by
///   the stroke width) is drawn as a closed triangle outline.
/// - Dashes are the renderer's own pattern (SVG `4,3` / `6,3` in the JS).
/// - Box and inhibitor-port outlines use the renderer's stroke width (1.5
///   in the JS), and the bold weight labels are regular.
/// - Text `dy` offsets are folded into `y`; the box label's
///   `dominant-baseline="central"` becomes a baseline shift of 0.35 em.
pub fn drawing(d: &Diagram) -> ArtifactData {
    let svg_width = d.width.max(200.0);
    let svg_height = d.height.max(100.0);
    let mut shapes = vec![DrawShape::Rect {
        x: 0.0,
        y: 0.0,
        w: svg_width,
        h: svg_height,
        fill: Some(BACKGROUND.into()),
        stroke: None,
        radius: 4.0,
    }];
    let text = |x: f64, y: f64, s: &str, size: f64, anchor: &str, color: &str| DrawShape::Text {
        x,
        y,
        text: s.to_string(),
        size,
        anchor: anchor.into(),
        color: color.into(),
    };
    let input_port = |x: f64, y: f64, inhibit: bool| DrawShape::Circle {
        x,
        y,
        r: PORT_RADIUS,
        fill: (!inhibit).then(|| PORT.into()),
        stroke: inhibit.then(|| ALERT.into()),
    };

    // Boundary inputs entering from the top (dv:442).
    for pid in &d.boundary_inputs {
        for b in &d.boxes {
            for (i, port) in b.inputs.iter().enumerate() {
                if &port.wire != pid {
                    continue;
                }
                let px = port_x(b, i, b.inputs.len());
                let py = b.y;
                let start_y = (py - LAYER_GAP / 2.0).max(0.0);
                shapes.push(bezier(
                    [
                        (px, start_y),
                        (px, start_y + 15.0),
                        (px, py - 15.0),
                        (px, py),
                    ],
                    WIRE,
                    port.inhibit,
                ));
                shapes.push(text(
                    px,
                    start_y + (py - start_y) / 2.0 - 3.0,
                    pid,
                    LABEL_FONT_SIZE,
                    "middle",
                    LABEL,
                ));
                shapes.push(input_port(px, py, port.inhibit));
            }
        }
    }

    // Forward wires (dv:471), labeled at the midpoint, with the weight in
    // red beside it when above 1.
    for w in d.wires.iter().filter(|w| !w.back) {
        let ((fx, fy), (tx, ty)) = (w.from, w.to);
        let mid_y = (fy + ty) / 2.0;
        let color = if w.inhibit { ALERT } else { WIRE };
        shapes.push(bezier(
            [(fx, fy), (fx, mid_y), (tx, mid_y), (tx, ty)],
            color,
            w.inhibit,
        ));
        let label_x = (fx + tx) / 2.0;
        shapes.push(text(
            label_x,
            mid_y - 4.0,
            &w.wire,
            LABEL_FONT_SIZE,
            "middle",
            LABEL,
        ));
        if let Some(wt) = w.weight.filter(|&wt| wt > 1.0) {
            shapes.push(text(
                label_x + 8.0,
                mid_y + 8.0,
                &fmt_weight(wt),
                LABEL_FONT_SIZE,
                "start",
                ALERT,
            ));
        }
    }

    // Back edges, around the right side (dv:491).
    for w in d.wires.iter().filter(|w| w.back) {
        let ((fx, fy), (tx, ty)) = (w.from, w.to);
        let right_x = svg_width - PADDING / 2.0;
        let (mid_y1, mid_y2) = (fy + 20.0, ty - 20.0);
        let mid = (fy + ty) / 2.0;
        shapes.push(bezier(
            [
                (fx, fy),
                (fx + 30.0, mid_y1),
                (right_x, mid_y1),
                (right_x, mid),
            ],
            ALERT,
            true,
        ));
        let leg = [
            (right_x, mid),
            (right_x, mid_y2),
            (tx + 30.0, mid_y2),
            (tx, ty),
        ];
        shapes.push(bezier(leg, ALERT, true));
        shapes.push(arrowhead(leg, ALERT));
        shapes.push(text(
            right_x - 4.0,
            mid - 3.0,
            &w.wire,
            LABEL_FONT_SIZE,
            "end",
            ALERT,
        ));
    }

    // Boxes and their ports (dv:504).
    for b in &d.boxes {
        shapes.push(DrawShape::Rect {
            x: b.x,
            y: b.y,
            w: b.width,
            h: BOX_HEIGHT,
            fill: Some(BOX_FILL.into()),
            stroke: Some(BOX_INK.into()),
            radius: 6.0,
        });
        shapes.push(text(
            b.x + b.width / 2.0,
            b.y + BOX_HEIGHT / 2.0 + CENTRAL_DY * FONT_SIZE,
            &b.label,
            FONT_SIZE,
            "middle",
            BOX_INK,
        ));
        for (i, port) in b.inputs.iter().enumerate() {
            shapes.push(input_port(port_x(b, i, b.inputs.len()), b.y, port.inhibit));
        }
        for i in 0..b.outputs.len() {
            shapes.push(input_port(
                port_x(b, i, b.outputs.len()),
                b.y + BOX_HEIGHT,
                false,
            ));
        }
    }

    // Boundary outputs leaving at the bottom (dv:529).
    for pid in &d.boundary_outputs {
        for b in &d.boxes {
            for (i, port) in b.outputs.iter().enumerate() {
                if &port.wire != pid {
                    continue;
                }
                let px = port_x(b, i, b.outputs.len());
                let py = b.y + BOX_HEIGHT;
                let end_y = py + LAYER_GAP / 2.0;
                shapes.push(bezier(
                    [(px, py), (px, py + 15.0), (px, end_y - 15.0), (px, end_y)],
                    WIRE,
                    false,
                ));
                shapes.push(text(
                    px,
                    py + (end_y - py) / 2.0 - 3.0,
                    pid,
                    LABEL_FONT_SIZE,
                    "middle",
                    LABEL,
                ));
            }
        }
    }

    ArtifactData::Drawing {
        width: svg_width,
        height: svg_height,
        shapes,
    }
}

/// The string diagram of `doc` as a drawing: [`drawing`] of [`translate`].
pub fn string_diagram(doc: &Document) -> ArtifactData {
    drawing(&translate(doc))
}

/// A cubic Bézier as a polyline; a straight one (all control points on one
/// vertical or horizontal line) as its two end points.
fn bezier(c: [(f64, f64); 4], color: &str, dashed: bool) -> DrawShape {
    let straight = c.iter().all(|p| p.0 == c[0].0) || c.iter().all(|p| p.1 == c[0].1);
    let points = if straight {
        vec![c[0], c[3]]
    } else {
        (0..=BEZIER_SEGMENTS)
            .map(|i| {
                let t = i as f64 / BEZIER_SEGMENTS as f64;
                let u = 1.0 - t;
                let (a, b, cc, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                (
                    a * c[0].0 + b * c[1].0 + cc * c[2].0 + d * c[3].0,
                    a * c[0].1 + b * c[1].1 + cc * c[2].1 + d * c[3].1,
                )
            })
            .collect()
    };
    DrawShape::Line {
        points,
        color: color.into(),
        width: WIRE_WIDTH,
        dashed,
    }
}

/// The `arrowhead-back` marker (dv:435) at the end of a Bézier: the
/// triangle `0,0 8,3 0,6` with its tip on the end point, pointing along the
/// end tangent, scaled by the stroke width (SVG `markerUnits` default).
fn arrowhead(c: [(f64, f64); 4], color: &str) -> DrawShape {
    let tip = c[3];
    let from = if c[2] != tip { c[2] } else { c[1] };
    let (dx, dy) = (tip.0 - from.0, tip.1 - from.1);
    let len = dx.hypot(dy);
    let (ux, uy) = if len > 0.0 {
        (dx / len, dy / len)
    } else {
        (0.0, 1.0)
    };
    let (back, half) = (8.0 * WIRE_WIDTH, 3.0 * WIRE_WIDTH);
    let base = (tip.0 - ux * back, tip.1 - uy * back);
    let left = (base.0 - uy * half, base.1 + ux * half);
    let right = (base.0 + uy * half, base.1 - ux * half);
    DrawShape::Line {
        points: vec![tip, left, right, tip],
        color: color.into(),
        width: WIRE_WIDTH,
        dashed: false,
    }
}

/// A weight as JS prints a number: no trailing `.0`.
fn fmt_weight(w: f64) -> String {
    if w.fract() == 0.0 && w.abs() < 1e15 {
        format!("{}", w as i64)
    } else {
        format!("{w}")
    }
}

/// `_applyStringDiagramLayout` (pv:6381): positions every place and
/// transition of the net itself as a string diagram, transitions on layers
/// 180 apart (centered on x = 400, 120 apart, from y = 100) and each place
/// on its wire between producers and consumers, then shifted so nothing is
/// below 100 (`_normalizeNodePositions(100)`, pv:5960). Returns `(id, x, y)`
/// for places then transitions, in pflow.xyz's iteration order, as
/// [`crate::layout::layout`] does.
pub fn net_layout(doc: &Document) -> Vec<(String, f64, f64)> {
    const LAYER_SPACING: f64 = 180.0;
    const BOX_SPACING: f64 = 120.0;
    const START_Y: f64 = 100.0;
    let dag = Dag::new(doc);
    if dag.places.is_empty() && dag.transitions.is_empty() {
        return vec![];
    }
    let max_layer = dag.layers.len() - 1;

    // Transitions (pv:6531).
    let mut t_pos = vec![(0.0, 0.0); dag.transitions.len()];
    for (lvl, layer) in dag.layers.iter().enumerate() {
        let total_width = (layer.len() as f64 - 1.0) * BOX_SPACING;
        let start_x = 400.0 - total_width / 2.0;
        for (i, &v) in layer.iter().enumerate() {
            t_pos[v] = (
                js_round(start_x + i as f64 * BOX_SPACING),
                js_round(START_Y + lvl as f64 * LAYER_SPACING),
            );
        }
    }

    // Places along their wires (pv:6547).
    let lvl = |t: usize| dag.level[t] as f64;
    let mean = |ts: &[usize], f: &dyn Fn(usize) -> f64| {
        ts.iter().map(|&t| f(t)).sum::<f64>() / ts.len() as f64
    };
    let p_pos: Vec<(f64, f64)> = (0..dag.places.len())
        .map(|p| {
            let (ins, outs) = (&dag.producers[p], &dag.consumers[p]);
            let x_of = |t: usize| t_pos[t].0;
            match (ins.is_empty(), outs.is_empty()) {
                (false, false) => {
                    let mid_layer = (mean(ins, &lvl) + mean(outs, &lvl)) / 2.0;
                    (
                        js_round((mean(ins, &x_of) + mean(outs, &x_of)) / 2.0),
                        js_round(START_Y + mid_layer * LAYER_SPACING),
                    )
                }
                (false, true) => {
                    let top = ins.iter().map(|&t| dag.level[t]).max().unwrap_or(0);
                    (
                        js_round(mean(ins, &x_of)),
                        js_round(START_Y + (top as f64 + 0.5) * LAYER_SPACING),
                    )
                }
                (true, false) => {
                    let bottom = outs
                        .iter()
                        .map(|&t| dag.level[t])
                        .fold(max_layer, usize::min);
                    (
                        js_round(mean(outs, &x_of)),
                        js_round(START_Y + (bottom as f64 - 0.5) * LAYER_SPACING),
                    )
                }
                (true, true) => (
                    400.0,
                    js_round(START_Y + (max_layer as f64 + 1.5) * LAYER_SPACING),
                ),
            }
        })
        .collect();

    // _normalizeNodePositions(100) (pv:6597, defined at pv:5960).
    let all = p_pos.iter().chain(&t_pos);
    let min_x = all.clone().map(|p| p.0).fold(f64::INFINITY, f64::min);
    let min_y = all.map(|p| p.1).fold(f64::INFINITY, f64::min);
    let dx = if min_x < 100.0 { 100.0 - min_x } else { 0.0 };
    let dy = if min_y < 100.0 { 100.0 - min_y } else { 0.0 };
    dag.places
        .iter()
        .zip(&p_pos)
        .chain(dag.transitions.iter().zip(&t_pos))
        .map(|(id, &(x, y))| (id.to_string(), x + dx, y + dy))
        .collect()
}

/// JS own-property order: array-index keys ascending by value, then the
/// rest in insertion order (the same rule as `layout.rs`).
fn js_key_order<'a>(ids: impl Iterator<Item = &'a str>) -> Vec<&'a str> {
    let index = |id: &str| -> Option<u32> {
        let v: u32 = id.parse().ok()?;
        (v != u32::MAX && v.to_string() == id).then_some(v)
    };
    let mut ints = vec![];
    let mut rest = vec![];
    for id in ids {
        match index(id) {
            Some(v) => ints.push((v, id)),
            None => rest.push(id),
        }
    }
    ints.sort_by_key(|&(v, _)| v);
    ints.into_iter().map(|(_, id)| id).chain(rest).collect()
}

/// `Math.round`: halves round toward +∞.
fn js_round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 {
        f + 1.0
    } else {
        f
    }
}

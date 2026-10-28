//! The string view, transition level: diagram-viewer.js's drawing of the
//! flat net (orders) and of the Connect-4 net, from `core::string_diagram`
//! (a port checked number for number against the JS, see
//! `crates/core/tests/diagram_viewer_fixtures.rs`). Boxes are transitions,
//! places are wire types, back edges dashed red round the right side.
//!
//! Orders: the flat net `bundle.flatten` returns, the transition ports of
//! the pattern overlaid on each box so event links can be dragged here too.
//! Connect-4: 222 boxes in 150 layers is too large to read on one stage, so
//! a scaled overview in five columns and a 1:1 lens, as the prototype does.

use std::sync::OnceLock;

use pflow_native_core::string_diagram::{self as sd, Diagram};

use super::b_draw::{dv, theme, *};
use super::b_orders::{dv_cite, go_cite, hover_path, snap_port, DV_REV};
use super::*;

const OX: f32 = 24.0;
const OY: f32 = 64.0;

// ------------------------------------------------------------ the toggle

/// The two level buttons above the drawing (`view.composition {level}`),
/// at the top of `area`; returns the area below them.
pub(super) fn toggle(c: &mut Ctx, area: Rect, orders: bool) -> Rect {
    let h = 24.0;
    let mut x = area.left() + 2.0;
    let y = area.top() + 2.0;
    for (id, label) in [
        ("pattern", "pattern level"),
        ("transition", "transition level (diagram-viewer.js)"),
    ] {
        let w = label.len() as f32 * 6.6 + 22.0;
        let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
        c.button(
            r,
            label,
            c.level == id,
            "view.composition",
            json!({"level": id}),
        );
        c.hover(
            r,
            info(format!("string view: {label}"))
                .fact(
                    "toggle",
                    "view.composition {level: \"pattern\" | \"transition\"}; the default is pattern",
                )
                .note(if id == "pattern" {
                    "This view's own drawing of the instances (orders) or of the plan (Connect-4)."
                } else {
                    "diagram-viewer.js's drawing of the flat net (orders) or of the Connect-4 net: every transition a box, every place a wire type."
                }),
        );
        x += w + 4.0;
    }
    let hint = match (c.level.as_str(), orders) {
        ("pattern", true) => "drag from any port: only the links go-pflow's validateLink allows light up",
        ("pattern", false) => "drag a palette piece onto a slot: plan.rs's check decides",
        (_, true) => "drawn by diagram-viewer.js's algorithm (core::string_diagram); the squares are transition ports you can drag",
        (_, false) => "diagram-viewer.js's algorithm; click or drag the overview to move the lens",
    };
    c.text(
        Pos2::new(x + 8.0, y + h / 2.0),
        Align2::LEFT_CENTER,
        hint,
        11.0,
        theme::MUTED,
    );
    Rect::from_min_max(Pos2::new(area.left(), y + h + 8.0), area.right_bottom())
}

// ------------------------------------------------------------ drawing a Diagram

/// Draws `diag` as generateSVG does (dv:419-543), restricted to the design y
/// range `rows` when given (the lens, the overview columns).
fn draw_diagram(d: &D, diag: &Diagram, rows: Option<(f32, f32)>, simple: bool) {
    let (svg_w, _) = diag.svg_size();
    let vis = |y0: f64, y1: f64| {
        rows.is_none_or(|(a, b)| y1 as f32 >= a - 50.0 && (y0 as f32) <= b + 50.0)
    };
    let f = |x: f64, y: f64| (x as f32, y as f32);
    // boundary stubs
    for s in diag.stubs().iter().filter(|s| s.input) {
        if !vis(s.y_free, s.y_port) {
            continue;
        }
        let dash = s.inhibit.then_some((4.0, 3.0));
        if !simple {
            d.cubic(
                [
                    f(s.x, s.y_free),
                    f(s.x, s.y_free + 15.0),
                    f(s.x, s.y_port - 15.0),
                    f(s.x, s.y_port),
                ],
                dv::WIRE,
                1.5,
                dash,
            );
            d.text(
                f(s.x, s.y_free + (s.y_port - s.y_free) / 2.0 - 3.0 + 3.0),
                Align2::CENTER_BOTTOM,
                &s.wire,
                dv::LABEL,
                dv::LABEL_FILL,
                false,
            );
            if s.inhibit {
                d.disc(
                    f(s.x, s.y_port),
                    dv::PORT_R,
                    Color32::TRANSPARENT,
                    dv::BACK,
                    1.5,
                );
            } else {
                d.dot(f(s.x, s.y_port), dv::PORT_R, dv::PORT);
            }
        } else {
            d.line(f(s.x, s.y_free), f(s.x, s.y_port), dv::WIRE, 1.0);
        }
    }
    // forward wires
    for w in diag.wires.iter().filter(|w| !w.back) {
        if !vis(w.from.1.min(w.to.1), w.from.1.max(w.to.1)) {
            continue;
        }
        let col = if w.inhibit { dv::BACK } else { dv::WIRE };
        if simple {
            d.line(f(w.from.0, w.from.1), f(w.to.0, w.to.1), col, 1.0);
            continue;
        }
        d.cubic(
            w.forward_curve().map(|p| f(p.0, p.1)),
            col,
            1.5,
            w.inhibit.then_some((4.0, 3.0)),
        );
        let mid_y = (w.from.1 + w.to.1) / 2.0;
        let lx = (w.from.0 + w.to.0) / 2.0;
        d.text(
            f(lx, mid_y - 4.0 + 3.0),
            Align2::CENTER_BOTTOM,
            &w.wire,
            dv::LABEL,
            dv::LABEL_FILL,
            false,
        );
        if let Some(wt) = w.weight.filter(|&x| x > 1.0) {
            d.text(
                f(lx + 8.0, mid_y + 8.0 + 3.0),
                Align2::LEFT_BOTTOM,
                &format!("{wt}"),
                dv::LABEL,
                dv::BACK,
                false,
            );
        }
    }
    // back edges, round the right side
    for w in diag.wires.iter().filter(|w| w.back) {
        let (a, b, lab) = w.back_curves(svg_w);
        if !vis(w.from.1.min(w.to.1), w.from.1.max(w.to.1)) {
            continue;
        }
        if simple {
            let soft = dv::BACK.gamma_multiply(0.3);
            d.line(
                f(w.from.0, w.from.1),
                f(svg_w - 15.0, (w.from.1 + w.to.1) / 2.0),
                soft,
                0.8,
            );
            d.line(
                f(svg_w - 15.0, (w.from.1 + w.to.1) / 2.0),
                f(w.to.0, w.to.1),
                soft,
                0.8,
            );
            continue;
        }
        d.cubic(a.map(|p| f(p.0, p.1)), dv::BACK, 1.5, Some(dv::BACK_DASH));
        d.cubic(b.map(|p| f(p.0, p.1)), dv::BACK, 1.5, Some(dv::BACK_DASH));
        d.arrow(f(b[3].0, b[3].1), f(b[2].0, b[2].1), dv::BACK);
        d.text(
            f(lab.0, lab.1 + 3.0),
            Align2::RIGHT_BOTTOM,
            &w.wire,
            dv::LABEL,
            dv::BACK,
            false,
        );
    }
    // boxes and their ports
    for b in &diag.boxes {
        if !vis(b.y, b.y + sd::BOX_HEIGHT) {
            continue;
        }
        let (x, y, w) = (b.x as f32, b.y as f32, b.width as f32);
        d.rect(
            x,
            y,
            w,
            dv::BOX_H,
            dv::RX,
            Color32::WHITE,
            dv::STROKE,
            dv::SW,
            None,
        );
        if simple {
            continue;
        }
        d.text(
            (x + w / 2.0, y + dv::BOX_H / 2.0),
            Align2::CENTER_CENTER,
            &b.label,
            sd::FONT_SIZE as f32,
            dv::STROKE,
            false,
        );
        for (i, p) in b.inputs.iter().enumerate() {
            let at = f(sd::port_x(b, i, b.inputs.len()), b.y);
            if p.inhibit {
                d.disc(at, dv::PORT_R, Color32::TRANSPARENT, dv::BACK, 1.5);
            } else {
                d.dot(at, dv::PORT_R, dv::PORT);
            }
        }
        for i in 0..b.outputs.len() {
            d.dot(
                f(sd::port_x(b, i, b.outputs.len()), b.y + sd::BOX_HEIGHT),
                dv::PORT_R,
                dv::PORT,
            );
        }
    }
    // boundary outputs
    for s in diag.stubs().iter().filter(|s| !s.input) {
        if !vis(s.y_port, s.y_free) {
            continue;
        }
        if simple {
            d.line(f(s.x, s.y_port), f(s.x, s.y_free), dv::WIRE, 1.0);
            continue;
        }
        d.cubic(
            [
                f(s.x, s.y_port),
                f(s.x, s.y_port + 15.0),
                f(s.x, s.y_free - 15.0),
                f(s.x, s.y_free),
            ],
            dv::WIRE,
            1.5,
            None,
        );
        d.text(
            f(s.x, s.y_port + (s.y_free - s.y_port) / 2.0 - 3.0 + 3.0),
            Align2::CENTER_BOTTOM,
            &s.wire,
            dv::LABEL,
            dv::LABEL_FILL,
            false,
        );
    }
}

fn layers(d: &Diagram) -> usize {
    d.boxes.iter().map(|b| b.layer).max().map_or(0, |m| m + 1)
}

// ------------------------------------------------------------ manifest

/// The transition level's manifest entries for a diagram (docs/cv-notes/b.md).
fn emit_orders(c: &mut Ctx, diag: &Diagram, flat: &Value) {
    let fused: BTreeSet<&str> = arr(&flat["transitions"])
        .iter()
        .filter(|t| t["fused"] == true)
        .map(|t| s(&t["id"]))
        .collect();
    for b in &diag.boxes {
        c.scene.emit(
            "tbox",
            &b.id,
            json!({"layer": b.layer, "fused": fused.contains(b.id.as_str()),
                   "inputs": b.inputs.len(), "outputs": b.outputs.len()}),
        );
    }
    // a wire is named by the transitions it joins and the place it carries
    let owner = |x: f64, y: f64, out: bool| -> String {
        diag.boxes
            .iter()
            .find(|b| {
                let py = if out { b.y + sd::BOX_HEIGHT } else { b.y };
                let n = if out { b.outputs.len() } else { b.inputs.len() };
                py == y && (0..n).any(|i| sd::port_x(b, i, n) == x)
            })
            .map(|b| b.id.clone())
            .unwrap_or_default()
    };
    for w in &diag.wires {
        let id = format!(
            "{}->{}:{}",
            owner(w.from.0, w.from.1, true),
            owner(w.to.0, w.to.1, false),
            w.wire
        );
        c.scene.emit(
            "twire",
            &id,
            json!({"back": w.back, "inhibit": w.inhibit, "weight": w.weight.unwrap_or(1.0) as i64}),
        );
    }
    for (side, list) in [
        ("in", &diag.boundary_inputs),
        ("out", &diag.boundary_outputs),
    ] {
        for p in list {
            c.scene.emit("tboundary", p, json!({"side": side}));
        }
    }
    for (a, b) in &diag.back_edges {
        c.scene.emit("tback", &format!("{a}->{b}"), json!({}));
    }
}

// ------------------------------------------------------------ orders

fn flat_diagram(c: &Ctx) -> Option<(Diagram, Value)> {
    let flat = c.d.flat.clone()?;
    if flat["ok"] != true {
        return None;
    }
    Some((sd::translate(&sd::flat_document(&flat)), flat))
}

fn flat_transition_info(t: &Value) -> Info {
    let members = strs(&t["members"]);
    let mut i = info(format!("flat transition {}", s(&t["id"])))
        .fact("concept", "https://ontology.pflow.xyz/ns#Transition (ns:Transition)")
        .fact("members", members.join(", "))
        .fact("drawn by", "diagram-viewer.js's algorithm: a box (rx 6, white, #333), inputs on top, outputs below")
        .cite(dv_cite(1, 5, "transitions → boxes (morphisms), places → wire types (objects), arcs → typed ports and wires"))
        .cite(dv_cite(503, 526, "boxes: rx 6, white, #333 1.5; port dots r 4 #666"))
        .cite(go_cite("metamodel/compose_flatten.go", 72, 80, "Flatten: transition classes by union-find"));
    if !s(&t["description"]).is_empty() {
        i = i.note(s(&t["description"]));
    }
    if members.len() > 1 {
        i = i.fact("fused", "one rendezvous: pre/postset is the union of the members', so it fires only when every member could");
    }
    i
}

fn flat_place_info(p: &Value) -> Info {
    let members = strs(&p["members"]);
    let mut i = info(format!("wire {}", s(&p["id"])))
        .fact("concept", "https://ontology.pflow.xyz/ns#Place (ns:Place)")
        .fact("members", members.join(", "))
        .fact("initial", p["initial"].to_string())
        .fact("drawn by", "diagram-viewer.js's algorithm: a place is a wire type, the label of every wire it carries")
        .cite(dv_cite(470, 481, "wires: #888, 1.5px, vertical cubics; 10px #666 label at the midpoint"))
        .cite(dv_cite(1, 5, "places → wire types (objects)"));
    if members.len() > 1 {
        i = i.fact(
            "fused",
            "a wire: places fused by token/data links; initial markings summed",
        );
    }
    i
}

pub(super) fn orders(c: &mut Ctx, area: Rect) {
    let area = toggle(c, area, true);
    let Some((diag, flat)) = flat_diagram(c) else {
        let e =
            c.d.flat
                .as_ref()
                .map(|f| s(&f["error"]).to_string())
                .unwrap_or_default();
        c.note(
            "warn",
            format!("go-pflow Flatten (pflow-compose) refuses this composition ({e}), so there is no flat net to draw."),
        );
        return;
    };
    emit_orders(c, &diag, &flat);
    let names = subnets(c)
        .iter()
        .map(|x| s(&x["id"]).to_string())
        .collect::<Vec<_>>()
        .join(" ⊗ ");
    let (dv_w, dv_h) = {
        let (w, h) = diag.svg_size();
        (w as f32, h as f32)
    };
    let probe = D {
        p: c.p.clone(),
        xf: Xf {
            origin: area.left_top(),
            k: 1.0,
        },
    };
    // the caption: what was drawn and what was left out
    let reads = arr(&flat["arcs"])
        .iter()
        .filter(|a| s(&a["type"]) == "read")
        .count();
    let fused: Vec<&Value> = arr(&flat["transitions"])
        .iter()
        .filter(|t| t["fused"] == true)
        .collect();
    let wires: Vec<&Value> = arr(&flat["places"])
        .iter()
        .filter(|p| arr(&p["members"]).len() > 1)
        .collect();
    let back_n = diag.wires.iter().filter(|w| w.back).count();
    let mut para: Vec<(String, f32, bool, Option<Color32>)> = vec![];
    para.push((
        "drawn by diagram-viewer.js's algorithm".into(),
        13.0,
        true,
        None,
    ));
    para.push((format!("pflow.xyz public/diagram-viewer.js @{DV_REV} (MIT), ported to core::string_diagram and checked against the JS's own output number for number (crates/core/tests/diagram_viewer_fixtures.rs). The drawing here paints what the port computed."), 11.0, false, Some(theme::MUTED)));
    para.push(("Its convention, from its header comment (lines 1-5): transitions are boxes (morphisms), places are wire types (objects), arcs are typed ports and wires. Inputs sit on top, outputs below; a cycle is broken by DFS and its back edges go round the right side, dashed red.".into(), 11.0, false, None));
    para.push((format!("Input: bundle.flatten, the flat net pflow-compose builds: {} places, {} transitions, {} arcs drawn.", arr(&flat["places"]).len(), arr(&flat["transitions"]).len(), arr(&flat["arcs"]).len() - reads), 11.0, false, None));
    if fused.is_empty() {
        para.push((
            "No event links, so no box is fused: every box is one subnet's transition.".into(),
            11.0,
            false,
            None,
        ));
    } else {
        let list: Vec<String> = fused
            .iter()
            .map(|t| strs(&t["members"]).join(" ⊗ "))
            .collect();
        para.push((
            format!(
                "Fused boxes ({}): each is one event-link class, named fused:<sorted members>: {}.",
                fused.len(),
                list.join("; ")
            ),
            11.0,
            false,
            Some(kind_color("event")),
        ));
    }
    if !wires.is_empty() {
        let list: Vec<String> = wires
            .iter()
            .map(|p| strs(&p["members"]).join(" = "))
            .collect();
        para.push((
            format!(
                "Shared places ({}): wire:<representative>, from token or data links: {}.",
                wires.len(),
                list.join("; ")
            ),
            11.0,
            false,
            Some(kind_color("token")),
        ));
    }
    if reads > 0 {
        para.push((format!("Left out: {reads} read arc{} (guard links lowered to \"> 0\"). diagram-viewer.js knows only input and output arcs (buildBoxes, lines 20-60): it would draw a read arc as a consuming input, which it is not.", if reads == 1 { "" } else { "s" }), 11.0, false, Some(kind_color("guard"))));
    } else {
        para.push(("No guard links, so there are no read arcs to leave out (diagram-viewer.js has no read arcs: lines 20-60).".into(), 11.0, false, Some(theme::MUTED)));
    }
    para.push((format!("{} boxes in {} layers, {} wires ({back_n} back edges); boundary in: {}; boundary out: {}.", diag.boxes.len(), layers(&diag), diag.wires.len(), if diag.boundary_inputs.is_empty() { "none".into() } else { diag.boundary_inputs.join(", ") }, if diag.boundary_outputs.is_empty() { "none".into() } else { diag.boundary_outputs.join(", ") }), 11.0, false, Some(theme::MUTED)));
    para.push(("Where labels overlap (several boundary wires on one box edge), that is its output too: nothing is moved or redrawn.".into(), 11.0, false, Some(theme::MUTED)));
    para.push(("Overlaid by this view, not part of its output: the small squares on the box edges are the pattern's transition ports (in on the left, out on the right), so event links can be dragged here as well. Place ports are not overlaid: here a place is a wire label, not a port, so token, data and guard links are made at the pattern level.".into(), 11.0, false, Some(theme::MUTED)));
    let side = dv_w <= 640.0;
    let (cap_x, cap_y) = if side {
        (OX + dv_w + 36.0, OY)
    } else {
        (OX, OY + dv_h + 30.0)
    };
    let cap_w = if side {
        1150.0 - cap_x - 24.0
    } else {
        1100.0f32.max(dv_w) - 24.0
    };
    let mut cap_lines: Vec<(String, f32, bool, Option<Color32>, bool)> = vec![];
    for (t, size, bold, col) in &para {
        for (i, l) in wrap(&probe, t, cap_w, *size).into_iter().enumerate() {
            cap_lines.push((l, *size, *bold && i == 0, *col, false));
        }
        cap_lines.push((String::new(), 0.0, false, None, true));
    }
    let cap_h: f32 = cap_lines
        .iter()
        .map(|l| if l.4 { 7.0 } else { l.1 + 4.0 })
        .sum();
    let w = 1150.0f32.max(OX + dv_w + 24.0);
    let h = (OY + dv_h + 24.0).max(cap_y + cap_h + 24.0);
    let k = (area.width() / w).min(area.height() / h).min(1.4);
    let d = D {
        p: c.p.clone(),
        xf: Xf {
            origin: area.left_top(),
            k,
        },
    };
    d.rect(
        0.0,
        0.0,
        w,
        h,
        0.0,
        dv::PAPER,
        Color32::TRANSPARENT,
        0.0,
        None,
    );
    d.text(
        (OX, 28.0),
        Align2::LEFT_BOTTOM,
        &format!("flat net of {names}"),
        15.0,
        theme::PAPER_INK,
        false,
    );
    d.text((OX, 48.0), Align2::LEFT_BOTTOM, "transition level: every transition a box, every place a wire type; what fusion does to the boxes is visible here", 11.0, theme::MUTED, false);

    // the diagram, offset to (OX, OY)
    let dd = D {
        p: d.p.clone(),
        xf: Xf {
            origin: d.pt((OX, OY)),
            k,
        },
    };
    dd.rect(
        0.0,
        0.0,
        dv_w,
        dv_h,
        4.0,
        dv::PAPER,
        Color32::TRANSPARENT,
        0.0,
        None,
    );
    draw_diagram(&dd, &diag, None, false);
    d.rect(
        OX - 0.5,
        OY - 0.5,
        dv_w + 1.0,
        dv_h + 1.0,
        4.0,
        Color32::TRANSPARENT,
        hex("#dde1e6"),
        1.0,
        None,
    );

    // inspectable: boxes, and each wire label's place
    let by_t: BTreeMap<&str, &Value> = arr(&flat["transitions"])
        .iter()
        .map(|t| (s(&t["id"]), t))
        .collect();
    let by_p: BTreeMap<&str, &Value> = arr(&flat["places"])
        .iter()
        .map(|p| (s(&p["id"]), p))
        .collect();
    for w in &diag.wires {
        if let Some(p) = by_p.get(w.wire.as_str()) {
            let pts: Vec<Pt> = if w.back {
                let (a, b, _) = w.back_curves(dv_w as f64);
                flatten(a.map(|q| (q.0 as f32, q.1 as f32)), 12)
                    .into_iter()
                    .chain(flatten(b.map(|q| (q.0 as f32, q.1 as f32)), 12))
                    .collect()
            } else {
                flatten(w.forward_curve().map(|q| (q.0 as f32, q.1 as f32)), 12)
            };
            let pts: Vec<Pt> = pts.into_iter().map(|(x, y)| (x + OX, y + OY)).collect();
            hover_path(c, &d, &pts, 5.0, flat_place_info(p));
        }
    }
    for st in diag.stubs() {
        if let Some(p) = by_p.get(st.wire.as_str()) {
            let pts = [
                (st.x as f32 + OX, st.y_free as f32 + OY),
                (st.x as f32 + OX, st.y_port as f32 + OY),
            ];
            hover_path(c, &d, &pts, 5.0, flat_place_info(p));
        }
    }
    for b in &diag.boxes {
        if let Some(t) = by_t.get(b.id.as_str()) {
            c.hover(
                d.rect_r(OX + b.x as f32, OY + b.y as f32, b.width as f32, dv::BOX_H),
                flat_transition_info(t),
            );
        }
    }

    // merge animation: the boxes that fused slide from where they were
    let ev = c.anim(|e| match e {
        Event::LinkAdded { key, kind } => Some((key.clone(), kind.clone())),
        _ => None,
    });
    let mut flash: Option<(String, Color32, f32)> = None;
    if let Some(((key, kind), t)) = &ev {
        let col = kind_color(kind);
        let link = links(c).into_iter().find(|l| s(&l["key"]) == *key);
        if let (Some(l), Some(prev)) = (link, c.d.prev_flat.clone()) {
            if kind == "event" && prev["ok"] == true {
                let pdiag = sd::translate(&sd::flat_document(&prev));
                let fsub = s(&l["from"]["subnet"]).to_string();
                let fport = s(&l["from"]["port"]).to_string();
                let tname = subnets(c)
                    .iter()
                    .find(|x| s(&x["id"]) == fsub)
                    .and_then(|x| arr(&x["ports"]).iter().find(|p| s(&p["id"]) == fport))
                    .map(|p| s(&p["transition"]).to_string())
                    .unwrap_or_default();
                let flat_id = s(&flat["transition_map"][&fsub][&tname]).to_string();
                if let Some(target) = diag.boxes.iter().find(|b| b.id == flat_id) {
                    // the members of the fused box, where the previous layout had them
                    for mem in strs(
                        &by_t
                            .get(flat_id.as_str())
                            .map_or(Value::Null, |t| t["members"].clone()),
                    ) {
                        let (ms, mt) = mem.split_once('/').unwrap_or(("", &mem));
                        let prev_id = prev["transition_map"][ms][mt].as_str().unwrap_or("");
                        if let Some(pb) = pdiag.boxes.iter().find(|b| b.id == prev_id) {
                            let e = (*t / 0.8).min(1.0);
                            let (x, y) = (
                                pb.x as f32 + (target.x as f32 - pb.x as f32) * e,
                                pb.y as f32 + (target.y as f32 - pb.y as f32) * e,
                            );
                            let alpha = if *t > 0.8 {
                                1.0 - (*t - 0.8) / 0.2
                            } else {
                                1.0
                            };
                            d.rect(
                                OX + x,
                                OY + y,
                                pb.width as f32,
                                dv::BOX_H,
                                dv::RX,
                                Color32::TRANSPARENT,
                                col.gamma_multiply(alpha),
                                2.0,
                                Some((5.0, 3.0)),
                            );
                        }
                    }
                    flash = Some((flat_id, col, *t));
                }
            }
        }
    }
    if let Some((id, col, t)) = flash {
        if let Some(b) = diag.boxes.iter().find(|b| b.id == id) {
            let e = (t * std::f32::consts::PI).sin();
            d.rect(
                OX + b.x as f32,
                OY + b.y as f32,
                b.width as f32,
                dv::BOX_H,
                dv::RX,
                Color32::TRANSPARENT,
                col,
                1.5 + 2.0 * e,
                None,
            );
        }
    }

    // overlay: the pattern's transition ports on each box
    let subs = subnets(c);
    for b in &diag.boxes {
        let Some(t) = by_t.get(b.id.as_str()) else {
            continue;
        };
        let mut ports: Vec<(String, Value)> = vec![];
        for mem in strs(&t["members"]) {
            let (ms, mt) = mem.split_once('/').unwrap_or(("", &mem));
            if let Some(sub) = subs.iter().find(|x| s(&x["id"]) == ms) {
                for p in arr(&sub["ports"]) {
                    if s(&p["target"]) == "transition" && s(&p["transition"]) == mt {
                        ports.push((ms.to_string(), p.clone()));
                    }
                }
            }
        }
        for left in [true, false] {
            let list: Vec<&(String, Value)> = ports
                .iter()
                .filter(|(_, p)| (s(&p["kind"]) == "in") == left)
                .collect();
            for (i, (sub, p)) in list.iter().enumerate() {
                let x = OX + b.x as f32 + if left { 0.0 } else { b.width as f32 };
                let y = OY + b.y as f32 + dv::BOX_H * (i as f32 + 1.0) / (list.len() as f32 + 1.0);
                let hit = Rect::from_center_size(d.pt((x, y)), Vec2::splat(14.0 * k.max(0.8)));
                let pid = s(&p["id"]);
                let i_ = info(format!("port {sub}:{pid}"))
                    .fact("kind (ns:portKind)", s(&p["kind"]))
                    .fact("target (ns:portTarget)", "transition")
                    .fact("transition", s(&p["transition"]))
                    .fact("on box", b.id.as_str())
                    .fact("overlay", "drawn by this view on top of diagram-viewer.js's box; not part of its output")
                    .fact("drag", "from here to link: the lit ports are the ones validateLink allows");
                let (ring, dim) = snap_port(c, hit, sub, pid, i_);
                let col = theme::TRANSITION_PORT;
                let col = if dim { col.gamma_multiply(0.3) } else { col };
                d.rect(
                    x - 3.5,
                    y - 3.5,
                    7.0,
                    7.0,
                    0.0,
                    col,
                    Color32::WHITE,
                    1.0,
                    None,
                );
                if let Some(rc) = ring {
                    d.ring((x, y), 8.0, rc, 2.5);
                }
            }
        }
    }

    // caption
    let mut y = cap_y + 12.0;
    for (t, size, bold, col, gap) in &cap_lines {
        if *gap {
            y += 7.0;
            continue;
        }
        let colr = col.unwrap_or(theme::PAPER_INK);
        d.text((cap_x, y + 3.0), Align2::LEFT_BOTTOM, t, *size, colr, false);
        if *bold {
            d.text(
                (cap_x + 0.4, y + 3.0),
                Align2::LEFT_BOTTOM,
                t,
                *size,
                colr,
                false,
            );
        }
        y += size + 4.0;
    }
    c.hover(
        d.rect_r(cap_x, cap_y, cap_w, cap_h),
        info("diagram-viewer.js (pflow-xyz, MIT)")
            .note("Petri net → string diagram: transitions are boxes, places are wire types, arcs are typed ports and wires.")
            .cite(dv_cite(1, 5, "transitions → boxes (morphisms), places → wire types (objects), arcs → typed ports and wires"))
            .cite(dv_cite(20, 60, "buildBoxes: an arc P→T is an input port, T→P an output port; nothing else"))
            .cite(dv_cite(108, 128, "DFS cycle breaking: the back edges"))
            .cite(dv_cite(130, 169, "longest-path layering"))
            .cite(dv_cite(490, 501, "back edges: dashed #c44 (6,3), routed around the right side, arrowhead"))
            .cite(go_cite("metamodel/compose_flatten.go", 424, 482, "applyGuardLinks: a guard link becomes a read arc")),
    );
    if reads > 0 {
        c.note("na", format!("{reads} read arc{} from guard links {} left out: diagram-viewer.js draws only input and output arcs, and would show a read as a consuming input.", if reads == 1 { "" } else { "s" }, if reads == 1 { "is" } else { "are" }));
    }
    c.note("honesty", "Transition level: the drawing is diagram-viewer.js's algorithm run in Rust (core::string_diagram, compared with its output on the orders flat net, Connect-4 and small nets). Read arcs (lowered guard links) have no counterpart there and are left out, and the port squares on top of its boxes are this view's addition.");
}

// ------------------------------------------------------------ Connect-4

fn c4_diagram() -> &'static Diagram {
    static D: OnceLock<Diagram> = OnceLock::new();
    D.get_or_init(|| {
        sd::translate(
            &Document::from_json(sp::CONNECT4_JSON.as_bytes()).expect("connect-4.json is a net"),
        )
    })
}

fn c4_transition_info(c: &Ctx, id: &str) -> Info {
    let doc = c.d.c4.as_ref();
    let t = doc.and_then(|d| d.transition(id));
    let ins: Vec<String> = doc
        .map(|d| {
            d.arcs
                .iter()
                .filter(|a| a.target == id && d.place(&a.source).is_some())
                .map(|a| {
                    format!(
                        "{}{}",
                        a.source,
                        if a.inhibit { " (inhibitor)" } else { "" }
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    let outs: Vec<String> = doc
        .map(|d| {
            d.arcs
                .iter()
                .filter(|a| a.source == id && d.place(&a.target).is_some())
                .map(|a| a.target.clone())
                .collect()
        })
        .unwrap_or_default();
    info(format!("transition {id} (Connect-4 net)"))
        .fact("concept", "https://ontology.pflow.xyz/ns#Transition (ns:Transition)")
        .fact("role", t.map_or("default", |t| t.role()))
        .fact("inputs (top ports)", if ins.is_empty() { "none".into() } else { ins.join(", ") })
        .fact("outputs (bottom ports)", if outs.is_empty() { "none".into() } else { outs.join(", ") })
        .fact("drawn by", "diagram-viewer.js's algorithm: a box; places are the labels of its wires")
        .cite("pflow-rs crates/pflow-metamodel/tests/fixtures/connect-4.json (the Connect-4 net; examples/connect-4.json is byte-identical)")
        .cite(dv_cite(1, 5, "transitions → boxes (morphisms), places → wire types (objects), arcs → typed ports and wires"))
        .cite(dv_cite(503, 526, "boxes: rx 6, white, #333 1.5; port dots r 4 #666"))
}

fn layer_range(d: &Diagram, y0: f32, y1: f32) -> String {
    let (mut lo, mut hi) = (usize::MAX, 0usize);
    for b in &d.boxes {
        if b.y as f32 + dv::BOX_H >= y0 && b.y as f32 <= y1 {
            lo = lo.min(b.layer);
            hi = hi.max(b.layer);
        }
    }
    if lo == usize::MAX {
        "–".into()
    } else if lo == hi {
        format!("{lo}")
    } else {
        format!("{lo}–{hi}")
    }
}

pub(super) fn c4(c: &mut Ctx, area: Rect) {
    let area = toggle(c, area, false);
    let diag = c4_diagram();
    let (dw, dh) = {
        let (w, h) = diag.svg_size();
        (w as f32, h as f32)
    };
    let back = diag.wires.iter().filter(|w| w.back).count();
    let cross = diag.wires.iter().filter(|w| w.crossing).count();
    let nl = layers(diag);
    c.scene.emit(
        "tnet",
        "connect-4",
        json!({"boxes": diag.boxes.len(), "layers": nl, "wires": diag.wires.len(),
               "back_wires": back, "crossing_wires": cross,
               "boundary_in": diag.boundary_inputs.len(), "boundary_out": diag.boundary_outputs.len()}),
    );
    let n_cols = 5usize;
    let gap = 10.0f32;
    let (ov_w, ov_h) = (520.0f32, 690.0f32);
    let kk = ((ov_w - (n_cols as f32 - 1.0) * gap) / (n_cols as f32 * dw))
        .min(ov_h * n_cols as f32 / dh);
    let slice_h = dh / n_cols as f32;
    let (col_w, col_h) = (dw * kk, slice_h * kk);
    let w_tot = 1150.0f32;
    let lx = OX + n_cols as f32 * col_w + (n_cols as f32 - 1.0) * gap + 30.0;
    let lens_w = dw.min(w_tot - lx - 20.0);
    let lens_h = col_h.round();
    let oy = 78.0f32;
    let h_tot = oy + col_h.max(lens_h) + 34.0;
    let k = (area.width() / w_tot).min(area.height() / h_tot).min(1.4);
    let d = D {
        p: c.p.clone(),
        xf: Xf {
            origin: area.left_top(),
            k,
        },
    };
    d.rect(
        0.0,
        0.0,
        w_tot,
        h_tot,
        0.0,
        dv::PAPER,
        Color32::TRANSPARENT,
        0.0,
        None,
    );
    d.text(
        (OX, 26.0),
        Align2::LEFT_BOTTOM,
        "diagram-viewer.js's algorithm on the Connect-4 net",
        15.0,
        theme::PAPER_INK,
        false,
    );
    let (np, nt, na) = net_counts(c);
    d.text(
        (OX, 46.0),
        Align2::LEFT_BOTTOM,
        &format!("{np} places, {nt} transitions, {na} arcs → {} boxes in {nl} layers, {} wires ({} flagged back, {} marked crossing; {} distinct back edges), {} × {} px", diag.boxes.len(), fmt_n(diag.wires.len() as f64), fmt_n(back as f64), fmt_n(cross as f64), fmt_n(diag.back_edges.len() as f64), fmt_n(dw as f64), fmt_n(dh as f64)),
        11.0,
        Color32::from_gray(0x55),
        false,
    );
    d.text((OX, 66.0), Align2::LEFT_BOTTOM, &format!("overview: the whole output in {n_cols} columns at {:.1}% (top to bottom, then left to right)", kk * 100.0), 10.5, theme::MUTED, false);
    d.text(
        (lx, 66.0),
        Align2::LEFT_BOTTOM,
        "lens: the same output at 1:1 (scroll here, or click / drag the overview)",
        10.5,
        theme::MUTED,
        false,
    );

    let lens_y = (c.lens_y as f32).clamp(0.0, (dh - lens_h).max(0.0));
    // overview columns
    for i in 0..n_cols {
        let x = OX + i as f32 * (col_w + gap);
        d.rect(
            x - 0.5,
            oy - 0.5,
            col_w + 1.0,
            col_h + 1.0,
            0.0,
            Color32::WHITE,
            hex("#d4d8dd"),
            1.0,
            None,
        );
        let clip = d.rect_r(x, oy, col_w, col_h);
        // design -> window: scale kk, shifted so slice i starts at the top of the column
        let od = D {
            p: d.p.with_clip_rect(clip.intersect(d.p.clip_rect())),
            xf: Xf {
                origin: d.pt((x, oy - i as f32 * slice_h * kk)),
                k: k * kk,
            },
        };
        draw_diagram(
            &od,
            diag,
            Some((i as f32 * slice_h, (i as f32 + 1.0) * slice_h)),
            true,
        );
        d.text(
            (x + col_w / 2.0, oy + col_h + 14.0),
            Align2::CENTER_BOTTOM,
            &format!(
                "layers {}",
                layer_range(diag, i as f32 * slice_h, (i as f32 + 1.0) * slice_h)
            ),
            9.0,
            Color32::from_gray(0x88),
            false,
        );
        // where the lens is
        let (a, b) = (
            lens_y.max(i as f32 * slice_h),
            (lens_y + lens_h).min((i as f32 + 1.0) * slice_h),
        );
        if b > a {
            let r = d.rect_r(
                x - 2.0,
                oy + (a - i as f32 * slice_h) * kk,
                col_w + 4.0,
                ((b - a) * kk).max(3.0),
            );
            d.p.rect(
                r,
                0.0,
                hex("#2f6fb3").gamma_multiply(0.16),
                Stroke::new(1.2, hex("#2f6fb3")),
                StrokeKind::Middle,
            );
        }
    }
    let ov_area = Rect::from_min_max(
        d.pt((OX, oy)),
        d.pt((
            OX + n_cols as f32 * col_w + (n_cols as f32 - 1.0) * gap,
            oy + col_h,
        )),
    );
    c.hover(
        ov_area,
        info("diagram-viewer.js's output for the Connect-4 net, scaled")
            .fact("what", format!("The algorithm's output, drawn small in {n_cols} columns at {:.1}%. At this size its 12px box labels are about {:.1}px: the overview shows the shape (a {nl}-layer column with back edges down its right side), not the boxes.", kk * 100.0, 12.0 * kk))
            .fact("size", format!("{} × {} px", fmt_n(dw as f64), fmt_n(dh as f64)))
            .fact("interact", "click or drag to move the lens (view.composition {lens_y})")
            .cite("pflow-rs crates/pflow-metamodel/tests/fixtures/connect-4.json (the Connect-4 net)")
            .cite(dv_cite(130, 169, "longest-path layering"))
            .cite(dv_cite(490, 501, "back edges: dashed #c44 (6,3), routed around the right side, arrowhead")),
    );
    // the lens: the real drawing, 1:1, clipped
    let lens_r = d.rect_r(lx, oy, lens_w, lens_h);
    d.p.rect_filled(lens_r, 0.0, dv::PAPER);
    let ld = D {
        p: d.p.with_clip_rect(lens_r.intersect(d.p.clip_rect())),
        xf: Xf {
            origin: d.pt((lx, oy - lens_y)),
            k,
        },
    };
    draw_diagram(&ld, diag, Some((lens_y, lens_y + lens_h)), false);
    d.rect(
        lx - 0.5,
        oy - 0.5,
        lens_w + 1.0,
        lens_h + 1.0,
        3.0,
        Color32::TRANSPARENT,
        hex("#9aa1aa"),
        1.0,
        None,
    );
    d.text(
        (lx + lens_w, oy + lens_h + 16.0),
        Align2::RIGHT_BOTTOM,
        &format!(
            "y {}–{} of {} · layers {}",
            fmt_n(lens_y as f64),
            fmt_n((lens_y + lens_h) as f64),
            fmt_n(dh as f64),
            layer_range(diag, lens_y, lens_y + lens_h)
        ),
        9.5,
        Color32::from_gray(0x88),
        false,
    );
    for b in &diag.boxes {
        if b.y as f32 + dv::BOX_H >= lens_y
            && (b.y as f32) <= lens_y + lens_h
            && (b.x as f32) < lens_w
        {
            let r = d.rect_r(
                lx + b.x as f32,
                oy + b.y as f32 - lens_y,
                b.width as f32,
                dv::BOX_H,
            );
            if r.intersects(lens_r) {
                let i = c4_transition_info(c, &b.id);
                c.hover(r.intersect(lens_r), i);
            }
        }
    }
    // interaction: the overview moves the lens, the wheel scrolls it
    let down = c.p.ctx().input(|i| i.pointer.primary_down());
    if let (Some(ptr), true) = (c.pointer, down) {
        if c.press_origin
            .is_some_and(|o| ov_area.expand(2.0).contains(o))
        {
            let px = (ptr.x - d.pt((OX, 0.0)).x) / k;
            let col =
                (((px + gap / 2.0) / (col_w + gap)).floor().max(0.0) as usize).min(n_cols - 1);
            let py = ((ptr.y - d.pt((0.0, oy)).y) / k).clamp(0.0, col_h);
            let y = (col as f32 * slice_h + py / kk - lens_h / 2.0)
                .clamp(0.0, (dh - lens_h).max(0.0)) as f64;
            if (y - c.lens_y).abs() > 0.5 {
                c.command("view.composition", json!({"lens_y": y}));
            }
        }
    }
    if c.pointer.is_some_and(|p| lens_r.contains(p)) {
        let dy = c.p.ctx().input(|i| i.smooth_scroll_delta.y);
        if dy != 0.0 {
            let y = (lens_y - dy / k).clamp(0.0, (dh - lens_h).max(0.0)) as f64;
            c.command("view.composition", json!({"lens_y": y}));
        }
    }
    c.note("na", format!("Too large to read on one stage, so this is a scaled overview with a 1:1 lens, not a fit-to-width drawing. diagram-viewer.js lays the Connect-4 net out as one column of {nl} layers, {} × {} px, with {} wires; fitted to this stage its labels would be under 2 px. This level is the net the plan searches, not the plan, so it has no slots: drop pieces at the pattern level.", fmt_n(dw as f64), fmt_n(dh as f64), fmt_n(diag.wires.len() as f64)));
    c.note("honesty", "Transition level: the drawing is diagram-viewer.js's algorithm run in Rust (core::string_diagram), checked against its output on this net: 222 boxes, 14,296 wires, 3,663 of them back (3,406 distinct back edges), same positions. Both panes show that output as it comes, nothing redrawn.");
}

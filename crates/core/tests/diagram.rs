//! The String Diagram view (`view.string_diagram`, `core::string_diagram`).
//!
//! Golden values were captured by running pflow.xyz's own
//! `diagram-viewer.js` (`translatePetriNet`, `generateSVG`) and
//! `_applyStringDiagramLayout` (petri-view.js pv:6381) under node on the
//! same files. During the port every element of the JS SVG was also matched
//! one for one against the drawing for all seven examples.

use pflow_native_core::artifact::DrawShape;
use pflow_native_core::string_diagram::{net_layout, string_diagram, translate, Diagram};
use pflow_native_core::{ArtifactData, Document, Session};
use serde_json::{json, Value};

const SIR: &[u8] = include_bytes!("../../../examples/sir.json");
const COFFEE: &[u8] = include_bytes!("../../../examples/coffee-shop.json");
const TTT: &[u8] = include_bytes!("../../../examples/tic-tac-toe.json");
/// Array-index ids ("10", "2", "7") that JS walks first, a cycle t → u → t
/// through `b` with a parallel arc, inhibitors, a weight-3 arc.
const MIXED: &[u8] = br#"{"token":[],
 "places":{"b":{"initial":[1]},"10":{},"2":{},"a":{},"z":{},"q":{}},
 "transitions":{"t":{},"u":{},"v":{},"7":{}},
 "arcs":[{"source":"b","target":"t"},{"source":"t","target":"10","weight":[3]},
         {"source":"10","target":"u"},{"source":"u","target":"b"},
         {"source":"a","target":"v","inhibitTransition":true},{"source":"v","target":"z"},
         {"source":"z","target":"v"},{"source":"u","target":"a"},
         {"source":"2","target":"7","inhibitTransition":true},{"source":"7","target":"q"},
         {"source":"q","target":"t","inhibitTransition":true},{"source":"b","target":"t"}]}"#;

fn doc(bytes: &[u8]) -> Document {
    Document::from_json(bytes).unwrap()
}

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// (id, layer, x, y, width) per box, in transition order.
fn boxes(d: &Diagram) -> Vec<(&str, usize, f64, f64, f64)> {
    d.boxes
        .iter()
        .map(|b| (b.id.as_str(), b.layer, b.x, b.y, b.width))
        .collect()
}

/// (place, fromX, fromY, toX, toY, back, inhibit, weight[0]) per wire.
type W<'a> = (&'a str, f64, f64, f64, f64, bool, bool, f64);
fn wires(d: &Diagram) -> Vec<W<'_>> {
    d.wires
        .iter()
        .map(|w| {
            let (f, t) = (w.from, w.to);
            let wt = w.weight.unwrap_or(0.0);
            (w.wire.as_str(), f.0, f.1, t.0, t.1, w.back, w.inhibit, wt)
        })
        .collect()
}

#[test]
fn translation_matches_pflow_xyz() {
    let d = translate(&doc(SIR));
    assert_eq!(
        boxes(&d),
        [
            ("infect", 0, 30.0, 30.0, 80.0),
            ("recover", 1, 30.0, 150.0, 80.0)
        ]
    );
    assert_eq!(
        wires(&d),
        [
            ("I", 70.0, 70.0, 82.0, 30.0, false, false, 2.0),
            ("I", 70.0, 70.0, 70.0, 150.0, false, false, 2.0)
        ]
    );
    assert_eq!((d.width, d.height), (140.0, 300.0));
    assert_eq!(
        (d.boundary_inputs, d.boundary_outputs),
        (vec!["S".to_string()], vec!["R".to_string()])
    );
    assert!(d.back_edges.is_empty());

    let d = translate(&doc(COFFEE));
    assert_eq!(
        boxes(&d),
        [
            ("BrewCoffee", 1, 30.0, 150.0, 98.0),
            ("CustomerArrives", 0, 30.0, 30.0, 137.0),
            ("CustomerLeaves", 2, 30.0, 270.0, 129.20000000000002),
            ("RestockBeans", 0, 207.0, 30.0, 113.60000000000001),
            ("RestockCups", 0, 360.6, 30.0, 105.80000000000001),
        ]
    );
    assert_eq!(
        wires(&d),
        [
            ("CoffeeBeans", 263.8, 70.0, 79.0, 150.0, false, false, 0.0),
            ("EmptyCups", 413.5, 70.0, 103.0, 150.0, false, false, 0.0),
            (
                "ServedCoffee",
                79.0,
                190.0,
                94.60000000000001,
                270.0,
                false,
                false,
                1.0
            ),
            (
                "WaitingCustomers",
                98.5,
                70.0,
                55.0,
                150.0,
                false,
                false,
                1.0
            ),
        ]
    );
    assert_eq!((d.width, d.height), (496.40000000000003, 340.0));
    assert!(d.boundary_inputs.is_empty() && d.boundary_outputs.is_empty());

    let d = translate(&doc(MIXED));
    assert_eq!(
        boxes(&d),
        [
            ("7", 0, 30.0, 30.0, 80.0),
            ("t", 1, 30.0, 150.0, 80.0),
            ("u", 2, 30.0, 270.0, 80.0),
            ("v", 3, 30.0, 390.0, 80.0),
        ]
    );
    assert_eq!(
        wires(&d),
        [
            ("10", 70.0, 190.0, 70.0, 270.0, false, false, 3.0),
            ("b", 58.0, 310.0, 94.0, 150.0, true, false, 1.0),
            ("b", 58.0, 310.0, 94.0, 150.0, true, false, 1.0),
            ("a", 82.0, 310.0, 58.0, 390.0, false, true, 1.0),
            ("z", 70.0, 430.0, 82.0, 390.0, false, false, 1.0),
            ("q", 70.0, 70.0, 70.0, 150.0, false, true, 1.0),
        ]
    );
    assert_eq!(d.back_edges, [("u".to_string(), "t".to_string())]);
    assert_eq!((d.width, d.height), (140.0, 460.0));
    assert_eq!(d.boundary_inputs, ["2"]);

    let d = translate(&doc(TTT));
    let want: &[(&str, usize, f64, f64, f64)] = &[
        ("o_play_00", 0, 30.0, 30.0, 90.2),
        ("o_play_01", 2, 30.0, 270.0, 90.2),
        ("o_play_02", 4, 30.0, 510.0, 90.2),
        ("o_play_10", 6, 30.0, 750.0, 90.2),
        ("o_play_11", 8, 30.0, 990.0, 90.2),
        ("o_play_12", 10, 30.0, 1230.0, 90.2),
        ("o_play_20", 12, 30.0, 1470.0, 90.2),
        ("o_play_21", 14, 30.0, 1710.0, 90.2),
        ("o_play_22", 16, 30.0, 1950.0, 90.2),
        ("o_win_anti", 22, 30.0, 2670.0, 98.0),
        ("o_win_col0", 18, 30.0, 2190.0, 98.0),
        ("o_win_col1", 21, 30.0, 2550.0, 98.0),
        ("o_win_col2", 23, 30.0, 2790.0, 98.0),
        ("o_win_diag", 19, 30.0, 2310.0, 98.0),
        ("o_win_row0", 17, 30.0, 2070.0, 98.0),
        ("o_win_row1", 20, 30.0, 2430.0, 98.0),
        ("o_win_row2", 24, 30.0, 2910.0, 98.0),
        ("x_play_00", 1, 30.0, 150.0, 90.2),
        ("x_play_01", 3, 30.0, 390.0, 90.2),
        ("x_play_02", 5, 30.0, 630.0, 90.2),
        ("x_play_10", 7, 30.0, 870.0, 90.2),
        ("x_play_11", 9, 30.0, 1110.0, 90.2),
        ("x_play_12", 11, 30.0, 1350.0, 90.2),
        ("x_play_20", 13, 30.0, 1590.0, 90.2),
        ("x_play_21", 15, 30.0, 1830.0, 90.2),
        ("x_play_22", 17, 168.0, 2070.0, 90.2),
        ("x_win_anti", 23, 168.0, 2790.0, 98.0),
        ("x_win_col0", 19, 168.0, 2310.0, 98.0),
        ("x_win_col1", 22, 168.0, 2670.0, 98.0),
        ("x_win_col2", 24, 168.0, 2910.0, 98.0),
        ("x_win_diag", 20, 168.0, 2430.0, 98.0),
        ("x_win_row0", 18, 168.0, 2190.0, 98.0),
        ("x_win_row1", 21, 168.0, 2550.0, 98.0),
        ("x_win_row2", 25, 30.0, 3030.0, 98.0),
    ];
    assert_eq!(boxes(&d), want);
    assert_eq!((d.width, d.height), (296.0, 3180.0));
    assert_eq!(d.wires.len(), 490);
    assert_eq!(d.wires.iter().filter(|w| w.back).count(), 125);
    assert_eq!(d.back_edges.len(), 125);
    assert_eq!(
        d.back_edges[0],
        ("o_win_col0".to_string(), "o_win_row0".to_string())
    );
    assert_eq!(d.boundary_inputs.len(), 9);
    assert_eq!(d.boundary_outputs, ["win_o", "win_x"]);
}

#[test]
fn net_layout_matches_pflow_xyz() {
    let pos = |b: &[u8]| -> Vec<(String, f64, f64)> { net_layout(&doc(b)) };
    let want = |v: &[(&str, f64, f64)]| -> Vec<(String, f64, f64)> {
        v.iter().map(|&(i, x, y)| (i.to_string(), x, y)).collect()
    };
    assert_eq!(
        pos(SIR),
        want(&[
            ("S", 400.0, 100.0),
            ("I", 400.0, 235.0),
            ("R", 400.0, 460.0),
            ("infect", 400.0, 190.0),
            ("recover", 400.0, 370.0),
        ])
    );
    assert_eq!(
        pos(COFFEE),
        want(&[
            ("CoffeeBeans", 400.0, 190.0),
            ("EmptyCups", 460.0, 190.0),
            ("ServedCoffee", 400.0, 370.0),
            ("WaitingCustomers", 340.0, 190.0),
            ("BrewCoffee", 400.0, 280.0),
            ("CustomerArrives", 280.0, 100.0),
            ("CustomerLeaves", 400.0, 460.0),
            ("RestockBeans", 400.0, 100.0),
            ("RestockCups", 520.0, 100.0),
        ])
    );
    assert_eq!(
        pos(MIXED),
        want(&[
            ("2", 400.0, 100.0),
            ("10", 400.0, 460.0),
            ("b", 400.0, 460.0),
            ("a", 400.0, 640.0),
            ("z", 400.0, 730.0),
            ("q", 400.0, 280.0),
            ("7", 400.0, 190.0),
            ("t", 400.0, 370.0),
            ("u", 400.0, 550.0),
            ("v", 400.0, 730.0),
        ])
    );
    let ttt = pos(TTT);
    assert_eq!(ttt.len(), 65);
    for (id, x, y) in [
        ("o00", 348.0, 3025.0),
        ("o_turn", 416.0, 2292.0),
        ("p22", 430.0, 2980.0),
        ("win_x", 453.0, 4780.0),
        ("x_turn", 388.0, 2207.0),
        ("x_play_22", 460.0, 3250.0),
        ("x_win_row2", 400.0, 4690.0),
    ] {
        assert!(ttt.contains(&(id.to_string(), x, y)), "{id}");
    }
    assert!(net_layout(&Document::default()).is_empty());
}

fn drawing(a: &ArtifactData) -> (f64, f64, &[DrawShape]) {
    match a {
        ArtifactData::Drawing {
            width,
            height,
            shapes,
        } => (*width, *height, shapes),
        other => panic!("not a drawing: {}", other.kind()),
    }
}

fn texts(shapes: &[DrawShape]) -> Vec<&str> {
    shapes
        .iter()
        .filter_map(|s| match s {
            DrawShape::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

/// Every anchor point of every shape, for the NaN and bounds checks.
fn points(s: &DrawShape) -> Vec<(f64, f64)> {
    match s {
        DrawShape::Line { points, .. } | DrawShape::Polygon { points, .. } => points.clone(),
        DrawShape::Rect { x, y, w, h, .. } => vec![(*x, *y), (x + w, y + h)],
        DrawShape::Circle { x, y, .. } | DrawShape::Text { x, y, .. } => vec![(*x, *y)],
    }
}

#[test]
fn command_draws_every_example() {
    for name in ["sir", "coffee-shop", "tic-tac-toe"] {
        let mut s = Session::default();
        run(&mut s, "file.open_example", json!({"name": name}));
        let out = run(&mut s, "view.string_diagram", Value::Null);
        let id = out["artifact"].as_str().unwrap().to_string();
        let art = s.artifact(&id).unwrap().clone();
        assert_eq!(art.title, "String diagram");
        assert_eq!(art.source.command, "view.string_diagram");
        let (w, h, shapes) = drawing(&art.data);
        assert_eq!(
            (out["width"].as_f64(), out["height"].as_f64()),
            (Some(w), Some(h))
        );

        // Deterministic, and the same as the library call.
        assert_eq!(art.data, string_diagram(&s.doc), "{name}");
        let again = run(&mut s, "view.string_diagram", Value::Null);
        let id = again["artifact"].as_str().unwrap().to_string();
        assert_eq!(s.artifact(&id).unwrap().data, art.data, "{name}");
        assert_eq!(
            s.artifacts
                .iter()
                .filter(|a| a.title == "String diagram")
                .count(),
            1,
            "{name}: a re-run replaces the drawing"
        );

        // Every transition is a labeled box; every connected place labels a wire.
        let labels = texts(shapes);
        let rects: Vec<(f64, f64, f64, f64)> = shapes
            .iter()
            .filter_map(|s| match s {
                DrawShape::Rect { x, y, w, h, .. } => Some((*x, *y, *w, *h)),
                _ => None,
            })
            .collect();
        assert_eq!(
            rects.len(),
            s.doc.transitions.len() + 1,
            "{name}: boxes + background"
        );
        for t in &s.doc.transitions {
            assert!(
                labels.contains(&t.id.as_str()),
                "{name}: box label {}",
                t.id
            );
        }
        for p in &s.doc.places {
            let connected = s
                .doc
                .arcs
                .iter()
                .any(|a| a.source == p.id || a.target == p.id);
            if connected {
                assert!(
                    labels.contains(&p.id.as_str()),
                    "{name}: wire label {}",
                    p.id
                );
            }
        }

        // Finite and inside the canvas.
        assert!(w.is_finite() && h.is_finite() && w >= 200.0 && h >= 100.0);
        for sh in shapes {
            for (x, y) in points(sh) {
                assert!(x.is_finite() && y.is_finite(), "{name}: NaN in {sh:?}");
                assert!(
                    (0.0..=w).contains(&x) && (0.0..=h).contains(&y),
                    "{name}: ({x}, {y}) outside {w}×{h} in {sh:?}"
                );
            }
        }

        let svg = run(
            &mut s,
            "export.artifact",
            json!({"id": id, "format": "svg"}),
        );
        let svg = svg.as_str().unwrap();
        assert!(
            svg.starts_with("<svg") && svg.contains("<polyline"),
            "{name}"
        );
        assert_eq!(svg.matches("<text").count(), labels.len(), "{name}");
    }
}

#[test]
fn drawing_styles_follow_the_viewer() {
    let art = string_diagram(&doc(MIXED));
    let (w, h, shapes) = drawing(&art);
    assert_eq!((w, h), (200.0, 460.0));
    // Background first, the viewer's #fafafa.
    assert!(
        matches!(&shapes[0], DrawShape::Rect { fill: Some(f), radius, .. } if f == "#fafafa" && *radius == 4.0)
    );
    // Inhibitor input ports are red rings; the weight-3 arc is labeled in red.
    let rings = shapes
        .iter()
        .filter(|s| matches!(s, DrawShape::Circle { fill: None, stroke: Some(c), .. } if c == "#cc4444"))
        .count();
    assert_eq!(
        rings, 4,
        "2 on the boundary wire + 1 per inhibitor port drawn on the boxes"
    );
    assert!(shapes.iter().any(|s| matches!(s,
        DrawShape::Text { text, color, anchor, x, y, .. }
            if text == "3" && color == "#cc4444" && anchor == "start" && *x == 78.0 && *y == 238.0)));
    // The back edge u → t (twice: two parallel b → t arcs) is dashed red,
    // routed through x = 200 - 15, ending in an arrowhead at t's input port.
    let back: Vec<_> = shapes
        .iter()
        .filter_map(|s| match s {
            DrawShape::Line {
                points,
                color,
                dashed: true,
                ..
            } if color == "#cc4444" => Some(points),
            _ => None,
        })
        .filter(|p| p.iter().any(|&(x, _)| x == 185.0))
        .collect();
    assert_eq!(back.len(), 4);
    // Back edges dash 6,3; arrowheads are filled red triangles tipped at
    // t's input port.
    let arrows: Vec<_> = shapes
        .iter()
        .filter_map(|s| match s {
            DrawShape::Polygon {
                points,
                fill: Some(f),
                ..
            } if f == "#cc4444" && points.len() == 3 => Some(points),
            _ => None,
        })
        .collect();
    assert_eq!(arrows.len(), 2);
    assert_eq!(arrows[0][0], (94.0, 150.0));
    assert!(shapes.iter().any(|s| matches!(
        s,
        DrawShape::Line {
            dash: Some((6.0, 3.0)),
            ..
        }
    )));
    // Weight labels are bold; box outlines 1.5.
    assert!(shapes
        .iter()
        .any(|s| matches!(s, DrawShape::Text { text, bold: true, .. } if text == "3")));
    assert!(shapes
        .iter()
        .any(|s| matches!(s, DrawShape::Rect { stroke_width, .. } if *stroke_width == 1.5)));
    // Box label: centered (dominant-baseline central), 12px, #333 ink.
    assert!(shapes.iter().any(|s| matches!(s,
        DrawShape::Text { text, size, color, x, central: true, .. } if text == "t" && *size == 12.0 && color == "#333333" && *x == 70.0)));
}

#[test]
fn empty_net_is_a_blank_canvas() {
    let mut s = Session::default();
    let out = run(&mut s, "view.string_diagram", Value::Null);
    assert_eq!(out["width"], 200.0);
    assert_eq!(out["height"], 100.0);
    let (_, _, shapes) = drawing(&s.latest("drawing").unwrap().data);
    assert_eq!(shapes.len(), 1, "background only");
    let places_only = doc(br#"{"places":{"p":{}},"transitions":{},"arcs":[]}"#);
    let d = translate(&places_only);
    assert!(d.boxes.is_empty() && (d.width, d.height) == (0.0, 0.0));
    assert_eq!(net_layout(&places_only), [("p".to_string(), 400.0, 370.0)]);
}

#[test]
fn listed_in_the_view_menu_and_journaled() {
    let reg = pflow_native_core::registry::registry();
    let c = reg.find("view.string_diagram").unwrap();
    assert_eq!(c.menu, "View");
    assert!(c.journal);
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(&mut s, "view.string_diagram", Value::Null);
    assert_eq!(s.journal.last().unwrap().command, "view.string_diagram");
}

#[test]
fn string_layout_is_a_net_layout_algorithm() {
    let mut s = pflow_native_core::Session::default();
    s.execute(
        "file.open_example",
        serde_json::json!({"name": "coffee-shop"}),
    )
    .unwrap();
    let r = s
        .execute("net.layout", serde_json::json!({"algorithm": "string"}))
        .unwrap();
    assert_eq!(r["algorithm"], "string");
    let pos: Vec<(String, f64, f64)> = pflow_native_core::string_diagram::net_layout(&s.doc);
    for (id, x, y) in pos {
        assert_eq!(s.doc.position(&id), Some((x, y)), "{id}");
    }
}

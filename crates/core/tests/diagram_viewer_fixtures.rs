//! `core::string_diagram::translate` against pflow.xyz's own
//! `diagram-viewer.js`, number for number, on the nets in
//! `tests/fixtures/diagram-viewer/` (provenance: its README.md; the outputs
//! are `translatePetriNet`'s, written by `gen.mjs`).

use pflow_native_core::string_diagram::{flat_document, translate, Diagram};
use pflow_native_core::{Document, Session};
use serde_json::{json, Value};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/diagram-viewer");

fn read(name: &str) -> Value {
    let p = format!("{DIR}/{name}");
    serde_json::from_slice(&std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))).unwrap()
}

fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}

/// What the layout must equal: every box field, every wire row, the
/// boundaries, the back edges and the size.
fn check(name: &str, d: &Diagram, js: &Value) {
    let ports = |ports: &[pflow_native_core::string_diagram::Port], jp: &Value, what: &str| {
        let jp = jp.as_array().unwrap();
        assert_eq!(ports.len(), jp.len(), "{name}: {what} port count");
        for (p, q) in ports.iter().zip(jp) {
            assert_eq!(p.wire, q["wireType"].as_str().unwrap(), "{name}: {what}");
            assert_eq!(p.inhibit, q["inhibit"].as_bool().unwrap(), "{name}: {what}");
            assert_eq!(
                p.weight,
                q["weight"][0].as_f64(),
                "{name}: {what} {} weight",
                p.wire
            );
        }
    };
    let jb = js["boxes"].as_array().unwrap();
    assert_eq!(d.boxes.len(), jb.len(), "{name}: boxes");
    for (b, q) in d.boxes.iter().zip(jb) {
        let id = q["id"].as_str().unwrap();
        assert_eq!(b.id, id, "{name}: box order");
        assert_eq!(b.label, q["label"].as_str().unwrap(), "{name}: {id} label");
        assert_eq!(b.layer as f64, f(&q["layer"]), "{name}: {id} layer");
        assert_eq!(
            b.pos_in_layer as f64,
            f(&q["posInLayer"]),
            "{name}: {id} posInLayer"
        );
        assert_eq!(b.x, f(&q["x"]), "{name}: {id} x");
        assert_eq!(b.y, f(&q["y"]), "{name}: {id} y");
        assert_eq!(b.width, f(&q["width"]), "{name}: {id} width");
        ports(&b.inputs, &q["inputs"], &format!("{id} inputs"));
        ports(&b.outputs, &q["outputs"], &format!("{id} outputs"));
    }
    // wires: rows of wireFields
    assert_eq!(
        js["wireFields"],
        json!([
            "wireType",
            "fromX",
            "fromY",
            "toX",
            "toY",
            "isCrossing",
            "isBack",
            "inhibit",
            "weight0"
        ])
    );
    let jw = js["wires"].as_array().unwrap();
    assert_eq!(d.wires.len(), jw.len(), "{name}: wires");
    for (i, (w, r)) in d.wires.iter().zip(jw).enumerate() {
        let ctx = format!("{name}: wire {i} ({})", w.wire);
        assert_eq!(w.wire, r[0].as_str().unwrap(), "{ctx}");
        assert_eq!(
            (w.from.0, w.from.1, w.to.0, w.to.1),
            (f(&r[1]), f(&r[2]), f(&r[3]), f(&r[4])),
            "{ctx}: ends"
        );
        assert_eq!(w.crossing, r[5].as_bool().unwrap(), "{ctx}: isCrossing");
        assert_eq!(w.back, r[6].as_bool().unwrap(), "{ctx}: isBack");
        assert_eq!(w.inhibit, r[7].as_bool().unwrap(), "{ctx}: inhibit");
        assert_eq!(w.weight, r[8].as_f64(), "{ctx}: weight");
    }
    let strs = |v: &Value| -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(
        d.boundary_inputs,
        strs(&js["boundaryInputs"]),
        "{name}: boundary in"
    );
    assert_eq!(
        d.boundary_outputs,
        strs(&js["boundaryOutputs"]),
        "{name}: boundary out"
    );
    let back: Vec<String> = d
        .back_edges
        .iter()
        .map(|(a, b)| format!("{a}->{b}"))
        .collect();
    assert_eq!(back, strs(&js["backEdges"]), "{name}: back edges");
    assert_eq!(
        (d.width, d.height),
        (f(&js["width"]), f(&js["height"])),
        "{name}: size"
    );
}

fn input_matches(js: &Value, bytes: &[u8]) {
    assert_eq!(
        js["input"]["bytes"].as_u64().unwrap(),
        bytes.len() as u64,
        "the fixture was made from a different input: rerun gen.mjs (see README.md)"
    );
}

#[test]
fn orders_flat_net_default_matches_diagram_viewer() {
    let js = read("orders-default.layout.json");
    let flat = read("orders-default.flat.json");
    let d = translate(&flat_document(&flat));
    check("orders-default", &d, &js);
    // the nets in the fixture are what bundle.flatten returns now
    let mut s = Session::default();
    s.execute("bundle.open", json!({})).unwrap();
    let now = s.execute("bundle.flatten", json!({})).unwrap();
    assert_eq!(now, flat, "bundle.flatten changed: regenerate the fixtures");
    assert_eq!(d.back_edges.len(), 3);
}

#[test]
fn orders_flat_net_payment2_pushout_matches_diagram_viewer() {
    let js = read("orders-payment2-pushout.layout.json");
    let flat = read("orders-payment2-pushout.flat.json");
    let d = translate(&flat_document(&flat));
    check("orders-payment2-pushout", &d, &js);
    let mut s = Session::default();
    s.execute("bundle.open", json!({})).unwrap();
    s.execute("bundle.add_instance", json!({"id": "payment2"}))
        .unwrap();
    s.execute(
        "bundle.add_link",
        json!({"kind": "token", "from": "payment:account:customer", "to": "payment2:account:customer"}),
    )
    .unwrap();
    assert_eq!(
        s.execute("bundle.flatten", json!({})).unwrap(),
        flat,
        "bundle.flatten changed: regenerate the fixtures"
    );
}

#[test]
fn connect4_matches_diagram_viewer() {
    let js = read("connect-4.layout.json");
    let bytes = include_bytes!("../../../examples/connect-4.json");
    input_matches(&js, bytes);
    let d = translate(&Document::from_json(bytes).unwrap());
    check("connect-4", &d, &js);
    assert_eq!(d.boxes.len(), 222);
}

#[test]
fn small_nets_match_diagram_viewer() {
    for name in ["sir", "coffee-shop", "mixed"] {
        let bytes = std::fs::read(format!("{DIR}/{name}.net.json")).unwrap();
        let js = read(&format!("{name}.layout.json"));
        input_matches(&js, &bytes);
        check(name, &translate(&Document::from_json(&bytes).unwrap()), &js);
    }
}

#[test]
fn geometry_helpers_follow_generate_svg() {
    let flat = read("orders-default.flat.json");
    let d = translate(&flat_document(&flat));
    let (w, _) = d.svg_size();
    let back = d.wires.iter().find(|w| w.back).expect("a back edge");
    let (a, b, label) = back.back_curves(w);
    assert_eq!(a[3], b[0]);
    assert_eq!(b[3], back.to);
    assert_eq!(a[2].0, w - 15.0);
    assert_eq!(label.0, w - 19.0);
    let fwd = d.wires.iter().find(|w| !w.back).unwrap().forward_curve();
    assert_eq!(fwd[1].0, fwd[0].0);
    assert_eq!(fwd[1].1, fwd[2].1);
    // every boundary place has a stub per port that carries it
    let stubs = d.stubs();
    for p in &d.boundary_inputs {
        assert!(stubs.iter().any(|s| s.input && &s.wire == p), "{p}");
    }
    for p in &d.boundary_outputs {
        assert!(stubs.iter().any(|s| !s.input && &s.wire == p), "{p}");
    }
}

#[test]
fn transition_layout_query_returns_the_layout() {
    let mut s = Session::default();
    s.execute("bundle.open", json!({})).unwrap();
    let out = s.execute("diagram.transition_layout", json!({})).unwrap();
    let js = read("orders-default.layout.json");
    assert_eq!(out["source"], "flat");
    assert_eq!(out["boxes"].as_array().unwrap().len(), 9);
    assert_eq!(out["wire_count"], js["wires"].as_array().unwrap().len());
    assert_eq!(f(&out["width"]), f(&js["width"]));
    assert_eq!(out["back_edges"].as_array().unwrap().len(), 3);
    // a box's ports carry their positions
    let b = &out["boxes"][0];
    assert_eq!(b["height"], 40.0);
    assert!(b["inputs"][0]["x"].is_number());
    let c4 = s
        .execute(
            "diagram.transition_layout",
            json!({"source": "connect4", "wires": false}),
        )
        .unwrap();
    assert_eq!(c4["boxes"].as_array().unwrap().len(), 222);
    assert_eq!(c4["wire_count"], 14296);
    assert_eq!(c4["wires"].as_array().unwrap().len(), 0);
    assert!(s
        .execute("diagram.transition_layout", json!({"source": "x"}))
        .is_err());
    // a query: not journaled
    assert!(s
        .journal
        .iter()
        .all(|j| j.command != "diagram.transition_layout"));
}

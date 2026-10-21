//! The String Diagram view (pflow.xyz's "String Diagram" dialog, pv:6324,
//! drawn by `diagram-viewer.js`): see [`crate::string_diagram`].

use serde_json::{json, Value};

use crate::registry::{always, CommandSpec};
use crate::session::{Session, Step};
use crate::string_diagram;
use crate::Result;

pub const SET: &[CommandSpec] = &[CommandSpec {
    id: "view.string_diagram",
    label: "String Diagram",
    help: "Draw the net as a monoidal string diagram (transitions as boxes, places as typed \
           wires, back edges routed around the right) and store it as a drawing artifact, \
           exportable as SVG.",
    menu: "View",
    shortcut: None,
    args: &[],
    enabled: always,
    run: string_diagram_cmd,
    journal: true,
}];

fn string_diagram_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let d = string_diagram::translate(&sess.doc);
    let data = string_diagram::drawing(&d);
    let (width, height) = match &data {
        crate::ArtifactData::Drawing { width, height, .. } => (*width, *height),
        _ => unreachable!("drawing() returns a Drawing"),
    };
    let boxes: Vec<Value> = d
        .boxes
        .iter()
        .map(|b| json!({"id": b.id, "layer": b.layer, "x": b.x, "y": b.y, "width": b.width}))
        .collect();
    let back_edges: Vec<Value> = d.back_edges.iter().map(|(a, b)| json!([a, b])).collect();
    let id = sess.add_artifact(
        "String diagram".into(),
        Step {
            command: "view.string_diagram".into(),
            args: a.clone(),
        },
        data,
    );
    Ok(json!({
        "artifact": id,
        "width": width,
        "height": height,
        "boxes": boxes,
        "wires": d.wires.len(),
        "back_edges": back_edges,
        "boundary_inputs": d.boundary_inputs,
        "boundary_outputs": d.boundary_outputs,
    }))
}

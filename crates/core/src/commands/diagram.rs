//! The String Diagram view (pflow.xyz's "String Diagram" dialog, pv:6324,
//! drawn by `diagram-viewer.js`): see [`crate::string_diagram`].

use serde_json::{json, Value};

use crate::registry::{always, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::string_diagram;
use crate::{Error, Result};

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
},
CommandSpec {
    id: "diagram.transition_layout",
    label: "Transition-level Layout",
    help: "The string diagram's layout, exactly as pflow.xyz's diagram-viewer.js computes it (translatePetriNet): \
           boxes with their input and output ports' positions, wires with their ends (back and crossing flags), \
           boundary places, back edges and the size. `source`: `flat` (the open composition's `bundle.flatten`, \
           the default) or `connect4` (the Connect-4 net the plan searches). `wires: false` leaves the wire list \
           out (the counts stay). A query: nothing is stored.",
    menu: "",
    shortcut: None,
    args: &[
        opt("source", Str, "flat (default) or connect4"),
        opt("wires", Bool, "include the wire list (default true)"),
    ],
    enabled: always,
    run: transition_layout,
    journal: false,
}];

/// `diagram.transition_layout`: the layout of the flat orders net or the
/// Connect-4 net.
fn transition_layout(sess: &mut Session, a: &Value) -> Result<Value> {
    let src = s(a, "source").unwrap_or("flat");
    let doc = match src {
        "flat" => {
            let c = sess
                .composition
                .as_ref()
                .ok_or_else(|| Error::BadArgs("no composition is open (bundle.open)".into()))?;
            let flat = crate::composition::flatten_json(&c.bundle);
            if flat["ok"] != true {
                return Err(Error::BadArgs(format!(
                    "the composition does not flatten: {}",
                    flat["error"].as_str().unwrap_or("")
                )));
            }
            string_diagram::flat_document(&flat)
        }
        "connect4" => crate::Document::from_json(crate::search_plan::CONNECT4_JSON.as_bytes())?,
        other => {
            return Err(Error::BadArgs(format!(
                "source is \"flat\" or \"connect4\", not {other:?}"
            )))
        }
    };
    let d = string_diagram::translate(&doc);
    let mut out = d.to_json(crate::registry::b(a, "wires").unwrap_or(true));
    out["source"] = json!(src);
    Ok(out)
}

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

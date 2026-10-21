//! Showing analysis results on the net: highlighting node sets (siphons,
//! invariant supports, orbits) and replaying traces (counterexamples,
//! reach witnesses) onto the canvas through the marking.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::artifact::ArtifactData;
use crate::registry::{always, arg, n, opt, req, s, strs, ArgType::*, CommandSpec};
use crate::session::{Highlight, Session};
use crate::{Error, Result};

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "view.highlight",
        label: "Highlight Nodes",
        help: "Emphasize nodes (and optionally arcs) on the canvas, e.g. a siphon or an invariant's support.",
        menu: "",
        shortcut: None,
        args: &[
            arg("nodes", StrList, "place/transition ids"),
            opt("label", Str, "what the highlight means"),
            opt("weights", Json, "{node: number} badges, e.g. invariant coefficients"),
        ],
        enabled: always,
        run: highlight,
        journal: false,
    },
    CommandSpec {
        id: "view.clear_highlight",
        label: "Clear Highlight",
        help: "Remove the canvas highlight.",
        menu: "View",
        shortcut: None,
        args: &[],
        enabled: always,
        run: clear,
        journal: false,
    },
    CommandSpec {
        id: "artifact.show_set",
        label: "Show Set on Net",
        help: "Highlight one set of a `sets` artifact (or one node of a `graph`'s data.nodes) on the canvas.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "artifact id"), arg("index", Int, "set index")],
        enabled: always,
        run: show_set,
        journal: false,
    },
    CommandSpec {
        id: "sim.goto",
        label: "Go to Trace Step",
        help: "Load a marking onto the net: step `step` of a trace artifact (0 = the start when the trace begins \
               with an initial marking; that step's transition is highlighted), or node `node` of a graph artifact \
               whose nodes carry data.marking (a reachability graph). One undo step.",
        menu: "",
        shortcut: None,
        args: &[
            arg("id", Str, "trace or graph artifact id"),
            opt("step", Int, "trace step index"),
            opt("node", Str, "graph node id (its data.marking is loaded)"),
        ],
        enabled: always,
        run: goto,
        journal: true,
    },
];

fn check_nodes(sess: &Session, nodes: &[String]) -> Result<()> {
    match nodes.iter().find(|n| sess.doc.kind_of(n).is_none()) {
        Some(n) => Err(Error::NotFound(format!("no node {n:?}"))),
        None => Ok(()),
    }
}

fn highlight(sess: &mut Session, a: &Value) -> Result<Value> {
    let nodes = strs(a, "nodes").unwrap();
    check_nodes(sess, &nodes)?;
    let weights: BTreeMap<String, f64> = match a.get("weights") {
        None | Some(Value::Null) => BTreeMap::new(),
        Some(w) => serde_json::from_value(w.clone())
            .map_err(|e| Error::BadArgs(format!("weights: {e}")))?,
    };
    let h = Highlight {
        label: s(a, "label").unwrap_or("").to_string(),
        nodes,
        arcs: vec![],
        weights,
    };
    let out = json!(h);
    sess.highlight = Some(h);
    Ok(out)
}

fn clear(sess: &mut Session, _: &Value) -> Result<Value> {
    sess.highlight = None;
    Ok(Value::Null)
}

fn show_set(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    let index = n(a, "index").unwrap() as usize;
    let art = sess
        .artifact(id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
    let h = match &art.data {
        ArtifactData::Sets { sets } => {
            let set = sets.get(index).ok_or_else(|| {
                Error::BadArgs(format!("set {index} out of range ({} sets)", sets.len()))
            })?;
            Highlight {
                label: set.label.clone(),
                nodes: set.nodes.clone(),
                arcs: vec![],
                weights: set.weights.clone(),
            }
        }
        other => {
            return Err(Error::BadArgs(format!(
                "artifact {id} is a {}, not a sets artifact",
                other.kind()
            )))
        }
    };
    check_nodes(sess, &h.nodes)?;
    let out = json!(h);
    sess.highlight = Some(h);
    Ok(out)
}

fn goto(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    let art = sess
        .artifact(id)
        .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
    let (label, transition, marking): (String, String, BTreeMap<String, Vec<i64>>) =
        match (&art.data, n(a, "step"), s(a, "node")) {
            (ArtifactData::Trace { steps }, Some(step), None) => {
                if step < 0.0 || step as usize >= steps.len() {
                    return Err(Error::BadArgs(format!(
                        "step {step} out of range (0..{})",
                        steps.len()
                    )));
                }
                let st = &steps[step as usize];
                (
                    format!("step {step}: {}", st.transition),
                    st.transition.clone(),
                    st.marking.clone(),
                )
            }
            (ArtifactData::Graph { nodes, .. }, None, Some(node)) => {
                let gn = nodes
                    .iter()
                    .find(|g| g.id == node)
                    .ok_or_else(|| Error::NotFound(format!("no node {node:?} in graph {id}")))?;
                let mk: BTreeMap<String, Vec<i64>> =
                    serde_json::from_value(gn.data["marking"].clone()).map_err(|_| {
                        Error::BadArgs(format!("graph node {node} carries no data.marking"))
                    })?;
                (format!("state {node}: {}", gn.label), String::new(), mk)
            }
            (ArtifactData::Trace { .. }, _, _) => {
                return Err(Error::BadArgs(
                    "a trace needs `step` (and no `node`)".into(),
                ))
            }
            (ArtifactData::Graph { .. }, _, _) => {
                return Err(Error::BadArgs(
                    "a graph needs `node` (and no `step`)".into(),
                ))
            }
            (other, _, _) => {
                return Err(Error::BadArgs(format!(
                    "artifact {id} is a {}, not a trace or graph",
                    other.kind()
                )))
            }
        };
    let n_colors = sess.doc.colors();
    let mut mk = sess.marking.clone();
    for (place, v) in &marking {
        if sess.doc.place(place).is_none() {
            return Err(Error::Invalid(format!(
                "{label} names place {place:?}, not in this net (was it edited?)"
            )));
        }
        let mut v = v.clone();
        v.resize(n_colors, 0);
        mk.insert(place.clone(), v);
    }
    sess.set_marking(mk, &label);
    let nodes = if sess.doc.transition(&transition).is_some() {
        vec![transition.clone()]
    } else {
        vec![]
    };
    sess.highlight = Some(Highlight {
        label: label.clone(),
        nodes,
        arcs: vec![],
        weights: BTreeMap::new(),
    });
    Ok(json!({ "loaded": label, "transition": transition, "marking": sess.marking }))
}

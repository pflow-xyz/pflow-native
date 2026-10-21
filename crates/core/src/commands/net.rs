use serde_json::{json, Value};

use crate::doc::{Arc, NodeKind, Place, Transition};
use crate::registry::{always, arg, b, n, nums, opt, req, s, strs, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

/// pflow.xyz snaps to a 10-unit grid.
pub const GRID: f64 = 10.0;

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "net.add_place",
        label: "Add Place",
        help: "Add a place at (x, y).",
        menu: "",
        shortcut: None,
        args: &[
            arg("x", Num, "x coordinate"),
            arg("y", Num, "y coordinate"),
            opt("id", Str, "id; generated as p<n> if absent"),
            opt("label", Str, "display label"),
            opt("initial", NumList, "initial tokens (per color)"),
            opt("capacity", NumList, "capacity per color, 0 = unbounded"),
            opt("snap", Bool, "snap to the 10-unit grid (default true)"),
        ],
        enabled: always,
        run: add_place,
        journal: true,
    },
    CommandSpec {
        id: "net.add_transition",
        label: "Add Transition",
        help: "Add a transition at (x, y).",
        menu: "",
        shortcut: None,
        args: &[
            arg("x", Num, "x coordinate"),
            arg("y", Num, "y coordinate"),
            opt("id", Str, "id; generated as t<n> if absent"),
            opt("label", Str, "display label"),
            opt("snap", Bool, "snap to the 10-unit grid (default true)"),
        ],
        enabled: always,
        run: add_transition,
        journal: true,
    },
    CommandSpec {
        id: "net.add_arc",
        label: "Add Arc",
        help: "Connect a place and a transition (either direction). inhibit=true makes an \
               input arc an inhibitor and an output arc a read arc.",
        menu: "",
        shortcut: None,
        args: &[
            arg("source", Str, "source node id"),
            arg("target", Str, "target node id"),
            opt("weight", NumList, "weight per color (default 1)"),
            opt("inhibit", Bool, "inhibitor / read arc"),
        ],
        enabled: always,
        run: add_arc,
        journal: true,
    },
    CommandSpec {
        id: "net.set_arc",
        label: "Set Arc",
        help: "Change an arc's weight and/or inhibitor flag.",
        menu: "",
        shortcut: None,
        args: &[
            arg("source", Str, "source node id"),
            arg("target", Str, "target node id"),
            opt("weight", NumList, "weight per color"),
            opt("inhibit", Bool, "inhibitor / read arc"),
        ],
        enabled: always,
        run: set_arc,
        journal: true,
    },
    CommandSpec {
        id: "net.delete_arc",
        label: "Delete Arc",
        help: "Remove the arc source → target.",
        menu: "",
        shortcut: None,
        args: &[
            arg("source", Str, "source node id"),
            arg("target", Str, "target node id"),
        ],
        enabled: always,
        run: delete_arc,
        journal: true,
    },
    CommandSpec {
        id: "net.move",
        label: "Move Node",
        help: "Move a node to (x, y).",
        menu: "",
        shortcut: None,
        args: &[
            arg("id", Str, "node id"),
            arg("x", Num, "x"),
            arg("y", Num, "y"),
            opt("snap", Bool, "snap to the grid (default true)"),
        ],
        enabled: always,
        run: move_to,
        journal: true,
    },
    CommandSpec {
        id: "net.move_by",
        label: "Move Nodes",
        help: "Move nodes (default: the selection) by (dx, dy).",
        menu: "",
        shortcut: None,
        args: &[
            arg("dx", Num, "x offset"),
            arg("dy", Num, "y offset"),
            opt("ids", StrList, "node ids; defaults to the selection"),
            opt("snap", Bool, "snap results to the grid (default true)"),
        ],
        enabled: always,
        run: move_by,
        journal: true,
    },
    CommandSpec {
        id: "net.set_label",
        label: "Set Label",
        help: "Set a node's display label (single line, ≤100 chars; empty clears it).",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "node id"), arg("label", Str, "new label")],
        enabled: always,
        run: set_label,
        journal: true,
    },
    CommandSpec {
        id: "net.set_place",
        label: "Set Place",
        help: "Set a place's initial tokens and/or capacity (per color). Resets the marking.",
        menu: "",
        shortcut: None,
        args: &[
            arg("id", Str, "place id"),
            opt("initial", NumList, "initial tokens per color"),
            opt("capacity", NumList, "capacity per color, 0 = unbounded"),
        ],
        enabled: always,
        run: set_place,
        journal: true,
    },
    CommandSpec {
        id: "net.add_tokens",
        label: "Add Tokens",
        help: "Add delta (may be negative) to a place's initial tokens of one color; \
               the Token tool's click / right-click.",
        menu: "",
        shortcut: None,
        args: &[
            arg("id", Str, "place id"),
            opt("delta", Int, "tokens to add (default 1)"),
            opt("color", Int, "color index (default 0)"),
        ],
        enabled: always,
        run: add_tokens,
        journal: true,
    },
    CommandSpec {
        id: "net.info",
        label: "Net Info",
        help: "Places, transitions and arcs with current tokens and enablement.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: info,
        journal: false,
    },
];

fn snap(v: f64, on: bool) -> f64 {
    if on {
        (v / GRID).round() * GRID
    } else {
        v
    }
}

fn check_new_id(s: &Session, id: &str) -> Result<()> {
    if id.is_empty() || id.chars().any(|c| c.is_whitespace()) {
        return Err(Error::BadArgs(format!("invalid id {id:?}")));
    }
    if s.doc.kind_of(id).is_some() {
        return Err(Error::Invalid(format!("id {id:?} already exists")));
    }
    Ok(())
}

fn check_label(label: &str) -> Result<()> {
    if label.contains('\n') || label.chars().count() > 100 {
        return Err(Error::BadArgs(
            "labels are single-line, at most 100 characters".into(),
        ));
    }
    Ok(())
}

fn color_vec(v: Option<Vec<f64>>, n: usize, default: f64) -> Vec<f64> {
    let mut v = v.unwrap_or_else(|| vec![default]);
    v.resize(n, 0.0);
    v
}

fn add_place(sess: &mut Session, a: &Value) -> Result<Value> {
    let on = b(a, "snap").unwrap_or(true);
    let id = match s(a, "id") {
        Some(id) => {
            check_new_id(sess, id)?;
            id.to_string()
        }
        None => sess.doc.fresh_id("p"),
    };
    if let Some(l) = s(a, "label") {
        check_label(l)?;
    }
    let ncol = sess.doc.colors();
    let p = Place {
        id: id.clone(),
        label: s(a, "label").map(str::to_string),
        initial: color_vec(nums(a, "initial"), ncol, 0.0),
        capacity: color_vec(nums(a, "capacity"), ncol, 0.0),
        x: snap(n(a, "x").unwrap(), on),
        y: snap(n(a, "y").unwrap(), on),
        extra: Default::default(),
    };
    sess.edit("add place", |s| {
        let tokens = p.initial.iter().map(|f| f.round() as i64).collect();
        s.marking.insert(p.id.clone(), tokens);
        s.doc.places.push(p);
        s.selection = vec![id.clone()];
        Ok(json!({ "id": id }))
    })
}

fn add_transition(sess: &mut Session, a: &Value) -> Result<Value> {
    let on = b(a, "snap").unwrap_or(true);
    let id = match s(a, "id") {
        Some(id) => {
            check_new_id(sess, id)?;
            id.to_string()
        }
        None => sess.doc.fresh_id("t"),
    };
    if let Some(l) = s(a, "label") {
        check_label(l)?;
    }
    let t = Transition {
        id: id.clone(),
        label: s(a, "label").map(str::to_string),
        role: None,
        x: snap(n(a, "x").unwrap(), on),
        y: snap(n(a, "y").unwrap(), on),
        extra: Default::default(),
    };
    sess.edit("add transition", |s| {
        s.doc.transitions.push(t);
        s.selection = vec![id.clone()];
        Ok(json!({ "id": id }))
    })
}

fn add_arc(sess: &mut Session, a: &Value) -> Result<Value> {
    let (src, dst) = (req(a, "source"), req(a, "target"));
    let ks = sess
        .doc
        .kind_of(src)
        .ok_or_else(|| Error::NotFound(format!("no node {src:?}")))?;
    let kt = sess
        .doc
        .kind_of(dst)
        .ok_or_else(|| Error::NotFound(format!("no node {dst:?}")))?;
    if ks == kt {
        let what = if ks == NodeKind::Place {
            "places"
        } else {
            "transitions"
        };
        return Err(Error::Invalid(format!("an arc cannot join two {what}")));
    }
    if sess.doc.arc_index(src, dst).is_some() {
        return Err(Error::Invalid(format!("arc {src} → {dst} already exists")));
    }
    let weight = color_vec(nums(a, "weight"), sess.doc.colors(), 1.0);
    if weight.iter().any(|w| *w < 0.0) {
        return Err(Error::BadArgs("weights must be non-negative".into()));
    }
    let arc = Arc {
        source: src.to_string(),
        target: dst.to_string(),
        weight,
        inhibit: b(a, "inhibit").unwrap_or(false),
        extra: Default::default(),
    };
    sess.edit("add arc", |s| {
        s.doc.arcs.push(arc);
        Ok(json!({ "source": src, "target": dst }))
    })
}

fn find_arc(sess: &Session, a: &Value) -> Result<usize> {
    let (src, dst) = (req(a, "source"), req(a, "target"));
    sess.doc
        .arc_index(src, dst)
        .ok_or_else(|| Error::NotFound(format!("no arc {src} → {dst}")))
}

fn set_arc(sess: &mut Session, a: &Value) -> Result<Value> {
    let i = find_arc(sess, a)?;
    let ncol = sess.doc.colors();
    let weight = nums(a, "weight").map(|w| color_vec(Some(w), ncol, 1.0));
    if weight.as_ref().is_some_and(|w| w.iter().any(|x| *x < 0.0)) {
        return Err(Error::BadArgs("weights must be non-negative".into()));
    }
    let inhibit = b(a, "inhibit");
    sess.edit("set arc", |s| {
        let arc = &mut s.doc.arcs[i];
        if let Some(w) = weight {
            arc.weight = w;
        }
        if let Some(inh) = inhibit {
            arc.inhibit = inh;
        }
        Ok(json!({ "weight": arc.weight, "inhibit": arc.inhibit }))
    })
}

fn delete_arc(sess: &mut Session, a: &Value) -> Result<Value> {
    let i = find_arc(sess, a)?;
    sess.edit("delete arc", |s| {
        let arc = s.doc.arcs.remove(i);
        Ok(json!({ "deleted": [arc.source, arc.target] }))
    })
}

fn set_pos(sess: &mut Session, id: &str, x: f64, y: f64) {
    if let Some(p) = sess.doc.place_mut(id) {
        (p.x, p.y) = (x, y);
    } else if let Some(t) = sess.doc.transition_mut(id) {
        (t.x, t.y) = (x, y);
    }
}

fn move_to(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    if sess.doc.kind_of(id).is_none() {
        return Err(Error::NotFound(format!("no node {id:?}")));
    }
    let on = b(a, "snap").unwrap_or(true);
    let (x, y) = (snap(n(a, "x").unwrap(), on), snap(n(a, "y").unwrap(), on));
    sess.edit("move", |s| {
        set_pos(s, id, x, y);
        Ok(json!({ "id": id, "x": x, "y": y }))
    })
}

fn move_by(sess: &mut Session, a: &Value) -> Result<Value> {
    let ids = strs(a, "ids").unwrap_or_else(|| sess.selection.clone());
    if ids.is_empty() {
        return Err(Error::BadArgs("no nodes to move".into()));
    }
    let on = b(a, "snap").unwrap_or(true);
    let (dx, dy) = (n(a, "dx").unwrap(), n(a, "dy").unwrap());
    let mut moves = vec![];
    for id in &ids {
        let (x, y) = sess
            .doc
            .position(id)
            .ok_or_else(|| Error::NotFound(format!("no node {id:?}")))?;
        moves.push((id.clone(), snap(x + dx, on), snap(y + dy, on)));
    }
    sess.edit("move", |s| {
        for (id, x, y) in &moves {
            set_pos(s, id, *x, *y);
        }
        Ok(json!({ "moved": ids }))
    })
}

fn set_label(sess: &mut Session, a: &Value) -> Result<Value> {
    let (id, label) = (req(a, "id"), req(a, "label"));
    check_label(label)?;
    let label = (!label.trim().is_empty()).then(|| label.to_string());
    if sess.doc.kind_of(id).is_none() {
        return Err(Error::NotFound(format!("no node {id:?}")));
    }
    sess.edit("label", |s| {
        if let Some(p) = s.doc.place_mut(id) {
            p.label = label.clone();
        } else if let Some(t) = s.doc.transition_mut(id) {
            t.label = label.clone();
        }
        Ok(json!({ "id": id, "label": label }))
    })
}

fn set_place(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    if sess.doc.place(id).is_none() {
        return Err(Error::NotFound(format!("no place {id:?}")));
    }
    let ncol = sess.doc.colors();
    let initial = nums(a, "initial").map(|v| color_vec(Some(v), ncol, 0.0));
    let capacity = nums(a, "capacity").map(|v| color_vec(Some(v), ncol, 0.0));
    if initial.iter().chain(&capacity).flatten().any(|x| *x < 0.0) {
        return Err(Error::BadArgs(
            "token counts and capacities are non-negative".into(),
        ));
    }
    sess.edit("set place", |s| {
        let p = s.doc.place_mut(id).expect("checked");
        if let Some(v) = initial {
            p.initial = v;
        }
        if let Some(v) = capacity {
            p.capacity = v;
        }
        let out = json!({ "id": id, "initial": p.initial, "capacity": p.capacity });
        s.reset_marking();
        Ok(out)
    })
}

fn add_tokens(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id");
    let delta = n(a, "delta").unwrap_or(1.0);
    let color = n(a, "color").unwrap_or(0.0) as usize;
    let p = sess
        .doc
        .place(id)
        .ok_or_else(|| Error::NotFound(format!("no place {id:?}")))?;
    if color >= sess.doc.colors() {
        return Err(Error::BadArgs(format!("color {color} out of range")));
    }
    let mut initial = p.initial.clone();
    initial.resize(sess.doc.colors(), 0.0);
    initial[color] = (initial[color] + delta).max(0.0);
    let cap = p.capacity.get(color).copied().unwrap_or(0.0);
    if cap > 0.0 && initial[color] > cap {
        return Err(Error::Invalid(format!("{id} is at capacity ({cap})")));
    }
    sess.edit("tokens", |s| {
        s.doc.place_mut(id).expect("checked").initial = initial.clone();
        s.reset_marking();
        Ok(json!({ "id": id, "initial": initial }))
    })
}

fn info(sess: &mut Session, _: &Value) -> Result<Value> {
    let enabled = sess.enabled_transitions();
    let places: Vec<Value> = sess
        .doc
        .places
        .iter()
        .map(|p| {
            json!({
                "id": p.id, "label": p.label, "x": p.x, "y": p.y,
                "initial": p.initial, "capacity": p.capacity,
                "tokens": sess.marking.get(&p.id),
            })
        })
        .collect();
    let transitions: Vec<Value> = sess
        .doc
        .transitions
        .iter()
        .map(|t| {
            json!({
                "id": t.id, "label": t.label, "x": t.x, "y": t.y,
                "rate": sess.rates.get(&t.id).copied().unwrap_or(1.0),
                "enabled": enabled.contains(&t.id),
            })
        })
        .collect();
    Ok(json!({
        "colors": sess.doc.token,
        "places": places,
        "transitions": transitions,
        "arcs": sess.doc.arcs,
        "selection": sess.selection,
        "dirty": sess.is_dirty(),
        "fired": sess.fired,
    }))
}

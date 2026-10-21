//! Composition (phase 6, sim.pflow.xyz's composition family): the
//! component library, fusing a component into the open net, structural
//! edit ops applied through the `net.*` commands, and diffs between
//! library models. The pure halves live in [`crate::compose`].

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

use crate::artifact::{ArtifactData, NodeSet};
use crate::compose::{self, Change, Params};
use crate::doc::{Arc, Document, NodeKind, Place, Transition};
use crate::registry::{always, arg, n, opt, req, s, ArgType::*, CommandSpec};
use crate::session::{Highlight, Session, Step};
use crate::{Error, Result};

/// Most operations one `compose.extend` call takes.
pub const MAX_OPS: usize = 1000;

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "compose.components",
        label: "Components",
        help: "The component library: sim.pflow.xyz's registry (arrivals, service, hazard, inventory, \
               decision, mailbox, datastore) and pflow-compose's templates (sir, seir, queue, \
               producer-consumer, workflow) and subnets (bounded-queue, resource-pool), each with its \
               ports (role: in/out/inout/observe), params with defaults, and discipline notes.",
        menu: "Compose",
        shortcut: None,
        args: &[],
        enabled: always,
        run: components,
        journal: false,
    },
    CommandSpec {
        id: "compose.fuse",
        label: "Fuse Component…",
        help: "Instantiate a library component into the open net (sim_compose): its nodes are renamed \
               <prefix>_<id> and placed at (x, y) (default: right of the net), and each port in `attach` \
               is fused onto an existing place (the shared place replaces the component's copy). Declared \
               rates become session rates. On a net with several token colors, `color` picks the one the \
               component's tokens and weights use. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[
            arg("component", Str, "library component name (compose.components)"),
            opt("attach", Json, "{port: existing place id} fusions"),
            opt("params", Json, "{param: number} overrides of the defaults"),
            opt("prefix", Str, "instance prefix for created ids (default: the component name)"),
            opt("x", Num, "left of the placed component (default: right of the net)"),
            opt("y", Num, "top of the placed component (default: the net's top)"),
            opt("color", Json, "token color (index or name) on a multi-color net"),
        ],
        enabled: always,
        run: fuse,
        journal: true,
    },
    CommandSpec {
        id: "compose.extend",
        label: "Extend (Edit Ops)…",
        help: "Apply sim_extend's structural edit ops to the open net, each as the matching registry \
               command, all as one undo step (any failure rolls everything back): add_place {id, initial, \
               capacity, x, y, label}, add_transition {id, rate, x, y, label}, add_arc {from, to, weight, \
               type: \"\"|inhibitor|read}, remove_place / remove_transition / remove {id} (arcs go too), \
               remove_arc {from, to}, set_rate {id, rate}, set_initial {id, initial}, set_capacity {id, \
               capacity}, set_arc {from, to, weight, type}, rename {id, new_id}, set_label {id, label}, \
               move {id, x, y}. Returns the steps run and the before/after diff.",
        menu: "Compose",
        shortcut: None,
        args: &[arg("ops", Json, "array of {op, ...} objects (or a JSON string of one)")],
        enabled: always,
        run: extend,
        journal: true,
    },
    CommandSpec {
        id: "compose.diff",
        label: "Diff Models…",
        help: "Structural diff of two models (sim_diff): library names or \"current\". Stores a table \
               (kind, id, change, before, after), a summary report, a side-by-side drawing, and, when one \
               side is the open net, node sets (added / removed / changed) to highlight on the canvas.",
        menu: "Compose",
        shortcut: None,
        args: &[
            opt("a", Str, "before: library name or \"current\" (default)"),
            arg("b", Str, "after: library name or \"current\""),
        ],
        enabled: always,
        run: diff,
        journal: true,
    },
];

fn components(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(compose::components()))
}

// ---------------------------------------------------------------- fuse

/// A JSON object argument, possibly given as a JSON string (sim's form).
fn object(a: &Value, k: &str) -> Result<Map<String, Value>> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(Map::new()),
        Some(Value::Object(m)) => Ok(m.clone()),
        Some(Value::String(t)) if t.trim().is_empty() => Ok(Map::new()),
        Some(Value::String(t)) => match serde_json::from_str(t) {
            Ok(Value::Object(m)) => Ok(m),
            _ => Err(Error::BadArgs(format!("{k} must be a JSON object"))),
        },
        Some(other) => Err(Error::BadArgs(format!(
            "{k} must be a JSON object, got {other}"
        ))),
    }
}

fn check_id(id: &str, what: &str) -> Result<()> {
    if id.is_empty() || id.chars().any(char::is_whitespace) {
        return Err(Error::BadArgs(format!("invalid {what} {id:?}")));
    }
    Ok(())
}

/// The color a component's single-color tokens go to.
fn color_index(doc: &Document, v: Option<&Value>) -> Result<usize> {
    let n = doc.colors();
    let names: Vec<String> = doc
        .token
        .iter()
        .map(|t| t.rsplit('/').next().unwrap_or(t).to_string())
        .collect();
    match v {
        None | Some(Value::Null) if n == 1 => Ok(0),
        None | Some(Value::Null) => Err(Error::BadArgs(format!(
            "the net has {n} token colors ({}); components are single-color, so pass `color` \
             (an index or name) for the one their tokens and weights use",
            names.join(", ")
        ))),
        Some(Value::Number(x)) => match x.as_u64() {
            Some(i) if (i as usize) < n => Ok(i as usize),
            _ => Err(Error::BadArgs(format!("color {x} out of range (0..{n})"))),
        },
        Some(Value::String(c)) => doc
            .token
            .iter()
            .zip(&names)
            .position(|(full, short)| full == c || short == c)
            .ok_or_else(|| {
                Error::BadArgs(format!(
                    "no token color {c:?}; the net has {}",
                    names.join(", ")
                ))
            }),
        Some(other) => Err(Error::BadArgs(format!(
            "color must be an index or a name, got {other}"
        ))),
    }
}

fn on_color(x: f64, c: usize, n: usize) -> Vec<f64> {
    let mut v = vec![0.0; n];
    v[c] = x;
    v
}

fn snap(v: f64, on: bool) -> f64 {
    if on {
        (v / super::net::GRID).round() * super::net::GRID
    } else {
        v
    }
}

fn fuse(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = req(a, "component");
    let mut explicit = Params::new();
    for (k, v) in object(a, "params")? {
        let x = v
            .as_f64()
            .ok_or_else(|| Error::BadArgs(format!("param {k:?} must be a number, got {v}")))?;
        explicit.insert(k, x);
    }
    let mut attach = BTreeMap::new();
    for (k, v) in object(a, "attach")? {
        let t = v
            .as_str()
            .ok_or_else(|| Error::BadArgs(format!("attach {k:?} must name a place id, got {v}")))?;
        attach.insert(k, t.to_string());
    }
    let inst = compose::instantiate(name, &explicit)?;
    let prefix = s(a, "prefix").unwrap_or(inst.name).to_string();
    check_id(&prefix, "prefix")?;

    // Port → fragment place → existing place.
    let mut fused: BTreeMap<String, String> = BTreeMap::new();
    let mut pairs = vec![];
    for (port, target) in &attach {
        let spec = inst.ports.iter().find(|p| &p.name == port).ok_or_else(|| {
            let names: Vec<&str> = inst.ports.iter().map(|p| p.name.as_str()).collect();
            Error::BadArgs(format!(
                "component {:?} has no port {port:?}; its ports are {names:?}",
                inst.name
            ))
        })?;
        if sess.doc.place(target).is_none() {
            return Err(Error::NotFound(format!(
                "attach {port}={target}: the net has no place {target:?}"
            )));
        }
        match fused.get(&spec.place) {
            Some(prev) if prev != target => {
                return Err(Error::BadArgs(format!(
                "ports sharing place {:?} are attached to different places ({prev:?}, {target:?})",
                spec.place
            )))
            }
            _ => {
                fused.insert(spec.place.clone(), target.clone());
            }
        }
        pairs.push(json!({ "port": port, "role": spec.role, "place": target }));
    }
    let attached: BTreeSet<String> = fused.keys().cloned().collect();
    compose::check_attached_params(&inst, &explicit, &attached)?;

    let ncol = sess.doc.colors();
    let color = color_index(&sess.doc, a.get("color"))?;
    let frag = &inst.fragment;
    let rename = |id: &str| -> String {
        fused
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!("{prefix}_{id}"))
    };
    let on = sess.view.settings.snap;
    let (ox, oy) = match (n(a, "x"), n(a, "y"), sess.doc.bounds()) {
        (x, y, Some((_, y0, x1, _))) => (x.unwrap_or(x1 + 150.0), y.unwrap_or(y0)),
        (x, y, None) => (x.unwrap_or(100.0), y.unwrap_or(100.0)),
    };
    let places: Vec<Place> = frag
        .places
        .iter()
        .filter(|p| !fused.contains_key(&p.id))
        .map(|p| Place {
            id: rename(&p.id),
            label: p.label.clone(),
            initial: on_color(p.initial, color, ncol),
            capacity: on_color(p.capacity, color, ncol),
            x: snap(ox + p.x, on),
            y: snap(oy + p.y, on),
            extra: Default::default(),
        })
        .collect();
    let transitions: Vec<Transition> = frag
        .transitions
        .iter()
        .map(|t| Transition {
            id: rename(&t.id),
            label: t.label.clone(),
            role: None,
            x: snap(ox + t.x, on),
            y: snap(oy + t.y, on),
            extra: Default::default(),
        })
        .collect();
    let mut created = BTreeSet::new();
    for id in places
        .iter()
        .map(|p| &p.id)
        .chain(transitions.iter().map(|t| &t.id))
    {
        if sess.doc.kind_of(id).is_some() || !created.insert(id.clone()) {
            return Err(Error::Invalid(format!(
                "id {id:?} already exists; pick another prefix"
            )));
        }
    }
    let mut arcs: Vec<Arc> = vec![];
    let mut notes = vec![];
    for fa in &frag.arcs {
        let (src, dst) = (rename(&fa.source), rename(&fa.target));
        if !fa.kinetic {
            notes.push(format!(
                "{src} -> {dst} is non-kinetic in {}; the pflow.xyz document has no kinetic flag, \
                 so the ODE treats it as mass-action",
                inst.name
            ));
        }
        let weight = on_color(fa.weight, color, ncol);
        let existing = arcs.iter_mut().find(|x| x.source == src && x.target == dst);
        if let Some(x) = existing {
            if x.inhibit != fa.inhibit {
                return Err(Error::Invalid(format!(
                    "fusing makes two different arcs {src} -> {dst}"
                )));
            }
            for (w, add) in x.weight.iter_mut().zip(&weight) {
                *w += add;
            }
            continue;
        }
        if sess.doc.arc_index(&src, &dst).is_some() {
            return Err(Error::Invalid(format!(
                "fusing would duplicate the existing arc {src} -> {dst}"
            )));
        }
        arcs.push(Arc {
            source: src,
            target: dst,
            weight,
            inhibit: fa.inhibit,
            extra: Default::default(),
        });
    }
    let rates: BTreeMap<String, f64> = frag
        .transitions
        .iter()
        .filter_map(|t| t.rate.map(|r| (rename(&t.id), r)))
        .collect();
    let place_ids: Vec<String> = places.iter().map(|p| p.id.clone()).collect();
    let transition_ids: Vec<String> = transitions.iter().map(|t| t.id.clone()).collect();
    let n_arcs = arcs.len();
    let label = format!("fuse {} as {prefix}", inst.name);
    sess.edit(&label, |s| {
        for p in &places {
            let tokens = p.initial.iter().map(|f| f.round() as i64).collect();
            s.marking.insert(p.id.clone(), tokens);
        }
        s.doc.places.extend(places);
        s.doc.transitions.extend(transitions);
        s.doc.arcs.extend(arcs);
        s.selection = place_ids.iter().chain(&transition_ids).cloned().collect();
        Ok(())
    })?;
    for (id, r) in &rates {
        sess.rates.insert(id.clone(), *r);
    }
    let mut lit: Vec<String> = sess.selection.clone();
    lit.extend(fused.values().cloned());
    sess.highlight = Some(Highlight {
        label,
        nodes: lit,
        arcs: vec![],
        weights: BTreeMap::new(),
    });
    Ok(json!({
        "component": inst.name,
        "prefix": prefix,
        "params": inst.params,
        "places": place_ids,
        "transitions": transition_ids,
        "arcs": n_arcs,
        "fused": pairs,
        "rates": rates,
        "color": color,
        "at": [ox, oy],
        "notes": notes,
    }))
}

// -------------------------------------------------------------- extend

/// The open net as the ops see it while they are being translated, so
/// later ops can name what earlier ones made (or removed).
struct Shadow {
    nodes: BTreeMap<String, NodeKind>,
    arcs: BTreeSet<(String, String)>,
    /// Arcs a removed node took with it.
    cascaded: BTreeSet<(String, String)>,
    /// Next automatic position for a place / a transition.
    place_at: (f64, f64),
    transition_at: (f64, f64),
}

impl Shadow {
    fn new(doc: &Document) -> Self {
        let nodes = doc
            .places
            .iter()
            .map(|p| (p.id.clone(), NodeKind::Place))
            .chain(
                doc.transitions
                    .iter()
                    .map(|t| (t.id.clone(), NodeKind::Transition)),
            )
            .collect();
        let arcs = doc
            .arcs
            .iter()
            .map(|a| (a.source.clone(), a.target.clone()))
            .collect();
        let (x, y) = match doc.bounds() {
            Some((_, y0, x1, _)) => (x1 + 120.0, y0),
            None => (100.0, 100.0),
        };
        Shadow {
            nodes,
            arcs,
            cascaded: BTreeSet::new(),
            place_at: (x, y),
            transition_at: (x + 100.0, y),
        }
    }

    fn kind(&self, id: &str) -> Option<NodeKind> {
        self.nodes.get(id).copied()
    }

    fn remove_node(&mut self, id: &str) {
        self.nodes.remove(id);
        let gone: Vec<_> = self
            .arcs
            .iter()
            .filter(|(s, t)| s == id || t == id)
            .cloned()
            .collect();
        for a in gone {
            self.arcs.remove(&a);
            self.cascaded.insert(a);
        }
    }
}

fn field<'a>(op: &'a Map<String, Value>, k: &str) -> Option<&'a Value> {
    op.get(k).filter(|v| !v.is_null())
}

fn text<'a>(op: &'a Map<String, Value>, k: &str) -> &'a str {
    field(op, k).and_then(Value::as_str).unwrap_or("")
}

fn need<'a>(
    op: &'a Map<String, Value>,
    k: &str,
    name: &str,
) -> std::result::Result<&'a str, String> {
    match text(op, k) {
        "" => Err(format!("missing '{k}' for {name}")),
        v => Ok(v),
    }
}

/// A number or per-color number list, as `net.*` takes them.
fn numbers(op: &Map<String, Value>, k: &str) -> std::result::Result<Option<Value>, String> {
    match field(op, k) {
        None => Ok(None),
        Some(v @ Value::Number(_)) => Ok(Some(v.clone())),
        Some(v @ Value::Array(a)) if a.iter().all(Value::is_number) => Ok(Some(v.clone())),
        Some(v) => Err(format!(
            "'{k}' must be a number or a list of numbers, got {v}"
        )),
    }
}

fn step(command: &str, args: Value) -> Step {
    Step {
        command: command.into(),
        args,
    }
}

/// Translates one op into registry steps against the shadow net.
fn translate(
    sh: &mut Shadow,
    op: &Map<String, Value>,
    notes: &mut Vec<String>,
) -> std::result::Result<Vec<Step>, String> {
    let name = text(op, "op");
    let label = |args: &mut Map<String, Value>| {
        let l = match text(op, "label") {
            "" => text(op, "description"),
            l => l,
        };
        if !l.is_empty() {
            args.insert("label".into(), json!(l));
        }
    };
    let pos = |args: &mut Map<String, Value>, at: &mut (f64, f64)| {
        match (
            field(op, "x").and_then(Value::as_f64),
            field(op, "y").and_then(Value::as_f64),
        ) {
            (Some(x), Some(y)) => {
                args.insert("x".into(), json!(x));
                args.insert("y".into(), json!(y));
            }
            (None, None) => {
                args.insert("x".into(), json!(at.0));
                args.insert("y".into(), json!(at.1));
                at.1 += 80.0;
            }
            _ => return Err("give both 'x' and 'y', or neither".to_string()),
        }
        Ok(())
    };
    let new_node = |sh: &Shadow, id: &str| {
        if id.chars().any(char::is_whitespace) {
            Err(format!("invalid id {id:?}"))
        } else if sh.kind(id).is_some() {
            Err(format!("id {id:?} already exists"))
        } else {
            Ok(())
        }
    };
    let deprecated = |what: &str| {
        Err(format!(
            "{what} is deprecated: roles are now managed via extensions"
        ))
    };
    Ok(match name {
        "add_place" => {
            let id = need(op, "id", name)?;
            new_node(sh, id)?;
            let mut args = Map::new();
            args.insert("id".into(), json!(id));
            pos(&mut args, &mut sh.place_at)?;
            label(&mut args);
            for k in ["initial", "capacity"] {
                if let Some(v) = numbers(op, k)? {
                    args.insert(k.into(), v);
                }
            }
            args.insert("snap".into(), json!(false));
            sh.nodes.insert(id.into(), NodeKind::Place);
            vec![step("net.add_place", Value::Object(args))]
        }
        "add_transition" => {
            let id = need(op, "id", name)?;
            new_node(sh, id)?;
            if !text(op, "guard").is_empty() {
                return Err("guards are not representable in a pflow.xyz document".into());
            }
            if !text(op, "event").is_empty() {
                notes.push(format!(
                    "{id}: event {:?} ignored (no events in a pflow.xyz document)",
                    text(op, "event")
                ));
            }
            let mut args = Map::new();
            args.insert("id".into(), json!(id));
            pos(&mut args, &mut sh.transition_at)?;
            label(&mut args);
            args.insert("snap".into(), json!(false));
            sh.nodes.insert(id.into(), NodeKind::Transition);
            let mut steps = vec![step("net.add_transition", Value::Object(args))];
            if let Some(r) = field(op, "rate") {
                steps.push(step("sim.set_rate", json!({ "id": id, "rate": r })));
            }
            steps
        }
        "add_arc" | "set_arc" => {
            let (from, to) = (need(op, "from", name)?, need(op, "to", name)?);
            let typ = text(op, "type");
            let inhibit = match (typ, field(op, "inhibit").and_then(Value::as_bool)) {
                ("", Some(b)) => Some(b),
                ("", None) => (name == "add_arc").then_some(false),
                ("inhibitor" | "read", _) => Some(true),
                (t, _) => {
                    return Err(format!(
                        "arc type must be \"\", \"inhibitor\" or \"read\", got {t:?}"
                    ))
                }
            };
            // sim's read arc runs place → transition; the document's runs
            // transition → place (an output-side inhibit).
            let (src, dst) = if typ == "read" && sh.kind(from) == Some(NodeKind::Place) {
                (to, from)
            } else {
                (from, to)
            };
            if typ == "inhibitor" && sh.kind(src) != Some(NodeKind::Place) {
                return Err(format!(
                    "an inhibitor arc runs place -> transition, not {src} -> {dst}"
                ));
            }
            if field(op, "kinetic").and_then(Value::as_bool) == Some(false) {
                notes.push(format!("{src} -> {dst}: kinetic false dropped (no kinetic flag in a pflow.xyz document)"));
            }
            let mut args = Map::new();
            args.insert("source".into(), json!(src));
            args.insert("target".into(), json!(dst));
            if let Some(w) = numbers(op, "weight")? {
                if w.as_f64() != Some(0.0) {
                    args.insert("weight".into(), w);
                }
            }
            if let Some(b) = inhibit {
                args.insert("inhibit".into(), json!(b));
            }
            if name == "add_arc" {
                sh.arcs.insert((src.into(), dst.into()));
                vec![step("net.add_arc", Value::Object(args))]
            } else {
                vec![step("net.set_arc", Value::Object(args))]
            }
        }
        "remove_place" | "remove_transition" | "remove" => {
            let id = need(op, "id", name)?;
            let want = match name {
                "remove_place" => Some(NodeKind::Place),
                "remove_transition" => Some(NodeKind::Transition),
                _ => None,
            };
            match (sh.kind(id), want) {
                (None, _) => return Err(format!("no node {id:?}")),
                (Some(k), Some(w)) if k != w => {
                    return Err(format!(
                        "{id:?} is not a {}",
                        if w == NodeKind::Place {
                            "place"
                        } else {
                            "transition"
                        }
                    ))
                }
                _ => {}
            }
            sh.remove_node(id);
            vec![step("edit.delete", json!({ "ids": [id] }))]
        }
        "remove_arc" => {
            let (from, to) = (need(op, "from", name)?, need(op, "to", name)?);
            let key = |s: &str, t: &str| (s.to_string(), t.to_string());
            if sh.arcs.remove(&key(from, to)) {
                vec![step(
                    "net.delete_arc",
                    json!({ "source": from, "target": to }),
                )]
            } else if sh.kind(from) == Some(NodeKind::Place) && sh.arcs.remove(&key(to, from)) {
                // A read arc named sim's way round.
                vec![step(
                    "net.delete_arc",
                    json!({ "source": to, "target": from }),
                )]
            } else if sh.cascaded.contains(&key(from, to)) || sh.cascaded.contains(&key(to, from)) {
                notes.push(format!("{from} -> {to} already went with a removed node"));
                vec![]
            } else {
                return Err(format!("no arc {from} -> {to}"));
            }
        }
        "set_rate" => {
            let id = need(op, "id", name)?;
            let r = field(op, "rate").ok_or("missing 'rate' for set_rate")?;
            vec![step("sim.set_rate", json!({ "id": id, "rate": r }))]
        }
        "set_initial" | "set_capacity" => {
            let id = need(op, "id", name)?;
            let k = &name[4..];
            let v = numbers(op, k)?.ok_or_else(|| format!("missing '{k}' for {name}"))?;
            vec![step("net.set_place", json!({ "id": id, k: v }))]
        }
        "rename" => {
            let id = need(op, "id", name)?;
            let new_id = match text(op, "new_id") {
                "" => need(op, "to", name)?,
                v => v,
            };
            let kind = sh.kind(id).ok_or_else(|| format!("no node {id:?}"))?;
            new_node(sh, new_id)?;
            sh.nodes.remove(id);
            sh.nodes.insert(new_id.into(), kind);
            let arcs: Vec<_> = std::mem::take(&mut sh.arcs).into_iter().collect();
            sh.arcs = arcs
                .into_iter()
                .map(|(s, t)| {
                    let r = |x: String| if x == id { new_id.to_string() } else { x };
                    (r(s), r(t))
                })
                .collect();
            vec![step("net.rename", json!({ "id": id, "new_id": new_id }))]
        }
        "set_label" => {
            let id = need(op, "id", name)?;
            vec![step(
                "net.set_label",
                json!({ "id": id, "label": text(op, "label") }),
            )]
        }
        "move" => {
            let id = need(op, "id", name)?;
            let (Some(x), Some(y)) = (
                field(op, "x").and_then(Value::as_f64),
                field(op, "y").and_then(Value::as_f64),
            ) else {
                return Err("move needs 'x' and 'y'".into());
            };
            vec![step(
                "net.move",
                json!({ "id": id, "x": x, "y": y, "snap": false }),
            )]
        }
        "add_role" | "add_access" | "remove_role" | "remove_access" => return deprecated(name),
        "add_event" | "add_event_field" | "remove_event" | "add_binding" | "remove_binding" => {
            return Err("events and bindings are not representable in a pflow.xyz document".into())
        }
        "" => return Err("missing 'op' field".into()),
        other => return Err(format!("unknown operation {other:?}")),
    })
}

fn parse_ops(a: &Value) -> Result<Vec<Map<String, Value>>> {
    let v = match a.get("ops") {
        Some(Value::String(t)) => {
            serde_json::from_str(t).map_err(|e| Error::BadArgs(format!("ops JSON: {e}")))?
        }
        Some(v) => v.clone(),
        None => Value::Null,
    };
    let list = v
        .as_array()
        .ok_or_else(|| Error::BadArgs("ops must be an array of {op, ...} objects".into()))?;
    if list.is_empty() {
        return Err(Error::BadArgs("no operations given".into()));
    }
    if list.len() > MAX_OPS {
        return Err(Error::BadArgs(format!(
            "at most {MAX_OPS} operations per call"
        )));
    }
    list.iter()
        .enumerate()
        .map(|(i, o)| {
            o.as_object()
                .cloned()
                .ok_or_else(|| Error::BadArgs(format!("operation {i} is not an object")))
        })
        .collect()
}

/// Nodes of `doc` a diff touches, for the canvas.
fn touched(doc: &Document, changes: &[Change], which: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for c in changes.iter().filter(|c| which.contains(&c.change)) {
        for n in &c.nodes {
            if doc.kind_of(n).is_some() && !out.contains(n) {
                out.push(n.clone());
            }
        }
    }
    out
}

fn extend(sess: &mut Session, a: &Value) -> Result<Value> {
    let ops = parse_ops(a)?;
    let mut sh = Shadow::new(&sess.doc);
    let mut steps = vec![];
    let mut applied = vec![];
    let mut errors = vec![];
    let mut notes = vec![];
    for (i, op) in ops.iter().enumerate() {
        match translate(&mut sh, op, &mut notes) {
            Ok(s) => {
                steps.extend(s);
                applied.push(text(op, "op").to_string());
            }
            Err(e) => errors.push(format!("operation {i} ({}): {e}", text(op, "op"))),
        }
    }
    if !errors.is_empty() {
        return Err(Error::Invalid(format!(
            "nothing applied: {}",
            errors.join("; ")
        )));
    }
    let before = sess.doc.clone();
    let rates = sess.rates.clone();
    if let Err(e) = sess.batch(&steps) {
        sess.rates = rates;
        return Err(e);
    }
    let changes = compose::diff(&before, &sess.doc);
    let lit = touched(&sess.doc, &changes, &["added", "modified", "removed"]);
    sess.highlight = (!lit.is_empty()).then(|| Highlight {
        label: "compose.extend: changed".into(),
        nodes: lit,
        arcs: vec![],
        weights: BTreeMap::new(),
    });
    Ok(json!({
        "applied": applied,
        "steps": steps,
        "diff": compose::sim_shape(&changes),
        "notes": notes,
    }))
}

// ---------------------------------------------------------------- diff

fn is_current(name: &str) -> bool {
    name.is_empty() || name == "current"
}

fn diff(sess: &mut Session, a: &Value) -> Result<Value> {
    let name_a = s(a, "a").unwrap_or("current").to_string();
    let name_b = req(a, "b").to_string();
    let da = super::models::resolve(sess, &name_a)?;
    let db = super::models::resolve(sess, &name_b)?;
    let changes = compose::diff(&da, &db);
    let source = Step {
        command: "compose.diff".into(),
        args: a.clone(),
    };
    let title = format!("Diff {name_a} → {name_b}");
    let rows: Vec<Vec<Value>> = changes
        .iter()
        .map(|c| {
            vec![
                json!(c.kind),
                json!(c.id),
                json!(c.change),
                json!(c.before),
                json!(c.after),
            ]
        })
        .collect();
    let table = sess.add_artifact(
        format!("{title}: changes"),
        source.clone(),
        ArtifactData::Table {
            columns: ["kind", "id", "change", "before", "after"]
                .map(String::from)
                .to_vec(),
            rows,
        },
    );
    let count = |c: &str| changes.iter().filter(|x| x.change == c).count();
    let sim = compose::sim_shape(&changes);
    let mut notes =
        vec!["positions are not compared; rates are session-only and not part of a model"];
    let sets = if is_current(&name_a) || is_current(&name_b) {
        let doc = &sess.doc;
        let set = |label: &str, which: &[&str], note: &str| NodeSet {
            label: label.into(),
            nodes: touched(doc, &changes, which),
            weights: BTreeMap::new(),
            note: note.into(),
        };
        let side = if is_current(&name_a) { "a" } else { "b" };
        let data = ArtifactData::Sets {
            sets: vec![
                set(
                    "added",
                    &["added"],
                    "nodes added, and endpoints of added arcs, present in the open net",
                ),
                set(
                    "removed",
                    &["removed"],
                    "nodes removed, and endpoints of removed arcs, present in the open net",
                ),
                set(
                    "changed",
                    &["added", "removed", "modified"],
                    "every open-net node the diff touches",
                ),
            ],
        };
        let id = sess.add_artifact(
            format!("{title}: on the open net ({side})"),
            source.clone(),
            data,
        );
        Some(id)
    } else {
        notes.push("neither side is the open net, so no node sets were stored");
        None
    };
    let (width, height, shapes) = compose::side_by_side((&name_a, &da), (&name_b, &db), &changes);
    let drawing = sess.add_artifact(
        format!("{title}: side by side"),
        source.clone(),
        ArtifactData::Drawing {
            width,
            height,
            shapes,
        },
    );
    let report_body = json!({
        "a": name_a,
        "b": name_b,
        "has_changes": !changes.is_empty(),
        "added": count("added"),
        "removed": count("removed"),
        "modified": count("modified"),
        "diff": sim,
        "notes": notes,
    });
    let report = sess.add_artifact(
        format!("{title}: summary"),
        source,
        ArtifactData::Report(report_body.clone()),
    );
    let mut artifacts = vec![json!(table), json!(report), json!(drawing)];
    if let Some(id) = &sets {
        artifacts.push(json!(id));
    }
    let mut out = report_body;
    out["table"] = json!(table);
    out["report"] = json!(report);
    out["drawing"] = json!(drawing);
    out["sets"] = json!(sets);
    out["artifacts"] = json!(artifacts);
    out["changes"] = json!(changes);
    Ok(out)
}

//! Systems commands: ports on models, checked bindings, stored systems and
//! pipeline runs (sim.pflow.xyz's sim_bind / sim_get_binding /
//! sim_list_bindings / sim_create_system / sim_system / sim_run_pipeline).
//! The rules and engines live in [`crate::systems`]; this file reads
//! arguments, keeps the state and stores artifacts.
//!
//! # Where the state lives
//!
//! Ports are part of each model's document (its `"ports"` field), so they
//! save, load, copy into the library and undo with it. Bindings and systems
//! are not part of any one model (a binding never mutates either end, as in
//! sim); until `Session` grows a field for them they are kept in the model
//! library under the reserved name [`STATE_MODEL`], as the `"systems"` field
//! of an otherwise empty document, behind [`state`] / [`save_state`]. Like
//! the rest of the library they are not part of undo.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::artifact::{ArtifactData, GraphEdge, GraphNode, Series};
use crate::doc::Document;
use crate::registry::{always, arg, b, n, opt, s, strs, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::systems::{
    self as sy, Binding, BindingCheck, Engine, ModelInput, PortDecl, PortRef, RunOptions, System,
    SystemsState, UnitConversion,
};
use crate::{Error, Result};

/// The reserved library entry bindings and systems are kept in.
pub const STATE_MODEL: &str = "@systems";
const STATE_FIELD: &str = "systems";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "system.set_port",
        label: "Declare Port…",
        help: "Declare (or with remove: true, withdraw) a port on a model: an output on a place (its trajectory is what a binding reads) or an input on a transition (its rate is what a binding drives), with an optional kind and unit that bindings must match. Stored in the document's \"ports\" field, so it saves with the net.",
        menu: "Systems",
        shortcut: None,
        args: &[
            arg("id", Str, "element id: a place for an output port, a transition for an input port"),
            arg("direction", Str, "output (out) or input (in)"),
            opt("model", Str, "library model name, or \"current\" (default)"),
            opt("kind", Str, "port kind, compared exactly when binding (e.g. \"OrderStream\")"),
            opt("unit", Str, "unit the port counts in (e.g. \"order\"), compared exactly"),
            opt("description", Str, "free text"),
            opt("remove", Bool, "withdraw the port instead of declaring it"),
        ],
        enabled: always,
        run: set_port,
        journal: true,
    },
    CommandSpec {
        id: "system.ports",
        label: "List Ports",
        help: "Declared ports of a model, or (no model) of the open net and every library model that declares any.",
        menu: "",
        shortcut: None,
        args: &[opt("model", Str, "library model name or \"current\"")],
        enabled: always,
        run: ports,
        journal: false,
    },
    CommandSpec {
        id: "system.bind",
        label: "Bind Ports…",
        help: "Record a checked binding from from_model's output port (a place) to to_model's input port (a transition), as sim_bind: both ports exist with the right direction and element, kinds are equal, units agree where both declare one (or a declared conversion joins them), a numeric transform is at most 1e6, a delay is in (0, 10000] hours. A refusal stores nothing; the same binding twice returns the same id.",
        menu: "Systems",
        shortcut: None,
        args: &[
            arg("from_model", Str, "model supplying the output (library name or \"current\")"),
            arg("from_port", Str, "element id of its declared output port (a place)"),
            arg("to_model", Str, "model receiving it"),
            arg("to_port", Str, "element id of its declared input port (a transition)"),
            opt("transform", Json, "a plain non-negative number scales the resampled rate; other text is recorded and run 1:1"),
            opt("convert_from", Str, "declared unit conversion: from_port's unit"),
            opt("convert_to", Str, "declared unit conversion: to_port's unit"),
            opt("convert_factor", Num, "how many convert_to one convert_from is (1e-6..1e6)"),
            opt("delay", Num, "hours after the source's flow that to_port receives it, in (0, 10000]; a loop between models needs one"),
        ],
        enabled: always,
        run: bind,
        journal: true,
    },
    CommandSpec {
        id: "system.binding",
        label: "Get Binding",
        help: "A binding by id with its check computed now (sim_get_binding): ok (with the unit verdict), refused (a model changed since it was bound) or uncheckable (a model it names is gone).",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "binding id")],
        enabled: always,
        run: get_binding,
        journal: false,
    },
    CommandSpec {
        id: "system.bindings",
        label: "List Bindings",
        help: "Every binding (or those touching `model`), with its check verdict now.",
        menu: "",
        shortcut: None,
        args: &[opt("model", Str, "only bindings from or to this model")],
        enabled: always,
        run: list_bindings,
        journal: false,
    },
    CommandSpec {
        id: "system.unbind",
        label: "Delete Binding",
        help: "Remove a binding. Refused while a stored system names it (system.delete that first).",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "binding id")],
        enabled: always,
        run: unbind,
        journal: true,
    },
    CommandSpec {
        id: "system.create",
        label: "Create System…",
        help: "Store a named system of models and bindings (sim_create_system): every binding must pass its check now, and the set must close no loop without a delayed binding, take each input port once, fit the schedule-segment cap and hold no loop over 4 models. Binding endpoints join the models (implied). Stores a Graph (one node per model, one edge per binding) and a Report.",
        menu: "Systems",
        shortcut: None,
        args: &[
            arg("name", Str, "system name"),
            opt("models", StrList, "model names (library names or \"current\")"),
            opt("bindings", StrList, "binding ids from system.bind"),
            opt("overwrite", Bool, "replace an existing system of this name"),
        ],
        enabled: always,
        run: create,
        journal: true,
    },
    CommandSpec {
        id: "system.show",
        label: "Show System…",
        help: "A stored system's view, computed now (sim_system {id}): each model (present or missing) with its ports, each binding with its check, whether the set runs as a pipeline (order, loops, maxHours), and for every port what is bound to it and what else in the system could be. Stores a Graph and a Report.",
        menu: "Systems",
        shortcut: None,
        args: &[arg("name", Str, "system name")],
        enabled: always,
        run: show,
        journal: true,
    },
    CommandSpec {
        id: "system.list",
        label: "List Systems",
        help: "Stored systems with their models and bindings.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: list_systems,
        journal: false,
    },
    CommandSpec {
        id: "system.delete",
        label: "Delete System",
        help: "Remove a stored system (its bindings stay).",
        menu: "",
        shortcut: None,
        args: &[arg("name", Str, "system name")],
        enabled: always,
        run: delete_system,
        journal: true,
    },
    CommandSpec {
        id: "system.run",
        label: "Run Pipeline…",
        help: "Run a system (or a list of bindings) as a pipeline (sim_run_pipeline): every model runs as itself in topological order, each binding's source place trajectory resampled into the target transition's rate schedule (× transform × conversion, shifted by its delay); a loop through a delayed binding is solved by sweeps to its fixed point. Stores a TimeSeries overlay (group = model), a Report (order, loop verdict, per-binding flows, assumptions) and, with graph: true, an annotated Graph.",
        menu: "Systems",
        shortcut: None,
        args: &[
            opt("name", Str, "stored system to run"),
            opt("bindings", StrList, "binding ids to run instead of a system"),
            opt("hours", Num, "horizon shared by every model (default 8, ≤ 10000)"),
            opt("t_end", Num, "alias of hours"),
            opt("engine", Str, "ode (default: mass-action ODE, deterministic) or ssa (sim's scenario engine)"),
            opt("seed", Int, "ssa: shared seed (default 1)"),
            opt("realizations", Int, "ssa: realizations per model (default 1, ≤ 200)"),
            opt("sweeps", Int, "most sweeps per loop (2..24; default: as many as the loop's delay guarantees)"),
            opt("tolerance", Num, "a loop also settles when its cut schedule moves by at most this (rate/h) between sweeps (default 0: exact fixed point)"),
            opt("graph", Bool, "also store a Graph annotated with the run"),
        ],
        enabled: always,
        run: run_cmd,
        journal: true,
    },
];

// ------------------------------------------------------------------ state

/// The bindings and systems this session holds.
pub fn state(sess: &Session) -> SystemsState {
    sess.models
        .get(STATE_MODEL)
        .and_then(|d| d.extra.get(STATE_FIELD))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Replaces the bindings and systems this session holds.
pub fn save_state(sess: &mut Session, st: &SystemsState) {
    if st.bindings.is_empty() && st.systems.is_empty() {
        sess.models.remove(STATE_MODEL);
        return;
    }
    let mut doc = Document::default();
    doc.extra.insert(
        STATE_FIELD.into(),
        serde_json::to_value(st).expect("serializable"),
    );
    sess.models.insert(STATE_MODEL.into(), doc);
}

fn source(command: &str, a: &Value) -> Step {
    Step {
        command: command.into(),
        args: a.clone(),
    }
}

/// A model name as a binding stores it: "" is "current".
fn model_name(name: &str) -> Result<String> {
    match name {
        "" | "current" => Ok("current".into()),
        STATE_MODEL => Err(Error::BadArgs(format!(
            "{STATE_MODEL} is reserved for the systems state, not a model"
        ))),
        n => Ok(n.into()),
    }
}

fn doc_of(sess: &Session, name: &str) -> Result<Document> {
    super::models::resolve(sess, &model_name(name)?)
}

/// A model as a run reads it: a library model from its initial marking with
/// its declared rates; the open net from the session marking with the
/// session rates on top.
fn model_input(sess: &Session, name: &str) -> Result<ModelInput> {
    if name == "current" {
        let doc = sess.doc.clone();
        Ok(ModelInput {
            rates: sy::rates_with(&sy::declared_rates(&doc), &sess.rates),
            marking: sess.marking.clone(),
            doc,
        })
    } else {
        Ok(ModelInput::from_doc(doc_of(sess, name)?))
    }
}

fn text_ok(what: &str, v: &str) -> Result<()> {
    if v.chars().any(char::is_control) {
        return Err(Error::BadArgs(format!("{what} must be one line of text")));
    }
    Ok(())
}

// ------------------------------------------------------------------ ports

fn set_port(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "id").unwrap().to_string();
    let direction = sy::parse_direction(s(a, "direction").unwrap())?;
    let model = model_name(s(a, "model").unwrap_or("current"))?;
    let remove = b(a, "remove").unwrap_or(false);
    let decl = PortDecl {
        id: id.clone(),
        direction: direction.into(),
        kind: s(a, "kind").unwrap_or("").into(),
        unit: s(a, "unit").unwrap_or("").into(),
        description: s(a, "description").unwrap_or("").into(),
    };
    text_ok("kind", &decl.kind)?;
    text_ok("unit", &decl.unit)?;
    let doc = doc_of(sess, &model)?;
    let element = match doc.kind_of(&id) {
        Some(crate::doc::NodeKind::Place) => "place",
        Some(crate::doc::NodeKind::Transition) => "transition",
        None if remove => "",
        None => {
            return Err(Error::NotFound(format!(
                "model {model} has no place or transition {id:?}"
            )))
        }
    };
    let apply = |d: &mut Document| -> Result<bool> {
        if remove {
            Ok(sy::remove_port(d, &id, direction))
        } else {
            sy::set_port(d, decl.clone());
            Ok(true)
        }
    };
    let changed = if model == "current" {
        sess.edit(if remove { "remove port" } else { "set port" }, |sess| {
            let changed = apply(&mut sess.doc)?;
            if !changed {
                return Err(Error::NotFound(format!(
                    "model current declares no {direction} port {id:?}"
                )));
            }
            Ok(changed)
        })?
    } else {
        let d = sess.models.get_mut(&model).expect("resolved above");
        let changed = apply(d)?;
        if !changed {
            return Err(Error::NotFound(format!(
                "model {model} declares no {direction} port {id:?}"
            )));
        }
        changed
    };
    let mut out = json!({"model": model, "removed": remove && changed});
    if !remove {
        out["port"] = json!({
            "id": id, "element": element, "direction": direction,
            "kind": decl.kind, "unit": decl.unit, "description": decl.description,
        });
        let fit = matches!(
            (direction, element),
            (sy::OUTPUT, "place") | (sy::INPUT, "transition")
        );
        if !fit {
            out["warning"] = json!(format!(
                "an {direction} port on a {element} cannot be bound: outputs are places (a trajectory to read), inputs are transitions (a rate to drive)"
            ));
        }
    }
    Ok(out)
}

fn ports_of(name: &str, doc: &Document) -> Value {
    let mut v = json!({"model": name, "ports": sy::ports(doc)});
    let stale = sy::stale_ports(doc);
    if !stale.is_empty() {
        v["stale"] = json!(stale);
    }
    v
}

fn ports(sess: &mut Session, a: &Value) -> Result<Value> {
    if let Some(m) = s(a, "model") {
        let name = model_name(m)?;
        return Ok(ports_of(&name, &doc_of(sess, &name)?));
    }
    let mut out = vec![ports_of("current", &sess.doc)];
    for (name, doc) in &sess.models {
        if name != STATE_MODEL && !sy::declarations(doc).is_empty() {
            out.push(ports_of(name, doc));
        }
    }
    Ok(json!(out))
}

// --------------------------------------------------------------- bindings

/// A binding's check against the models as they are now.
fn check_now(sess: &Session, b: &Binding) -> BindingCheck {
    let from = doc_of(sess, &b.from.model);
    let to = doc_of(sess, &b.to.model);
    match (from, to) {
        (Ok(f), Ok(t)) => sy::check_binding(&f, &t, b),
        (Err(_), _) => BindingCheck::uncheckable(
            b,
            format!("model {} is not in the library (model.list); the binding cannot be checked against it", b.from.model),
        ),
        (_, Err(_)) => BindingCheck::uncheckable(
            b,
            format!("model {} is not in the library (model.list); the binding cannot be checked against it", b.to.model),
        ),
    }
}

fn binding_value(id: &str, bd: &Binding, check: &BindingCheck) -> Value {
    let mut v = json!({
        "id": id,
        "from": bd.from,
        "to": bd.to,
        "check": check,
    });
    if !bd.transform.is_empty() {
        v["transform"] = json!(bd.transform);
    }
    if let Some(c) = &bd.convert {
        v["convert"] = json!(c);
    }
    if bd.delay != 0.0 {
        v["delay"] = json!(bd.delay);
    }
    if check.verdict == sy::BIND_REFUSED {
        v["legacy"] = json!(format!(
            "bound before a model changed; system.bind and system.run both refuse it now: {}",
            check.reason
        ));
    }
    v
}

fn bind(sess: &mut Session, a: &Value) -> Result<Value> {
    let transform = match a.get("transform") {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(t)) => t.clone(),
        Some(Value::Number(x)) => sy::num(x.as_f64().unwrap_or(1.0)),
        Some(other) => {
            return Err(Error::BadArgs(format!(
                "transform must be a number or text, got {other}"
            )))
        }
    };
    let convert = match (
        s(a, "convert_from"),
        s(a, "convert_to"),
        n(a, "convert_factor"),
    ) {
        (None, None, None) => None,
        (Some(f), Some(t), Some(x)) => Some(UnitConversion {
            from: f.into(),
            to: t.into(),
            factor: x,
        }),
        _ => {
            return Err(Error::BadArgs(
                "pass convert_from, convert_to and convert_factor together, or none of them".into(),
            ))
        }
    };
    let bd = Binding {
        from: PortRef {
            model: model_name(s(a, "from_model").unwrap())?,
            port: s(a, "from_port").unwrap().into(),
        },
        to: PortRef {
            model: model_name(s(a, "to_model").unwrap())?,
            port: s(a, "to_port").unwrap().into(),
        },
        transform,
        convert,
        delay: n(a, "delay").unwrap_or(0.0),
    };
    for m in [&bd.from.model, &bd.to.model] {
        doc_of(sess, m)?;
    }
    let check = check_now(sess, &bd);
    if !check.is_ok() {
        return Err(check.to_error());
    }
    let id = sy::binding_id(&bd);
    let mut st = state(sess);
    let existed = st.bindings.insert(id.clone(), bd.clone()).is_some();
    save_state(sess, &st);
    let mut out = binding_value(&id, &bd, &check);
    out["existed"] = json!(existed);
    Ok(out)
}

fn get_binding(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "id").unwrap();
    let st = state(sess);
    let bd = st
        .bindings
        .get(id)
        .ok_or_else(|| Error::NotFound(format!("no binding {id:?} (system.bindings)")))?;
    let mut v = binding_value(id, bd, &check_now(sess, bd));
    v["systems"] = json!(st.systems_naming(id));
    Ok(v)
}

fn list_bindings(sess: &mut Session, a: &Value) -> Result<Value> {
    let filter = s(a, "model").map(model_name).transpose()?;
    let st = state(sess);
    Ok(json!(st
        .bindings
        .iter()
        .filter(|(_, bd)| filter
            .as_ref()
            .is_none_or(|m| &bd.from.model == m || &bd.to.model == m))
        .map(|(id, bd)| binding_value(id, bd, &check_now(sess, bd)))
        .collect::<Vec<_>>()))
}

fn unbind(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "id").unwrap();
    let mut st = state(sess);
    if !st.bindings.contains_key(id) {
        return Err(Error::NotFound(format!("no binding {id:?}")));
    }
    let naming = st.systems_naming(id);
    if !naming.is_empty() {
        return Err(Error::Invalid(format!(
            "binding {id} is part of system(s) {naming:?}; system.delete them first"
        )));
    }
    st.bindings.remove(id);
    save_state(sess, &st);
    Ok(json!({"deleted": id}))
}

// ---------------------------------------------------------------- systems

fn lookup(st: &SystemsState, ids: &[String]) -> Result<Vec<(String, Binding)>> {
    ids.iter()
        .map(|id| {
            st.bindings
                .get(id)
                .cloned()
                .map(|bd| (id.clone(), bd))
                .ok_or_else(|| Error::NotFound(format!("no binding {id:?} (system.bindings)")))
        })
        .collect()
}

fn create(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    if name.is_empty() || name.len() > sy::MAX_SYSTEM_NAME_BYTES {
        return Err(Error::BadArgs(format!(
            "a system name is 1..{} bytes",
            sy::MAX_SYSTEM_NAME_BYTES
        )));
    }
    text_ok("name", name)?;
    let mut st = state(sess);
    if st.systems.contains_key(name) && !b(a, "overwrite").unwrap_or(false) {
        return Err(Error::Invalid(format!(
            "system {name:?} exists (pass overwrite: true)"
        )));
    }
    let models: Vec<String> = strs(a, "models")
        .unwrap_or_default()
        .iter()
        .map(|m| model_name(m))
        .collect::<Result<_>>()?;
    let ids = strs(a, "bindings").unwrap_or_default();
    if models.is_empty() && ids.is_empty() {
        return Err(Error::BadArgs("a system needs models or bindings".into()));
    }
    let bindings = lookup(&st, &ids)?;
    let mut implied = vec![];
    let mut all = models.clone();
    for (_, bd) in &bindings {
        for m in [&bd.from.model, &bd.to.model] {
            if !all.contains(m) {
                all.push(m.clone());
                implied.push(m.clone());
            }
        }
    }
    let sys = System::new(name, all, ids);
    if sys.models.len() > sy::MAX_SYSTEM_MODELS || sys.bindings.len() > sy::MAX_SYSTEM_BINDINGS {
        return Err(Error::BadArgs(format!(
            "a system holds at most {} models and {} bindings",
            sy::MAX_SYSTEM_MODELS,
            sy::MAX_SYSTEM_BINDINGS
        )));
    }
    for m in &sys.models {
        doc_of(sess, m)?;
    }
    for (id, bd) in &bindings {
        let c = check_now(sess, bd);
        if !c.is_ok() {
            return Err(Error::Invalid(format!("binding {id}: {}", c.to_error())));
        }
    }
    if !bindings.is_empty() {
        sy::plan(bindings)?;
    }
    st.systems.insert(name.into(), sys.clone());
    save_state(sess, &st);
    let mut out = view(sess, &sys, &st, source("system.create", a))?;
    implied.sort();
    out["implied"] = json!(implied);
    Ok(out)
}

fn show(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    let st = state(sess);
    let sys = st
        .systems
        .get(name)
        .cloned()
        .ok_or_else(|| Error::NotFound(format!("no system {name:?} (system.list)")))?;
    view(sess, &sys, &st, source("system.show", a))
}

fn list_systems(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!(state(sess).systems.values().collect::<Vec<_>>()))
}

fn delete_system(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = s(a, "name").unwrap();
    let mut st = state(sess);
    st.systems
        .remove(name)
        .ok_or_else(|| Error::NotFound(format!("no system {name:?}")))?;
    save_state(sess, &st);
    Ok(json!({"deleted": name}))
}

/// sim_system {id}: the view computed now, as a Graph and a Report.
fn view(sess: &mut Session, sys: &System, st: &SystemsState, src: Step) -> Result<Value> {
    let docs: BTreeMap<String, Option<Document>> = sys
        .models
        .iter()
        .map(|m| (m.clone(), doc_of(sess, m).ok()))
        .collect();
    let bindings: Vec<(String, Option<Binding>)> = sys
        .bindings
        .iter()
        .map(|id| (id.clone(), st.bindings.get(id).cloned()))
        .collect();

    let mut binding_rows = vec![];
    let mut checks: BTreeMap<String, BindingCheck> = BTreeMap::new();
    for (id, bd) in &bindings {
        match bd {
            Some(bd) => {
                let c = check_now(sess, bd);
                binding_rows.push(binding_value(id, bd, &c));
                checks.insert(id.clone(), c);
            }
            None => binding_rows.push(json!({"id": id, "missing": true})),
        }
    }
    let present: Vec<(String, Binding)> = bindings
        .iter()
        .filter_map(|(id, bd)| bd.clone().map(|bd| (id.clone(), bd)))
        .collect();
    let pipeline = if present.is_empty() {
        json!({"runnable": false, "reason": "no bindings"})
    } else {
        match sy::plan(present.clone()) {
            Ok(p) => {
                let all_ok =
                    checks.values().all(BindingCheck::is_ok) && present.len() == bindings.len();
                let mut v = json!({
                    "runnable": all_ok,
                    "order": p.order,
                    "loops": p.loops,
                });
                if !p.loops.is_empty() {
                    v["maxHours"] = json!(sy::max_hours(&p));
                }
                if !all_ok {
                    v["reason"] = json!("a binding is missing or its check fails now");
                }
                v
            }
            Err(r) => json!({"runnable": false, "refusal": {"code": r.code, "reason": r.msg}}),
        }
    };

    // What binds to each port: the bindings using it, and the ports of the
    // other models of the system a binding without a conversion would join.
    let all_ports: Vec<(String, sy::Port)> = docs
        .iter()
        .filter_map(|(m, d)| d.as_ref().map(|d| (m, d)))
        .flat_map(|(m, d)| sy::ports(d).into_iter().map(move |p| (m.clone(), p)))
        .collect();
    let mut port_rows = vec![];
    for (m, p) in &all_ports {
        let bound: Vec<Value> = present
            .iter()
            .filter(|(_, bd)| {
                (p.direction == sy::OUTPUT && &bd.from.model == m && bd.from.port == p.id)
                    || (p.direction == sy::INPUT && &bd.to.model == m && bd.to.port == p.id)
            })
            .map(|(id, bd)| {
                let other = if p.direction == sy::OUTPUT {
                    &bd.to
                } else {
                    &bd.from
                };
                json!({"binding": id, "model": other.model, "port": other.port})
            })
            .collect();
        let bindable = |q: &sy::Port| {
            q.direction != p.direction
                && q.kind == p.kind
                && (q.unit.is_empty() || p.unit.is_empty() || q.unit == p.unit)
                && matches!(
                    (q.direction.as_str(), q.element.as_str()),
                    (sy::OUTPUT, "place") | (sy::INPUT, "transition")
                )
        };
        let candidates: Vec<Value> = all_ports
            .iter()
            .filter(|(qm, q)| qm != m && bindable(q))
            .map(|(qm, q)| json!({"model": qm, "port": q.id, "unit": q.unit}))
            .collect();
        port_rows.push(json!({
            "model": m,
            "port": p.id,
            "element": p.element,
            "direction": p.direction,
            "kind": p.kind,
            "unit": p.unit,
            "bound": bound,
            "candidates": candidates,
        }));
    }

    let nodes: Vec<GraphNode> = docs
        .iter()
        .map(|(m, d)| GraphNode {
            id: m.clone(),
            label: m.clone(),
            data: match d {
                Some(d) => json!({
                    "places": d.places.len(),
                    "transitions": d.transitions.len(),
                    "ports": sy::ports(d),
                    "title": d.extra.get("name"),
                }),
                None => json!({"missing": true}),
            },
        })
        .collect();
    let edges: Vec<GraphEdge> = present
        .iter()
        .map(|(id, bd)| {
            let c = &checks[id];
            GraphEdge {
                source: bd.from.model.clone(),
                target: bd.to.model.clone(),
                label: edge_label(bd),
                data: json!({
                    "binding": id,
                    "from_port": bd.from.port,
                    "to_port": bd.to.port,
                    "unit": c.unit,
                    "delay": bd.delay,
                    "transform": bd.transform,
                    "convert": bd.convert,
                    "verdict": c.verdict,
                }),
            }
        })
        .collect();
    let model_rows: Vec<Value> = docs
        .iter()
        .map(|(m, d)| json!({"model": m, "present": d.is_some()}))
        .collect();
    let shape = if present.is_empty() {
        "separate"
    } else {
        "pipeline"
    };
    let report = json!({
        "name": sys.name,
        "id": sys.id,
        "shape": shape,
        "models": model_rows,
        "bindings": binding_rows,
        "pipeline": pipeline,
        "ports": port_rows,
    });
    let title = format!("System {}", sys.name);
    let graph = sess.add_artifact(
        title.clone(),
        src.clone(),
        ArtifactData::Graph { nodes, edges },
    );
    let rep = sess.add_artifact(
        format!("{title} report"),
        src,
        ArtifactData::Report(report.clone()),
    );
    let mut out = report;
    out["graph"] = json!(graph);
    out["report"] = json!(rep);
    Ok(out)
}

fn edge_label(bd: &Binding) -> String {
    let mut l = format!("{}→{}", bd.from.port, bd.to.port);
    if bd.delay != 0.0 {
        l.push_str(&format!(" (+{} h)", sy::num(bd.delay)));
    }
    l
}

// -------------------------------------------------------------------- run

fn run_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = state(sess);
    let (label, ids) = match (s(a, "name"), strs(a, "bindings")) {
        (Some(name), None) => {
            let sys = st
                .systems
                .get(name)
                .ok_or_else(|| Error::NotFound(format!("no system {name:?} (system.list)")))?;
            (name.to_string(), sys.bindings.clone())
        }
        (None, Some(ids)) => ("bindings".to_string(), ids),
        _ => {
            return Err(Error::BadArgs(
                "give exactly one of name (a stored system) or bindings (binding ids)".into(),
            ))
        }
    };
    let bindings = lookup(&st, &ids)?;
    let hours = match (n(a, "hours"), n(a, "t_end")) {
        (Some(_), Some(_)) => {
            return Err(Error::BadArgs("give hours or t_end, not both".into()));
        }
        (Some(h), None) | (None, Some(h)) => h,
        (None, None) => sy::DEFAULT_HOURS,
    };
    let engine = Engine::parse(s(a, "engine").unwrap_or("ode"))?;
    let seed = match a.get("seed").filter(|v| !v.is_null()) {
        Some(v) => crate::stochastic::parse_seed(v, "seed")?,
        None => 1,
    };
    let count = |k: &str| -> Result<Option<usize>> {
        match n(a, k) {
            None => Ok(None),
            Some(x) if x >= 0.0 => Ok(Some(x as usize)),
            Some(x) => Err(Error::BadArgs(format!("{k} must be ≥ 0, got {x}"))),
        }
    };
    let opts = RunOptions {
        hours,
        engine,
        seed,
        realizations: count("realizations")?.unwrap_or(1),
        sweeps: count("sweeps")?,
        tolerance: n(a, "tolerance").unwrap_or(0.0),
    };
    let mut models = BTreeMap::new();
    for (_, bd) in &bindings {
        for m in [&bd.from.model, &bd.to.model] {
            if !models.contains_key(m) {
                models.insert(m.clone(), model_input(sess, m)?);
            }
        }
    }
    let res = sy::run_pipeline(bindings.clone(), &models, &opts)?;

    let grid: Vec<f64> = (0..=sy::SEGMENT_BUDGET)
        .map(|k| sy::grid_at(opts.hours, k, sy::SEGMENT_BUDGET))
        .collect();
    let mut finals = Map::new();
    let mut series = vec![];
    for (m, r) in &res.steps {
        finals.insert(m.clone(), Value::Object(r.finals()));
        for (name, v) in r.names.iter().zip(&r.values) {
            let mut s = Series::new(name.clone(), crate::sweep::resample(&r.t, v, &grid));
            s.group = Some(m.clone());
            series.push(s);
        }
    }
    let flows: Vec<Value> = bindings
        .iter()
        .map(|(id, bd)| {
            let segs = res.schedules.get(id);
            let total = segs.map(|s| sy::delivered(s, opts.hours));
            json!({
                "binding": id,
                "from": format!("{}.{}", bd.from.model, bd.from.port),
                "to": format!("{}.{}", bd.to.model, bd.to.port),
                "transform": bd.transform,
                "convert": bd.convert,
                "delay": bd.delay,
                "delivered": total,
                "meanRate": total.map(|t| t / opts.hours),
                "inTransit": res.in_transit.get(id),
                "schedule": segs.map(|s| s.iter().map(|g| json!([g.until, g.value])).collect::<Vec<_>>()),
            })
        })
        .collect();
    let mut report = json!({
        "system": label,
        "engine": engine.name(),
        "hours": opts.hours,
        "order": res.order,
        "verdict": res.verdict(),
        "flows": flows,
        "final": finals,
        "assumptions": res.assumptions,
        "inTransit": res.in_transit,
    });
    if engine == Engine::Ssa {
        report["seed"] = json!(seed);
        report["realizations"] = json!(opts.realizations.max(1));
    }
    if let Some(l) = &res.loop_reading {
        report["loop"] = l.clone();
        report["sweeps"] = l["sweepsRun"].clone();
    }
    let title = format!("Pipeline {label}");
    let src = source("system.run", a);
    let mut out = report.clone();
    if !series.is_empty() {
        let ts = sess.add_artifact(
            title.clone(),
            src.clone(),
            ArtifactData::TimeSeries {
                x_label: "t (h)".into(),
                x: grid,
                series,
            },
        );
        out["timeseries"] = json!(ts);
    }
    out["report"] = json!(sess.add_artifact(
        format!("{title} report"),
        src.clone(),
        ArtifactData::Report(report),
    ));
    if b(a, "graph").unwrap_or(false) {
        let nodes = res
            .order
            .iter()
            .map(|m| GraphNode {
                id: m.clone(),
                label: m.clone(),
                data: json!({"final": finals.get(m), "ran": finals.contains_key(m)}),
            })
            .collect();
        let edges = bindings
            .iter()
            .map(|(id, bd)| {
                let total = res.schedules.get(id).map(|s| sy::delivered(s, opts.hours));
                GraphEdge {
                    source: bd.from.model.clone(),
                    target: bd.to.model.clone(),
                    label: match total {
                        Some(t) => format!("{} ({})", edge_label(bd), crate::sweep::fmt_sig(t, 4)),
                        None => edge_label(bd),
                    },
                    data: json!({"binding": id, "delivered": total, "inTransit": res.in_transit.get(id), "delay": bd.delay}),
                }
            })
            .collect();
        out["graph"] = json!(sess.add_artifact(
            format!("{title} graph"),
            src,
            ArtifactData::Graph { nodes, edges },
        ));
    }
    Ok(out)
}

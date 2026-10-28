//! The open composition of pattern instances (`bundle.*`), for the
//! composition views: instances built by pflow-compose's constructors,
//! links validated by pflow-compose's `Bundle::validate` before they are
//! added, and the queries the views draw from (verdicts, drag candidates,
//! the flat net, scenario runs, the lens test, the ontology facts). The
//! logic lives in [`crate::composition`] and [`crate::lens`].
//!
//! Edits are undoable (`Session::edit`) and journaled; queries are not
//! journaled. Queries that produce data store an artifact.

use pflow_compose::Link;
use serde_json::{json, Value};

use crate::artifact::ArtifactData;
use crate::composition::{self as comp, Composition};
use crate::lens;
use crate::registry::{always, arg, b, opt, req, s, strs, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::{Error, Result};

fn need(sess: &Session) -> std::result::Result<(), String> {
    if sess.composition.is_some() {
        Ok(())
    } else {
        Err("no composition is open (bundle.open)".into())
    }
}

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "bundle.instances",
        label: "Composition Instances",
        help: "The instance catalog: six pattern instances (orders, workers, payment; extras payment2, \
               orders2, workers2), each with its pattern, constructor parameters (judgment), why, and \
               whether pflow-compose can build it at this pin (the saga cannot yet). Also the default \
               instances and links.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: instances,
        journal: false,
    },
    CommandSpec {
        id: "bundle.scenarios",
        label: "Composition Scenarios",
        help: "The firing scripts (members \"<subnet>/<transition>\") with their expected predicates, \
               judgment frozen from the prototype: commit, refund, charge-fails, second-order.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: scenarios,
        journal: false,
    },
    CommandSpec {
        id: "bundle.ontology",
        label: "Composition Ontology",
        help: "The ontology facts the views draw (patterns, ports, link kinds, idioms, caveats, IRIs) \
               with sources and bibliography: ontology-pflow-xyz ns/patterns.ttl@bc4a996, via the \
               prototype's ontology.js (see its `provenance`).",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: ontology,
        journal: false,
    },
    CommandSpec {
        id: "bundle.open",
        label: "Open Composition",
        help: "Open a composition, replacing any open one: `instances` from the catalog (default \
               orders, workers, payment) and `links` (default: the three idioms). An instance \
               pflow-compose cannot build is listed under `unavailable`, and a link to it under \
               `dropped_links`; every other link is validated as bundle.add_link does. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[
            opt("instances", StrList, "catalog instance ids (bundle.instances)"),
            opt("links", Json, "[{kind, from, to}], ends {subnet, port} or \"subnet:port\""),
        ],
        enabled: always,
        run: open,
        journal: true,
    },
    CommandSpec {
        id: "bundle.close",
        label: "Close Composition",
        help: "Close the open composition. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[],
        enabled: need,
        run: close,
        journal: true,
    },
    CommandSpec {
        id: "bundle.state",
        label: "Composition State",
        help: "The open composition: instances, subnets (ports with their ontology keys, places, \
               transitions, arcs, constraints), links (index, key, idiom, leak), unavailable \
               instances and dropped links.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: need,
        run: state,
        journal: false,
    },
    CommandSpec {
        id: "bundle.add_instance",
        label: "Add Instance…",
        help: "Add a catalog instance to the composition (e.g. an extra: payment2, orders2, workers2). \
               One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[arg("id", Str, "catalog instance id")],
        enabled: need,
        run: add_instance,
        journal: true,
    },
    CommandSpec {
        id: "bundle.remove_instance",
        label: "Remove Instance…",
        help: "Remove an instance and every link touching it. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[arg("id", Str, "instance id")],
        enabled: need,
        run: remove_instance,
        journal: true,
    },
    CommandSpec {
        id: "bundle.add_link",
        label: "Add Link…",
        help: "Validate a link (pflow-compose's Bundle::validate, the go-pflow validateLink port, and \
               the page rules PAGE_SAME_SUBNET, PAGE_DUPLICATE), then add it; refused with each error \
               as \"[CODE] message\". Returns the link's index, idiom and the rule card. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[
            arg("kind", Str, "token, data, event or guard"),
            arg("from", Json, "{subnet, port} or \"subnet:port\""),
            arg("to", Json, "{subnet, port} or \"subnet:port\""),
        ],
        enabled: need,
        run: add_link,
        journal: true,
    },
    CommandSpec {
        id: "bundle.remove_link",
        label: "Remove Link…",
        help: "Remove a link by `index`, or by `kind`, `from` and `to` (an event link either way \
               round). One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[
            opt("index", Int, "the link's index (bundle.state)"),
            opt("kind", Str, "token, data, event or guard"),
            opt("from", Json, "{subnet, port} or \"subnet:port\""),
            opt("to", Json, "{subnet, port} or \"subnet:port\""),
        ],
        enabled: need,
        run: remove_link,
        journal: true,
    },
    CommandSpec {
        id: "bundle.reset_links",
        label: "Reset Links",
        help: "Replace the links with the three idioms (those whose ends are in the composition). \
               One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[],
        enabled: need,
        run: reset_links,
        journal: true,
    },
    CommandSpec {
        id: "bundle.clear_links",
        label: "Clear Links",
        help: "Remove every link. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[],
        enabled: need,
        run: clear_links,
        journal: true,
    },
    CommandSpec {
        id: "bundle.validate",
        label: "Validate Composition",
        help: "pflow-compose's Bundle::validate on the open composition: valid, errors and warnings \
               ({code, message, element}). Stores a report.",
        menu: "Compose",
        shortcut: None,
        args: &[],
        enabled: need,
        run: validate,
        journal: false,
    },
    CommandSpec {
        id: "bundle.candidates",
        label: "Link Candidates",
        help: "Every other port of the composition judged as the far end of a link dragged from \
               `from` (the prototype's rules.candidates): kinds tried in order (transition→transition: \
               event; transition→place: guard; place→transition: guard oriented transition→place; \
               place→place: token, then data), the first legal one wins; `prefer` restricts to one kind. \
               Each verdict is pflow-compose's, plus the page rules (rule: \"page\").",
        menu: "",
        shortcut: None,
        args: &[
            arg("from", Json, "{subnet, port} or \"subnet:port\""),
            opt("prefer", Str, "\"auto\" (default) or token, data, event, guard"),
        ],
        enabled: need,
        run: candidates,
        journal: false,
    },
    CommandSpec {
        id: "bundle.verdicts",
        label: "All Link Verdicts",
        help: "pflow-compose's verdict on every ordered port pair on two different subnets × every \
               link kind (the shape of the prototype's go-pflow receipt rows): over the open \
               composition's instances, or over `instances` from the catalog. Stores a table.",
        menu: "Compose",
        shortcut: None,
        args: &[opt("instances", StrList, "catalog instance ids (default: the composition's)")],
        enabled: always,
        run: verdicts,
        journal: false,
    },
    CommandSpec {
        id: "bundle.flatten",
        label: "Flatten Composition",
        help: "pflow-compose's Bundle::flatten_with_map: the flat net (places, transitions with their \
               members, arcs, constraints), wires, fused groups, the place and transition maps, \
               warnings and each transition's initial enablement (pflow-metamodel's reason), or \
               {ok: false, error}. Stores a report.",
        menu: "Compose",
        shortcut: None,
        args: &[],
        enabled: need,
        run: flatten,
        journal: false,
    },
    CommandSpec {
        id: "bundle.incidence",
        label: "Composition Incidence",
        help: "The flat net's incidence matrix C = post − pre (rows: flat places, cols: flat transitions), each \
               subnet's own block, and each declared constraint read as a P-invariant y (1 on each place it \
               names) with holds = (y·C = 0 and y·M₀ = n), or null when it is not a plain sum of tokens(…) == n.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: need,
        run: incidence,
        journal: false,
    },
    CommandSpec {
        id: "bundle.run",
        label: "Run Scenario…",
        help: "Fire a scenario (bundle.scenarios) or a `script` of members \"<subnet>/<transition>\" on \
               the flat net from its initial marking, through pflow-metamodel's enabled/fire; stops at \
               the first member that is not in the net or cannot fire, then evaluates the predicates. \
               Stores a report.",
        menu: "Compose",
        shortcut: None,
        args: &[
            opt("scenario", Str, "scenario id"),
            opt("script", StrList, "members to fire, instead of a scenario"),
        ],
        enabled: need,
        run: run_cmd,
        journal: false,
    },
    CommandSpec {
        id: "bundle.boundaries",
        label: "Event Classes as Boundaries",
        help: "The composition's event classes (Flatten's fused groups) read as nets with boundaries: \
               each class is one port of the shared boundary and one composite transition; with the \
               composite's enablement at the flat initial marking (pflow-metamodel's enabled_why_not) \
               and each member's own, asked on a one-transition model of its arcs (and any guard link \
               gating it) over the flat places.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: need,
        run: boundaries,
        journal: false,
    },
    CommandSpec {
        id: "bundle.game",
        label: "Token Game",
        help: "The token game on the flat net: the marking (flat place -> tokens), the transitions fired \
               since the last reset, each flat transition's enablement at it (pflow-metamodel's \
               enabled_why_not, with the refusal), the selected scenario's progress (steps taken, and \
               once done the predicates at the marking reached) and each declared constraint \
               evaluated at the marking (holding at one marking proves nothing). Starts at the \
               initial marking; any edit that changes the flat net starts it afresh.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: need,
        run: game,
        journal: false,
    },
    CommandSpec {
        id: "bundle.fire",
        label: "Fire Flat Transition…",
        help: "Fire a flat transition (its flat id, or a member \"<subnet>/<transition>\") in the token \
               game, through pflow-metamodel's enabled_why_not / fire; refused with the reason when it \
               is not enabled. Leaves any selected scenario. One undo step; returns bundle.game.",
        menu: "Compose",
        shortcut: None,
        args: &[arg("transition", Str, "flat transition id, or a member \"<subnet>/<transition>\"")],
        enabled: need,
        run: fire,
        journal: true,
    },
    CommandSpec {
        id: "bundle.reset_marking",
        label: "Reset Token Game",
        help: "Back to the flat net's initial marking: nothing fired, no scenario selected. One undo \
               step; returns bundle.game.",
        menu: "Compose",
        shortcut: None,
        args: &[],
        enabled: need,
        run: reset_marking,
        journal: true,
    },
    CommandSpec {
        id: "bundle.scenario",
        label: "Select Scenario…",
        help: "Select a scenario (bundle.scenarios) for the token game: back to the initial marking with \
               none of its script fired. bundle.step then fires it a step at a time. One undo step; \
               returns bundle.game.",
        menu: "Compose",
        shortcut: None,
        args: &[arg("id", Str, "scenario id")],
        enabled: need,
        run: select_scenario,
        journal: true,
    },
    CommandSpec {
        id: "bundle.step",
        label: "Step Scenario",
        help: "Take the selected scenario's next step (or, with all: true, the rest of them): its member \
               is resolved to the flat transition that contains it and fired when enabled; the first one \
               that is not enabled stops the script. When the script is done its predicates are \
               evaluated (judgment, frozen; the firing is pflow-metamodel's). One undo step; returns \
               bundle.game.",
        menu: "Compose",
        shortcut: None,
        args: &[opt("all", Bool, "run to the end (default: one step)")],
        enabled: need,
        run: step,
        journal: true,
    },
    CommandSpec {
        id: "bundle.lens",
        label: "Lens Test",
        help: "The lens test on the open composition (the prototype's lensfind.js): backward pairs, \
               the forward pass of the commit scenario, each step and whether it composes as a lens, \
               the scenario runs, whether the strict test holds, and (unless fixes: false) each \
               one-link repair re-validated, flattened and analysed. Stores a report.",
        menu: "Compose",
        shortcut: None,
        args: &[opt("fixes", Bool, "also try the one-link repairs (default true)")],
        enabled: need,
        run: lens_cmd,
        journal: false,
    },
];

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

fn comp_ref(sess: &Session) -> Result<&Composition> {
    sess.composition
        .as_ref()
        .ok_or_else(|| Error::Invalid("no composition is open (bundle.open)".into()))
}

fn bad(e: String) -> Error {
    Error::BadArgs(e)
}

// ------------------------------------------------------------ catalog

fn instances(_: &mut Session, _: &Value) -> Result<Value> {
    let list: Vec<Value> = comp::catalog()
        .iter()
        .map(|s| {
            let built = comp::build(s);
            let mut v = json!(s);
            v["available"] = json!(built.is_ok());
            v["reason"] = built.err().map_or(Value::Null, Value::from);
            v
        })
        .collect();
    Ok(json!({
        "instances": list,
        "default": comp::DEFAULT_INSTANCES,
        "default_links": comp::default_links().iter().map(comp::link_json).collect::<Vec<_>>(),
        "params_note": comp::PARAMS_NOTE,
    }))
}

fn scenarios(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({ "scenarios": comp::scenarios() }))
}

fn ontology(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(comp::ontology().clone())
}

// ------------------------------------------------------------ edits

fn parse_links(v: Option<&Value>) -> Result<Option<Vec<Link>>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Array(a)) => a
            .iter()
            .map(|l| comp::parse_link(l).map_err(bad))
            .collect::<Result<Vec<_>>>()
            .map(Some),
        Some(other) => Err(bad(format!("links must be an array, got {other}"))),
    }
}

fn open(sess: &mut Session, a: &Value) -> Result<Value> {
    let ids = strs(a, "instances").unwrap_or_else(|| {
        comp::DEFAULT_INSTANCES
            .iter()
            .map(|s| s.to_string())
            .collect()
    });
    let links = parse_links(a.get("links"))?.unwrap_or_else(comp::default_links);
    let c = Composition::open(&ids, &links).map_err(Error::Invalid)?;
    let out = c.to_json();
    sess.edit("open composition", |s| {
        s.composition = Some(c);
        Ok(())
    })?;
    Ok(out)
}

fn close(sess: &mut Session, _: &Value) -> Result<Value> {
    sess.edit("close composition", |s| {
        s.composition = None;
        Ok(json!({ "closed": true }))
    })
}

fn state(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(comp_ref(sess)?.to_json())
}

/// Applies `f` to the open composition as one undoable edit.
fn edit_comp<T>(
    sess: &mut Session,
    label: &str,
    f: impl FnOnce(&mut Composition) -> Result<T>,
) -> Result<T> {
    sess.edit(label, |s| {
        let c = s
            .composition
            .as_mut()
            .ok_or_else(|| Error::Invalid("no composition is open (bundle.open)".into()))?;
        f(c)
    })
}

fn add_instance(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id").to_string();
    edit_comp(sess, &format!("add instance {id}"), |c| {
        c.add_instance(&id).map_err(Error::Invalid)?;
        let spec = c.spec(&id).expect("just added").clone();
        let sub = c.subnet(&id).expect("just added");
        Ok(json!({ "added": id, "subnet": comp::subnet_json(&spec, sub) }))
    })
}

fn remove_instance(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id").to_string();
    edit_comp(sess, &format!("remove instance {id}"), |c| {
        let gone = c.remove_instance(&id).map_err(Error::Invalid)?;
        Ok(json!({
            "removed": id,
            "links_removed": gone.iter().map(comp::link_json).collect::<Vec<_>>(),
        }))
    })
}

fn link_arg(a: &Value) -> Result<Link> {
    let kind = comp::parse_kind(req(a, "kind")).map_err(bad)?;
    let from = comp::parse_endpoint(&a["from"]).map_err(bad)?;
    let to = comp::parse_endpoint(&a["to"]).map_err(bad)?;
    Ok(comp::link(kind, from, to))
}

fn add_link(sess: &mut Session, a: &Value) -> Result<Value> {
    let l = link_arg(a)?;
    edit_comp(sess, &format!("add link {}", comp::link_key(&l)), |c| {
        let v = c
            .add_link(l.clone())
            .map_err(|v| Error::Invalid(v.refusal()))?;
        let idiom = c.idiom_of(&l);
        Ok(json!({
            "index": c.bundle.links.len() - 1,
            "link": comp::link_json(&l),
            "key": comp::link_key(&l),
            "idiom": idiom.map_or(Value::Null, |x| x["key"].clone()),
            "leak": idiom.is_some_and(|x| x["leak"].as_bool() == Some(true)),
            "rule": comp::rule_for(&l, &v),
            "verdict": v,
        }))
    })
}

fn remove_link(sess: &mut Session, a: &Value) -> Result<Value> {
    let index = a.get("index").and_then(Value::as_f64).map(|x| x as usize);
    let by_link = if a.get("kind").is_some_and(|v| !v.is_null()) {
        Some(link_arg(a)?)
    } else {
        None
    };
    edit_comp(sess, "remove link", |c| {
        let i = match (index, &by_link) {
            (Some(i), None) if i < c.bundle.links.len() => i,
            (Some(i), None) => {
                return Err(Error::BadArgs(format!(
                    "no link {i}; the composition has {}",
                    c.bundle.links.len()
                )))
            }
            (None, Some(l)) => {
                let key = comp::link_key(l);
                let rev = comp::link_key(&comp::link(l.kind, l.to.clone(), l.from.clone()));
                c.bundle
                    .links
                    .iter()
                    .position(|x| {
                        comp::link_key(x) == key
                            || (l.kind == pflow_compose::LinkKind::Event
                                && comp::link_key(x) == rev)
                    })
                    .ok_or_else(|| Error::NotFound(format!("no link {key}")))?
            }
            _ => {
                return Err(Error::BadArgs(
                    "pass `index`, or `kind`, `from` and `to`".into(),
                ))
            }
        };
        let gone = c.bundle.links.remove(i);
        Ok(json!({ "removed": comp::link_json(&gone), "index": i }))
    })
}

fn reset_links(sess: &mut Session, _: &Value) -> Result<Value> {
    edit_comp(sess, "reset links", |c| {
        c.bundle.links.clear();
        let mut dropped = vec![];
        for l in comp::default_links() {
            if c.subnet(&l.from.subnet).is_none() || c.subnet(&l.to.subnet).is_none() {
                dropped.push(json!({"link": comp::link_json(&l), "reason": "an end is not in the composition"}));
                continue;
            }
            c.add_link(l).map_err(|v| Error::Invalid(v.refusal()))?;
        }
        Ok(json!({ "links": c.links_json(), "dropped_links": dropped }))
    })
}

fn clear_links(sess: &mut Session, _: &Value) -> Result<Value> {
    edit_comp(sess, "clear links", |c| {
        let n = c.bundle.links.len();
        c.bundle.links.clear();
        Ok(json!({ "removed": n }))
    })
}

// ------------------------------------------------------------ queries

fn validate(sess: &mut Session, a: &Value) -> Result<Value> {
    let res = comp_ref(sess)?.bundle.validate();
    let issues = |v: &[pflow_compose::ValidationError]| -> Vec<Value> {
        v.iter()
            .map(|e| json!({"code": e.code, "message": e.message, "element": e.element}))
            .collect()
    };
    let report = json!({
        "valid": res.valid,
        "errors": issues(&res.errors),
        "warnings": issues(&res.warnings),
    });
    let id = sess.add_artifact(
        "Composition validation".into(),
        source("bundle.validate", a),
        ArtifactData::Report(report.clone()),
    );
    let mut out = report;
    out["artifact"] = json!(id);
    Ok(out)
}

fn candidates(sess: &mut Session, a: &Value) -> Result<Value> {
    let from = comp::parse_endpoint(&a["from"]).map_err(bad)?;
    let prefer = s(a, "prefer").unwrap_or("auto");
    let list = comp::candidates(comp_ref(sess)?, &from, prefer).map_err(bad)?;
    let lit = list.iter().filter(|c| c["ok"] == json!(true)).count();
    Ok(json!({
        "from": {"subnet": from.subnet, "port": from.port},
        "prefer": prefer,
        "lit": lit,
        "candidates": list,
    }))
}

fn verdicts(sess: &mut Session, a: &Value) -> Result<Value> {
    let mut built: Vec<pflow_compose::Subnet> = vec![];
    let mut unavailable = vec![];
    match strs(a, "instances") {
        Some(ids) => {
            for id in ids {
                let spec = comp::instance(&id)
                    .ok_or_else(|| Error::BadArgs(format!("no instance {id:?}")))?;
                match comp::build(spec) {
                    Ok(s) => built.push(s),
                    Err(e) => unavailable.push(json!({"instance": id, "reason": e})),
                }
            }
        }
        None => {
            let c = comp_ref(sess)?;
            built = c.bundle.subnets.clone();
        }
    }
    let refs: Vec<&pflow_compose::Subnet> = built.iter().collect();
    let rows = comp::all_verdicts(&refs);
    let mut legal = serde_json::Map::new();
    for k in comp::KINDS {
        let name = comp::kind_name(k);
        legal.insert(
            name.into(),
            json!(rows.iter().filter(|r| r.ok && r.kind == name).count()),
        );
    }
    let table = ArtifactData::Table {
        columns: ["kind", "from", "to", "ok", "codes", "warnings", "messages"]
            .map(String::from)
            .to_vec(),
        rows: rows
            .iter()
            .map(|r| {
                vec![
                    json!(r.kind),
                    json!(r.from),
                    json!(r.to),
                    json!(r.ok),
                    json!(r.codes.join(" ")),
                    json!(r.warnings.join(" ")),
                    json!(r.messages.join(" | ")),
                ]
            })
            .collect(),
    };
    let id = sess.add_artifact("Link verdicts".into(), source("bundle.verdicts", a), table);
    Ok(json!({
        "instances": built.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        "unavailable": unavailable,
        "total": rows.len(),
        "legal": legal,
        "rows": rows,
        "artifact": id,
    }))
}

fn flatten(sess: &mut Session, a: &Value) -> Result<Value> {
    let mut out = comp::flatten_json(&comp_ref(sess)?.bundle);
    let id = sess.add_artifact(
        "Flat composition".into(),
        source("bundle.flatten", a),
        ArtifactData::Report(out.clone()),
    );
    out["artifact"] = json!(id);
    Ok(out)
}

fn incidence(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(comp::incidence_json(&comp_ref(sess)?.bundle))
}

fn run_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let c = comp_ref(sess)?;
    let flat = comp::flatten(&c.bundle).map_err(Error::Invalid)?;
    let r = match (s(a, "scenario"), strs(a, "script")) {
        (Some(id), None) => {
            let sc = comp::scenario(id).ok_or_else(|| {
                let ids: Vec<&str> = comp::scenarios().iter().map(|s| s.id).collect();
                Error::BadArgs(format!("no scenario {id:?}; there are {ids:?}"))
            })?;
            comp::run_scenario(&flat, sc)
        }
        (None, Some(script)) => comp::run(&flat, "script", "script", &script, &[]),
        _ => return Err(bad("pass `scenario` or `script`".into())),
    };
    let mut out = json!(r);
    let id = sess.add_artifact(
        format!("Run: {}", r.title),
        source("bundle.run", a),
        ArtifactData::Report(out.clone()),
    );
    out["artifact"] = json!(id);
    Ok(out)
}

// ------------------------------------------------------------ the token game

fn flat_of(c: &Composition) -> Result<comp::Flat> {
    comp::flatten(&c.bundle).map_err(Error::Invalid)
}

fn boundaries(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(comp::boundaries::boundaries_json(comp_ref(sess)?))
}

fn game(sess: &mut Session, _: &Value) -> Result<Value> {
    let c = comp_ref(sess)?;
    Ok(comp::game::game_json(&c.bundle, &c.game))
}

fn fire(sess: &mut Session, a: &Value) -> Result<Value> {
    let t = req(a, "transition").to_string();
    edit_comp(sess, &format!("fire {t}"), |c| {
        let flat = flat_of(c)?;
        // A member "subnet/transition" names the flat transition it is in.
        let id = if flat.model.transitions.iter().any(|x| x.id == t) {
            t.clone()
        } else {
            flat.transition_of(&t)
                .map(str::to_string)
                .unwrap_or(t.clone())
        };
        let mut g = c.game.of(&flat);
        g.fire(&flat, &id).map_err(Error::Invalid)?;
        c.game = g;
        Ok(comp::game::game_json(&c.bundle, &c.game))
    })
}

fn reset_marking(sess: &mut Session, _: &Value) -> Result<Value> {
    edit_comp(sess, "reset token game", |c| {
        let flat = flat_of(c)?;
        c.game = c.game.reset(&flat);
        Ok(comp::game::game_json(&c.bundle, &c.game))
    })
}

fn select_scenario(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "id").to_string();
    edit_comp(sess, &format!("scenario {id}"), |c| {
        let flat = flat_of(c)?;
        c.game = c.game.select(&flat, &id).map_err(Error::BadArgs)?;
        Ok(comp::game::game_json(&c.bundle, &c.game))
    })
}

fn step(sess: &mut Session, a: &Value) -> Result<Value> {
    let all = b(a, "all").unwrap_or(false);
    edit_comp(sess, "step scenario", |c| {
        let flat = flat_of(c)?;
        let mut g = c.game.of(&flat);
        loop {
            let more = g.step(&flat).map_err(Error::Invalid)?;
            if !all || !more {
                break;
            }
        }
        c.game = g;
        Ok(comp::game::game_json(&c.bundle, &c.game))
    })
}

fn lens_cmd(sess: &mut Session, a: &Value) -> Result<Value> {
    let mut out = lens::report(comp_ref(sess)?, b(a, "fixes").unwrap_or(true));
    let id = sess.add_artifact(
        "Lens test".into(),
        source("bundle.lens", a),
        ArtifactData::Report(out.clone()),
    );
    out["artifact"] = json!(id);
    Ok(out)
}

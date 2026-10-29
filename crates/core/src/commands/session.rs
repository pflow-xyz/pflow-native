//! `session.*`: the state that outlives one document, saved and restored
//! whole. It is for a front end with no data directory to keep it in:
//! pflow.xyz's page saves `session.dump_state` in browser storage after
//! each command that changes it and restores it at startup, so the
//! library, systems, macros, notes, results and store survive a reload.
//! (The native app autosaves the library and systems itself, and its store
//! is files on disk.)
//!
//! The document, marking, rates, selection, view and undo history are not
//! part of it: the page owns the document, and history ends with the page.
//! What the composition views show is: the open composition (its document,
//! either form, with the content attached to it), the open search plan, and
//! which view, example and mode is up, so the page and the app come back to
//! the same drawing.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;

use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

use crate::artifact::Artifact;
use crate::doc::Document;
use crate::registry::{always, arg, opt, strs, ArgType::*, CommandSpec};
use crate::session::{Note, Session, Step, MAX_ARTIFACTS};
use crate::store;
use crate::systems::SystemsState;
use crate::{Error, Result};

/// The `format` of a saved state.
pub const FORMAT: &str = "pflow-native-session-state";
/// The `version` this build writes, and the newest it reads.
pub const VERSION: u64 = 1;
/// The parts of a state, in the order they are written.
pub const PARTS: &[&str] = &[
    "models",
    "systems",
    "macros",
    "recording",
    "notes",
    "artifacts",
    "store",
    "composition",
    "plan",
    "composition_view",
];
/// Header keys beside the parts.
const HEADER: &[&str] = &["format", "version", "pflowNative"];

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "session.dump_state",
        label: "Dump Session State",
        help: "The state that outlives one document, as one JSON object for a front end to save (pflow.xyz's page keeps it \
               in browser storage): models (the library), systems, macros, recording (the steps of a recording in progress, \
               or null), notes, artifacts (results), store (every file of the local store, as text), composition (the open \
               composition as its document, either form, with its content when that is not the library's, beside the \
               instances and links of the older shape; or null when none is open), plan (the open search plan, or null) and \
               composition_view (which composition view, example and mode is shown); `parts` picks some. \
               Not the document, marking, rates, selection or undo history.",
        menu: "",
        shortcut: None,
        args: &[opt(
            "parts",
            StrList,
            "models, systems, macros, recording, notes, artifacts, store, composition, plan, composition_view (default all)",
        )],
        enabled: always,
        run: dump,
        journal: false,
    },
    CommandSpec {
        id: "session.restore_state",
        label: "Restore Session State",
        help: "Restore what session.dump_state saved: each part present replaces that part, parts absent are kept. All or \
               nothing: every part is checked before anything changes, the store's files against their content ids. The \
               store part is refused unless the store is in memory (the browser's), so a saved state never overwrites a \
               store on disk. Two things are let through rather than refuse the whole state, and the result lists them: \
               a result this build cannot read back is dropped (a number that was not finite is saved as null; its \
               command recomputes it), and a macro step naming a command this build lacks is kept as saved (macro.play \
               refuses it). The document, marking, rates, selection and undo history are untouched.",
        menu: "",
        shortcut: None,
        args: &[arg(
            "state",
            Json,
            "the state as session.dump_state returns it (object or string)",
        )],
        enabled: always,
        run: restore,
        journal: false,
    },
];

fn bad(part: &str, msg: impl Display) -> Error {
    Error::BadArgs(format!("state.{part}: {msg}"))
}

fn unknown_part(p: &str) -> Error {
    Error::BadArgs(format!(
        "unknown part {p:?}; a session state has {}",
        PARTS.join(", ")
    ))
}

fn dump(sess: &mut Session, a: &Value) -> Result<Value> {
    let parts = match strs(a, "parts") {
        Some(want) => {
            if let Some(p) = want.iter().find(|p| !PARTS.contains(&p.as_str())) {
                return Err(unknown_part(p));
            }
            want
        }
        None => PARTS.iter().map(|p| p.to_string()).collect(),
    };
    let mut out = json!({
        "format": FORMAT,
        "version": VERSION,
        "pflowNative": env!("CARGO_PKG_VERSION"),
    });
    for part in PARTS.iter().filter(|p| parts.iter().any(|w| w == *p)) {
        out[*part] = dump_part(sess, part)?;
    }
    Ok(out)
}

fn dump_part(sess: &Session, part: &str) -> Result<Value> {
    let (next_artifact, next_note) = sess.id_counters();
    Ok(match part {
        "models" => Value::Object(
            sess.models
                .iter()
                .map(|(name, d)| (name.clone(), d.to_value()))
                .collect(),
        ),
        "systems" => serde_json::to_value(&sess.systems).expect("serializable"),
        "macros" => json!(sess.macros),
        "recording" => match sess.recording {
            Some(start) => json!(sess.journal[start.min(sess.journal.len())..]),
            None => Value::Null,
        },
        "notes" => json!({ "items": sess.notes, "next": next_note }),
        "artifacts" => json!({ "items": sess.artifacts, "next": next_artifact }),
        "store" => {
            let st = store::open();
            json!({ "location": st.location(), "blobs": st.dump_blobs()? })
        }
        "composition" => match &sess.composition {
            Some(c) => crate::composition::document::dump_part(c)?,
            None => Value::Null,
        },
        "plan" => match &sess.search_plan {
            Some(p) => p.plan.clone(),
            None => Value::Null,
        },
        "composition_view" => serde_json::to_value(&sess.composition_view).expect("serializable"),
        _ => unreachable!("checked against PARTS"),
    })
}

/// A checked state: each field is a part that was present.
#[derive(Default)]
struct Parsed {
    models: Option<BTreeMap<String, Document>>,
    systems: Option<SystemsState>,
    macros: Option<BTreeMap<String, Vec<Step>>>,
    /// `Some(None)`: restored as not recording.
    recording: Option<Option<Vec<Step>>>,
    notes: Option<(Vec<Note>, u64)>,
    /// The results that read back, their counter, and the ids of those
    /// that did not.
    artifacts: Option<(Vec<Artifact>, u64, Vec<String>)>,
    store: Option<BTreeMap<String, String>>,
    /// `Some(None)`: restored as no composition open.
    composition: Option<Option<crate::composition::Composition>>,
    /// `Some(None)`: restored as no plan open.
    plan: Option<Option<Value>>,
    composition_view: Option<crate::session::CompositionView>,
    /// What was let through, for the summary.
    warnings: Vec<String>,
}

fn restore(sess: &mut Session, a: &Value) -> Result<Value> {
    let state = match &a["state"] {
        Value::String(t) => {
            serde_json::from_str(t).map_err(|e| Error::Parse(format!("state is not JSON: {e}")))?
        }
        v => v.clone(),
    };
    let p = parse(sess, &state)?;
    let mut out = Map::new();
    // The store goes first: it is the one part that can still be refused
    // (a store on disk), and replace_all checks everything before it
    // changes anything, so a refusal leaves the session untouched too.
    if let Some(blobs) = &p.store {
        out.insert("store".into(), json!(store::open().replace_all(blobs)?));
    }
    if let Some(models) = p.models {
        out.insert("models".into(), json!(models.len()));
        sess.models = models;
    }
    if let Some(sy) = p.systems {
        out.insert(
            "systems".into(),
            json!({ "bindings": sy.bindings.len(), "systems": sy.systems.len() }),
        );
        sess.systems = sy;
    }
    if let Some(macros) = p.macros {
        out.insert("macros".into(), json!(macros.len()));
        sess.macros = macros;
    }
    if let Some(rec) = p.recording {
        // A recording in progress resumes: its steps rejoin the journal, so
        // macro.stop cuts them with whatever is recorded after the restore.
        out.insert("recording".into(), json!(rec.as_ref().map(Vec::len)));
        sess.recording = rec.map(|steps| {
            let start = sess.journal.len();
            sess.journal.extend(steps);
            start
        });
    }
    if let Some((items, next)) = p.notes {
        out.insert("notes".into(), json!(items.len()));
        sess.replace_notes(items, next);
    }
    if let Some((items, next, dropped)) = p.artifacts {
        out.insert("artifacts".into(), json!(items.len()));
        if !dropped.is_empty() {
            out.insert("artifacts_dropped".into(), json!(dropped));
        }
        sess.replace_artifacts(items, next);
    }
    if let Some(c) = p.composition {
        out.insert(
            "composition".into(),
            json!(c.as_ref().map(|c| c.specs.len())),
        );
        // The views re-read on a new revision; nothing to redraw when there
        // was none and none comes back.
        if sess.composition.is_some() || c.is_some() {
            sess.revision += 1;
        }
        sess.composition = c;
    }
    if let Some(plan) = p.plan {
        out.insert("plan".into(), json!(plan.is_some()));
        if sess.search_plan.is_some() || plan.is_some() {
            sess.revision += 1;
        }
        sess.search_plan = plan.map(|plan| crate::search_plan::OpenPlan {
            plan,
            game: Default::default(),
        });
    }
    if let Some(cv) = p.composition_view {
        out.insert("composition_view".into(), json!(cv.view));
        sess.composition_view = cv;
    }
    if !p.warnings.is_empty() {
        out.insert("warnings".into(), json!(p.warnings));
    }
    Ok(Value::Object(out))
}

/// Parses and checks every part present; nothing is applied.
fn parse(sess: &Session, state: &Value) -> Result<Parsed> {
    let obj = state.as_object().ok_or_else(|| {
        Error::BadArgs("state must be a JSON object, as session.dump_state returns it".into())
    })?;
    match obj.get("format").and_then(Value::as_str) {
        Some(FORMAT) => {}
        Some(f) => {
            return Err(Error::BadArgs(format!(
                "state has format {f:?}, not {FORMAT:?}: it is not a saved pflow-native session"
            )))
        }
        None => {
            return Err(Error::BadArgs(format!(
            "state has no format; a saved session has \"format\": {FORMAT:?} (session.dump_state)"
        )))
        }
    }
    match obj.get("version").and_then(Value::as_u64) {
        Some(v) if v > VERSION => {
            return Err(Error::Invalid(format!(
                "state is version {v} (written by pflow-native {}); this build, {}, reads version {VERSION} and older",
                obj.get("pflowNative")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown"),
                env!("CARGO_PKG_VERSION")
            )))
        }
        Some(v) if v >= 1 => {}
        _ => {
            return Err(Error::BadArgs(format!(
                "state has no version (a positive integer; {VERSION} for this build)"
            )))
        }
    }
    if let Some(k) = obj
        .keys()
        .find(|k| !HEADER.contains(&k.as_str()) && !PARTS.contains(&k.as_str()))
    {
        return Err(unknown_part(k));
    }
    let mut p = Parsed::default();
    if let Some(v) = obj.get("models") {
        p.models = Some(parse_models(v)?);
    }
    if let Some(v) = obj.get("systems") {
        p.systems = Some(typed("systems", v)?);
    }
    if let Some(v) = obj.get("macros") {
        let macros: BTreeMap<String, Vec<Step>> = typed("macros", v)?;
        for (name, steps) in &macros {
            unknown_steps(sess, &format!("macro {name:?}"), steps, &mut p.warnings);
        }
        p.macros = Some(macros);
    }
    if let Some(v) = obj.get("recording") {
        p.recording = Some(match v {
            Value::Null => None,
            v => {
                let steps: Vec<Step> = typed("recording", v)?;
                unknown_steps(sess, "the recording", &steps, &mut p.warnings);
                Some(steps)
            }
        });
    }
    if let Some(v) = obj.get("notes") {
        let (items, next) = parse_items("notes", v, 'n', super::note::MAX_NOTES)?;
        let items: Vec<Note> = items
            .iter()
            .map(|n| typed("notes", n))
            .collect::<Result<_>>()?;
        if let Some(n) = items
            .iter()
            .find(|n| n.text.chars().count() > super::note::MAX_TEXT)
        {
            return Err(bad(
                "notes",
                format!(
                    "note {} is longer than {} characters",
                    n.id,
                    super::note::MAX_TEXT
                ),
            ));
        }
        p.notes = Some((items, next));
    }
    if let Some(v) = obj.get("composition") {
        p.composition = Some(match v {
            Value::Null => None,
            // The document (either form) when the state has one; the older
            // `{instances, links}` of catalog ids otherwise (a state a build
            // before composition documents saved).
            v => Some(
                crate::composition::document::restore_part(v, &super::bundle_doc::from_store)
                    .map_err(|e| bad("composition", e))?,
            ),
        });
    }
    if let Some(v) = obj.get("plan") {
        p.plan = Some(match v {
            Value::Null => None,
            v @ Value::Object(_) => Some(crate::search_plan::canonical(v)),
            other => return Err(bad("plan", format!("a plan object or null, not {other}"))),
        });
    }
    if let Some(v) = obj.get("composition_view") {
        let cv: crate::session::CompositionView = typed("composition_view", v)?;
        if !super::view::COMPOSITION_VIEWS.contains(&cv.view.as_str()) {
            return Err(bad(
                "composition_view",
                format!("no composition view {:?}", cv.view),
            ));
        }
        if !["orders", "connect4"].contains(&cv.example.as_str()) {
            return Err(bad(
                "composition_view",
                format!("no example {:?}", cv.example),
            ));
        }
        p.composition_view = Some(cv);
    }
    if let Some(v) = obj.get("artifacts") {
        let (raw, next) = parse_items("artifacts", v, 'a', MAX_ARTIFACTS)?;
        // A result is derived data: one this build cannot read back is
        // dropped, not a reason to refuse the library, macros and store
        // beside it. JSON has no NaN or infinity, so a result holding one
        // (a diverging ODE) was saved with null there, and an f64 does not
        // read null. Its id stays counted in `next`, so a note pinned to it
        // is left orphaned, never moved onto a new result.
        let (mut items, mut dropped) = (vec![], vec![]);
        for r in &raw {
            let read = serde_json::from_value::<Artifact>(r.clone())
                .map_err(|e| e.to_string())
                .and_then(|a| a.data.validate().map(|()| a));
            match read {
                Ok(a) => items.push(a),
                Err(e) => {
                    let id = r["id"].as_str().unwrap_or_default().to_string();
                    let rerun = match r["source"]["command"].as_str() {
                        Some(c) if !c.is_empty() => format!("; {c} recomputes it"),
                        _ => String::new(),
                    };
                    p.warnings
                        .push(format!("result {id} was not restored: {e}{rerun}"));
                    dropped.push(id);
                }
            }
        }
        p.artifacts = Some((items, next, dropped));
    }
    if let Some(v) = obj.get("store") {
        p.store = Some(parse_store(v)?);
    }
    Ok(p)
}

/// Warns about each step whose command this build does not have (a state
/// saved by a native build, with `remote.*` or `rnn.*`, or by an older
/// one). The steps are kept as saved, as the user's own work: `macro.play`
/// refuses such a step when it is reached, and rolls the macro back.
fn unknown_steps(sess: &Session, what: &str, steps: &[Step], warnings: &mut Vec<String>) {
    for (i, st) in steps.iter().enumerate() {
        if sess.registry().find(&st.command).is_none() {
            warnings.push(format!(
                "{what}, step {i}: this build has no command {:?}, so playing it will fail there",
                st.command
            ));
        }
    }
}

fn typed<T: DeserializeOwned>(part: &str, v: &Value) -> Result<T> {
    serde_json::from_value(v.clone()).map_err(|e| bad(part, e))
}

fn parse_models(v: &Value) -> Result<BTreeMap<String, Document>> {
    let m = v
        .as_object()
        .ok_or_else(|| bad("models", "expected an object of name → document"))?;
    m.iter()
        .map(|(name, d)| {
            if !super::models::valid_name(name) {
                return Err(bad("models", format!("invalid model name {name:?}")));
            }
            let doc = Document::from_value(d).map_err(|e| bad("models", format!("{name}: {e}")))?;
            Ok((name.clone(), doc))
        })
        .collect()
}

/// The largest id counter a restore takes. A session hands out one id per
/// result or note, so a real one never gets near it, and counting on from
/// it can never wrap round to an id in use.
pub const MAX_NEXT: u64 = u32::MAX as u64;

/// `{"items": [...], "next": n}`, checked as a list of ids; the caller
/// reads the items. Each item is an object with a unique, non-empty `id`,
/// and `next` (default: the highest) is at least every `<prefix><n>` id and
/// at most [`MAX_NEXT`], so the next id handed out is a new one.
fn parse_items(part: &str, v: &Value, prefix: char, max: usize) -> Result<(Vec<Value>, u64)> {
    let obj = v
        .as_object()
        .ok_or_else(|| bad(part, "expected {\"items\": [...], \"next\": n}"))?;
    if let Some(k) = obj.keys().find(|k| *k != "items" && *k != "next") {
        return Err(bad(
            part,
            format!("unknown key {k:?}; takes items and next"),
        ));
    }
    let items: Vec<Value> = match obj.get("items") {
        Some(v) => typed(part, v)?,
        None => vec![],
    };
    if items.len() > max {
        return Err(bad(part, format!("{} items; at most {max}", items.len())));
    }
    let mut seen = BTreeSet::new();
    let mut highest = 0;
    for (i, it) in items.iter().enumerate() {
        let id = it
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| bad(part, format!("item {i} has no string id")))?;
        if id.is_empty() || !seen.insert(id) {
            return Err(bad(part, format!("id {id:?} is empty or repeated")));
        }
        if let Some(n) = id.strip_prefix(prefix).and_then(|d| d.parse::<u64>().ok()) {
            highest = highest.max(n);
        }
    }
    let next = match obj.get("next") {
        None | Some(Value::Null) => highest,
        Some(v) => v.as_u64().ok_or_else(|| {
            bad(
                part,
                format!("next must be a non-negative integer, got {v}"),
            )
        })?,
    };
    if next < highest {
        return Err(bad(
            part,
            format!(
                "next is {next}, but {prefix}{highest} exists, so the next id would repeat one"
            ),
        ));
    }
    if next > MAX_NEXT {
        return Err(bad(
            part,
            format!("next is {next}; a restored id counter is at most {MAX_NEXT}"),
        ));
    }
    Ok((items, next))
}

fn parse_store(v: &Value) -> Result<BTreeMap<String, String>> {
    let obj = v
        .as_object()
        .ok_or_else(|| bad("store", "expected {\"location\", \"blobs\": {key: text}}"))?;
    if let Some(k) = obj.keys().find(|k| *k != "location" && *k != "blobs") {
        return Err(bad(
            "store",
            format!("unknown key {k:?}; takes location and blobs"),
        ));
    }
    match obj.get("blobs") {
        Some(b) => typed("store", b),
        None => Err(bad("store", "no blobs")),
    }
}

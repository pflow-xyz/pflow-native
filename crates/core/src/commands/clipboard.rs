//! Copy / cut / paste / duplicate. A clip is a sub-document (the copied
//! nodes and the arcs among them) held in the session, so agents get the
//! same behavior; copy and cut also return it as text in the pflow
//! clipboard format (`docs/clipboard-format.md`, the contract pflow.xyz
//! implements on its side), and paste takes that text back. The app
//! carries it through the system clipboard.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde_json::{json, Map, Number, Value};

use crate::doc::{Document, NodeKind};
use crate::registry::{always, n, opt, ArgType::*, CommandSpec};
use crate::session::Session;
use crate::{Error, Result};

/// The clipboard format version this build reads and writes.
pub const CLIP_VERSION: u64 = 1;

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "edit.copy",
        label: "Copy",
        help: "Copy the selected nodes and the arcs between them. Returns the clip as pflow \
               clipboard text (`clip`, see docs/clipboard-format.md) and keeps it in the session.",
        menu: "Edit",
        shortcut: Some("Cmd+C"),
        args: &[],
        enabled: has_selection,
        run: copy,
        journal: true,
    },
    CommandSpec {
        id: "edit.cut",
        label: "Cut",
        help: "Copy the selection (returning `clip`, as edit.copy), then delete it.",
        menu: "Edit",
        shortcut: Some("Cmd+X"),
        args: &[],
        enabled: has_selection,
        run: cut,
        journal: true,
    },
    CommandSpec {
        id: "edit.paste",
        label: "Paste",
        help: "Paste `clip` (pflow clipboard text, or a whole pflow net), else the session \
               clipboard, offset by (dx, dy) with fresh ids; the pasted nodes become the \
               selection and their rates become session rates. One undo step.",
        menu: "Edit",
        shortcut: Some("Cmd+V"),
        args: &[
            opt(
                "clip",
                Json,
                "a pflow clip (pflowClip version 1) or a pflow net, as JSON text or an object; \
                 default: the session clipboard",
            ),
            opt("dx", Num, "x offset (default 40)"),
            opt("dy", Num, "y offset (default 40)"),
        ],
        // Always: `enabled` cannot see arguments, and a `clip` needs no
        // session clipboard (the app passes the system clipboard's text).
        // With neither, `paste` refuses with "clipboard is empty".
        enabled: always,
        run: paste,
        journal: true,
    },
    CommandSpec {
        id: "edit.duplicate",
        label: "Duplicate",
        help: "Copy and paste the selection in one step.",
        menu: "Edit",
        shortcut: Some("Cmd+D"),
        args: &[],
        enabled: has_selection,
        run: duplicate,
        journal: true,
    },
];

/// Why a text or JSON value is not something `edit.paste` takes.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ClipError {
    /// Neither a pflow clip nor a pflow net: a reader falls back to its
    /// in-app clipboard.
    #[error("not a pflow clip: {0}")]
    NotAClip(String),
    /// A clip in a later format than this build reads: refused, not guessed at.
    #[error(
        "this clip is pflow clipboard format version {0}; pflow-native reads version {CLIP_VERSION}"
    )]
    TooNew(Number),
    /// A pflow clip (or net) whose contents are not a well-formed fragment.
    #[error("bad pflow clip: {0}")]
    Malformed(String),
}

impl From<ClipError> for Error {
    fn from(e: ClipError) -> Self {
        Error::Invalid(e.to_string())
    }
}

/// Classifies clipboard text by the format's reader rules: a version 1
/// clip, or a plain pflow net (pasted whole), is returned as the fragment
/// to paste (rates still in the transitions' `rate` fields). The app and
/// `edit.paste` share it, so they agree on what pastes.
pub fn parse_clip(text: &str) -> std::result::Result<Document, ClipError> {
    let v: Value = serde_json::from_str(text.trim())
        .map_err(|e| ClipError::NotAClip(format!("not JSON ({e})")))?;
    clip_from_value(&v)
}

/// [`parse_clip`] for an already-parsed JSON value.
pub fn clip_from_value(v: &Value) -> std::result::Result<Document, ClipError> {
    let obj = v
        .as_object()
        .ok_or_else(|| ClipError::NotAClip("not a JSON object".into()))?;
    match obj.get("pflowClip") {
        Some(marker) => match marker.get("version").and_then(Value::as_number) {
            Some(n) if n.as_f64() == Some(CLIP_VERSION as f64) => {}
            Some(n) if n.as_f64().is_some_and(|f| f > CLIP_VERSION as f64) => {
                return Err(ClipError::TooNew(n.clone()))
            }
            _ => {
                return Err(ClipError::NotAClip(
                    "pflowClip has no version this format defines".into(),
                ))
            }
        },
        None => {
            let pflow = obj.get("@context").and_then(Value::as_str)
                == Some(crate::cid::PFLOW_CONTEXT_URL)
                || obj.get("@type").and_then(Value::as_str) == Some("PetriNet");
            if !pflow || !(obj.contains_key("places") || obj.contains_key("transitions")) {
                return Err(ClipError::NotAClip(
                    "no pflowClip marker, and not a pflow PetriNet document".into(),
                ));
            }
        }
    }
    shape(obj).map_err(ClipError::Malformed)?;
    let mut doc = Document::from_value(v).map_err(|e| {
        ClipError::Malformed(match e {
            Error::Parse(why) => why,
            e => e.to_string(),
        })
    })?;
    doc.extra.shift_remove("pflowClip");
    check(&doc).map_err(ClipError::Malformed)?;
    Ok(doc)
}

/// `Document::from_value` skips a `transitions` or `arcs` of the wrong
/// shape and reads a node that is not an object as a default one; from a
/// file that is lenience, from a clip a paste that silently comes out
/// partial.
fn shape(obj: &Map<String, Value>) -> std::result::Result<(), String> {
    for (key, what) in [("places", "place"), ("transitions", "transition")] {
        match obj.get(key) {
            None => {}
            Some(Value::Object(nodes)) => {
                if let Some(id) = nodes
                    .iter()
                    .find_map(|(id, n)| (!n.is_object()).then_some(id))
                {
                    return Err(format!("{what} {id:?} is not an object"));
                }
            }
            Some(_) => return Err(format!("{key:?} must be an object keyed by id")),
        }
    }
    match obj.get("arcs") {
        None | Some(Value::Array(_)) => Ok(()),
        Some(_) => Err("\"arcs\" must be an array".into()),
    }
}

/// What a foreign clip must be for paste: at least one node, ids unique
/// across places and transitions, every arc between two of its nodes
/// joining a place and a transition, and no negative tokens, capacities
/// or weights. An arc with an end outside the clip is not part of it (a
/// clip holds the arcs whose two ends were copied), so paste skips it, as
/// pflow.xyz's does. Parallel arcs are kept (pflow.xyz draws them). The
/// session clipboard is not checked: it is the session's own nodes, pasted
/// as they are.
fn check(clip: &Document) -> std::result::Result<(), String> {
    if clip.places.is_empty() && clip.transitions.is_empty() {
        return Err("it holds no places or transitions".into());
    }
    let mut kinds = HashMap::new();
    let nodes = clip
        .places
        .iter()
        .map(|p| (p.id.as_str(), NodeKind::Place))
        .chain(
            clip.transitions
                .iter()
                .map(|t| (t.id.as_str(), NodeKind::Transition)),
        );
    for (id, kind) in nodes {
        if kinds.insert(id, kind).is_some() {
            return Err(format!("id {id:?} names two nodes"));
        }
    }
    if let Some(p) = clip
        .places
        .iter()
        .find(|p| p.initial.iter().chain(&p.capacity).any(|v| *v < 0.0))
    {
        return Err(format!(
            "place {:?} has a negative initial or capacity",
            p.id
        ));
    }
    for a in &clip.arcs {
        let arc = format!("arc {} → {}", a.source, a.target);
        let (Some(&from), Some(&to)) = (kinds.get(a.source.as_str()), kinds.get(a.target.as_str()))
        else {
            continue; // skipped by put()
        };
        if from == to {
            let what = if from == NodeKind::Place {
                "places"
            } else {
                "transitions"
            };
            return Err(format!("{arc} joins two {what}"));
        }
        if a.weight.iter().any(|w| *w < 0.0) {
            return Err(format!("{arc} has a negative weight"));
        }
    }
    Ok(())
}

/// `clip` as the system clipboard carries it: pflow JSON-LD plus the
/// `pflowClip` marker, unsealed (no `@id`).
fn clip_text(clip: &Document) -> String {
    let mut marked = clip.clone();
    marked.extra = Map::from_iter([(
        "pflowClip".to_string(),
        json!({ "version": CLIP_VERSION, "origin": "pflow-native" }),
    )]);
    serde_json::to_string_pretty(&marked.to_value()).expect("serializable")
}

fn has_selection(s: &Session) -> std::result::Result<(), String> {
    (!s.selection.is_empty())
        .then_some(())
        .ok_or_else(|| "nothing selected".into())
}

fn copy(sess: &mut Session, _: &Value) -> Result<Value> {
    let sel: HashSet<&String> = sess.selection.iter().collect();
    let doc = &sess.doc;
    let nodes = Document {
        token: doc.token.clone(),
        places: doc
            .places
            .iter()
            .filter(|p| sel.contains(&p.id))
            .cloned()
            .collect(),
        transitions: doc
            .transitions
            .iter()
            .filter(|t| sel.contains(&t.id))
            .cloned()
            .collect(),
        arcs: doc
            .arcs
            .iter()
            .filter(|a| sel.contains(&a.source) && sel.contains(&a.target))
            .cloned()
            .collect(),
        extra: Default::default(),
    };
    // Rates ride in the transitions' `rate` fields, as file.save writes
    // them, so a paste here or in pflow.xyz brings them along.
    let clip = super::file::with_rates(&nodes, &sess.rates);
    let out = json!({
        "places": clip.places.len(),
        "transitions": clip.transitions.len(),
        "arcs": clip.arcs.len(),
        "clip": clip_text(&clip),
    });
    sess.clipboard = Some(clip);
    Ok(out)
}

fn cut(sess: &mut Session, a: &Value) -> Result<Value> {
    let out = copy(sess, a)?;
    // Directly, not via execute: a nested command would not get its own
    // undo point.
    super::edit::delete(sess, &json!({}))?;
    Ok(out)
}

fn paste(sess: &mut Session, a: &Value) -> Result<Value> {
    let mut clip = match a.get("clip") {
        Some(Value::String(text)) => parse_clip(text)?,
        Some(v) if !v.is_null() => clip_from_value(v)?,
        _ => sess.clipboard.clone().ok_or_else(|| {
            Error::Invalid("clipboard is empty: copy something first, or pass `clip`".into())
        })?,
    };
    let (dx, dy) = (n(a, "dx").unwrap_or(40.0), n(a, "dy").unwrap_or(40.0));
    let rates = super::file::take_rates(&mut clip);
    if rates.is_empty() {
        sess.edit("paste", |s| put(s, clip, &rates, dx, dy))
    } else {
        // Rates that arrive with the clip undo with it.
        sess.edit_with_rates("paste", |s| put(s, clip, &rates, dx, dy))
    }
}

/// First-free `p{n}` / `t{n}` ids, as [`Document::fresh_id`] picks them,
/// without rescanning the net for each pasted node: a pasted net can be
/// large, and the app pastes on the UI thread.
struct FreshIds {
    used: HashSet<String>,
    next: HashMap<&'static str, usize>,
}

impl FreshIds {
    fn new(doc: &Document) -> Self {
        let places = doc.places.iter().map(|p| p.id.clone());
        let used = places.chain(doc.transitions.iter().map(|t| t.id.clone()));
        Self {
            used: used.collect(),
            next: HashMap::new(),
        }
    }

    /// Ids are only added during a paste, so the first free one never
    /// moves back and each search resumes where the last one stopped.
    fn take(&mut self, prefix: &'static str) -> String {
        let n = self.next.entry(prefix).or_default();
        loop {
            let id = format!("{prefix}{n}");
            *n += 1;
            if self.used.insert(id.clone()) {
                return id;
            }
        }
    }
}

/// Adds a clip under fresh ids; `rates` are keyed by clip id.
fn put(
    s: &mut Session,
    clip: Document,
    rates: &BTreeMap<String, f64>,
    dx: f64,
    dy: f64,
) -> Result<Value> {
    let ncol = s.doc.colors();
    let mut fresh = FreshIds::new(&s.doc);
    let mut map = HashMap::new();
    let mut new_ids = vec![];
    for mut p in clip.places {
        let id = fresh.take("p");
        map.insert(std::mem::replace(&mut p.id, id.clone()), id.clone());
        p.x += dx;
        p.y += dy;
        p.initial.resize(ncol, 0.0);
        p.capacity.resize(ncol, 0.0);
        s.marking.insert(
            id.clone(),
            p.initial.iter().map(|f| f.round() as i64).collect(),
        );
        s.doc.places.push(p);
        new_ids.push(id);
    }
    for mut t in clip.transitions {
        let id = fresh.take("t");
        let old = std::mem::replace(&mut t.id, id.clone());
        if let Some(r) = rates.get(&old) {
            s.rates.insert(id.clone(), *r);
        }
        // A `rate` take_rates left is not a usable one (negative, not a
        // number); dropped, so file.save and the next copy do not pass it
        // on. pflow.xyz ignores the field, so a paste never fails on it.
        t.extra.shift_remove("rate");
        map.insert(old, id.clone());
        t.x += dx;
        t.y += dy;
        s.doc.transitions.push(t);
        new_ids.push(id);
    }
    let mut skipped = 0;
    for mut arc in clip.arcs {
        let (Some(source), Some(target)) = (map.get(&arc.source), map.get(&arc.target)) else {
            // An end outside the clip (check()): not part of it.
            skipped += 1;
            continue;
        };
        arc.source = source.clone();
        arc.target = target.clone();
        if arc.weight.is_empty() {
            arc.weight.push(1.0);
        }
        arc.weight.resize(ncol, 0.0);
        s.doc.arcs.push(arc);
    }
    s.selection = new_ids.clone();
    let mut out = json!({ "pasted": new_ids });
    if skipped > 0 {
        out["skippedArcs"] = json!(skipped);
    }
    Ok(out)
}

fn duplicate(sess: &mut Session, _: &Value) -> Result<Value> {
    let saved = sess.clipboard.take();
    let out = copy(sess, &Value::Null).and_then(|_| paste(sess, &json!({})));
    sess.clipboard = saved;
    out
}

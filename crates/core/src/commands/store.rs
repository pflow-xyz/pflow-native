//! `store.*`: the local content-addressed store ([`crate::store`]) —
//! models by CID, the knowledge graph (link / edges / neighbors / query),
//! collections, licenses, versioning and content-addressed tables. sim's
//! `create_model` … `map_list` family, against a store on this machine.
//!
//! The store is per-user and outlives the session, so it is reached
//! through [`crate::store::open`], not a `Session` field (see that module).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde_json::{json, Value};

use super::bundle_doc;
use crate::artifact::{ArtifactData, GraphEdge, GraphNode};
use crate::composition::content::ModelContent;
use crate::composition::document::{self, refuse, value_arg, Form};
use crate::composition::{self as comp, Composition};
use crate::doc::Document;
use crate::registry::{always, arg, b, n, opt, s, strs, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::store::{self, predicate, DocKind, Snapshot};
use crate::{Error, Result};

const MENU: &str = "Store";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "store.put",
        label: "Put in Store…",
        help:
            "Seal a document (its pflow.xyz CID) and keep it in the local store. By default the \
               open net, or a library net by `model`. `composition: true` stores the open \
               composition (`form` auto, composition or bundle, as bundle.export); `content: true` \
               with it also stores the attached content, recorded as about the composition's CID, \
               and alone stores just that content. `document` stores any PetriNet, PetriNetBundle, \
               PatternComposition or ModelContent document you pass. Idempotent by CID: storing the \
               same document again returns existed: true and keeps the first metadata. `parents` \
               records provenance (a net defaults to its own `parents`). A bundle, composition or \
               content result carries `kind`; a net's result is as it always was.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt(
                "model",
                Str,
                "\"current\" (default) or a library model name",
            ),
            opt(
                "title",
                Str,
                "title for listings (defaults to the document's name)",
            ),
            opt(
                "parents",
                StrList,
                "CIDs this model was derived from, nearest first",
            ),
            opt(
                "composition",
                Bool,
                "store the open composition instead of a net",
            ),
            opt(
                "form",
                Str,
                "with composition: auto (default), composition or bundle",
            ),
            opt(
                "content",
                Bool,
                "store the open composition's content (with composition: both)",
            ),
            opt(
                "document",
                Json,
                "any PetriNet, PetriNetBundle, PatternComposition or ModelContent document (or a string holding it)",
            ),
        ],
        enabled: always,
        run: put,
        journal: true,
    },
    CommandSpec {
        id: "store.get",
        label: "Get Stored Model",
        help: "A stored model's pflow.xyz JSON-LD (with @id = its CID).",
        menu: "",
        shortcut: None,
        args: &[arg("cid", Str, "model CID")],
        enabled: always,
        run: get,
        journal: false,
    },
    CommandSpec {
        id: "store.open",
        label: "Open from Store…",
        help: "Open a stored document by its kind (undoable). A net becomes the open net (it has no \
               file path); a composition or bundle becomes the open composition, as bundle.load \
               {cid} does; content attaches to the open composition (refused when none is open, \
               or when an element id does not resolve).",
        menu: MENU,
        shortcut: None,
        args: &[arg("cid", Str, "model CID")],
        enabled: always,
        run: open,
        journal: true,
    },
    CommandSpec {
        id: "store.list",
        label: "List Stored Models",
        help: "Stored models, newest first, with title, kind (net, bundle, composition or content), \
               size, created (ms), collections, license, successor and, for content, the \
               composition it is about. `query` filters by title or CID substring, `kind` by kind, \
               `about` to the content stored about that composition CID. Stores a table artifact.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt(
                "query",
                Str,
                "case-insensitive substring of the title or CID",
            ),
            opt("kind", Str, "only this kind: net, bundle, composition or content"),
            opt("about", Str, "only content about this composition CID"),
            opt("limit", Int, "at most this many rows (default 100)"),
        ],
        enabled: always,
        run: list,
        journal: false,
    },
    CommandSpec {
        id: "store.delete",
        label: "Delete from Store",
        help: "Remove a stored model. Refused for a licensed model (irrevocable) and, unless \
               force, for one any edge names; edges are kept either way.",
        menu: "",
        shortcut: None,
        args: &[
            arg("cid", Str, "model CID"),
            opt("force", Bool, "delete even though edges name it"),
        ],
        enabled: always,
        run: delete,
        journal: true,
    },
    CommandSpec {
        id: "store.link",
        label: "Link…",
        help: "Record a typed edge between stored entities (sim_link): predicate from sim's \
               registry (@type, about, broader, cites, closeMatch, hasMember, related), both ends \
               stored entities of the kinds it relates. Re-linking returns the existing edge.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("subject", Str, "id of the entity the edge starts from"),
            arg(
                "predicate",
                Str,
                "registered predicate (store.info lists them)",
            ),
            arg(
                "object",
                Str,
                "id of the entity the edge points to; for @type a model kind",
            ),
        ],
        enabled: always,
        run: link,
        journal: true,
    },
    CommandSpec {
        id: "store.edges",
        label: "Edges",
        help: "Every relation touching an entity, as subject or object, ordered by time. \
               Stores a table artifact.",
        menu: "",
        shortcut: None,
        args: &[
            arg("id", Str, "entity id"),
            opt("predicate", Str, "only this predicate's edges"),
        ],
        enabled: always,
        run: edges,
        journal: false,
    },
    CommandSpec {
        id: "store.neighbors",
        label: "Neighbors",
        help:
            "One hop from an entity through the registry (sim_neighbors): no predicate = objects \
               of its outgoing edges; an inverse name (narrower) reads its stored predicate \
               backwards; a symmetric one counts both ways. direction: out (default), in, both.",
        menu: "",
        shortcut: None,
        args: &[
            arg("id", Str, "entity id"),
            opt("predicate", Str, "restrict to this predicate"),
            opt("direction", Str, "out | in | both"),
        ],
        enabled: always,
        run: neighbors,
        journal: false,
    },
    CommandSpec {
        id: "store.query",
        label: "Query Graph…",
        help: "Conjunctive pattern query over the edges (sim_query): {select, where: [{subject, \
               predicate, object, mode: edge|path|closure, maxHops}]}, ?name for variables; no \
               variables = a yes/no question. Stores table and graph artifacts.",
        menu: MENU,
        shortcut: None,
        args: &[arg(
            "pattern",
            Json,
            "the query: {select?, where}, a pattern list, or its JSON text",
        )],
        enabled: always,
        run: query,
        journal: false,
    },
    CommandSpec {
        id: "store.collection_create",
        label: "New Collection…",
        help: "Create a named collection (the name is its content id, so creating it twice \
               returns the same collection).",
        menu: MENU,
        shortcut: None,
        args: &[arg("name", Str, "collection name")],
        enabled: always,
        run: collection_create,
        journal: true,
    },
    CommandSpec {
        id: "store.collection_add",
        label: "Add to Collection…",
        help: "Add a stored entity to a collection (a hasMember edge).",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("collection", Str, "collection id or name"),
            arg("cid", Str, "model CID (or any stored entity id)"),
        ],
        enabled: always,
        run: collection_add,
        journal: true,
    },
    CommandSpec {
        id: "store.collection_list",
        label: "List Collections",
        help: "Collections with their member counts. Stores a table artifact.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: collection_list,
        journal: false,
    },
    CommandSpec {
        id: "store.license",
        label: "License…",
        help: "Dedicate a stored model to the commons (CC0-1.0, CC-BY-4.0, CC-BY-SA-4.0). \
               Irrevocable: a licensed model cannot be relicensed or deleted.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("cid", Str, "model CID"),
            arg("license", Str, "CC0-1.0 | CC-BY-4.0 | CC-BY-SA-4.0"),
        ],
        enabled: always,
        run: license,
        journal: true,
    },
    CommandSpec {
        id: "store.supersede",
        label: "Supersede…",
        help: "Record that `new` replaces `old` (a supersededBy edge). Old edges stay; \
               store.migration_offers lists those that can be carried forward.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("old", Str, "the superseded model's CID"),
            arg("new", Str, "its successor's CID"),
        ],
        enabled: always,
        run: supersede,
        journal: true,
    },
    CommandSpec {
        id: "store.migration_offers",
        label: "Migration Offers",
        help: "What supersession does to each edge of a superseded model: taxonomic edges are \
               offered for the current successor (with the link check on the proposed edge); \
               tool-written and structural edges stay. Stores a table artifact.",
        menu: "",
        shortcut: None,
        args: &[
            arg("cid", Str, "the superseded model's CID"),
            opt("predicate", Str, "only this predicate's edges"),
        ],
        enabled: always,
        run: migration_offers,
        journal: false,
    },
    CommandSpec {
        id: "store.accept_migration",
        label: "Accept Migration…",
        help: "Carry offered edges of a superseded model to its current successor: each becomes \
               the same edge on the successor plus a migratedFrom edge back (the old edge stays). \
               All-or-nothing; `edges` defaults to every pending offer.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("old", Str, "the superseded model's CID"),
            opt(
                "new",
                Str,
                "the successor you read the offers against (checked)",
            ),
            opt(
                "edges",
                StrList,
                "relation ids from store.migration_offers (1–25)",
            ),
        ],
        enabled: always,
        run: accept_migration,
        journal: true,
    },
    CommandSpec {
        id: "store.map_put",
        label: "Put Table…",
        help: "Store a key→value table; its id is the hash of its canonical JSON (sim's map id), \
               so the same data in any key order is the same map.",
        menu: "",
        shortcut: None,
        args: &[arg("data", Json, "a JSON object (or its JSON text)")],
        enabled: always,
        run: map_put,
        journal: true,
    },
    CommandSpec {
        id: "store.map_get",
        label: "Get Table",
        help: "A stored table's data by id.",
        menu: "",
        shortcut: None,
        args: &[arg("id", Str, "map id")],
        enabled: always,
        run: map_get,
        journal: false,
    },
    CommandSpec {
        id: "store.map_list",
        label: "List Tables",
        help: "The id and key count of every stored table. Stores a table artifact.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: map_list,
        journal: false,
    },
    CommandSpec {
        id: "store.graph",
        label: "Knowledge Graph",
        help: "The store as a graph: models, collections and the edges between them (plus \
               provenance parents). `around` limits it to `depth` hops (default 2) of one entity.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("around", Str, "entity id (or collection name) to centre on"),
            opt("depth", Int, "hops from `around` (default 2)"),
        ],
        enabled: always,
        run: graph,
        journal: false,
    },
    CommandSpec {
        id: "store.info",
        label: "Store Info",
        help: "Where the store lives, what it holds, and the predicate registry.",
        menu: MENU,
        shortcut: None,
        args: &[],
        enabled: always,
        run: info,
        journal: false,
    },
];

fn src(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

fn table(
    sess: &mut Session,
    title: &str,
    step: Step,
    columns: &[&str],
    rows: Vec<Vec<Value>>,
) -> String {
    sess.add_artifact(
        title.into(),
        step,
        ArtifactData::Table {
            columns: columns.iter().map(|c| c.to_string()).collect(),
            rows,
        },
    )
}

/// A short label for an id: a model's title, a collection's name.
fn label(snap: &Snapshot, id: &str) -> String {
    if let Some(m) = snap.models.get(id) {
        if !m.title.is_empty() {
            return m.title.clone();
        }
    }
    if let Some(c) = snap.collections.get(id) {
        return c.name.clone();
    }
    if id.len() > 12 {
        format!("{}…", &id[..10])
    } else {
        id.to_string()
    }
}

fn put(sess: &mut Session, a: &Value) -> Result<Value> {
    let (composition, content) = (
        b(a, "composition").unwrap_or(false),
        b(a, "content").unwrap_or(false),
    );
    let document = a.get("document").filter(|v| !v.is_null());
    if s(a, "form").is_some() && !composition {
        return Err(Error::BadArgs(
            "form says which document `composition: true` stores; pass it with composition".into(),
        ));
    }
    if let Some(d) = document {
        if composition || content || s(a, "model").is_some() {
            return Err(Error::BadArgs(
                "document is the whole request: pass it without model, composition or content"
                    .into(),
            ));
        }
        return put_given(a, d);
    }
    if composition || content {
        if s(a, "model").is_some() {
            return Err(Error::BadArgs(
                "model names a net; composition and content store the open composition".into(),
            ));
        }
        return put_composition(sess, a, composition, content);
    }
    let name = s(a, "model").unwrap_or("current");
    // The open net is stored as a save would write it, rates included.
    let doc = if matches!(name, "" | "current") {
        sess.export_doc()
    } else {
        super::models::resolve(sess, name)?
    };
    store::open().put_model(&doc, s(a, "title"), strs(a, "parents"))
}

/// The places and transitions of the flat net, or 0 and 0 when it does not
/// flatten (what the index shows for a composition).
fn flat_counts(c: &Composition) -> (usize, usize) {
    comp::flatten(&c.bundle)
        .map(|f| (f.model.places.len(), f.model.transitions.len()))
        .unwrap_or((0, 0))
}

/// Adds `note` to a result, after any note it already has.
fn add_note(out: &mut Value, note: Option<String>) {
    let Some(n) = note else { return };
    out["note"] = match out["note"].as_str() {
        Some(prev) => json!(format!("{prev}; {n}")),
        None => json!(n),
    };
}

/// `store.put {composition, content}`: the open composition, its content, or both.
fn put_composition(sess: &Session, a: &Value, composition: bool, content: bool) -> Result<Value> {
    let c = sess.composition.as_ref().ok_or_else(|| {
        Error::Invalid("no composition is open (bundle.open, bundle.load)".into())
    })?;
    let form = match s(a, "form").unwrap_or("auto") {
        "auto" => None,
        "composition" => Some(Form::Composition),
        "bundle" => Some(Form::Bundle),
        other => {
            return Err(Error::BadArgs(format!(
                "form {other:?}; there are auto, composition, bundle"
            )))
        }
    };
    // Everything that can be refused is, before anything is written.
    let k = if content {
        let k = c.doc.content.as_ref().ok_or_else(|| {
            refuse(
                "NO_CONTENT",
                "no model content is attached to the open composition (bundle.load {content}, content.load)",
            )
        })?;
        k.cid()?;
        Some(k)
    } else {
        None
    };
    let st = store::open();
    let (mut out, about) = if composition {
        let exp = c.export(form)?;
        let kind = match exp.form {
            Form::Composition => DocKind::Composition,
            Form::Bundle => DocKind::Bundle,
        };
        let mut out = st.put_document(
            &exp.document,
            kind,
            s(a, "title"),
            strs(a, "parents"),
            None,
            flat_counts(c),
        )?;
        if let Some(bc) = &exp.bundle_cid {
            out["bundle_cid"] = json!(bc);
        }
        // Content the user attached is part of the model's meaning; the library's is the build's
        // and needs no storing. Say when it was left out.
        if !content && c.doc.content_source == document::ContentSource::Loaded {
            add_note(
                &mut out,
                Some(
                    "this composition has content attached that was not stored; store.put {content: true} stores it, about this composition"
                        .into(),
                ),
            );
        }
        (Some(out), exp.cid)
    } else {
        (None, c.export(None)?.cid)
    };
    if let Some(k) = k {
        // Alone, the content takes the title and parents; beside a
        // composition they are the composition's.
        let alone = out.is_none();
        let r = st.put_document(
            &k.to_document(),
            DocKind::Content,
            s(a, "title").filter(|_| alone),
            strs(a, "parents").filter(|_| alone),
            Some(&about),
            (0, 0),
        )?;
        match &mut out {
            Some(o) => o["content"] = json!({"cid": r["cid"], "existed": r["existed"]}),
            None => out = Some(r),
        }
    }
    Ok(out.expect("composition or content was asked for"))
}

/// `store.put {document}`: a document of any kind the store keeps.
fn put_given(a: &Value, d: &Value) -> Result<Value> {
    let doc = value_arg(d, "document")?;
    let kind = store::require_kind(&doc)?;
    let (title, parents) = (s(a, "title"), strs(a, "parents"));
    let st = store::open();
    match kind {
        DocKind::Content => {
            // Its composition is the one it says it was written against.
            let (about, note) = match doc.get("composition").and_then(Value::as_str) {
                Some(c) if store::canon::is_model_cid(c) => (Some(c), None),
                Some(c) => (
                    None,
                    Some(format!(
                        "the document's composition {c:?} is not a model CID, so no `about` was recorded"
                    )),
                ),
                None => (None, None),
            };
            // Stored as the document every other consumer names (`content.export`, `about`, a
            // receipt's `content`): the normalised one, `check` and `scenario` as canonical text.
            // A document that does not parse is stored as given, as a composition that does not
            // load is, and `store.open` refuses it until it does.
            let (stored, normalised) = match ModelContent::parse(&doc) {
                Ok(mc) => {
                    let normal = mc.to_document();
                    let (got, want) = (crate::cid::seal(&doc).ok(), crate::cid::seal(&normal).ok());
                    let said = (got != want).then(|| {
                        format!(
                            "stored as the normalised document (check and scenario as canonical text), which is what content.export writes and receipts name; the document as received seals to {}",
                            got.unwrap_or_default()
                        )
                    });
                    (normal, said)
                }
                Err(e) => (
                    doc.clone(),
                    Some(format!(
                        "the document does not parse as model content ({e}); it is stored as given, and store.open refuses it until it does"
                    )),
                ),
            };
            let mut out = st.put_document(&stored, kind, title, parents, about, (0, 0))?;
            add_note(&mut out, note);
            add_note(&mut out, normalised);
            Ok(out)
        }
        DocKind::Composition | DocKind::Bundle => {
            // Counts come from loading it. One that does not load is stored as given.
            let mut bare = doc.clone();
            if let Value::Object(m) = &mut bare {
                m.shift_remove("@id");
            }
            let (counts, canonical, mut note) =
                match document::load(&bare, &bundle_doc::from_store) {
                    Ok(l) => {
                        let exp = l.composition.export(Some(l.form)).ok();
                        (
                            flat_counts(&l.composition),
                            exp,
                            None,
                        )
                    }
                    Err(e) => (
                        (0, 0),
                        None,
                        Some(format!(
                            "the document does not load ({e}); it is stored as given, and store.open refuses it until it does"
                        )),
                    ),
                };
            let mut out = st.put_document(&doc, kind, title, parents, None, counts)?;
            // What model identity says of it: a bare wrapper of one part IS that part's net, which
            // the store files under the wrapper's own CID, a document of its own.
            bundle_doc::identity_into(&mut out, &bare);
            // The store writes the CID as `@id` and the loaders are given the document without it,
            // so an `@id` of the author's does not come back with the document.
            if let Some(own) = doc.get("@id").filter(|own| **own != out["cid"]) {
                note = Some(match note {
                    Some(n) => n,
                    None => format!(
                        "the document's own @id {own} is replaced by its CID in the store, and store.open gives the document without it"
                    ),
                });
            }
            if let Some(exp) = canonical {
                if let Some(bc) = &exp.bundle_cid {
                    out["bundle_cid"] = json!(bc);
                }
                if out["cid"] != json!(exp.cid) {
                    note = Some(format!(
                        "this is not the form bundle.export writes (that is {}); bundle.load it and bundle.export for the canonical document",
                        exp.cid
                    ));
                }
            }
            add_note(&mut out, note);
            Ok(out)
        }
        DocKind::Net => st.put_model(&Document::from_value(&doc)?, title, parents),
        DocKind::Unknown => unreachable!("require_kind refused it"),
    }
}

fn get(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().get_model(s(a, "cid").unwrap())
}

fn open(sess: &mut Session, a: &Value) -> Result<Value> {
    let cid = s(a, "cid").unwrap();
    let v = store::open().get_model(cid)?;
    match store::require_kind(&v)? {
        DocKind::Net => open_net(sess, cid, &v),
        DocKind::Composition | DocKind::Bundle => open_composition(sess, cid),
        DocKind::Content => open_content(sess, cid),
        DocKind::Unknown => unreachable!("require_kind refused it"),
    }
}

fn open_net(sess: &mut Session, cid: &str, v: &Value) -> Result<Value> {
    let doc = Document::from_value(v)?;
    let out = json!({
        "cid": cid,
        "places": doc.places.len(),
        "transitions": doc.transitions.len(),
        "arcs": doc.arcs.len(),
    });
    super::file::replace(sess, doc)?;
    sess.path = None;
    Ok(out)
}

/// A stored composition or bundle becomes the open composition, exactly as
/// `bundle.load {cid}` opens it.
fn open_composition(sess: &mut Session, cid: &str) -> Result<Value> {
    let (c, form) = bundle_doc::build(&json!({ "cid": cid }))?;
    let mut out = bundle_doc::load_result(&c, form)?;
    out["kind"] = json!(form.name());
    sess.edit("open composition from store", |s| {
        s.composition = Some(c);
        Ok(())
    })?;
    Ok(out)
}

/// Stored content attaches to the open composition, strictly (an element id
/// that does not resolve refuses it, as `bundle.load {content}` does).
fn open_content(sess: &mut Session, cid: &str) -> Result<Value> {
    let st = store::open();
    let mc = ModelContent::parse(&st.get_sealed(cid)?)?;
    if sess.composition.is_none() {
        return Err(refuse(
            "NO_COMPOSITION",
            format!(
                "no composition is open: content {cid} attaches to one (bundle.open, bundle.load, or store.open a composition first)"
            ),
        ));
    }
    let about = st.model_meta(cid)?.and_then(|m| m.about);
    let (summary, open_cid) = sess.edit("attach content from store", |s| {
        let c = s.composition.as_mut().expect("checked above");
        bundle_doc::attach(c, mc, true)?;
        Ok((document::content_json(c), c.document_cid().ok()))
    })?;
    let mut out = json!({"cid": cid, "kind": "content", "about": about, "content": summary});
    if let (Some(a), Some(open)) = (&about, &open_cid) {
        if a != open {
            out["note"] = json!(format!(
                "this content was stored about {a}; the open composition is {open}"
            ));
        }
    }
    Ok(out)
}

fn list(sess: &mut Session, a: &Value) -> Result<Value> {
    let snap = store::open().snapshot()?;
    let q = s(a, "query").unwrap_or("").to_lowercase();
    let limit = n(a, "limit").map_or(100, |l| l.max(0.0) as usize);
    let kind = s(a, "kind").filter(|k| !k.is_empty());
    let about = s(a, "about").filter(|k| !k.is_empty());
    if let Some(k) = kind {
        if !["net", "bundle", "composition", "content"].contains(&k) {
            return Err(Error::BadArgs(format!(
                "kind {k:?}; there are net, bundle, composition, content"
            )));
        }
    }
    let kind_name = |m: &store::ModelMeta| DocKind::from_index(m.kind.as_deref()).name();
    let mut all: Vec<(&String, &store::ModelMeta)> = snap
        .models
        .iter()
        .filter(|(cid, m)| {
            q.is_empty() || cid.to_lowercase().contains(&q) || m.title.to_lowercase().contains(&q)
        })
        .filter(|(_, m)| kind.is_none_or(|k| kind_name(m) == k))
        .filter(|(_, m)| about.is_none_or(|a| m.about.as_deref() == Some(a)))
        .collect();
    all.sort_by(|x, y| y.1.created.cmp(&x.1.created).then(x.0.cmp(y.0)));
    let total = all.len();
    let rows: Vec<Value> = all
        .into_iter()
        .take(limit)
        .map(|(cid, m)| {
            json!({
                "cid": cid,
                "title": m.title,
                "kind": kind_name(m),
                "about": m.about,
                "places": m.places,
                "transitions": m.transitions,
                "created": m.created,
                "collections": snap.collections_of(cid),
                "license": m.license,
                "superseded_by": m.superseded_by,
                "parents": m.parents,
            })
        })
        .collect();
    let cols = [
        "cid",
        "title",
        "kind",
        "places",
        "transitions",
        "created",
        "collections",
        "license",
        "superseded_by",
        "about",
    ];
    let trows = rows
        .iter()
        .map(|r| {
            cols.iter()
                .map(|c| match &r[*c] {
                    Value::Array(v) => json!(v
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")),
                    v => v.clone(),
                })
                .collect()
        })
        .collect();
    let art = table(sess, "Stored models", src("store.list", a), &cols, trows);
    Ok(json!({"rows": rows, "total": total, "location": store::open().location(), "artifact": art}))
}

fn delete(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().delete_model(s(a, "cid").unwrap(), b(a, "force").unwrap_or(false))
}

fn link(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().link(
        s(a, "subject").unwrap(),
        s(a, "predicate").unwrap(),
        s(a, "object").unwrap(),
    )
}

fn edges(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "id").unwrap();
    let list = store::open().edges(id, s(a, "predicate").filter(|p| !p.is_empty()))?;
    let cols = ["id", "subject", "predicate", "object", "axis", "at"];
    let rows = list
        .iter()
        .map(|e| cols.iter().map(|c| e[*c].clone()).collect())
        .collect();
    let art = table(
        sess,
        &format!("Edges of {id}"),
        src("store.edges", a),
        &cols,
        rows,
    );
    Ok(json!({"id": id, "edges": list, "artifact": art}))
}

fn neighbors(_: &mut Session, a: &Value) -> Result<Value> {
    Ok(json!(store::open().neighbors(
        s(a, "id").unwrap(),
        s(a, "predicate"),
        s(a, "direction"),
    )?))
}

fn query(sess: &mut Session, a: &Value) -> Result<Value> {
    let snap = store::open().snapshot()?;
    let ans = store::query::run(&snap, &a["pattern"])?;
    let step = src("store.query", a);
    let mut out = ans.json;
    if !ans.select.is_empty() {
        let mut cols: Vec<&str> = ans.select.iter().map(String::as_str).collect();
        cols.push("support");
        let rows_json = out
            .get("rows")
            .or_else(|| out.get("partialRows"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let rows = ans
            .rows
            .iter()
            .zip(&rows_json)
            .map(|(vals, rj)| {
                let mut r: Vec<Value> = vals.iter().map(|v| json!(v)).collect();
                let edges: Vec<String> = rj["support"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|s| s["edges"].as_array().cloned().unwrap_or_default())
                    .filter_map(|e| e.as_str().map(str::to_string))
                    .collect();
                r.push(json!(edges.join(" ")));
                r
            })
            .collect();
        out["table"] = json!(table(sess, "Store query", step.clone(), &cols, rows));
    }
    // The subgraph the answer rests on.
    let mut ids: BTreeSet<String> = ans.rows.iter().flatten().cloned().collect();
    let mut gedges = vec![];
    for eid in &ans.used {
        if let Some(r) = snap.relations.get(eid) {
            ids.insert(r.subject.clone());
            ids.insert(node_id(r));
            gedges.push(edge_of(eid, r));
        }
    }
    let nodes = ids.iter().map(|id| node(&snap, id)).collect();
    out["graph"] = json!(sess.add_artifact(
        "Store query graph".into(),
        step,
        ArtifactData::Graph {
            nodes,
            edges: gedges
        },
    ));
    Ok(out)
}

fn collection_create(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().create_collection(s(a, "name").unwrap())
}

fn collection_add(_: &mut Session, a: &Value) -> Result<Value> {
    let st = store::open();
    let col = st.resolve_collection(s(a, "collection").unwrap())?;
    st.link(&col, predicate::HAS_MEMBER, s(a, "cid").unwrap())
}

fn collection_list(sess: &mut Session, a: &Value) -> Result<Value> {
    let snap = store::open().snapshot()?;
    let rows: Vec<Value> = snap
        .collections
        .iter()
        .map(|(id, c)| {
            let members: Vec<&String> = snap
                .relations
                .values()
                .filter(|r| r.predicate == predicate::HAS_MEMBER && &r.subject == id)
                .map(|r| &r.object)
                .collect();
            json!({"id": id, "name": c.name, "members": members.len(), "member_ids": members})
        })
        .collect();
    let trows = rows
        .iter()
        .map(|r| vec![r["id"].clone(), r["name"].clone(), r["members"].clone()])
        .collect();
    let art = table(
        sess,
        "Collections",
        src("store.collection_list", a),
        &["id", "name", "members"],
        trows,
    );
    Ok(json!({"collections": rows, "artifact": art}))
}

fn license(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().set_license(s(a, "cid").unwrap(), s(a, "license").unwrap())
}

fn supersede(_: &mut Session, a: &Value) -> Result<Value> {
    let (old, new) = (s(a, "old").unwrap(), s(a, "new").unwrap());
    let mut out = store::open().supersede(old, new)?;
    // A net replaced by a composition (or content by a net) is allowed, since the chain is about
    // what replaces what; say it, because the usual case is a document of the same kind.
    let kind = |cid: &str| {
        store::open()
            .get_model(cid)
            .ok()
            .map(|v| store::kind_of(&v).name())
    };
    if let (Some(a), Some(b)) = (kind(old), kind(new)) {
        if a != b {
            add_note(
                &mut out,
                Some(format!(
                    "the superseded document is a {a} and its successor a {b}"
                )),
            );
        }
    }
    Ok(out)
}

fn migration_offers(sess: &mut Session, a: &Value) -> Result<Value> {
    let cid = s(a, "cid").unwrap();
    let mut out =
        store::open().migration_offers(cid, s(a, "predicate").filter(|p| !p.is_empty()))?;
    let cols = [
        "id",
        "subject",
        "predicate",
        "object",
        "disposition",
        "status",
        "check",
        "reason",
    ];
    let rows = out["edges"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| {
            cols.iter()
                .map(|c| match *c {
                    "check" => e["check"]["result"].clone(),
                    c => e[c].clone(),
                })
                .collect()
        })
        .collect();
    out["artifact"] = json!(table(
        sess,
        &format!("Migration offers for {cid}"),
        src("store.migration_offers", a),
        &cols,
        rows
    ));
    Ok(out)
}

fn accept_migration(_: &mut Session, a: &Value) -> Result<Value> {
    store::open().accept_migration(s(a, "old").unwrap(), s(a, "new"), strs(a, "edges"))
}

fn map_put(_: &mut Session, a: &Value) -> Result<Value> {
    let data = match &a["data"] {
        Value::String(t) => serde_json::from_str::<Value>(t)
            .map_err(|e| Error::BadArgs(format!("data is not a JSON object: {e}")))?,
        v => v.clone(),
    };
    store::open().put_map(&data)
}

fn map_get(_: &mut Session, a: &Value) -> Result<Value> {
    let id = s(a, "id").unwrap();
    Ok(json!({"id": id, "Data": store::open().get_map(id)?}))
}

fn map_list(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = store::open();
    let mut rows = vec![];
    for id in st.list_maps()? {
        let keys = st.get_map(&id)?.as_object().map_or(0, |m| m.len());
        rows.push(json!({"id": id, "keys": keys}));
    }
    let trows = rows
        .iter()
        .map(|r| vec![r["id"].clone(), r["keys"].clone()])
        .collect();
    let art = table(
        sess,
        "Stored tables",
        src("store.map_list", a),
        &["id", "keys"],
        trows,
    );
    Ok(json!({"maps": rows, "artifact": art}))
}

/// The graph node an edge's object is: the entity, or a `type:<Kind>`
/// literal node for `@type`.
fn node_id(r: &store::Relation) -> String {
    if r.predicate == predicate::TYPE {
        format!("type:{}", r.object)
    } else {
        r.object.clone()
    }
}

fn edge_of(eid: &str, r: &store::Relation) -> GraphEdge {
    GraphEdge {
        source: r.subject.clone(),
        target: node_id(r),
        label: r.predicate.clone(),
        data: json!({"id": eid, "axis": predicate::axis_of(&r.predicate)}),
    }
}

fn node(snap: &Snapshot, id: &str) -> GraphNode {
    if let Some(kind) = id.strip_prefix("type:") {
        return GraphNode {
            id: id.into(),
            label: kind.into(),
            data: json!({"kind": "type"}),
        };
    }
    let kind = snap.kind_of(id).map_or("missing", |k| k.name());
    let mut data = json!({"kind": kind});
    if let Some(m) = snap.models.get(id) {
        data["license"] = json!(m.license);
        data["superseded_by"] = json!(m.superseded_by);
        data["places"] = json!(m.places);
        data["transitions"] = json!(m.transitions);
    }
    GraphNode {
        id: id.into(),
        label: label(snap, id),
        data,
    }
}

fn graph(sess: &mut Session, a: &Value) -> Result<Value> {
    let st = store::open();
    let snap = st.snapshot()?;
    // Every edge between entities (migratedFrom relates two edges, so it
    // is left out), plus provenance: child → parent.
    let mut all: Vec<GraphEdge> = snap
        .relations
        .iter()
        .filter(|(_, r)| r.predicate != predicate::MIGRATED_FROM)
        .map(|(id, r)| edge_of(id, r))
        .collect();
    for (cid, m) in &snap.models {
        for p in &m.parents {
            all.push(GraphEdge {
                source: cid.clone(),
                target: p.clone(),
                label: "parent".into(),
                data: json!({"lineage": true}),
            });
        }
    }
    let mut keep: BTreeSet<String> = BTreeSet::new();
    let edges: Vec<GraphEdge> = match s(a, "around").filter(|x| !x.is_empty()) {
        None => {
            keep.extend(snap.models.keys().cloned());
            keep.extend(snap.collections.keys().cloned());
            for e in &all {
                keep.insert(e.source.clone());
                keep.insert(e.target.clone());
            }
            all
        }
        Some(around) => {
            let start = if snap.kind_of(around).is_some() {
                around.to_string()
            } else {
                st.resolve_collection(around).map_err(|_| {
                    Error::NotFound(format!(
                        "{around:?} is not a stored entity id or a collection name"
                    ))
                })?
            };
            let depth = n(a, "depth").map_or(2, |d| d.max(0.0) as usize);
            let mut adj: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
            for e in &all {
                adj.entry(&e.source).or_default().push(&e.target);
                adj.entry(&e.target).or_default().push(&e.source);
            }
            let mut dist: BTreeMap<String, usize> = [(start.clone(), 0)].into();
            let mut queue: VecDeque<String> = [start].into();
            while let Some(cur) = queue.pop_front() {
                let d = dist[&cur];
                if d == depth {
                    continue;
                }
                for nb in adj.get(cur.as_str()).into_iter().flatten() {
                    if !dist.contains_key(*nb) {
                        dist.insert(nb.to_string(), d + 1);
                        queue.push_back(nb.to_string());
                    }
                }
            }
            keep.extend(dist.into_keys());
            all.into_iter()
                .filter(|e| keep.contains(&e.source) && keep.contains(&e.target))
                .collect()
        }
    };
    let nodes: Vec<GraphNode> = keep.iter().map(|id| node(&snap, id)).collect();
    let summary = json!({"nodes": nodes.len(), "edges": edges.len()});
    let art = sess.add_artifact(
        "Knowledge graph".into(),
        src("store.graph", a),
        ArtifactData::Graph { nodes, edges },
    );
    let mut out = summary;
    out["artifact"] = json!(art);
    Ok(out)
}

fn info(_: &mut Session, _: &Value) -> Result<Value> {
    let st = store::open();
    let snap = st.snapshot()?;
    Ok(json!({
        "location": st.location(),
        "models": snap.models.len(),
        "relations": snap.relations.len(),
        "collections": snap.collections.len(),
        "maps": st.list_maps()?.len(),
        "licenses": store::LICENSES,
        "model_kinds": predicate::MODEL_KINDS,
        "predicates": predicate::REGISTRY.iter().map(|p| p.to_json()).collect::<Vec<_>>(),
    }))
}

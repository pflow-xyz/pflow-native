//! The local, persistent, content-addressed store: sim.pflow.xyz's
//! `pkg/modelstore` semantics, as far as they make sense on one machine
//! with one user.
//!
//! - **Models** are addressed by their pflow.xyz CID ([`crate::cid::seal`]),
//!   so a model stored here has the id pflow.xyz gives it. Storing the same
//!   net twice is the same model. Metadata (title, creation time, parents,
//!   license, successor) lives beside the model in an index, never inside
//!   it, for sim's reason: an envelope must not change the content's id.
//! - **Relations** are typed edges between stored entities, under sim's
//!   predicate registry ([`predicate`]), checked as `sim_link` checks them.
//! - **Collections** are named groupings whose membership is `hasMember`
//!   edges; **maps** are content-addressed key→value tables. Both get
//!   sim's ids (sha256 of Go's canonical JSON, 24 hex; [`canon`]).
//! - **Versioning** mirrors sim: `supersede` records the successor and a
//!   `supersededBy` edge; migration offers list what happens to each edge
//!   of the old model; accepting an offer writes the edge on the current
//!   successor plus a `migratedFrom` edge back to the old one (the old edge
//!   stays, as on sim).
//!
//! # One user
//!
//! sim checks who may write what (owners, write rules, attribution). Here
//! the person at the keyboard owns every entity, so every ownership check
//! passes and edges carry no `attributedTo`. Licenses stay irrevocable and
//! a licensed model cannot be deleted, as on sim.
//!
//! # Layout
//!
//! Through a [`StoreBackend`] (files under `<data_dir>/store/` natively,
//! memory on the web, which the page saves and restores whole through
//! `session.dump_state` / `session.restore_state`):
//!
//! | Key | What |
//! |---|---|
//! | `models/<cid>.jsonld` | the document, `@id` set to its CID; a net, a bundle, a composition or a content document ([`DocKind`]) |
//! | `index.json` | `{"models": {cid: {title, created, kind?, about?, parents, license, superseded_by, places, transitions}}}` |
//! | `edges.json` | `{"relations": {id: {Subject, Predicate, Object, At}}}` (sim's Relation JSON) |
//! | `collections.json` | `{"collections": {id: {Name}}}` |
//! | `maps/<id>.json` | `{"Data": {...}}`, the canonical bytes the id is computed over |
//!
//! A document that is not a net says so in its index entry (`kind`: `bundle`,
//! `composition` or `content`); a net's entry has no `kind`, so it is byte for
//! byte what it was before the other kinds were stored. A content document's
//! `about` is the CID of the composition it describes. Every kind is a model
//! to the rest of the store (edges, collections, licenses, versions).
//!
//! [`LAYOUT_PREFIXES`] lists the directories of this table and
//! [`LayoutFile::of`] its keys. A new key goes in both, or
//! [`Store::dump_blobs`] leaves it out of a saved state and
//! [`Store::replace_all`] refuses it.
//!
//! # Why a module-level handle
//!
//! The store is per-user state shared by every session in the process (the
//! app's document, its control channel, an MCP session) and outlives any
//! one document; it is not part of undo. So the commands reach it through
//! [`open`], a process-wide handle created lazily from
//! [`crate::outbound::data_dir`], rather than through a `Session` field.
//! [`set_thread_store`] overrides it per thread (tests, embedders). Moving
//! the handle into `Session` later only changes [`open`]'s callers.

pub mod backend;
pub mod canon;
pub mod kind;
pub mod predicate;
pub mod query;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub use backend::{FsBackend, MemoryBackend, StoreBackend};
pub use kind::{kind_of, require_kind, DocKind};
use predicate::{Fate, Kind, Range};

use crate::doc::Document;
use crate::{Error, Result};

/// Metadata kept beside a stored model (sim's lineage + ownership
/// envelopes, minus the owner).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ModelMeta {
    #[serde(default)]
    pub title: String,
    /// Milliseconds since the epoch when first stored (0 on the web build).
    #[serde(default)]
    pub created: u64,
    /// `bundle`, `composition` or `content`; absent for a net.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// A content document's composition: the CID it describes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub about: Option<String>,
    /// The models this one was derived from (provenance), nearest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parents: Vec<String>,
    /// A commons dedication; set once, irrevocable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// The model that replaces this one (`store.supersede`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(default)]
    pub places: usize,
    #[serde(default)]
    pub transitions: usize,
}

/// A stored edge, in sim's `Relation` JSON shape (Go field names), so its
/// id is computed over the same bytes sim would hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Relation {
    #[serde(rename = "Subject")]
    pub subject: String,
    #[serde(rename = "Predicate")]
    pub predicate: String,
    #[serde(rename = "Object")]
    pub object: String,
    /// RFC 3339; Go's zero time for tool-recorded edges.
    #[serde(rename = "At")]
    pub at: String,
    #[serde(
        rename = "attributedTo",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub attributed_to: Option<String>,
}

impl Relation {
    pub fn new(subject: &str, predicate: &str, object: &str, at: String) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            at,
            attributed_to: None,
        }
    }

    /// The relation's content id.
    pub fn id(&self) -> String {
        canon::content_id(&serde_json::to_value(self).expect("plain strings"))
    }

    /// The edge as `store.edges` reports it.
    pub fn to_json(&self, id: &str) -> Value {
        json!({
            "id": id,
            "subject": self.subject,
            "predicate": self.predicate,
            "object": self.object,
            "at": self.at,
            "axis": predicate::axis_of(&self.predicate),
        })
    }

    fn touches(&self, id: &str) -> bool {
        self.subject == id || self.object == id
    }

    /// Same triple, either orientation for a symmetric predicate.
    fn same_triple(&self, s: &str, p: &str, o: &str) -> bool {
        self.predicate == p
            && ((self.subject == s && self.object == o)
                || (predicate::is_symmetric(p) && self.subject == o && self.object == s))
    }
}

/// A named grouping (sim's `Collection` with no owner: membership is
/// `hasMember` edges, so the id never changes when something joins).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Collection {
    #[serde(rename = "Name")]
    pub name: String,
}

impl Collection {
    pub fn id(&self) -> String {
        canon::content_id(&serde_json::to_value(self).expect("a string"))
    }
}

/// sim's commons-license allowlist.
pub const LICENSES: &[&str] = &["CC0-1.0", "CC-BY-4.0", "CC-BY-SA-4.0"];
/// sim's bound on a stored map or collection.
pub const MAX_ENTITY_BYTES: usize = 256 * 1024;
/// sim's supersession-chain bound (hops).
pub const MAX_CHAIN: usize = 64;
/// sim's bound on one accept.
pub const MAX_ACCEPTS: usize = 25;

const INDEX: &str = "index.json";
const EDGES: &str = "edges.json";
const COLLECTIONS: &str = "collections.json";

/// Where the layout's keys live: the root files, then each directory.
/// Backends list one directory level (the file system's `list` is not
/// recursive), so a whole-store dump lists each of these.
pub const LAYOUT_PREFIXES: &[&str] = &["", "models/", "maps/"];

/// A file of the layout (the table in the module doc), by its key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutFile<'a> {
    Index,
    Edges,
    Collections,
    /// `models/<cid>.jsonld`.
    Model(&'a str),
    /// `maps/<id>.json`.
    Map(&'a str),
}

impl<'a> LayoutFile<'a> {
    /// The layout file `key` names, if any. Anything else in the store's
    /// directory (a file someone put there) is not part of the store.
    fn of(key: &'a str) -> Option<Self> {
        let named = |dir: &str, ext: &str| key.strip_prefix(dir)?.strip_suffix(ext);
        match key {
            INDEX => Some(Self::Index),
            EDGES => Some(Self::Edges),
            COLLECTIONS => Some(Self::Collections),
            _ => named("models/", ".jsonld")
                .filter(|c| canon::is_model_cid(c))
                .map(Self::Model)
                .or_else(|| {
                    named("maps/", ".json")
                        .filter(|i| canon::is_sha24(i))
                        .map(Self::Map)
                }),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct IndexFile {
    #[serde(default)]
    models: BTreeMap<String, ModelMeta>,
}

#[derive(Default, Serialize, Deserialize)]
struct EdgesFile {
    #[serde(default)]
    relations: BTreeMap<String, Relation>,
}

#[derive(Default, Serialize, Deserialize)]
struct CollectionsFile {
    #[serde(default)]
    collections: BTreeMap<String, Collection>,
}

fn model_key(cid: &str) -> String {
    format!("models/{cid}.jsonld")
}

fn map_key(id: &str) -> String {
    format!("maps/{id}.json")
}

/// A refusal: the request was understood and the answer is no.
fn refuse(msg: impl Into<String>) -> Error {
    Error::Invalid(msg.into())
}

/// A content-addressed entry of file `key` must sit under its own id.
fn check_content_id(key: &str, what: &str, id: &str, content: &str) -> Result<()> {
    if id == content {
        return Ok(());
    }
    Err(refuse(format!(
        "store {key}: {what} {id} holds the content of {content}; an entry's id is its content's"
    )))
}

const ID_REMEDY: &str = "pass a model's CID (store.list, file.cid) or the 24-hex id a store.* command returned for a collection, map or relation";

/// Everything but the maps, read once per operation.
pub struct Snapshot {
    pub models: BTreeMap<String, ModelMeta>,
    pub relations: BTreeMap<String, Relation>,
    pub collections: BTreeMap<String, Collection>,
    has_map: BTreeSet<String>,
}

impl Snapshot {
    /// Which kind of stored entity `id` is, if any.
    pub fn kind_of(&self, id: &str) -> Option<Kind> {
        if self.models.contains_key(id) {
            Some(Kind::Model)
        } else if self.collections.contains_key(id) {
            Some(Kind::Collection)
        } else if self.has_map.contains(id) {
            Some(Kind::Map)
        } else if self.relations.contains_key(id) {
            Some(Kind::Relation)
        } else {
            None
        }
    }

    /// Relations touching `id`, ordered by time then id (sim's order).
    pub fn edges_of(&self, id: &str) -> Vec<(&String, &Relation)> {
        let mut out: Vec<_> = self
            .relations
            .iter()
            .filter(|(_, r)| r.touches(id))
            .collect();
        sort_edges(&mut out);
        out
    }

    /// An existing relation with this triple (either orientation when
    /// symmetric), whatever its time; ties go to the smallest id.
    pub fn find(&self, s: &str, p: &str, o: &str) -> Option<String> {
        self.relations
            .iter()
            .find(|(_, r)| r.same_triple(s, p, o))
            .map(|(id, _)| id.clone())
    }

    /// sim's `CheckLink`: vocabulary, then both ends, in sim's order; the
    /// first failure is returned as a refusal.
    pub fn check_link(&self, subject: &str, pred: &str, object: &str) -> Result<()> {
        let linkable = predicate::linkable_names();
        if subject.is_empty() {
            return Err(refuse(
                "subject is empty; pass the id of the stored entity the edge starts from",
            ));
        }
        if pred.is_empty() {
            return Err(refuse(format!(
                "predicate is empty; store.link writes one of {linkable}"
            )));
        }
        if object.is_empty() {
            return Err(refuse("object is empty; pass the id of the stored entity the edge points to (for @type, a model kind)"));
        }
        let p = predicate::lookup(pred).ok_or_else(|| {
            refuse(format!(
                "predicate {pred:?} is not registered; store.link writes one of {linkable} (store.info lists each one's subject and object kinds)"
            ))
        })?;
        if let Some(stored) = p.stored_as {
            return Err(refuse(format!(
                "predicate {:?} is the inverse of {stored:?} and is never stored; link it the other way round: store.link {{subject: <object>, predicate: {stored:?}, object: <subject>}}",
                p.name
            )));
        }
        if p.axis == predicate::Axis::Structural {
            return Err(refuse(format!(
                "predicate {:?} is structural: the edge exists only with a witness, so store.link cannot assert it; a prover (sim_prove) records it together with the witness it finds",
                p.name
            )));
        }
        if !p.writable_by(predicate::WRITER_LINK) {
            return Err(tool_written_refusal(p));
        }
        self.check_end("subject", subject, p.name, p.domain)?;
        match p.range {
            Range::Literal(_) => {
                if !predicate::MODEL_KINDS.contains(&object) {
                    return Err(refuse(format!(
                        "{:?} takes a type name from https://sim.pflow.xyz/ns/models/v1; {object:?} is not one ({})",
                        p.name,
                        predicate::MODEL_KINDS.join(", ")
                    )));
                }
            }
            Range::Kinds(kinds) => self.check_end("object", object, p.name, kinds)?,
        }
        if subject == object {
            return Err(refuse(format!(
                "subject and object are both {subject}; {:?} relates two different entities",
                p.name
            )));
        }
        Ok(())
    }

    fn check_end(&self, end: &str, id: &str, pred: &str, want: &[Kind]) -> Result<()> {
        if !canon::is_model_cid(id) && !canon::is_sha24(id) {
            return Err(refuse(format!(
                "{end} {id:?} is not a content id; {ID_REMEDY}"
            )));
        }
        match self.kind_of(id) {
            Some(k) if want.contains(&k) => Ok(()),
            None => Err(refuse(format!(
                "{end} {id} is not a stored entity: no model, collection, map or relation in the local store has that id (store.put stores a model, store.collection_create a collection)"
            ))),
            Some(k) => {
                let mut msg = format!(
                    "{end} {id} is {} {}; {pred:?} needs {} as its {end}",
                    predicate::article(k.name()),
                    k.name(),
                    predicate::kind_list(want)
                );
                if want == [Kind::Collection] {
                    msg.push_str(" (store.collection_create makes one)");
                }
                Err(refuse(msg))
            }
        }
    }

    /// The supersession chain from `cid` to its current successor.
    pub fn chain(&self, cid: &str) -> Chain {
        let mut ch = Chain {
            model: cid.into(),
            successor: None,
            current: None,
            chain: vec![cid.into()],
            status: "not-superseded",
        };
        let Some(first) = self.models.get(cid).and_then(|m| m.superseded_by.clone()) else {
            return ch;
        };
        ch.successor = Some(first.clone());
        let mut seen: BTreeSet<String> = [cid.to_string()].into();
        let mut next = first;
        loop {
            if seen.contains(&next) {
                ch.chain.push(next);
                ch.status = "cycle";
                return ch;
            }
            if ch.chain.len() > MAX_CHAIN {
                ch.status = "truncated";
                return ch;
            }
            ch.chain.push(next.clone());
            seen.insert(next.clone());
            match self.models.get(&next) {
                None => {
                    ch.status = "successor-deleted";
                    return ch;
                }
                Some(m) => match &m.superseded_by {
                    Some(n) => next = n.clone(),
                    None => {
                        ch.current = Some(next);
                        ch.status = "complete";
                        return ch;
                    }
                },
            }
        }
    }

    /// Collections with a `hasMember` edge to `id`.
    pub fn collections_of(&self, id: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .relations
            .values()
            .filter(|r| r.predicate == predicate::HAS_MEMBER && r.object == id)
            .filter_map(|r| self.collections.get(&r.subject).map(|c| c.name.clone()))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Every ancestor of `cid` through recorded parents (cycle-safe).
    fn ancestry(&self, cid: &str) -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut queue: VecDeque<String> = [cid.to_string()].into();
        while let Some(cur) = queue.pop_front() {
            for p in self
                .models
                .get(&cur)
                .map(|m| m.parents.as_slice())
                .unwrap_or(&[])
            {
                if seen.insert(p.clone()) {
                    queue.push_back(p.clone());
                }
            }
        }
        seen
    }
}

pub(crate) fn sort_edges(edges: &mut [(&String, &Relation)]) {
    edges.sort_by(|a, b| (canon::time_key(&a.1.at), a.0).cmp(&(canon::time_key(&b.1.at), b.0)));
}

fn tool_written_refusal(p: &predicate::Predicate) -> Error {
    refuse(match p.name {
        "from" | "to" | "feeds" => format!(
            "predicate {:?} is recorded only by sim_bind, together with the binding it describes; the local store keeps no bindings (system.bind binds library models)",
            p.name
        ),
        "produced" => format!(
            "predicate {:?} is recorded only by sim_prompt and sim_reroll, for the result a prompt actually returned; store.link cannot assert it",
            p.name
        ),
        predicate::SUPERSEDED_BY => format!(
            "predicate {:?} is recorded only by store.supersede (sim_supersede_model); call store.supersede {{old, new}} instead",
            p.name
        ),
        predicate::MIGRATED_FROM => format!(
            "predicate {:?} is recorded only by store.accept_migration, which checks that the edge it carries is offered; call store.migration_offers and store.accept_migration instead",
            p.name
        ),
        _ => format!(
            "predicate {:?} is recorded only by {}; store.link cannot assert it",
            p.name,
            p.written_by.join(" and ")
        ),
    })
}

/// A model's supersession chain (sim's `SupersessionChain`).
#[derive(Debug, Clone, Serialize)]
pub struct Chain {
    pub model: String,
    pub successor: Option<String>,
    /// The terminal successor, only when `status` is "complete".
    pub current: Option<String>,
    pub chain: Vec<String>,
    /// complete | not-superseded | cycle | truncated | successor-deleted
    pub status: &'static str,
}

impl Chain {
    fn note(&self) -> String {
        let last = self.chain.last().cloned().unwrap_or_default();
        match self.status {
            "cycle" => format!(
                "the supersession chain from {} cycles ({}), so it has no current successor and nothing can be migrated; store.supersede one of these onto a model outside the cycle",
                self.model,
                self.chain.join(" → ")
            ),
            "truncated" => format!(
                "the supersession chain from {} is longer than {MAX_CHAIN} hops (stopped at {last}); store.supersede {{old: {}, new: <latest>}} points it straight at the current version",
                self.model, self.model
            ),
            "successor-deleted" => format!(
                "the current successor of {}, {last}, is no longer stored, so nothing can be migrated to it; store.supersede it onto a stored model",
                self.model
            ),
            _ => String::new(),
        }
    }
}

/// The store: a backend plus a lock that serializes this process's writes.
/// Every operation reads what it needs from the backend, so two handles on
/// one directory see each other's writes (last writer wins between
/// processes).
pub struct Store {
    backend: Box<dyn StoreBackend>,
    write: Mutex<()>,
    /// The documents of the kinds [`Store::get_model`] checks that sealed to their name, by CID,
    /// with the digest of the file bytes that did: a second read of the same bytes is not sealed
    /// again. Per handle (this process's), never persisted, and keyed by the bytes, so a file
    /// changed since is read, and sealed, afresh.
    sealed: Mutex<BTreeMap<String, [u8; 32]>>,
}

/// Most entries `Store::sealed` keeps; past it the memo starts over (a read then seals once more).
const MAX_SEALED_MEMO: usize = 4096;

impl Store {
    pub fn new(backend: impl StoreBackend + 'static) -> Self {
        Self {
            backend: Box::new(backend),
            write: Mutex::new(()),
            sealed: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn memory() -> Self {
        Self::new(MemoryBackend::new())
    }

    pub fn filesystem(root: impl Into<PathBuf>) -> Self {
        Self::new(FsBackend::new(root))
    }

    pub fn location(&self) -> String {
        self.backend.location()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.write.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn load<T: DeserializeOwned + Default>(&self, key: &str) -> Result<T> {
        match self.backend.get(key)? {
            None => Ok(T::default()),
            Some(b) => {
                serde_json::from_slice(&b).map_err(|e| Error::Parse(format!("store {key}: {e}")))
            }
        }
    }

    fn save<T: Serialize>(&self, key: &str, v: &T) -> Result<()> {
        let mut text = serde_json::to_string_pretty(v).expect("serializable");
        text.push('\n');
        self.backend.put(key, text.as_bytes())
    }

    fn save_models(&self, models: &BTreeMap<String, ModelMeta>) -> Result<()> {
        self.save(INDEX, &json!({ "models": models }))
    }

    fn save_relations(&self, relations: &BTreeMap<String, Relation>) -> Result<()> {
        self.save(EDGES, &json!({ "relations": relations }))
    }

    /// Reads the index, the edges and the collections, and lists the maps.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let index: IndexFile = self.load(INDEX)?;
        let edges: EdgesFile = self.load(EDGES)?;
        let cols: CollectionsFile = self.load(COLLECTIONS)?;
        Ok(Snapshot {
            models: index.models,
            relations: edges.relations,
            collections: cols.collections,
            has_map: self.list_maps()?.into_iter().collect(),
        })
    }

    // ---- models ----

    /// Seals and stores a net ([`Store::put_document`] over the document as
    /// a save would write it). Idempotent by CID: a second put of the same
    /// net returns `existed: true` and keeps the first metadata (a missing
    /// title or parent list is filled in).
    pub fn put_model(
        &self,
        doc: &Document,
        title: Option<&str>,
        parents: Option<Vec<String>>,
    ) -> Result<Value> {
        self.put_document(
            &doc.to_value(),
            DocKind::Net,
            title,
            parents,
            None,
            (doc.places.len(), doc.transitions.len()),
        )
    }

    /// Seals and stores any document of a [`DocKind`] the store keeps, under
    /// its pflow CID (`@id` is added after sealing, which ignores it). The
    /// rules are `put_model`'s: idempotent by CID, the first metadata
    /// stands, parents must be model CIDs and may not close a cycle. `kind`
    /// must be what [`kind_of`] says of `doc` (`[DOCUMENT_KIND]` otherwise).
    ///
    /// The title defaults to the document's `name`. `counts` are the places
    /// and transitions the index shows (a net's own; a composition's flat
    /// net, or 0 and 0 when it does not flatten; 0 and 0 for content).
    /// `about` is a content document's composition: any model CID, with a
    /// `note` in the result when that CID is not a stored composition or
    /// bundle. The result is `put_model`'s, plus `kind` when the index
    /// records one and `about` for content.
    ///
    /// A file already stored under the CID that is not a document of that CID (a composition,
    /// bundle or content file altered in place, which [`Store::get_model`] refuses) is written
    /// over from the document put: the result is `existed: true, repaired: true`, with a note.
    /// A net's file is left as it is.
    pub fn put_document(
        &self,
        doc: &Value,
        kind: DocKind,
        title: Option<&str>,
        parents: Option<Vec<String>>,
        about: Option<&str>,
        counts: (usize, usize),
    ) -> Result<Value> {
        let found = require_kind(doc)?;
        if kind != found {
            return Err(crate::composition::document::refuse(
                "DOCUMENT_KIND",
                format!(
                    "stored as {}, but the document is {}",
                    kind.name(),
                    found.name()
                ),
            ));
        }
        if let Some(a) = about {
            if kind != DocKind::Content {
                return Err(Error::BadArgs(format!(
                    "about relates a content document to its composition; a {} has none",
                    kind.name()
                )));
            }
            if !canon::is_model_cid(a) {
                return Err(Error::BadArgs(format!(
                    "about {a:?} is not a model CID (z…, the composition's `cid`)"
                )));
            }
        }
        let _g = self.lock();
        let mut v = doc.clone();
        let cid = crate::cid::seal(&v)?;
        if let Value::Object(m) = &mut v {
            m.insert("@id".into(), json!(cid));
        }
        let snap = self.snapshot()?;
        let mut models = snap.models.clone();
        // Parents: as given, else a net's own `parents` field (the other
        // kinds have no such field).
        let mut parents = parents.unwrap_or_else(|| {
            if kind != DocKind::Net {
                return vec![];
            }
            v.get("parents")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|p| p.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default()
        });
        for p in &parents {
            if !canon::is_model_cid(p) {
                return Err(Error::BadArgs(format!(
                    "parent {p:?} is not a model CID (z…, as file.cid returns)"
                )));
            }
        }
        let mut notes = vec![];
        let mut dedup = BTreeSet::new();
        parents.retain(|p| {
            if p == &cid {
                notes.push(format!("{p} is this model itself; not recorded as its parent"));
                return false;
            }
            if snap.ancestry(p).contains(&cid) {
                notes.push(format!(
                    "{p} descends from this model; recording it as a parent would close a cycle, so it was not"
                ));
                return false;
            }
            dedup.insert(p.clone())
        });
        let key = model_key(&cid);
        let text = || {
            let mut text = serde_json::to_string_pretty(&v).expect("json");
            text.push('\n');
            text
        };
        let mut repaired = false;
        match self.backend.get(&key)? {
            None => {
                let text = text();
                self.backend.put(&key, text.as_bytes())?;
                // What was just written for a document that sealed to `cid` seals to it.
                if kind != DocKind::Net {
                    self.remember_sealed(&cid, Sha256::digest(&text).into());
                }
            }
            // A net is as it was stored. The other kinds are read by content
            // ([`Store::get_model`]), so a file there that does not seal to its name is a file
            // that cannot be served: the document being put is the proof of what the name stands
            // for, and it is written over the file. (Bytes equal to what would be written seal
            // by construction.)
            Some(have) if kind != DocKind::Net => {
                let text = text();
                if have == text.as_bytes() || self.file_seals(&cid, &have) {
                    self.remember_sealed(&cid, Sha256::digest(&have).into());
                } else {
                    self.backend.put(&key, text.as_bytes())?;
                    self.remember_sealed(&cid, Sha256::digest(&text).into());
                    repaired = true;
                    notes.push(format!(
                        "the stored file for {cid} did not seal to it (the store file had been altered); it was rewritten from this document"
                    ));
                }
            }
            Some(_) => {}
        }
        // Read from the snapshot above, so a composition this same command
        // stored a moment ago counts.
        let about_note = about.and_then(|a| match snap.models.get(a) {
            None => Some(format!(
                "{a} is not in the local store; store.put the composition this content is about"
            )),
            Some(m) => match DocKind::from_index(m.kind.as_deref()) {
                DocKind::Composition | DocKind::Bundle => None,
                other => Some(format!(
                    "{a} is stored as a {}, not a composition or bundle",
                    other.name()
                )),
            },
        });
        let existed = models.contains_key(&cid);
        let meta = models.entry(cid.clone()).or_insert_with(|| ModelMeta {
            created: canon::now_ms(),
            kind: kind.index_name().map(str::to_string),
            ..Default::default()
        });
        let before = meta.clone();
        if meta.title.is_empty() {
            meta.title = title
                .map(str::to_string)
                .or_else(|| v.get("name").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_default();
        }
        if meta.parents.is_empty() {
            meta.parents = parents;
        } else if !parents.is_empty() && meta.parents != parents {
            notes.push("this model's parents were recorded when it was first stored; the first record stands".into());
        }
        match (&meta.about, about) {
            (None, Some(a)) => meta.about = Some(a.to_string()),
            (Some(first), Some(a)) if first != a => notes.push(
                "this content's composition was recorded when it was first stored; the first record stands".into(),
            ),
            _ => {}
        }
        meta.places = counts.0;
        meta.transitions = counts.1;
        let meta = meta.clone();
        if !existed || meta != before {
            self.save_models(&models)?;
        }
        let mut out = json!({ "cid": cid });
        if let Some(k) = &meta.kind {
            out["kind"] = json!(k);
        }
        out["existed"] = json!(existed);
        if repaired {
            out["repaired"] = json!(true);
        }
        out["title"] = json!(meta.title);
        out["parents"] = json!(meta.parents);
        out["created"] = json!(meta.created);
        if let Some(a) = &meta.about {
            out["about"] = json!(a);
        }
        if !notes.is_empty() {
            out["notes"] = json!(notes);
        }
        if let Some(n) = about_note {
            out["note"] = json!(n);
        }
        Ok(out)
    }

    /// The stored document (with `@id` = its CID).
    ///
    /// A net is parsed and nothing more, as it always was: sealing one costs its size squared
    /// (connect-4's takes tens of seconds) and every net the editor opens is read here. A
    /// composition, a bundle or a content document is addressed by its content and read as that:
    /// it is sealed again and a file that seals to another CID is refused (`[STORE_ALTERED]`)
    /// rather than served as `cid` (to `bundle.load {cid}`, to a bundle's `cid` subnets, to
    /// `store.open`, to a receipt's replay). The seal is paid once per handle for the same bytes
    /// ([`Store::sealed`]); a document `put_document` wrote is known to seal already.
    pub fn get_model(&self, cid: &str) -> Result<Value> {
        let bytes = self.backend.get(&model_key(cid))?.ok_or_else(|| {
            Error::NotFound(format!("no model {cid} in the local store (store.list)"))
        })?;
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Parse(format!("stored model {cid}: {e}")))?;
        if kind_of(&v) != DocKind::Net {
            self.check_sealed(cid, &bytes, &v)?;
        }
        Ok(v)
    }

    /// `[STORE_ALTERED]` unless `v`, parsed from `bytes`, seals to `cid`; once per handle for the
    /// same bytes. A document too large to seal is refused as such, not as altered.
    fn check_sealed(&self, cid: &str, bytes: &[u8], v: &Value) -> Result<()> {
        let digest: [u8; 32] = Sha256::digest(bytes).into();
        if self
            .sealed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(cid)
            == Some(&digest)
        {
            return Ok(());
        }
        let altered = |what: String| {
            crate::composition::document::refuse(
                "STORE_ALTERED",
                format!(
                    "the stored document under {cid} is a different document ({what}); the store file was altered"
                ),
            )
        };
        match crate::cid::seal(v) {
            Ok(sealed) if sealed == cid => {}
            Ok(sealed) => return Err(altered(format!("it seals to {sealed}"))),
            Err(e) if crate::cid::is_too_complex(&e) => return Err(e),
            Err(e) => return Err(altered(format!("it does not seal: {e}"))),
        }
        self.remember_sealed(cid, digest);
        Ok(())
    }

    /// Whether `bytes` are a document that seals to `cid` ([`Store::check_sealed`], yes or no).
    fn file_seals(&self, cid: &str, bytes: &[u8]) -> bool {
        serde_json::from_slice::<Value>(bytes)
            .is_ok_and(|v| self.check_sealed(cid, bytes, &v).is_ok())
    }

    fn remember_sealed(&self, cid: &str, digest: [u8; 32]) {
        let mut memo = self.sealed.lock().unwrap_or_else(|e| e.into_inner());
        if memo.len() >= MAX_SEALED_MEMO && !memo.contains_key(cid) {
            memo.clear();
        }
        memo.insert(cid.to_string(), digest);
    }

    /// The stored document as it was sealed: [`Store::get_model`] without
    /// the `@id` the store added. A composition, bundle or content loader
    /// refuses a key it does not know, so those documents are opened from
    /// this, and an unedited raw Bundle round-trips to the bytes it was
    /// stored from.
    pub fn get_sealed(&self, cid: &str) -> Result<Value> {
        let mut v = self.get_model(cid)?;
        if let Value::Object(m) = &mut v {
            m.shift_remove("@id");
        }
        Ok(v)
    }

    pub fn model_meta(&self, cid: &str) -> Result<Option<ModelMeta>> {
        Ok(self.load::<IndexFile>(INDEX)?.models.remove(cid))
    }

    /// Removes a model. Refused for a licensed model (the dedication is
    /// irrevocable) and, unless `force`, for one any edge names. Edges are
    /// immutable records and are kept either way; re-storing the same net
    /// revives them.
    pub fn delete_model(&self, cid: &str, force: bool) -> Result<Value> {
        let _g = self.lock();
        let snap = self.snapshot()?;
        let meta = snap.models.get(cid).ok_or_else(|| {
            Error::NotFound(format!("no model {cid} in the local store (store.list)"))
        })?;
        if let Some(l) = &meta.license {
            return Err(refuse(format!(
                "model {cid} was dedicated to the commons under {l}; the dedication is irrevocable, so the model stays"
            )));
        }
        let edges = snap.edges_of(cid);
        if !edges.is_empty() && !force {
            let ids: Vec<&str> = edges.iter().take(5).map(|(id, _)| id.as_str()).collect();
            return Err(refuse(format!(
                "model {cid} is named by {} edge(s) ({}{}); pass force: true to delete it anyway (the edges stay, naming a model that is no longer stored)",
                edges.len(),
                ids.join(", "),
                if edges.len() > 5 { ", …" } else { "" }
            )));
        }
        let referenced_by: Vec<&String> = snap
            .models
            .iter()
            .filter(|(_, m)| m.superseded_by.as_deref() == Some(cid))
            .map(|(id, _)| id)
            .collect();
        let mut models = snap.models.clone();
        models.remove(cid);
        self.backend.delete(&model_key(cid))?;
        self.save_models(&models)?;
        Ok(json!({
            "deleted": cid,
            "dangling_edges": edges.len(),
            "successor_of": referenced_by,
        }))
    }

    // ---- relations ----

    /// `store.link`: [`Snapshot::check_link`], then reuse an existing edge
    /// with the same triple or store a new one stamped now.
    pub fn link(&self, subject: &str, pred: &str, object: &str) -> Result<Value> {
        let _g = self.lock();
        let snap = self.snapshot()?;
        snap.check_link(subject, pred, object)?;
        self.put_edge_locked(snap, subject, pred, object, canon::rfc3339(canon::now_ms()))
    }

    /// Records an edge as one of sim's tools would (`writer` must be in
    /// the predicate's `writtenBy`), with the zero time, so its id depends
    /// on its ends alone. Both ends must be stored entities of the
    /// predicate's kinds. This is the door for a local prover
    /// (`sim_prove`) to record structural edges.
    pub fn put_edge_as(
        &self,
        writer: &str,
        subject: &str,
        pred: &str,
        object: &str,
    ) -> Result<Value> {
        let _g = self.lock();
        let snap = self.snapshot()?;
        let p = predicate::lookup(pred)
            .filter(|p| p.stored_as.is_none())
            .ok_or_else(|| {
                refuse(format!(
                    "predicate {pred:?} is not a stored registered predicate"
                ))
            })?;
        if !p.writable_by(writer) {
            return Err(refuse(format!(
                "predicate {pred:?} is not written by {writer}"
            )));
        }
        snap.check_end("subject", subject, p.name, p.domain)?;
        if let Range::Kinds(k) = p.range {
            snap.check_end("object", object, p.name, k)?;
        }
        self.put_edge_locked(snap, subject, pred, object, canon::ZERO_TIME.into())
    }

    fn put_edge_locked(
        &self,
        snap: Snapshot,
        subject: &str,
        pred: &str,
        object: &str,
        at: String,
    ) -> Result<Value> {
        let axis = predicate::axis_of(pred);
        if let Some(id) = snap.find(subject, pred, object) {
            return Ok(
                json!({"id": id, "subject": subject, "predicate": pred, "object": object, "axis": axis, "existing": true}),
            );
        }
        let rel = Relation::new(subject, pred, object, at);
        let id = rel.id();
        let mut relations = snap.relations;
        relations.insert(id.clone(), rel);
        self.save_relations(&relations)?;
        Ok(
            json!({"id": id, "subject": subject, "predicate": pred, "object": object, "axis": axis, "existing": false}),
        )
    }

    /// Every relation touching `id`, optionally one predicate's.
    pub fn edges(&self, id: &str, pred: Option<&str>) -> Result<Vec<Value>> {
        let snap = self.snapshot()?;
        if let Some(p) = pred {
            check_filter_predicate(&snap, p, false)?;
        }
        Ok(snap
            .edges_of(id)
            .into_iter()
            .filter(|(_, r)| pred.is_none_or(|p| r.predicate == p))
            .map(|(rid, r)| r.to_json(rid))
            .collect())
    }

    /// One hop from `id`, read through the registry as sim_neighbors does:
    /// no predicate = the objects of every edge out of `id`; an inverse
    /// name reads its stored predicate backwards; a symmetric one counts
    /// both directions. `direction` ("out" default, "in", "both") turns
    /// the reading round or widens it. Ordered by time, then edge id; not
    /// de-duplicated.
    pub fn neighbors(
        &self,
        id: &str,
        pred: Option<&str>,
        direction: Option<&str>,
    ) -> Result<Vec<String>> {
        let snap = self.snapshot()?;
        let pred = pred.filter(|p| !p.is_empty());
        if let Some(p) = pred {
            check_filter_predicate(&snap, p, true)?;
        }
        let (stored, mut fwd, mut back) = match pred.and_then(predicate::lookup) {
            Some(p) if p.stored_as.is_some() => (p.stored_as, false, true),
            Some(p) if p.symmetric => (Some(p.name), true, true),
            Some(p) => (Some(p.name), true, false),
            None => (pred, true, false),
        };
        match direction.unwrap_or("out") {
            "out" => {}
            "in" => std::mem::swap(&mut fwd, &mut back),
            "both" => (fwd, back) = (true, true),
            d => {
                return Err(Error::BadArgs(format!(
                    "direction {d:?} is not one of out, in, both"
                )))
            }
        }
        let mut hits: Vec<(&String, &Relation, String)> = vec![];
        let mut counted = BTreeSet::new();
        for (rid, r) in &snap.relations {
            if stored.is_some_and(|p| r.predicate != p) {
                continue;
            }
            if fwd && r.subject == id {
                counted.insert(rid);
                hits.push((rid, r, r.object.clone()));
            }
            if back && r.object == id && !counted.contains(rid) {
                hits.push((rid, r, r.subject.clone()));
            }
        }
        hits.sort_by(|a, b| (canon::time_key(&a.1.at), a.0).cmp(&(canon::time_key(&b.1.at), b.0)));
        Ok(hits.into_iter().map(|(_, _, n)| n).collect())
    }

    // ---- collections ----

    /// Creates (or finds: the id is the name's) a collection.
    pub fn create_collection(&self, name: &str) -> Result<Value> {
        if name.is_empty() {
            return Err(Error::BadArgs(
                "name is empty; a collection is named, and the name is its content id".into(),
            ));
        }
        let _g = self.lock();
        let mut cols: CollectionsFile = self.load(COLLECTIONS)?;
        let c = Collection { name: name.into() };
        let bytes = canon::go_json(&serde_json::to_value(&c).expect("a string"));
        if bytes.len() > MAX_ENTITY_BYTES {
            return Err(Error::BadArgs(format!(
                "collection rejected: {} bytes exceeds the {MAX_ENTITY_BYTES} byte limit",
                bytes.len()
            )));
        }
        let id = c.id();
        let existed = cols.collections.contains_key(&id);
        if !existed {
            cols.collections.insert(id.clone(), c);
            self.save(COLLECTIONS, &cols)?;
        }
        Ok(json!({"id": id, "name": name, "existed": existed}))
    }

    /// A collection id from an id or a name.
    pub fn resolve_collection(&self, id_or_name: &str) -> Result<String> {
        let cols: CollectionsFile = self.load(COLLECTIONS)?;
        if cols.collections.contains_key(id_or_name) {
            return Ok(id_or_name.into());
        }
        let id = Collection {
            name: id_or_name.into(),
        }
        .id();
        if cols.collections.contains_key(&id) {
            return Ok(id);
        }
        Err(Error::NotFound(format!(
            "no collection {id_or_name:?} (by id or name); store.collection_create makes one"
        )))
    }

    // ---- licenses and versions ----

    /// Dedicates a model to the commons. Irrevocable: a second license,
    /// even the same one, is refused.
    pub fn set_license(&self, cid: &str, license: &str) -> Result<Value> {
        let _g = self.lock();
        let mut index: IndexFile = self.load(INDEX)?;
        let meta = index.models.get_mut(cid).ok_or_else(|| {
            Error::NotFound(format!("no model {cid} in the local store (store.list)"))
        })?;
        if let Some(l) = &meta.license {
            return Err(refuse(format!(
                "model {cid} is already dedicated under {l}, and a commons dedication is irrevocable"
            )));
        }
        if !LICENSES.contains(&license) {
            return Err(Error::BadArgs(format!(
                "license must be one of {}",
                LICENSES.join(", ")
            )));
        }
        meta.license = Some(license.into());
        self.save_models(&index.models)?;
        Ok(json!({"cid": cid, "license": license}))
    }

    /// Marks `old` as replaced by `new` and records the `supersededBy`
    /// edge (zero time, so a retry converges on one edge). Re-pointing is
    /// allowed; cycles are reported by the chain walk, not refused.
    pub fn supersede(&self, old: &str, new: &str) -> Result<Value> {
        if old == new {
            return Err(refuse("a model cannot supersede itself"));
        }
        let snap = self.snapshot()?;
        for (end, id) in [("model", old), ("successor", new)] {
            if !snap.models.contains_key(id) {
                return Err(Error::NotFound(format!(
                    "{end} {id} is not a stored model (store.put stores one)"
                )));
            }
        }
        {
            let _g = self.lock();
            let mut index: IndexFile = self.load(INDEX)?;
            if let Some(m) = index.models.get_mut(old) {
                m.superseded_by = Some(new.into());
            }
            self.save_models(&index.models)?;
        }
        let edge = self.put_edge_as(
            predicate::WRITER_SUPERSEDE,
            old,
            predicate::SUPERSEDED_BY,
            new,
        )?;
        let snap = self.snapshot()?;
        let (mut offered, mut stays) = (0, 0);
        for (_, r) in snap.edges_of(old) {
            if Fate::of(&r.predicate) == Fate::Offered {
                offered += 1;
            } else {
                stays += 1;
            }
        }
        Ok(json!({
            "old": old,
            "new": new,
            "edge": edge["id"],
            "chain": snap.chain(old),
            "edges": {"offered": offered, "stays": stays},
            "note": "the old model's edges stay where they are; store.migration_offers lists those that can be carried to the successor",
        }))
    }

    /// What supersession does to every edge touching `cid` (sim's
    /// `MigrationOffers`, unpaged): offered taxonomic edges with their
    /// proposed form on the current successor and the link check on it
    /// now; tool-written, structural and unregistered edges stay.
    pub fn migration_offers(&self, cid: &str, pred: Option<&str>) -> Result<Value> {
        let snap = self.snapshot()?;
        let ch = migration_chain(&snap, cid)?;
        if let Some(p) = pred {
            check_filter_predicate(&snap, p, false)?;
        }
        let complete = ch.status == "complete";
        let target = ch
            .current
            .clone()
            .unwrap_or_else(|| "the current successor".into());
        let mut rows = vec![];
        let (mut offered, mut stays) = (0, 0);
        for (eid, r) in snap.edges_of(cid) {
            if pred.is_some_and(|p| r.predicate != p) {
                continue;
            }
            let fate = Fate::of(&r.predicate);
            let mut row = r.to_json(eid);
            row["class"] = json!(fate.name());
            if fate != Fate::Offered {
                stays += 1;
                row["disposition"] = json!("stays");
                row["reason"] = json!(stay_reason(r, cid, &target));
                rows.push(row);
                continue;
            }
            offered += 1;
            row["disposition"] = json!("offered");
            let migrations = migrations_of(&snap, eid);
            row["migrations"] = json!(migrations);
            if !complete {
                row["status"] = json!("no-target");
                rows.push(row);
                continue;
            }
            let (ps, po) = (
                swap_end(&r.subject, cid, &target),
                swap_end(&r.object, cid, &target),
            );
            row["proposed"] = json!({"subject": ps, "predicate": r.predicate, "object": po});
            row["check"] = match snap.check_link(&ps, &r.predicate, &po) {
                Ok(()) => json!({"result": "passes"}),
                Err(e) => json!({"result": "refused", "message": e.to_string()}),
            };
            let accepted = migrations.iter().any(|m| {
                snap.relations
                    .get(m["migrated"].as_str().unwrap_or(""))
                    .is_some_and(|mr| mr.same_triple(&ps, &r.predicate, &po))
            });
            row["status"] = json!(if accepted { "accepted" } else { "pending" });
            rows.push(row);
        }
        let note = if complete {
            format!(
                "{cid}'s edges stay where they are; accepting an offer (store.accept_migration) adds the proposed edge on {target} with a migratedFrom edge pointing back"
            )
        } else {
            ch.note()
        };
        Ok(json!({
            "model": cid,
            "successor": ch.successor,
            "current": ch.current,
            "chain": ch.chain,
            "chainStatus": ch.status,
            "predicate": pred,
            "note": note,
            "edges": rows,
            "counts": {"offered": offered, "stays": stays, "total": offered + stays},
        }))
    }

    /// Carries offered edges of a superseded model forward to its current
    /// successor (sim's `AcceptMigration`): every edge is checked before
    /// anything is written, and one refusal refuses the call. Each accepted
    /// edge is written with the zero time (reusing an existing edge with
    /// the same triple), then a `migratedFrom` edge from it to the old
    /// edge, so a re-run converges on the same ids. `edges` defaults to
    /// every pending offer.
    pub fn accept_migration(
        &self,
        old: &str,
        successor: Option<&str>,
        edges: Option<Vec<String>>,
    ) -> Result<Value> {
        let snap = self.snapshot()?;
        let ch = migration_chain(&snap, old)?;
        if ch.status != "complete" {
            return Err(refuse(format!("{} Nothing was migrated.", ch.note())));
        }
        let target = ch.current.clone().expect("complete chain");
        if let Some(s) = successor.filter(|s| *s != target) {
            return Err(refuse(format!(
                "the current successor of {old} is {target} (chain {}), not {s}; accept against {target}. Nothing was migrated.",
                ch.chain.join(" → ")
            )));
        }
        let ids = match edges {
            Some(ids) => {
                if ids.is_empty() || ids.len() > MAX_ACCEPTS {
                    return Err(Error::BadArgs(format!(
                        "edges must list 1 to {MAX_ACCEPTS} relation ids from store.migration_offers; got {}. Nothing was migrated",
                        ids.len()
                    )));
                }
                for (i, id) in ids.iter().enumerate() {
                    if !canon::is_sha24(id) {
                        return Err(Error::BadArgs(format!(
                            "edges[{i}] is {id:?}, not a relation id (24 lowercase hex characters); nothing was migrated"
                        )));
                    }
                    if let Some(j) = ids[..i].iter().position(|x| x == id) {
                        return Err(Error::BadArgs(format!(
                            "edges[{i}] repeats edges[{j}] ({id}); list each edge once. Nothing was migrated"
                        )));
                    }
                }
                ids
            }
            None => {
                let offers = self.migration_offers(old, None)?;
                let pending: Vec<String> = offers["edges"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|r| r["status"] == "pending" && r["check"]["result"] == "passes")
                    .filter_map(|r| r["id"].as_str().map(str::to_string))
                    .collect();
                if pending.is_empty() {
                    return Err(refuse(format!(
                        "{old} has no pending offer that passes the link check; store.migration_offers lists them. Nothing was migrated"
                    )));
                }
                pending
            }
        };
        let mut refusals = vec![];
        let mut olds = vec![];
        for id in &ids {
            let Some(r) = snap.relations.get(id) else {
                refusals.push(format!(
                    "edge {id} is not a stored relation; pass an edge id store.migration_offers listed"
                ));
                continue;
            };
            if !r.touches(old) {
                refusals.push(format!(
                    "edge {id} does not touch {old}: it relates {} and {}",
                    r.subject, r.object
                ));
                continue;
            }
            if Fate::of(&r.predicate) != Fate::Offered {
                refusals.push(format!(
                    "edge {id} stays with {old} and is never migrated — {}",
                    stay_reason(r, old, &target)
                ));
                continue;
            }
            let (ps, po) = (
                swap_end(&r.subject, old, &target),
                swap_end(&r.object, old, &target),
            );
            if let Err(e) = snap.check_link(&ps, &r.predicate, &po) {
                refusals.push(format!(
                    "edge {id} cannot be migrated: the edge it would become ({ps} {} {po}) is refused by the same check store.link applies: {e}",
                    r.predicate
                ));
                continue;
            }
            olds.push((id.clone(), ps, r.predicate.clone(), po));
        }
        if !refusals.is_empty() {
            return Err(refuse(format!(
                "nothing was migrated: {} of the {} edges you listed cannot be accepted — {}",
                refusals.len(),
                ids.len(),
                refusals.join("; ")
            )));
        }
        let _g = self.lock();
        let mut relations = self.snapshot()?.relations;
        let mut migrated = vec![];
        for (id, ps, p, po) in olds {
            let found = relations
                .iter()
                .find(|(_, r)| r.same_triple(&ps, &p, &po))
                .map(|(k, _)| k.clone());
            let existing = found.is_some();
            let mid = found.unwrap_or_else(|| {
                let rel = Relation::new(&ps, &p, &po, canon::ZERO_TIME.into());
                let mid = rel.id();
                relations.insert(mid.clone(), rel);
                mid
            });
            let prov = Relation::new(&mid, predicate::MIGRATED_FROM, &id, canon::ZERO_TIME.into());
            let pid = prov.id();
            relations.entry(pid.clone()).or_insert(prov);
            migrated.push(
                json!({"edge": id, "migrated": mid, "provenance": pid, "existing": existing}),
            );
        }
        self.save_relations(&relations)?;
        Ok(json!({"model": old, "successor": target, "chain": ch.chain, "migrated": migrated}))
    }

    // ---- maps ----

    /// Stores a key→value table; the id is sim's (`{"Data": data}`'s
    /// canonical bytes), so key order does not matter.
    pub fn put_map(&self, data: &Value) -> Result<Value> {
        if !data.is_object() {
            return Err(Error::BadArgs(
                "data must be a JSON object (key → value)".into(),
            ));
        }
        let bytes = canon::go_json(&json!({ "Data": data }));
        if bytes.len() > MAX_ENTITY_BYTES {
            return Err(Error::BadArgs(format!(
                "map rejected: {} bytes exceeds the {MAX_ENTITY_BYTES} byte limit",
                bytes.len()
            )));
        }
        let id = canon::sha24(bytes.as_bytes());
        let _g = self.lock();
        let existed = self.backend.get(&map_key(&id))?.is_some();
        if !existed {
            self.backend.put(&map_key(&id), bytes.as_bytes())?;
        }
        Ok(json!({"id": id, "existed": existed, "keys": data.as_object().map_or(0, |m| m.len())}))
    }

    /// The table stored under `id` (its `Data`).
    pub fn get_map(&self, id: &str) -> Result<Value> {
        let bytes = self
            .backend
            .get(&map_key(id))?
            .ok_or_else(|| Error::NotFound(format!("no map {id:?} (store.map_list)")))?;
        let v: Value = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Parse(format!("stored map {id}: {e}")))?;
        Ok(v.get("Data").cloned().unwrap_or(Value::Null))
    }

    pub fn list_maps(&self) -> Result<Vec<String>> {
        Ok(self
            .backend
            .list("maps/")?
            .into_iter()
            .filter_map(|k| {
                k.strip_prefix("maps/")
                    .and_then(|n| n.strip_suffix(".json"))
                    .map(str::to_string)
            })
            .collect())
    }

    // ---- the whole store, as text ----

    /// Whether the store outlives the process (files on disk).
    pub fn persistent(&self) -> bool {
        self.backend.persistent()
    }

    /// Every file of the layout, by key, as text: what `session.dump_state`
    /// saves for a page that has no other place to keep the store. Taken
    /// under the write lock, so it is one state of this process's writes.
    pub fn dump_blobs(&self) -> Result<BTreeMap<String, String>> {
        let _g = self.lock();
        let mut out = BTreeMap::new();
        for prefix in LAYOUT_PREFIXES {
            for key in self.backend.list(prefix)? {
                // A memory backend lists every key under "".
                if LayoutFile::of(&key).is_none() || out.contains_key(&key) {
                    continue;
                }
                let Some(bytes) = self.backend.get(&key)? else {
                    continue; // deleted by another process since the listing
                };
                let text = String::from_utf8(bytes).map_err(|_| {
                    refuse(format!(
                        "store file {key} (in {}) is not UTF-8 text, and a saved state carries the store as text; remove or re-create it",
                        self.location()
                    ))
                })?;
                out.insert(key, text);
            }
        }
        Ok(out)
    }

    /// What [`Store::replace_all`] checks before it changes anything: this
    /// store is a memory one, every key is a file of the layout, and every
    /// file holds what its key says, as the store writes it. A model file
    /// is a document of a kind the store keeps ([`DocKind`]) whose CID
    /// ([`crate::cid::seal`]) is the one in its key, a map's content id is
    /// its key's, the index, edges and collections parse as their own
    /// shape with each entry under its content id, and every model the
    /// index lists has its file, of the kind the index gives it. So a
    /// damaged or edited state is refused rather than serving one net under
    /// another's CID. Edges and lineage may name entities that are gone:
    /// `delete_model {force}` leaves them so.
    fn check_blobs(&self, blobs: &BTreeMap<String, String>) -> Result<()> {
        if self.persistent() {
            return Err(refuse(format!(
                "the store at {} is on disk; a saved state replaces only a memory store (the browser's), so it never overwrites a store on disk",
                self.location()
            )));
        }
        let mut model_files: BTreeMap<&str, DocKind> = BTreeMap::new();
        let mut index = IndexFile::default();
        for (key, text) in blobs {
            backend::check_key(key)?;
            let file = LayoutFile::of(key).ok_or_else(|| {
                Error::BadArgs(format!(
                    "store key {key:?} is not a file of the store's layout ({INDEX}, {EDGES}, {COLLECTIONS}, models/<cid>.jsonld, maps/<id>.json)"
                ))
            })?;
            let damaged = |e: &dyn std::fmt::Display| Error::Parse(format!("store {key}: {e}"));
            let json = || serde_json::from_str::<Value>(text).map_err(|e| damaged(&e));
            match file {
                LayoutFile::Index => {
                    index = serde_json::from_str(text).map_err(|e| damaged(&e))?;
                }
                LayoutFile::Edges => {
                    let f: EdgesFile = serde_json::from_str(text).map_err(|e| damaged(&e))?;
                    for (id, r) in &f.relations {
                        check_content_id(key, "relation", id, &r.id())?;
                    }
                }
                LayoutFile::Collections => {
                    let f: CollectionsFile = serde_json::from_str(text).map_err(|e| damaged(&e))?;
                    for (id, c) in &f.collections {
                        check_content_id(key, "collection", id, &c.id())?;
                    }
                }
                LayoutFile::Model(cid) => {
                    let v = json()?;
                    let named = |e: Error| match e {
                        Error::Parse(m) => damaged(&m),
                        e => damaged(&e),
                    };
                    // A net must read as one; for a file that is no kind of
                    // document the store keeps, this says what is wrong
                    // with it (not an object, say) before the kind check.
                    let kind = kind_of(&v);
                    if matches!(kind, DocKind::Net | DocKind::Unknown) {
                        Document::from_value(&v).map_err(named)?;
                    }
                    if kind == DocKind::Unknown {
                        return Err(refuse(format!(
                            "store {key}: the document there is of no kind the store keeps (a PetriNet, PetriNetBundle, PatternComposition or ModelContent)"
                        )));
                    }
                    let sealed = crate::cid::seal(&v).map_err(named)?;
                    if sealed != cid {
                        return Err(refuse(format!(
                            "store {key}: the document there has CID {sealed}, not the {cid} its key names"
                        )));
                    }
                    model_files.insert(cid, kind);
                }
                LayoutFile::Map(id) => {
                    let v = json()?;
                    if !v.get("Data").is_some_and(Value::is_object) {
                        return Err(refuse(format!(
                            "store {key}: a map file is {{\"Data\": {{key: value}}}}"
                        )));
                    }
                    check_content_id(key, "map", id, &canon::content_id(&v))?;
                }
            }
        }
        if let Some(cid) = index
            .models
            .keys()
            .find(|c| !model_files.contains_key(c.as_str()))
        {
            return Err(refuse(format!(
                "store {INDEX} lists model {cid}, but {} is not in the state",
                model_key(cid)
            )));
        }
        // The index says which kind each document is; the document says so
        // too, and they must agree.
        for (cid, meta) in &index.models {
            let listed = DocKind::from_index(meta.kind.as_deref());
            let actual = model_files[cid.as_str()];
            if listed != actual {
                return Err(refuse(format!(
                    "store {INDEX} lists model {cid} as a {}, but {} holds a {}",
                    listed.name(),
                    model_key(cid),
                    actual.name()
                )));
            }
        }
        Ok(())
    }

    /// Replaces the whole content with `blobs`, as [`Store::dump_blobs`]
    /// returns them: keys not in `blobs` are deleted. Checked first
    /// ([`Store::check_blobs`]), so a refusal leaves the store as it was.
    pub fn replace_all(&self, blobs: &BTreeMap<String, String>) -> Result<usize> {
        self.check_blobs(blobs)?;
        let _g = self.lock();
        let mut stale = BTreeSet::new();
        for prefix in LAYOUT_PREFIXES {
            stale.extend(
                self.backend
                    .list(prefix)?
                    .into_iter()
                    .filter(|k| !blobs.contains_key(k)),
            );
        }
        for (key, text) in blobs {
            self.backend.put(key, text.as_bytes())?;
        }
        for key in stale {
            self.backend.delete(&key)?;
        }
        Ok(blobs.len())
    }
}

/// A predicate filter must be registered and stored, or used by an edge.
/// An inverse-only name (narrower) is refused where `inverse` is false:
/// it is never stored, so filtering stored edges by it finds nothing.
fn check_filter_predicate(snap: &Snapshot, p: &str, inverse: bool) -> Result<()> {
    match predicate::lookup(p) {
        Some(pr) if pr.stored_as.is_some() && !inverse => Err(Error::BadArgs(format!(
            "predicate {p:?} is {} read backwards and is never stored, so no edge carries it; pass predicate {:?}, which lists those edges in either direction",
            pr.stored_as.unwrap_or_default(),
            pr.stored_as.unwrap_or_default()
        ))),
        Some(_) => Ok(()),
        None if snap.relations.values().any(|r| r.predicate == p) => Ok(()),
        None => Err(Error::BadArgs(format!(
            "predicate {p:?} is not registered and no stored edge uses it; registered predicates are {}",
            predicate::registered_names()
        ))),
    }
}

/// The model check and chain walk shared by offers and accept.
fn migration_chain(snap: &Snapshot, cid: &str) -> Result<Chain> {
    match snap.kind_of(cid) {
        None => {
            return Err(Error::NotFound(format!(
                "model {cid} is not stored: no entity has that id (store.list)"
            )))
        }
        Some(Kind::Model) => {}
        Some(k) => {
            return Err(refuse(format!(
                "{cid} is {} {}, not a model; migration offers are about models superseded with store.supersede",
                predicate::article(k.name()),
                k.name()
            )))
        }
    }
    let ch = snap.chain(cid);
    if ch.status == "not-superseded" {
        return Err(refuse(format!(
            "model {cid} is not superseded: its metadata names no successor, so none of its edges has anywhere to go; store.supersede {{old, new}} records one"
        )));
    }
    Ok(ch)
}

fn swap_end(end: &str, a: &str, t: &str) -> String {
    if end == a { t } else { end }.to_string()
}

/// Earlier acceptances of an edge: `migratedFrom` edges pointing at it.
fn migrations_of(snap: &Snapshot, eid: &str) -> Vec<Value> {
    snap.relations
        .iter()
        .filter(|(_, r)| r.predicate == predicate::MIGRATED_FROM && r.object == eid)
        .map(|(pid, r)| {
            let m = snap.relations.get(&r.subject);
            json!({
                "migrated": r.subject,
                "provenance": pid,
                "subject": m.map(|m| &m.subject),
                "predicate": m.map(|m| &m.predicate),
                "object": m.map(|m| &m.object),
            })
        })
        .collect()
}

fn stay_reason(r: &Relation, a: &str, t: &str) -> String {
    let (s2, o2) = (swap_end(&r.subject, a, t), swap_end(&r.object, a, t));
    match Fate::of(&r.predicate) {
        Fate::Structural => format!(
            "structural: proved against {a}'s exact structure; it says nothing about {t}. A prover (sim_prove {s2} {} {o2}) records a new edge only if it finds a witness",
            r.predicate
        ),
        Fate::Unregistered => format!(
            "predicate {:?} predates the registry and is not offered; store.link under a registered predicate ({}) asserts the successor's edge afresh",
            r.predicate,
            predicate::linkable_names()
        ),
        _ => match r.predicate.as_str() {
            predicate::SUPERSEDED_BY => {
                "version history: supersededBy stays with the models it relates".into()
            }
            _ => format!(
                "recorded only by {}; it stays with {a}",
                predicate::lookup(&r.predicate)
                    .map(|p| p.written_by.join(" and "))
                    .unwrap_or_default()
            ),
        },
    }
}

// ---- the process-wide handle ----

static GLOBAL: Mutex<Option<(Option<PathBuf>, Arc<Store>)>> = Mutex::new(None);

thread_local! {
    static OVERRIDE: std::cell::RefCell<Option<Arc<Store>>> = const { std::cell::RefCell::new(None) };
}

/// The store the `store.*` commands use: this thread's override if set,
/// else the process-wide store under `<data_dir>/store/` (re-created when
/// `PFLOW_NATIVE_HOME` changes), else (the web build, no data dir) a
/// process-wide memory store.
pub fn open() -> Arc<Store> {
    if let Some(s) = OVERRIDE.with(|o| o.borrow().clone()) {
        return s;
    }
    let dir = crate::outbound::data_dir().map(|d| d.join("store"));
    let mut g = GLOBAL.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((d, s)) = &*g {
        if *d == dir {
            return s.clone();
        }
    }
    let s = Arc::new(match &dir {
        Some(d) => Store::filesystem(d),
        None => Store::memory(),
    });
    *g = Some((dir, s.clone()));
    s
}

/// Makes `store` this thread's store (tests inject a memory or temporary
/// directory store this way); `None` clears the override. Returns the
/// previous override.
pub fn set_thread_store(store: Option<Arc<Store>>) -> Option<Arc<Store>> {
    OVERRIDE.with(|o| std::mem::replace(&mut *o.borrow_mut(), store))
}

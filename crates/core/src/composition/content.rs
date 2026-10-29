//! Curated model content (models-first D11): what a model means, as data
//! keyed by the model's own element ids, so a view reads it instead of
//! hard-coding it per example (docs/composition-documents.md §3).
//!
//! A [`ModelContent`] is a list of [`Entry`]s: findings (with the registry
//! check that backs them), judgments, caveats, hover notes, declared
//! scenarios and the per-view convention examples. [`validate`] checks that
//! every element id an entry names resolves against the composition.
//! Library content (`orders`, `connect4`) ships in `library/*.content.json`.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde_json::{json, Map, Value};

use super::document::{canonical_json, refuse, CONTEXT};
use super::{flatten, link_key, ontology, Composition, ExpectResult, Flat, Run};
use crate::{Error, Result};

pub const TYPE_CONTENT: &str = "ModelContent";
pub const KINDS: [&str; 6] = [
    "finding",
    "judgment",
    "caveat",
    "scenario",
    "convention-example",
    "note",
];
/// The ten composition views.
pub const VIEWS: [&str; 10] = [
    "cospan",
    "spiders",
    "operad",
    "string",
    "flat",
    "boundaries",
    "lens",
    "incidence",
    "process",
    "laws",
];
pub const OPS: [&str; 9] = [
    "==", "!=", "<", "<=", ">", ">=", "contains", "exists", "absent",
];
pub const SCENARIO_OPS: [&str; 6] = ["==", "!=", "<", "<=", ">", ">="];
/// Bounds on a content document, so a loaded one cannot ask for unbounded
/// work or memory (a document is also at most `document::MAX_DOCUMENT_BYTES`).
pub const MAX_ENTRIES: usize = 1000;
pub const MAX_TEXT_BYTES: usize = 32 * 1024;
pub const MAX_ABOUT: usize = 512;
/// A title is one line, a cite a pointer: neither is prose.
pub const MAX_TITLE_BYTES: usize = 512;
pub const MAX_CITES: usize = 64;
pub const MAX_CITE_FIELD_BYTES: usize = 2 * 1024;
/// Most registry queries all the checks of one document may run (the library's 278 fit with room):
/// `content.check`, `content.receipt` and a verifier's replay each run every one.
pub const MAX_CHECK_STEPS: usize = 1024;
/// Most members a declared scenario's `script` (or a check step's `args.script`) fires, and most
/// predicates it expects. A run keeps the marking after every step and `comp::run` stops at
/// [`super::MAX_SCRIPT_STEPS`], so a script of a transition with no input arc, repeated, cost time
/// and memory in proportion to a 4 MiB document. The library's longest is 6 members.
pub const MAX_SCRIPT_STEPS: usize = super::MAX_SCRIPT_STEPS;
pub const MAX_EXPECT_ROWS: usize = 256;

// ------------------------------------------------------------ types

/// One entry of a content document.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub kind: String,
    pub order: Option<i64>,
    pub title: String,
    pub text: String,
    pub about: Vec<String>,
    pub views: Vec<String>,
    /// `{repo, rev, path, from, to, note}` or `{bib}`.
    pub cites: Vec<Value>,
    /// `{claim, steps: [{command, args, expect: [{path, op, value}]}]}`.
    pub check: Option<Value>,
    /// `{script, expect: [{place, op, n, says}], note}`.
    pub scenario: Option<Value>,
}

/// What a content document is about.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Subject {
    /// `composition` or `plan`.
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelContent {
    pub name: String,
    pub subject: Subject,
    /// The CID of the composition document this was written against.
    pub composition: Option<String>,
    pub entries: Vec<Entry>,
}

const DOC_KEYS: [&str; 7] = [
    "@context",
    "@id",
    "@type",
    "name",
    "subject",
    "composition",
    "entries",
];
const ENTRY_KEYS: [&str; 10] = [
    "id", "kind", "order", "title", "text", "about", "views", "cites", "check", "scenario",
];

fn field_err(msg: impl std::fmt::Display) -> Error {
    refuse("CONTENT_FIELD", msg)
}

fn str_list(m: &Map<String, Value>, k: &str, what: &str) -> Result<Vec<String>> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(vec![]),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| {
                x.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| field_err(format!("{what}: {k} must be a list of strings")))
            })
            .collect(),
        Some(_) => Err(field_err(format!("{what}: {k} must be a list of strings"))),
    }
}

/// An object, or a string holding one (the exported form).
fn object_or_text(v: &Value, what: &str) -> Result<Value> {
    let o = match v {
        Value::String(s) => serde_json::from_str::<Value>(s)
            .map_err(|e| field_err(format!("{what} is not JSON text: {e}")))?,
        o => o.clone(),
    };
    if o.is_object() {
        Ok(o)
    } else {
        Err(field_err(format!("{what} must be an object")))
    }
}

impl Entry {
    fn parse(v: &Value) -> Result<Entry> {
        let m = v
            .as_object()
            .ok_or_else(|| field_err("an entry must be an object"))?;
        let id = m
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| field_err("an entry needs a string id"))?
            .to_string();
        let what = format!("entry {id:?}");
        for k in m.keys() {
            if !ENTRY_KEYS.contains(&k.as_str()) {
                return Err(field_err(format!(
                    "{what}: unknown key {k:?} (it would not survive an export)"
                )));
            }
        }
        let s = |k: &str| -> Result<String> {
            match m.get(k) {
                None | Some(Value::Null) => Ok(String::new()),
                Some(Value::String(x)) => Ok(x.clone()),
                Some(_) => Err(field_err(format!("{what}: {k} must be a string"))),
            }
        };
        let cites = match m.get("cites") {
            None | Some(Value::Null) => vec![],
            Some(Value::Array(a)) => a.clone(),
            Some(_) => return Err(field_err(format!("{what}: cites must be a list"))),
        };
        let order = match m.get("order") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_i64()
                    .ok_or_else(|| field_err(format!("{what}: order must be an integer")))?,
            ),
        };
        Ok(Entry {
            kind: s("kind")?,
            order,
            title: s("title")?,
            text: s("text")?,
            about: str_list(m, "about", &what)?,
            views: str_list(m, "views", &what)?,
            cites,
            check: m
                .get("check")
                .filter(|v| !v.is_null())
                .map(|v| object_or_text(v, &format!("{what}: check")))
                .transpose()?,
            scenario: m
                .get("scenario")
                .filter(|v| !v.is_null())
                .map(|v| object_or_text(v, &format!("{what}: scenario")))
                .transpose()?,
            id,
        })
    }

    /// The exported form: ordered or nested values as canonical text.
    fn to_json(&self) -> Value {
        let mut v = json!({"id": self.id, "kind": self.kind});
        if let Some(o) = self.order {
            v["order"] = json!(o);
        }
        if !self.title.is_empty() {
            v["title"] = json!(self.title);
        }
        if !self.text.is_empty() {
            v["text"] = json!(self.text);
        }
        if !self.about.is_empty() {
            v["about"] = json!(self.about);
        }
        if !self.views.is_empty() {
            v["views"] = json!(self.views);
        }
        if !self.cites.is_empty() {
            v["cites"] = json!(self.cites);
        }
        if let Some(c) = &self.check {
            v["check"] = json!(canonical_json(c));
        }
        if let Some(s) = &self.scenario {
            v["scenario"] = json!(canonical_json(s));
        }
        v
    }

    /// The entry for a view to read: `check` and `scenario` as objects.
    pub fn to_view_json(&self) -> Value {
        let mut v = self.to_json();
        if let Some(c) = &self.check {
            v["check"] = c.clone();
        }
        if let Some(s) = &self.scenario {
            v["scenario"] = s.clone();
        }
        v
    }
}

impl ModelContent {
    /// Reads a content document (`check` and `scenario` as objects or as
    /// canonical text). Structure only: [`validate`] says whether it is
    /// sound.
    pub fn parse(doc: &Value) -> Result<ModelContent> {
        let m = doc
            .as_object()
            .ok_or_else(|| refuse("CONTENT_TYPE", "a content document must be an object"))?;
        if m.get("@type").and_then(Value::as_str) != Some(TYPE_CONTENT) {
            return Err(refuse(
                "CONTENT_TYPE",
                format!("@type must be {TYPE_CONTENT:?}"),
            ));
        }
        if m.get("@context").and_then(Value::as_str) != Some(CONTEXT) {
            return Err(refuse(
                "CONTENT_TYPE",
                format!("@context must be {CONTEXT:?}"),
            ));
        }
        for k in m.keys() {
            if !DOC_KEYS.contains(&k.as_str()) {
                return Err(field_err(format!("unknown key {k:?}")));
            }
        }
        // `store.get` returns the document with the `@id` the store added; the seal ignores a
        // top-level `@id`, so the one that is the document's own CID is accepted (an export drops it).
        if let Some(id) = m.get("@id") {
            let want = crate::cid::seal(doc)?;
            if id.as_str() != Some(want.as_str()) {
                return Err(field_err(format!(
                    "\"@id\" must be the CID of the document ({want}) or absent"
                )));
            }
        }
        let subject = match m.get("subject") {
            Some(Value::Object(s)) => Subject {
                kind: s
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                name: s
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            },
            _ => return Err(field_err("subject must be {kind, name}")),
        };
        if !["composition", "plan"].contains(&subject.kind.as_str()) {
            return Err(field_err(format!(
                "subject.kind must be \"composition\" or \"plan\", not {:?}",
                subject.kind
            )));
        }
        let entries = m
            .get("entries")
            .and_then(Value::as_array)
            .ok_or_else(|| field_err("entries must be a list"))?
            .iter()
            .map(Entry::parse)
            .collect::<Result<Vec<_>>>()?;
        Ok(ModelContent {
            name: m
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            subject,
            composition: m
                .get("composition")
                .and_then(Value::as_str)
                .map(str::to_string),
            entries,
        })
    }

    /// The exported document (what is stored and sealed).
    pub fn to_document(&self) -> Value {
        let mut v = json!({
            "@context": CONTEXT,
            "@type": TYPE_CONTENT,
            "name": self.name,
            "subject": {"kind": self.subject.kind, "name": self.subject.name},
            "entries": self.entries.iter().map(Entry::to_json).collect::<Vec<_>>(),
        });
        if let Some(c) = &self.composition {
            v["composition"] = json!(c);
        }
        v
    }

    pub fn cid(&self) -> Result<String> {
        crate::cid::seal(&self.to_document())
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Entries in display order: `order` first, then the document's order.
    pub fn sorted(&self) -> Vec<&Entry> {
        let mut v: Vec<(usize, &Entry)> = self.entries.iter().enumerate().collect();
        v.sort_by_key(|(i, e)| (e.order.unwrap_or(i64::MAX), *i));
        v.into_iter().map(|(_, e)| e).collect()
    }

    /// The declared scenarios (`scenario` entries that parse).
    pub fn scenarios(&self) -> Vec<DeclaredScenario> {
        self.sorted()
            .into_iter()
            .filter(|e| e.kind == "scenario")
            .filter_map(DeclaredScenario::of)
            .collect()
    }

    /// With this entry's check, if it has one.
    pub fn checks(&self) -> Vec<&Entry> {
        self.sorted()
            .into_iter()
            .filter(|e| e.check.is_some())
            .collect()
    }
}

// ------------------------------------------------------------ scenarios

#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredExpect {
    pub place: String,
    pub op: String,
    pub n: i64,
    pub says: String,
}

/// A declared firing script: `scenario` entries, owned (the static
/// [`super::Scenario`] list is the orders library's, derived from the same
/// content).
#[derive(Debug, Clone, PartialEq)]
pub struct DeclaredScenario {
    pub id: String,
    pub title: String,
    pub script: Vec<String>,
    pub expect: Vec<DeclaredExpect>,
    pub note: String,
}

impl DeclaredScenario {
    pub fn of(e: &Entry) -> Option<DeclaredScenario> {
        let s = e.scenario.as_ref()?;
        let script = s
            .get("script")?
            .as_array()?
            .iter()
            .map(|x| x.as_str().map(str::to_string))
            .collect::<Option<Vec<_>>>()?;
        let expect = s
            .get("expect")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .map(|x| {
                        Some(DeclaredExpect {
                            place: x.get("place")?.as_str()?.to_string(),
                            op: x
                                .get("op")
                                .and_then(Value::as_str)
                                .unwrap_or("==")
                                .to_string(),
                            n: x.get("n")?.as_i64()?,
                            says: x
                                .get("says")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string(),
                        })
                    })
                    .collect::<Option<Vec<_>>>()
            })
            .unwrap_or(Some(vec![]))?;
        Some(DeclaredScenario {
            id: e.id.clone(),
            title: e.title.clone(),
            script,
            expect,
            note: s
                .get("note")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        })
    }

    /// The shape `bundle.scenarios` has always returned.
    pub fn to_json(&self) -> Value {
        let mut v = json!({
            "id": self.id,
            "title": self.title,
            "script": self.script,
            "expect": self.expect.iter().map(|e| json!({
                "place": e.place, "op": e.op, "n": e.n, "says": e.says,
            })).collect::<Vec<_>>(),
        });
        if !self.note.is_empty() {
            v["note"] = json!(self.note);
        }
        v
    }
}

pub(super) fn compare(op: &str, actual: i64, n: i64) -> bool {
    match op {
        "==" => actual == n,
        "!=" => actual != n,
        "<" => actual < n,
        "<=" => actual <= n,
        ">" => actual > n,
        ">=" => actual >= n,
        _ => false,
    }
}

/// Fires a declared scenario on the flat net (`comp::run`) and evaluates
/// its predicates on the marking reached.
pub fn run_declared(flat: &Flat, sc: &DeclaredScenario) -> Run {
    let mut r = super::run(flat, &sc.id, &sc.title, &sc.script, &[]);
    r.expect = sc
        .expect
        .iter()
        .map(|e| {
            let f = flat.place_of(&e.place).map(str::to_string);
            let actual = f
                .as_ref()
                .and_then(|p| r.final_marking.get(p))
                .copied()
                .unwrap_or(0);
            ExpectResult {
                place: e.place.clone(),
                flat: f,
                op: e.op.clone(),
                n: e.n,
                says: e.says.clone(),
                actual,
                pass: compare(&e.op, actual, e.n),
            }
        })
        .collect();
    r
}

// ------------------------------------------------------------ library

/// A content document that ships with the build.
pub struct Library {
    pub id: &'static str,
    pub content: ModelContent,
    /// The instances a composition must include for this content to attach.
    pub anchors: &'static [&'static str],
}

const ORDERS_JSON: &str = include_str!("library/orders.content.json");
const CONNECT4_JSON: &str = include_str!("library/connect4.content.json");

pub fn library() -> &'static [Library] {
    static L: OnceLock<Vec<Library>> = OnceLock::new();
    L.get_or_init(|| {
        let parse = |text: &str, id: &str| {
            let v: Value = serde_json::from_str(text)
                .unwrap_or_else(|e| panic!("library {id} is not JSON: {e}"));
            ModelContent::parse(&v).unwrap_or_else(|e| panic!("library {id}: {e}"))
        };
        vec![
            Library {
                id: "orders",
                content: parse(ORDERS_JSON, "orders"),
                anchors: &["orders", "workers", "payment"],
            },
            Library {
                id: "connect4",
                content: parse(CONNECT4_JSON, "connect4"),
                anchors: &[],
            },
        ]
    })
}

pub fn library_entry(id: &str) -> Option<&'static Library> {
    library().iter().find(|l| l.id == id)
}

/// The library content written against the composition document `cid`
/// (its `composition` field): what a document that IS a library example
/// gets back when it is loaded.
pub fn library_for_cid(cid: &str) -> Option<&'static Library> {
    library()
        .iter()
        .find(|l| l.content.composition.as_deref() == Some(cid))
}

/// The library content a set of instance ids attaches (those with anchors
/// that the instances include).
pub fn library_for_instances(instances: &[String]) -> Option<&'static Library> {
    library().iter().find(|l| {
        !l.anchors.is_empty() && l.anchors.iter().all(|a| instances.iter().any(|i| i == a))
    })
}

// ------------------------------------------------------------ validation

#[derive(Debug, Clone, PartialEq)]
pub struct Issue {
    pub code: String,
    pub entry: Option<String>,
    pub reference: Option<String>,
    pub message: String,
}

impl Issue {
    fn new(code: &str, entry: Option<&str>, reference: Option<&str>, message: String) -> Issue {
        Issue {
            code: code.into(),
            entry: entry.map(str::to_string),
            reference: reference.map(str::to_string),
            message,
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "code": self.code,
            "entry": self.entry,
            "ref": self.reference,
            "message": self.message,
        })
    }
}

/// The codes of an issue that is "this names something the model does not have": an error under
/// `strict`, a warning (the entry is dormant) otherwise.
const UNRESOLVED_CODES: [&str; 3] = [
    "ID_UNRESOLVED",
    "SCENARIO_UNRESOLVED",
    "CHECK_REF_UNRESOLVED",
];

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
}

impl Report {
    pub fn valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// `{entry, ref}` of every entry id that did not resolve (error or
    /// warning).
    pub fn unresolved(&self) -> Vec<Value> {
        self.errors
            .iter()
            .chain(&self.warnings)
            .filter(|i| UNRESOLVED_CODES.contains(&i.code.as_str()))
            .map(|i| json!({"entry": i.entry, "ref": i.reference}))
            .collect()
    }

    pub fn to_json(&self) -> Value {
        json!({
            "valid": self.valid(),
            "errors": self.errors.iter().map(Issue::to_json).collect::<Vec<_>>(),
            "warnings": self.warnings.iter().map(Issue::to_json).collect::<Vec<_>>(),
        })
    }

    /// `[CODE] message; …` for a refusal.
    pub fn refusal(&self) -> String {
        self.errors
            .iter()
            .map(|i| format!("[{}] {}", i.code, i.message))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

/// The element ids of a composition, by type (spec §3.2).
pub struct Ids {
    sets: BTreeMap<&'static str, BTreeSet<String>>,
    flat_ok: bool,
}

const TYPES: [&str; 16] = [
    "model",
    "instance",
    "place",
    "transition",
    "port",
    "link",
    "constraint",
    "scenario",
    "flat-place",
    "flat-transition",
    "idiom",
    "view",
    "piece",
    "slot",
    "net-place",
    "net-transition",
];

impl Ids {
    /// The element ids of the Connect-4 search plan: its pieces, template
    /// slots and net, plus the views (spec §3.2, plan subject).
    pub fn of_plan() -> Ids {
        let mut sets: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
        let mut put = |t: &'static str, id: String| {
            sets.entry(t).or_default().insert(id);
        };
        for p in crate::search_plan::pieces() {
            put("piece", p.name.to_string());
        }
        for sl in crate::search_plan::SLOTS.iter() {
            put("slot", sl.id.to_string());
        }
        if let Ok(m) = crate::search_plan::connect4_model() {
            for p in &m.places {
                put("net-place", p.id.clone());
            }
            for t in &m.transitions {
                put("net-transition", t.id.clone());
            }
        }
        for v in VIEWS {
            put("view", v.to_string());
        }
        Ids {
            sets,
            flat_ok: false,
        }
    }

    pub fn of(c: &Composition, declared_scenarios: &[String]) -> Ids {
        let mut sets: BTreeMap<&'static str, BTreeSet<String>> = BTreeMap::new();
        let mut put = |t: &'static str, id: String| {
            sets.entry(t).or_default().insert(id);
        };
        for s in &c.bundle.subnets {
            put("instance", s.id.clone());
            for p in &s.model.places {
                put("place", format!("{}/{}", s.id, p.id));
            }
            for t in &s.model.transitions {
                put("transition", format!("{}/{}", s.id, t.id));
            }
            for p in &s.ports {
                put("port", format!("{}:{}", s.id, p.id));
            }
            for k in &s.model.constraints {
                put("constraint", format!("{}/{}", s.id, k.id));
            }
        }
        for k in &c.bundle.constraints {
            put("constraint", k.id.clone());
        }
        for l in &c.bundle.links {
            put("link", link_key(l));
            if l.kind == pflow_compose::LinkKind::Event {
                put(
                    "link",
                    link_key(&super::link(l.kind, l.to.clone(), l.from.clone())),
                );
            }
        }
        let flat_ok = match flatten(&c.bundle) {
            Ok(f) => {
                for p in &f.model.places {
                    put("flat-place", p.id.clone());
                }
                for t in &f.model.transitions {
                    put("flat-transition", t.id.clone());
                }
                true
            }
            Err(_) => false,
        };
        if let Some(a) = ontology()["idioms"].as_array() {
            for i in a {
                if let Some(k) = i["key"].as_str() {
                    put("idiom", k.to_string());
                }
            }
        }
        for v in VIEWS {
            put("view", v.to_string());
        }
        for s in declared_scenarios {
            put("scenario", s.clone());
        }
        Ids { sets, flat_ok }
    }

    /// `Ok(true)` when `reference` names an element; `Err` when it is not
    /// `<type>:<ref>` of a known type.
    pub fn resolves(&self, reference: &str) -> std::result::Result<bool, String> {
        let (t, r) = reference
            .split_once(':')
            .ok_or_else(|| format!("{reference:?} is not <type>:<ref>"))?;
        let t = TYPES
            .iter()
            .find(|x| **x == t)
            .ok_or_else(|| format!("unknown element type {t:?} in {reference:?}"))?;
        Ok(match *t {
            // `model:` names the whole model: its ref is empty (docs §3.2), so `model:orders` is a
            // typo for a thing that is not there, not a second way to say "everything".
            "model" => r.is_empty(),
            "flat-place" | "flat-transition" if !self.flat_ok => false,
            t => self.sets.get(t).is_some_and(|s| s.contains(r)),
        })
    }
}

/// The entries of `content` whose element ids all resolve against `c`, in display order: what a
/// view may read and draw. The others are dormant (`content.get` leaves them out unless asked for
/// `all`).
pub fn live_entries<'a>(content: &'a ModelContent, c: &Composition) -> Vec<&'a Entry> {
    let declared: Vec<String> = content
        .entries
        .iter()
        .filter(|e| e.kind == "scenario")
        .map(|e| e.id.clone())
        .collect();
    let ids = Ids::of(c, &declared);
    content
        .sorted()
        .into_iter()
        .filter(|e| e.about.iter().all(|r| ids.resolves(r).unwrap_or(false)))
        .collect()
}

/// How many entries of `content` name an element `c` does not have: dormant, kept and shown by no
/// view.
pub fn dormant_count(content: &ModelContent, c: &Composition) -> usize {
    content.entries.len() - live_entries(content, c).len()
}

fn bib_keys() -> BTreeSet<String> {
    ontology()["bib"]
        .as_object()
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

/// Checks `content` against the composition `c`. `strict`: an id that does
/// not resolve is an error (an explicit load); otherwise a warning (library
/// content on an edited composition: the entry is dormant).
pub fn validate(content: &ModelContent, c: &Composition, strict: bool) -> Report {
    let mut r = Report::default();
    let declared: Vec<String> = content
        .entries
        .iter()
        .filter(|e| e.kind == "scenario")
        .map(|e| e.id.clone())
        .collect();
    let ids = Ids::of(c, &declared);
    let flat = flatten(&c.bundle).ok();
    validate_entries(content, &mut r, |reference| ids.resolves(reference), strict);
    // Scenario members and places need the flat net.
    for e in content.entries.iter().filter(|e| e.kind == "scenario") {
        if DeclaredScenario::of(e).is_none() {
            r.errors.push(Issue::new(
                "SCENARIO_INVALID",
                Some(&e.id),
                None,
                "the scenario must be {script: [\"<subnet>/<transition>\", …], expect: [{place, op, n, says}]}".into(),
            ));
        }
    }
    for sc in content.scenarios() {
        for e in &sc.expect {
            if !SCENARIO_OPS.contains(&e.op.as_str()) {
                r.errors.push(Issue::new(
                    "SCENARIO_INVALID",
                    Some(&sc.id),
                    None,
                    format!("unknown op {:?}; there are {SCENARIO_OPS:?}", e.op),
                ));
            }
        }
        if let Some(flat) = &flat {
            // A member or place the model does not have: an error for an explicit load, and for
            // library content on an edited model a warning (the scenario is dormant: it still
            // runs, and stops where the member is missing, as the prototype's does).
            let mut unresolved = |reference: &str, message: String| {
                if strict {
                    r.errors.push(Issue::new(
                        "SCENARIO_INVALID",
                        Some(&sc.id),
                        Some(reference),
                        message,
                    ));
                } else {
                    r.warnings.push(Issue::new(
                        "SCENARIO_UNRESOLVED",
                        Some(&sc.id),
                        Some(reference),
                        message,
                    ));
                }
            };
            for m in &sc.script {
                if flat.transition_of(m).is_none() {
                    unresolved(
                        m,
                        format!("script member {m:?} is not a transition of the composition"),
                    );
                }
            }
            for e in &sc.expect {
                if flat.place_of(&e.place).is_none() {
                    unresolved(
                        &e.place,
                        format!(
                            "expected place {:?} is not a place of the composition",
                            e.place
                        ),
                    );
                }
            }
        }
    }
    // The scenarios and script members a check names must be there too: a check that runs a
    // scenario the model does not declare would load clean and fail only when run.
    let scenario_ids: BTreeSet<&str> = declared.iter().map(String::as_str).collect();
    for e in &content.entries {
        for (reference, message) in check_refs(e, &scenario_ids, flat.as_ref()) {
            let issue = Issue::new(
                "CHECK_REF_UNRESOLVED",
                Some(&e.id),
                Some(&reference),
                message,
            );
            if strict {
                r.errors.push(issue)
            } else {
                r.warnings.push(issue)
            }
        }
    }
    if let Some(cid) = &content.composition {
        if let Ok(own) = c.document_cid() {
            if &own != cid {
                r.warnings.push(Issue::new(
                    "COMPOSITION_CID_STALE",
                    None,
                    Some(cid),
                    format!("written against {cid}; the open composition is {own}"),
                ));
            }
        }
    }
    r
}

/// The names a check's steps use that the model does not have: `args.scenario` (a declared
/// scenario) and `args.script` members (transitions of the flat net).
fn check_refs(e: &Entry, scenarios: &BTreeSet<&str>, flat: Option<&Flat>) -> Vec<(String, String)> {
    let mut out = vec![];
    let steps = e
        .check
        .as_ref()
        .and_then(|c| c.get("steps"))
        .and_then(Value::as_array);
    for (i, st) in steps.into_iter().flatten().enumerate() {
        let args = st.get("args");
        if let Some(sc) = args.and_then(|a| a.get("scenario")).and_then(Value::as_str) {
            if !scenarios.contains(sc) {
                out.push((
                    format!("scenario:{sc}"),
                    format!("step {i} runs scenario {sc:?}, which the content does not declare"),
                ));
            }
        }
        let script = args.and_then(|a| a.get("script")).and_then(Value::as_array);
        for m in script.into_iter().flatten().filter_map(Value::as_str) {
            if flat.is_some_and(|f| f.transition_of(m).is_none()) {
                out.push((
                    format!("transition:{m}"),
                    format!("step {i} fires {m:?}, which is not a transition of the composition"),
                ));
            }
        }
    }
    out
}

/// [`validate`] for a plan subject: structure, and every `about` id against
/// the Connect-4 plan's pieces, slots and net.
pub fn validate_plan(content: &ModelContent, strict: bool) -> Report {
    let mut r = Report::default();
    let ids = Ids::of_plan();
    validate_entries(content, &mut r, |reference| ids.resolves(reference), strict);
    for e in content.entries.iter().filter(|e| e.kind == "scenario") {
        r.errors.push(Issue::new(
            "SCENARIO_INVALID",
            Some(&e.id),
            None,
            "a plan declares no firing scenarios".into(),
        ));
    }
    r
}

/// The bounds on a content document (`CONTENT_LIMIT`): how many entries, how long a title, text,
/// cite list, `about` list, script or list of expected rows, and how many check steps in all.
/// Nothing here needs the model, so `session.restore_state` applies it to content it reads back
/// too ([`limit_report`]).
fn check_limits(content: &ModelContent, r: &mut Report) {
    let mut limit = |entry: Option<&str>, message: String| {
        r.errors
            .push(Issue::new("CONTENT_LIMIT", entry, None, message))
    };
    if content.entries.len() > MAX_ENTRIES {
        limit(
            None,
            format!(
                "{} entries; at most {MAX_ENTRIES} are accepted",
                content.entries.len()
            ),
        );
    }
    let steps: usize = content
        .entries
        .iter()
        .filter_map(|e| e.check.as_ref()?.get("steps")?.as_array().map(Vec::len))
        .sum();
    if steps > MAX_CHECK_STEPS {
        limit(
            None,
            format!("the checks have {steps} steps in all; at most {MAX_CHECK_STEPS} are accepted"),
        );
    }
    for e in &content.entries {
        let id = Some(e.id.as_str());
        if e.title.len() > MAX_TITLE_BYTES || e.cites.len() > MAX_CITES {
            limit(
                id,
                format!(
                    "a title of {} bytes and {} cites; at most {MAX_TITLE_BYTES} bytes and {MAX_CITES} cites",
                    e.title.len(),
                    e.cites.len()
                ),
            );
        }
        if e.text.len() > MAX_TEXT_BYTES || e.about.len() > MAX_ABOUT {
            limit(
                id,
                format!(
                    "text of {} bytes and {} about ids; at most {MAX_TEXT_BYTES} bytes and {MAX_ABOUT} ids",
                    e.text.len(),
                    e.about.len()
                ),
            );
        }
        let len = |v: Option<&Value>| v.and_then(Value::as_array).map_or(0, Vec::len);
        if let Some(sc) = &e.scenario {
            let (script, expect) = (len(sc.get("script")), len(sc.get("expect")));
            if script > MAX_SCRIPT_STEPS || expect > MAX_EXPECT_ROWS {
                limit(
                    id,
                    format!(
                        "a scenario of {script} members and {expect} expected rows; at most {MAX_SCRIPT_STEPS} members and {MAX_EXPECT_ROWS} rows"
                    ),
                );
            }
        }
        for (i, st) in e
            .check
            .as_ref()
            .and_then(|c| c.get("steps")?.as_array())
            .into_iter()
            .flatten()
            .enumerate()
        {
            let (script, rows) = (
                len(st.get("args").and_then(|a| a.get("script"))),
                len(st.get("expect")),
            );
            if script > MAX_SCRIPT_STEPS || rows > MAX_EXPECT_ROWS {
                limit(
                    id,
                    format!(
                        "check step {i} fires {script} members and expects {rows} rows; at most {MAX_SCRIPT_STEPS} members and {MAX_EXPECT_ROWS} rows"
                    ),
                );
            }
        }
    }
}

/// The report of [`check_limits`] alone: what `session.restore_state` refuses content for.
pub fn limit_report(content: &ModelContent) -> Report {
    let mut r = Report::default();
    check_limits(content, &mut r);
    r
}

/// The checks that do not depend on the subject: structure, kinds, views,
/// cites, check syntax; `resolve` decides the element ids.
pub fn validate_entries(
    content: &ModelContent,
    r: &mut Report,
    resolve: impl Fn(&str) -> std::result::Result<bool, String>,
    strict: bool,
) {
    let bib = bib_keys();
    let mut seen = BTreeSet::new();
    check_limits(content, r);
    let mut conventions: BTreeMap<&str, &str> = BTreeMap::new();
    for e in &content.entries {
        let id = Some(e.id.as_str());
        let ok_id = !e.id.is_empty()
            && e.id
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric())
            && e.id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c));
        if !ok_id {
            r.errors.push(Issue::new(
                "ENTRY_ID",
                id,
                None,
                format!("entry id {:?} must match [a-z0-9][a-z0-9._-]*", e.id),
            ));
        }
        if !seen.insert(e.id.clone()) {
            r.errors.push(Issue::new(
                "ENTRY_DUPLICATE",
                id,
                None,
                format!("entry id {:?} appears twice", e.id),
            ));
        }
        if !KINDS.contains(&e.kind.as_str()) {
            r.errors.push(Issue::new(
                "ENTRY_KIND",
                id,
                None,
                format!("kind {:?}; there are {KINDS:?}", e.kind),
            ));
        }
        if e.kind != "scenario" && e.text.trim().is_empty() {
            r.errors.push(Issue::new(
                "CONTENT_FIELD",
                id,
                None,
                "text is required".into(),
            ));
        }
        if e.scenario.is_some() && e.kind != "scenario" {
            r.errors.push(Issue::new(
                "CONTENT_FIELD",
                id,
                None,
                "only an entry of kind scenario carries a scenario".into(),
            ));
        }
        if e.about.is_empty() {
            r.errors.push(Issue::new(
                "ENTRY_ABOUT_EMPTY",
                id,
                None,
                "an entry names at least one element (model: for the whole model)".into(),
            ));
        }
        for reference in &e.about {
            match resolve(reference) {
                Err(msg) => r
                    .errors
                    .push(Issue::new("ID_SYNTAX", id, Some(reference), msg)),
                Ok(true) => {}
                Ok(false) => {
                    let issue = Issue::new(
                        "ID_UNRESOLVED",
                        id,
                        Some(reference),
                        format!("{reference} does not name an element of the model"),
                    );
                    if strict {
                        r.errors.push(issue)
                    } else {
                        r.warnings.push(issue)
                    }
                }
            }
        }
        for v in &e.views {
            if !VIEWS.contains(&v.as_str()) {
                r.errors.push(Issue::new(
                    "ENTRY_VIEW",
                    id,
                    Some(v),
                    format!("no view {v:?}; there are {VIEWS:?}"),
                ));
            }
        }
        if e.kind == "convention-example" && e.views.len() != 1 {
            r.errors.push(Issue::new(
                "ENTRY_VIEW",
                id,
                None,
                "a convention-example names exactly one view".into(),
            ));
        } else if e.kind == "convention-example" {
            // A view shows one convention example: a second would be drawn beside the first.
            if let Some(first) = conventions.insert(e.views[0].as_str(), e.id.as_str()) {
                r.warnings.push(Issue::new(
                    "CONVENTION_DUPLICATE",
                    id,
                    Some(&e.views[0]),
                    format!(
                        "view {:?} already has the convention example {first:?}; both are drawn",
                        e.views[0]
                    ),
                ));
            }
        }
        for c in &e.cites {
            if let Some(msg) = cite_problem(c) {
                r.errors.push(Issue::new("CITE_INVALID", id, None, msg));
            } else if let Some(k) = c.get("bib").and_then(Value::as_str) {
                if !bib.contains(k) {
                    r.errors.push(Issue::new(
                        "CITE_BIB_UNKNOWN",
                        id,
                        Some(k),
                        format!("no bibliography key {k:?} in the ontology"),
                    ));
                }
            }
        }
        if let Some(chk) = &e.check {
            for (code, msg) in super::checks::check_issues(chk) {
                r.errors.push(Issue::new(code, id, None, msg));
            }
        }
    }
}

/// What is wrong with a cite, if anything: `{bib: "<key>"}` (a key of the ontology's `bib`, checked
/// by the caller) or a source cite `{repo, path, rev?, from?, to?, note?}`; nothing else, so a
/// panel never has a line of empty strings to print.
fn cite_problem(c: &Value) -> Option<String> {
    let Some(m) = c.as_object() else {
        return Some(format!("a cite must be an object, not {c}"));
    };
    let text = |k: &str| m.get(k).and_then(Value::as_str);
    let line = |k: &str| m.get(k).and_then(Value::as_u64).filter(|n| *n > 0);
    if m.contains_key("bib") {
        if let Some(k) = m.keys().find(|k| *k != "bib") {
            return Some(format!("a bib cite is {{bib}} alone, not also {k:?}"));
        }
        return match text("bib") {
            Some(k) if !k.is_empty() => None,
            _ => Some("bib must be a non-empty string".into()),
        };
    }
    for k in m.keys() {
        if !["repo", "path", "rev", "from", "to", "note"].contains(&k.as_str()) {
            return Some(format!(
                "unknown key {k:?} in a cite (it is {{bib}} or {{repo, path, rev, from, to, note}})"
            ));
        }
    }
    for k in ["repo", "path"] {
        if text(k).is_none_or(str::is_empty) {
            return Some(format!("a source cite needs a non-empty {k}"));
        }
    }
    for k in ["rev", "note"] {
        if m.get(k).is_some_and(|v| !v.is_string()) {
            return Some(format!("{k} must be a string"));
        }
    }
    for k in ["from", "to"] {
        if m.get(k).is_some() && line(k).is_none() {
            return Some(format!("{k} must be a positive line number"));
        }
    }
    if let (Some(a), Some(b)) = (line("from"), line("to")) {
        if a > b {
            return Some(format!("from {a} is after to {b}"));
        }
    }
    let big = m
        .values()
        .filter_map(Value::as_str)
        .any(|t| t.len() > MAX_CITE_FIELD_BYTES);
    big.then(|| format!("a cite field is over {MAX_CITE_FIELD_BYTES} bytes"))
}

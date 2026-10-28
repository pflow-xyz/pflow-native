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

const DOC_KEYS: [&str; 6] = [
    "@context",
    "@type",
    "name",
    "subject",
    "composition",
    "entries",
];
const ENTRY_KEYS: [&str; 11] = [
    "id", "kind", "order", "title", "text", "about", "views", "cites", "check", "scenario", "by",
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
        Ok(Entry {
            kind: s("kind")?,
            order: m.get("order").and_then(Value::as_i64),
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

fn compare(op: &str, actual: i64, n: i64) -> bool {
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
const CONNECT4_JSON: &str = include_str!("library/connect4.stub.json");

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
            .filter(|i| i.code == "ID_UNRESOLVED")
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

const TYPES: [&str; 12] = [
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
];

impl Ids {
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
            "model" => true,
            "flat-place" | "flat-transition" if !self.flat_ok => false,
            t => self.sets.get(t).is_some_and(|s| s.contains(r)),
        })
    }
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
            for m in &sc.script {
                if flat.transition_of(m).is_none() {
                    r.errors.push(Issue::new(
                        "SCENARIO_INVALID",
                        Some(&sc.id),
                        Some(m),
                        format!("script member {m:?} is not a transition of the composition"),
                    ));
                }
            }
            for e in &sc.expect {
                if flat.place_of(&e.place).is_none() {
                    r.errors.push(Issue::new(
                        "SCENARIO_INVALID",
                        Some(&sc.id),
                        Some(&e.place),
                        format!(
                            "expected place {:?} is not a place of the composition",
                            e.place
                        ),
                    ));
                }
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
        if e.kind != "scenario" && e.text.is_empty() {
            r.errors.push(Issue::new(
                "CONTENT_FIELD",
                id,
                None,
                "text is required".into(),
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
        }
        for c in &e.cites {
            if let Some(k) = c.get("bib").and_then(Value::as_str) {
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
            for msg in super::checks::check_problems(chk) {
                let code = if msg.starts_with("command ") {
                    "CHECK_COMMAND_FORBIDDEN"
                } else {
                    "CHECK_INVALID"
                };
                r.errors.push(Issue::new(code, id, None, msg));
            }
        }
    }
}

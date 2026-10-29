//! Composition documents: the two forms `bundle.load` reads and
//! `bundle.export` writes (docs/composition-documents.md).
//!
//! * **Form (a)**, `PatternComposition`: the prototype's `INSTANCE_SPECS`
//!   shape, `{id, pattern, params}` per instance plus typed links. The
//!   subnets are rebuilt by pflow-compose's constructors, so only the recipe
//!   is stored. It is what the views need: port schemas, ontology keys,
//!   idiom lighting, snap-together.
//! * **Form (b)**, a raw `PetriNetBundle`: parsed by pflow-compose's `Bundle`
//!   (which drops keys it does not know), with the JSON as loaded kept beside
//!   it so an unedited export is the bytes received and keeps its CID.
//!
//! Sealing is `crate::cid::seal` on the exported JSON and nothing else.
//! `params` is a canonical-JSON *string* in an exported document: only
//! `arcs`, `token`, `weight`, `capacity`, `initial` and `parents` are lists
//! to the sealer; every other array hashes as an unordered set, so ordered
//! or nested parameters (a saga's `steps`) must travel as text the hash
//! covers.

use std::collections::BTreeMap;

use pflow_compose::{ArcMergePolicy, Bundle, Link, LinkKind, Subnet};
use serde_json::{json, Map, Value};

use super::content::{self, ModelContent};
use super::{
    build, catalog, instance, kind_name, link_key, parse_endpoint, parse_kind, Composition,
    InstanceSpec,
};
use crate::{Error, Result};

pub const CONTEXT: &str = "https://pflow.xyz/schema";
pub const TYPE_COMPOSITION: &str = "PatternComposition";
pub const TYPE_BUNDLE: &str = "PetriNetBundle";
pub const PATTERNS: [&str; 3] = ["queue", "resource-pool", "saga"];

/// `[CODE] message`, the shape of every refusal in this feature.
pub fn refuse(code: &str, msg: impl std::fmt::Display) -> Error {
    Error::Invalid(format!("[{code}] {msg}"))
}

// ------------------------------------------------------------ canonical JSON

/// The canonical text of a parameter value: [`crate::model_id::canonical_json`]'s
/// rule (object keys sorted bytewise at every depth, no whitespace, numbers
/// spelled as `JSON.stringify` does, strings escaped as it does; not Go's
/// HTML-escaping `receipt.canonical`), so the writer and the identity rule
/// cannot drift. It is what a writer emits for a value it built; a string
/// received in a document is kept as received.
pub fn canonical_json(v: &Value) -> String {
    crate::model_id::canonical_value(v)
}

/// Most bytes of a document `bundle.load` / `content.load` accept.
pub const MAX_DOCUMENT_BYTES: usize = 4 * 1024 * 1024;

/// Most instances (subnets) and links a loaded composition may have. The byte cap bounds the size of
/// a document, not the work: flattening is superlinear in both (about 7 s at 1000 chained queues,
/// 78 s at 3000, and `bundle.state` flattens several times), and the commands run on the caller's
/// thread. A hand-made model is a handful of each.
pub const MAX_INSTANCES: usize = 128;
pub const MAX_LINKS: usize = 1000;

fn within(what: &str, n: usize, max: usize) -> Result<()> {
    if n > max {
        return Err(refuse(
            "COMPOSITION_LIMIT",
            format!("{n} {what}; at most {max} are accepted"),
        ));
    }
    Ok(())
}

/// A JSON value from a value or a string holding it, refused over
/// [`MAX_DOCUMENT_BYTES`].
pub fn value_arg(v: &Value, what: &str) -> Result<Value> {
    let out = match v {
        Value::String(s) => {
            if s.len() > MAX_DOCUMENT_BYTES {
                return Err(refuse(
                    "DOCUMENT_TOO_LARGE",
                    format!("{what} is {} bytes; at most {MAX_DOCUMENT_BYTES}", s.len()),
                ));
            }
            serde_json::from_str(s)
                .map_err(|e| Error::BadArgs(format!("{what}: not valid JSON: {e}")))?
        }
        other => other.clone(),
    };
    let n = out.to_string().len();
    if n > MAX_DOCUMENT_BYTES {
        return Err(refuse(
            "DOCUMENT_TOO_LARGE",
            format!("{what} is {n} bytes; at most {MAX_DOCUMENT_BYTES}"),
        ));
    }
    Ok(out)
}

// ------------------------------------------------------------ state

/// Where a composition came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Origin {
    /// Built from catalog instances (`bundle.open`).
    #[default]
    Catalog,
    /// Loaded from a `PatternComposition` document.
    Document,
    /// Loaded from a raw `PetriNetBundle`.
    Bundle,
}

impl Origin {
    pub fn name(self) -> &'static str {
        match self {
            Origin::Catalog => "catalog",
            Origin::Document => "document",
            Origin::Bundle => "bundle",
        }
    }
}

/// Which document form to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Form {
    Composition,
    Bundle,
}

impl Form {
    pub fn name(self) -> &'static str {
        match self {
            Form::Composition => "composition",
            Form::Bundle => "bundle",
        }
    }
}

/// Where the attached content came from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ContentSource {
    #[default]
    Bare,
    /// A library entry (`content::library`), by id.
    Library(String),
    /// Loaded by `content.load` / `bundle.load {content}`.
    Loaded,
}

impl ContentSource {
    pub fn label(&self) -> String {
        match self {
            ContentSource::Bare => "none".into(),
            ContentSource::Library(id) => format!("library:{id}"),
            ContentSource::Loaded => "loaded".into(),
        }
    }
}

/// What a composition carries beyond its bundle: where it came from, the
/// document text a raw load must give back, and the content attached to it.
#[derive(Debug, Clone, Default)]
pub struct DocState {
    pub origin: Origin,
    pub name: String,
    pub description: String,
    /// The Bundle JSON as received (form b); an unedited export returns it,
    /// an edited one is merged from it ([`Composition::to_bundle_document`]).
    pub original: Option<Value>,
    /// What the parsed bundle serialised to at load, part references
    /// resolved: the composition is unedited while it still does.
    pub baseline: Option<Value>,
    /// Form (a): each instance's `params` text as received, so a sealed
    /// string is written back as it came and never re-canonicalised.
    pub params_text: BTreeMap<String, String>,
    pub content: Option<ModelContent>,
    pub content_source: ContentSource,
}

impl DocState {
    /// Attaches library content when the instances include the library
    /// entry's anchor set (orders, workers, payment).
    pub fn attach_library_for(&mut self, instances: &[String]) {
        if let Some(lib) = content::library_for_instances(instances) {
            self.content = Some(lib.content.clone());
            self.content_source = ContentSource::Library(lib.id.to_string());
        }
    }
}

// ------------------------------------------------------------ form (a): in

fn obj<'a>(v: &'a Value, what: &str) -> Result<&'a Map<String, Value>> {
    v.as_object()
        .ok_or_else(|| refuse("COMPOSITION_FIELD", format!("{what} must be an object")))
}

fn text_field(m: &Map<String, Value>, k: &str, what: &str) -> Result<String> {
    m.get(k)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            refuse(
                "COMPOSITION_FIELD",
                format!("{what}: {k:?} must be a string"),
            )
        })
}

/// An optional string field: absent or `null` is `None`; anything else that is not a string is
/// refused, because a mistyped value would be dropped by the next export.
fn opt_text(m: &Map<String, Value>, k: &str, what: &str) -> Result<Option<String>> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(refuse(
            "COMPOSITION_FIELD",
            format!("{what}: {k:?} must be a string"),
        )),
    }
}

fn only_keys(m: &Map<String, Value>, allowed: &[&str], what: &str) -> Result<()> {
    for k in m.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(refuse(
                "COMPOSITION_FIELD",
                format!("{what}: unknown key {k:?} (it would not survive an export)"),
            ));
        }
    }
    Ok(())
}

fn check_context(m: &Map<String, Value>) -> Result<()> {
    match m.get("@context").and_then(Value::as_str) {
        Some(CONTEXT) => Ok(()),
        other => Err(refuse(
            "DOCUMENT_CONTEXT",
            format!("@context must be {CONTEXT:?}, not {other:?}"),
        )),
    }
}

/// `params` as the object a constructor takes: an object, or a string
/// holding one.
fn params_object(v: &Value, id: &str) -> Result<Value> {
    let p = match v {
        Value::String(s) => serde_json::from_str::<Value>(s).map_err(|e| {
            refuse(
                "PARAMS_INVALID",
                format!("instance {id:?}: params is not JSON text: {e}"),
            )
        })?,
        o => o.clone(),
    };
    if p.is_object() {
        Ok(p)
    } else {
        Err(refuse(
            "PARAMS_INVALID",
            format!("instance {id:?}: params must be an object"),
        ))
    }
}

/// An instance of a `PatternComposition` document as the spec the
/// constructor builds from. `extra` is the catalog's flag when the instance
/// is exactly a catalog extra.
fn spec_from_json(inst: &Value) -> Result<InstanceSpec> {
    let m = obj(inst, "an instance")?;
    only_keys(m, &["id", "pattern", "label", "why", "params"], "instance")?;
    let id = text_field(m, "id", "instance")?;
    if id.is_empty() || id.contains(':') || id.contains('/') {
        return Err(refuse(
            "INSTANCE_ID",
            format!("instance id {id:?} must be non-empty and contain neither ':' nor '/'"),
        ));
    }
    let pattern = text_field(m, "pattern", &format!("instance {id:?}"))?;
    if !PATTERNS.contains(&pattern.as_str()) {
        return Err(refuse(
            "PATTERN_UNKNOWN",
            format!("instance {id:?}: no pattern {pattern:?}; there are {PATTERNS:?}"),
        ));
    }
    let params = params_object(
        m.get("params").ok_or_else(|| {
            refuse(
                "COMPOSITION_FIELD",
                format!("instance {id:?}: params is required"),
            )
        })?,
        &id,
    )?;
    let what = format!("instance {id:?}");
    let label = opt_text(m, "label", &what)?.unwrap_or_else(|| id.clone());
    let why = opt_text(m, "why", &what)?.unwrap_or_default();
    let extra = instance(&id).is_some_and(|c| {
        c.extra && c.pattern == pattern && canonical_json(&c.params) == canonical_json(&params)
    });
    Ok(InstanceSpec {
        id,
        pattern,
        label,
        extra,
        params,
        why,
    })
}

/// A link of a document: `{kind, from, to}` and pflow-compose's optional
/// `id`, `rename`, `condition`, `lowering`.
pub fn link_from_json(v: &Value) -> Result<Link> {
    let m = obj(v, "a link")?;
    only_keys(
        m,
        &[
            "kind",
            "from",
            "to",
            "id",
            "rename",
            "condition",
            "lowering",
        ],
        "link",
    )?;
    let kind =
        parse_kind(&text_field(m, "kind", "link")?).map_err(|e| refuse("COMPOSITION_FIELD", e))?;
    let end = |k: &str| {
        let v = m.get(k).unwrap_or(&Value::Null);
        if let Value::Object(o) = v {
            only_keys(
                o,
                &["subnet", "port", "place", "transition"],
                &format!("link {k}"),
            )?;
        }
        parse_endpoint(v).map_err(|e| refuse("COMPOSITION_FIELD", format!("link {k}: {e}")))
    };
    let mut l = Link {
        kind,
        from: end("from")?,
        to: end("to")?,
        ..Default::default()
    };
    if let Some(s) = opt_text(m, "id", "link")? {
        l.id = s;
    }
    if let Some(s) = opt_text(m, "condition", "link")? {
        l.condition = s;
    }
    if let Some(s) = opt_text(m, "lowering", "link")? {
        l.lowering = s;
    }
    if let Some(r) = m.get("rename") {
        let r = obj(r, "link rename")?;
        for (k, v) in r {
            let v = v
                .as_str()
                .ok_or_else(|| refuse("COMPOSITION_FIELD", "link rename values must be strings"))?;
            l.rename.insert(k.clone(), v.into());
        }
    }
    Ok(l)
}

fn arc_merge_from(v: &str) -> Result<ArcMergePolicy> {
    match v {
        "sum" => Ok(ArcMergePolicy::Sum),
        "max" => Ok(ArcMergePolicy::Max),
        other => Err(refuse(
            "COMPOSITION_FIELD",
            format!("arc_merge must be \"sum\" or \"max\", not {other:?}"),
        )),
    }
}

impl Composition {
    /// Builds a composition from a `PatternComposition` document: every
    /// instance by its constructor, every link through
    /// [`Composition::add_link`] (so a refused link refuses the document).
    pub fn from_composition_document(doc: &Value) -> Result<Composition> {
        let m = obj(doc, "the document")?;
        check_context(m)?;
        if m.get("@type").and_then(Value::as_str) != Some(TYPE_COMPOSITION) {
            return Err(refuse(
                "COMPOSITION_TYPE",
                format!("@type must be {TYPE_COMPOSITION:?}"),
            ));
        }
        only_keys(
            m,
            &[
                "@context",
                "@id",
                "@type",
                "name",
                "description",
                "namespace",
                "arc_merge",
                "instances",
                "links",
            ],
            "document",
        )?;
        let insts = m
            .get("instances")
            .and_then(Value::as_array)
            .ok_or_else(|| refuse("COMPOSITION_FIELD", "instances must be an array"))?;
        within("instances", insts.len(), MAX_INSTANCES)?;
        if let Some(links) = m.get("links").and_then(Value::as_array) {
            within("links", links.len(), MAX_LINKS)?;
        }
        // After every refusal that costs nothing: sealing the received JSON is
        // the expensive step, and a document that is refused for its keys or
        // its size must not be canonicalised first.
        if let Some(id) = m.get("@id") {
            // `store.get` returns the document with the `@id` the store
            // added; the seal ignores a top-level `@id`, so accept the one
            // that is this document's CID (an export drops it).
            let want = crate::cid::seal(doc)?;
            if id.as_str() != Some(want.as_str()) {
                return Err(refuse(
                    "COMPOSITION_FIELD",
                    format!("document: \"@id\" must be the CID of the document ({want}) or absent"),
                ));
            }
        }
        let mut c = Composition::open(&[], &[]).map_err(Error::Invalid)?;
        for inst in insts {
            let spec = spec_from_json(inst)?;
            if let Some(text) = inst.get("params").and_then(Value::as_str) {
                c.doc.params_text.insert(spec.id.clone(), text.to_string());
            }
            if c.spec(&spec.id).is_some() {
                return Err(refuse(
                    "INSTANCE_DUPLICATE",
                    format!("instance {:?} appears twice", spec.id),
                ));
            }
            let sub = build(&spec)
                .map_err(|e| refuse("INSTANCE_BUILD", format!("instance {:?}: {e}", spec.id)))?;
            if sub.id != spec.id {
                return Err(refuse(
                    "INSTANCE_ID",
                    format!(
                        "instance {:?}: its params build the subnet {:?}; the ids must agree",
                        spec.id, sub.id
                    ),
                ));
            }
            c.bundle.add_subnet(sub);
            c.specs.push(spec);
        }
        if let Some(links) = m.get("links") {
            let links = links
                .as_array()
                .ok_or_else(|| refuse("COMPOSITION_FIELD", "links must be an array"))?;
            for l in links {
                let l = link_from_json(l)?;
                let key = link_key(&l);
                c.add_link(l)
                    .map_err(|v| refuse("LINK_REFUSED", format!("{key}: {}", v.refusal())))?;
            }
        }
        let name = opt_text(m, "name", "document")?.unwrap_or_else(|| "composition".into());
        c.bundle.name = name.clone();
        c.bundle.description = opt_text(m, "description", "document")?.unwrap_or_default();
        if let Some(b) = m.get("namespace") {
            c.bundle.namespace = Some(
                b.as_bool()
                    .ok_or_else(|| refuse("COMPOSITION_FIELD", "namespace must be a boolean"))?,
            );
        }
        if let Some(a) = m.get("arc_merge") {
            c.bundle.arc_merge =
                Some(arc_merge_from(a.as_str().ok_or_else(|| {
                    refuse("COMPOSITION_FIELD", "arc_merge must be a string")
                })?)?);
        }
        c.doc = DocState {
            origin: Origin::Document,
            name,
            description: c.bundle.description.clone(),
            params_text: std::mem::take(&mut c.doc.params_text),
            ..Default::default()
        };
        Ok(c)
    }

    /// The `params` text an instance is written with: the string received
    /// when it still says what the spec says (a sealed string is never
    /// re-canonicalised), else the canonical text of the spec's value.
    fn params_text(&self, s: &InstanceSpec) -> String {
        self.doc
            .params_text
            .get(&s.id)
            .filter(|t| serde_json::from_str::<Value>(t).is_ok_and(|v| v == s.params))
            .cloned()
            .unwrap_or_else(|| canonical_json(&s.params))
    }

    /// The `PatternComposition` document of this composition: instances in
    /// order, links in order. Refused when an instance has no pattern (it
    /// came from a raw Bundle).
    pub fn to_composition_document(&self) -> Result<Value> {
        if let Some(s) = self.specs.iter().find(|s| s.pattern.is_empty()) {
            return Err(refuse(
                "EXPORT_NEEDS_PATTERNS",
                format!(
                    "subnet {:?} has no pattern (it came from a raw Bundle); export it as a bundle",
                    s.id
                ),
            ));
        }
        let instances: Vec<Value> = self
            .specs
            .iter()
            .map(|s| {
                let mut v = json!({
                    "id": s.id,
                    "pattern": s.pattern,
                    "params": self.params_text(s),
                });
                if s.label != s.id {
                    v["label"] = json!(s.label);
                }
                if !s.why.is_empty() {
                    v["why"] = json!(s.why);
                }
                v
            })
            .collect();
        let mut doc = json!({
            "@context": CONTEXT,
            "@type": TYPE_COMPOSITION,
            "name": self.bundle.name,
            "instances": instances,
        });
        if !self.bundle.description.is_empty() {
            doc["description"] = json!(self.bundle.description);
        }
        if let Some(b) = self.bundle.namespace {
            doc["namespace"] = json!(b);
        }
        if let Some(a) = self.bundle.arc_merge {
            doc["arc_merge"] = json!(match a {
                ArcMergePolicy::Sum => "sum",
                ArcMergePolicy::Max => "max",
            });
        }
        if !self.bundle.links.is_empty() {
            doc["links"] = json!(self
                .bundle
                .links
                .iter()
                .map(link_to_json)
                .collect::<Vec<_>>());
        }
        Ok(doc)
    }
}

/// A link as a document writes it.
pub fn link_to_json(l: &Link) -> Value {
    let end = |e: &pflow_compose::Endpoint| {
        let mut v = json!({"subnet": e.subnet});
        for (k, x) in [
            ("port", &e.port),
            ("place", &e.place),
            ("transition", &e.transition),
        ] {
            if !x.is_empty() {
                v[k] = json!(x);
            }
        }
        v
    };
    let mut v = json!({"kind": kind_name(l.kind), "from": end(&l.from), "to": end(&l.to)});
    if !l.id.is_empty() {
        v["id"] = json!(l.id);
    }
    if !l.condition.is_empty() {
        v["condition"] = json!(l.condition);
    }
    if !l.lowering.is_empty() {
        v["lowering"] = json!(l.lowering);
    }
    if !l.rename.is_empty() {
        let sorted: BTreeMap<&String, &String> = l.rename.iter().collect();
        v["rename"] = json!(sorted);
    }
    v
}

// ------------------------------------------------------------ form (b)

/// How [`Composition::from_bundle_document`] finds a subnet that is only a
/// reference (`cid`, no `model`): the stored net's JSON.
pub type PartResolver<'a> = &'a dyn Fn(&str) -> Option<Value>;

fn keys_not_kept(orig: &Value, kept: &Value, path: &str, out: &mut Vec<String>) {
    match (orig, kept) {
        (Value::Object(o), Value::Object(k)) => {
            for (key, v) in o {
                let p = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match k.get(key) {
                    Some(kv) => keys_not_kept(v, kv, &p, out),
                    None if !is_empty_value(v) => out.push(p),
                    None => {}
                }
            }
        }
        (Value::Array(o), Value::Array(k)) => {
            for (i, (a, b)) in o.iter().zip(k).enumerate() {
                keys_not_kept(a, b, &format!("{path}[{i}]"), out);
            }
        }
        _ => {}
    }
}

fn is_empty_value(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Object(m) => m.is_empty(),
        _ => false,
    }
}

impl Composition {
    /// Opens a raw `PetriNetBundle`. Validation errors do not block the
    /// load (the report is `bundle.validate`'s); a malformed document, the
    /// `tokenmodel/subnet` shape, and a part reference that cannot be
    /// resolved do.
    pub fn from_bundle_document(doc: &Value, resolve: PartResolver) -> Result<Composition> {
        let m = obj(doc, "the document")
            .map_err(|_| refuse("DOCUMENT_TYPE", "a Bundle document must be an object"))?;
        match m.get("@type").and_then(Value::as_str) {
            Some(TYPE_BUNDLE) => {}
            None if m.contains_key("subnets") => {}
            other => {
                return Err(refuse(
                    "DOCUMENT_TYPE",
                    format!("@type must be {TYPE_COMPOSITION:?} or {TYPE_BUNDLE:?}, not {other:?}"),
                ))
            }
        }
        if m.get("links").and_then(Value::as_array).is_some_and(|ls| {
            ls.iter()
                .any(|l| l.get("from_subnet").is_some() || l.get("to_subnet").is_some())
        }) {
            return Err(refuse(
                "BUNDLE_SHAPE_TOKENMODEL",
                "this is go-pflow's tokenmodel/subnet bundle (links use from_subnet / to_subnet), \
                 which means something else than metamodel.Bundle; it is not loaded",
            ));
        }
        check_context(m)?;
        for (key, what, max) in [
            ("subnets", "subnets", MAX_INSTANCES),
            ("links", "links", MAX_LINKS),
        ] {
            if let Some(a) = m.get(key).and_then(Value::as_array) {
                within(what, a.len(), max)?;
            }
        }
        // Resolve part references into inline subnets before parsing.
        let mut resolved = doc.clone();
        if let Some(subs) = resolved.get_mut("subnets").and_then(Value::as_array_mut) {
            for (i, s) in subs.iter_mut().enumerate() {
                let (has_model, cid) = (
                    s.get("model").is_some(),
                    s.get("cid").and_then(Value::as_str).map(str::to_string),
                );
                if let (false, Some(cid)) = (has_model, cid) {
                    let net = resolve(&cid).ok_or_else(|| {
                        refuse(
                            "BUNDLE_PART_UNRESOLVED",
                            format!(
                                "subnets[{i}] refers to {cid}, which is not in the local store \
                                 (store.put the net, or inline it)"
                            ),
                        )
                    })?;
                    let sub = subnet_from_net(s, &net).map_err(|e| {
                        refuse("BUNDLE_PART_UNRESOLVED", format!("subnets[{i}] {cid}: {e}"))
                    })?;
                    // Resolved only to flatten and draw: the received subnet
                    // (its `cid`, and any key beside it) is what an export
                    // gives back. Keys beside the `cid` ride along so
                    // `preserved_keys` still names them.
                    let mut inlined = serde_json::to_value(&sub).expect("subnet serializes");
                    if let (Some(from), Some(to)) = (s.as_object(), inlined.as_object_mut()) {
                        for (k, v) in from {
                            if k != "cid" && !to.contains_key(k) {
                                to.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    *s = inlined;
                }
            }
        }
        let bundle: Bundle =
            serde_json::from_value(resolved.clone()).map_err(|e| refuse("BUNDLE_PARSE", e))?;
        let specs: Vec<InstanceSpec> = bundle
            .subnets
            .iter()
            .map(|s| InstanceSpec {
                id: s.id.clone(),
                pattern: String::new(),
                label: s.id.clone(),
                extra: false,
                params: json!({}),
                why: String::new(),
            })
            .collect();
        let mut c = Composition::open(&[], &[]).map_err(Error::Invalid)?;
        c.doc = DocState {
            origin: Origin::Bundle,
            name: bundle.name.clone(),
            description: bundle.description.clone(),
            original: Some(doc.clone()),
            baseline: Some(serde_json::to_value(&bundle).expect("bundle serializes")),
            ..Default::default()
        };
        c.bundle = bundle;
        c.specs = specs;
        Ok(c)
    }

    /// The Bundle JSON of this composition. A raw load gives back what it
    /// received: the document itself while it is unedited (so its CID is the
    /// seal of the bytes received, whatever was resolved to draw it), and,
    /// once edited, what pflow-compose serialises with every part, link and
    /// key the edit did not touch written as it was received. Anything else
    /// is what pflow-compose serialises.
    pub fn to_bundle_document(&self) -> Value {
        let cur = serde_json::to_value(&self.bundle).expect("bundle serializes");
        match (&self.doc.original, &self.doc.baseline) {
            (Some(orig), Some(base)) if *base == cur => orig.clone(),
            (Some(orig), Some(base)) => merge_received(orig, base, cur),
            _ => cur,
        }
    }

    /// Whether the bundle is what the raw document parsed to.
    pub fn unedited(&self) -> bool {
        self.doc.baseline.as_ref().is_some_and(|b| {
            serde_json::to_value(&self.bundle)
                .ok()
                .is_some_and(|cur| *b == cur)
        })
    }

    /// JSON paths of keys the received document has that this build writes
    /// back but does not interpret (`subnets[0].pattern`, a top-level
    /// `@id`). Read from what an export gives now, so an edit that removes
    /// the part or link carrying a key drops it from the list. A part
    /// reference's `cid` is not one: it is how the part was resolved.
    pub fn preserved_keys(&self) -> Vec<String> {
        if self.doc.original.is_none() {
            return vec![];
        }
        let cur = serde_json::to_value(&self.bundle).expect("bundle serializes");
        let exported = self.to_bundle_document();
        let mut out = vec![];
        keys_not_kept(&exported, &cur, "", &mut out);
        let subs = exported.get("subnets").and_then(Value::as_array);
        out.retain(|p| {
            let by_ref = p
                .strip_prefix("subnets[")
                .and_then(|r| r.strip_suffix("].cid"))
                .and_then(|i| i.parse::<usize>().ok())
                .and_then(|i| subs.and_then(|s| s.get(i)))
                .is_some_and(|s| s.get("model").is_none());
            !by_ref
        });
        out
    }
}

/// An edited raw load, exported: `cur` (what pflow-compose serialises) with
/// what the received document had and the edit left alone put back. A
/// subnet or link unchanged since load is the received JSON, so a part keeps
/// its `cid` and a recipe or extra key survives; a changed one gets the
/// unknown keys its received form had; top-level unknown keys are kept.
fn merge_received(orig: &Value, base: &Value, mut cur: Value) -> Value {
    let unknown = |from: &Value, kept: &Value, to: &mut Value| {
        if let (Some(f), Some(k), Some(t)) =
            (from.as_object(), kept.as_object(), to.as_object_mut())
        {
            for (key, v) in f {
                if !k.contains_key(key) && !t.contains_key(key) && !is_empty_value(v) {
                    t.insert(key.clone(), v.clone());
                }
            }
        }
    };
    let list = |v: &'_ Value, k: &str| -> Vec<Value> {
        v.get(k)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };
    for key in ["subnets", "links"] {
        let (o, b) = (list(orig, key), list(base, key));
        let Some(arr) = cur.get_mut(key).and_then(Value::as_array_mut) else {
            continue;
        };
        for item in arr.iter_mut() {
            let same = |x: &Value| key == "links" || x.get("id") == item.get("id");
            let at = b.iter().position(|x| same(x) && x == item);
            if let Some(i) = at.filter(|i| *i < o.len()) {
                *item = o[i].clone();
            } else if key == "subnets" {
                if let Some(i) = b.iter().position(same).filter(|i| *i < o.len()) {
                    let mut with = item.clone();
                    unknown(&o[i], &b[i], &mut with);
                    if let Some(m) = with.as_object_mut() {
                        m.remove("cid");
                    }
                    *item = with;
                }
            }
        }
    }
    unknown(orig, base, &mut cur);
    cur
}

/// A subnet from a stored net: the net compiled to a metamodel `Model`,
/// under the reference's `id` and `net_type`.
fn subnet_from_net(reference: &Value, net: &Value) -> Result<Subnet> {
    let doc = crate::doc::Document::from_value(net).map_err(|e| e.to_string());
    let doc = doc.map_err(Error::Invalid)?;
    let compiled = crate::sim::Compiled::new(&doc)?;
    let mut v = json!({
        "@type": "PetriNet",
        "id": reference.get("id").cloned().unwrap_or(Value::Null),
        "model": serde_json::to_value(&compiled.model).map_err(|e| Error::Parse(e.to_string()))?,
    });
    if let Some(t) = reference.get("net_type") {
        v["net_type"] = t.clone();
    }
    serde_json::from_value(v).map_err(|e| Error::Parse(e.to_string()))
}

// ------------------------------------------------------------ load / export

/// A document loaded: the composition and the form it was.
pub struct Loaded {
    pub form: Form,
    pub composition: Composition,
}

/// Loads either form, by `@type`.
pub fn load(doc: &Value, resolve: PartResolver) -> Result<Loaded> {
    let ty = doc.get("@type").and_then(Value::as_str);
    if ty == Some(TYPE_COMPOSITION) {
        let mut composition = Composition::from_composition_document(doc)?;
        // A document that is a library example gets the library's content
        // back: the content records the CID it was written against. That
        // is the CID of the document the composition exports (params as
        // canonical text), not of the spelling received: `params` as an
        // object is the same composition and must find the same content.
        if let Some(lib) = composition
            .export(Some(Form::Composition))
            .ok()
            .and_then(|e| content::library_for_cid(&e.cid))
        {
            composition.doc.content = Some(lib.content.clone());
            composition.doc.content_source = ContentSource::Library(lib.id.to_string());
        }
        Ok(Loaded {
            form: Form::Composition,
            composition,
        })
    } else {
        Ok(Loaded {
            form: Form::Bundle,
            composition: Composition::from_bundle_document(doc, resolve)?,
        })
    }
}

/// The composition as `session.dump_state` keeps it: the document (either
/// form, so a loaded Bundle or a non-catalog composition survives), plus the
/// content when it is not the library's (the library's is a name).
pub fn dump_part(c: &Composition) -> Result<Value> {
    let exp = c.export(None)?;
    // `instances` and `links` are the shape this part had when it saved
    // catalog ids only; a reader of that shape still reads them.
    let mut v = json!({
        "instances": c.specs.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        "links": c.bundle.links.iter().map(super::link_json).collect::<Vec<_>>(),
        "form": exp.form.name(),
        "origin": c.doc.origin.name(),
        "document": exp.document,
    });
    match (&c.doc.content_source, &c.doc.content) {
        (ContentSource::Library(id), _) => v["library"] = json!(id),
        (ContentSource::Loaded, Some(k)) => v["content"] = k.to_document(),
        _ => {}
    }
    Ok(v)
}

/// Reads [`dump_part`]'s shape back (and the older `{instances, links}` of
/// catalog ids). Content that no longer resolves is attached dormant.
pub fn restore_part(v: &Value, resolve: PartResolver) -> Result<Composition> {
    if v.get("document").is_none() && v.get("instances").is_some() {
        let ids: Vec<String> = serde_json::from_value(v["instances"].clone())
            .map_err(|e| Error::BadArgs(format!("composition instances: {e}")))?;
        let links = v["links"]
            .as_array()
            .into_iter()
            .flatten()
            .map(parse_link_value)
            .collect::<Result<Vec<_>>>()?;
        return Composition::open(&ids, &links).map_err(Error::Invalid);
    }
    let doc = v
        .get("document")
        .ok_or_else(|| Error::BadArgs("composition: `document` is required".into()))?;
    let mut c = load(doc, resolve)?.composition;
    // A catalog-built composition is the same document as one loaded from its export, but it is
    // not read the same: the parameters are this page's choice only for the catalog's, and
    // `bundle.state` says so (`params_note`). The part says where it came from, as main's did by
    // restoring through `Composition::open`; a loaded document or a raw Bundle is what `load` made.
    if v.get("origin").and_then(Value::as_str) == Some(Origin::Catalog.name())
        && c.doc.origin == Origin::Document
    {
        c.doc.origin = Origin::Catalog;
    }
    if let Some(id) = v.get("library").and_then(Value::as_str) {
        let lib = content::library_entry(id)
            .ok_or_else(|| Error::NotFound(format!("no library content {id:?}")))?;
        c.doc.content = Some(lib.content.clone());
        c.doc.content_source = ContentSource::Library(id.to_string());
    } else if let Some(k) = v.get("content").filter(|k| !k.is_null()) {
        let k = ModelContent::parse(k)?;
        // A saved state is not trusted more than a document: the bounds hold, element ids need not
        // (content whose ids no longer resolve is attached dormant).
        let report = content::limit_report(&k);
        if !report.valid() {
            return Err(refuse("CONTENT_REFUSED", report.refusal()));
        }
        c.doc.content = Some(k);
        c.doc.content_source = ContentSource::Loaded;
    }
    Ok(c)
}

fn parse_link_value(l: &Value) -> Result<Link> {
    super::parse_link(l).map_err(|e| Error::BadArgs(format!("composition link: {e}")))
}

/// What `bundle.export` writes.
#[derive(Debug, Clone)]
pub struct Exported {
    pub form: Form,
    pub document: Value,
    pub cid: String,
    /// Form (a) only, and only from [`Composition::export_with_bundle_cid`]:
    /// the seal of the plain Bundle it flattens to.
    pub bundle_cid: Option<String>,
    /// Why `bundle_cid` is missing from an export that asked for it: the
    /// flattened Bundle is too large to seal (`[DOCUMENT_TOO_COMPLEX]`).
    pub bundle_cid_error: Option<String>,
}

impl Composition {
    /// The form `export(None)` writes: a raw load stays a bundle; a
    /// composition whose every instance has a pattern is form (a).
    pub fn natural_form(&self) -> Form {
        if self.doc.origin == Origin::Bundle || self.specs.iter().any(|s| s.pattern.is_empty()) {
            Form::Bundle
        } else {
            Form::Composition
        }
    }

    /// The document and its CID. Sealing is the cost: a form (a) document
    /// holds its parameters as text and seals in a moment, a Bundle is
    /// sealed whole (and refused with `[DOCUMENT_TOO_COMPLEX]` past
    /// [`crate::cid::seal_work_limit`]). `bundle_cid` is left to
    /// [`Composition::export_with_bundle_cid`].
    pub fn export(&self, form: Option<Form>) -> Result<Exported> {
        let form = form.unwrap_or_else(|| self.natural_form());
        let document = match form {
            Form::Composition => self.to_composition_document()?,
            Form::Bundle => self.to_bundle_document(),
        };
        let cid = crate::cid::seal(&document)?;
        Ok(Exported {
            form,
            document,
            cid,
            bundle_cid: None,
            bundle_cid_error: None,
        })
    }

    /// [`Composition::export`] and, for form (a), the seal of the Bundle it
    /// flattens to. That seal is the expensive one (a saga's flattened arcs
    /// are a long list), only a person reading the result wants it, and
    /// `bundle.load`, `bundle.export` and `store.put` are the commands that
    /// show it: the CID receipts name, the autosave and the content checks
    /// ([`Composition::document_cid`], [`dump_part`]) do not pay for it.
    pub fn export_with_bundle_cid(&self, form: Option<Form>) -> Result<Exported> {
        let mut e = self.export(form)?;
        if e.form == Form::Composition {
            match crate::cid::seal(&self.to_bundle_document()) {
                Ok(cid) => e.bundle_cid = Some(cid),
                Err(err) => e.bundle_cid_error = Some(err.to_string()),
            }
        }
        Ok(e)
    }

    /// The CID `receipts` and the store name this composition by.
    pub fn document_cid(&self) -> Result<String> {
        Ok(self.export(None)?.cid)
    }
}

// ------------------------------------------------------------ what is missing

/// What this composition cannot show, written for a person
/// (docs/composition-documents.md §2).
pub fn missing(c: &Composition) -> Vec<Value> {
    let mut out = vec![];
    let mut add = |code: &str, text: String| out.push(json!({"code": code, "text": text}));
    let bare: Vec<&str> = c
        .specs
        .iter()
        .filter(|s| s.pattern.is_empty())
        .map(|s| s.id.as_str())
        .collect();
    if !bare.is_empty() {
        add(
            "NO_PATTERN",
            format!(
                "{} of {} subnets carry no pattern ({}): no ontology keys on their ports, no idiom or \
                 leak lighting, no Operad holes or pattern hovers for them",
                bare.len(),
                c.specs.len(),
                bare.join(", ")
            ),
        );
    }
    let portless: Vec<&str> = c
        .bundle
        .subnets
        .iter()
        .filter(|s| s.ports.is_empty())
        .map(|s| s.id.as_str())
        .collect();
    if !portless.is_empty() && !c.bundle.subnets.is_empty() {
        add(
            "NO_PORTS",
            format!(
                "{} subnet(s) declare no ports ({}): nothing to drag a link from or to, so snap-together \
                 offers no candidates there; a link already in the document is drawn between ports only \
                 where both its ends are ports, and the Flat net, Boundaries and Laws views read every link \
                 through Flatten",
                portless.len(),
                portless.join(", ")
            ),
        );
    }
    let scenarios = c.doc.content.as_ref().map_or(0, |k| k.scenarios().len());
    if scenarios == 0 {
        add(
            "NO_SCENARIOS",
            "no scenario is declared for this model: the Process view has no run to draw, the token game \
             has none to step, and the lens test has no commit run to read as its forward pass, so it \
             gives no verdict; the flat net and incidence still work"
                .into(),
        );
    }
    if let Some(k) = &c.doc.content {
        let dormant = content::dormant_count(k, c);
        if dormant > 0 {
            add(
                "DORMANT",
                format!(
                    "{dormant} of {} content entries name elements this model does not have (it was \
                     edited after the content was written, or the content is for another model): they are \
                     kept with the content and no view shows them",
                    k.entries.len()
                ),
            );
        }
    }
    if c.doc.content.is_none() {
        add(
            "NO_CONTENT",
            "no model content is attached: no findings, notes or convention examples for this model, \
             only what the views can derive from the net"
                .into(),
        );
    }
    add(
        "NO_LAYOUT",
        "the document stores no positions; the views compute their own layout".into(),
    );
    if super::flatten(&c.bundle).is_err() {
        add(
            "FLATTEN_FAILED",
            "pflow-compose cannot flatten this composition (bundle.flatten gives its reason): the flat \
             net, incidence and the token game are unavailable"
                .into(),
        );
    }
    let preserved = c.preserved_keys();
    if !preserved.is_empty() {
        add(
            "KEYS_PRESERVED",
            format!(
                "the document has keys this build keeps but does not interpret: {}",
                preserved.join(", ")
            ),
        );
    }
    out
}

/// A summary of what a view can rely on, as booleans and counts.
pub fn capabilities(c: &Composition) -> Value {
    let patterned = !c.specs.is_empty() && c.specs.iter().all(|s| !s.pattern.is_empty());
    let ports: usize = c.bundle.subnets.iter().map(|s| s.ports.len()).sum();
    // A link joins two subnets: one subnet with ports has nothing to snap to.
    let with_ports = c
        .bundle
        .subnets
        .iter()
        .filter(|s| !s.ports.is_empty())
        .count();
    json!({
        "flatten": super::flatten(&c.bundle).is_ok(),
        "patterns": patterned,
        "ontology_keys": patterned,
        "idioms": patterned,
        "ports": ports,
        "snap_together": with_ports >= 2,
        "scenarios": c.doc.content.as_ref().map_or(0, |k| k.scenarios().len()),
        "content_entries": c.doc.content.as_ref().map_or(0, |k| k.entries.len()),
    })
}

/// `bundle.state`'s `document` object.
pub fn document_json(c: &Composition) -> Value {
    json!({
        "form": c.natural_form().name(),
        "origin": c.doc.origin.name(),
        "name": c.bundle.name,
        "description": c.bundle.description,
        "preserved_keys": c.preserved_keys(),
        "edited": c.doc.original.is_some() && !c.unedited(),
    })
}

/// `bundle.state`'s `content` summary.
pub fn content_json(c: &Composition) -> Value {
    match &c.doc.content {
        None => json!({"source": "none", "entries": 0}),
        Some(k) => {
            let report = content::validate(k, c, false);
            let mut kinds = serde_json::Map::new();
            for e in &k.entries {
                let n = kinds.get(&e.kind).and_then(Value::as_i64).unwrap_or(0);
                kinds.insert(e.kind.clone(), json!(n + 1));
            }
            json!({
                "source": c.doc.content_source.label(),
                "name": k.name,
                "entries": k.entries.len(),
                "kinds": kinds,
                "unresolved": report.unresolved(),
                "warnings": report.warnings.iter().map(content::Issue::to_json).collect::<Vec<_>>(),
                "dormant": content::dormant_count(k, c),
                "scenarios": k.scenarios().iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
            })
        }
    }
}

/// The kind name of a link kind (re-exported for the commands).
pub fn link_kind_name(k: LinkKind) -> &'static str {
    kind_name(k)
}

/// The catalog's instance ids, for error messages.
pub fn catalog_ids() -> Vec<&'static str> {
    catalog().iter().map(|s| s.id.as_str()).collect()
}

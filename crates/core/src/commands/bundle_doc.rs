//! `bundle.load` and `bundle.export`: the two composition document forms
//! (docs/composition-documents.md §1), and the helpers the store and the
//! content commands share. The document logic lives in
//! [`crate::composition::document`]; sealing is [`crate::cid::seal`] on the
//! exported JSON and nothing else.

use serde_json::{json, Value};

use crate::artifact::ArtifactData;
use crate::composition::content::{self, DeclaredScenario, ModelContent};
use crate::composition::document::{self, refuse, value_arg, Form, Loaded};
use crate::composition::{self as comp, Composition};
use crate::registry::{always, b, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::store;
use crate::{Error, Result};

fn need(sess: &Session) -> std::result::Result<(), String> {
    if sess.composition.is_some() {
        Ok(())
    } else {
        Err("no composition is open (bundle.open, bundle.load)".into())
    }
}

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "bundle.load",
        label: "Load Composition…",
        help: "Open a composition from a document, replacing any open one. `document` is either form: a \
               PatternComposition ({instances: [{id, pattern, params}], links}, rebuilt by pflow-compose's \
               constructors: what snap-together, port colours and idiom lighting need) or a raw PetriNetBundle \
               (loaded as written; views say what it lacks). Or `cid`, a document in the local store. \
               `content` (a ModelContent document) or `library` (\"orders\") attaches model content, refused \
               when an element id does not resolve unless `strict` is false. Returns the form, the document's \
               CID, the Bundle validation, whether it flattens, and what is missing. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[
            opt("document", Json, "a PatternComposition or PetriNetBundle document (or a string holding it)"),
            opt("cid", Str, "a composition or bundle document in the local store"),
            opt("content", Json, "a ModelContent document to attach"),
            opt("library", Str, "library content to attach (content.library)"),
            opt("strict", Bool, "refuse content whose element ids do not resolve (default true)"),
        ],
        enabled: always,
        run: load,
        journal: true,
    },
    CommandSpec {
        id: "bundle.export",
        label: "Export Composition",
        help: "The open composition as a document: `form` auto (the form it was loaded or built as), \
               composition (PatternComposition; refused for a raw Bundle's subnets, which have no pattern) or \
               bundle (the PetriNetBundle: the loaded JSON while a raw load is unedited, else what \
               pflow-compose serialises). Returns the document and its CID (pflow's seal of exactly that \
               JSON); form composition also gives `bundle_cid`, the seal of the plain Bundle it flattens to. \
               `content: true` adds the attached content document. Stores a report.",
        menu: "Compose",
        shortcut: None,
        args: &[
            opt("form", Str, "auto (default), composition or bundle"),
            opt("content", Bool, "also export the attached content"),
        ],
        enabled: need,
        run: export,
        journal: false,
    },
];

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

/// The resolver for `cid` subnets: a net in the local store. A net whose `@type` names its net
/// type (`ResourceNet`, as examples/producer-consumer.json) is a net too, though `kind_of` only
/// calls the plain `PetriNet` one so.
pub fn from_store(cid: &str) -> Option<Value> {
    let v = store::open().get_model(cid).ok()?;
    let is_net = match store::kind_of(&v) {
        store::DocKind::Net => true,
        store::DocKind::Unknown => v.get("places").is_some() && v.get("transitions").is_some(),
        _ => false,
    };
    is_net.then_some(v)
}

/// The report shape of `bundle.validate`.
pub fn validation_json(c: &Composition) -> Value {
    let res = c.bundle.validate();
    let issues = |v: &[pflow_compose::ValidationError]| -> Vec<Value> {
        v.iter()
            .map(|e| json!({"code": e.code, "message": e.message, "element": e.element}))
            .collect()
    };
    json!({"valid": res.valid, "errors": issues(&res.errors), "warnings": issues(&res.warnings)})
}

/// What `bundle.load` returns for a composition just built.
pub fn load_result(c: &Composition, form: Form) -> Result<Value> {
    let exp = c.export(Some(form))?;
    let flat = comp::flatten(&c.bundle).err();
    Ok(json!({
        "form": form.name(),
        "name": c.bundle.name,
        "cid": exp.cid,
        "bundle_cid": exp.bundle_cid,
        "instances": c.specs.iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
        "links": c.links_json(),
        "validation": validation_json(c),
        "flatten": match flat { None => json!({"ok": true}), Some(e) => json!({"ok": false, "error": e}) },
        "missing": document::missing(c),
        "content": document::content_json(c),
    }))
}

/// Builds the composition a `bundle.load` argument set describes (nothing
/// is opened).
pub fn build(a: &Value) -> Result<(Composition, Form)> {
    let doc = match (a.get("document").filter(|v| !v.is_null()), s(a, "cid")) {
        (Some(d), None) => value_arg(d, "document")?,
        (None, Some(cid)) => store::open().get_model(cid)?,
        _ => return Err(Error::BadArgs("pass `document` or `cid`".into())),
    };
    let strict = b(a, "strict").unwrap_or(true);
    let Loaded {
        form,
        composition: mut c,
    } = document::load(&doc, &from_store)?;
    if let Some(cd) = a.get("content").filter(|v| !v.is_null()) {
        attach(
            &mut c,
            ModelContent::parse(&value_arg(cd, "content")?)?,
            strict,
        )?;
    } else if let Some(lib) = s(a, "library") {
        let l = content::library_entry(lib).ok_or_else(|| {
            Error::NotFound(format!(
                "no library content {lib:?}; there is {:?}",
                content::library().iter().map(|l| l.id).collect::<Vec<_>>()
            ))
        })?;
        attach(&mut c, l.content.clone(), strict)?;
        c.doc.content_source = document::ContentSource::Library(lib.to_string());
    }
    Ok((c, form))
}

/// Validates `mc` against `c` and attaches it (refused on an error).
pub fn attach(c: &mut Composition, mc: ModelContent, strict: bool) -> Result<()> {
    if mc.subject.kind != "composition" {
        return Err(refuse(
            "CONTENT_SUBJECT_UNSUPPORTED",
            format!(
                "content about a {:?} cannot be attached to a composition",
                mc.subject.kind
            ),
        ));
    }
    let report = content::validate(&mc, c, strict);
    if !report.valid() {
        return Err(refuse("CONTENT_REFUSED", report.refusal()));
    }
    c.doc.content = Some(mc);
    c.doc.content_source = document::ContentSource::Loaded;
    Ok(())
}

fn load(sess: &mut Session, a: &Value) -> Result<Value> {
    let (c, form) = build(a)?;
    let out = load_result(&c, form)?;
    sess.edit("load composition", |s| {
        s.composition = Some(c);
        Ok(())
    })?;
    Ok(out)
}

fn export(sess: &mut Session, a: &Value) -> Result<Value> {
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
    let exp = c.export(form)?;
    let mut out = json!({
        "form": exp.form.name(),
        "document": exp.document,
        "cid": exp.cid,
        "bundle_cid": exp.bundle_cid,
    });
    if b(a, "content").unwrap_or(false) {
        out["content"] = match &c.doc.content {
            Some(k) => json!({"document": k.to_document(), "cid": k.cid()?}),
            None => Value::Null,
        };
    }
    let id = sess.add_artifact(
        format!("Composition document ({})", exp.form.name()),
        source("bundle.export", a),
        ArtifactData::Report(out.clone()),
    );
    out["artifact"] = json!(id);
    Ok(out)
}

// ------------------------------------------------------------ scenarios

/// The declared scenarios the views and `bundle.run` read: the open
/// composition's content, else (none open) the orders library's. The label
/// says where they came from.
pub fn declared_scenarios(sess: &Session) -> (String, Vec<DeclaredScenario>) {
    match &sess.composition {
        Some(c) => (
            c.doc.content_source.label(),
            c.doc
                .content
                .as_ref()
                .map(ModelContent::scenarios)
                .unwrap_or_default(),
        ),
        None => (
            "library:orders".into(),
            content::library_entry("orders")
                .map(|l| l.content.scenarios())
                .unwrap_or_default(),
        ),
    }
}

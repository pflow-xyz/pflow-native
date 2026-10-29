//! `content.*`: the curated content of a model (models-first D11;
//! docs/composition-documents.md §3): the findings, notes, declared
//! scenarios and convention examples a view reads instead of hard-coding,
//! keyed by the model's own element ids. The types, the validator and the
//! check runner are in [`crate::composition::content`] and
//! [`crate::composition::checks`].
//!
//! A composition's content lives with the composition (undo, clone and
//! export carry it). The Connect-4 plan's content is library-only.

use serde_json::{json, Value};

use crate::artifact::ArtifactData;
use crate::composition::checks::{self, Base};
use crate::composition::content::{self, Entry, Ids, ModelContent};
use crate::composition::document::{self, value_arg, ContentSource};
use crate::registry::{always, b, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::{Error, Result};

const MENU: &str = "Compose";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "content.library",
        label: "Content Library",
        help: "The content that ships with the build: {id, subject, name, entries, checks, scenarios}. \
               `orders` attaches to a composition that includes orders, workers and payment (bundle.open \
               with defaults); `connect4` is the search plan's.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: library,
        journal: false,
    },
    CommandSpec {
        id: "content.get",
        label: "Model Content",
        help: "The content a view reads: {source, subject, name, entries, total, dormant}. Default subject is \
               the open composition's content (`composition`); `plan` is the Connect-4 plan's. Entries whose \
               element ids do not resolve against the model are dormant and left out unless `all`. Filters: \
               `kind`, `about` (an element id an entry names), `view` (entries that name the view or no view).",
        menu: "",
        shortcut: None,
        args: &[
            opt("subject", Str, "composition (default) or plan"),
            opt("kind", Str, "finding, judgment, caveat, scenario, convention-example or note"),
            opt("about", Str, "an element id (place:workers/in_use)"),
            opt("view", Str, "a composition view id"),
            opt("all", Bool, "include dormant entries (marked dormant: true)"),
        ],
        enabled: always,
        run: get,
        journal: false,
    },
    CommandSpec {
        id: "content.load",
        label: "Load Content…",
        help: "Attach a ModelContent document (or `library`) to the open composition, replacing its content. \
               Refused with [CONTENT_REFUSED] when the document is unsound or an element id does not \
               resolve, unless `strict` is false (then the entry is dormant). One undo step.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("document", Json, "a ModelContent document (or a string holding it)"),
            opt("library", Str, "library content id (content.library)"),
            opt("strict", Bool, "refuse unresolved element ids (default true)"),
        ],
        enabled: needs_composition,
        run: load,
        journal: true,
    },
    CommandSpec {
        id: "content.clear",
        label: "Clear Content",
        help: "Detach the composition's content, leaving it bare. One undo step.",
        menu: MENU,
        shortcut: None,
        args: &[],
        enabled: needs_composition,
        run: clear,
        journal: true,
    },
    CommandSpec {
        id: "content.export",
        label: "Export Content",
        help: "The attached content (or `library`) as a document: {document, cid}, the CID being pflow's seal \
               of exactly that JSON. Ordered or nested values (a scenario, a check) are canonical-JSON text \
               in it. Stores a report.",
        menu: "",
        shortcut: None,
        args: &[opt("library", Str, "export a library entry instead")],
        enabled: always,
        run: export,
        journal: false,
    },
    CommandSpec {
        id: "content.validate",
        label: "Validate Content",
        help: "Whether the content is sound against its model: structure, kinds, views, bibliography keys, \
               scenarios, check shapes, and every element id it names. {valid, errors, warnings}, each \
               {code, entry, ref, message}. `strict` (default true) makes an unresolved id an error. \
               Stores a report.",
        menu: "",
        shortcut: None,
        args: &[
            opt("subject", Str, "composition (default) or plan"),
            opt("strict", Bool, "unresolved ids are errors (default true)"),
        ],
        enabled: always,
        run: validate,
        journal: false,
    },
    CommandSpec {
        id: "content.check",
        label: "Run Content Checks",
        help: "Run the registry checks that back findings, each on a scratch session so this session is \
               untouched: one `entry`, or `all` (the default). {checks: [{id, ok, claim, steps}], passed, \
               failed, total}. Stores a report.",
        menu: "",
        shortcut: None,
        args: &[
            opt("subject", Str, "composition (default) or plan"),
            opt("entry", Str, "one entry id"),
            opt("all", Bool, "every entry that has a check (default)"),
        ],
        enabled: always,
        run: check,
        journal: false,
    },
];

fn needs_composition(sess: &Session) -> std::result::Result<(), String> {
    if sess.composition.is_some() {
        Ok(())
    } else {
        Err("no composition is open (bundle.open, bundle.load)".into())
    }
}

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Subject {
    Composition,
    Plan,
}

fn subject(a: &Value) -> Result<Subject> {
    match s(a, "subject").unwrap_or("composition") {
        "composition" => Ok(Subject::Composition),
        "plan" => Ok(Subject::Plan),
        other => Err(Error::BadArgs(format!(
            "subject {other:?}; there are composition and plan"
        ))),
    }
}

/// The content of the subject, and where it came from. A plan's is the
/// library's `connect4`.
fn content_of(sess: &Session, subject: Subject) -> Result<(String, Option<&ModelContent>)> {
    match subject {
        Subject::Composition => {
            let c = sess.composition.as_ref().ok_or_else(|| {
                Error::Invalid("no composition is open (bundle.open, bundle.load)".into())
            })?;
            Ok((c.doc.content_source.label(), c.doc.content.as_ref()))
        }
        Subject::Plan => Ok((
            "library:connect4".into(),
            content::library_entry("connect4").map(|l| &l.content),
        )),
    }
}

fn library(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({
        "library": content::library().iter().map(|l| json!({
            "id": l.id,
            "subject": {"kind": l.content.subject.kind, "name": l.content.subject.name},
            "name": l.content.name,
            "entries": l.content.entries.len(),
            "checks": l.content.checks().len(),
            "scenarios": l.content.scenarios().iter().map(|s| s.id.clone()).collect::<Vec<_>>(),
            "anchors": l.anchors,
        })).collect::<Vec<_>>(),
    }))
}

/// Whether every element id an entry names resolves.
fn resolved(ids: &Ids, e: &Entry) -> bool {
    e.about.iter().all(|r| ids.resolves(r).unwrap_or(false))
}

fn get(sess: &mut Session, a: &Value) -> Result<Value> {
    let sub = subject(a)?;
    let (src, mc) = content_of(sess, sub)?;
    let Some(mc) = mc else {
        return Ok(json!({
            "source": src,
            "subject": if sub == Subject::Plan { "plan" } else { "composition" },
            "name": "",
            "entries": [],
            "total": 0,
            "dormant": 0, "note": "no model content is attached (NO_CONTENT)",
        }));
    };
    let declared: Vec<String> = mc
        .entries
        .iter()
        .filter(|e| e.kind == "scenario")
        .map(|e| e.id.clone())
        .collect();
    let ids = match (sub, &sess.composition) {
        (Subject::Composition, Some(c)) => Ids::of(c, &declared),
        _ => Ids::of_plan(),
    };
    let (kind, about, view) = (s(a, "kind"), s(a, "about"), s(a, "view"));
    let all = b(a, "all").unwrap_or(false);
    let mut dormant = 0;
    let mut out = vec![];
    for e in mc.sorted() {
        let live = resolved(&ids, e);
        if !live {
            dormant += 1;
            if !all {
                continue;
            }
        }
        if kind.is_some_and(|k| e.kind != k)
            || about.is_some_and(|x| !e.about.iter().any(|r| r == x))
            || view.is_some_and(|v| !e.views.is_empty() && !e.views.iter().any(|x| x == v))
        {
            continue;
        }
        let mut v = e.to_view_json();
        if all {
            v["dormant"] = json!(!live);
        }
        out.push(v);
    }
    Ok(json!({
        "source": src,
        "subject": mc.subject.kind,
        "name": mc.name,
        "entries": out,
        "total": mc.entries.len(),
        "dormant": dormant,
    }))
}

fn load(sess: &mut Session, a: &Value) -> Result<Value> {
    let strict = b(a, "strict").unwrap_or(true);
    let mut received = None;
    let (mc, src) = match (a.get("document").filter(|v| !v.is_null()), s(a, "library")) {
        (Some(d), None) => {
            let doc = value_arg(d, "document")?;
            let mc = ModelContent::parse(&doc)?;
            received = Some(doc);
            (mc, ContentSource::Loaded)
        }
        (None, Some(id)) => (
            content::library_entry(id)
                .ok_or_else(|| {
                    Error::NotFound(format!("no library content {id:?} (content.library)"))
                })?
                .content
                .clone(),
            ContentSource::Library(id.to_string()),
        ),
        _ => return Err(Error::BadArgs("pass `document` or `library`".into())),
    };
    sess.edit("load content", |sess| {
        let c = sess.composition.as_mut().ok_or_else(|| {
            Error::Invalid("no composition is open (bundle.open, bundle.load)".into())
        })?;
        // Sealing comes after every refusal: a document that is refused for its keys, its ids or
        // its size is not canonicalised first.
        let normal = mc.to_document();
        crate::commands::bundle_doc::attach(c, mc, strict)?;
        c.doc.content_source = src;
        let cid = crate::cid::seal(&normal)?;
        Ok(document::content_json(c).tap(|v| {
            v["cid"] = json!(cid);
            // The CID is the seal of the normalised document (`check` and `scenario` as canonical
            // text, nothing the build does not read): the one `content.export`, `store.put` and the
            // receipts name. A document received in another spelling seals to another CID, said here.
            if let Some(r) = received
                .filter(|r| *r != normal)
                .and_then(|r| crate::cid::seal(&r).ok())
                .filter(|r| *r != cid)
            {
                v["received_cid"] = json!(r);
            }
        }))
    })
}

fn clear(sess: &mut Session, _: &Value) -> Result<Value> {
    sess.edit("clear content", |sess| {
        let c = sess.composition.as_mut().ok_or_else(|| {
            Error::Invalid("no composition is open (bundle.open, bundle.load)".into())
        })?;
        let had = c.doc.content.take().map_or(0, |k| k.entries.len());
        c.doc.content_source = ContentSource::Bare;
        Ok(json!({"cleared": had}))
    })
}

fn export(sess: &mut Session, a: &Value) -> Result<Value> {
    let mc: &ModelContent = match s(a, "library") {
        Some(id) => {
            &content::library_entry(id)
                .ok_or_else(|| {
                    Error::NotFound(format!("no library content {id:?} (content.library)"))
                })?
                .content
        }
        None => sess
            .composition
            .as_ref()
            .ok_or_else(|| {
                Error::Invalid("no composition is open (bundle.open, bundle.load)".into())
            })?
            .doc
            .content
            .as_ref()
            .ok_or_else(|| Error::Invalid("[NO_CONTENT] no content is attached".into()))?,
    };
    let mut out = json!({"document": mc.to_document(), "cid": mc.cid()?});
    let id = sess.add_artifact(
        format!("Model content ({})", mc.name),
        source("content.export", a),
        ArtifactData::Report(out.clone()),
    );
    out["artifact"] = json!(id);
    Ok(out)
}

fn validate(sess: &mut Session, a: &Value) -> Result<Value> {
    let sub = subject(a)?;
    let strict = b(a, "strict").unwrap_or(true);
    let (src, mc) = content_of(sess, sub)?;
    let mc = mc.ok_or_else(|| Error::Invalid("[NO_CONTENT] no content is attached".into()))?;
    let report = match (sub, &sess.composition) {
        (Subject::Composition, Some(c)) => content::validate(mc, c, strict),
        _ => content::validate_plan(mc, strict),
    };
    let mut out = report.to_json();
    out["source"] = json!(src);
    out["entries"] = json!(mc.entries.len());
    out["unresolved"] = json!(report.unresolved());
    let id = sess.add_artifact(
        "Content validation".into(),
        source("content.validate", a),
        ArtifactData::Report(out.clone()),
    );
    out["artifact"] = json!(id);
    Ok(out)
}

fn check(sess: &mut Session, a: &Value) -> Result<Value> {
    let sub = subject(a)?;
    let (src, mc) = content_of(sess, sub)?;
    let mc = mc.ok_or_else(|| Error::Invalid("[NO_CONTENT] no content is attached".into()))?;
    let entries: Vec<&Entry> = match s(a, "entry") {
        Some(id) => vec![mc
            .entry(id)
            .filter(|e| e.check.is_some())
            .ok_or_else(|| Error::NotFound(format!("no entry {id:?} with a check")))?],
        None => mc.checks(),
    };
    let base = match (sub, &sess.composition) {
        (Subject::Composition, Some(c)) => Base::Composition(c),
        (Subject::Composition, None) => unreachable!("content_of refused"),
        (Subject::Plan, _) => Base::Plan(sess.search_plan.as_ref()),
    };
    let results: Vec<Value> = entries
        .iter()
        .map(|e| checks::run_check(&base, e))
        .collect();
    let passed = results.iter().filter(|r| r["ok"] == json!(true)).count();
    let mut out = json!({
        "source": src,
        "checks": results,
        "passed": passed,
        "failed": results.len() - passed,
        "total": results.len(),
    });
    let id = sess.add_artifact(
        "Content checks".into(),
        source("content.check", a),
        ArtifactData::Report(out.clone()),
    );
    out["artifact"] = json!(id);
    Ok(out)
}

/// `value.tap(|v| …)`, to set a key on a freshly built result.
trait Tap: Sized {
    fn tap(self, f: impl FnOnce(&mut Self)) -> Self;
}

impl Tap for Value {
    fn tap(mut self, f: impl FnOnce(&mut Self)) -> Self {
        f(&mut self);
        self
    }
}

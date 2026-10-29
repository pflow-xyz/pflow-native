//! Receipts for a composition and its content (docs/composition-documents.md
//! §6). `provenance::issue` signs; its `Receipt` is unchanged, so what is new
//! here is only the claim: three `scenario.kind` values, each a small JSON
//! record that [`replay`] turns back into the result a hash was taken over.
//!
//! * `composition-run`: a firing script on the composition's flat net
//!   (`content::run_declared`); the result is the [`super::Run`] as JSON.
//! * `composition-check`: one content entry's check on a scratch session
//!   (`checks::run_check`); the result is the check result.
//! * `composition-content`: the content as a whole, `{valid, unresolved,
//!   checks: [{id, ok}]}`.
//!
//! The receipt's `model` is the composition document's CID
//! ([`Composition::document_cid`]). A claim is issued by running the very
//! [`replay`] a verifier runs, on the scenario as the receipt will store it,
//! so the two cannot disagree for want of a normalisation. A replay needs
//! nothing the receipt does not name: the run's script and predicates and the
//! check itself are in the signed scenario, and the composition and (for a
//! check or the content) the content document are found by their CIDs, in the
//! open session, the library or the local store.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use super::checks::{self, Base};
use super::content::{self, DeclaredScenario, Entry, ModelContent};
use super::document::{self, refuse, ContentSource};
use super::{flatten, Composition};
use crate::artifact::ArtifactData;
use crate::commands::bundle_doc;
use crate::provenance::{self as pv, Receipt};
use crate::registry::b;
use crate::session::{Session, Step};
use crate::store;
use crate::{Error, Result};

pub const RUN: &str = "composition-run";
pub const CHECK: &str = "composition-check";
pub const CONTENT: &str = "composition-content";
pub const KINDS: [&str; 3] = [RUN, CHECK, CONTENT];
/// What the receipt's `engine` says produced the result.
pub const ENGINE: &str = "pflow-compose";
/// How the result is digested (`versions.resultDigest`).
pub const DIGEST: &str = "sha256 over the canonical JSON of the run, the check result or the content result (docs/composition-documents.md §6)";

/// The receipt's `scenario.kind`, or "".
pub fn kind_of(r: &Receipt) -> &str {
    r.scenario
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// Whether `provenance.verify_receipt` hands this receipt to
/// [`verify_command`]. The prefix, not [`KINDS`], so a kind a later build adds
/// is `uncheckable` here rather than misread as a net's scenario.
pub fn is_composition_receipt(r: &Receipt) -> bool {
    kind_of(r).starts_with("composition-")
}

// ------------------------------------------------------------ the claims

/// The scenario as `Receipt` stores it (`provenance::issue` canonicalises it
/// the same way), so what is run is what is signed and what is verified.
fn stored(v: Value) -> Value {
    serde_json::from_str(&pv::canonical(&v)).unwrap_or(v)
}

/// The scenario of a `composition-run` receipt. `content` is the CID of the
/// content the scenario was declared in; none for a bare script.
pub fn run_scenario(c: &Composition, sc: &DeclaredScenario, content: Option<String>) -> Value {
    stored(json!({
        "kind": RUN,
        "form": c.natural_form().name(),
        "content": content,
        "scenario": {
            "id": sc.id,
            "title": sc.title,
            "script": sc.script,
            "expect": sc.expect.iter().map(|e| json!({
                "place": e.place, "op": e.op, "n": e.n, "says": e.says,
            })).collect::<Vec<_>>(),
        },
    }))
}

/// The scenario of a `composition-check` receipt for `mc`'s entry `id`.
pub fn check_scenario(c: &Composition, mc: &ModelContent, id: &str) -> Result<Value> {
    let with_check = || -> Vec<&str> { mc.checks().iter().map(|e| e.id.as_str()).collect() };
    let e = mc.entry(id).ok_or_else(|| {
        Error::NotFound(format!(
            "no entry {id:?} in the attached content; the entries with a check are {:?}",
            with_check()
        ))
    })?;
    let check = e.check.as_ref().ok_or_else(|| {
        Error::BadArgs(format!(
            "entry {id:?} has no check; the entries with one are {:?}",
            with_check()
        ))
    })?;
    Ok(stored(json!({
        "kind": CHECK,
        "form": c.natural_form().name(),
        "content": mc.cid()?,
        "entry": e.id,
        "check": check,
    })))
}

/// The scenario of a `composition-content` receipt for `mc`.
pub fn content_scenario(c: &Composition, mc: &ModelContent) -> Result<Value> {
    Ok(stored(json!({
        "kind": CONTENT,
        "form": c.natural_form().name(),
        "content": mc.cid()?,
    })))
}

/// Runs `scenario` on `c` (with `content` for a check or the content) and
/// signs the claim: `model` is `c`'s document CID, `resultHash` the hash of
/// what the replay gave. Returns the receipt and that result.
pub fn issue(
    key: &pv::ReceiptKey,
    c: &Composition,
    content: Option<&ModelContent>,
    scenario: Value,
) -> Result<(Receipt, Value)> {
    let result = replay(c, content, &scenario).map_err(Error::Invalid)?;
    let receipt = pv::issue(
        key,
        &c.document_cid()?,
        &BTreeMap::new(),
        scenario,
        checks::result_hash(&result),
        ENGINE,
        pv::versions(DIGEST),
        pv::unix_now(),
    );
    Ok((receipt, result))
}

// ------------------------------------------------------------ the replay

/// The content as a store gives it back: through its exported document, so
/// `check` and `scenario` are the canonical text's values. A check run on
/// this copy runs the same in the session that issued the receipt and in one
/// that reads the content from the store.
fn as_stored(mc: &ModelContent) -> std::result::Result<ModelContent, String> {
    ModelContent::parse(&mc.to_document()).map_err(|e| e.to_string())
}

/// `c` with `content` attached, which a check or the content receipt runs
/// against (its `bundle.run {scenario}` reads the declared scenarios). The
/// content must be the one the scenario names.
fn with_content(
    c: &Composition,
    content: Option<&ModelContent>,
    scenario: &Value,
) -> std::result::Result<Composition, String> {
    let mc = content.ok_or("this kind of receipt replays against its content document")?;
    let cid = mc.cid().map_err(|e| e.to_string())?;
    if scenario["content"].as_str() != Some(cid.as_str()) {
        return Err(format!(
            "the content given is {cid}, not the {} the receipt names",
            scenario["content"]
        ));
    }
    let mut c = c.clone();
    let same = c
        .doc
        .content
        .as_ref()
        .is_some_and(|k| k.cid().ok().as_deref() == Some(cid.as_str()));
    c.doc.content = Some(as_stored(mc)?);
    if !same {
        c.doc.content_source = ContentSource::Loaded;
    }
    Ok(c)
}

/// The result a receipt's `scenario` hashes to, run on `c`. `content` is the
/// content document the scenario names (unused by a run). Err: the scenario
/// is malformed or cannot be run here.
pub fn replay(
    c: &Composition,
    content: Option<&ModelContent>,
    scenario: &Value,
) -> std::result::Result<Value, String> {
    match scenario.get("kind").and_then(Value::as_str) {
        Some(RUN) => replay_run(c, scenario),
        Some(CHECK) => replay_check(&with_content(c, content, scenario)?, scenario),
        Some(CONTENT) => replay_content(&with_content(c, content, scenario)?),
        other => Err(format!(
            "{other:?} is not a composition receipt kind; there are {KINDS:?}"
        )),
    }
}

fn replay_run(c: &Composition, scenario: &Value) -> std::result::Result<Value, String> {
    let s = scenario
        .get("scenario")
        .filter(|s| s.is_object())
        .ok_or("the receipt carries no scenario")?;
    // The same reading `content` gives a declared scenario.
    let entry = Entry {
        id: s["id"]
            .as_str()
            .ok_or("the scenario has no id")?
            .to_string(),
        kind: "scenario".into(),
        order: None,
        title: s["title"].as_str().unwrap_or_default().to_string(),
        text: String::new(),
        about: vec![],
        views: vec![],
        cites: vec![],
        check: None,
        scenario: Some(json!({"script": s["script"], "expect": s["expect"]})),
    };
    let sc = DeclaredScenario::of(&entry).ok_or(
        "the scenario must be {id, title, script: [\"<subnet>/<transition>\", …], expect: [{place, op, n, says}]}",
    )?;
    let flat = flatten(&c.bundle).map_err(|e| format!("the composition does not flatten: {e}"))?;
    Ok(serde_json::to_value(content::run_declared(&flat, &sc)).expect("a Run serializes"))
}

fn replay_check(c: &Composition, scenario: &Value) -> std::result::Result<Value, String> {
    let id = scenario["entry"]
        .as_str()
        .ok_or("the receipt names no entry")?;
    let check = scenario
        .get("check")
        .filter(|v| v.is_object())
        .ok_or("the receipt carries no check")?;
    let entry = Entry {
        id: id.to_string(),
        kind: "finding".into(),
        order: None,
        title: String::new(),
        text: String::new(),
        about: vec![],
        views: vec![],
        cites: vec![],
        check: Some(check.clone()),
        scenario: None,
    };
    Ok(checks::run_check(&Base::Composition(c), &entry))
}

/// `{valid, unresolved, checks}` of the content attached to `c`: the strict
/// validation and every check, each list in id order (the document's array
/// order is not part of its CID, so it is not part of the result).
fn replay_content(c: &Composition) -> std::result::Result<Value, String> {
    let mc = c.doc.content.as_ref().ok_or("no content is attached")?;
    let report = content::validate(mc, c, true);
    let unresolved: BTreeSet<(String, String)> = report
        .errors
        .iter()
        .chain(&report.warnings)
        .filter(|i| i.code == "ID_UNRESOLVED")
        .map(|i| {
            (
                i.entry.clone().unwrap_or_default(),
                i.reference.clone().unwrap_or_default(),
            )
        })
        .collect();
    let ran: BTreeMap<&str, bool> = mc
        .checks()
        .into_iter()
        .map(|e| {
            let r = checks::run_check(&Base::Composition(c), e);
            (e.id.as_str(), r["ok"].as_bool().unwrap_or(false))
        })
        .collect();
    Ok(json!({
        "valid": report.valid(),
        "unresolved": unresolved.iter().map(|(e, r)| json!({"entry": e, "ref": r})).collect::<Vec<_>>(),
        "checks": ran.iter().map(|(id, ok)| json!({"id": id, "ok": ok})).collect::<Vec<_>>(),
    }))
}

// ------------------------------------------------------------ finding the documents

/// The stored document under `cid` as it was sealed (`Store::get_sealed`: the
/// loaders refuse the `@id` the store adds). Err: why it is not there; `how`
/// says where to put it.
fn stored_document(cid: &str, what: &str, how: &str) -> std::result::Result<Value, String> {
    match store::open().get_sealed(cid) {
        Ok(v) => Ok(v),
        Err(Error::NotFound(_)) => Err(format!(
            "{what} {cid} not found: {how}, or put it in the local store (store.put), then verify again"
        )),
        Err(e) => Err(format!("{what} {cid}: {e}")),
    }
}

/// The composition whose document CID is `model`, and where it was found:
/// the open one when it is that document, else the stored document, rebuilt.
/// Only a composition that exports as exactly `model` counts, so a store
/// file that is not the document its name says is not trusted.
pub fn find_composition(
    sess: &Session,
    model: &str,
) -> std::result::Result<(String, Composition), String> {
    if let Some(c) = &sess.composition {
        if c.document_cid().ok().as_deref() == Some(model) {
            return Ok(("current".into(), c.clone()));
        }
    }
    let v = stored_document(model, "composition", "open it (bundle.open, bundle.load)")?;
    match store::kind_of(&v) {
        store::DocKind::Composition | store::DocKind::Bundle => {}
        other => {
            return Err(format!(
                "{model} is a {} document in the store, not a composition",
                other.name()
            ))
        }
    }
    let c = document::load(&v, &bundle_doc::from_store)
        .map_err(|e| format!("the stored document {model} does not load: {e}"))?
        .composition;
    match c.document_cid() {
        Ok(cid) if cid == model => Ok(("store".into(), c)),
        Ok(cid) => Err(format!(
            "the stored document under {model} is a different document (it exports as {cid}); the store file was altered"
        )),
        Err(e) => Err(format!("the stored document {model}: {e}")),
    }
}

/// The content document whose CID is `cid`, and where it was found: the
/// composition's own or the open session's attached content (`library:<id>`
/// when that is the library's, as a stored composition that is a library
/// example gets it back; else `current`), the library, the local store.
pub fn find_content(
    sess: &Session,
    c: &Composition,
    cid: &str,
) -> std::result::Result<(String, ModelContent), String> {
    let is = |mc: &ModelContent| mc.cid().ok().as_deref() == Some(cid);
    let attached = [Some(c), sess.composition.as_ref()]
        .into_iter()
        .flatten()
        .filter_map(|o| o.doc.content.as_ref().map(|mc| (mc, &o.doc.content_source)));
    if let Some((mc, source)) = attached.into_iter().find(|(m, _)| is(m)) {
        let from = match source {
            ContentSource::Library(id) => format!("library:{id}"),
            _ => "current".into(),
        };
        return Ok((from, mc.clone()));
    }
    if let Some(l) = content::library().iter().find(|l| is(&l.content)) {
        return Ok((format!("library:{}", l.id), l.content.clone()));
    }
    let v = stored_document(
        cid,
        "content",
        "attach it (bundle.load {content}, content.load)",
    )?;
    if store::kind_of(&v) != store::DocKind::Content {
        return Err(format!(
            "{cid} is a {} document in the store, not content",
            store::kind_of(&v).name()
        ));
    }
    let mc = ModelContent::parse(&v).map_err(|e| format!("the stored content {cid}: {e}"))?;
    if !is(&mc) {
        return Err(format!(
            "the stored document under {cid} is a different document; the store file was altered"
        ));
    }
    Ok(("store".into(), mc))
}

// ------------------------------------------------------------ provenance.verify_receipt

struct Replayed {
    model_from: String,
    content_from: Option<String>,
    hash: String,
}

/// Finds what the receipt names and replays it. Err: why that cannot be
/// done (the receipt is then `uncheckable`).
fn replay_receipt(sess: &Session, r: &Receipt) -> std::result::Result<Replayed, String> {
    let kind = kind_of(r);
    if !KINDS.contains(&kind) {
        return Err(format!(
            "{kind:?} is not a composition receipt kind this build replays; there are {KINDS:?}"
        ));
    }
    let (model_from, c) = find_composition(sess, &r.model)?;
    let (content, content_from) = if kind == RUN {
        (None, None)
    } else {
        let cid = r.scenario["content"]
            .as_str()
            .ok_or("the receipt names no content")?;
        let (from, mc) = find_content(sess, &c, cid)?;
        (Some(mc), Some(from))
    };
    let result = replay(&c, content.as_ref(), &r.scenario)?;
    Ok(Replayed {
        model_from,
        content_from,
        hash: checks::result_hash(&result),
    })
}

/// `provenance.verify_receipt` for a composition receipt: the signature
/// against the key it embeds, and a replay of what it certified on the
/// composition it names, with the verdicts and keys a net receipt gets
/// (`valid`; `invalid` for a bad signature or a replay that diverges;
/// `uncheckable` when the composition or content is not to be found or the
/// scenario cannot be run).
pub fn verify_command(sess: &mut Session, a: &Value, r: &Receipt) -> Result<Value> {
    let replay = b(a, "replay").unwrap_or(true);
    let mut out = json!({
        "revisionAtIssue": r.revision,
        "revisionNow": pv::revision(),
        "model": r.model,
        "kind": kind_of(r),
    });
    let sig = r.verify_signature();
    out["signatureValid"] = json!(sig.is_ok());
    match &sig {
        Ok(kid) => {
            out["keyId"] = json!(kid);
            let local = pv::local_key_id();
            out["trustedKey"] = json!(local.as_deref() == Some(kid.as_str()));
            if r.key_ephemeral {
                out["keyNote"] = json!("signed under an ephemeral key");
            }
        }
        Err(e) => out["signatureError"] = json!(e),
    }
    let mut replay_ok = !replay;
    if replay {
        out["replayed"] = json!(false);
        if sig.is_err() {
            out["replayError"] = json!(
                "not replayed: the signature does not verify, so there is no signed claim to check"
            );
        } else {
            match replay_receipt(sess, r) {
                Err(e) => out["replayError"] = json!(e),
                Ok(done) => {
                    out["replayed"] = json!(true);
                    out["modelFrom"] = json!(done.model_from);
                    if let Some(from) = done.content_from {
                        out["contentFrom"] = json!(from);
                    }
                    let matches = done.hash == r.result_hash;
                    out["resultMatches"] = json!(matches);
                    replay_ok = matches;
                    if !matches {
                        out["replayHash"] = json!(done.hash);
                        out["note"] = json!("signature intact but the replay diverges; if revisionNow differs from revisionAtIssue this is engine drift, not forgery");
                    }
                }
            }
        }
    }
    let valid = sig.is_ok() && replay_ok;
    out["verdict"] = json!(if valid {
        pv::VALID
    } else if sig.is_err() || out["resultMatches"] == json!(false) {
        pv::INVALID
    } else {
        pv::UNCHECKABLE
    });
    let id = sess.add_artifact(
        "Receipt verification".into(),
        Step {
            command: "provenance.verify_receipt".into(),
            args: a.clone(),
        },
        ArtifactData::Report(out.clone()),
    );
    out["report"] = json!(id);
    Ok(out)
}

/// The refusal for a command that needs attached content.
pub fn no_content() -> Error {
    refuse(
        "NO_CONTENT",
        "no model content is attached to the open composition (bundle.load {content}, content.load)",
    )
}

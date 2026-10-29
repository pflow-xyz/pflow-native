//! `bundle.receipt` and `content.receipt`: signed receipts for a firing
//! script on the open composition, for one content check, and for the
//! attached content as a whole (docs/composition-documents.md §6). The claims
//! and their replay are in [`crate::composition::receipt`]; this file reads
//! arguments, signs with the receipt key and stores the report.
//! `provenance.verify_receipt` checks what these issue.

use serde_json::{json, Value};

use super::bundle_doc::declared_scenarios;
use crate::artifact::ArtifactData;
use crate::composition::content::DeclaredScenario;
use crate::composition::receipt as rc;
use crate::composition::Composition;
use crate::provenance::{self as pv, Receipt};
use crate::registry::{opt, s, strs, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
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
        id: "bundle.receipt",
        label: "Receipt for a Run…",
        help: "Fire a declared `scenario` (bundle.scenarios) or a `script` of members \"<subnet>/<transition>\" on the \
               open composition's flat net, as bundle.run does, and sign a composition-run receipt: an Ed25519 \
               certificate over (the composition document's CID, the scenario as run, sha256 of the run result, \
               versions, issue time, key id, public key), in the receipt format sim.receipt uses. A run that stops or \
               fails its predicates is receipted as it happened. provenance.verify_receipt replays it on the \
               composition it names (the open one, or a document in the local store). Signed with the key in \
               <data_dir>/keys/receipt-ed25519.json (created on first use), or an ephemeral key on the web (the \
               receipt says so). Returns {result, receipt, report} and stores the report.",
        menu: "Compose",
        shortcut: None,
        args: &[
            opt("scenario", Str, "a declared scenario id"),
            opt("script", StrList, "members to fire, instead of a scenario"),
        ],
        enabled: need,
        run: bundle_receipt,
        journal: false,
    },
    CommandSpec {
        id: "content.receipt",
        label: "Receipt for Content…",
        help: "Sign a receipt for the attached model content. With `entry` (an entry with a check): a composition-check \
               receipt over that check's result on a scratch session, ok or not. Without: a composition-content \
               receipt over {valid (strict validation), unresolved element ids, checks: [{id, ok}]}. Both name the \
               composition document's CID and the content document's CID; provenance.verify_receipt finds them (the \
               open session, the library, the local store), replays, and compares the hash. Returns {result, receipt, \
               report} and stores the report.",
        menu: "Compose",
        shortcut: None,
        args: &[opt(
            "entry",
            Str,
            "an entry id with a check; omit to receipt the whole content",
        )],
        enabled: need,
        run: content_receipt,
        journal: false,
    },
];

fn comp_ref(sess: &Session) -> Result<&Composition> {
    sess.composition
        .as_ref()
        .ok_or_else(|| Error::Invalid("no composition is open (bundle.open, bundle.load)".into()))
}

/// The output of a receipt command, with its report stored, as sim.receipt
/// returns it.
fn finish(
    sess: &mut Session,
    command: &str,
    a: &Value,
    title: String,
    result: Value,
    receipt: &Receipt,
    key: &pv::ReceiptKey,
) -> Result<Value> {
    let mut out = json!({
        "result": result,
        "receipt": serde_json::to_value(receipt).expect("serializable"),
    });
    let report = sess.add_artifact(
        title,
        Step {
            command: command.into(),
            args: a.clone(),
        },
        ArtifactData::Report(out.clone()),
    );
    out["report"] = json!(report);
    if key.ephemeral() {
        out["note"] = json!("signed with an ephemeral key (no data directory): the receipt verifies against its embedded key, but this key is gone when the process exits");
    }
    if key.created {
        out["keyCreated"] = json!(key.path);
    }
    Ok(out)
}

fn bundle_receipt(sess: &mut Session, a: &Value) -> Result<Value> {
    let c = comp_ref(sess)?;
    let (sc, content) = match (s(a, "scenario"), strs(a, "script")) {
        (Some(id), None) => {
            let (_, declared) = declared_scenarios(sess);
            let ids: Vec<String> = declared.iter().map(|d| d.id.clone()).collect();
            let sc = declared.into_iter().find(|d| d.id == id).ok_or_else(|| {
                Error::BadArgs(format!(
                    "no scenario {id:?} is declared for this model; there are {ids:?}"
                ))
            })?;
            let content = c.doc.content.as_ref().map(|k| k.cid()).transpose()?;
            (sc, content)
        }
        (None, Some(script)) => (
            DeclaredScenario {
                id: "script".into(),
                title: "script".into(),
                script,
                expect: vec![],
                note: String::new(),
            },
            None,
        ),
        _ => return Err(Error::BadArgs("pass `scenario` or `script`".into())),
    };
    let key = pv::receipt_key()?;
    let (receipt, result) = rc::issue(&key, c, None, rc::run_scenario(c, &sc, content))?;
    let title = format!("Receipt: run {} ({})", sc.id, receipt.key_id);
    finish(sess, "bundle.receipt", a, title, result, &receipt, &key)
}

fn content_receipt(sess: &mut Session, a: &Value) -> Result<Value> {
    let c = comp_ref(sess)?;
    let mc = c.doc.content.clone().ok_or_else(rc::no_content)?;
    let (scenario, what) = match s(a, "entry") {
        Some(id) => (rc::check_scenario(c, &mc, id)?, format!("check {id}")),
        None => (rc::content_scenario(c, &mc)?, "content".to_string()),
    };
    let key = pv::receipt_key()?;
    let (receipt, result) = rc::issue(&key, c, Some(&mc), scenario)?;
    let title = format!("Receipt: {what} ({})", receipt.key_id);
    finish(sess, "content.receipt", a, title, result, &receipt, &key)
}

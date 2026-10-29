//! Provenance (phase 7): signed run receipts (`sim.receipt`,
//! `provenance.verify_receipt`, `provenance.key`), local witness checking
//! (`provenance.check_witness`) and zero-knowledge proofs of firings
//! (`zk.*`, cargo feature `zk`). The machinery is in [`crate::provenance`]
//! and [`crate::zk`]; this file reads arguments and stores artifacts.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::file::{take_rates, with_rates};
use crate::artifact::{ArtifactData, Series, TraceStep};
use crate::provenance::{self as pv, Mapping, Verdict};
use crate::registry::{always, arg, b, n, opt, req, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::stochastic as st;
use crate::zk;
use crate::{Error, Result};

const MENU: &str = "Provenance";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "sim.receipt",
        label: "Run Scenario with Receipt",
        help: "sim_receipt: run a seeded sim.scenario and sign a run receipt, an Ed25519 certificate over the canonical JSON of \
               (model CID, the session rates sealed into it, the scenario as run, sha256 of the result, engine, versions, issue time, key \
               id, public key), in sim.pflow.xyz's receipt format. The model CID is file.cid, the net as file.save writes it; modelRates \
               (absent when there are none) are the rates in it, so the model is found again after the rates change. The certified \
               scenario has every default filled in and the full start marking and every rate written out, so \
               provenance.verify_receipt can replay it with nothing else. Signed with the key in <data_dir>/keys/receipt-ed25519.json \
               (created on first use), or an ephemeral key on the web (the receipt says so). Stores the time series (unless summary) and a \
               report holding {result, receipt}.",
        menu: MENU,
        shortcut: None,
        args: &[opt("scenario", Json, "scenario object (or a string holding one), as sim.scenario takes; default {} (8 hours, 16 realizations, seed 20260809)")],
        enabled: always,
        run: receipt,
        journal: true,
    },
    CommandSpec {
        id: "provenance.verify_receipt",
        label: "Verify Receipt",
        help: "Check a run receipt two ways, as sim's /api/receipts/verify: the Ed25519 signature against the key it embeds \
               (trustedKey says whether that is this machine's key), and a replay of its certified scenario on the model it names \
               (the open net, or a library model with that CID; also once their rates have changed, rebuilt under the receipt's \
               modelRates), which must reproduce resultHash. A receipt without modelRates that was issued with session rates names \
               the net under those rates; they are rebuilt from the rates its scenario ran with, unless the scenario overrode one, \
               and then it is found only where they are again (set them back, or a library copy saved under them).",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("receipt", Json, "the receipt object (or the {result, receipt} sim.receipt returned, or a string holding either)"),
            opt("replay", Bool, "re-run the scenario and compare result hashes (default true)"),
        ],
        enabled: always,
        run: verify_receipt,
        journal: true,
    },
    CommandSpec {
        id: "provenance.key",
        label: "Receipt Signing Key",
        help: "The receipt signing key's public half: key id (first 8 bytes of sha256(public key), hex), public key (base64), where it \
               is stored and its file permissions. Creates the key on first use. Never shows the secret.",
        menu: MENU,
        shortcut: None,
        args: &[],
        enabled: always,
        run: key,
        journal: false,
    },
    CommandSpec {
        id: "provenance.check_witness",
        label: "Check Witness",
        help: "Re-check a witness without searching (the local half of sim_check_witness): a trace artifact (analysis.reach / \
               analysis.verify) or `trace` is replayed through the metamodel firing rule and every claimed marking compared; a reach \
               answer (`reach`, or a reach report artifact) is checked as sim's reachcheck does (a yes trace replays from from and \
               ends on to; a no certificate is a P-invariant, y·C = 0, that separates from and to); a `morphism` {definition or \
               predicate, subject, object, map: {places, transitions}} is checked as an isomorphism (coloured-net-v1) or an induced \
               embedding (induced-incidence-v1). Stores a report {verdict: valid | invalid | uncheckable, reason, failures}.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("artifact", Str, "a trace artifact, or a report artifact holding a reach answer"),
            opt("trace", Json, "a trace: [{transition, marking}] (step 0 the start, transition \"\"), {steps: [...]}, or a list of transition ids from the initial marking"),
            opt("reach", Json, "a reach answer, exactly as analysis.reach returned it"),
            opt("morphism", Json, "{definition: coloured-net-v1 | induced-incidence-v1 (or predicate: isomorphicTo | subnetOf), subject, object (library model names, \"current\", or documents), map: {places, transitions}}"),
        ],
        enabled: always,
        run: check_witness,
        journal: true,
    },
    CommandSpec {
        id: "zk.setup",
        label: "ZK Setup",
        help: "Groth16 (BN254) setup of the transition circuit for the open net (pflow-zk-arkworks): one proving key for every \
               transition. Keys are cached per circuit digest in memory and under <data_dir>/zk/<digest>/; a net whose incidence did \
               not change reuses them (markings and positions do not matter). Takes about a second on small nets in release.",
        menu: MENU,
        shortcut: None,
        args: &[opt("force", Bool, "run a fresh setup even if keys exist (old proofs then verify only against their embedded key)")],
        enabled: zk_available,
        run: zk_setup,
        journal: true,
    },
    CommandSpec {
        id: "zk.prove_fire",
        label: "ZK Prove Firing",
        help: "Prove that firing `transition` takes the current marking to the next one: a Groth16 proof whose public inputs are the \
               Poseidon roots of both markings and the transition index. Refuses a firing the metamodel rule rejects. Stores a report \
               {proof (base64), publicInputs, verifyingKey, statement, caveats, timings}; zk.verify checks it.",
        menu: MENU,
        shortcut: None,
        args: &[arg("transition", Str, "transition id, enabled at the current marking")],
        enabled: zk_available,
        run: zk_prove_fire,
        journal: true,
    },
    CommandSpec {
        id: "zk.prove_trace",
        label: "ZK Prove Trace",
        help: "Prove each firing of a trace artifact (step 0 = its start marking), one Groth16 proof per firing, chained by state \
               roots. The trace must replay under the metamodel rule.",
        menu: MENU,
        shortcut: None,
        args: &[
            arg("artifact", Str, "trace artifact id"),
            opt("max_steps", Int, "refuse traces with more firings than this (default 64, ≤ 256)"),
        ],
        enabled: zk_available,
        run: zk_prove_trace,
        journal: true,
    },
    CommandSpec {
        id: "zk.verify",
        label: "ZK Verify",
        help: "Verify a zk.prove_fire or zk.prove_trace proof (a report artifact id, or the proof object): against this machine's keys \
               for its circuit when they exist (trustedKey), else against the verifying key it embeds. A trace proof also has to chain.",
        menu: MENU,
        shortcut: None,
        args: &[arg("proof", Json, "a proof report's artifact id, or the proof object (or a string holding it)")],
        enabled: zk_available,
        run: zk_verify,
        journal: false,
    },
];

fn zk_available(_: &Session) -> std::result::Result<(), String> {
    zk::available()
}

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

// ================================================================ receipts

fn receipt(sess: &mut Session, a: &Value) -> Result<Value> {
    let scenario = match a.get("scenario").filter(|v| !v.is_null()) {
        Some(v) => st::json_arg(v, "scenario")?,
        None => json!({}),
    };
    let certified = pv::certify_scenario(&sess.doc, &sess.marking, &sess.rates, &scenario)?;
    let run = pv::run_certified(&sess.doc, &certified)?;
    let model = pv::model_cid(&sess.export_doc())?;
    // The rates export_doc wrote into that document (those of its
    // transitions): with the bare net they rebuild `model` once the session
    // rates move on.
    let model_rates: BTreeMap<String, f64> = sess
        .rates
        .iter()
        .filter(|(t, _)| sess.doc.transition(t).is_some())
        .map(|(t, r)| (t.clone(), *r))
        .collect();
    let key = pv::receipt_key()?;
    let receipt = pv::issue(
        &key,
        &model,
        &model_rates,
        certified,
        run.result_hash.clone(),
        run.scenario.engine.name(),
        pv::versions(run.digest),
        pv::unix_now(),
    );
    let name = run
        .scenario
        .name
        .clone()
        .unwrap_or_else(|| "scenario".into());
    let mut artifacts = vec![];
    if !run.scenario.summary {
        let e = &run.run.series;
        let series = e
            .names
            .iter()
            .enumerate()
            .map(|(i, nm)| Series {
                name: nm.clone(),
                values: e.mean[i].clone(),
                band: e.sd.as_ref().map(|sd| sd[i].clone()),
                group: None,
            })
            .collect();
        artifacts.push(sess.add_artifact(
            format!(
                "Receipted scenario {name} ({}, seed {})",
                run.scenario.engine.name(),
                run.scenario.seed
            ),
            source("sim.receipt", a),
            ArtifactData::TimeSeries {
                x_label: "t".into(),
                x: e.t.clone(),
                series,
            },
        ));
    }
    let receipt_json = serde_json::to_value(&receipt).expect("serializable");
    let mut out = json!({ "result": run.result, "receipt": receipt_json });
    let report = sess.add_artifact(
        format!("Receipt {name} ({})", receipt.key_id),
        source("sim.receipt", a),
        ArtifactData::Report(out.clone()),
    );
    artifacts.push(report.clone());
    out["artifacts"] = json!(artifacts);
    out["report"] = json!(report);
    if key.ephemeral() {
        out["note"] = json!("signed with an ephemeral key (no data directory): the receipt verifies against its embedded key, but this key is gone when the process exits");
    }
    if key.created {
        out["keyCreated"] = json!(key.path);
    }
    Ok(out)
}

fn verify_receipt(sess: &mut Session, a: &Value) -> Result<Value> {
    let r = pv::parse_receipt(&a["receipt"])?;
    if crate::composition::receipt::is_composition_receipt(&r) {
        return crate::composition::receipt::verify_command(sess, a, &r);
    }
    let replay = b(a, "replay").unwrap_or(true);
    let mut out = json!({
        "revisionAtIssue": r.revision,
        "revisionNow": pv::revision(),
        "model": r.model,
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
            match find_model(sess, &r)? {
                None => {
                    out["replayError"] = json!(format!(
                        "model {} not found: open it, or add it to the library (model.save or model.import), then verify again",
                        r.model
                    ))
                }
                Some((where_, doc)) => match pv::run_certified(&doc, &r.scenario) {
                    Err(e) => out["replayError"] = json!(e.to_string()),
                    Ok(run) => {
                        out["replayed"] = json!(true);
                        out["modelFrom"] = json!(where_);
                        let matches = run.result_hash == r.result_hash;
                        out["resultMatches"] = json!(matches);
                        replay_ok = matches;
                        if !matches {
                            out["replayHash"] = json!(run.result_hash);
                            out["note"] = json!("signature intact but the replay diverges; if revisionNow differs from revisionAtIssue this is engine drift, not forgery");
                        }
                    }
                },
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
        source("provenance.verify_receipt", a),
        ArtifactData::Report(out.clone()),
    );
    out["report"] = json!(id);
    Ok(out)
}

/// The open net or a library model whose CID is the receipt's model, and
/// where it was found. Every candidate is matched by CID alone, so nothing
/// but the net the receipt names can match; a receipt replays with the
/// rates it certified, so one rebuilt under other rates replays the same.
/// The nets as they stand come first (the open one, then the library), as
/// before `modelRates`, so a receipt is found where it was and under the
/// same name; the rebuilds under `modelRates` only after.
fn find_model(sess: &Session, r: &pv::Receipt) -> Result<Option<(String, crate::Document)>> {
    let cid = r.model.as_str();
    // The open net as saved now, or without rates (a receipt from before
    // model CIDs covered rates).
    if pv::model_cid(&sess.export_doc())? == cid || pv::model_cid(&sess.doc)? == cid {
        return Ok(Some(("current".into(), sess.doc.clone())));
    }
    for (name, doc) in &sess.models {
        if pv::model_cid(doc).ok().as_deref() == Some(cid) {
            return Ok(Some((name.clone(), doc.clone())));
        }
    }
    // The rates sealed into the model at issue: rebuild the net under them,
    // as the session rates may have moved since.
    let Some(at_issue) = r.rates_at_issue() else {
        return Ok(None);
    };
    for rates in rebuild_rates(r, at_issue) {
        if !rates.is_empty() && pv::model_cid(&with_rates(&sess.doc, &rates))? == cid {
            return Ok(Some(("current".into(), sess.doc.clone())));
        }
        // A library entry keeps the rates it was saved with: strip them first.
        for (name, doc) in &sess.models {
            let mut bare = doc.clone();
            take_rates(&mut bare);
            let rebuilt = with_rates(&bare, &rates);
            if pv::model_cid(&rebuilt).ok().as_deref() == Some(cid) {
                return Ok(Some((name.clone(), rebuilt)));
            }
        }
    }
    Ok(None)
}

/// The rate sets the receipt's model may be sealed under, `modelRates`
/// first. A receipt from before that field names the net under the session
/// rates of its issue without recording them apart, but its certified
/// scenario wrote them out among every rate it ran with (the scenario's
/// own, or 1 where none was set). So after no rates, try every rate it ran
/// with, then those without the 1s. That misses a session rate the scenario
/// overrode, and a wrong guess cannot match: the CID decides.
fn rebuild_rates(r: &pv::Receipt, at_issue: BTreeMap<String, f64>) -> Vec<BTreeMap<String, f64>> {
    let mut sets = vec![at_issue];
    if !r.model_rates.is_empty() {
        return sets;
    }
    let ran: Option<BTreeMap<String, f64>> = r.scenario["rates"].as_object().and_then(|m| {
        m.iter()
            .map(|(t, v)| Some((t.clone(), v.as_f64()?)))
            .collect()
    });
    if let Some(ran) = ran {
        let set: BTreeMap<String, f64> = ran
            .iter()
            .filter(|(_, r)| **r != 1.0)
            .map(|(t, r)| (t.clone(), *r))
            .collect();
        for s in [ran, set] {
            if !sets.contains(&s) {
                sets.push(s);
            }
        }
    }
    sets
}

fn key(_: &mut Session, _: &Value) -> Result<Value> {
    let k = pv::receipt_key()?;
    Ok(k.describe())
}

// ================================================================ witnesses

fn check_witness(sess: &mut Session, a: &Value) -> Result<Value> {
    let given: Vec<&str> = ["artifact", "trace", "reach", "morphism"]
        .into_iter()
        .filter(|k| a.get(*k).is_some_and(|v| !v.is_null()))
        .collect();
    if given.len() != 1 {
        return Err(Error::BadArgs(format!(
            "give exactly one of artifact, trace, reach or morphism (got {given:?})"
        )));
    }
    let (kind, v): (&str, Verdict) = match given[0] {
        "artifact" => {
            let id = req(a, "artifact");
            let art = sess
                .artifact(id)
                .ok_or_else(|| Error::NotFound(format!("no artifact {id:?}")))?;
            match &art.data {
                ArtifactData::Trace { steps } => {
                    let to = (art.source.command == "analysis.reach")
                        .then(|| art.source.args.get("to").cloned())
                        .flatten();
                    ("trace", check_trace_with_to(&sess.doc, steps, to.as_ref()))
                }
                ArtifactData::Report(r) if r.get("definition").is_some() => {
                    ("reach", pv::check_reach_answer(&sess.doc, r))
                }
                other => {
                    return Err(Error::BadArgs(format!(
                    "artifact {id} is a {} with no witness to check (a trace, or a reach report)",
                    other.kind()
                )))
                }
            }
        }
        "trace" => (
            "trace",
            trace_verdict(&sess.doc, &st::json_arg(&a["trace"], "trace")?)?,
        ),
        "reach" => (
            "reach",
            pv::check_reach_answer(&sess.doc, &st::json_arg(&a["reach"], "reach")?),
        ),
        _ => (
            "morphism",
            check_morphism(sess, &st::json_arg(&a["morphism"], "morphism")?)?,
        ),
    };
    let mut out = json!({
        "kind": kind,
        "verdict": v.verdict,
        "reason": v.reason,
        "failures": v.failures,
        "checker": "local: replays and algebra only, no search (sim_check_witness's checks, run against the open net)",
    });
    if kind == "morphism" {
        out["notCompared"] = json!(pv::NOT_COMPARED);
    }
    let id = sess.add_artifact(
        format!("Witness check ({kind}): {}", v.verdict),
        source("provenance.check_witness", a),
        ArtifactData::Report(out.clone()),
    );
    out["report"] = json!(id);
    Ok(out)
}

/// A reach trace artifact also has to end on the `to` it was asked for.
fn check_trace_with_to(doc: &crate::Document, steps: &[TraceStep], to: Option<&Value>) -> Verdict {
    let Some(to) = to else {
        return pv::check_trace(doc, steps, None);
    };
    let space = match crate::statespace::Space::new(doc) {
        Ok(s) => s,
        Err(e) => return Verdict::uncheckable(format!("the net does not compile: {e}")),
    };
    match space.partial(to, "to") {
        Ok(c) => {
            let cons: Vec<(String, i64)> = c
                .into_iter()
                .map(|(i, n)| (space.net.places[i].clone(), n))
                .collect();
            pv::check_trace(doc, steps, Some((&space.net, &cons)))
        }
        Err(e) => Verdict::uncheckable(format!(
            "the reach question's to no longer resolves on this net (was it edited?): {e}"
        )),
    }
}

/// A trace argument: steps, {steps}, or transition ids from the initial
/// marking (only legality is claimed then).
fn trace_verdict(doc: &crate::Document, v: &Value) -> Result<Verdict> {
    let list = match v {
        Value::Object(o) => o
            .get("steps")
            .cloned()
            .ok_or_else(|| Error::BadArgs("a trace object needs steps".into()))?,
        other => other.clone(),
    };
    let arr = list
        .as_array()
        .ok_or_else(|| Error::BadArgs("trace must be a list of steps".into()))?;
    if !arr.is_empty() && arr.iter().all(Value::is_string) {
        let compiled = match crate::sim::Compiled::new(doc) {
            Ok(c) => c,
            Err(e) => {
                return Ok(Verdict::uncheckable(format!(
                    "the net does not compile: {e}"
                )))
            }
        };
        let mut mk = crate::sim::initial_marking(doc);
        for (i, t) in arr.iter().filter_map(Value::as_str).enumerate() {
            match compiled.fire(t, &mk) {
                Ok(next) => mk = next,
                Err(e) => {
                    return Ok(Verdict::invalid(vec![format!(
                        "step {}: {t} cannot fire at the marking before it: {e}",
                        i + 1
                    )]))
                }
            }
        }
        return Ok(Verdict::valid(format!(
            "the {}-step firing sequence replays from the declared initial marking through the metamodel firing rule",
            arr.len()
        )));
    }
    let steps: Vec<TraceStep> =
        serde_json::from_value(list).map_err(|e| Error::BadArgs(format!("trace steps: {e}")))?;
    Ok(pv::check_trace(doc, &steps, None))
}

fn morphism_doc(sess: &Session, v: &Value, side: &str) -> Result<crate::Document> {
    match v {
        Value::String(name) => crate::commands::models::resolve(sess, name),
        Value::Object(_) => crate::Document::from_value(v),
        Value::Null => Err(Error::BadArgs(format!("morphism needs a {side}"))),
        other => Err(Error::BadArgs(format!(
            "morphism {side} must be a model name or a document, got {other}"
        ))),
    }
}

fn check_morphism(sess: &Session, w: &Value) -> Result<Verdict> {
    let def = match (
        w.get("definition").and_then(Value::as_str),
        w.get("predicate").and_then(Value::as_str),
    ) {
        (Some(d), _) => d.to_string(),
        (None, Some("isomorphicTo")) => pv::DEF_ISOMORPHISM.into(),
        (None, Some("subnetOf")) => pv::DEF_EMBEDDING.into(),
        (None, Some(p)) => {
            return Ok(Verdict::uncheckable(format!(
                "predicate {p:?} is not checked locally (isomorphicTo and subnetOf are); use sim_check_witness"
            )))
        }
        (None, None) => {
            return Err(Error::BadArgs(
                "morphism needs a definition (coloured-net-v1 | induced-incidence-v1) or a predicate (isomorphicTo | subnetOf)".into(),
            ))
        }
    };
    let subject = morphism_doc(sess, &w["subject"], "subject")?;
    let object = morphism_doc(sess, &w["object"], "object")?;
    let map_v = w
        .get("map")
        .or_else(|| w.get("mapping"))
        .ok_or_else(|| Error::BadArgs("morphism needs a map {places, transitions}".into()))?;
    let mp: Mapping = serde_json::from_value(map_v.clone()).map_err(|e| {
        Error::BadArgs(format!(
            "map must be {{places: {{id: id}}, transitions: {{id: id}}}}: {e}"
        ))
    })?;
    let (a, b) = match (
        pv::single_color_model(&subject, "subject"),
        pv::single_color_model(&object, "object"),
    ) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return Ok(Verdict::uncheckable(e)),
    };
    Ok(pv::check_mapping(&def, &a, &b, &mp))
}

// ======================================================================= zk

fn zk_setup(sess: &mut Session, a: &Value) -> Result<Value> {
    let mut out = zk::setup(&sess.doc, b(a, "force").unwrap_or(false))?;
    let id = sess.add_artifact(
        "ZK setup".into(),
        source("zk.setup", a),
        ArtifactData::Report(out.clone()),
    );
    out["report"] = json!(id);
    Ok(out)
}

fn zk_prove_fire(sess: &mut Session, a: &Value) -> Result<Value> {
    let t = req(a, "transition");
    if sess.doc.transition(t).is_none() {
        return Err(Error::NotFound(format!("no transition {t:?}")));
    }
    let mut out = zk::prove_fire(&sess.export_doc(), &sess.marking, t)?;
    let id = sess.add_artifact(
        format!("ZK proof: fire {t}"),
        source("zk.prove_fire", a),
        ArtifactData::Report(out.clone()),
    );
    out["report"] = json!(id);
    Ok(out)
}

fn zk_prove_trace(sess: &mut Session, a: &Value) -> Result<Value> {
    let id = req(a, "artifact");
    let max = match n(a, "max_steps") {
        None => zk::DEFAULT_TRACE_PROOFS,
        Some(x) if (1.0..=zk::MAX_TRACE_PROOFS as f64).contains(&x) => x as usize,
        Some(x) => {
            return Err(Error::BadArgs(format!(
                "max_steps {x} is outside 1..{}",
                zk::MAX_TRACE_PROOFS
            )))
        }
    };
    let steps = match sess.artifact(id).map(|a| &a.data) {
        Some(ArtifactData::Trace { steps }) => steps.clone(),
        Some(other) => {
            return Err(Error::BadArgs(format!(
                "artifact {id} is a {}, not a trace",
                other.kind()
            )))
        }
        None => return Err(Error::NotFound(format!("no artifact {id:?}"))),
    };
    let mut out = zk::prove_trace(&sess.export_doc(), &steps, max)?;
    let rid = sess.add_artifact(
        format!(
            "ZK proof: trace {id} ({} firings)",
            steps.len().saturating_sub(1)
        ),
        source("zk.prove_trace", a),
        ArtifactData::Report(out.clone()),
    );
    out["report"] = json!(rid);
    Ok(out)
}

fn zk_verify(sess: &mut Session, a: &Value) -> Result<Value> {
    let proof = match &a["proof"] {
        Value::String(id) if sess.artifact(id).is_some() => {
            match &sess.artifact(id).unwrap().data {
                ArtifactData::Report(r) => r.clone(),
                other => {
                    return Err(Error::BadArgs(format!(
                        "artifact {id} is a {}, not a proof report",
                        other.kind()
                    )))
                }
            }
        }
        v => st::json_arg(v, "proof")?,
    };
    zk::verify(&sess.doc, &proof)
}

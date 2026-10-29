//! Receipts for compositions and their content (docs/composition-documents.md
//! §6): `bundle.receipt` and `content.receipt` sign a run, one check and the
//! content as a whole; `provenance.verify_receipt` replays them on the
//! composition (and content) the receipt names.
//!
//! The content is declared inline here, so the tests do not depend on what
//! the orders library ships. Nothing touches the real user data directory:
//! PFLOW_NATIVE_HOME is a per-process temporary directory, and each test has
//! its own thread-local store, kept in memory with a handle the test plants
//! documents through (`store.put` takes a net; a composition or content
//! document is put here as the store lays it out, `models/<cid>.jsonld`).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use pflow_native_core::cid;
use pflow_native_core::composition::content::ModelContent;
use pflow_native_core::provenance as pv;
use pflow_native_core::store::{self, MemoryBackend, Store, StoreBackend};
use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

// ------------------------------------------------------------ harness

fn home() -> &'static PathBuf {
    static H: OnceLock<PathBuf> = OnceLock::new();
    H.get_or_init(|| {
        // Sweep what earlier runs left (an hour old or more).
        if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
            for e in rd.flatten() {
                let stale = e
                    .metadata()
                    .and_then(|m| m.modified())
                    .is_ok_and(|t| t.elapsed().is_ok_and(|age| age.as_secs() > 3600));
                if stale
                    && e.file_name()
                        .to_string_lossy()
                        .starts_with("pn-composition-receipts-test-")
                {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        let d = std::env::temp_dir().join(format!(
            "pn-composition-receipts-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("PFLOW_NATIVE_HOME", &d);
        d
    })
}

/// A store backend the test keeps a handle on.
struct Shared(Arc<MemoryBackend>);

impl StoreBackend for Shared {
    fn get(&self, key: &str) -> pflow_native_core::Result<Option<Vec<u8>>> {
        self.0.get(key)
    }
    fn put(&self, key: &str, data: &[u8]) -> pflow_native_core::Result<()> {
        self.0.put(key, data)
    }
    fn delete(&self, key: &str) -> pflow_native_core::Result<bool> {
        self.0.delete(key)
    }
    fn list(&self, prefix: &str) -> pflow_native_core::Result<Vec<String>> {
        self.0.list(prefix)
    }
    fn location(&self) -> String {
        self.0.location()
    }
    fn persistent(&self) -> bool {
        self.0.persistent()
    }
}

/// An empty store for this test's thread; the handle plants documents.
fn world() -> Arc<MemoryBackend> {
    home();
    let mem = Arc::new(MemoryBackend::new());
    store::set_thread_store(Some(Arc::new(Store::new(Shared(mem.clone())))));
    mem
}

/// Puts `doc` in the store under its own seal, with the `@id` the store
/// writes; returns the CID.
fn stash(mem: &MemoryBackend, doc: &Value) -> String {
    let cid = cid::seal(doc).unwrap();
    plant(mem, &cid, doc);
    cid
}

/// Puts `doc` in the store under `name`, whatever its seal is.
fn plant(mem: &MemoryBackend, name: &str, doc: &Value) {
    let mut v = doc.clone();
    v["@id"] = json!(name);
    let mut text = serde_json::to_string_pretty(&v).unwrap();
    text.push('\n');
    mem.put(&format!("models/{name}.jsonld"), text.as_bytes())
        .unwrap();
}

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    home();
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn err(s: &mut Session, id: &str, args: Value) -> String {
    home();
    match s.execute(id, args) {
        Ok(v) => panic!("{id} should fail, got {v}"),
        Err(e) => e.to_string(),
    }
}

fn report(s: &Session, id: &Value) -> Value {
    match &s.artifact(id.as_str().unwrap()).unwrap().data {
        ArtifactData::Report(r) => r.clone(),
        other => panic!("not a report: {}", other.kind()),
    }
}

fn verify(s: &mut Session, receipt: &Value) -> Value {
    run(
        s,
        "provenance.verify_receipt",
        json!({ "receipt": receipt }),
    )
}

/// Signs `receipt` again after `edit`: a signature that verifies over a claim
/// the replay can then disagree with.
fn resign(receipt: &Value, edit: impl FnOnce(&mut pv::Receipt)) -> Value {
    let mut r = pv::parse_receipt(receipt).unwrap();
    edit(&mut r);
    let key = pv::receipt_key().unwrap();
    r.signature = pv::b64(&key.sign(&r.signing_bytes()));
    serde_json::to_value(&r).unwrap()
}

// ------------------------------------------------------------ the model

/// The orders composition as `bundle.open` builds it, exported as a document.
fn orders_document() -> Value {
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    run(&mut s, "bundle.export", json!({}))["document"].clone()
}

/// The orders composition under another name, so it is no longer the
/// document the library's content is written against (`bundle.load` gives
/// that one its library content back): a composition with no content.
fn bare_document() -> Value {
    let mut d = orders_document();
    d["name"] = json!("orders-without-content");
    d
}

fn declared(id: &str, title: &str, script: Value, expect: Value) -> Value {
    json!({
        "id": id, "kind": "scenario", "title": title,
        "about": [format!("scenario:{id}")],
        "scenario": {"script": script, "expect": expect},
    })
}

fn run_check(claim: &str, scenario: &str, expect: Value) -> Value {
    json!({"claim": claim, "steps": [
        {"command": "bundle.run", "args": {"scenario": scenario}, "expect": expect},
    ]})
}

/// A ModelContent for the orders composition: two scenarios and two findings
/// that hold, plus `extra` entries.
fn content_with(extra: Vec<Value>) -> Value {
    let mut entries = vec![
        declared(
            "commit",
            "An order is paid",
            json!(["orders/dequeue", "payment/do_charge", "payment/commit"]),
            json!([
                {"place": "payment/completed", "op": "==", "n": 1, "says": "the saga completes"},
                {"place": "workers/available", "op": "==", "n": 2, "says": "the worker returns to the pool"},
            ]),
        ),
        declared(
            "refund",
            "The payment fails after the charge (refund)",
            json!([
                "orders/dequeue",
                "payment/do_charge",
                "payment/fail_commit",
                "payment/undo_charge",
                "payment/undo_reserve",
            ]),
            json!([
                {"place": "payment/rolled_back", "op": "==", "n": 1, "says": "the saga rolls back"},
                {"place": "payment/customer", "op": "==", "n": 1, "says": "the customer is refunded"},
                {"place": "workers/available", "op": "==", "n": 2, "says": "the worker returns to the pool"},
            ]),
        ),
        json!({
            "id": "commit-completes", "kind": "finding", "order": 10,
            "title": "A paid order completes",
            "text": "The commit scenario runs to its end and both predicates hold.",
            "about": ["scenario:commit"],
            "check": run_check("the commit run completes and its predicates pass", "commit", json!([
                {"path": "/completed", "op": "==", "value": true},
                {"path": "/expect/0/pass", "op": "==", "value": true},
                {"path": "/expect/1/pass", "op": "==", "value": true},
            ])),
        }),
        json!({
            "id": "pool-leaks", "kind": "finding", "order": 11,
            "title": "The pool leaks a worker on rollback",
            "text": "In the refund scenario the worker is not returned: release is fused only with commit.",
            "about": ["scenario:refund"],
            "check": run_check("the refund run completes with the worker still in use", "refund", json!([
                {"path": "/completed", "op": "==", "value": true},
                {"path": "/expect/2/pass", "op": "==", "value": false},
            ])),
        }),
    ];
    entries.extend(extra);
    json!({
        "@context": "https://pflow.xyz/schema",
        "@type": "ModelContent",
        "name": "orders-receipts-test",
        "subject": {"kind": "composition", "name": "orders"},
        "entries": entries,
    })
}

fn content() -> Value {
    content_with(vec![])
}

/// A finding whose check does not hold: the commit run does complete.
fn wrong_claim() -> Value {
    json!({
        "id": "wrong-claim", "kind": "finding", "order": 12,
        "text": "A claim the registry refutes: the commit run does not complete.",
        "about": ["scenario:commit"],
        "check": run_check("the commit run does not complete", "commit", json!([
            {"path": "/completed", "op": "==", "value": false},
        ])),
    })
}

/// The content document as a store keeps it (the CID is its seal).
fn content_document(content: &Value) -> Value {
    ModelContent::parse(content).unwrap().to_document()
}

fn content_cid(content: &Value) -> String {
    ModelContent::parse(content).unwrap().cid().unwrap()
}

/// The orders composition with `content` attached, as `bundle.load` opens it.
fn session_with(content: Value) -> Session {
    let mut s = Session::default();
    run(
        &mut s,
        "bundle.load",
        json!({"document": orders_document(), "content": content}),
    );
    s
}

fn session() -> Session {
    session_with(content())
}

fn assert_valid(v: &Value) {
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["replayed"], true, "{v}");
    assert_eq!(v["resultMatches"], true, "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
}

fn assert_verdict(v: &Value, verdict: &str) {
    assert_eq!(v["verdict"], verdict, "{v}");
}

// ------------------------------------------------------------ run receipts

#[test]
fn run_receipt_signs_and_verifies() {
    world();
    let mut s = session();
    let doc = run(&mut s, "bundle.export", json!({}));
    let journaled = s.journal.len();
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    assert_eq!(s.journal.len(), journaled, "a receipt is a query");

    let r = &out["receipt"];
    for k in [
        "model",
        "scenario",
        "resultHash",
        "engine",
        "revision",
        "issuedAt",
        "keyId",
        "publicKey",
        "signature",
    ] {
        assert!(r.get(k).is_some(), "receipt lacks {k}: {r}");
    }
    assert_eq!(r["model"], doc["cid"], "the model is the document's CID");
    assert_eq!(r["engine"], "pflow-compose");
    assert!(r["resultHash"].as_str().unwrap().starts_with("sha256:"));
    let sc = &r["scenario"];
    assert_eq!(sc["kind"], "composition-run");
    assert_eq!(sc["form"], "composition");
    assert_eq!(sc["content"], json!(content_cid(&content())));
    assert_eq!(sc["scenario"]["id"], "commit");
    assert_eq!(sc["scenario"]["title"], "An order is paid");
    assert_eq!(
        sc["scenario"]["script"],
        json!(["orders/dequeue", "payment/do_charge", "payment/commit"])
    );
    assert_eq!(sc["scenario"]["expect"][0]["place"], "payment/completed");
    // The result is the run, and the run is what bundle.run gives.
    assert_eq!(out["result"]["completed"], true);
    assert_eq!(out["result"]["expect"][0]["pass"], true);
    let mut ran = run(&mut s, "bundle.run", json!({"scenario": "commit"}));
    ran.as_object_mut().unwrap().remove("artifact");
    assert_eq!(out["result"], ran);
    // The key is this machine's, under PFLOW_NATIVE_HOME.
    let key = run(&mut s, "provenance.key", json!({}));
    assert_eq!(key["keyId"], r["keyId"]);
    assert!(home().join(pv::KEY_FILE).exists());
    // The stored report holds {result, receipt}.
    let rep = report(&s, &out["report"]);
    assert_eq!(rep["receipt"], *r);
    assert_eq!(rep["result"], out["result"]);

    let v = verify(&mut s, &out);
    assert_valid(&v);
    assert_eq!(v["kind"], "composition-run");
    assert_eq!(v["trustedKey"], true, "{v}");
    assert_eq!(v["modelFrom"], "current");
    assert_eq!(v["model"], doc["cid"]);
    assert!(v["revisionNow"].is_string() && v["revisionAtIssue"].is_string());
    // The receipt alone (not the {result, receipt} wrapper) verifies too, and
    // the verification is a stored report.
    let v2 = verify(&mut s, r);
    assert_valid(&v2);
    let stored = report(&s, &v2["report"]);
    assert_eq!(stored["verdict"], "valid");
    // Signature only, without a replay.
    let v3 = run(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": r, "replay": false}),
    );
    assert!(v3.get("replayed").is_none(), "{v3}");
    assert_eq!(v3["verdict"], "valid");
}

#[test]
fn script_receipt_covers_a_run_that_stops_and_one_with_no_content() {
    world();
    // A bare composition: no content, so only a script can be receipted.
    let mut s = Session::default();
    let loaded = run(&mut s, "bundle.load", json!({"document": bare_document()}));
    assert_eq!(loaded["content"]["source"], "none", "{loaded}");
    let out = run(
        &mut s,
        "bundle.receipt",
        json!({"script": ["orders/dequeue", "payment/commit"]}),
    );
    let sc = &out["receipt"]["scenario"];
    assert_eq!(sc["scenario"]["id"], "script");
    assert_eq!(sc["scenario"]["expect"], json!([]));
    assert_eq!(sc["content"], Value::Null);
    // payment/commit needs the charge first: the run stops there, and that is
    // what the receipt certifies.
    assert_eq!(out["result"]["completed"], false);
    assert_eq!(out["result"]["stopped_step"], "payment/commit");
    let v = verify(&mut s, &out);
    assert_valid(&v);
    assert_eq!(v["modelFrom"], "current");
    assert!(v.get("contentFrom").is_none());
}

#[test]
fn a_failing_scenario_is_receipted_as_it_happened() {
    world();
    let mut s = session();
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "refund"}));
    // The refund run completes but the worker is not returned (the leak).
    assert_eq!(out["result"]["completed"], true);
    assert_eq!(out["result"]["expect"][2]["pass"], false);
    assert_valid(&verify(&mut s, &out));
}

// ------------------------------------------------------------ check and content receipts

#[test]
fn check_receipt_signs_and_verifies() {
    world();
    let mut s = session();
    let doc = run(&mut s, "bundle.export", json!({}));
    let out = run(
        &mut s,
        "content.receipt",
        json!({"entry": "commit-completes"}),
    );
    let sc = &out["receipt"]["scenario"];
    assert_eq!(out["receipt"]["model"], doc["cid"]);
    assert_eq!(sc["kind"], "composition-check");
    assert_eq!(sc["form"], "composition");
    assert_eq!(sc["content"], json!(content_cid(&content())));
    assert_eq!(sc["entry"], "commit-completes");
    assert_eq!(sc["check"]["steps"][0]["command"], "bundle.run");
    assert_eq!(out["result"]["id"], "commit-completes");
    assert_eq!(out["result"]["ok"], true);
    assert_eq!(out["result"]["steps"][0]["expect"][0]["actual"], true);
    let v = verify(&mut s, &out);
    assert_valid(&v);
    assert_eq!(v["kind"], "composition-check");
    assert_eq!(v["modelFrom"], "current");
    assert_eq!(v["contentFrom"], "current");
    assert_eq!(v["trustedKey"], true);
    // The check ran on a scratch session: the caller's artifacts hold only
    // the reports.
    let titles: Vec<String> = s.artifacts.iter().map(|a| a.title.clone()).collect();
    assert!(!titles.iter().any(|t| t.starts_with("Run:")), "{titles:?}");
}

#[test]
fn content_receipt_signs_and_verifies() {
    world();
    let mut s = session();
    let out = run(&mut s, "content.receipt", json!({}));
    let sc = &out["receipt"]["scenario"];
    assert_eq!(sc["kind"], "composition-content");
    assert_eq!(sc["content"], json!(content_cid(&content())));
    // Strict validation passes, nothing is unresolved, and the checks are
    // listed by id.
    assert_eq!(
        out["result"],
        json!({
            "valid": true,
            "unresolved": [],
            "checks": [
                {"id": "commit-completes", "ok": true},
                {"id": "pool-leaks", "ok": true},
            ],
        })
    );
    let v = verify(&mut s, &out);
    assert_valid(&v);
    assert_eq!(v["kind"], "composition-content");
    assert_eq!(v["contentFrom"], "current");
}

#[test]
fn a_check_that_fails_is_receipted_honestly() {
    world();
    let mut s = session_with(content_with(vec![wrong_claim()]));
    let out = run(&mut s, "content.receipt", json!({"entry": "wrong-claim"}));
    assert_eq!(out["result"]["ok"], false);
    assert_eq!(out["result"]["steps"][0]["ok"], false);
    assert_eq!(out["result"]["steps"][0]["expect"][0]["actual"], true);
    assert_eq!(out["result"]["steps"][0]["expect"][0]["ok"], false);
    // Honest, so it replays: the verdict is about the receipt, not the claim.
    let v = verify(&mut s, &out);
    assert_valid(&v);

    // The content receipt carries the failing check too.
    let all = run(&mut s, "content.receipt", json!({}));
    assert_eq!(all["result"]["valid"], true);
    assert_eq!(
        all["result"]["checks"],
        json!([
            {"id": "commit-completes", "ok": true},
            {"id": "pool-leaks", "ok": true},
            {"id": "wrong-claim", "ok": false},
        ])
    );
    assert_valid(&verify(&mut s, &all));
}

#[test]
fn the_receipt_round_trips_through_parse_receipt() {
    world();
    let mut s = session();
    for out in [
        run(&mut s, "bundle.receipt", json!({"scenario": "commit"})),
        run(&mut s, "content.receipt", json!({"entry": "pool-leaks"})),
        run(&mut s, "content.receipt", json!({})),
    ] {
        let wrapped = pv::parse_receipt(&out).unwrap();
        let bare = pv::parse_receipt(&out["receipt"]).unwrap();
        let text = pv::parse_receipt(&json!(out["receipt"].to_string())).unwrap();
        assert_eq!(wrapped, bare);
        assert_eq!(bare, text);
        // Re-serialised, it is the receipt that was issued (deny_unknown_fields
        // and all), and it still verifies.
        assert_eq!(serde_json::to_value(&bare).unwrap(), out["receipt"]);
        assert!(bare.verify_signature().is_ok());
        assert_valid(&verify(&mut s, &serde_json::to_value(&bare).unwrap()));
    }
}

// ------------------------------------------------------------ tampering

#[test]
fn a_tampered_receipt_is_invalid() {
    world();
    let mut s = session();
    for out in [
        run(&mut s, "bundle.receipt", json!({"scenario": "commit"})),
        run(
            &mut s,
            "content.receipt",
            json!({"entry": "commit-completes"}),
        ),
        run(&mut s, "content.receipt", json!({})),
    ] {
        let r = &out["receipt"];
        assert_valid(&verify(&mut s, r));

        // The hash, edited: the signature no longer covers the receipt.
        let mut t = r.clone();
        t["resultHash"] = json!(format!("sha256:{}", "0".repeat(64)));
        let v = verify(&mut s, &t);
        assert_eq!(v["signatureValid"], false, "{v}");
        assert_eq!(v["replayed"], false, "{v}");
        assert_verdict(&v, "invalid");

        // The scenario, edited: likewise.
        let mut t = r.clone();
        t["scenario"]["form"] = json!("bundle");
        let v = verify(&mut s, &t);
        assert_eq!(v["signatureValid"], false, "{v}");
        assert_verdict(&v, "invalid");

        // A different signature.
        let mut t = r.clone();
        t["signature"] = json!(pv::b64(&[7u8; 64]));
        assert_verdict(&verify(&mut s, &t), "invalid");
    }

    // The script, edited.
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    let mut t = out["receipt"].clone();
    t["scenario"]["scenario"]["script"] = json!(["orders/dequeue"]);
    let v = verify(&mut s, &t);
    assert_eq!(v["signatureValid"], false, "{v}");
    assert_verdict(&v, "invalid");
    assert_eq!(v["replayed"], false);
    assert!(v["replayError"].as_str().unwrap().contains("signature"));

    // The check, edited.
    let out = run(
        &mut s,
        "content.receipt",
        json!({"entry": "commit-completes"}),
    );
    let mut t = out["receipt"].clone();
    t["scenario"]["check"]["steps"][0]["expect"][0]["value"] = json!(false);
    let v = verify(&mut s, &t);
    assert_eq!(v["signatureValid"], false, "{v}");
    assert_verdict(&v, "invalid");
}

#[test]
fn a_signed_claim_the_replay_contradicts_is_invalid() {
    world();
    let mut s = session();

    // Signed over a different script, with the old result hash.
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    let forged = resign(&out["receipt"], |r| {
        r.scenario["scenario"]["script"] = json!(["orders/dequeue"]);
    });
    let v = verify(&mut s, &forged);
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["replayed"], true, "{v}");
    assert_eq!(v["resultMatches"], false, "{v}");
    assert_ne!(v["replayHash"], out["receipt"]["resultHash"]);
    assert_verdict(&v, "invalid");

    // Signed over the same claim with a made-up result hash.
    let forged = resign(&out["receipt"], |r| {
        r.result_hash = format!("sha256:{}", "ab".repeat(32));
    });
    let v = verify(&mut s, &forged);
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["resultMatches"], false, "{v}");
    assert_eq!(v["replayHash"], out["receipt"]["resultHash"]);
    assert_verdict(&v, "invalid");

    // A check receipt re-signed over a check that does not say what the
    // original did.
    let out = run(
        &mut s,
        "content.receipt",
        json!({"entry": "commit-completes"}),
    );
    let forged = resign(&out["receipt"], |r| {
        r.scenario["check"]["steps"][0]["expect"][0]["value"] = json!(false);
    });
    let v = verify(&mut s, &forged);
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["resultMatches"], false, "{v}");
    assert_verdict(&v, "invalid");

    // A check that tries to run an editing command is not run by a verifier:
    // the runner refuses it, so the result differs and the receipt is invalid.
    let forged = resign(&out["receipt"], |r| {
        r.scenario["check"]["steps"] = json!([{
            "command": "edit.undo", "args": {},
            "expect": [{"path": "", "op": "exists"}],
        }]);
    });
    let v = verify(&mut s, &forged);
    assert_eq!(v["resultMatches"], false, "{v}");
    assert_verdict(&v, "invalid");
    assert!(
        s.composition.is_some(),
        "the undo was not run on the caller"
    );

    // The content receipt re-signed over a hash the content does not give.
    let out = run(&mut s, "content.receipt", json!({}));
    let forged = resign(&out["receipt"], |r| {
        r.result_hash = format!("sha256:{}", "cd".repeat(32));
    });
    let v = verify(&mut s, &forged);
    assert_eq!(v["resultMatches"], false, "{v}");
    assert_eq!(v["replayHash"], out["receipt"]["resultHash"]);
    assert_verdict(&v, "invalid");
}

#[test]
fn a_tampered_model_cid_is_invalid_or_uncheckable() {
    let mem = world();
    let mut s = session();
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    let bogus = "z4EBG9j1SUhuY7XnEcPfHQPzQkmqjUtgrD2wHADw8qdRbpnNt6W";
    assert_ne!(out["receipt"]["model"], bogus);

    // Edited without signing again: the signature is what fails.
    let mut t = out["receipt"].clone();
    t["model"] = json!(bogus);
    let v = verify(&mut s, &t);
    assert_eq!(v["signatureValid"], false, "{v}");
    assert_verdict(&v, "invalid");

    // Signed again over a CID nothing has: no composition to replay on.
    let renamed = resign(&out["receipt"], |r| r.model = bogus.to_string());
    let v = verify(&mut s, &renamed);
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["replayed"], false, "{v}");
    assert!(v.get("resultMatches").is_none());
    assert!(
        v["replayError"].as_str().unwrap().contains("not found"),
        "{v}"
    );
    assert_verdict(&v, "uncheckable");

    // A store cannot hold a document under a CID that is not its seal (the
    // store seals what it is given), but a file can be put there by hand. The
    // verifier does not take it for the model: it exports as another CID.
    plant(&mem, bogus, &orders_document());
    let v = verify(&mut s, &renamed);
    assert_eq!(v["replayed"], false, "{v}");
    assert!(
        v["replayError"]
            .as_str()
            .unwrap()
            .contains("different document"),
        "{v}"
    );
    assert_verdict(&v, "uncheckable");

    // A net put there by hand is no composition. (The store does not seal a net it reads, as it
    // never did: a net's seal costs its size squared, so what refuses it here is its kind.)
    let mut other = Session::default();
    run(&mut other, "file.open_example", json!({"name": "sir"}));
    let net = other.export_doc().to_value();
    plant(&mem, bogus, &net);
    let v = verify(&mut s, &renamed);
    assert!(
        v["replayError"]
            .as_str()
            .unwrap()
            .contains("is a net document in the store, not a composition"),
        "{v}"
    );
    assert_verdict(&v, "uncheckable");

    // A net under its own CID is a net, and not a composition either.
    let net_cid = stash(&mem, &net);
    let named_net = resign(&out["receipt"], |r| r.model = net_cid.clone());
    let v = verify(&mut s, &named_net);
    assert!(
        v["replayError"]
            .as_str()
            .unwrap()
            .contains("not a composition"),
        "{v}"
    );
    assert_verdict(&v, "uncheckable");
}

#[test]
fn an_unknown_composition_kind_is_uncheckable() {
    world();
    let mut s = session();
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    let future = resign(&out["receipt"], |r| {
        r.scenario["kind"] = json!("composition-from-the-future");
    });
    let v = verify(&mut s, &future);
    assert_eq!(v["signatureValid"], true, "{v}");
    assert!(
        v["replayError"]
            .as_str()
            .unwrap()
            .contains("not a composition receipt kind"),
        "{v}"
    );
    assert_verdict(&v, "uncheckable");
}

// ------------------------------------------------------------ finding the composition

#[test]
fn an_edited_composition_still_verifies_the_old_receipt_if_the_original_is_stored() {
    let mem = world();
    let mut s = session();
    let original = run(&mut s, "bundle.export", json!({}));
    let receipts = [
        run(&mut s, "bundle.receipt", json!({"scenario": "commit"})),
        run(
            &mut s,
            "content.receipt",
            json!({"entry": "commit-completes"}),
        ),
        run(&mut s, "content.receipt", json!({})),
    ];
    for r in &receipts {
        assert_valid(&verify(&mut s, r));
    }
    let commit_worker_returns = |s: &mut Session| {
        run(s, "bundle.run", json!({"scenario": "commit"}))["expect"][1]["pass"].clone()
    };
    assert_eq!(commit_worker_returns(&mut s), true);

    // Edit: the release -> commit link goes, so the worker no longer returns.
    run(&mut s, "bundle.remove_link", json!({"index": 2}));
    let edited = run(&mut s, "bundle.export", json!({}));
    assert_ne!(edited["cid"], original["cid"]);
    assert_eq!(commit_worker_returns(&mut s), false);

    // The original is nowhere: the receipts are about a model we do not have.
    for r in &receipts {
        let v = verify(&mut s, r);
        assert_eq!(v["signatureValid"], true, "{v}");
        assert_eq!(v["replayed"], false, "{v}");
        assert!(
            v["replayError"].as_str().unwrap().contains("not found"),
            "{v}"
        );
        assert_verdict(&v, "uncheckable");
    }

    // Put the original in the store, and they verify, replayed on it.
    assert_eq!(stash(&mem, &original["document"]), original["cid"]);
    for r in &receipts {
        let v = verify(&mut s, r);
        assert_valid(&v);
        assert_eq!(v["modelFrom"], "store", "{v}");
    }
    // The content the edited composition still carries is the one the check
    // names; the replay did not run on the edited links.
    let v = verify(&mut s, &receipts[1]);
    assert_eq!(v["contentFrom"], "current");
    assert_eq!(commit_worker_returns(&mut s), false);
}

#[test]
fn a_fresh_session_verifies_from_the_store_alone() {
    let mem = world();
    let mut s = session();
    let original = run(&mut s, "bundle.export", json!({}));
    let receipts = [
        run(&mut s, "bundle.receipt", json!({"scenario": "refund"})),
        run(&mut s, "content.receipt", json!({"entry": "pool-leaks"})),
        run(&mut s, "content.receipt", json!({})),
    ];
    // Nothing in the store yet: a session that has nothing open can do nothing.
    let mut fresh = Session::default();
    for r in &receipts {
        assert_verdict(&verify(&mut fresh, r), "uncheckable");
    }
    // The composition alone settles the run; the check and the content also
    // need the content document.
    stash(&mem, &original["document"]);
    let v = verify(&mut fresh, &receipts[0]);
    assert_valid(&v);
    assert_eq!(v["modelFrom"], "store");
    for r in &receipts[1..] {
        let v = verify(&mut fresh, r);
        assert!(
            v["replayError"].as_str().unwrap().contains("content"),
            "{v}"
        );
        assert_verdict(&v, "uncheckable");
    }
    // With both stored, all three verify; the store's copy is read back
    // through its text, and the replay is the same.
    let stored = stash(&mem, &content_document(&content()));
    assert_eq!(stored, content_cid(&content()));
    for r in &receipts {
        let v = verify(&mut fresh, r);
        assert_valid(&v);
        assert_eq!(v["modelFrom"], "store", "{v}");
    }
    assert_eq!(verify(&mut fresh, &receipts[1])["contentFrom"], "store");
    // A key the machine has never seen is still a valid signature; here it is
    // this machine's.
    assert_eq!(verify(&mut fresh, &receipts[0])["trustedKey"], true);
}

#[test]
fn store_put_makes_the_documents_a_fresh_session_can_verify_from() {
    // The stored path as a user takes it: store.put on the open composition
    // and its content (no hand-planted files), then a session with nothing
    // open verifies all three kinds from the store alone.
    world();
    let mut s = session();
    let receipts = [
        run(&mut s, "bundle.receipt", json!({"scenario": "refund"})),
        run(&mut s, "content.receipt", json!({"entry": "pool-leaks"})),
        run(&mut s, "content.receipt", json!({})),
    ];
    let put = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    assert_eq!(put["cid"], receipts[0]["receipt"]["model"], "{put}");
    assert_eq!(
        put["content"]["cid"], receipts[1]["receipt"]["scenario"]["content"],
        "{put}"
    );

    let mut fresh = Session::default();
    for r in &receipts {
        let v = verify(&mut fresh, r);
        assert_valid(&v);
        assert_eq!(v["modelFrom"], "store", "{v}");
    }
    assert!(verify(&mut fresh, &receipts[0])
        .get("contentFrom")
        .is_none());
    assert_eq!(verify(&mut fresh, &receipts[1])["contentFrom"], "store");
    assert_eq!(verify(&mut fresh, &receipts[2])["contentFrom"], "store");

    // The same stored copies, opened with store.open, verify as the open one.
    let mut opened = Session::default();
    run(&mut opened, "store.open", json!({"cid": put["cid"]}));
    let v = verify(&mut opened, &receipts[0]);
    assert_valid(&v);
    assert_eq!(v["modelFrom"], "current");
    run(
        &mut opened,
        "store.open",
        json!({"cid": put["content"]["cid"]}),
    );
    let v = verify(&mut opened, &receipts[1]);
    assert_valid(&v);
    assert_eq!(v["contentFrom"], "current");
}

#[test]
fn the_content_is_pinned_by_its_cid() {
    let mem = world();
    let mut s = session();
    let original = run(&mut s, "bundle.export", json!({}));
    let check = run(
        &mut s,
        "content.receipt",
        json!({"entry": "commit-completes"}),
    );
    let whole = run(&mut s, "content.receipt", json!({}));
    stash(&mem, &original["document"]);

    // Another content on the same composition: `commit` now means something
    // else, and the old finding is gone.
    let other = json!({
        "@context": "https://pflow.xyz/schema",
        "@type": "ModelContent",
        "name": "orders-other",
        "subject": {"kind": "composition", "name": "orders"},
        "entries": [declared(
            "commit",
            "Only a dequeue",
            json!(["orders/dequeue"]),
            json!([{"place": "orders/items", "op": "==", "n": 1, "says": "one left"}]),
        )],
    });
    run(
        &mut s,
        "bundle.load",
        json!({"document": original["document"], "content": other}),
    );
    // The same composition document, so it is the one found; its content is
    // not the receipt's, and the receipt's is nowhere.
    for r in [&check, &whole] {
        let v = verify(&mut s, r);
        assert_eq!(v["signatureValid"], true, "{v}");
        assert!(
            v["replayError"].as_str().unwrap().contains("content"),
            "{v}"
        );
        assert_verdict(&v, "uncheckable");
    }
    // In the store, it replays on that content, not on the one now attached
    // (whose `commit` runs a different script).
    stash(&mem, &content_document(&content()));
    for r in [&check, &whole] {
        let v = verify(&mut s, r);
        assert_valid(&v);
        assert_eq!(v["modelFrom"], "current");
        assert_eq!(v["contentFrom"], "store");
    }
}

#[test]
fn library_content_is_found_in_the_library() {
    let mem = world();
    // bundle.open attaches the orders library's content to the default
    // instances. Its pool-leak finding (the refund scenario) is the check the
    // library ships for the leak; its receipt, the refund run's and the whole
    // content's all replay.
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    let doc = run(&mut s, "bundle.export", json!({}));
    let leak = run(
        &mut s,
        "content.receipt",
        json!({"entry": "find-pool-leaks-on-rollback"}),
    );
    assert_eq!(leak["result"]["ok"], true, "{leak}");
    let refund = run(&mut s, "bundle.receipt", json!({"scenario": "refund"}));
    assert_eq!(refund["result"]["completed"], true);
    let out = run(&mut s, "content.receipt", json!({}));
    assert_eq!(out["receipt"]["scenario"]["kind"], "composition-content");
    assert_eq!(out["result"]["valid"], true, "{out}");
    assert!(
        out["result"]["checks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["ok"] == true),
        "{out}"
    );
    for r in [&leak, &refund, &out] {
        assert_valid(&verify(&mut s, r));
    }
    assert_eq!(verify(&mut s, &leak)["contentFrom"], "library:orders");
    // A session with the composition in the store and nothing open rebuilds
    // it, library content and all (a library example gets its content back
    // on load), so the content is the library's, not the store's.
    stash(&mem, &doc["document"]);
    let mut fresh = Session::default();
    for r in [&leak, &refund, &out] {
        let v = verify(&mut fresh, r);
        assert_valid(&v);
        assert_eq!(v["modelFrom"], "store");
    }
    assert_eq!(verify(&mut fresh, &leak)["contentFrom"], "library:orders");
    assert_eq!(verify(&mut fresh, &out)["contentFrom"], "library:orders");
}

#[test]
fn a_raw_bundle_is_receipted_and_found_again() {
    let mem = world();
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/bundle/cafe.bundle.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let cafe: Value = serde_json::from_str(&text).unwrap();
    let mut s = Session::default();
    let loaded = run(&mut s, "bundle.load", json!({"document": cafe}));
    assert_eq!(loaded["form"], "bundle");
    let out = run(
        &mut s,
        "bundle.receipt",
        json!({"script": ["counter/order", "counter/start_espresso"]}),
    );
    // The cafe's document CID is the seal of the bytes as received.
    assert_eq!(out["receipt"]["model"], loaded["cid"]);
    assert_eq!(
        out["receipt"]["model"],
        "z4EBG9j1SUhuY7XnEcPfHQPzQkmqjUtgrD2wHADw8qdRbpnNt6W"
    );
    assert_eq!(out["receipt"]["scenario"]["form"], "bundle");
    let v = verify(&mut s, &out);
    assert_valid(&v);
    assert_eq!(v["modelFrom"], "current");

    // A bundle has no content unless it is given some.
    let e = err(&mut s, "content.receipt", json!({}));
    assert!(e.contains("[NO_CONTENT]"), "{e}");

    // From the store, in a session with nothing open.
    let mut fresh = Session::default();
    assert_verdict(&verify(&mut fresh, &out), "uncheckable");
    assert_eq!(stash(&mem, &cafe), out["receipt"]["model"]);
    let v = verify(&mut fresh, &out);
    assert_valid(&v);
    assert_eq!(v["modelFrom"], "store");
}

#[test]
fn a_stored_document_without_the_stores_id_is_found_too() {
    let mem = world();
    let mut s = session();
    let original = run(&mut s, "bundle.export", json!({}));
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    // Put by hand, exactly as sealed (no `@id`).
    let cid = cid::seal(&original["document"]).unwrap();
    let mut text = serde_json::to_string_pretty(&original["document"]).unwrap();
    text.push('\n');
    mem.put(&format!("models/{cid}.jsonld"), text.as_bytes())
        .unwrap();
    let mut fresh = Session::default();
    let v = verify(&mut fresh, &out);
    assert_valid(&v);
    assert_eq!(v["modelFrom"], "store");
}

// ------------------------------------------------------------ refusals and the net path

#[test]
fn the_commands_refuse_what_cannot_be_receipted() {
    world();
    let mut none = Session::default();
    for (id, args) in [
        ("bundle.receipt", json!({"scenario": "commit"})),
        ("content.receipt", json!({})),
    ] {
        let e = err(&mut none, id, args);
        assert!(e.contains("no composition is open"), "{id}: {e}");
    }

    let mut s = session();
    let e = err(&mut s, "bundle.receipt", json!({}));
    assert!(e.contains("pass `scenario` or `script`"), "{e}");
    let e = err(
        &mut s,
        "bundle.receipt",
        json!({"scenario": "commit", "script": ["orders/dequeue"]}),
    );
    assert!(e.contains("pass `scenario` or `script`"), "{e}");
    let e = err(&mut s, "bundle.receipt", json!({"scenario": "nope"}));
    assert!(
        e.contains("no scenario \"nope\"") && e.contains("commit"),
        "{e}"
    );
    let e = err(&mut s, "content.receipt", json!({"entry": "nope"}));
    assert!(
        e.contains("no entry \"nope\"") && e.contains("commit-completes"),
        "{e}"
    );
    let e = err(&mut s, "content.receipt", json!({"entry": "commit"}));
    assert!(
        e.contains("has no check") && e.contains("pool-leaks"),
        "{e}"
    );

    // A composition with no content: scenarios are not declared, and there is
    // no content to receipt.
    let mut bare = Session::default();
    let loaded = run(
        &mut bare,
        "bundle.load",
        json!({"document": bare_document()}),
    );
    assert_eq!(loaded["content"]["source"], "none", "{loaded}");
    let e = err(&mut bare, "content.receipt", json!({}));
    assert!(e.contains("[NO_CONTENT]"), "{e}");
    let e = err(&mut bare, "bundle.receipt", json!({"scenario": "commit"}));
    assert!(e.contains("no scenario \"commit\""), "{e}");
}

#[test]
fn net_receipts_still_verify_through_the_same_command() {
    world();
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 3, "samples": 10}}),
    );
    let v = verify(&mut s, &out);
    assert_eq!(v["verdict"], "valid", "{v}");
    assert_eq!(v["resultMatches"], true);
    assert!(v.get("kind").is_none(), "the net path is unchanged: {v}");
    // And a composition receipt does not verify as a net's: the composition
    // path answers, whatever net is open.
    let mut c = session();
    let r = run(&mut c, "bundle.receipt", json!({"scenario": "commit"}));
    let v = verify(&mut s, &r);
    assert_eq!(v["kind"], "composition-run", "{v}");
    assert_verdict(&v, "uncheckable");
}

// ------------------------------------------------------------ a loosely written content document

/// `store.put {document}` stored a content document exactly as given, so one written the way the
/// prototype writes it (`check` as an object) sat under a CID nothing else names: `content.export`,
/// `about` and a receipt's `content` all name the normalised document. It is now stored as that
/// document, and the receipt verifies from the store in a fresh session.
#[test]
fn a_loose_content_document_is_stored_as_the_one_every_consumer_names() {
    world();
    let loose = content();
    assert!(loose["entries"][2]["check"].is_object(), "written loose");
    assert_ne!(cid::seal(&loose).unwrap(), content_cid(&loose));

    let mut s = session_with(loose.clone());
    let receipt = run(
        &mut s,
        "content.receipt",
        json!({"entry": "commit-completes"}),
    );
    let named = receipt["receipt"]["scenario"]["content"].clone();
    assert_eq!(named, json!(content_cid(&loose)));

    let put = run(&mut s, "store.put", json!({"document": loose}));
    assert_eq!(
        put["cid"], named,
        "stored under the CID the receipt names: {put}"
    );
    assert!(
        put["note"].as_str().unwrap().contains("normalised"),
        "it says it changed the spelling: {put}"
    );
    assert!(put["note"]
        .as_str()
        .unwrap()
        .contains(&cid::seal(&loose).unwrap()));
    // The stored document is the one content.export writes.
    let exported = run(&mut s, "content.export", json!({}));
    assert_eq!(
        store::open().get_sealed(named.as_str().unwrap()).unwrap(),
        exported["document"]
    );

    run(&mut s, "store.put", json!({"composition": true}));
    let mut fresh = Session::default();
    let v = verify(&mut fresh, &receipt);
    assert_valid(&v);
    assert_eq!(v["contentFrom"], "store", "{v}");

    // A document already in the normal form is stored without a word about it.
    let again = run(
        &mut s,
        "store.put",
        json!({"document": exported["document"]}),
    );
    assert_eq!(again["cid"], named);
    assert!(again.get("note").is_none(), "{again}");
}

#[test]
fn a_content_document_that_does_not_parse_is_stored_as_given_and_says_so() {
    world();
    let mut s = Session::default();
    let junk = json!({
        "@context": "https://pflow.xyz/schema", "@type": "ModelContent",
        "entries": "x", "junk": 1,
    });
    let put = run(&mut s, "store.put", json!({"document": junk}));
    assert_eq!(put["cid"], json!(cid::seal(&junk).unwrap()));
    assert!(
        put["note"]
            .as_str()
            .unwrap()
            .contains("does not parse as model content"),
        "{put}"
    );
    run(&mut s, "bundle.open", json!({}));
    let e = err(&mut s, "store.open", json!({"cid": put["cid"]}));
    assert!(e.starts_with("[CONTENT_FIELD]"), "{e}");
}

// ------------------------------------------------------------ labels, trust, and the content receipt's negative case

/// `receipt` re-signed by `key` after `edit`, with the key's id and public key embedded.
fn resign_with(
    receipt: &Value,
    key: &pv::ReceiptKey,
    edit: impl FnOnce(&mut pv::Receipt),
) -> Value {
    let mut r = pv::parse_receipt(receipt).unwrap();
    edit(&mut r);
    r.key_id = key.key_id();
    r.public_key = key.public_key();
    r.key_ephemeral = key.ephemeral();
    r.signature = pv::b64(&key.sign(&r.signing_bytes()));
    serde_json::to_value(&r).unwrap()
}

/// A replay runs the check or the script *in the receipt*. Without a comparison with the content
/// the receipt names, a receipt re-signed over a check that is trivially true, under the id of a
/// real entry of the real content, verified as that entry's.
#[test]
fn a_receipt_re_signed_over_another_check_under_a_real_entry_id_is_invalid() {
    world();
    let mut s = session();
    let out = run(
        &mut s,
        "content.receipt",
        json!({"entry": "commit-completes"}),
    );
    let trivial = json!({"claim": "state exists", "steps": [
        {"command": "bundle.state", "args": {}, "expect": [{"path": "", "op": "exists"}]}]});
    // First sign it with a made-up hash to learn what the replay of the trivial check gives, then
    // sign that: a forger's receipt whose result hash is exactly right.
    let probe = resign(&out["receipt"], |r| r.scenario["check"] = trivial.clone());
    let replay_hash = verify(&mut s, &probe)["replayHash"].clone();
    let forged = resign(&out["receipt"], |r| {
        r.scenario["check"] = trivial.clone();
        r.result_hash = replay_hash.as_str().unwrap().to_string();
    });
    let v = verify(&mut s, &forged);
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["resultMatches"], true, "the hash is right: {v}");
    assert!(
        v["bindingError"]
            .as_str()
            .unwrap()
            .contains("commit-completes"),
        "{v}"
    );
    assert_verdict(&v, "invalid");
    // The honest one is untouched, and says nothing of binding.
    let v = verify(&mut s, &out["receipt"]);
    assert_valid(&v);
    assert!(
        v.get("bindingError").is_none() && v.get("bindingNote").is_none(),
        "{v}"
    );
}

#[test]
fn a_run_receipt_whose_script_is_not_the_declared_scenarios_is_invalid() {
    world();
    let mut s = session();
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    assert_eq!(
        out["receipt"]["scenario"]["content"],
        json!(content_cid(&content()))
    );
    let other = |r: &mut pv::Receipt| r.scenario["scenario"]["script"] = json!(["orders/dequeue"]);
    let probe = resign(&out["receipt"], other);
    let replay_hash = verify(&mut s, &probe)["replayHash"]
        .as_str()
        .unwrap()
        .to_string();
    let forged = resign(&out["receipt"], |r| {
        other(r);
        r.result_hash = replay_hash.clone();
    });
    let v = verify(&mut s, &forged);
    assert_eq!(v["resultMatches"], true, "{v}");
    assert!(
        v["bindingError"].as_str().unwrap().contains("\"commit\""),
        "{v}"
    );
    assert_verdict(&v, "invalid");
    // A bare script has no declared scenario to compare with.
    let script = run(
        &mut s,
        "bundle.receipt",
        json!({"script": ["orders/dequeue"]}),
    );
    assert_valid(&verify(&mut s, &script["receipt"]));
}

#[test]
fn a_run_receipt_whose_content_is_gone_is_valid_and_says_the_label_is_unchecked() {
    world();
    let mut s = session();
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    // Another content on the same composition: the receipt's content is nowhere.
    let mut t = Session::default();
    run(
        &mut t,
        "bundle.load",
        json!({"document": orders_document(), "content": content_with(vec![wrong_claim()])}),
    );
    run(&mut t, "content.clear", json!({}));
    let v = verify(&mut t, &out["receipt"]);
    assert_valid(&v);
    assert!(
        v["bindingNote"].as_str().unwrap().contains("unchecked"),
        "{v}"
    );
}

#[test]
fn a_foreign_key_is_not_trusted_and_an_ephemeral_one_says_so() {
    world();
    let mut s = session();
    let out = run(&mut s, "bundle.receipt", json!({"scenario": "commit"}));
    let own = verify(&mut s, &out["receipt"]);
    assert_eq!(own["trustedKey"], true, "{own}");
    assert!(own.get("keyNote").is_none(), "{own}");

    let foreign = pv::ReceiptKey::from_seed([7u8; 32], Some(PathBuf::from("/nowhere/key.json")));
    let theirs = resign_with(&out["receipt"], &foreign, |_| {});
    let v = verify(&mut s, &theirs);
    assert_valid(&v);
    assert_eq!(
        v["trustedKey"], false,
        "a valid signature by somebody else: {v}"
    );
    assert_ne!(v["keyId"], own["keyId"]);
    assert!(v.get("keyNote").is_none(), "{v}");

    let ephemeral = pv::ReceiptKey::from_seed([9u8; 32], None);
    let v = verify(&mut s, &resign_with(&out["receipt"], &ephemeral, |_| {}));
    assert_valid(&v);
    assert_eq!(v["trustedKey"], false, "{v}");
    assert_eq!(v["keyNote"], "signed under an ephemeral key", "{v}");

    // The net path answers the same keys the same way (they share one helper).
    let mut n = Session::default();
    run(&mut n, "file.open_example", json!({"name": "sir"}));
    let net = run(
        &mut n,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 3, "samples": 10}}),
    );
    let v = verify(&mut n, &resign_with(&net["receipt"], &ephemeral, |_| {}));
    assert_eq!(v["trustedKey"], false, "{v}");
    assert_eq!(v["keyNote"], "signed under an ephemeral key", "{v}");
}

/// A net receipt re-signed to claim a composition kind: the model it names is a net, not a
/// composition, so it cannot be replayed as one.
#[test]
fn a_net_receipt_claiming_a_composition_kind_is_uncheckable() {
    world();
    let mut n = Session::default();
    run(&mut n, "file.open_example", json!({"name": "sir"}));
    let net = run(
        &mut n,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 3, "samples": 10}}),
    );
    run(&mut n, "store.put", json!({}));
    let forged = resign(&net["receipt"], |r| {
        r.scenario = json!({"kind": "composition-run", "form": "composition", "content": null,
            "scenario": {"id": "x", "title": "x", "script": [], "expect": []}});
    });
    let v = verify(&mut n, &forged);
    assert_eq!(v["kind"], "composition-run", "{v}");
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_verdict(&v, "uncheckable");
}

/// What the content receipt signs for content that is not sound: `valid` is false, the unresolved
/// element ids are listed, and the replay reproduces both, so the receipt is `valid` as a record of
/// what was found.
#[test]
fn a_content_receipt_records_unsound_content_as_it_is() {
    world();
    let dormant = json!({
        "id": "ghost", "kind": "finding", "text": "about a place the model lacks",
        "about": ["place:orders/nope"],
    });
    let mut s = Session::default();
    run(
        &mut s,
        "bundle.load",
        json!({"document": orders_document(), "content": content()}),
    );
    run(
        &mut s,
        "content.load",
        json!({"document": content_with(vec![dormant]), "strict": false}),
    );
    let out = run(&mut s, "content.receipt", json!({}));
    assert_eq!(out["result"]["valid"], false, "{}", out["result"]);
    assert_eq!(
        out["result"]["unresolved"],
        json!([{"entry": "ghost", "ref": "place:orders/nope"}])
    );
    let v = verify(&mut s, &out["receipt"]);
    assert_valid(&v);
    // A receipt that claims the content was sound is not what the replay gives.
    let probe = resign(&out["receipt"], |r| {
        r.result_hash = format!("sha256:{}", "00".repeat(32))
    });
    let lie = verify(&mut s, &probe);
    assert_eq!(lie["resultMatches"], false);
    assert_verdict(&lie, "invalid");
}

/// A stored content document is only taken for the content its name says if it hashes to that name.
#[test]
fn a_stored_content_file_that_is_not_the_document_its_name_says_is_not_trusted() {
    let mem = world();
    let mut s = session();
    let receipt = run(
        &mut s,
        "content.receipt",
        json!({"entry": "commit-completes"}),
    );
    let named = content_cid(&content());
    let composition = run(&mut s, "bundle.export", json!({}));
    stash(&mem, &composition["document"]);
    let mut altered = content_document(&content());
    altered["name"] = json!("not the content the receipt names");
    plant(&mem, &named, &altered);

    let mut fresh = Session::default();
    let v = verify(&mut fresh, &receipt["receipt"]);
    assert_eq!(v["replayed"], false, "{v}");
    assert!(
        v["replayError"]
            .as_str()
            .unwrap()
            .contains("different document"),
        "{v}"
    );
    assert_verdict(&v, "uncheckable");
    // Put right, the same receipt verifies.
    plant(&mem, &named, &content_document(&content()));
    assert_valid(&verify(&mut fresh, &receipt["receipt"]));
}

/// A check cannot sign: `bundle.receipt` is a query but not a read of the model.
#[test]
fn a_check_that_asks_for_a_receipt_is_refused_at_load() {
    world();
    let signs = json!({
        "id": "signer", "kind": "finding", "text": "a check that signs", "about": ["model:"],
        "check": {"claim": "c", "steps": [
            {"command": "bundle.receipt", "args": {"script": ["orders/dequeue"]},
             "expect": [{"path": "/receipt/signature", "op": "exists"}]}]},
    });
    let mut s = Session::default();
    run(
        &mut s,
        "bundle.load",
        json!({"document": orders_document()}),
    );
    let e = err(
        &mut s,
        "content.load",
        json!({"document": content_with(vec![signs])}),
    );
    assert!(
        e.contains("[CHECK_COMMAND_FORBIDDEN]") && e.contains("not a read"),
        "{e}"
    );
}

// ------------------------------------------------------------ the store is addressed by content

fn model_fixture(name: &str) -> Value {
    let p = format!(
        "{}/tests/fixtures/model/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_slice(&std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}"))).unwrap()
}

/// Edits the stored file for `cid` in place (its name stays the same).
fn tamper(mem: &MemoryBackend, cid: &str, edit: impl FnOnce(&mut Value)) {
    let key = format!("models/{cid}.jsonld");
    let mut v: Value = serde_json::from_slice(&mem.get(&key).unwrap().unwrap()).unwrap();
    edit(&mut v);
    mem.put(&key, serde_json::to_string_pretty(&v).unwrap().as_bytes())
        .unwrap();
}

/// A file in the store that no longer seals to its name is not served as that CID: not by
/// `store.get`, `store.open`, `bundle.load {cid}`, nor as a part named by `cid` (where a tampered
/// net would otherwise stand in for the one the document names).
#[test]
fn a_store_file_that_is_not_the_document_its_name_says_is_refused_on_every_read() {
    let mem = world();
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    let put = run(&mut s, "store.put", json!({"composition": true}));
    let named = put["cid"].as_str().unwrap().to_string();

    // Untouched, it reads.
    run(&mut s, "store.get", json!({"cid": named}));
    run(&mut s, "bundle.load", json!({"cid": named}));

    tamper(&mem, &named, |v| v["name"] = json!("TAMPERED"));
    for (command, args) in [
        ("store.get", json!({"cid": named})),
        ("store.open", json!({"cid": named})),
        ("bundle.load", json!({"cid": named})),
    ] {
        let e = err(&mut s, command, args);
        assert!(
            e.starts_with("[STORE_ALTERED] ") && e.contains("different document"),
            "{command}: {e}"
        );
        assert!(e.contains(&named), "{command} names the CID: {e}");
    }

    // A part named by cid is a net, and a net is read as it always was (see
    // `a_stored_net_is_read_without_sealing_it`). A file there that is another kind of document
    // and does not seal to the name is not served as the part.
    let maker = model_fixture("part-maker.jsonld");
    let taker = model_fixture("part-taker.jsonld");
    let part = run(&mut s, "store.put", json!({"document": maker}))["cid"]
        .as_str()
        .unwrap()
        .to_string();
    run(&mut s, "store.put", json!({"document": taker}));
    let handoff = model_fixture("compose-handoff.jsonld");
    run(&mut s, "bundle.load", json!({"document": handoff.clone()}));
    plant(&mem, &part, &orders_document());
    let e = err(&mut s, "bundle.load", json!({"document": handoff}));
    assert!(e.contains("BUNDLE_PART_UNRESOLVED"), "{e}");
}

/// A second handle on the same files: what another window, or the next run, has. It has not
/// sealed anything yet.
fn second_handle(mem: &Arc<MemoryBackend>) {
    store::set_thread_store(Some(Arc::new(Store::new(Shared(mem.clone())))));
}

fn cid_of(v: &Value) -> String {
    v["cid"].as_str().unwrap().to_string()
}

fn seals_since(before: u64) -> u64 {
    cid::seals_on_this_thread() - before
}

/// A net is read as it always was: parsed, and not sealed. A net seals in time that grows with
/// the square of its arcs (connect-4's takes tens of seconds), and every net the editor opens
/// comes through the store's read. `store.get`, `store.open`, a bundle's `cid` part and
/// `store.supersede` pay nothing for the name being a seal, and a net file changed in place is
/// read like any other net (the other kinds are checked: the next test).
#[test]
fn a_stored_net_is_read_without_sealing_it() {
    let mem = world();
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let net = cid_of(&run(&mut s, "store.put", json!({})));

    let before = cid::seals_on_this_thread();
    store::open().get_model(&net).unwrap();
    run(&mut s, "store.get", json!({"cid": net}));
    run(&mut s, "store.open", json!({"cid": net}));
    assert_eq!(seals_since(before), 0, "reading a net seals nothing");

    // Naming nets as the parts of a Bundle: the one seal is the Bundle's own CID.
    let maker = model_fixture("part-maker.jsonld");
    let taker = model_fixture("part-taker.jsonld");
    run(&mut s, "store.put", json!({"document": maker}));
    run(&mut s, "store.put", json!({"document": taker}));
    let before = cid::seals_on_this_thread();
    run(
        &mut s,
        "bundle.load",
        json!({"document": model_fixture("compose-handoff.jsonld")}),
    );
    assert_eq!(
        seals_since(before),
        2,
        "the Bundle's own CID twice (its export, its model identity), and its parts not at all"
    );

    tamper(&mem, &net, |v| v["x-altered"] = json!(true));
    let before = cid::seals_on_this_thread();
    let got = run(&mut s, "store.get", json!({"cid": net}));
    assert_eq!(got["x-altered"], true, "a net file is served as it is");
    assert_eq!(seals_since(before), 0);
}

/// A composition, a bundle and a content document are addressed by their content and read as
/// that: a file that does not seal to its name is refused (`[STORE_ALTERED]`). The seal is paid
/// once per handle for the same bytes, so a document read twice, or by `store.open` after
/// `bundle.load {cid}`, is sealed once; a document the handle put is known to seal; and a file
/// that changed is sealed again, whatever the handle remembers of the bytes before.
#[test]
fn the_other_kinds_are_sealed_once_per_handle_and_again_when_the_bytes_change() {
    let mem = world();
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    let comp = cid_of(&run(&mut s, "store.put", json!({"composition": true})));
    let bundle = cid_of(&run(
        &mut s,
        "store.put",
        json!({"document": serde_json::from_str::<Value>(include_str!(
            "fixtures/bundle/cafe.bundle.json"
        )).unwrap()}),
    ));
    let content = cid_of(&run(
        &mut s,
        "store.put",
        json!({"document": content_document(&content())}),
    ));
    let kinds = [
        ("composition", &comp),
        ("bundle", &bundle),
        ("content", &content),
    ];

    // The handle that put them knows they seal.
    let before = cid::seals_on_this_thread();
    for (what, cid) in kinds {
        store::open()
            .get_model(cid)
            .unwrap_or_else(|e| panic!("{what}: {e}"));
    }
    assert_eq!(
        seals_since(before),
        0,
        "what this handle put is not sealed again"
    );

    // Another handle has to look once, and once is enough, whichever way it reads.
    second_handle(&mem);
    for (what, cid) in kinds {
        let before = cid::seals_on_this_thread();
        store::open().get_model(cid).unwrap();
        store::open().get_sealed(cid).unwrap();
        assert_eq!(seals_since(before), 1, "{what}: one seal for two reads");
    }
    // `store.open` of a composition reads it twice (its kind, then its document): the check is
    // one seal on a handle that has not seen the file, and none on one that has.
    let open_it = || {
        let before = cid::seals_on_this_thread();
        run(&mut Session::default(), "store.open", json!({"cid": comp}));
        seals_since(before)
    };
    second_handle(&mem);
    let (first, second) = (open_it(), open_it());
    assert_eq!(first, second + 1, "store.open checks the composition once");

    // A changed file is read afresh, refused, and the refusal is not remembered.
    for (what, cid) in kinds {
        tamper(&mem, cid, |v| v["name"] = json!("TAMPERED"));
        for _ in 0..2 {
            let before = cid::seals_on_this_thread();
            let e = store::open().get_model(cid).unwrap_err().to_string();
            assert!(
                e.starts_with("[STORE_ALTERED] ") && e.contains("different document"),
                "{what}: {e}"
            );
            assert_eq!(
                seals_since(before),
                1,
                "{what}: sealed again, refused again"
            );
        }
    }
}

/// `store.supersede` names the kinds of the two documents from the index: reading them to do it
/// would seal them.
#[test]
fn store_supersede_names_the_kinds_without_reading_the_documents() {
    let mem = world();
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let net = cid_of(&run(&mut s, "store.put", json!({})));
    run(&mut s, "bundle.open", json!({}));
    let comp = cid_of(&run(&mut s, "store.put", json!({"composition": true})));
    second_handle(&mem);

    let before = cid::seals_on_this_thread();
    let out = run(&mut s, "store.supersede", json!({"old": net, "new": comp}));
    assert_eq!(seals_since(before), 0, "{out}");
    assert!(
        out["note"]
            .as_str()
            .unwrap()
            .ends_with("the superseded document is a net and its successor a composition"),
        "{out}"
    );
}

/// A part named by `cid` that the store has a file for and does not serve is reported with the
/// cause (altered, or not a net), not as missing from the store; a CID it does not have still is.
#[test]
fn a_part_the_store_refuses_is_reported_with_its_cause_not_as_missing() {
    let mem = world();
    let mut s = Session::default();
    let handoff = model_fixture("compose-handoff.jsonld");
    let maker = handoff["subnets"][0]["cid"].as_str().unwrap().to_string();

    // Not in the store at all.
    let e = err(&mut s, "bundle.load", json!({"document": handoff.clone()}));
    assert!(e.contains("BUNDLE_PART_UNRESOLVED"), "{e}");
    assert!(e.contains("is not in the local store"), "{e}");

    // A file there that is another document than its name says.
    plant(&mem, &maker, &orders_document());
    let e = err(&mut s, "bundle.load", json!({"document": handoff.clone()}));
    assert!(
        e.contains("BUNDLE_PART_UNRESOLVED") && e.contains("subnets[0]"),
        "{e}"
    );
    assert!(e.contains(&maker), "{e}");
    assert!(e.contains("[STORE_ALTERED] "), "the cause is reported: {e}");
    assert!(
        !e.contains("is not in the local store"),
        "it is in the store: {e}"
    );

    // A document of another kind, stored under its own name: not a net.
    run(&mut s, "bundle.open", json!({}));
    let comp = cid_of(&run(&mut s, "store.put", json!({"composition": true})));
    let mut named = handoff;
    named["subnets"][0]["cid"] = json!(comp);
    let e = err(&mut s, "bundle.load", json!({"document": named}));
    assert!(e.contains("BUNDLE_PART_UNRESOLVED"), "{e}");
    assert!(e.contains("it is a composition document, not a net"), "{e}");
    assert!(!e.contains("is not in the local store"), "{e}");
}

/// Putting a document whose file is already there is idempotent, except for a file of the kinds
/// the store checks on read that does not seal to its name: the document being put is the proof
/// of what the name stands for, so the file is written over (`existed: true, repaired: true`)
/// and reads again. An intact file is left alone and costs no more than the first put did, and
/// a net's file is left as it is.
#[test]
fn putting_a_document_again_repairs_an_altered_file_of_the_checked_kinds_only() {
    let mem = world();
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    let cafe: Value =
        serde_json::from_str(include_str!("fixtures/bundle/cafe.bundle.json")).unwrap();
    let content = content_document(&content());
    let puts = [
        ("composition", json!({"composition": true})),
        ("bundle", json!({"document": cafe})),
        ("content", json!({"document": content})),
    ];
    for (what, args) in puts {
        let before = cid::seals_on_this_thread();
        let first = run(&mut s, "store.put", args.clone());
        let cost = seals_since(before);
        let cid = cid_of(&first);
        assert_eq!(first["existed"], false, "{what}: {first}");
        assert!(first.get("repaired").is_none(), "{what}: {first}");

        // Again, untouched: nothing to repair, and no seal of the file already there.
        let before = cid::seals_on_this_thread();
        let again = run(&mut s, "store.put", args.clone());
        assert_eq!(again["existed"], true, "{what}: {again}");
        assert!(again.get("repaired").is_none(), "{what}: {again}");
        assert_eq!(
            seals_since(before),
            cost,
            "{what}: the file is not sealed to be left alone"
        );

        // Altered in place: refused on read, and put over by the document.
        tamper(&mem, &cid, |v| v["name"] = json!("TAMPERED"));
        let e = err(&mut s, "store.get", json!({"cid": cid}));
        assert!(e.starts_with("[STORE_ALTERED] "), "{what}: {e}");
        let fixed = run(&mut s, "store.put", args.clone());
        assert_eq!(fixed["cid"], cid, "{what}: {fixed}");
        assert_eq!(fixed["existed"], true, "{what}: {fixed}");
        assert_eq!(fixed["repaired"], true, "{what}: {fixed}");
        assert!(
            fixed["notes"][0].as_str().unwrap().contains("did not seal"),
            "{what}: says so: {fixed}"
        );
        let got = run(&mut s, "store.get", json!({"cid": cid}));
        assert_ne!(
            got["name"], "TAMPERED",
            "{what}: the file is the document again"
        );
        // And a handle that never saw the file reads it.
        second_handle(&mem);
        store::open()
            .get_model(&cid)
            .unwrap_or_else(|e| panic!("{what}: {e}"));
        let last = run(&mut s, "store.put", args);
        assert!(last.get("repaired").is_none(), "{what}: {last}");
    }

    // A net's file is left as it is: it is not checked on read, so there is nothing to repair.
    let mut n = Session::default();
    run(&mut n, "file.open_example", json!({"name": "sir"}));
    let net = cid_of(&run(&mut n, "store.put", json!({})));
    tamper(&mem, &net, |v| v["x-altered"] = json!(true));
    let again = run(&mut n, "store.put", json!({}));
    assert_eq!(again["existed"], true, "{again}");
    assert!(again.get("repaired").is_none(), "{again}");
    assert_eq!(
        run(&mut n, "store.get", json!({"cid": net}))["x-altered"],
        true
    );
}

/// One saga instance of `n` steps (no moves) and a content document for it: a declared scenario
/// that runs every step and commits, and a finding whose check runs it. The saga flattens to a
/// Bundle whose arcs are a list `n` long, which is what sealing it is quadratic in.
fn saga_doc(n: usize) -> Value {
    let steps: Vec<Value> = (0..n)
        .map(|i| {
            json!({"id": format!("s{i}"), "name": format!("step {i}"),
            "compensation": format!("undo {i}"), "moves": []})
        })
        .collect();
    json!({"@context": "https://pflow.xyz/schema", "@type": "PatternComposition", "name": "t",
        "instances": [{"id": "payment", "pattern": "saga", "params": {
            "id": "payment",
            "accounts": {"customer": {"initial": 1, "shared": false},
                         "merchant": {"initial": 0, "shared": false}},
            "steps": steps}}],
        "links": []})
}

fn saga_content(n: usize) -> Value {
    let mut script: Vec<Value> = (0..n).map(|i| json!(format!("payment/do_s{i}"))).collect();
    script.push(json!("payment/commit"));
    json!({
        "@context": "https://pflow.xyz/schema", "@type": "ModelContent", "name": "saga",
        "subject": {"kind": "composition", "name": "t"},
        "entries": [
            declared("all", "Every step runs", json!(script), json!([
                {"place": "payment/completed", "op": "==", "n": 1, "says": "the saga completes"},
            ])),
            {"id": "completes", "kind": "finding", "order": 1, "text": "The saga completes.",
             "about": ["scenario:all"],
             "check": run_check("the run completes", "all", json!([
                {"path": "/completed", "op": "==", "value": true},
             ]))},
        ],
    })
}

/// `command` in `s`, and how many seals it performed.
fn counted(s: &mut Session, command: &str, args: Value) -> (Value, u64) {
    let before = cid::seals_on_this_thread();
    let out = run(s, command, args);
    (out, seals_since(before))
}

/// Receipts, their verification and the state of a composition with content attached name it by
/// its recipe and its content and never seal the Bundle it flattens to, which is the one costly
/// seal (the cost of a net is its size squared). A saga of 10 steps flattens to a Bundle that
/// needs 153 thousand steps to seal; the limit is set below that, so a command that sealed it
/// would be refused, and the seals each command performs are counted, like `dump_state`'s: the
/// document's CID and the content's, and no more.
#[test]
fn receipts_their_verification_and_the_state_never_seal_the_flattened_bundle() {
    world();
    let n = 10;
    let mut s = Session::default();
    run(
        &mut s,
        "bundle.load",
        json!({"document": saga_doc(n), "content": saga_content(n)}),
    );
    let plain = run(&mut s, "bundle.export", json!({"form": "bundle"}))["document"].clone();
    cid::limit_work_on_this_thread(Some(50_000));
    assert!(
        cid::is_too_complex(&cid::seal(&plain).unwrap_err()),
        "the premise: the flattened Bundle is past the limit"
    );

    let (state, seals) = counted(&mut s, "bundle.state", Value::Null);
    assert_eq!(state["content"]["entries"], 2, "{state}");
    assert_eq!(seals, 0, "bundle.state seals nothing");

    let (run_receipt, seals) = counted(&mut s, "bundle.receipt", json!({"scenario": "all"}));
    assert_eq!(seals, 2, "the document's CID and the content's");
    let (verdict, seals) = counted(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": run_receipt["receipt"]}),
    );
    assert_valid(&verdict);
    assert_eq!(seals, 2, "the replay names the document and the content");

    let (check_receipt, seals) = counted(&mut s, "content.receipt", json!({"entry": "completes"}));
    assert_eq!(
        seals, 4,
        "the recipe and the content, a few times over; no Bundle"
    );
    let (verdict, seals) = counted(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": check_receipt["receipt"]}),
    );
    assert_valid(&verdict);
    assert_eq!(seals, 4);

    let (content_receipt, seals) = counted(&mut s, "content.receipt", json!({}));
    assert_eq!(seals, 4);
    let (verdict, seals) = counted(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": content_receipt["receipt"]}),
    );
    assert_valid(&verdict);
    assert_eq!(seals, 4);
    cid::limit_work_on_this_thread(None);
}

// ------------------------------------------------------------ signature-only verification

fn verify_signature_only(s: &mut Session, receipt: &Value) -> Value {
    run(
        s,
        "provenance.verify_receipt",
        json!({"receipt": receipt, "replay": false}),
    )
}

/// With `replay: false` the signature is the only gate: `replay_ok` is true by construction, so a
/// verdict that forgot the signature would call a forged receipt valid. Run, check and content
/// receipts, each altered after signing (the claim, the signature) are invalid without a replay,
/// and a receipt signed by another key is valid but not trusted.
#[test]
fn a_forged_receipt_is_invalid_without_a_replay_for_every_kind() {
    world();
    let mut s = session();
    let receipts = [
        (
            "composition-run",
            run(&mut s, "bundle.receipt", json!({"scenario": "commit"})),
        ),
        (
            "check",
            run(
                &mut s,
                "content.receipt",
                json!({"entry": "commit-completes"}),
            ),
        ),
        ("content", run(&mut s, "content.receipt", json!({}))),
    ];
    let foreign = pv::ReceiptKey::from_seed([7u8; 32], Some(PathBuf::from("/nowhere/key.json")));
    for (what, out) in receipts {
        let good = &out["receipt"];
        let v = verify_signature_only(&mut s, good);
        assert_eq!(v["verdict"], "valid", "{what}: {v}");
        assert_eq!(v["signatureValid"], true, "{what}: {v}");
        assert!(v.get("replayed").is_none(), "{what}: {v}");

        // The claim changed after it was signed.
        let mut claim = good.clone();
        claim["resultHash"] = json!(format!("sha256:{}", "0".repeat(64)));
        let v = verify_signature_only(&mut s, &claim);
        assert_eq!(v["verdict"], "invalid", "{what}: {v}");
        assert_eq!(v["signatureValid"], false, "{what}: {v}");
        assert!(
            v.get("replayed").is_none() && v.get("resultMatches").is_none(),
            "{what}: nothing was replayed: {v}"
        );

        // The signature changed.
        let mut sig = good.clone();
        let text = sig["signature"].as_str().unwrap().to_string();
        let last = if text.ends_with('A') { 'B' } else { 'A' };
        sig["signature"] = json!(format!("{}{last}", &text[..text.len() - 1]));
        let v = verify_signature_only(&mut s, &sig);
        assert_eq!(v["verdict"], "invalid", "{what}: {v}");
        assert_eq!(v["signatureValid"], false, "{what}: {v}");

        // Signed again by somebody else: the signature holds, the key is not ours.
        let theirs = resign_with(good, &foreign, |_| {});
        let v = verify_signature_only(&mut s, &theirs);
        assert_eq!(v["verdict"], "valid", "{what}: {v}");
        assert_eq!(v["trustedKey"], false, "{what}: {v}");
        assert!(v.get("replayed").is_none(), "{what}: {v}");
    }
}

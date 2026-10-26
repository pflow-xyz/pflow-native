//! Phase 7 provenance: signed run receipts, local witness checking and
//! (with the `zk` feature) Groth16 proofs of firings.
//!
//! Nothing here touches the real user data directory: every test points
//! PFLOW_NATIVE_HOME at a per-process temporary directory first.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use pflow_native_core::provenance::{self as pv, load_or_create_key};
use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

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
                        .starts_with("pn-provenance-test-")
                {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        let d = std::env::temp_dir().join(format!("pn-provenance-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("PFLOW_NATIVE_HOME", &d);
        d
    })
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

fn example(name: &str) -> Session {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({ "name": name }));
    s
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

// ================================================================ receipts

#[test]
fn receipt_signs_and_verifies() {
    let mut s = example("sir");
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 2, "realizations": 3, "seed": 7, "samples": 20}}),
    );
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
    assert_eq!(r["model"], run(&mut s, "file.cid", json!({}))["cid"]);
    assert!(r["resultHash"].as_str().unwrap().starts_with("sha256:"));
    assert_eq!(r["engine"], "ssa");
    // The certified scenario carries everything a replay needs.
    let sc = &r["scenario"];
    assert_eq!(sc["seed"], 7);
    assert_eq!(sc["marking"], json!({"S": 999, "I": 1, "R": 0}));
    assert_eq!(sc["rates"]["infect"].as_f64(), Some(1.0));
    assert_eq!(sc["rates"]["recover"].as_f64(), Some(1.0));
    // The key is this machine's, kept under PFLOW_NATIVE_HOME.
    let key = run(&mut s, "provenance.key", json!({}));
    assert_eq!(key["keyId"], r["keyId"]);
    assert!(key.get("seed").is_none() && !key.to_string().contains("seed\""));
    assert!(home().join(pv::KEY_FILE).exists());

    // The canvas marking and session rates do not enter the replay.
    run(&mut s, "sim.fire", json!({"id": "infect"}));
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 3.0}),
    );
    let v = run(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": out.clone()}),
    );
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["trustedKey"], true, "{v}");
    assert_eq!(v["replayed"], true, "{v}");
    assert_eq!(v["resultMatches"], true, "{v}");
    assert_eq!(v["verdict"], "valid");
    // The stored report holds {result, receipt}.
    let rep = report(&s, &out["report"]);
    assert_eq!(rep["receipt"], *r);
}

#[test]
fn tampered_receipts_fail() {
    let mut s = example("sir");
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 3, "summary": true}}),
    );
    let good = out["receipt"].clone();

    let mut bad = good.clone();
    bad["resultHash"] = json!("sha256:0000");
    let v = run(&mut s, "provenance.verify_receipt", json!({"receipt": bad}));
    assert_eq!(v["signatureValid"], false);
    assert_eq!(v["verdict"], "invalid");
    assert!(v["replayError"].as_str().unwrap().contains("not replayed"));

    let mut bad = good.clone();
    bad["scenario"]["seed"] = json!(4);
    let v = run(&mut s, "provenance.verify_receipt", json!({"receipt": bad}));
    assert_eq!(v["signatureValid"], false);
    assert_eq!(v["verdict"], "invalid");

    let mut bad = good.clone();
    bad["extra"] = json!(1);
    assert!(
        err(&mut s, "provenance.verify_receipt", json!({"receipt": bad})).contains("unknown field")
    );

    // A well-signed receipt (another key) for a result the model does not
    // produce: the signature holds, the replay exposes it.
    let other = pv::ReceiptKey::from_seed([9u8; 32], None);
    let forged = pv::issue(
        &other,
        good["model"].as_str().unwrap(),
        &BTreeMap::new(),
        good["scenario"].clone(),
        "sha256:feed".into(),
        "ssa",
        pv::versions(pv::DIGEST_EXACT),
        0,
    );
    let v = run(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": forged}),
    );
    assert_eq!(v["signatureValid"], true);
    assert_eq!(v["trustedKey"], false);
    assert_eq!(v["resultMatches"], false);
    assert_eq!(v["verdict"], "invalid");

    // An edited net is a different model: the replay cannot find it.
    run(&mut s, "net.add_place", json!({"x": 10, "y": 10}));
    let v = run(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": good.clone()}),
    );
    assert_eq!(v["signatureValid"], true);
    assert_eq!(v["verdict"], "uncheckable");
    assert!(v["replayError"].as_str().unwrap().contains("not found"));
    // ... unless the original is in the library.
    run(&mut s, "edit.undo", json!({}));
    run(&mut s, "model.save", json!({"name": "orig"}));
    run(&mut s, "net.add_place", json!({"x": 10, "y": 10}));
    let v = run(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": good}),
    );
    assert_eq!(v["verdict"], "valid", "{v}");
    assert_eq!(v["modelFrom"], "orig");
}

#[test]
fn a_rated_net_saved_to_the_library_verifies() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cid/with-rates.jsonld");
    let mut s = Session::default();
    run(&mut s, "file.open", json!({ "path": path }));
    assert!(!s.rates.is_empty());
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 5, "summary": true}}),
    );
    assert_eq!(
        out["receipt"]["model"],
        run(&mut s, "file.cid", json!({}))["cid"]
    );
    run(&mut s, "model.save", json!({"name": "rated"}));
    run(&mut s, "file.new", json!({}));
    let v = run(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": out["receipt"]}),
    );
    assert_eq!(v["modelFrom"], "rated", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
}

/// The model a receipt names is the net under the session rates at issue,
/// so it stops being the open net's file.cid once any rate moves. The
/// receipt still finds it: open, or in the library under other rates.
#[test]
fn a_receipt_survives_a_rate_change() {
    let mut s = example("sir");
    // An integral rate and a fraction: both must rebuild to the same CID.
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": 0.002}),
    );
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 3.0}),
    );
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 2, "realizations": 3, "seed": 7, "summary": true}}),
    );
    let r = out["receipt"].clone();
    assert_eq!(r["model"], run(&mut s, "file.cid", json!({}))["cid"]);
    // Recorded as Go writes them.
    assert_eq!(r["modelRates"], json!({"infect": 0.002, "recover": 3}));

    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 0.5}),
    );
    assert_ne!(r["model"], run(&mut s, "file.cid", json!({}))["cid"]);
    let v = verify(&mut s, &r);
    assert_eq!(v["modelFrom"], "current", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");

    // Saved under the new rates, with another net open.
    run(&mut s, "model.save", json!({"name": "retuned"}));
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let v = verify(&mut s, &r);
    assert_eq!(v["modelFrom"], "retuned", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
}

/// A net that matches as it stands is found before one rebuilt under
/// modelRates: a library copy saved at issue answers by its name, as it
/// would have before modelRates, though the open net rebuilds too.
#[test]
fn an_exact_match_comes_before_a_rebuild() {
    let mut s = example("sir");
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 3.0}),
    );
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 3, "summary": true}}),
    );
    let r = out["receipt"].clone();
    run(&mut s, "model.save", json!({"name": "as-issued"}));
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 0.5}),
    );
    let v = verify(&mut s, &r);
    assert_eq!(v["modelFrom"], "as-issued", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
}

/// The same for a net whose file carries its rates (integers among them).
#[test]
fn a_rated_files_receipt_survives_a_rate_change() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cid/with-rates.jsonld");
    let mut s = Session::default();
    run(&mut s, "file.open", json!({ "path": path }));
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 5, "summary": true}}),
    );
    let r = out["receipt"].clone();
    assert_eq!(
        r["modelRates"],
        json!({"BrewCoffee": 0.25, "CustomerArrives": 2, "CustomerLeaves": 0.001, "RestockBeans": 720, "RestockCups": 0})
    );
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "CustomerArrives", "rate": 4}),
    );
    let v = verify(&mut s, &r);
    assert_eq!(v["modelFrom"], "current", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");

    run(&mut s, "model.save", json!({"name": "busier"}));
    run(&mut s, "file.new", json!({}));
    let v = verify(&mut s, &r);
    assert_eq!(v["modelFrom"], "busier", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
}

/// A receipt for a net with no session rates names the bare net, which a
/// library copy saved after rates were set still is, less its rates.
#[test]
fn a_rate_less_receipt_finds_its_net_saved_with_rates() {
    let mut s = example("sir");
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 3, "summary": true}}),
    );
    let r = out["receipt"].clone();
    // No rates, no field: the receipt has the shape it had before.
    assert!(r.get("modelRates").is_none(), "{r}");
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": 0.002}),
    );
    run(&mut s, "model.save", json!({"name": "rated-later"}));
    run(&mut s, "file.new", json!({}));
    let v = verify(&mut s, &r);
    assert_eq!(v["modelFrom"], "rated-later", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
}

/// modelRates is signed, and only ever rebuilds the net the receipt names:
/// rates that do not seal to `model` leave it not found.
#[test]
fn model_rates_are_signed_and_match_nothing_else() {
    let mut s = example("sir");
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 3.0}),
    );
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"hours": 1, "realizations": 2, "seed": 3, "summary": true}}),
    );
    let good = out["receipt"].clone();
    assert_eq!(good["modelRates"], json!({"recover": 3}));
    // Off the rates at issue: only modelRates finds the model now.
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "recover", "rate": 0.5}),
    );

    // Edited or stripped, the signature fails.
    let mut bad = good.clone();
    bad["modelRates"]["recover"] = json!(0.5);
    let v = verify(&mut s, &bad);
    assert_eq!(v["signatureValid"], false, "{v}");
    assert_eq!(v["verdict"], "invalid");
    let mut bad = good.clone();
    bad.as_object_mut().unwrap().remove("modelRates");
    let v = verify(&mut s, &bad);
    assert_eq!(v["signatureValid"], false, "{v}");
    assert_eq!(v["verdict"], "invalid");

    // Re-signed under another key with other rates: the signature holds,
    // but the net under those rates is another CID.
    let other = pv::ReceiptKey::from_seed([9u8; 32], None);
    let resign = |rates: &[(&str, f64)]| {
        let rates: BTreeMap<String, f64> = rates.iter().map(|(t, r)| (t.to_string(), *r)).collect();
        let r = pv::issue(
            &other,
            good["model"].as_str().unwrap(),
            &rates,
            good["scenario"].clone(),
            good["resultHash"].as_str().unwrap().into(),
            "ssa",
            pv::versions(pv::DIGEST_EXACT),
            0,
        );
        serde_json::to_value(r).unwrap()
    };
    let v = verify(&mut s, &resign(&[("recover", 3.0)]));
    assert_eq!(v["modelFrom"], "current", "control: {v}");
    assert_eq!(v["trustedKey"], false, "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
    for rates in [
        &[("recover", 0.5)][..],
        // An explicit 1 is not an absent rate: another document.
        &[("recover", 3.0), ("infect", 1.0)],
        &[("nope", 3.0)],
    ] {
        let v = verify(&mut s, &resign(rates));
        assert_eq!(v["signatureValid"], true, "{rates:?}: {v}");
        assert_eq!(v["verdict"], "uncheckable", "{rates:?}: {v}");
        assert!(
            v["replayError"].as_str().unwrap().contains("not found"),
            "{v}"
        );
    }
    // None at all reads as a receipt from before modelRates: the rates its
    // scenario ran with (recover 3, infect the default 1) rebuild the net.
    let v = verify(&mut s, &resign(&[]));
    assert_eq!(v["modelFrom"], "current", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");

    // A signed modelRates that is not numbers rebuilds nothing.
    let mut odd = pv::parse_receipt(&good).unwrap();
    odd.model_rates.insert("recover".into(), json!("fast"));
    odd.key_id = other.key_id();
    odd.public_key = other.public_key();
    odd.signature = pv::b64(&other.sign(&odd.signing_bytes()));
    let v = verify(&mut s, &serde_json::to_value(&odd).unwrap());
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["verdict"], "uncheckable", "{v}");

    // The right rates on an edited net, open or in the library, are
    // another net.
    run(&mut s, "net.add_place", json!({"x": 10, "y": 10}));
    let v = verify(&mut s, &good);
    assert_eq!(v["verdict"], "uncheckable", "{v}");
    run(&mut s, "model.save", json!({"name": "edited"}));
    run(&mut s, "file.new", json!({}));
    let v = verify(&mut s, &good);
    assert_eq!(v["verdict"], "uncheckable", "{v}");
    assert!(v["replayError"].as_str().unwrap().contains("not found"));
}

/// Receipts origin/main 1a44427 signed, before receipts recorded
/// `modelRates`, under a scratch key. Made with that commit's CLI
/// (`cargo build -p pflow-native-cli`), PFLOW_NATIVE_HOME pointing at a
/// scratch directory, then `.results[-1].receipt` of the output kept:
///
/// - with-rates: `batch -i tests/fixtures/cid/with-rates.jsonld STEPS`,
///   STEPS `[sim.receipt {scenario: {hours: 1, realizations: 2, seed: 5, summary: true}}]`
/// - sir: `batch [file.open_example {name: sir}, sim.receipt {scenario:
///   {hours: 2, realizations: 3, seed: 7, summary: true}}]`
/// - sir-set-rates: the same, with `sim.set_rate {id: infect, rate: 0.002}`
///   and `sim.set_rate {id: recover, rate: 3.0}` before sim.receipt
/// - sir-one-rate: the same as sir, with only `sim.set_rate {id: infect,
///   rate: 0.002}` before sim.receipt
/// - sir-overridden: sir-set-rates with `rates: {recover: 5}` in the scenario
fn main_receipt(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join(format!("tests/fixtures/receipts/main-1a44427-{name}.json"));
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// The net each main-1a44427 receipt was issued on, set up again.
fn main_receipt_net(name: &str) -> Session {
    let rated = |rates: &[(&str, f64)]| {
        let mut s = example("sir");
        for (id, rate) in rates {
            run(&mut s, "sim.set_rate", json!({"id": id, "rate": rate}));
        }
        s
    };
    match name {
        "with-rates" => {
            let mut s = Session::default();
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/cid/with-rates.jsonld");
            run(&mut s, "file.open", json!({ "path": path }));
            s
        }
        "sir" => example("sir"),
        "sir-set-rates" | "sir-overridden" => rated(&[("infect", 0.002), ("recover", 3.0)]),
        "sir-one-rate" => rated(&[("infect", 0.002)]),
        other => panic!("no fixture {other}"),
    }
}

const MAIN_RECEIPTS: &[&str] = &[
    "with-rates",
    "sir",
    "sir-set-rates",
    "sir-one-rate",
    "sir-overridden",
];

#[test]
fn receipts_main_issued_still_verify() {
    for name in MAIN_RECEIPTS {
        let r = main_receipt(name);
        assert!(r.get("modelRates").is_none(), "{name}");
        let mut s = main_receipt_net(name);
        let v = verify(&mut s, &r);
        assert_eq!(v["signatureValid"], true, "{name}: {v}");
        // Signed by the scratch key, not this test's.
        assert_eq!(v["trustedKey"], false, "{name}: {v}");
        assert_eq!(v["replayed"], true, "{name}: {v}");
        assert_eq!(v["resultMatches"], true, "{name}: {v}");
        assert_eq!(v["verdict"], "valid", "{name}: {v}");
        assert_eq!(v["modelFrom"], "current", "{name}: {v}");

        // And from the library, with nothing open.
        run(&mut s, "model.save", json!({"name": "lib"}));
        run(&mut s, "file.new", json!({}));
        let v = verify(&mut s, &r);
        assert_eq!(v["verdict"], "valid", "{name}: {v}");
        assert_eq!(v["modelFrom"], "lib", "{name}: {v}");
    }
}

/// A receipt from before modelRates that was issued with session rates
/// names the net under them, and records them only among the rates its
/// scenario ran with: the net is rebuilt from those once the session rates
/// move, open or saved under the moved rates. with-rates and sir-set-rates
/// set every rate; sir-one-rate left recover at the default 1, which the
/// scenario writes out and the model does not carry. A rate-less one names
/// the bare net, which any rates still strip to.
#[test]
fn a_receipt_from_before_model_rates_is_rebuilt_from_its_scenario() {
    for (name, id, moved) in [
        ("with-rates", "CustomerArrives", 4.0),
        ("sir-set-rates", "recover", 0.5),
        ("sir-one-rate", "infect", 0.01),
        ("sir-one-rate", "recover", 0.5),
        ("sir", "recover", 0.5),
    ] {
        let r = main_receipt(name);
        let mut s = main_receipt_net(name);
        run(&mut s, "sim.set_rate", json!({"id": id, "rate": moved}));
        let v = verify(&mut s, &r);
        assert_eq!(v["signatureValid"], true, "{name} {id}: {v}");
        assert_eq!(v["verdict"], "valid", "{name} {id}: {v}");
        assert_eq!(v["modelFrom"], "current", "{name} {id}: {v}");
        // Saved under the moved rates, with nothing open.
        run(&mut s, "model.save", json!({"name": "moved"}));
        run(&mut s, "file.new", json!({}));
        let v = verify(&mut s, &r);
        assert_eq!(v["verdict"], "valid", "{name} {id}: {v}");
        assert_eq!(v["modelFrom"], "moved", "{name} {id}: {v}");
    }
}

/// The limit of the fix: when the scenario overrode a session rate, a
/// receipt from before modelRates no longer holds the rate the model was
/// sealed under, so nothing rebuilds that net once the session rates move.
/// It is found again where they are: set back, or a library copy saved
/// under them.
#[test]
fn a_receipt_whose_scenario_overrode_a_rate_needs_its_rates() {
    let r = main_receipt("sir-overridden");
    assert_eq!(r["scenario"]["rates"]["recover"], 5);
    let set = |s: &mut Session, rate: f64| {
        run(s, "sim.set_rate", json!({"id": "recover", "rate": rate}));
    };
    let mut s = main_receipt_net("sir-overridden");
    set(&mut s, 0.5);
    let v = verify(&mut s, &r);
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["verdict"], "uncheckable", "{v}");
    assert!(
        v["replayError"].as_str().unwrap().contains("not found"),
        "{v}"
    );
    // The rates set back.
    set(&mut s, 3.0);
    let v = verify(&mut s, &r);
    assert_eq!(v["modelFrom"], "current", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
    // A copy saved under them, the rates moved again.
    run(&mut s, "model.save", json!({"name": "as-issued"}));
    set(&mut s, 0.5);
    let v = verify(&mut s, &r);
    assert_eq!(v["modelFrom"], "as-issued", "{v}");
    assert_eq!(v["verdict"], "valid", "{v}");
}

/// sim.pflow.xyz's own receipt from before its receipts recorded inputs:
/// sim-pflow-xyz `pkg/receipt/testdata/receipt-pre-inputs.json` at
/// baa2cd1b, copied unchanged (issued by sim 68b85b74, NewSigner over 32
/// zero bytes). Its signing bytes are sim's, so it verifies here.
#[test]
fn sims_pre_inputs_receipt_verifies() {
    let text = include_str!("fixtures/receipts/sim-receipt-pre-inputs.json");
    let r = pv::parse_receipt(&serde_json::from_str(text).unwrap()).unwrap();
    assert_eq!(r.verify_signature().unwrap(), "139e3940e64b5491");
    assert_eq!(
        r.public_key,
        pv::ReceiptKey::from_seed([0; 32], None).public_key()
    );
    // Re-encoding adds none of the fields sim lacks.
    let again = serde_json::to_value(&r).unwrap();
    for k in ["versions", "keyEphemeral", "modelRates"] {
        assert!(again.get(k).is_none(), "{k}: {again}");
    }
    // A real signature: one changed byte fails it.
    let mut bad = r.clone();
    bad.result_hash = "sha256:0000".into();
    assert!(bad.verify_signature().is_err());

    // Through the command: signed, not by this machine's key. Its model is
    // sim's store id, not a CID, so only the signature is checked.
    let mut s = Session::default();
    let v = run(
        &mut s,
        "provenance.verify_receipt",
        json!({"receipt": text, "replay": false}),
    );
    assert_eq!(v["signatureValid"], true, "{v}");
    assert_eq!(v["trustedKey"], false, "{v}");
}

#[test]
fn ode_receipts_replay() {
    let mut s = example("sir");
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": 0.002}),
    );
    let out = run(
        &mut s,
        "sim.receipt",
        json!({"scenario": {"engine": "ode", "hours": 10, "samples": 30}}),
    );
    assert_eq!(out["receipt"]["engine"], "ode");
    assert!(out["receipt"]["versions"]["resultDigest"]
        .as_str()
        .unwrap()
        .contains("rounded"));
    for _ in 0..3 {
        let v = run(
            &mut s,
            "provenance.verify_receipt",
            json!({"receipt": out["receipt"]}),
        );
        assert_eq!(v["verdict"], "valid", "{v}");
    }
}

#[test]
fn key_persists_across_loads() {
    let dir = std::env::temp_dir().join(format!("pn-provenance-key-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let a = load_or_create_key(&dir).unwrap();
    assert!(a.created);
    let b = load_or_create_key(&dir).unwrap();
    assert!(!b.created);
    assert_eq!(a.key_id(), b.key_id());
    assert_eq!(a.public_key(), b.public_key());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join(pv::KEY_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    // A signature from the first load verifies under the second.
    let r = pv::issue(
        &a,
        "zX",
        &BTreeMap::new(),
        json!({}),
        "sha256:00".into(),
        "ssa",
        json!({}),
        0,
    );
    assert_eq!(r.verify_signature().unwrap(), b.key_id());
    std::fs::remove_dir_all(&dir).unwrap();
}

// ================================================================ witnesses

#[test]
fn check_witness_reach_trace() {
    let mut s = example("sir");
    let out = run(&mut s, "analysis.reach", json!({"to": {"I": 0, "R": 1}}));
    assert_eq!(out["answer"], "yes");
    let trace = out["trace"].clone();
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"artifact": trace}),
    );
    assert_eq!(v["verdict"], "valid", "{v}");
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"reach": out.clone()}),
    );
    assert_eq!(v["verdict"], "valid", "{v}");

    // A tampered marking in the trace.
    let mut steps = match &s.artifact(trace.as_str().unwrap()).unwrap().data {
        ArtifactData::Trace { steps } => serde_json::to_value(steps).unwrap(),
        _ => unreachable!(),
    };
    steps[1]["marking"]["R"] = json!([2]);
    let v = run(&mut s, "provenance.check_witness", json!({"trace": steps}));
    assert_eq!(v["verdict"], "invalid");
    assert!(v["reason"].as_str().unwrap().contains("step 1"), "{v}");

    // A tampered reach answer: reached disagrees, then an illegal step.
    let mut bad = out.clone();
    bad["witness"]["trace"]["reached"]["R"] = json!([5]);
    let v = run(&mut s, "provenance.check_witness", json!({"reach": bad}));
    assert_eq!(v["verdict"], "invalid");
    let mut bad = out.clone();
    bad["witness"]["trace"]["steps"] = json!(["recover", "recover"]);
    let v = run(&mut s, "provenance.check_witness", json!({"reach": bad}));
    assert_eq!(v["verdict"], "invalid");

    // Transition ids alone: legality only.
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"trace": ["infect", "recover"]}),
    );
    assert_eq!(v["verdict"], "valid");
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"trace": ["recover", "recover"]}),
    );
    assert_eq!(v["verdict"], "invalid");
    assert!(err(&mut s, "provenance.check_witness", json!({})).contains("exactly one"));
}

#[test]
fn check_witness_verify_counterexample() {
    // Deadlock: SIR runs dry. verify stores the counterexample trace.
    let mut s = example("sir");
    run(&mut s, "file.load_json", json!({"json": small_sir(2, 1)}));
    let out = run(
        &mut s,
        "analysis.verify",
        json!({"properties": [{"kind": "deadlock-free"}]}),
    );
    let traces = out["traces"].as_array().unwrap().clone();
    assert!(!traces.is_empty(), "{out}");
    for t in traces {
        let v = run(&mut s, "provenance.check_witness", json!({"artifact": t}));
        assert_eq!(v["verdict"], "valid", "{v}");
    }
}

#[test]
fn check_witness_invariant_certificate() {
    let mut s = example("sir");
    let out = run(
        &mut s,
        "analysis.reach",
        json!({"to": {"S": 0, "I": 0, "R": 0}}),
    );
    assert_eq!(out["answer"], "no");
    assert_eq!(out["method"], "invariant", "{out}");
    let rep = out["artifact"].clone();
    let v = run(&mut s, "provenance.check_witness", json!({"artifact": rep}));
    assert_eq!(v["verdict"], "valid", "{v}");

    // Not an invariant.
    let mut bad = out.clone();
    let coeffs = bad["witness"]["invariant"]["coefficients"]
        .as_object_mut()
        .unwrap();
    let k = coeffs.keys().next().unwrap().clone();
    coeffs.insert(k, json!(2));
    let v = run(&mut s, "provenance.check_witness", json!({"reach": bad}));
    assert_eq!(v["verdict"], "invalid");
    assert!(
        v["reason"].as_str().unwrap().contains("not a P-invariant"),
        "{v}"
    );

    // Wrong stated value.
    let mut bad = out.clone();
    bad["witness"]["invariant"]["atFrom"] = json!(7);
    let v = run(&mut s, "provenance.check_witness", json!({"reach": bad}));
    assert_eq!(v["verdict"], "invalid");
    assert!(v["reason"].as_str().unwrap().contains("atFrom"), "{v}");

    // A non-separating claim: to equal to the invariant's value at from.
    let mut bad = out.clone();
    bad["to"] = json!({"S": 999, "I": 1, "R": 0});
    bad["witness"]["invariant"]["atTarget"] = bad["witness"]["invariant"]["atFrom"].clone();
    let v = run(&mut s, "provenance.check_witness", json!({"reach": bad}));
    assert_eq!(v["verdict"], "invalid", "{v}");

    // An exhaustive no has no certificate.
    let mut ex = out.clone();
    ex["method"] = json!("exhaustive");
    ex["witness"] = json!({"kind": "exhaustive", "exhaustive": {"states": 3}});
    let v = run(&mut s, "provenance.check_witness", json!({"reach": ex}));
    assert_eq!(v["verdict"], "uncheckable");
}

/// S --infect(S+I → 2I)--> I --recover--> R with small counts.
fn small_sir(s0: i64, i0: i64) -> Value {
    json!({
        "@context": "https://pflow.xyz/schema", "@type": "PetriNet", "@version": "1.1",
        "token": ["https://pflow.xyz/tokens/black"],
        "places": {
            "S": {"@type": "Place", "offset": 0, "initial": [s0], "capacity": [0], "x": 100, "y": 100},
            "I": {"@type": "Place", "offset": 1, "initial": [i0], "capacity": [0], "x": 200, "y": 100},
            "R": {"@type": "Place", "offset": 2, "initial": [0], "capacity": [0], "x": 300, "y": 100},
        },
        "transitions": {
            "infect": {"@type": "Transition", "x": 150, "y": 50},
            "recover": {"@type": "Transition", "x": 250, "y": 50},
        },
        "arcs": [
            {"@type": "Arrow", "source": "S", "target": "infect", "weight": [1]},
            {"@type": "Arrow", "source": "I", "target": "infect", "weight": [1]},
            {"@type": "Arrow", "source": "infect", "target": "I", "weight": [2]},
            {"@type": "Arrow", "source": "I", "target": "recover", "weight": [1]},
            {"@type": "Arrow", "source": "recover", "target": "R", "weight": [1]},
        ],
    })
}

/// A --t--> B.
fn chain() -> Value {
    json!({
        "@context": "https://pflow.xyz/schema", "@type": "PetriNet", "@version": "1.1",
        "token": ["https://pflow.xyz/tokens/black"],
        "places": {
            "A": {"@type": "Place", "offset": 0, "initial": [1], "capacity": [0], "x": 100, "y": 100},
            "B": {"@type": "Place", "offset": 1, "initial": [0], "capacity": [0], "x": 200, "y": 100},
        },
        "transitions": {"t": {"@type": "Transition", "x": 150, "y": 50}},
        "arcs": [
            {"@type": "Arrow", "source": "A", "target": "t", "weight": [1]},
            {"@type": "Arrow", "source": "t", "target": "B", "weight": [1]},
        ],
    })
}

#[test]
fn check_witness_morphisms() {
    let mut s = Session::default();
    run(&mut s, "file.load_json", json!({"json": small_sir(5, 1)}));
    let id = json!({"places": {"S": "S", "I": "I", "R": "R"}, "transitions": {"infect": "infect", "recover": "recover"}});
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"morphism": {"predicate": "isomorphicTo", "subject": "current", "object": small_sir(5, 1), "map": id}}),
    );
    assert_eq!(v["verdict"], "valid", "{v}");
    // Same shape, different initial marking: not coloured-net-v1.
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"morphism": {"predicate": "isomorphicTo", "subject": "current", "object": small_sir(4, 1), "map": id}}),
    );
    assert_eq!(v["verdict"], "invalid");
    assert!(v["reason"].as_str().unwrap().contains("initial"), "{v}");
    // Swapping the transitions breaks the arcs.
    let swapped = json!({"places": {"S": "S", "I": "I", "R": "R"}, "transitions": {"infect": "recover", "recover": "infect"}});
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"morphism": {"predicate": "isomorphicTo", "subject": "current", "object": "current", "map": swapped}}),
    );
    assert_eq!(v["verdict"], "invalid");

    // A --t--> B embeds as I --recover--> R, but not as S --infect--> I
    // (infect's arcs to I are a consume/1 and a produce/2, not A→t→B).
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"morphism": {"predicate": "subnetOf", "subject": chain(), "object": "current",
               "map": {"places": {"A": "I", "B": "R"}, "transitions": {"t": "recover"}}}}),
    );
    assert_eq!(v["verdict"], "valid", "{v}");
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"morphism": {"predicate": "subnetOf", "subject": chain(), "object": "current",
               "map": {"places": {"A": "S", "B": "I"}, "transitions": {"t": "infect"}}}}),
    );
    assert_eq!(v["verdict"], "invalid", "{v}");
    // Not injective.
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"morphism": {"definition": "induced-incidence-v1", "subject": chain(), "object": "current",
               "map": {"places": {"A": "I", "B": "I"}, "transitions": {"t": "recover"}}}}),
    );
    assert_eq!(v["verdict"], "invalid");
    assert!(v["reason"].as_str().unwrap().contains("injective"), "{v}");
    // Proof search predicates are remote.
    let v = run(
        &mut s,
        "provenance.check_witness",
        json!({"morphism": {"predicate": "refines", "subject": "current", "object": "current", "map": id}}),
    );
    assert_eq!(v["verdict"], "uncheckable");
}

// ======================================================================= zk

#[cfg(feature = "zk")]
#[test]
fn zk_prove_fire_and_verify() {
    let mut s = example("sir");
    let setup = run(&mut s, "zk.setup", json!({}));
    assert_eq!(setup["proofSystem"], "groth16-bn254");
    assert!(setup["constraints"].as_u64().unwrap() > 0);

    let out = run(&mut s, "zk.prove_fire", json!({"transition": "infect"}));
    assert_eq!(out["publicInputs"]["transitionIndex"], 0);
    assert_eq!(out["timings"]["keys"], "memory");
    let v = run(&mut s, "zk.verify", json!({"proof": out["report"]}));
    assert_eq!(v["verified"], true, "{v}");
    assert_eq!(v["trustedKey"], true);
    assert_eq!(v["matchesOpenNet"], true);

    // Tampered public inputs.
    let mut bad = out.clone();
    bad["publicInputs"]["transitionIndex"] = json!(1);
    let v = run(&mut s, "zk.verify", json!({"proof": bad}));
    assert_eq!(v["verified"], false, "{v}");
    let mut bad = out.clone();
    bad["publicInputs"]["preStateRoot"] = out["publicInputs"]["postStateRoot"].clone();
    let v = run(&mut s, "zk.verify", json!({"proof": bad}));
    assert_eq!(v["verified"], false, "{v}");

    // Not enabled: refused before proving.
    let mut empty = Session::default();
    run(
        &mut empty,
        "file.load_json",
        json!({"json": small_sir(0, 0)}),
    );
    assert!(
        err(&mut empty, "zk.prove_fire", json!({"transition": "infect"})).contains("cannot fire")
    );
}

#[cfg(feature = "zk")]
#[test]
fn zk_prove_trace_and_colored_net() {
    let mut s = example("sir");
    let out = run(&mut s, "analysis.reach", json!({"to": {"I": 0, "R": 1}}));
    let proof = run(&mut s, "zk.prove_trace", json!({"artifact": out["trace"]}));
    assert_eq!(proof["steps"].as_array().unwrap().len(), 1);
    let v = run(&mut s, "zk.verify", json!({"proof": proof.clone()}));
    assert_eq!(v["verified"], true, "{v}");

    // A colored net: places unfold per color.
    let mut c = example("coffee-shop");
    let enabled = run(&mut c, "sim.enabled", json!({}));
    let t = enabled[0]
        .as_str()
        .expect("an enabled transition")
        .to_string();
    let p = run(&mut c, "zk.prove_fire", json!({"transition": t}));
    let v = run(&mut c, "zk.verify", json!({"proof": p}));
    assert_eq!(v["verified"], true, "{v}");
    // A proof for another net's circuit verifies only against its own
    // embedded key, which is not trusted for this net.
    let mut step = proof["steps"][0].clone();
    step["circuit"] = proof["circuit"].clone();
    let v = run(&mut c, "zk.verify", json!({"proof": step}));
    assert_eq!(v["verified"], true, "{v}");
    assert_eq!(v["matchesOpenNet"], false, "{v}");
}

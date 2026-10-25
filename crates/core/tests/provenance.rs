//! Phase 7 provenance: signed run receipts, local witness checking and
//! (with the `zk` feature) Groth16 proofs of firings.
//!
//! Nothing here touches the real user data directory: every test points
//! PFLOW_NATIVE_HOME at a per-process temporary directory first.

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
    let r = pv::issue(&a, "zX", json!({}), "sha256:00".into(), "ssa", json!({}), 0);
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

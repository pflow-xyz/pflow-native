//! Model identity (`pflow_native_core::model_id`) against the vectors it
//! shares with pflow.xyz's `internal/model` (Go) and `public/model-cid.mjs`
//! (JS): `tests/fixtures/model/` is a copy of pflow-xyz `parity/model/`.
//!
//! - `golden.json`: every fixture's kind, `sealed` CID (the unchanged
//!   sealing contract, so this is also the Rust sealer's parity on
//!   compositions and plans), model CID and whether a bare wrapper reduced
//!   to its part; and the refusals, with the same codes in every language.
//! - `canonical.json`: canonical-JSON text vectors.
//! - the backport: every net in `tests/fixtures/cid/` (the pflow.xyz parity
//!   corpus, the gallery and the edge cases) keeps the CID it has always had.

use std::path::{Path, PathBuf};

use pflow_native_core::cid;
use pflow_native_core::model_id::{
    bare_wrapper, canonical_json, check, model_cid, model_id, new_plan,
};
use serde_json::{json, Value};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn load(path: &Path) -> Value {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_slice(&data).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn model(rel: &str) -> Value {
    load(&root().join("model").join(rel))
}

fn s(v: &Value) -> &str {
    v.as_str().unwrap()
}

#[test]
fn golden_vectors_match_go_and_js() {
    let golden = model("golden.json");
    let vectors = golden["vectors"].as_array().unwrap();
    assert!(vectors.len() >= 13, "{} vectors", vectors.len());
    let mut failures = vec![];
    for v in vectors {
        let name = s(&v["name"]);
        let doc = model(s(&v["file"]));
        let id = model_id(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        let want_parts: Vec<String> = v["parts"]
            .as_array()
            .map(|a| a.iter().map(|c| s(c).to_string()).collect())
            .unwrap_or_default();
        if id.sealed != s(&v["sealed"])
            || id.cid != s(&v["model"])
            || id.reduced != v["reduced"].as_bool().unwrap()
            || (!want_parts.is_empty() && id.parts != want_parts)
        {
            failures.push(format!(
                "{name}: got sealed={} model={} reduced={}\n   want sealed={} model={} reduced={}",
                id.sealed, id.cid, id.reduced, v["sealed"], v["model"], v["reduced"]
            ));
        }
        // The kind the vector states is the document's own @type (a reduced
        // wrapper reads as the net it is).
        let ty = doc["@type"].as_str().unwrap_or("");
        let kind_ok = match s(&v["kind"]) {
            "composition" => ty == "PetriNetBundle",
            "plan" => ty == "SearchPlan",
            "net" => ty != "SearchPlan" && (ty != "PetriNetBundle" || id.reduced),
            k => panic!("{name}: unknown kind {k}"),
        };
        if !kind_ok {
            failures.push(format!("{name}: kind {} vs @type {ty}", v["kind"]));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn refusals_have_the_same_codes() {
    let golden = model("golden.json");
    let refusals = golden["refusals"].as_array().unwrap();
    assert!(refusals.len() >= 13);
    for r in refusals {
        let err = check(&r["doc"]).expect_err(s(&r["name"]));
        let want = format!("[{}]", s(&r["code"]));
        assert!(
            err.to_string().contains(&want),
            "{}: got {err}, want {want}",
            r["name"]
        );
        // Identifying refuses the same document the same way.
        assert!(model_id(&r["doc"]).is_err(), "{}", r["name"]);
    }
}

#[test]
fn canonical_json_vectors() {
    let canon = model("canonical.json");
    for v in canon["vectors"].as_array().unwrap() {
        let got = canonical_json(s(&v["in"])).unwrap();
        assert_eq!(got, s(&v["out"]), "{}", v["name"]);
        assert_eq!(
            canonical_json(&got).unwrap(),
            got,
            "{}: idempotent",
            v["name"]
        );
    }
}

/// The backport: a net's model CID is the CID it has always had. Covers the
/// pflow.xyz parity corpus, the CID-named examples, the gallery and the 100+
/// edge cases (where a document the sealer refuses is refused here too).
#[test]
fn every_net_keeps_its_cid() {
    let dir = root().join("cid");
    let mut checked = 0;
    let mut check_one = |name: &str, doc: &Value, want: &str| {
        let id = model_id(doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(id.cid, want, "{name}");
        assert_eq!(id.sealed, want, "{name}");
        assert!(!id.reduced, "{name}");
        checked += 1;
    };
    let golden = load(&dir.join("golden.json"));
    for (name, want) in golden.as_object().unwrap() {
        if name != "_comment" {
            check_one(name, &load(&dir.join(name)), s(want));
        }
    }
    for e in std::fs::read_dir(&dir).unwrap() {
        let name = e.unwrap().file_name().into_string().unwrap();
        if let Some(want) = name.strip_suffix(".jsonld").filter(|n| n.starts_with('z')) {
            check_one(&name, &load(&dir.join(&name)), want);
        }
    }
    let gallery = load(&dir.join("gallery.json"));
    for (name, g) in gallery.as_object().unwrap() {
        if name != "_comment" {
            check_one(name, &load(&dir.join(name)), s(&g["cid"]));
        }
    }
    let cases = load(&dir.join("edge-cases.json"));
    let mut refused = 0;
    for case in cases.as_array().unwrap() {
        let name = s(&case["name"]);
        match (cid::seal(&case["doc"]), model_id(&case["doc"])) {
            (Ok(sealed), Ok(id)) => {
                assert_eq!(id.cid, sealed, "{name}");
                assert!(!id.reduced, "{name}");
                checked += 1;
            }
            (Err(_), Err(_)) => refused += 1,
            (a, b) => panic!("{name}: seal {a:?} but model_id {b:?}"),
        }
    }
    assert!(checked >= 100, "only {checked} documents checked");
    eprintln!("backport: {checked} nets keep their CID, {refused} refused by the sealer and by the rule alike");
}

/// Arrays under a term that is not an `@list` container hash as sets, which
/// is why recipes and plans are embedded as text.
#[test]
fn ordered_payloads_are_embedded_as_text() {
    let nested = |steps: &[&str]| json!({"@context": cid::PFLOW_CONTEXT_URL, "@type": "X", "params": {"steps": steps}});
    assert_eq!(
        cid::seal(&nested(&["reserve", "charge"])).unwrap(),
        cid::seal(&nested(&["charge", "reserve"])).unwrap(),
        "a nested array hashes as an unordered set"
    );
    let text = |steps: &[&str]| {
        let t = canonical_json(&json!({ "steps": steps }).to_string()).unwrap();
        json!({"@context": cid::PFLOW_CONTEXT_URL, "@type": "X", "params": t})
    };
    assert_ne!(
        cid::seal(&text(&["reserve", "charge"])).unwrap(),
        cid::seal(&text(&["charge", "reserve"])).unwrap(),
        "canonical text keeps the order in the CID"
    );
}

#[test]
fn a_plan_names_its_model_and_keeps_its_order() {
    let m = "z4EBG9jDsuUdvVN4bUXRHxoDG3p274FfhcJd8BiruRcE1jLCHi8";
    let a = new_plan(m, r#"{"order":["a","b"],"mode":"exact"}"#).unwrap();
    let same = new_plan(m, r#"{ "mode": "exact", "order": [ "a", "b" ] }"#).unwrap();
    let b = new_plan(m, r#"{"order":["b","a"],"mode":"exact"}"#).unwrap();
    assert_eq!(model_cid(&a).unwrap(), model_cid(&same).unwrap());
    assert_ne!(model_cid(&a).unwrap(), model_cid(&b).unwrap());
    assert!(new_plan("nope", "{}").is_err());
    // The plan golden in the shared vectors is this very construction.
    let golden = model("golden.json");
    let v = golden["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "plan-coffee-shop.jsonld")
        .unwrap();
    let doc = model("fixtures/plan-coffee-shop.jsonld");
    let plan_text = s(&doc["plan"]).to_string();
    assert_eq!(
        model_cid(&new_plan(m, &plan_text).unwrap()).unwrap(),
        s(&v["model"])
    );
}

#[test]
fn a_bare_wrapper_is_the_net_it_names() {
    let wrap = model("fixtures/wrap-coffee-shop.jsonld");
    let part = bare_wrapper(&wrap).unwrap();
    assert_eq!(model_cid(&wrap).unwrap(), part);
    assert_ne!(cid::seal(&wrap).unwrap(), part);
    let mut named = wrap.clone();
    named["name"] = json!("x");
    assert_eq!(bare_wrapper(&named), None);
    assert_ne!(model_cid(&named).unwrap(), part);
}

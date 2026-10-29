//! CID parity with pflow.xyz (internal/seal/seal.go and public/seal-cid.mjs).
//!
//! Fixtures in `tests/fixtures/cid/`:
//! - `golden.json` + its five fixtures: copied verbatim from pflow-xyz
//!   `parity/` (the cross-language contract).
//! - `z4EBG9….jsonld`: pflow-xyz `examples/`, named by their own CID.
//! - `gallery-*.json` + `gallery.json`: the pflow.xyz gallery as served by
//!   `/o/{cid}`, with the Go reference's CIDs (see the `_comment` there).
//! - `edge-cases.json`: synthetic documents probing json-gold's corners
//!   (odd keys, escapes, numbers, nulls, symmetric structures), with CIDs
//!   from the Go reference; `"cid": null` marks documents refused here.
//!   jsonld.js disagrees with Go on some of them (see crate::cid docs).

use std::path::{Path, PathBuf};

use pflow_native_core::{cid, ArtifactData, Document, Session};
use serde_json::{json, Value};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cid")
}

fn load(name: &str) -> Value {
    let data = std::fs::read(dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    serde_json::from_slice(&data).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn entries(file: &str) -> Vec<(String, Value)> {
    let v = load(file);
    v.as_object()
        .unwrap()
        .iter()
        .filter(|(k, _)| *k != "_comment")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Every fixture document with the CID the reference implementation gives it.
fn vectors() -> Vec<(String, Value, String)> {
    let mut out = vec![];
    for (name, want) in entries("golden.json") {
        out.push((
            name.clone(),
            load(&name),
            want.as_str().unwrap().to_string(),
        ));
    }
    for e in std::fs::read_dir(dir()).unwrap() {
        let name = e.unwrap().file_name().into_string().unwrap();
        if let Some(cid) = name.strip_suffix(".jsonld").filter(|n| n.starts_with('z')) {
            out.push((name.clone(), load(&name), cid.to_string()));
        }
    }
    for (name, g) in entries("gallery.json") {
        out.push((name.clone(), load(&name), g["cid"].as_str().unwrap().into()));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    assert!(out.len() >= 14, "fixtures missing: {}", out.len());
    out
}

#[test]
fn golden_vectors_are_byte_identical() {
    let mut failures = vec![];
    for (name, doc, want) in vectors() {
        let got = cid::seal(&doc).unwrap_or_else(|e| panic!("{name}: {e}"));
        if got != want {
            failures.push(format!("{name}: want {want}, got {got}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn examples_are_named_by_their_cid() {
    let mut n = 0;
    for e in std::fs::read_dir(dir()).unwrap() {
        let name = e.unwrap().file_name().into_string().unwrap();
        if let Some(want) = name.strip_suffix(".jsonld").filter(|n| n.starts_with('z')) {
            assert_eq!(cid::seal(&load(&name)).unwrap(), want, "{name}");
            n += 1;
        }
    }
    assert_eq!(n, 3);
}

/// pflow.xyz adds `author` after sealing, so a served net's `@id` is the CID
/// of the file without it (except the two sealed under the old contract).
#[test]
fn gallery_ids_are_the_cid_without_author() {
    let mut matched = 0;
    for (name, g) in entries("gallery.json") {
        let mut doc = load(&name);
        doc.as_object_mut().unwrap().remove("author");
        let got = cid::seal(&doc).unwrap();
        assert_eq!(got, g["cid_without_author"].as_str().unwrap(), "{name}");
        if got == g["id"].as_str().unwrap() {
            matched += 1;
        }
    }
    assert_eq!(
        matched, 4,
        "four gallery nets carry their current-contract CID"
    );
}

#[test]
fn edge_cases_match_go() {
    let mut failures = vec![];
    let cases = load("edge-cases.json");
    let cases = cases.as_array().unwrap();
    assert!(cases.len() >= 100, "{} edge cases", cases.len());
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let got = cid::seal(&case["doc"]).map_err(|e| e.to_string());
        match case.get("cid").and_then(Value::as_str) {
            Some(want) if got.as_deref() != Ok(want) => {
                failures.push(format!("{name}: want {want}, got {got:?}"))
            }
            None if got.is_ok() => failures.push(format!("{name}: expected an error")),
            _ => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn sealing_is_idempotent() {
    for (name, doc, want) in vectors() {
        let mut sealed = doc.clone();
        sealed["@id"] = json!(want);
        assert_eq!(cid::seal(&sealed).unwrap(), want, "{name}");
        sealed["@id"] = json!("zSomethingElse");
        assert_eq!(cid::seal(&sealed).unwrap(), want, "{name}");
    }
}

/// Our own load/save keeps every triple: from_value -> to_value has the
/// same CID as the original file.
#[test]
fn document_round_trip_keeps_the_cid() {
    for (name, doc, want) in vectors() {
        let ours = Document::from_value(&doc).unwrap().to_value();
        let got = cid::seal(&ours).unwrap();
        assert_eq!(
            got,
            want,
            "{name}: round trip changed the RDF\n{}",
            diff(&cid::nquads(&doc).unwrap(), &cid::nquads(&ours).unwrap())
        );
    }
}

/// The same through the session: load, `file.cid`, `file.seal`, save.
#[test]
fn file_cid_and_seal_commands() {
    let tmp = std::env::temp_dir().join(format!("pflow-native-cid-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    for (name, doc, want) in vectors() {
        let mut s = Session::default();
        s.execute("file.load_json", json!({ "json": doc })).unwrap();
        let out = s.execute("file.cid", json!({})).unwrap();
        assert_eq!(out["cid"], json!(want), "{name}: file.cid after load");

        let sealed = s.execute("file.seal", json!({})).unwrap();
        assert_eq!(sealed["cid"], json!(want), "{name}");
        assert_eq!(s.doc.to_value()["@id"], json!(want), "{name}: @id stored");
        let again = s.execute("file.seal", json!({})).unwrap();
        assert_eq!(again["cid"], json!(want), "{name}: sealing twice");
        assert_eq!(again["changed"], json!(false), "{name}");
        assert_eq!(
            s.execute("file.cid", json!({})).unwrap()["sealed"],
            json!(true)
        );

        // Saved, reopened: still the same CID, @id included.
        let path = tmp.join(&name);
        s.execute("file.save", json!({ "path": path })).unwrap();
        let mut t = Session::default();
        t.execute("file.open", json!({ "path": path })).unwrap();
        assert_eq!(t.doc.to_value()["@id"], json!(want), "{name}: @id saved");
        assert_eq!(
            t.execute("file.cid", json!({})).unwrap()["cid"],
            json!(want),
            "{name}: after save + open"
        );
    }
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn seal_is_undoable_and_tracks_edits() {
    let mut s = Session::default();
    s.execute("file.open_example", json!({ "name": "coffee-shop" }))
        .unwrap();
    let first = s.execute("file.seal", json!({})).unwrap()["cid"].clone();
    assert_eq!(s.doc.extra["@id"], first);
    s.execute("edit.undo", json!({})).unwrap();
    assert!(!s.doc.extra.contains_key("@id"));
    s.execute("edit.redo", json!({})).unwrap();

    s.execute("net.move", json!({ "id": "BrewCoffee", "x": 999, "y": 1 }))
        .unwrap();
    let now = s.execute("file.cid", json!({})).unwrap();
    assert_ne!(now["cid"], first, "an edit changes the content");
    assert_eq!(now["sealed"], json!(false), "the stored @id is now stale");
    let resealed = s.execute("file.seal", json!({})).unwrap();
    assert_eq!(resealed["cid"], now["cid"]);
    assert_eq!(resealed["changed"], json!(true));
}

#[test]
fn nquads_option_returns_the_preimage() {
    let mut s = Session::default();
    s.execute("file.open_example", json!({ "name": "coffee-shop" }))
        .unwrap();
    let out = s.execute("file.cid", json!({ "nquads": true })).unwrap();
    let nq = out["nquads"].as_str().unwrap();
    assert!(nq.starts_with("_:c14n0 "), "{nq}");
    assert_eq!(cid::cid_of_nquads(nq), out["cid"].as_str().unwrap());
}

fn diff(a: &str, b: &str) -> String {
    let strip = |s: &str| {
        let mut v: Vec<String> = s
            .lines()
            .map(|l| {
                l.split(' ')
                    .map(|w| if w.starts_with("_:") { "_:" } else { w })
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        v.sort();
        v
    };
    let (a, b) = (strip(a), strip(b));
    let only = |x: &[String], y: &[String], sign: char| {
        x.iter()
            .filter(|l| !y.contains(l))
            .map(|l| format!("{sign} {l}"))
            .collect::<Vec<_>>()
    };
    let mut out = only(&a, &b, '-');
    out.extend(only(&b, &a, '+'));
    out.join("\n")
}

// ------------------------------------------------------------ session rates
//
// A transition's numeric `rate` is lifted out of the document into
// `Session::rates` on load, and `file.save` puts it back. pflow.xyz's sealers
// hash the whole exported document, `rate` included, so every model CID has
// to be taken over what a save would write (`Session::export_doc`).

/// The web-generated `rated pair` fixtures in `edge-cases.json`.
fn rated_pair(name: &str) -> (Value, String) {
    let cases = load("edge-cases.json");
    let case = cases
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == format!("rated pair: {name}"))
        .unwrap_or_else(|| panic!("no edge case {name:?}"));
    (case["doc"].clone(), case["cid"].as_str().unwrap().into())
}

fn cid_of(s: &mut Session) -> String {
    s.execute("file.cid", json!({})).unwrap()["cid"]
        .as_str()
        .unwrap()
        .to_string()
}

fn loaded(doc: &Value) -> Session {
    let mut s = Session::default();
    s.execute("file.load_json", json!({ "json": doc })).unwrap();
    s
}

/// A -> go -> B, the smallest net with a rate on it.
fn a_go_b(rate: Option<f64>) -> Value {
    let mut t = json!({ "@type": "Transition", "x": 100, "y": 0, "label": "go" });
    if let Some(r) = rate {
        t["rate"] = json!(r);
    }
    json!({
        "@context": "https://pflow.xyz/schema",
        "@type": "PetriNet",
        "@version": "1.1",
        "token": ["https://pflow.xyz/tokens/black"],
        "places": {
            "A": { "@type": "Place", "offset": 0, "initial": [1], "capacity": [0], "x": 0, "y": 0, "label": "A" },
            "B": { "@type": "Place", "offset": 0, "initial": [0], "capacity": [0], "x": 200, "y": 0, "label": "B" },
        },
        "transitions": { "go": t },
        "arcs": [
            { "@type": "Arrow", "source": "A", "target": "go", "weight": [1], "inhibitTransition": false },
            { "@type": "Arrow", "source": "go", "target": "B", "weight": [1], "inhibitTransition": false },
        ],
    })
}

#[test]
fn file_cid_hashes_rates_like_the_web() {
    // pflow-xyz's own parity fixture (rates only), through the session.
    let mut s = loaded(&load("with-rates.jsonld"));
    assert!(
        !s.rates.is_empty(),
        "the rates were lifted into the session"
    );
    assert_eq!(
        cid_of(&mut s),
        "z4EBG9jA7f4q9yx4QjzH3RDPQXiqjnNTz3ALdpjHD3CRTbNrcCD"
    );

    // Web-generated variants of one net, each sealed by both web sealers.
    for name in [
        "rates + goal + view",
        "rates only (goals and views removed)",
        "no rates, goals or views",
    ] {
        let (doc, want) = rated_pair(name);
        let mut s = loaded(&doc);
        assert_eq!(cid_of(&mut s), want, "{name}");
    }
}

#[test]
fn top_level_goals_and_views_are_kept_and_hashed() {
    let (doc, want) = rated_pair("rates + goal + view");
    let mut s = loaded(&doc);
    assert_eq!(s.doc.extra["goals"], doc["goals"]);
    assert_eq!(s.doc.extra["views"], doc["views"]);
    assert_eq!(cid_of(&mut s), want);
    // Dropping them changes the CID to the web's rates-only value.
    let (_, rates_only) = rated_pair("rates only (goals and views removed)");
    s.doc.extra.shift_remove("goals");
    s.doc.extra.shift_remove("views");
    assert_eq!(cid_of(&mut s), rates_only);
}

#[test]
fn a_rate_set_in_the_session_changes_the_cid() {
    let mut s = loaded(&a_go_b(None));
    let plain = cid_of(&mut s);
    s.execute("sim.set_rate", json!({ "id": "go", "rate": 2.5 }))
        .unwrap();
    let rated = cid_of(&mut s);
    assert_ne!(rated, plain, "a session rate is part of the model");
    // It is the CID of the same net written with the rate in the file.
    assert_eq!(rated, cid::seal(&a_go_b(Some(2.5))).unwrap());
    assert_eq!(
        cid_of(&mut loaded(&a_go_b(Some(2.5)))),
        rated,
        "set_rate and a rate in the loaded file agree"
    );

    s.execute("sim.set_rate", json!({ "id": "go", "rate": 3.0 }))
        .unwrap();
    assert_ne!(cid_of(&mut s), rated);

    // The rated pair: moving a rate moves the CID, putting it back restores it.
    let (doc, want) = rated_pair("rates + goal + view");
    let mut s = loaded(&doc);
    s.execute("sim.set_rate", json!({ "id": "start", "rate": 4.0 }))
        .unwrap();
    assert_ne!(cid_of(&mut s), want);
    s.execute("sim.set_rate", json!({ "id": "start", "rate": 1.5 }))
        .unwrap();
    assert_eq!(cid_of(&mut s), want);
}

#[test]
fn a_sealed_rated_net_is_saved_with_its_own_id() {
    let tmp = std::env::temp_dir().join(format!("pflow-native-cid-rates-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let mut cases = vec![
        ("a-go-b.jsonld".to_string(), a_go_b(Some(2.5))),
        ("with-rates.jsonld".to_string(), load("with-rates.jsonld")),
    ];
    cases.push((
        "rated-pair.jsonld".to_string(),
        rated_pair("rates + goal + view").0,
    ));
    for (name, doc) in cases {
        let mut s = loaded(&doc);
        let sealed = s.execute("file.seal", json!({})).unwrap()["cid"].clone();
        assert_eq!(sealed, json!(cid_of(&mut s)), "{name}");
        assert_eq!(
            s.execute("file.cid", json!({})).unwrap()["sealed"],
            json!(true),
            "{name}"
        );
        let path = tmp.join(&name);
        s.execute("file.save", json!({ "path": path })).unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["@id"], sealed, "{name}: @id saved");
        assert!(
            saved["transitions"]
                .as_object()
                .unwrap()
                .values()
                .any(|t| t.get("rate").is_some()),
            "{name}: the rates are in the saved file"
        );
        assert_eq!(
            json!(cid::seal(&saved).unwrap()),
            saved["@id"],
            "{name}: the saved bytes seal to the saved @id"
        );
    }
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn store_and_provenance_cids_agree_with_file_cid() {
    use pflow_native_core::store::{self, Store};
    store::set_thread_store(Some(std::sync::Arc::new(Store::memory())));
    for doc in [a_go_b(Some(2.5)), rated_pair("rates + goal + view").0] {
        let mut s = loaded(&doc);
        let want = cid_of(&mut s);
        assert_eq!(
            want,
            cid::seal(&doc).unwrap(),
            "file.cid seals the whole file"
        );
        let put = s.execute("store.put", json!({})).unwrap();
        assert_eq!(put["cid"], json!(want), "store.put");
        // The stored file seals to its name, rates included.
        let stored = store::open().get_model(&want).unwrap();
        assert_eq!(cid::seal(&stored).unwrap(), want);

        let out = s
            .execute(
                "sim.receipt",
                json!({ "scenario": { "t_end": 1, "realizations": 1 } }),
            )
            .unwrap();
        let report = s.artifact(out["report"].as_str().unwrap()).unwrap();
        let ArtifactData::Report(r) = &report.data else {
            panic!("a report")
        };
        assert_eq!(r["receipt"]["model"], json!(want), "sim.receipt model");

        // The same net, loaded with the rate set in the session instead.
        let mut t = loaded(&a_go_b(None));
        t.execute("sim.set_rate", json!({ "id": "go", "rate": 2.5 }))
            .unwrap();
        if doc == a_go_b(Some(2.5)) {
            assert_eq!(cid_of(&mut t), want);
        }
    }
}

#[test]
fn a_rated_net_keeps_its_rates_and_cid_through_the_library() {
    let mut s = loaded(&load("with-rates.jsonld"));
    let rates = s.rates.clone();
    assert!(!rates.is_empty());
    let want = cid_of(&mut s);
    s.execute("model.save", json!({ "name": "r" })).unwrap();
    s.execute("file.new", json!({})).unwrap();
    assert!(s.rates.is_empty());
    s.execute("model.open", json!({ "name": "r" })).unwrap();
    assert_eq!(s.rates, rates, "model.open restores the rates");
    assert_eq!(cid_of(&mut s), want, "file.cid after the round trip");
}

#[test]
fn store_put_of_a_saved_rated_net_is_its_file_cid() {
    use pflow_native_core::store::{self, Store};
    store::set_thread_store(Some(std::sync::Arc::new(Store::memory())));
    let mut s = loaded(&load("with-rates.jsonld"));
    let want = cid_of(&mut s);
    s.execute("model.save", json!({ "name": "r" })).unwrap();
    let put = s.execute("store.put", json!({ "model": "r" })).unwrap();
    assert_eq!(put["cid"], json!(want), "store.put of the library entry");
}

#[cfg(feature = "services")]
#[test]
fn outbound_cids_agree_with_file_cid() {
    let (doc, want) = rated_pair("rates + goal + view");
    let mut s = loaded(&doc);
    let save = s.execute("remote.pflow.save", json!({})).unwrap();
    assert_eq!(save["expected_cid"], json!(want), "remote.pflow.save");
    let share = s.execute("remote.pflow.share", json!({})).unwrap();
    assert!(
        share["jsonld"].as_str().unwrap().ends_with(&want),
        "{share}"
    );
    let link = s.execute("file.permalink", json!({})).unwrap();
    // The permalink carries the document a save would write, rates included.
    assert!(link.as_str().unwrap().contains("rate"), "{link}");
}

/// The largest nets of `examples/` seal, as they did before sealing was bounded: the work a document
/// may take grows with it (`cid::seal_work_limit`), so a net of 536 arcs is not mistaken for a
/// hostile document. (connect-4, 1924 arcs, needs 37 million steps and a minute in a debug build:
/// `cid::tests::the_limit_grows_with_the_document_between_a_floor_and_a_ceiling` pins that it fits.)
#[test]
fn the_larger_example_nets_still_seal() {
    let path = format!(
        "{}/../../examples/connect-3.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let net: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert!(net["arcs"].as_array().unwrap().len() > 500);
    let cid = cid::seal(&net).unwrap_or_else(|e| panic!("connect-3 should seal: {e}"));
    assert!(cid.starts_with('z'));
}

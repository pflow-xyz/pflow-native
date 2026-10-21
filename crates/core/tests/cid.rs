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

use pflow_native_core::{cid, Document, Session};
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

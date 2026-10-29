//! Round trips of the two composition document forms (docs/composition-documents.md §1, §2, §4):
//! a `PatternComposition` (the recipe of pattern instances and typed links) and a raw
//! `PetriNetBundle` (kept beside its parsed form so an unedited export is the bytes received).
//! Every CID here is `cid::seal` of the JSON the command exported; nothing in `cid.rs` changes.
//!
//! The tests declare whatever content they need inline: the orders library content is not what is
//! under test, and a test that needs scenarios must not depend on it.

use std::sync::Arc;

use pflow_native_core::cid;
use pflow_native_core::composition::Composition;
use pflow_native_core::store::{self, Store};
use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

/// cafe.bundle.json sealed as received.
const CAFE_CID: &str = "z4EBG9j1SUhuY7XnEcPfHQPzQkmqjUtgrD2wHADw8qdRbpnNt6W";
/// orders.composition.json, the document `bundle.open` + `bundle.export` writes.
const ORDERS_CID: &str = "z4EBG9jDqRnD5dM6u119z8r3V2WHsgMXU7Dxa6oJezBBSfCKk8L";
/// The plain Bundle that composition flattens to (what a Bundle-reading tool would hash).
const ORDERS_BUNDLE_CID: &str = "z4EBG9jEZCn3ovsw733ZiAjYt6AGYKH2Tdrf29vmqoF28cio1VY";
const CONTEXT: &str = "https://pflow.xyz/schema";

// ------------------------------------------------------------ helpers

/// A session over a fresh in-memory store for this thread, so a `cid` the test names is looked up
/// in nothing but what the test put there.
fn session() -> Session {
    store::set_thread_store(Some(Arc::new(Store::memory())));
    Session::default()
}

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn refused(s: &mut Session, id: &str, args: Value) -> String {
    match s.execute(id, args) {
        Ok(v) => panic!("{id} should be refused, got {v}"),
        Err(e) => e.to_string(),
    }
}

/// `bundle.load {document}` in a fresh session; the refusal text.
fn load_err(doc: &Value) -> String {
    refused(&mut session(), "bundle.load", json!({"document": doc}))
}

/// A refusal is `[CODE] message`: the code is the prefix.
fn assert_code(msg: &str, code: &str) {
    assert!(
        msg.starts_with(&format!("[{code}] ")),
        "expected a [{code}] refusal, got: {msg}"
    );
}

fn fixture_text(name: &str) -> String {
    let p = format!(
        "{}/tests/fixtures/bundle/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{p}: {e}"))
}

fn fixture(name: &str) -> Value {
    serde_json::from_str(&fixture_text(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn cafe() -> Value {
    fixture("cafe.bundle.json")
}

fn state(s: &mut Session) -> Value {
    run(s, "bundle.state", Value::Null)
}

/// `bundle.flatten` without the artifact id (the one thing that differs between two sessions).
fn flat(s: &mut Session) -> Value {
    let mut f = run(s, "bundle.flatten", Value::Null);
    f.as_object_mut().unwrap().remove("artifact");
    f
}

fn missing_codes(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|m| m["code"].as_str().unwrap().to_string())
        .collect()
}

/// Undo steps available (walks them and puts them back).
fn undo_depth(s: &mut Session) -> usize {
    let mut n = 0;
    while s.can_undo() {
        run(s, "edit.undo", Value::Null);
        n += 1;
    }
    for _ in 0..n {
        run(s, "edit.redo", Value::Null);
    }
    n
}

// The document pieces the refusal tests build from.

fn orders_inst() -> Value {
    json!({"id": "orders", "pattern": "queue",
        "params": {"ID": "orders", "Capacity": 3, "Initial": 2, "Payload": ""}})
}

fn workers_inst() -> Value {
    json!({"id": "workers", "pattern": "resource-pool",
        "params": {"id": "workers", "name": "workers", "capacity": 2}})
}

fn saga_params() -> Value {
    json!({
        "id": "payment",
        "accounts": {
            "customer": {"initial": 1, "shared": false},
            "merchant": {"initial": 0, "shared": false},
        },
        "steps": [
            {"id": "reserve", "name": "reserve stock", "compensation": "release stock", "moves": []},
            {"id": "charge", "name": "charge card", "compensation": "refund",
             "moves": [{"from": "customer", "to": "merchant", "amount": 1}]},
        ],
    })
}

fn payment_inst() -> Value {
    json!({"id": "payment", "pattern": "saga", "params": saga_params()})
}

fn event(from: (&str, &str), to: (&str, &str)) -> Value {
    json!({"kind": "event",
        "from": {"subnet": from.0, "port": from.1},
        "to": {"subnet": to.0, "port": to.1}})
}

fn dequeue_acquire() -> Value {
    event(("orders", "dequeue"), ("workers", "acquire"))
}

fn doc(instances: Value, links: Value) -> Value {
    json!({"@context": CONTEXT, "@type": "PatternComposition", "name": "t",
        "instances": instances, "links": links})
}

/// orders + workers + the dequeue/acquire idiom, params as objects.
fn small_doc() -> Value {
    doc(
        json!([orders_inst(), workers_inst()]),
        json!([dequeue_acquire()]),
    )
}

/// `d` with the value at JSON pointer `at` replaced (or added, when the key is not there yet).
fn with(d: &Value, at: &str, v: Value) -> Value {
    let mut d = d.clone();
    let (parent, key) = at.rsplit_once('/').expect("a pointer");
    match d.pointer_mut(parent).expect("the parent exists") {
        Value::Array(a) => a[key.parse::<usize>().expect("an index")] = v,
        Value::Object(m) => {
            m.insert(key.to_string(), v);
        }
        other => panic!("cannot set {key} on {other}"),
    }
    d
}

/// The golden document with every `params` as an object (the prototype's shape).
fn with_object_params(d: &Value) -> Value {
    let mut d = d.clone();
    for i in d["instances"].as_array_mut().unwrap() {
        let p: Value = serde_json::from_str(i["params"].as_str().unwrap()).unwrap();
        i["params"] = p;
    }
    d
}

// ------------------------------------------------------------ 1. form (a): orders

#[test]
fn orders_exports_as_a_pattern_composition_and_reloads_identically() {
    let mut a = session();
    run(&mut a, "bundle.open", json!({}));
    let exp = run(&mut a, "bundle.export", json!({}));
    assert_eq!(
        exp["form"], "composition",
        "the built-in example is form (a)"
    );
    assert_eq!(exp["cid"], ORDERS_CID);
    assert_eq!(exp["bundle_cid"], ORDERS_BUNDLE_CID);
    assert_eq!(
        cid::seal(&exp["document"]).unwrap(),
        ORDERS_CID,
        "the CID is the seal of the exported JSON and nothing else"
    );

    let mut b = session();
    let loaded = run(&mut b, "bundle.load", json!({"document": exp["document"]}));
    assert_eq!(loaded["form"], "composition");
    assert_eq!(loaded["cid"], ORDERS_CID);
    assert_eq!(loaded["bundle_cid"], ORDERS_BUNDLE_CID);
    assert_eq!(loaded["instances"], json!(["orders", "workers", "payment"]));
    assert_eq!(loaded["validation"]["valid"], true);
    assert_eq!(loaded["flatten"], json!({"ok": true}));

    let (sa, sb) = (state(&mut a), state(&mut b));
    for k in [
        "instances",
        "subnets",
        "links",
        "unavailable",
        "dropped_links",
    ] {
        assert_eq!(
            sa[k], sb[k],
            "bundle.state {k} differs after the round trip"
        );
    }
    assert_eq!(loaded["links"], sb["links"]);
    assert_eq!(sa["links"].as_array().unwrap().len(), 3);
    // Same composition, different provenance: the built-in one is a catalog opening.
    assert_eq!(sa["document"]["form"], "composition");
    assert_eq!(sb["document"]["form"], "composition");
    assert_eq!(sa["document"]["origin"], "catalog");
    assert_eq!(sb["document"]["origin"], "document");

    assert_eq!(flat(&mut a), flat(&mut b), "the same flat net");
    assert_eq!(
        run(&mut a, "bundle.incidence", Value::Null),
        run(&mut b, "bundle.incidence", Value::Null)
    );

    let again = run(&mut b, "bundle.export", json!({}));
    assert_eq!(again["document"], exp["document"], "identical re-export");
    assert_eq!(again["cid"], ORDERS_CID);
    assert_eq!(again["bundle_cid"], ORDERS_BUNDLE_CID);
    assert_eq!(
        serde_json::to_string(&again["document"]).unwrap(),
        serde_json::to_string(&exp["document"]).unwrap(),
        "down to the key order"
    );
}

#[test]
fn the_orders_golden_fixture_is_what_the_code_exports_and_loads_to_its_pinned_cid() {
    let text = fixture_text("orders.composition.json");
    let golden: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(cid::seal(&golden).unwrap(), ORDERS_CID);

    // Sane to read: the three catalog instances, params as canonical text, the three idioms.
    assert_eq!(golden["@context"], CONTEXT);
    assert_eq!(golden["@type"], "PatternComposition");
    let inst = golden["instances"].as_array().unwrap();
    let names: Vec<(&str, &str)> = inst
        .iter()
        .map(|i| (i["id"].as_str().unwrap(), i["pattern"].as_str().unwrap()))
        .collect();
    assert_eq!(
        names,
        [
            ("orders", "queue"),
            ("workers", "resource-pool"),
            ("payment", "saga")
        ]
    );
    for i in inst {
        let p = i["params"].as_str().expect("params travel as text");
        assert!(
            !p.contains('\n') && !p.contains(": "),
            "no layout whitespace: {p}"
        );
        assert_eq!(
            pflow_native_core::composition::document::canonical_json(
                &serde_json::from_str(p).unwrap()
            ),
            p,
            "{} params are canonical",
            i["id"]
        );
    }
    assert_eq!(golden["links"].as_array().unwrap().len(), 3);
    assert!(golden["links"]
        .as_array()
        .unwrap()
        .iter()
        .all(|l| l["kind"] == "event"));

    // The code still writes exactly this.
    let mut a = session();
    run(&mut a, "bundle.open", json!({}));
    assert_eq!(
        run(&mut a, "bundle.export", json!({}))["document"],
        golden,
        "the catalog or the writer changed: regenerate tests/fixtures/bundle/orders.composition.json \
         (and the pinned CIDs) only if that was meant"
    );

    // And it loads to the pinned CID whether it arrives as a value or as the file's text.
    for document in [golden.clone(), json!(text)] {
        let mut s = session();
        let out = run(&mut s, "bundle.load", json!({"document": document}));
        assert_eq!(out["cid"], ORDERS_CID);
        assert_eq!(out["bundle_cid"], ORDERS_BUNDLE_CID);
        assert_eq!(run(&mut s, "bundle.export", json!({}))["document"], golden);
    }
}

#[test]
fn extras_survive_the_round_trip_and_keep_their_flag() {
    let mut a = session();
    run(&mut a, "bundle.open", json!({}));
    for id in ["payment2", "orders2", "workers2"] {
        run(&mut a, "bundle.add_instance", json!({"id": id}));
    }
    // A second queue, joined to the first by a token link (out of one into in of the other).
    run(
        &mut a,
        "bundle.add_link",
        json!({"kind": "token", "from": "orders:out", "to": "orders2:in"}),
    );
    let exp = run(&mut a, "bundle.export", json!({}));
    assert_eq!(exp["form"], "composition");
    assert_eq!(exp["document"]["instances"].as_array().unwrap().len(), 6);
    assert_eq!(exp["document"]["links"].as_array().unwrap().len(), 4);
    assert_ne!(exp["cid"], ORDERS_CID);

    let mut b = session();
    let loaded = run(&mut b, "bundle.load", json!({"document": exp["document"]}));
    assert_eq!(loaded["cid"], exp["cid"]);
    assert_eq!(loaded["bundle_cid"], exp["bundle_cid"]);
    let sb = state(&mut b);
    let flags: Vec<(String, bool)> = sb["subnets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| (s["id"].as_str().unwrap().into(), s["extra"] == true))
        .collect();
    assert_eq!(
        flags,
        [
            ("orders".into(), false),
            ("workers".into(), false),
            ("payment".into(), false),
            ("payment2".into(), true),
            ("orders2".into(), true),
            ("workers2".into(), true),
        ],
        "`extra` is read back from the catalog, so the palette still knows them"
    );
    let sa = state(&mut a);
    assert_eq!(sa["subnets"], sb["subnets"]);
    assert_eq!(sa["links"], sb["links"]);
    assert_eq!(flat(&mut a), flat(&mut b));
    let again = run(&mut b, "bundle.export", json!({}));
    assert_eq!(again["document"], exp["document"]);
    assert_eq!(again["cid"], exp["cid"]);
}

#[test]
fn an_instance_is_an_extra_only_when_it_is_exactly_the_catalog_extra() {
    // `intake` is not in the catalog; `orders2` here has a different capacity from the catalog's.
    let mut near = orders_inst();
    near["id"] = json!("orders2");
    near["params"]["ID"] = json!("orders2");
    near["params"]["Capacity"] = json!(5);
    near["params"]["Initial"] = json!(0);
    let mut intake = orders_inst();
    intake["id"] = json!("intake");
    intake["params"]["ID"] = json!("intake");
    let mut exact = orders_inst();
    exact["id"] = json!("orders2");
    exact["params"] = json!({"ID": "orders2", "Capacity": 3, "Initial": 0, "Payload": ""});

    let mut s = session();
    run(
        &mut s,
        "bundle.load",
        json!({"document": doc(json!([orders_inst(), intake, near]), json!([]))}),
    );
    let st = state(&mut s);
    let extras: Vec<bool> = st["subnets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["extra"] == true)
        .collect();
    assert_eq!(extras, [false, false, false]);

    let mut s = session();
    run(
        &mut s,
        "bundle.load",
        json!({"document": doc(json!([orders_inst(), exact]), json!([]))}),
    );
    let st = state(&mut s);
    assert_eq!(st["subnets"][1]["extra"], true);
    // Two queues side by side export and reload.
    let exp = run(&mut s, "bundle.export", json!({}));
    let mut t = session();
    let out = run(&mut t, "bundle.load", json!({"document": exp["document"]}));
    assert_eq!(out["cid"], exp["cid"]);
    assert_eq!(state(&mut t)["subnets"], st["subnets"]);
}

#[test]
fn namespace_arc_merge_description_label_why_and_link_fields_round_trip() {
    let orders = pflow_native_core::composition::document::canonical_json(&orders_inst()["params"]);
    let workers =
        pflow_native_core::composition::document::canonical_json(&workers_inst()["params"]);
    let document = json!({
        "@context": CONTEXT,
        "@type": "PatternComposition",
        "name": "café",
        "description": "a queue and a pool, joined by one idiom and watched by a guard",
        "namespace": true,
        "arc_merge": "max",
        "instances": [
            {"id": "orders", "pattern": "queue", "params": orders,
             "label": "Order queue", "why": "two waiting, so the pool has something to take"},
            {"id": "workers", "pattern": "resource-pool", "params": workers,
             "label": "Baristas", "why": "\"a fixed number of workers\": two"},
        ],
        "links": [
            {"kind": "event", "id": "served",
             "from": {"subnet": "orders", "port": "dequeue"},
             "to": {"subnet": "workers", "port": "acquire"}},
            {"kind": "guard", "condition": ">= 1", "lowering": "auto",
             "from": {"subnet": "orders", "port": "dequeue"},
             "to": {"subnet": "workers", "port": "available"}},
        ],
    });
    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": document}));
    assert_eq!(out["form"], "composition");
    assert_eq!(out["name"], "café");
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(
        exp["document"], document,
        "every field comes back as written"
    );
    assert_eq!(exp["cid"], out["cid"]);
    assert_eq!(cid::seal(&document).unwrap(), exp["cid"]);

    let st = state(&mut s);
    assert_eq!(st["document"]["name"], "café");
    assert_eq!(
        st["document"]["description"],
        "a queue and a pool, joined by one idiom and watched by a guard"
    );
    assert_eq!(st["subnets"][0]["label"], "Order queue");
    assert_eq!(
        st["subnets"][1]["why"],
        "\"a fixed number of workers\": two"
    );

    // namespace and arc_merge are the Bundle's too.
    let bundle = run(&mut s, "bundle.export", json!({"form": "bundle"}));
    assert_eq!(bundle["document"]["namespace"], true);
    assert_eq!(bundle["document"]["arc_merge"], "max");
    assert_eq!(bundle["document"]["links"][0]["id"], "served");
    assert_eq!(bundle["document"]["links"][1]["condition"], ">= 1");

    let mut t = session();
    run(&mut t, "bundle.load", json!({"document": exp["document"]}));
    assert_eq!(
        run(&mut t, "bundle.export", json!({}))["document"],
        document
    );
}

// ------------------------------------------------------------ 2. the array-order hazard

#[test]
fn reordering_instances_and_links_keeps_the_cid() {
    let golden = fixture("orders.composition.json");
    let mut shuffled = golden.clone();
    shuffled["instances"].as_array_mut().unwrap().reverse();
    shuffled["links"].as_array_mut().unwrap().rotate_left(1);
    assert_ne!(shuffled, golden.clone(), "the arrays really are reordered");
    // To the sealer both arrays are sets...
    assert_eq!(cid::seal(&shuffled).unwrap(), ORDERS_CID);

    // ...so the loader agrees, and the text keeps the order it was given.
    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": shuffled}));
    assert_eq!(out["cid"], ORDERS_CID);
    assert_eq!(out["bundle_cid"], ORDERS_BUNDLE_CID);
    assert_eq!(out["instances"], json!(["payment", "workers", "orders"]));
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["cid"], ORDERS_CID);
    assert_eq!(
        exp["document"], shuffled,
        "order is kept by the text, not the hash"
    );
    let first_ends: Vec<&str> = exp["document"]["links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["from"]["subnet"].as_str().unwrap())
        .collect();
    assert_eq!(
        first_ends,
        ["orders", "workers", "orders"],
        "links keep the rotated order too"
    );
    // The same composition either way: same links, same flat net up to the order of places.
    let mut g = session();
    run(&mut g, "bundle.load", json!({"document": golden}));
    let keys = |s: &mut Session| -> Vec<String> {
        let mut k: Vec<String> = state(s)["links"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["key"].as_str().unwrap().to_string())
            .collect();
        k.sort();
        k
    };
    assert_eq!(keys(&mut s), keys(&mut g));
}

#[test]
fn reordering_a_sagas_steps_changes_the_params_text_and_the_cid() {
    let golden = fixture("orders.composition.json");
    let forward = with_object_params(&golden);
    let mut backward = forward.clone();
    backward["instances"][2]["params"]["steps"]
        .as_array_mut()
        .unwrap()
        .reverse();

    // The hazard: in object form `steps` is an array the sealer treats as a set, so the two
    // documents seal alike although they are two different sagas.
    assert_eq!(
        cid::seal(&forward).unwrap(),
        cid::seal(&backward).unwrap(),
        "object-form arrays hash as sets"
    );
    // The loader closes it: params leave as canonical text, which the hash covers byte for byte.
    let (mut f, mut b) = (session(), session());
    let lf = run(&mut f, "bundle.load", json!({"document": forward}));
    let lb = run(&mut b, "bundle.load", json!({"document": backward}));
    assert_eq!(
        lf["cid"], ORDERS_CID,
        "object form canonicalises to the golden text"
    );
    assert_ne!(
        lf["cid"], lb["cid"],
        "reordered steps are a different document"
    );
    assert_ne!(lf["bundle_cid"], lb["bundle_cid"], "and a different net");

    let ef = run(&mut f, "bundle.export", json!({}));
    let eb = run(&mut b, "bundle.export", json!({}));
    let (pf, pb) = (
        ef["document"]["instances"][2]["params"].as_str().unwrap(),
        eb["document"]["instances"][2]["params"].as_str().unwrap(),
    );
    assert_ne!(pf, pb, "a different canonical params string");
    let at = |p: &str, step: &str| p.find(&format!("\"id\":\"{step}\"")).unwrap();
    assert!(at(pf, "reserve") < at(pf, "charge"));
    assert!(at(pb, "charge") < at(pb, "reserve"));
    assert_eq!(cid::seal(&eb["document"]).unwrap(), lb["cid"]);

    // Each is stable on its own: reload the reordered export and it is the same document.
    let mut again = session();
    let r = run(
        &mut again,
        "bundle.load",
        json!({"document": eb["document"]}),
    );
    assert_eq!(r["cid"], lb["cid"]);
    assert_eq!(
        run(&mut again, "bundle.export", json!({}))["document"],
        eb["document"]
    );
}

#[test]
fn params_as_an_object_or_a_canonical_string_load_alike_and_a_sealed_string_is_never_rewritten() {
    let canonical = fixture("orders.composition.json")["instances"][2]["params"]
        .as_str()
        .unwrap()
        .to_string();
    // Same saga: another key order, whitespace, a pretty-printed layout, a \u escape.
    let respaced = r#" {
        "steps" : [
            { "name": "reserve stock", "moves": [], "id": "reserve", "compensation": "release stock" },
            { "moves": [ { "to": "merchant", "from": "customer", "amount": 1 } ],
              "name": "charge card", "id": "charge", "compensation": "refund" }
        ],
        "id" : "payment",
        "accounts" : { "merchant": { "shared": false, "initial": 0 },
                       "customer": { "initial": 1, "shared": false } }
    } "#;
    let escaped = canonical.replacen("reserve stock", "reserve\\u0020stock", 1);
    assert_ne!(escaped, canonical);
    let strings = [
        ("respaced string", respaced.to_string()),
        (
            "pretty-printed string",
            serde_json::to_string_pretty(&saga_params()).unwrap(),
        ),
        ("string with a \\u escape", escaped),
    ];

    // An object and the canonical string are one document: the export is the canonical string.
    let mut first: Option<(Value, Value)> = None;
    for (name, params) in [
        ("object", saga_params()),
        ("canonical string", json!(canonical)),
    ] {
        let mut inst = payment_inst();
        inst["params"] = params;
        let d = doc(json!([inst]), json!([]));
        let mut s = session();
        let out = run(&mut s, "bundle.load", json!({"document": d}));
        let exp = run(&mut s, "bundle.export", json!({}));
        assert_eq!(
            exp["document"]["instances"][0]["params"],
            json!(canonical),
            "{name}: written as the canonical string"
        );
        assert_eq!(out["cid"], exp["cid"], "{name}");
        let sub = state(&mut s)["subnets"][0].clone();
        match &first {
            None => first = Some((json!(exp["cid"]), sub)),
            Some((cid0, sub0)) => {
                assert_eq!(&exp["cid"], cid0, "{name}: one CID");
                assert_eq!(&sub, sub0, "{name}: one subnet");
            }
        }
    }
    let (canonical_cid, canonical_sub) = first.unwrap();

    // A string is sealed as it came (identity rule: a consumer never re-canonicalises a sealed
    // string): the composition built is the same, the document and its CID are the received ones,
    // and store.put of the received bytes names the same CID the loader reports.
    for (name, text) in strings {
        let mut inst = payment_inst();
        inst["params"] = json!(text);
        let mut d = doc(json!([inst]), json!([]));
        d.as_object_mut().unwrap().remove("links"); // the writer omits an empty list
        let mut s = session();
        let out = run(&mut s, "bundle.load", json!({"document": d}));
        let exp = run(&mut s, "bundle.export", json!({}));
        assert_eq!(
            exp["document"]["instances"][0]["params"],
            json!(text),
            "{name}: params written back unchanged"
        );
        assert_eq!(
            exp["document"], d,
            "{name}: the document is the one received"
        );
        assert_eq!(
            out["cid"],
            cid::seal(&d).unwrap(),
            "{name}: the seal of what was received"
        );
        assert_eq!(exp["cid"], out["cid"], "{name}");
        assert_ne!(
            exp["cid"], canonical_cid,
            "{name}: another sealed string, another CID"
        );
        assert_eq!(
            run(&mut s, "store.put", json!({"document": d}))["cid"],
            out["cid"],
            "{name}: the store names the document by the same CID"
        );
        assert_eq!(
            state(&mut s)["subnets"][0],
            canonical_sub,
            "{name}: one subnet"
        );
        // Idempotent: the export loads to itself.
        let mut t = session();
        let again = run(&mut t, "bundle.load", json!({"document": exp["document"]}));
        assert_eq!(
            again["cid"], exp["cid"],
            "{name}: load then export is stable"
        );
        assert_eq!(
            run(&mut t, "bundle.export", json!({}))["document"],
            exp["document"],
            "{name}"
        );
    }
}

/// The writer is the identity rule's canonical form, numbers included: an extra parameter key the
/// constructor ignores is still part of the text the hash covers.
#[test]
fn params_written_from_an_object_are_the_identity_rules_canonical_text() {
    let raw = r#"{"Payload":"","Initial":2,"ID":"orders","Capacity":3,"big":1e20,"odd":9007199254740993,"half":0.5,"w":"a\u00e9<>&"}"#;
    let mut inst = orders_inst();
    inst["params"] = serde_json::from_str(raw).unwrap();
    let mut s = session();
    run(
        &mut s,
        "bundle.load",
        json!({"document": doc(json!([inst]), json!([]))}),
    );
    let exp = run(&mut s, "bundle.export", json!({}));
    let text = exp["document"]["instances"][0]["params"].as_str().unwrap();
    assert_eq!(
        text,
        pflow_native_core::model_id::canonical_json(raw).unwrap()
    );
    assert!(text.contains("\"big\":100000000000000000000"), "{text}");
    assert!(text.contains("\"odd\":9007199254740992"), "{text}");
    assert!(
        text.contains("\"half\":0.5") && text.contains("\"w\":\"a\u{e9}<>&\""),
        "{text}"
    );
}

#[test]
fn a_bundle_with_no_context_or_a_wrong_one_is_refused_with_its_code() {
    for (name, ctx) in [
        ("missing", None),
        ("wrong", Some("https://example.com/other")),
    ] {
        let mut d = cafe();
        match ctx {
            None => {
                d.as_object_mut().unwrap().remove("@context");
            }
            Some(c) => d["@context"] = json!(c),
        }
        assert_code(&load_err(&d), "DOCUMENT_CONTEXT");
        let _ = name;
    }
}

#[test]
fn a_composition_carrying_its_own_cid_as_at_id_loads_and_exports_without_it() {
    let mut first = session();
    run(&mut first, "bundle.load", json!({"document": small_doc()}));
    let canon = run(&mut first, "bundle.export", json!({}));
    let (d, want) = (canon["document"].clone(), canon["cid"].clone());
    assert_eq!(json!(cid::seal(&d).unwrap()), want);
    let mut with_id = d.clone();
    with_id["@id"] = want.clone();
    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": with_id}));
    assert_eq!(out["cid"], want, "the seal ignores a top-level @id");
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["document"], d, "an export drops the @id");
    // What store.get returns for a stored composition loads as it is.
    let put = run(&mut s, "store.put", json!({"composition": true}));
    let got = run(&mut s, "store.get", json!({"cid": put["cid"]}));
    let mut t = session();
    let again = run(&mut t, "bundle.load", json!({"document": got}));
    assert_eq!(again["cid"], put["cid"]);
    // Someone else's CID there is refused: the document would not be what it says it is.
    let mut wrong = d;
    wrong["@id"] = json!("zNotTheCidOfThis");
    assert_code(&load_err(&wrong), "COMPOSITION_FIELD");
}

// ------------------------------------------------------------ 3. form (b): cafe

#[test]
fn cafe_loads_as_a_raw_bundle_and_says_what_it_lacks() {
    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": cafe()}));
    assert_eq!(out["form"], "bundle");
    assert_eq!(out["name"], "cafe-bundle");
    assert_eq!(out["cid"], CAFE_CID);
    assert_eq!(
        out["bundle_cid"],
        Value::Null,
        "only form (a) has a second CID"
    );
    assert_eq!(out["instances"], json!(["counter", "pantry", "staff"]));
    assert_eq!(out["flatten"], json!({"ok": true}));
    assert_eq!(out["validation"]["valid"], true);
    assert_eq!(out["validation"]["errors"], json!([]));

    // What it cannot draw, in order, each written for a person.
    assert_eq!(
        missing_codes(&out["missing"]),
        [
            "NO_PATTERN",
            "NO_PORTS",
            "NO_SCENARIOS",
            "NO_CONTENT",
            "NO_LAYOUT"
        ]
    );
    for m in out["missing"].as_array().unwrap() {
        let (code, text) = (m["code"].as_str().unwrap(), m["text"].as_str().unwrap());
        assert!(
            text.split_whitespace().count() >= 8,
            "{code} reads as a sentence: {text}"
        );
        assert!(
            !text.contains(code) && !text.contains('_') && !text.contains(['{', '}', '"']),
            "{code} is not a code dump: {text}"
        );
    }
    let text = |code: &str| -> String {
        out["missing"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["code"] == code)
            .unwrap()["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert!(
        text("NO_PATTERN").starts_with("3 of 3 subnets carry no pattern (counter, pantry, staff)")
    );
    assert!(text("NO_PORTS").contains("(counter, staff)"));
    assert!(
        !text("NO_PORTS").contains("pantry"),
        "pantry declares a port, so it is not listed"
    );
    assert!(text("NO_SCENARIOS").contains("no scenario is declared"));
    assert!(text("NO_CONTENT").contains("no model content is attached"));
    assert!(text("NO_LAYOUT").contains("no positions"));

    let st = state(&mut s);
    assert_eq!(st["document"]["form"], "bundle");
    assert_eq!(st["document"]["origin"], "bundle");
    assert_eq!(st["document"]["name"], "cafe-bundle");
    assert_eq!(st["document"]["edited"], false);
    assert_eq!(st["document"]["preserved_keys"], json!([]));
    assert_eq!(
        st["capabilities"],
        json!({
            "flatten": true, "patterns": false, "ontology_keys": false, "idioms": false,
            "ports": 1, "snap_together": true, "scenarios": 0, "content_entries": 0,
        })
    );
    assert_eq!(st["content"]["source"], "none");
    assert_eq!(
        missing_codes(&st["missing"]),
        missing_codes(&out["missing"])
    );
    for sub in st["subnets"].as_array().unwrap() {
        assert_eq!(sub["pattern"], "", "{} has no pattern", sub["id"]);
    }
}

#[test]
fn cafe_flattens_validates_and_has_an_incidence_matrix() {
    let mut s = session();
    run(&mut s, "bundle.load", json!({"document": cafe()}));

    let f = run(&mut s, "bundle.flatten", Value::Null);
    assert_eq!(f["ok"], true);
    assert_eq!(f["places"].as_array().unwrap().len(), 7);
    assert_eq!(f["transitions"].as_array().unwrap().len(), 6);
    let mut fused: Vec<&String> = f["fused_groups"].as_object().unwrap().keys().collect();
    fused.sort();
    assert_eq!(
        fused,
        [
            "espresso_finished",
            "espresso_started",
            "latte_finished",
            "latte_started"
        ],
        "event links with an id name the fused transition"
    );
    assert_eq!(
        f["fused_groups"]["espresso_started"],
        json!([
            "counter/start_espresso",
            "pantry/brew_espresso",
            "staff/acquire_espresso"
        ])
    );
    assert_eq!(f["wires"], json!({}));

    let v = run(&mut s, "bundle.validate", Value::Null);
    assert_eq!(v["valid"], true);
    assert_eq!(v["errors"], json!([]));
    let warnings: Vec<&str> = v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["code"].as_str().unwrap())
        .collect();
    assert_eq!(warnings, ["W_WORKFLOW_MULTI_CURSOR", "W_RESTRICTIVE_LINK"]);

    let inc = run(&mut s, "bundle.incidence", Value::Null);
    assert_eq!(inc["ok"], true);
    assert_eq!(inc["rows"].as_array().unwrap().len(), 7);
    assert_eq!(
        inc["cols"],
        json!([
            "counter/order",
            "espresso_started",
            "latte_started",
            "espresso_finished",
            "latte_finished",
            "pantry/restock"
        ])
    );
    assert_eq!(inc["rows"][3], "pantry/beans");
    assert_eq!(
        inc["matrix"][3],
        json!([0, -2, -1, 0, 0, 30]),
        "two beans per espresso, one per latte, thirty per restock"
    );
    assert_eq!(inc["invariants"], json!([]), "cafe declares no constraints");
    assert_eq!(inc["blocks"].as_array().unwrap().len(), 3);

    // Nothing to drag a link from: counter and staff declare no ports, so no candidates.
    let c = run(&mut s, "bundle.candidates", json!({"from": "pantry:beans"}));
    assert_eq!(c["lit"], 0);
    assert_eq!(c["candidates"], json!([]));
}

#[test]
fn cafe_link_ends_are_places_or_transitions_not_ports() {
    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": cafe()}));
    let links = out["links"].as_array().unwrap();
    assert_eq!(links.len(), 7);
    assert_eq!(links[0]["kind"], "event");
    assert_eq!(
        links[0]["from"],
        json!({"subnet": "counter", "port": "", "transition": "start_espresso"})
    );
    assert_eq!(
        links[0]["to"],
        json!({"subnet": "pantry", "port": "", "transition": "brew_espresso"})
    );
    assert_eq!(
        links[0]["key"],
        "event:counter:@transition/start_espresso->pantry:@transition/brew_espresso"
    );
    assert_eq!(links[0]["idiom"], Value::Null, "no pattern, no idiom");
    assert_eq!(links[0]["leak"], false);
    // The guard watches a port of the pantry: one end a transition, the other a port.
    assert_eq!(links[6]["kind"], "guard");
    assert_eq!(
        links[6]["key"],
        "guard:counter:@transition/start_espresso->pantry:beans"
    );
    assert_eq!(links[6]["to"]["port"], "beans");
    assert_eq!(links[6]["from"]["port"], "");
    for (i, l) in links.iter().enumerate() {
        assert_eq!(l["index"], i);
        let key = l["key"].as_str().unwrap();
        assert!(key.contains("@transition/"), "{key}");
    }
    assert_eq!(state(&mut s)["links"], out["links"]);
}

#[test]
fn cafe_exports_the_loaded_json_and_seals_to_the_known_cid() {
    let original = cafe();
    assert_eq!(cid::seal(&original).unwrap(), CAFE_CID);

    for how in ["value", "text"] {
        let arg = match how {
            "value" => original.clone(),
            _ => json!(fixture_text("cafe.bundle.json")),
        };
        let mut s = session();
        run(&mut s, "bundle.load", json!({"document": arg}));
        for form in [json!({}), json!({"form": "bundle"})] {
            let exp = run(&mut s, "bundle.export", form);
            assert_eq!(exp["form"], "bundle", "{how}");
            assert_eq!(exp["cid"], CAFE_CID, "{how}");
            assert_eq!(exp["bundle_cid"], Value::Null, "{how}");
            assert_eq!(exp["document"], original, "{how}: the loaded JSON exactly");
            assert_eq!(
                serde_json::to_string(&exp["document"]).unwrap(),
                serde_json::to_string(&original).unwrap(),
                "{how}: the bytes received, key order and all"
            );
        }
    }

    // No content attached, so `content: true` has nothing to add.
    let mut s = session();
    run(&mut s, "bundle.load", json!({"document": original}));
    let exp = run(&mut s, "bundle.export", json!({"content": true}));
    assert_eq!(exp["content"], Value::Null);
}

#[test]
fn editing_a_loaded_bundle_re_serialises_it_and_the_result_still_loads() {
    let original = cafe();

    // bundle.remove_link
    let mut s = session();
    run(&mut s, "bundle.load", json!({"document": original}));
    let removed = run(&mut s, "bundle.remove_link", json!({"index": 6}));
    assert_eq!(removed["removed"]["kind"], "guard");
    let st = state(&mut s);
    assert_eq!(st["document"]["edited"], true);
    assert_eq!(st["links"].as_array().unwrap().len(), 6);
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["form"], "bundle", "a loaded bundle stays a bundle");
    assert_ne!(exp["document"], original, "no longer the bytes received");
    assert_ne!(exp["cid"], CAFE_CID);
    assert_eq!(exp["document"]["@type"], "PetriNetBundle");
    assert_eq!(exp["document"]["links"].as_array().unwrap().len(), 6);
    assert_eq!(exp["document"]["subnets"].as_array().unwrap().len(), 3);
    assert_eq!(cid::seal(&exp["document"]).unwrap(), exp["cid"]);

    let mut t = session();
    let re = run(&mut t, "bundle.load", json!({"document": exp["document"]}));
    assert_eq!(re["form"], "bundle");
    assert_eq!(
        re["cid"], exp["cid"],
        "the edited export is itself a stable document"
    );
    assert_eq!(re["instances"], json!(["counter", "pantry", "staff"]));
    assert_eq!(re["links"].as_array().unwrap().len(), 6);
    assert_eq!(flat(&mut s).get("places"), flat(&mut t).get("places"));
    assert_eq!(
        run(&mut t, "bundle.export", json!({}))["document"],
        exp["document"],
        "unedited again, so now the received bytes are the edited export"
    );

    // Undo brings the original bytes back: the loaded JSON travels with the snapshot.
    run(&mut s, "edit.undo", Value::Null);
    let back = run(&mut s, "bundle.export", json!({}));
    assert_eq!(back["cid"], CAFE_CID);
    assert_eq!(back["document"], original);
    assert_eq!(state(&mut s)["document"]["edited"], false);

    // bundle.add_instance
    let mut s = session();
    run(&mut s, "bundle.load", json!({"document": original}));
    run(&mut s, "bundle.add_instance", json!({"id": "workers"}));
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["form"], "bundle");
    assert_ne!(exp["cid"], CAFE_CID);
    assert_eq!(exp["document"]["subnets"].as_array().unwrap().len(), 4);
    assert_eq!(exp["document"]["subnets"][3]["id"], "workers");
    let mut t = session();
    let re = run(&mut t, "bundle.load", json!({"document": exp["document"]}));
    assert_eq!(re["cid"], exp["cid"]);
    assert_eq!(
        re["instances"],
        json!(["counter", "pantry", "staff", "workers"])
    );
    assert_eq!(re["flatten"], json!({"ok": true}));
}

#[test]
fn reversed_subnets_and_links_load_and_the_unedited_export_returns_the_bytes_received() {
    let original = cafe();
    let mut reversed = original.clone();
    reversed["subnets"].as_array_mut().unwrap().reverse();
    reversed["links"].as_array_mut().unwrap().reverse();
    assert_ne!(reversed, original);
    assert_eq!(
        cid::seal(&reversed).unwrap(),
        CAFE_CID,
        "subnets and links are sets to the sealer"
    );

    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": reversed}));
    assert_eq!(out["cid"], CAFE_CID);
    assert_eq!(out["instances"], json!(["staff", "pantry", "counter"]));
    assert_eq!(out["flatten"], json!({"ok": true}));
    assert_eq!(
        out["links"][0]["kind"], "guard",
        "the order given is the order shown"
    );
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["cid"], CAFE_CID);
    assert_eq!(
        serde_json::to_string(&exp["document"]).unwrap(),
        serde_json::to_string(&reversed).unwrap(),
        "the bytes received, in the order received"
    );
    assert_eq!(state(&mut s)["document"]["edited"], false);

    // It is the same net: flatten agrees with the original's up to the order of places.
    let mut o = session();
    run(&mut o, "bundle.load", json!({"document": original}));
    let sorted = |s: &mut Session, k: &str| -> Vec<String> {
        let mut v: Vec<String> = run(s, "bundle.flatten", Value::Null)[k]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["id"].as_str().unwrap().to_string())
            .collect();
        v.sort();
        v
    };
    for k in ["places", "transitions"] {
        assert_eq!(sorted(&mut s, k), sorted(&mut o, k), "{k}");
    }
}

#[test]
fn extra_keys_a_build_does_not_interpret_survive_the_unedited_round_trip() {
    let mut with_keys = cafe();
    with_keys["subnets"][0]["pattern"] = json!("workflow");
    with_keys["subnets"][0]["params"] = json!({"places": 3});
    with_keys["x-provenance"] = json!({"minted_by": "a test"});
    assert_ne!(cid::seal(&with_keys).unwrap(), CAFE_CID);

    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": with_keys}));
    assert_eq!(
        out["cid"],
        json!(cid::seal(&with_keys).unwrap()),
        "the CID is that of the document as received, extra keys included"
    );
    assert_eq!(
        missing_codes(&out["missing"]),
        [
            "NO_PATTERN",
            "NO_PORTS",
            "NO_SCENARIOS",
            "NO_CONTENT",
            "NO_LAYOUT",
            "KEYS_PRESERVED"
        ]
    );
    let kept = out["missing"].as_array().unwrap().last().unwrap()["text"]
        .as_str()
        .unwrap()
        .to_string();
    for key in ["subnets[0].pattern", "subnets[0].params", "x-provenance"] {
        assert!(kept.contains(key), "{key} is named in: {kept}");
    }
    let st = state(&mut s);
    assert_eq!(
        st["document"]["preserved_keys"],
        json!(["subnets[0].pattern", "subnets[0].params", "x-provenance"])
    );
    // The key is kept but not read: the subnet is still patternless.
    assert_eq!(st["subnets"][0]["pattern"], "");

    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["cid"], out["cid"], "same CID");
    assert_eq!(exp["document"], with_keys);
    assert_eq!(
        serde_json::to_string(&exp["document"]).unwrap(),
        serde_json::to_string(&with_keys).unwrap()
    );
    // A document without them reports nothing preserved.
    let mut plain = session();
    let out = run(&mut plain, "bundle.load", json!({"document": cafe()}));
    assert!(!missing_codes(&out["missing"]).contains(&"KEYS_PRESERVED".to_string()));
}

#[test]
fn an_edit_of_a_raw_bundle_keeps_every_key_and_part_it_did_not_touch() {
    let mut with_keys = cafe();
    with_keys["subnets"][0]["pattern"] = json!("workflow");
    with_keys["subnets"][0]["params"] = json!({"places": 3});
    with_keys["subnets"][1]["extra_sub_key"] = json!("kept");
    with_keys["x-provenance"] = json!({"minted_by": "a test"});
    with_keys["links"][0]["future_link_key"] = json!(7);
    with_keys["links"][6]["on_the_removed_link"] = json!(true);
    let n = with_keys["links"].as_array().unwrap().len();

    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": with_keys}));
    assert_eq!(out["cid"], json!(cid::seal(&with_keys).unwrap()));
    // Remove the last link (the one carrying `on_the_removed_link`).
    run(&mut s, "bundle.remove_link", json!({"index": n - 1}));
    let st = state(&mut s);
    assert_eq!(st["document"]["edited"], true);
    let exp = run(&mut s, "bundle.export", json!({}));
    let doc = &exp["document"];
    assert_eq!(doc["links"].as_array().unwrap().len(), n - 1);
    assert_eq!(doc["subnets"][0]["pattern"], "workflow", "a recipe key");
    assert_eq!(doc["subnets"][0]["params"], json!({"places": 3}));
    assert_eq!(
        doc["subnets"][1]["extra_sub_key"], "kept",
        "a key on another subnet"
    );
    assert_eq!(
        doc["x-provenance"],
        json!({"minted_by": "a test"}),
        "a top-level key"
    );
    assert_eq!(doc["links"][0]["future_link_key"], 7, "a key on a link");
    assert!(
        !doc.to_string().contains("on_the_removed_link"),
        "the key of the link that was removed goes with it"
    );
    // Everything else is the document received, byte for byte, minus the removed link.
    let mut want = with_keys.clone();
    want["links"].as_array_mut().unwrap().pop();
    assert_eq!(doc, &want);
    assert_eq!(exp["cid"], json!(cid::seal(&want).unwrap()));

    // What state says it keeps is what the export has: the removed link's key is gone from the
    // list, the others are named.
    let kept = st["document"]["preserved_keys"].as_array().unwrap();
    for key in [
        "subnets[0].pattern",
        "subnets[0].params",
        "subnets[1].extra_sub_key",
        "x-provenance",
        "links[0].future_link_key",
    ] {
        assert!(kept.contains(&json!(key)), "{key} in {kept:?}");
    }
    assert!(
        !kept
            .iter()
            .any(|k| k.as_str().unwrap().contains("on_the_removed_link")),
        "{kept:?}"
    );
    assert!(missing_codes(&st["missing"]).contains(&"KEYS_PRESERVED".to_string()));

    // The edited export still loads, to itself.
    let mut again = session();
    let r = run(
        &mut again,
        "bundle.load",
        json!({"document": exp["document"]}),
    );
    assert_eq!(r["cid"], exp["cid"]);
    assert_eq!(
        run(&mut again, "bundle.export", json!({}))["document"],
        exp["document"]
    );

    // Undo is the document received.
    run(&mut s, "edit.undo", json!({}));
    assert_eq!(
        run(&mut s, "bundle.export", json!({}))["document"],
        with_keys
    );
}

#[test]
fn an_edit_of_a_bundle_that_names_its_parts_by_cid_keeps_the_references() {
    let root = format!(
        "{}/tests/fixtures/model/fixtures",
        env!("CARGO_MANIFEST_DIR")
    );
    let read = |f: &str| -> Value {
        serde_json::from_slice(&std::fs::read(format!("{root}/{f}")).unwrap()).unwrap()
    };
    let handoff = read("compose-handoff.jsonld");
    let mut s = session();
    for part in ["part-maker.jsonld", "part-taker.jsonld"] {
        run(&mut s, "store.put", json!({"document": read(part)}));
    }
    let out = run(&mut s, "bundle.load", json!({"document": handoff}));
    assert_eq!(out["cid"], json!(cid::seal(&handoff).unwrap()));
    run(&mut s, "bundle.remove_link", json!({"index": 0}));
    let exp = run(&mut s, "bundle.export", json!({}));
    let mut want = handoff.clone();
    want.as_object_mut().unwrap().remove("links");
    assert_eq!(
        exp["document"], want,
        "both parts are still named by cid, no model inlined"
    );
    assert_eq!(state(&mut s)["document"]["preserved_keys"], json!([]));
}

// ------------------------------------------------------------ 4. across the forms

#[test]
fn orders_as_a_bundle_loads_raw_flattens_alike_and_loses_only_its_patterns() {
    let mut a = session();
    run(&mut a, "bundle.open", json!({}));
    let as_composition = run(&mut a, "bundle.export", json!({}));
    let as_bundle = run(&mut a, "bundle.export", json!({"form": "bundle"}));
    assert_eq!(as_bundle["form"], "bundle");
    assert_eq!(as_bundle["bundle_cid"], Value::Null);
    assert_eq!(
        as_bundle["cid"], as_composition["bundle_cid"],
        "bundle_cid is the seal of the Bundle the composition flattens to"
    );
    assert_eq!(
        cid::seal(&as_bundle["document"]).unwrap(),
        ORDERS_BUNDLE_CID
    );
    assert_eq!(as_bundle["document"]["@type"], "PetriNetBundle");
    assert_eq!(
        as_bundle["document"]["subnets"].as_array().unwrap().len(),
        3
    );
    assert_eq!(as_bundle["document"]["links"].as_array().unwrap().len(), 3);

    let mut b = session();
    let raw = run(
        &mut b,
        "bundle.load",
        json!({"document": as_bundle["document"]}),
    );
    assert_eq!(raw["form"], "bundle");
    assert_eq!(
        raw["cid"], ORDERS_BUNDLE_CID,
        "the received bytes are the Bundle's seal"
    );
    assert_eq!(raw["cid"], as_composition["bundle_cid"]);
    assert_eq!(
        run(&mut b, "bundle.export", json!({}))["document"],
        as_bundle["document"]
    );

    // The same flat net, the same links, the same invariants.
    assert_eq!(flat(&mut a), flat(&mut b));
    let (sa, sb) = (state(&mut a), state(&mut b));
    let keys = |s: &Value| -> Vec<Value> {
        s["links"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["key"].clone())
            .collect()
    };
    assert_eq!(keys(&sa), keys(&sb));
    assert_eq!(
        run(&mut a, "bundle.incidence", Value::Null),
        run(&mut b, "bundle.incidence", Value::Null)
    );

    // It lost its patterns, and the views are told so...
    assert_eq!(sb["document"]["origin"], "bundle");
    let codes = missing_codes(&sb["missing"]);
    assert!(codes.contains(&"NO_PATTERN".to_string()), "{codes:?}");
    assert!(
        !codes.contains(&"NO_PORTS".to_string()),
        "the ports survive: {codes:?}"
    );
    assert_eq!(sb["capabilities"]["patterns"], false);
    assert_eq!(sb["capabilities"]["idioms"], false);
    let mut keyed = 0;
    for (x, y) in sa["subnets"]
        .as_array()
        .unwrap()
        .iter()
        .zip(sb["subnets"].as_array().unwrap())
    {
        assert_eq!(x["pattern"].as_str().map(|p| !p.is_empty()), Some(true));
        assert_eq!(y["pattern"], "");
        // ...no ontology keys on the ports, but the same ports.
        let (px, py) = (
            x["ports"].as_array().unwrap(),
            y["ports"].as_array().unwrap(),
        );
        assert_eq!(px.len(), py.len(), "{}", x["id"]);
        for (p, q) in px.iter().zip(py) {
            keyed += usize::from(p["pattern_port"].is_string());
            assert_eq!(q["pattern_port"], Value::Null, "{q}");
            for k in ["id", "kind", "target", "place", "transition", "schema"] {
                assert_eq!(p[k], q[k], "port {} {k}", p["id"]);
            }
        }
    }
    assert!(keyed > 0, "the composition's ports carry ontology keys");
    assert_eq!(
        sa["capabilities"]["ports"], sb["capabilities"]["ports"],
        "declared ports are kept"
    );
    // ...no idiom or leak flag on the links.
    assert!(sa["links"]
        .as_array()
        .unwrap()
        .iter()
        .any(|l| l["leak"] == true));
    for l in sb["links"].as_array().unwrap() {
        assert_eq!(l["idiom"], Value::Null);
        assert_eq!(l["leak"], false);
    }
    // Snap-together still lights ports: the verdicts come from pflow-compose, not the pattern.
    let ca = run(
        &mut a,
        "bundle.candidates",
        json!({"from": "orders:dequeue"}),
    );
    let cb = run(
        &mut b,
        "bundle.candidates",
        json!({"from": "orders:dequeue"}),
    );
    assert!(cb["lit"].as_u64().unwrap() > 0, "{cb}");
    assert_eq!(ca["lit"], cb["lit"]);
    let lit = |c: &Value| -> Vec<Value> {
        c["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|x| x["ok"] == true)
            .map(|x| x["key"].clone())
            .collect()
    };
    assert_eq!(lit(&ca), lit(&cb));

    // The pattern is gone for good: it cannot be written as form (a) again.
    let err = refused(&mut b, "bundle.export", json!({"form": "composition"}));
    assert_code(&err, "EXPORT_NEEDS_PATTERNS");
    assert!(
        err.contains("\"orders\""),
        "names the first patternless subnet: {err}"
    );
    // Auto stays the bundle.
    assert_eq!(run(&mut b, "bundle.export", json!({}))["form"], "bundle");
    // The composition it came from can still write either form.
    assert_eq!(
        run(&mut a, "bundle.export", json!({"form": "composition"}))["cid"],
        ORDERS_CID
    );
}

#[test]
fn a_composition_with_a_catalog_instance_added_to_a_raw_bundle_cannot_be_form_a() {
    let mut s = session();
    run(&mut s, "bundle.load", json!({"document": cafe()}));
    run(&mut s, "bundle.add_instance", json!({"id": "workers"}));
    let err = refused(&mut s, "bundle.export", json!({"form": "composition"}));
    assert_code(&err, "EXPORT_NEEDS_PATTERNS");
    assert!(err.contains("\"counter\""), "{err}");
}

// ------------------------------------------------------------ 5. every refusal

#[test]
fn composition_type_is_refused_by_the_form_a_reader() {
    // bundle.load routes on @type (anything else is a Bundle), so this is the reader's own guard.
    let mut d = small_doc();
    d["@type"] = json!("PetriNetBundle");
    let err = Composition::from_composition_document(&d)
        .expect_err("refused")
        .to_string();
    assert_code(&err, "COMPOSITION_TYPE");
    let err = Composition::from_composition_document(&json!({"@context": CONTEXT}))
        .expect_err("refused")
        .to_string();
    assert_code(&err, "COMPOSITION_TYPE");
}

#[test]
fn document_context_is_refused() {
    let mut d = small_doc();
    d["@context"] = json!("https://example.org/schema");
    assert_code(&load_err(&d), "DOCUMENT_CONTEXT");
    d.as_object_mut().unwrap().remove("@context");
    assert_code(&load_err(&d), "DOCUMENT_CONTEXT");
    d["@context"] = json!(["https://pflow.xyz/schema"]);
    assert_code(&load_err(&d), "DOCUMENT_CONTEXT");
}

#[test]
fn document_type_is_refused() {
    let mut d = small_doc();
    d["@type"] = json!("Nope");
    let err = load_err(&d);
    assert_code(&err, "DOCUMENT_TYPE");
    assert!(
        err.contains("PatternComposition") && err.contains("PetriNetBundle"),
        "names what it does take: {err}"
    );
    assert_code(&load_err(&json!([1, 2])), "DOCUMENT_TYPE");
    assert_code(
        &load_err(&json!({"name": "no type, no subnets"})),
        "DOCUMENT_TYPE",
    );
    // A stored net is not a composition either.
    let mut s = session();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let net = store::open().put_model(&s.doc, Some("sir"), None).unwrap();
    let err = refused(&mut s, "bundle.load", json!({"cid": net["cid"]}));
    assert_code(&err, "DOCUMENT_TYPE");
}

#[test]
fn load_needs_a_document_or_a_cid_and_says_what_is_wrong_with_it() {
    let mut s = session();
    assert!(refused(&mut s, "bundle.load", json!({})).contains("`document` or `cid`"));
    assert!(refused(
        &mut s,
        "bundle.load",
        json!({"document": small_doc(), "cid": ORDERS_CID})
    )
    .contains("`document` or `cid`"));
    let err = refused(&mut s, "bundle.load", json!({"document": "{not json"}));
    assert!(err.starts_with("document: not valid JSON"), "{err}");
    let err = refused(&mut s, "bundle.load", json!({"cid": ORDERS_CID}));
    assert!(
        err.contains("not in the local store") || err.contains("no model"),
        "{err}"
    );
}

#[test]
fn a_duplicate_instance_is_refused() {
    let d = doc(
        json!([orders_inst(), workers_inst(), orders_inst()]),
        json!([]),
    );
    let err = load_err(&d);
    assert_code(&err, "INSTANCE_DUPLICATE");
    assert!(err.contains("\"orders\""), "{err}");
}

#[test]
fn a_bad_instance_id_or_one_that_disagrees_with_its_params_is_refused() {
    for bad in ["a:b", "a/b", ""] {
        let mut i = orders_inst();
        i["id"] = json!(bad);
        let err = load_err(&doc(json!([i]), json!([])));
        assert_code(&err, "INSTANCE_ID");
        assert!(err.contains("neither ':' nor '/'"), "{bad:?}: {err}");
    }
    // The id the document names and the id the params build must be one.
    for (mut inst, key) in [
        (orders_inst(), "ID"),
        (workers_inst(), "id"),
        (payment_inst(), "id"),
    ] {
        inst["params"][key] = json!("other");
        let err = load_err(&doc(json!([inst.clone()]), json!([])));
        assert_code(&err, "INSTANCE_ID");
        assert!(
            err.contains("the ids must agree"),
            "{}: {err}",
            inst["pattern"]
        );
    }
    // The same mismatch, with params as text.
    let mut i = orders_inst();
    i["params"] = json!("{\"Capacity\":3,\"ID\":\"other\",\"Initial\":2,\"Payload\":\"\"}");
    assert_code(&load_err(&doc(json!([i]), json!([]))), "INSTANCE_ID");
}

#[test]
fn an_unknown_pattern_is_refused() {
    let mut i = orders_inst();
    i["pattern"] = json!("stack");
    let err = load_err(&doc(json!([i]), json!([])));
    assert_code(&err, "PATTERN_UNKNOWN");
    assert!(
        err.contains("queue") && err.contains("resource-pool") && err.contains("saga"),
        "{err}"
    );
    let mut i = orders_inst();
    i["pattern"] = json!("");
    assert_code(&load_err(&doc(json!([i]), json!([]))), "PATTERN_UNKNOWN");
}

#[test]
fn params_that_are_not_an_object_or_json_text_are_refused() {
    for bad in [
        json!("not json"),
        json!("{\"ID\":"),
        json!([1]),
        json!("[1]"),
        json!("3"),
        json!(7),
        json!(null),
        json!(true),
    ] {
        let mut i = orders_inst();
        i["params"] = bad.clone();
        assert_code(&load_err(&doc(json!([i]), json!([]))), "PARAMS_INVALID");
    }
    // Missing altogether is a missing field.
    let mut i = orders_inst();
    i.as_object_mut().unwrap().remove("params");
    assert_code(&load_err(&doc(json!([i]), json!([]))), "COMPOSITION_FIELD");
}

#[test]
fn a_constructor_that_refuses_its_parameters_refuses_the_document() {
    let mut cases: Vec<(&str, Value)> = vec![];
    let mut i = orders_inst();
    i["params"] = json!({});
    cases.push(("queue without params", i));
    let mut i = orders_inst();
    i["params"]["Capacity"] = json!("three");
    cases.push(("queue capacity a string", i));
    let mut i = orders_inst();
    i["params"]["Initial"] = json!(9);
    cases.push(("queue initial above capacity", i));
    let mut i = orders_inst();
    i["params"]["Capacity"] = json!(-1);
    cases.push(("queue capacity negative", i));
    let mut i = workers_inst();
    i["params"] = json!({"id": "workers"});
    cases.push(("pool without capacity", i));
    let mut i = payment_inst();
    i["params"]["steps"] = json!([]);
    cases.push(("saga without steps", i));
    let mut i = payment_inst();
    i["params"]["accounts"] = json!("none");
    cases.push(("saga accounts not an object", i));
    let mut i = payment_inst();
    i["params"]["steps"][1]["moves"][0]["to"] = json!("nobody");
    cases.push(("saga move to an unknown account", i));
    for (what, inst) in cases {
        let err = load_err(&doc(json!([inst]), json!([])));
        assert_code(&err, "INSTANCE_BUILD");
        assert!(
            err.contains("\"orders\"")
                || err.contains("\"workers\"")
                || err.contains("\"payment\""),
            "{what}: {err}"
        );
    }
    let mut i = payment_inst();
    i["params"]["steps"][1]["moves"][0]["to"] = json!("nobody");
    let err = load_err(&doc(json!([i]), json!([])));
    assert!(
        err.contains("unknown account \"nobody\""),
        "the constructor's message follows: {err}"
    );
}

#[test]
fn a_link_pflow_compose_or_the_page_refuses_refuses_the_document() {
    let two = json!([orders_inst(), workers_inst()]);
    let cases = [
        (
            "no such port",
            event(("orders", "nope"), ("workers", "acquire")),
            "E_BAD_ENDPOINT",
        ),
        (
            "no such subnet",
            event(("ghost", "dequeue"), ("workers", "acquire")),
            "E_BAD_ENDPOINT",
        ),
        (
            "both ends on one subnet",
            event(("orders", "dequeue"), ("orders", "enqueue")),
            "PAGE_SAME_SUBNET",
        ),
        (
            "a token link between transitions",
            json!({"kind": "token",
                "from": {"subnet": "orders", "port": "dequeue"},
                "to": {"subnet": "workers", "port": "acquire"}}),
            "E_ENDPOINT_KIND",
        ),
    ];
    for (what, link, inner) in cases {
        let err = load_err(&doc(two.clone(), json!([link])));
        assert_code(&err, "LINK_REFUSED");
        assert!(err.contains(&format!("[{inner}]")), "{what}: {err}");
    }
    // The same link twice: the page's rule, with its code.
    let err = load_err(&doc(
        two.clone(),
        json!([dequeue_acquire(), dequeue_acquire()]),
    ));
    assert_code(&err, "LINK_REFUSED");
    assert!(err.contains("[PAGE_DUPLICATE]"), "{err}");
    // An event link either way round is the same link.
    let err = load_err(&doc(
        two,
        json!([
            dequeue_acquire(),
            event(("workers", "acquire"), ("orders", "dequeue"))
        ]),
    ));
    assert_code(&err, "LINK_REFUSED");
    assert!(err.contains("[PAGE_DUPLICATE]"), "{err}");
    // An unknown link kind is a field problem, not a verdict.
    let mut l = dequeue_acquire();
    l["kind"] = json!("wire");
    assert_code(
        &load_err(&doc(json!([orders_inst(), workers_inst()]), json!([l]))),
        "COMPOSITION_FIELD",
    );
}

#[test]
fn an_unknown_key_is_refused_at_every_level() {
    let base = small_doc();
    let mut d = base.clone();
    d["zzz"] = json!(1);
    let err = load_err(&d);
    assert_code(&err, "COMPOSITION_FIELD");
    assert!(err.contains("\"zzz\"") && err.contains("document"), "{err}");

    let mut d = base.clone();
    d["instances"][0]["zzz"] = json!(1);
    let err = load_err(&d);
    assert_code(&err, "COMPOSITION_FIELD");
    assert!(err.contains("instance"), "{err}");

    let mut d = base.clone();
    d["links"][0]["zzz"] = json!(1);
    let err = load_err(&d);
    assert_code(&err, "COMPOSITION_FIELD");
    assert!(err.contains("link"), "{err}");

    // An end of a link: a key there would not survive an export either.
    for end in ["from", "to"] {
        let mut d = base.clone();
        d["links"][0][end]["zzz"] = json!(1);
        assert_code(&load_err(&d), "COMPOSITION_FIELD");
    }
    let mut l = dequeue_acquire();
    l["rename"] = json!({"a": 1});
    let mut d = base.clone();
    d["links"] = json!([l]);
    assert_code(&load_err(&d), "COMPOSITION_FIELD");

    // A key that is `null` is still a key.
    let mut d = base.clone();
    d["zzz"] = Value::Null;
    assert_code(&load_err(&d), "COMPOSITION_FIELD");
}

#[test]
fn a_mistyped_field_is_refused_rather_than_dropped() {
    let base = small_doc();
    // (what, JSON pointer, the wrong value)
    let cases = [
        ("instances not a list", "/instances", json!("orders")),
        ("links not a list", "/links", json!({})),
        ("name a number", "/name", json!(5)),
        ("description a list", "/description", json!(["a"])),
        ("namespace a string", "/namespace", json!("yes")),
        ("arc_merge unknown", "/arc_merge", json!("min")),
        ("arc_merge a number", "/arc_merge", json!(1)),
        ("instance id a number", "/instances/0/id", json!(1)),
        ("instance label a number", "/instances/0/label", json!(5)),
        ("instance why a number", "/instances/0/why", json!(5)),
        ("instance not an object", "/instances/0", json!("orders")),
        ("link not an object", "/links/0", json!("orders:dequeue")),
        ("link id a number", "/links/0/id", json!(5)),
        ("link condition a number", "/links/0/condition", json!(5)),
        ("link lowering a number", "/links/0/lowering", json!(5)),
        (
            "link end without a port",
            "/links/0/from",
            json!({"subnet": "orders"}),
        ),
        ("link end a number", "/links/0/to", json!(3)),
    ];
    for (what, at, wrong) in cases {
        let err = load_err(&with(&base, at, wrong));
        assert!(
            err.starts_with("[COMPOSITION_FIELD] "),
            "{what}: expected a [COMPOSITION_FIELD] refusal, got: {err}"
        );
    }
    // Null is as good as absent for an optional field.
    let mut d = base.clone();
    d["description"] = Value::Null;
    d["instances"][0]["label"] = Value::Null;
    d["instances"][0]["why"] = Value::Null;
    let mut s = session();
    run(&mut s, "bundle.load", json!({"document": d}));
}

#[test]
fn the_tokenmodel_subnet_shape_is_refused() {
    let tokenmodel = json!({
        "@context": CONTEXT,
        "@type": "PetriNetBundle",
        "name": "tokenmodel",
        "subnets": [],
        "links": [{"kind": "token",
            "from_subnet": "a", "from_place": "p", "to_subnet": "b", "to_place": "q"}],
    });
    let err = load_err(&tokenmodel);
    assert_code(&err, "BUNDLE_SHAPE_TOKENMODEL");
    assert!(err.contains("from_subnet"), "{err}");
    // Either end is enough, and it does not matter that the @type is missing.
    let mut one_end = tokenmodel.clone();
    one_end.as_object_mut().unwrap().remove("@type");
    one_end["links"] = json!([{"kind": "token", "to_subnet": "b"}]);
    assert_code(&load_err(&one_end), "BUNDLE_SHAPE_TOKENMODEL");
}

#[test]
fn a_bundle_that_does_not_parse_is_refused() {
    let ctx = json!(CONTEXT);
    assert_code(
        &load_err(&json!({"@context": ctx, "@type": "PetriNetBundle", "subnets": "x"})),
        "BUNDLE_PARSE",
    );
    assert_code(
        &load_err(
            &json!({"@context": ctx, "@type": "PetriNetBundle", "subnets": [{"id": "n", "model": 5}]}),
        ),
        "BUNDLE_PARSE",
    );
    assert_code(
        &load_err(
            &json!({"@context": ctx, "@type": "PetriNetBundle", "subnets": [], "links": "x"}),
        ),
        "BUNDLE_PARSE",
    );
    // A bundle with no @type but with subnets is read as one; a mistyped @type is a parse error.
    assert_code(
        &load_err(&json!({"@context": ctx, "@type": 5, "subnets": []})),
        "BUNDLE_PARSE",
    );
    let mut untyped = cafe();
    untyped.as_object_mut().unwrap().remove("@type");
    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": untyped}));
    assert_eq!(out["form"], "bundle");
}

#[test]
fn a_part_reference_the_store_cannot_resolve_is_refused_naming_its_cid() {
    let mut reference = json!({"@context": CONTEXT, "@type": "PetriNetBundle", "name": "parts",
    "subnets": [
        cafe()["subnets"][0].clone(),
        {"id": "pc", "net_type": "WorkflowNet", "cid": CAFE_CID},
    ]});
    let mut s = session();
    let err = refused(&mut s, "bundle.load", json!({"document": reference}));
    assert_code(&err, "BUNDLE_PART_UNRESOLVED");
    assert!(err.contains(CAFE_CID), "names the CID: {err}");
    assert!(err.contains("subnets[1]"), "and the subnet: {err}");
    // A reference that is not even a CID does not resolve either.
    reference["subnets"][1]["cid"] = json!("zb2rhnotacid");
    assert_code(
        &refused(&mut s, "bundle.load", json!({"document": reference})),
        "BUNDLE_PART_UNRESOLVED",
    );
}

#[test]
fn a_part_reference_to_a_net_in_the_store_is_resolved_to_draw_and_the_document_stays_as_received() {
    let mut s = session();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let sir = store::open().put_model(&s.doc, Some("sir"), None).unwrap();
    let cid = sir["cid"].as_str().unwrap().to_string();
    let reference = json!({"@context": CONTEXT, "@type": "PetriNetBundle", "name": "parts",
        "subnets": [{"id": "epi", "net_type": "WorkflowNet", "cid": cid}]});

    let out = run(&mut s, "bundle.load", json!({"document": reference}));
    assert_eq!(out["form"], "bundle");
    assert_eq!(out["instances"], json!(["epi"]));
    assert_eq!(out["flatten"], json!({"ok": true}));
    // The CID reported is the seal of the document received, and the model identity says the same
    // (a named wrapper is its own composition; its one part is listed).
    assert_eq!(out["cid"], cid::seal(&reference).unwrap());
    assert_eq!(out["model_cid"], out["cid"]);
    assert_eq!(out["reduced"], false);
    assert_eq!(out["parts"], json!([cid]));
    let f = flat(&mut s);
    let place_ids: Vec<&str> = f["places"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    // No `namespace`, so the one subnet's places keep their own ids.
    let mut place_ids = place_ids;
    place_ids.sort();
    assert_eq!(place_ids, ["I", "R", "S"], "the stored net's places");
    assert_eq!(f["transitions"].as_array().unwrap().len(), 2);
    let st = state(&mut s);
    let sub = &st["subnets"][0];
    assert_eq!(sub["net_type"], "WorkflowNet");
    assert_eq!(sub["places"].as_array().unwrap().len(), 3);
    // Resolved only to flatten and draw: nothing was edited, no key is "preserved", and the
    // export is the document received, `cid` and all.
    assert_eq!(st["document"]["edited"], false);
    assert_eq!(st["document"]["preserved_keys"], json!([]));
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["form"], "bundle");
    assert_eq!(exp["document"], reference);
    assert_eq!(exp["cid"], out["cid"]);
    assert_eq!(exp["model_cid"], out["model_cid"]);

    // It needs its part to draw, not to be named: an empty store refuses the load and says which
    // CID it wants; with the part there again the same CID comes back.
    let mut elsewhere = session();
    let err = refused(
        &mut elsewhere,
        "bundle.load",
        json!({"document": exp["document"]}),
    );
    assert_code(&err, "BUNDLE_PART_UNRESOLVED");
    assert!(err.contains(&cid), "{err}");
    run(&mut elsewhere, "file.open_example", json!({"name": "sir"}));
    let doc = elsewhere.doc.clone();
    store::open().put_model(&doc, Some("sir"), None).unwrap();
    let again = run(
        &mut elsewhere,
        "bundle.load",
        json!({"document": exp["document"]}),
    );
    assert_eq!(again["cid"], exp["cid"]);
    assert_eq!(flat(&mut elsewhere), flat(&mut s));
    assert_eq!(
        run(&mut elsewhere, "bundle.export", json!({}))["document"],
        exp["document"]
    );
}

/// The model-identity vectors that name their parts by CID (tests/fixtures/model/golden.json):
/// with the parts in the store, `bundle.load` reports the seal the vector says, and the model
/// CID, and `bundle.export` is the document received.
#[test]
fn the_by_reference_identity_vectors_load_to_their_golden_cids_and_export_unchanged() {
    let root = format!("{}/tests/fixtures", env!("CARGO_MANIFEST_DIR"));
    let read = |rel: &str| -> Value {
        let p = format!("{root}/{rel}");
        serde_json::from_slice(&std::fs::read(&p).unwrap_or_else(|e| panic!("{p}: {e}")))
            .unwrap_or_else(|e| panic!("{p}: {e}"))
    };
    let golden = read("model/golden.json");
    let mut checked = vec![];
    for v in golden["vectors"].as_array().unwrap() {
        let name = v["name"].as_str().unwrap();
        let parts = v["parts"].as_array().cloned().unwrap_or_default();
        // The vectors that are documents naming parts by CID (the parts themselves and the plans
        // have none).
        if parts.is_empty() || v["file"].as_str().unwrap().contains("plan-") {
            continue;
        }
        let document = read(&format!("model/{}", v["file"].as_str().unwrap()));
        let mut s = session();
        for file in [
            "model/fixtures/part-maker.jsonld",
            "model/fixtures/part-taker.jsonld",
            "cid/coffee-shop.jsonld",
        ] {
            run(&mut s, "store.put", json!({"document": read(file)}));
        }
        let out = run(&mut s, "bundle.load", json!({"document": document}));
        assert_eq!(
            out["cid"], v["sealed"],
            "{name}: load.cid is the seal of the document received"
        );
        assert_eq!(
            out["model_cid"], v["model"],
            "{name}: the model CID the identity rule gives"
        );
        assert_eq!(out["reduced"], v["reduced"], "{name}");
        assert_eq!(out["parts"], json!(parts), "{name}");
        let st = state(&mut s);
        assert_eq!(st["document"]["edited"], false, "{name}");
        assert_eq!(st["document"]["preserved_keys"], json!([]), "{name}");
        let exp = run(&mut s, "bundle.export", json!({}));
        assert_eq!(
            exp["document"], document,
            "{name}: export is the document received"
        );
        assert_eq!(exp["cid"], v["sealed"], "{name}");
        checked.push(name.to_string());
    }
    // compose-handoff, its relinked twin, and the two wrappers (one bare, so it reduces to its part).
    assert_eq!(checked.len(), 4, "{checked:?}");
}

#[test]
fn a_net_whose_type_names_its_net_type_resolves_too() {
    // examples/producer-consumer.json is `@type: "ResourceNet"`, which the store keeps as a net
    // (put_model seals it like any other) though `kind_of` only knows the plain `PetriNet`.
    let mut s = session();
    run(
        &mut s,
        "file.open_example",
        json!({"name": "producer-consumer"}),
    );
    assert_eq!(s.doc.to_value()["@type"], "ResourceNet");
    let stored = store::open().put_model(&s.doc, Some("pc"), None).unwrap();
    let reference = json!({"@context": CONTEXT, "@type": "PetriNetBundle", "name": "parts",
        "subnets": [{"id": "pc", "net_type": "ResourceNet", "cid": stored["cid"]}]});
    let out = run(&mut s, "bundle.load", json!({"document": reference}));
    assert_eq!(out["instances"], json!(["pc"]));
    assert_eq!(out["flatten"], json!({"ok": true}));
    assert!(!flat(&mut s)["places"].as_array().unwrap().is_empty());
}

// ------------------------------------------------------------ 6. the session around it

#[test]
fn bundle_load_is_one_undo_step() {
    let mut s = session();
    run(&mut s, "bundle.open", json!({}));
    let before = state(&mut s);
    let depth = undo_depth(&mut s);

    run(&mut s, "bundle.load", json!({"document": cafe()}));
    assert_eq!(
        s.journal.last().unwrap().command,
        "bundle.load",
        "an edit is journaled"
    );
    assert_eq!(undo_depth(&mut s), depth + 1, "exactly one step");
    let cafe_state = state(&mut s);
    assert_eq!(
        cafe_state["instances"],
        json!(["counter", "pantry", "staff"])
    );

    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(
        state(&mut s),
        before,
        "undo restores the previous composition"
    );
    assert_eq!(
        run(&mut s, "bundle.export", json!({}))["cid"],
        ORDERS_CID,
        "document state came back with it"
    );

    run(&mut s, "edit.redo", Value::Null);
    assert_eq!(state(&mut s), cafe_state, "redo brings it back");
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["cid"], CAFE_CID, "including the loaded bytes");
    assert_eq!(exp["document"], cafe());

    // Undo all the way: before bundle.open there was no composition.
    run(&mut s, "edit.undo", Value::Null);
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.composition.is_none());
    run(&mut s, "edit.redo", Value::Null);
    run(&mut s, "edit.redo", Value::Null);
    assert_eq!(state(&mut s), cafe_state);
}

#[test]
fn a_refused_load_changes_nothing() {
    let mut s = session();
    run(&mut s, "bundle.open", json!({}));
    let before = state(&mut s);
    let depth = undo_depth(&mut s);
    let journal = s.journal.len();
    let mut bad = small_doc();
    bad["instances"][0]["pattern"] = json!("stack");
    assert_code(
        &refused(&mut s, "bundle.load", json!({"document": bad})),
        "PATTERN_UNKNOWN",
    );
    let mut refused_content = small_doc();
    refused_content["name"] = json!("small");
    let content = json!({"@context": CONTEXT, "@type": "ModelContent", "name": "c",
        "subject": {"kind": "composition", "name": "c"},
        "entries": [{"id": "n", "kind": "note", "text": "t", "about": ["place:orders/nope"]}]});
    assert_code(
        &refused(
            &mut s,
            "bundle.load",
            json!({"document": refused_content, "content": content}),
        ),
        "CONTENT_REFUSED",
    );
    assert_eq!(state(&mut s), before, "the open composition is untouched");
    assert_eq!(s.journal.len(), journal, "nothing journaled");
    assert_eq!(undo_depth(&mut s), depth, "no undo step");
}

#[test]
fn bundle_export_stores_a_report_artifact_and_edits_nothing() {
    let mut s = session();
    run(&mut s, "bundle.open", json!({}));
    let depth = undo_depth(&mut s);
    let journal = s.journal.len();
    assert!(s.artifacts.is_empty());

    let exp = run(&mut s, "bundle.export", json!({}));
    let id = exp["artifact"].as_str().expect("an artifact id");
    let a = s.artifact(id).expect("stored in the session");
    assert_eq!(a.title, "Composition document (composition)");
    assert_eq!(a.source.command, "bundle.export");
    assert_eq!(a.data.kind(), "report");
    match &a.data {
        ArtifactData::Report(r) => {
            assert_eq!(r["cid"], exp["cid"]);
            assert_eq!(r["form"], "composition");
            assert_eq!(r["document"], exp["document"]);
            assert_eq!(r["bundle_cid"], exp["bundle_cid"]);
        }
        other => panic!("not a report: {other:?}"),
    }
    // A query: nothing in a macro, no undo step.
    assert_eq!(s.journal.len(), journal);
    assert_eq!(undo_depth(&mut s), depth);

    // Each form is its own artifact.
    let as_bundle = run(&mut s, "bundle.export", json!({"form": "bundle"}));
    assert_ne!(as_bundle["artifact"], exp["artifact"]);
    assert_eq!(
        s.artifact(as_bundle["artifact"].as_str().unwrap())
            .unwrap()
            .title,
        "Composition document (bundle)"
    );
    // Nothing open, nothing to export.
    let mut empty = session();
    assert!(refused(&mut empty, "bundle.export", json!({})).contains("no composition is open"));
    // Unknown forms are named.
    let err = refused(&mut s, "bundle.export", json!({"form": "yaml"}));
    assert!(err.contains("auto, composition, bundle"), "{err}");
}

#[test]
fn bundle_load_replaces_an_open_composition() {
    let mut s = session();
    run(&mut s, "bundle.open", json!({}));
    assert_eq!(
        state(&mut s)["instances"],
        json!(["orders", "workers", "payment"])
    );
    run(&mut s, "bundle.add_instance", json!({"id": "payment2"}));

    let doc = doc(
        json!([workers_inst(), orders_inst()]),
        json!([dequeue_acquire()]),
    );
    let out = run(&mut s, "bundle.load", json!({"document": doc}));
    assert_eq!(out["instances"], json!(["workers", "orders"]));
    let st = state(&mut s);
    assert_eq!(st["instances"], json!(["workers", "orders"]));
    assert_eq!(st["links"].as_array().unwrap().len(), 1);
    assert_eq!(st["document"]["origin"], "document");
    assert_eq!(st["document"]["name"], "t");
    let subnet_ids: Vec<&str> = st["subnets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(subnet_ids, ["workers", "orders"]);
    assert_eq!(st["unavailable"], json!([]));
    // One composition at a time: the other is not hiding anywhere.
    assert!(s.composition.as_ref().unwrap().spec("payment").is_none());

    // And a raw bundle replaces a document, and back.
    run(&mut s, "bundle.load", json!({"document": cafe()}));
    assert_eq!(state(&mut s)["document"]["origin"], "bundle");
    run(&mut s, "bundle.load", json!({"document": doc}));
    assert_eq!(state(&mut s)["document"]["origin"], "document");
}

// ------------------------------------------------------------ content on load

/// A ModelContent document about `small_doc()`: a declared scenario and a note.
fn small_content() -> Value {
    json!({
        "@context": CONTEXT,
        "@type": "ModelContent",
        "name": "small",
        "subject": {"kind": "composition", "name": "small"},
        "entries": [
            {"id": "serve-one", "kind": "scenario", "title": "serve one order", "about": ["model:"],
             "scenario": {"script": ["orders/dequeue"],
                 "expect": [{"place": "orders/items", "op": "==", "n": 1, "says": "one order left"}]}},
            {"id": "queue-note", "kind": "note", "text": "the order queue",
             "about": ["instance:orders", "place:orders/items"]},
        ],
    })
}

#[test]
fn bundle_load_attaches_inline_content() {
    let mut s = session();
    let out = run(
        &mut s,
        "bundle.load",
        json!({"document": small_doc(), "content": small_content()}),
    );
    assert_eq!(out["content"]["source"], "loaded");
    assert_eq!(out["content"]["name"], "small");
    assert_eq!(out["content"]["entries"], 2);
    assert_eq!(out["content"]["kinds"], json!({"scenario": 1, "note": 1}));
    assert_eq!(out["content"]["unresolved"], json!([]));
    assert_eq!(out["content"]["scenarios"], json!(["serve-one"]));
    assert_eq!(
        missing_codes(&out["missing"]),
        ["NO_LAYOUT"],
        "content answers NO_SCENARIOS and NO_CONTENT"
    );

    let st = state(&mut s);
    assert_eq!(st["capabilities"]["scenarios"], 1);
    assert_eq!(st["capabilities"]["content_entries"], 2);

    // The declared scenario is the model's: listed, and runnable on the flat net.
    let sc = run(&mut s, "bundle.scenarios", Value::Null);
    assert_eq!(sc["source"], "loaded");
    assert_eq!(sc["scenarios"][0]["id"], "serve-one");
    let r = run(&mut s, "bundle.run", json!({"scenario": "serve-one"}));
    assert_eq!(r["completed"], true);
    assert_eq!(r["expect"][0]["actual"], 1);
    assert_eq!(r["expect"][0]["pass"], true);
    assert!(refused(&mut s, "bundle.run", json!({"scenario": "commit"}))
        .contains("no scenario \"commit\""));

    // Content is not part of the composition document: the CID is the same with or without it.
    let exp = run(&mut s, "bundle.export", json!({"content": true}));
    let mut bare = session();
    let plain = run(&mut bare, "bundle.load", json!({"document": small_doc()}));
    assert_eq!(exp["cid"], plain["cid"]);
    assert_eq!(exp["content"]["document"]["@type"], "ModelContent");
    assert_eq!(
        exp["content"]["cid"],
        json!(cid::seal(&exp["content"]["document"]).unwrap())
    );
    // A bare composition says so, and exports no content.
    assert_eq!(
        missing_codes(&plain["missing"]),
        ["NO_SCENARIOS", "NO_CONTENT", "NO_LAYOUT"]
    );
    assert_eq!(
        run(&mut bare, "bundle.export", json!({"content": true}))["content"],
        Value::Null
    );
    assert!(
        refused(&mut bare, "bundle.run", json!({"scenario": "serve-one"})).contains("no scenario")
    );
}

#[test]
fn content_with_an_unresolved_id_is_refused_under_strict() {
    let mut bad = small_content();
    bad["entries"][1]["about"] = json!(["instance:orders", "place:orders/nope"]);
    // strict is the default...
    let mut s = session();
    let err = refused(
        &mut s,
        "bundle.load",
        json!({"document": small_doc(), "content": bad.clone()}),
    );
    assert_code(&err, "CONTENT_REFUSED");
    assert!(
        err.contains("[ID_UNRESOLVED]") && err.contains("place:orders/nope"),
        "{err}"
    );
    // ...and says so when asked for.
    let err2 = refused(
        &mut s,
        "bundle.load",
        json!({"document": small_doc(), "content": bad.clone(), "strict": true}),
    );
    assert_eq!(err, err2);
    assert!(s.composition.is_none(), "a refused load opens nothing");

    // Other structural problems are refused under either setting.
    let mut syntax = small_content();
    syntax["entries"][1]["about"] = json!(["wat:orders"]);
    for strict in [true, false] {
        let err = refused(
            &mut s,
            "bundle.load",
            json!({"document": small_doc(), "content": syntax.clone(), "strict": strict}),
        );
        assert_code(&err, "CONTENT_REFUSED");
        assert!(err.contains("[ID_SYNTAX]"), "{err}");
    }
    let mut wrong_type = small_content();
    wrong_type["@type"] = json!("PatternComposition");
    assert_code(
        &refused(
            &mut s,
            "bundle.load",
            json!({"document": small_doc(), "content": wrong_type}),
        ),
        "CONTENT_TYPE",
    );
    let mut plan = small_content();
    plan["subject"]["kind"] = json!("plan");
    assert_code(
        &refused(
            &mut s,
            "bundle.load",
            json!({"document": small_doc(), "content": plan}),
        ),
        "CONTENT_SUBJECT_UNSUPPORTED",
    );
    let err = refused(
        &mut s,
        "bundle.load",
        json!({"document": small_doc(), "library": "no-such-library"}),
    );
    assert!(err.contains("no library content"), "{err}");
}

#[test]
fn content_with_an_unresolved_id_attaches_dormant_when_not_strict() {
    let mut lax = small_content();
    lax["entries"][1]["about"] = json!(["instance:orders", "place:orders/nope"]);
    let mut s = session();
    let out = run(
        &mut s,
        "bundle.load",
        json!({"document": small_doc(), "content": lax, "strict": false}),
    );
    assert_eq!(out["content"]["source"], "loaded");
    assert_eq!(out["content"]["entries"], 2, "the dormant entry is kept");
    assert_eq!(
        out["content"]["unresolved"],
        json!([{"entry": "queue-note", "ref": "place:orders/nope"}])
    );
    // The entries that do resolve work as usual.
    assert_eq!(out["content"]["scenarios"], json!(["serve-one"]));
    assert_eq!(
        run(&mut s, "bundle.run", json!({"scenario": "serve-one"}))["completed"],
        true
    );
    assert_eq!(
        state(&mut s)["content"]["unresolved"],
        out["content"]["unresolved"]
    );
    // The document itself is untouched by it.
    let mut bare = session();
    let plain = run(&mut bare, "bundle.load", json!({"document": small_doc()}));
    assert_eq!(out["cid"], plain["cid"]);
}

#[test]
fn content_attaches_to_a_raw_bundle_too() {
    let content = json!({
        "@context": CONTEXT,
        "@type": "ModelContent",
        "name": "cafe",
        "subject": {"kind": "composition", "name": "cafe"},
        "entries": [
            {"id": "serve-espresso", "kind": "scenario", "about": ["model:"],
             "scenario": {"script": ["counter/order", "counter/start_espresso"],
                 "expect": [{"place": "counter/in_progress", "op": "==", "n": 1}]}},
            {"id": "beans-guard", "kind": "caveat", "text": "an espresso needs two beans",
             "about": ["link:guard:counter:@transition/start_espresso->pantry:beans",
                       "place:pantry/beans"]},
        ],
    });
    let mut s = session();
    let out = run(
        &mut s,
        "bundle.load",
        json!({"document": cafe(), "content": content}),
    );
    assert_eq!(
        out["cid"], CAFE_CID,
        "content does not change the document's CID"
    );
    assert_eq!(out["content"]["unresolved"], json!([]));
    assert_eq!(
        missing_codes(&out["missing"]),
        ["NO_PATTERN", "NO_PORTS", "NO_LAYOUT"],
        "scenarios and content are no longer missing"
    );
    let r = run(&mut s, "bundle.run", json!({"scenario": "serve-espresso"}));
    assert_eq!(r["completed"], true);
    assert_eq!(r["expect"][0]["pass"], true);
    assert_eq!(run(&mut s, "bundle.export", json!({}))["document"], cafe());
}

// ------------------------------------------------------------ library content on reload

#[test]
fn the_orders_document_reloads_with_its_library_content() {
    // A composition that IS a library example gets the library's content back: the content records
    // the CID of the document it was written against, and a document is its own CID.
    let mut a = session();
    run(&mut a, "bundle.open", json!({}));
    let sa = state(&mut a);
    assert_eq!(sa["content"]["source"], "library:orders");
    assert!(sa["content"]["entries"].as_u64().unwrap() > 0);
    assert!(sa["content"]["scenarios"]
        .as_array()
        .unwrap()
        .contains(&json!("refund")));

    let golden = fixture("orders.composition.json");
    let spelled = [
        ("the golden fixture", golden.clone()),
        (
            "the golden fixture's text",
            json!(fixture_text("orders.composition.json")),
        ),
        ("object-form params", with_object_params(&golden)),
        ("instances and links reordered", {
            let mut d = golden.clone();
            d["instances"].as_array_mut().unwrap().reverse();
            d["links"].as_array_mut().unwrap().rotate_left(1);
            d
        }),
    ];
    for (what, document) in spelled {
        let mut b = session();
        let out = run(&mut b, "bundle.load", json!({"document": document}));
        assert_eq!(out["cid"], ORDERS_CID, "{what}");
        assert_eq!(
            out["content"], sa["content"],
            "{what}: same content summary"
        );
        assert_eq!(
            state(&mut b)["capabilities"],
            sa["capabilities"],
            "{what}: same capabilities"
        );
        // Not missing what the content answers: only the layout, as for the catalog opening.
        assert_eq!(
            missing_codes(&out["missing"]),
            missing_codes(&sa["missing"]),
            "{what}"
        );
        assert!(!missing_codes(&out["missing"]).contains(&"NO_CONTENT".to_string()));
        // The declared scenarios run on the reloaded composition as on the original.
        assert_eq!(
            run(&mut b, "bundle.scenarios", Value::Null),
            run(&mut a, "bundle.scenarios", Value::Null),
            "{what}"
        );
        let (rb, ra) = (
            run(&mut b, "bundle.run", json!({"scenario": "refund"})),
            run(&mut a, "bundle.run", json!({"scenario": "refund"})),
        );
        assert_eq!(rb["completed"], true, "{what}");
        assert_eq!(rb["final"], ra["final"], "{what}: the same refund run");
        // Content is not part of the document: the export is the document alone, and asking for
        // the content gets the same content document the original has.
        let (eb, ea) = (
            run(&mut b, "bundle.export", json!({"content": true})),
            run(&mut a, "bundle.export", json!({"content": true})),
        );
        assert_eq!(eb["cid"], ORDERS_CID, "{what}");
        assert_eq!(eb["content"], ea["content"], "{what}");
    }
}

#[test]
fn a_changed_orders_document_is_a_different_cid_and_gets_no_library_content() {
    let golden = fixture("orders.composition.json");
    // One more waiting order: a different document, so the library's content does not claim it.
    let mut changed = with_object_params(&golden);
    changed["instances"][0]["params"]["Initial"] = json!(3);
    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": changed}));
    assert_ne!(out["cid"], ORDERS_CID);
    assert_eq!(out["content"], json!({"source": "none", "entries": 0}));
    assert_eq!(
        missing_codes(&out["missing"]),
        ["NO_SCENARIOS", "NO_CONTENT", "NO_LAYOUT"],
        "a user's composition is bare and says so"
    );
    assert!(refused(&mut s, "bundle.run", json!({"scenario": "refund"})).contains("no scenario"));

    // The same instances without the library: a renamed composition is another document too.
    let mut renamed = golden.clone();
    renamed["name"] = json!("my orders");
    let mut t = session();
    let out = run(&mut t, "bundle.load", json!({"document": renamed}));
    assert_ne!(out["cid"], ORDERS_CID);
    assert_eq!(out["content"]["source"], "none");

    // An explicit `content` argument takes the place of the library's.
    let mut u = session();
    let out = run(
        &mut u,
        "bundle.load",
        json!({"document": golden, "content": {"@context": CONTEXT, "@type": "ModelContent",
            "name": "mine", "subject": {"kind": "composition", "name": "orders"},
            "entries": [{"id": "n", "kind": "note", "text": "mine", "about": ["instance:orders"]}]}}),
    );
    assert_eq!(out["cid"], ORDERS_CID);
    assert_eq!(out["content"]["source"], "loaded");
    assert_eq!(out["content"]["name"], "mine");
    assert_eq!(out["content"]["entries"], 1);
}

// ------------------------------------------------------------ size

#[test]
fn a_document_over_the_size_limit_is_refused_before_it_is_read() {
    use pflow_native_core::composition::document::MAX_DOCUMENT_BYTES;
    let big = "x".repeat(MAX_DOCUMENT_BYTES + 1);

    // As text: refused without parsing it (the text is not even JSON).
    let err = refused(
        &mut session(),
        "bundle.load",
        json!({"document": big.clone()}),
    );
    assert_code(&err, "DOCUMENT_TOO_LARGE");
    assert!(
        err.contains("document") && err.contains(&MAX_DOCUMENT_BYTES.to_string()),
        "names the argument and the limit: {err}"
    );

    // As a value: the size is that of its JSON.
    let mut fat = small_doc();
    fat["description"] = json!(big.clone());
    assert_code(&load_err(&fat), "DOCUMENT_TOO_LARGE");
    let mut fat_bundle = cafe();
    fat_bundle["description"] = json!(big.clone());
    assert_code(&load_err(&fat_bundle), "DOCUMENT_TOO_LARGE");

    // The content argument has the same limit.
    let mut fat_content = small_content();
    fat_content["entries"][1]["text"] = json!(big);
    let err = refused(
        &mut session(),
        "bundle.load",
        json!({"document": small_doc(), "content": fat_content}),
    );
    assert_code(&err, "DOCUMENT_TOO_LARGE");
    assert!(err.contains("content"), "{err}");

    // A refused load leaves the open composition alone; a document under the limit loads.
    let mut s = session();
    run(&mut s, "bundle.open", json!({}));
    let before = state(&mut s);
    let mut fat = small_doc();
    fat["description"] = json!("y".repeat(MAX_DOCUMENT_BYTES + 1));
    assert_code(
        &refused(&mut s, "bundle.load", json!({"document": fat})),
        "DOCUMENT_TOO_LARGE",
    );
    assert_eq!(state(&mut s), before);
    let mut roomy = small_doc();
    roomy["description"] = json!("z".repeat(MAX_DOCUMENT_BYTES / 2));
    run(&mut s, "bundle.load", json!({"document": roomy}));
}

// ------------------------------------------------------------ model identity (src/model_id.rs)

#[test]
fn the_exported_documents_agree_with_the_identity_rule() {
    use pflow_native_core::composition::document::canonical_json as writer_canonical;
    use pflow_native_core::model_id::{canonical_json, check, model_id};

    // A PatternComposition has no part CIDs, so it is its own @type and its CID is its seal.
    let golden = fixture("orders.composition.json");
    check(&golden).expect("the rule passes a composition document unexamined");
    let id = model_id(&golden).unwrap();
    assert_eq!(id.sealed, ORDERS_CID);
    assert_eq!(id.cid, ORDERS_CID, "the model CID is the seal");
    assert!(!id.reduced && id.parts.is_empty());

    // The two canonicalisers agree, so a sealed `params` string is never changed by a consumer
    // that applies the rule to it. Strings with the characters the escapes differ on.
    let mut tricky = payment_inst();
    tricky["params"]["steps"][0]["name"] =
        json!("reserve <stock> & \"hold\" \\ é 日本 \u{1} \u{7f}");
    tricky["params"]["accounts"]["merchant & co"] = json!({"initial": 0, "shared": true});
    tricky["params"]["accounts"]["customer"]["initial"] = json!(12);
    for (what, inst) in [
        ("the golden's saga", payment_inst()),
        ("a saga with awkward strings", tricky),
        ("a queue", orders_inst()),
        ("a pool", workers_inst()),
    ] {
        let mut s = session();
        run(
            &mut s,
            "bundle.load",
            json!({"document": doc(json!([inst]), json!([]))}),
        );
        let exp = run(&mut s, "bundle.export", json!({}));
        let text = exp["document"]["instances"][0]["params"].as_str().unwrap();
        assert_eq!(
            canonical_json(text).unwrap(),
            text,
            "{what}: the identity rule's canonical form is what the writer wrote"
        );
        assert_eq!(
            writer_canonical(&serde_json::from_str(text).unwrap()),
            text,
            "{what}"
        );
        // And it is not re-canonicalised on the way back in: load takes the bytes as the recipe.
        let mut t = session();
        let again = run(&mut t, "bundle.load", json!({"document": exp["document"]}));
        assert_eq!(again["cid"], exp["cid"], "{what}");
        assert_eq!(
            model_id(&exp["document"]).unwrap().sealed,
            exp["cid"],
            "{what}: bundle.export's cid is the rule's `sealed`"
        );
    }

    // The Bundle form of the same composition and the raw cafe bundle: same rule, same seal.
    let mut a = session();
    run(&mut a, "bundle.open", json!({}));
    let as_bundle = run(&mut a, "bundle.export", json!({"form": "bundle"}));
    check(&as_bundle["document"]).expect("the exported Bundle passes the structural check");
    let id = model_id(&as_bundle["document"]).unwrap();
    assert_eq!(id.sealed, ORDERS_BUNDLE_CID);
    assert_eq!(id.sealed, as_bundle["cid"]);
    assert_eq!(id.cid, id.sealed, "three inline parts: not a bare wrapper");
    assert!(id.parts.is_empty());
    let id = model_id(&cafe()).unwrap();
    assert_eq!(id.sealed, CAFE_CID);
    assert_eq!(id.cid, CAFE_CID);
}

#[test]
fn a_recipe_a_build_does_not_interpret_is_kept_and_is_what_the_identity_rule_calls_a_recipe() {
    use pflow_native_core::model_id::{check, model_id};

    // The identity runs' recipe on a subnet: `pattern` and `params` (canonical text). bundle.load
    // keeps both on an unedited round trip without reading them, so the CID is the document's.
    let mut with_recipe = cafe();
    with_recipe["subnets"][0]["pattern"] = json!("workflow");
    with_recipe["subnets"][0]["params"] = json!("{\"places\":3}");
    check(&with_recipe).expect("a recipe is a pattern name and canonical JSON text");
    let want = model_id(&with_recipe).unwrap();
    assert_ne!(
        want.sealed, CAFE_CID,
        "the recipe is part of the sealed document"
    );

    let mut s = session();
    let out = run(&mut s, "bundle.load", json!({"document": with_recipe}));
    assert_eq!(out["cid"], want.sealed.as_str());
    assert_eq!(
        missing_codes(&out["missing"]).last().unwrap(),
        "KEYS_PRESERVED"
    );
    let exp = run(&mut s, "bundle.export", json!({}));
    assert_eq!(exp["document"], with_recipe);
    assert_eq!(exp["cid"], want.sealed.as_str());
    assert_eq!(
        state(&mut s)["subnets"][0]["pattern"],
        "",
        "kept, not interpreted"
    );
}

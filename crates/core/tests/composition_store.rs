//! Storing compositions, bundles and content (docs/composition-documents.md
//! §5): `store.put` / `get` / `list` / `open` / `delete` by kind, the
//! unchanged index entry of a net, the rest of the store treating every kind
//! as a model, and the whole store surviving `dump_blobs` / `replace_all`.
//! Every test gives its thread a fresh memory store, as tests/store.rs does.

use std::sync::Arc;

use pflow_native_core::store::{self, DocKind, Store};
use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

/// The CID of `fixtures/bundle/cafe.bundle.json` as pflow's seal gives it.
const CAFE_CID: &str = "z4EBG9j1SUhuY7XnEcPfHQPzQkmqjUtgrD2wHADw8qdRbpnNt6W";

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn refused(s: &mut Session, id: &str, args: Value) -> String {
    match s.execute(id, args) {
        Ok(v) => panic!("{id} should be refused, got {v}"),
        Err(e) => e.to_string(),
    }
}

/// A session over a fresh in-memory store for this thread.
fn session() -> Session {
    store::set_thread_store(Some(Arc::new(Store::memory())));
    Session::default()
}

/// A second session over the same store: what a new window sees.
fn fresh() -> Session {
    Session::default()
}

fn cafe() -> Value {
    serde_json::from_str(include_str!("fixtures/bundle/cafe.bundle.json")).unwrap()
}

/// Content that resolves against the orders composition (no library content
/// is needed: this is its own small document).
fn orders_content() -> Value {
    json!({
        "@context": "https://pflow.xyz/schema",
        "@type": "ModelContent",
        "name": "orders notes",
        "subject": {"kind": "composition", "name": "orders"},
        "entries": [
            {"id": "bounded", "kind": "note", "order": 1,
             "text": "The orders queue is bounded, so the capacity invariant exists.",
             "about": ["instance:orders"]},
            {"id": "pool", "kind": "caveat", "order": 2,
             "text": "The pool counts workers in use.",
             "about": ["place:workers/in_use"]}
        ]
    })
}

/// The orders composition as a PatternComposition document (the built-in
/// example exports as form (a)).
fn orders_document(s: &mut Session) -> Value {
    run(s, "bundle.open", json!({}));
    run(s, "bundle.export", json!({"form": "composition"}))["document"].clone()
}

/// A session with the orders composition open and `orders_content()`
/// attached.
fn orders_with_content() -> Session {
    let mut s = session();
    let doc = orders_document(&mut s);
    run(
        &mut s,
        "bundle.load",
        json!({"document": doc, "content": orders_content()}),
    );
    s
}

/// `bundle.state` without the parts that say where a composition came from
/// (opened from the catalog, or loaded from a document).
fn shape(state: &Value) -> Value {
    let mut v = state.clone();
    for k in [
        "document",
        "content",
        "capabilities",
        "missing",
        "params_note",
    ] {
        v.as_object_mut().unwrap().remove(k);
    }
    v
}

/// `bundle.flatten` without the id of the artifact it stored, which counts
/// what the session did before.
fn flatten(s: &mut Session) -> Value {
    let mut v = run(s, "bundle.flatten", Value::Null);
    v.as_object_mut().unwrap().remove("artifact");
    v
}

fn cid_of(v: &Value) -> String {
    v["cid"].as_str().unwrap().to_string()
}

fn row<'a>(list: &'a Value, cid: &str) -> &'a Value {
    list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["cid"] == json!(cid))
        .unwrap_or_else(|| panic!("{cid} is not listed in {list}"))
}

#[test]
fn a_nets_index_entry_is_byte_for_byte_what_it_was() {
    let mut s = session();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let put = run(&mut s, "store.put", json!({"title": "SIR"}));
    // The result has the keys it always had, in order.
    let keys: Vec<&String> = put.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["cid", "existed", "title", "parents", "created"]);
    let (cid, created) = (cid_of(&put), put["created"].as_u64().unwrap());
    assert_eq!(cid, "z4EBG9j78kMucqFkb2KMNrXjUrygdVYF3s1V7ueZseEhwRYvmBq");

    let state = run(&mut s, "session.dump_state", json!({"parts": ["store"]}));
    let index = state["store"]["blobs"]["index.json"].as_str().unwrap();
    // No `kind`, no `about`: the entry is what the store wrote before other
    // kinds could be stored.
    let expected = r#"{
  "models": {
    "@CID": {
      "title": "SIR",
      "created": @CREATED,
      "places": 3,
      "transitions": 2
    }
  }
}
"#
    .replace("@CID", &cid)
    .replace("@CREATED", &created.to_string());
    assert_eq!(index, expected);

    // And a net stored by `document` is the same entry as one stored by `model`.
    let doc = run(&mut s, "store.get", json!({"cid": cid}));
    let again = run(&mut s, "store.put", json!({"document": doc}));
    assert_eq!(again["cid"], json!(cid));
    assert_eq!(again["existed"], true);
    assert!(again.get("kind").is_none(), "a net's result has no kind");
    let row = run(&mut s, "store.list", Value::Null)["rows"][0].clone();
    assert_eq!(
        row["kind"], "net",
        "listings say net for an entry with no kind"
    );
    assert_eq!(row["about"], Value::Null);
}

#[test]
fn a_net_typed_by_its_net_type_is_still_a_net() {
    // examples/producer-consumer.json is `@type: "ResourceNet"`: the store
    // has always kept it as a net, by `model`, by `document`, on open and
    // across dump and restore.
    let typed = json!({"@type": "ResourceNet", "places": {}, "transitions": {}});
    assert_eq!(store::kind_of(&typed), DocKind::Net);
    assert_eq!(
        store::kind_of(&json!({"@type": "ResourceNet"})),
        DocKind::Unknown,
        "a type alone is not a net"
    );

    let mut s = session();
    run(
        &mut s,
        "file.open_example",
        json!({"name": "producer-consumer"}),
    );
    assert_eq!(s.doc.to_value()["@type"], "ResourceNet");
    let put = run(&mut s, "store.put", json!({"title": "pc"}));
    assert!(put.get("kind").is_none(), "{put}");
    let doc = run(&mut s, "store.get", json!({"cid": put["cid"]}));
    let again = run(&mut s, "store.put", json!({"document": doc}));
    assert_eq!(again["cid"], put["cid"]);
    assert_eq!(again["existed"], true);
    assert!(again.get("kind").is_none(), "{again}");
    assert_eq!(
        run(&mut s, "store.list", Value::Null)["rows"][0]["kind"],
        "net"
    );

    let mut t = fresh();
    run(&mut t, "store.open", json!({"cid": put["cid"]}));
    assert_eq!(t.doc.to_value()["@type"], "ResourceNet");
    assert_eq!(t.doc.places.len(), s.doc.places.len());

    let blobs = store::open().dump_blobs().unwrap();
    let copy = Store::memory();
    copy.replace_all(&blobs).unwrap();
    assert_eq!(copy.dump_blobs().unwrap(), blobs);
}

#[test]
fn put_get_list_open_delete_of_every_kind() {
    let mut s = orders_with_content();
    let flat = flatten(&mut s);
    let flat_counts = (
        flat["places"].as_array().unwrap().len(),
        flat["transitions"].as_array().unwrap().len(),
    );
    let exported = run(&mut s, "bundle.export", json!({"content": true}));

    // A net, the open composition with its content, and a raw bundle.
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let net = run(&mut s, "store.put", json!({"title": "SIR"}));
    let comp = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true, "title": "Orders"}),
    );
    let bundle = run(&mut s, "store.put", json!({"document": cafe()}));

    assert_eq!(comp["kind"], "composition");
    assert_eq!(comp["existed"], false);
    assert_eq!(comp["title"], "Orders");
    assert_eq!(comp["parents"], json!([]));
    assert_eq!(
        comp["cid"], exported["cid"],
        "the CID is the exported document's"
    );
    assert_eq!(comp["bundle_cid"], exported["bundle_cid"]);
    assert_ne!(comp["bundle_cid"], comp["cid"]);
    assert_eq!(comp["content"]["cid"], exported["content"]["cid"]);
    assert_eq!(comp["content"]["existed"], false);
    assert!(comp["created"].as_u64().unwrap() > 0);

    assert_eq!(bundle["kind"], "bundle");
    assert_eq!(bundle["cid"], CAFE_CID);
    assert_eq!(
        bundle["title"], "cafe-bundle",
        "the title defaults to the name"
    );
    assert!(bundle.get("bundle_cid").is_none());

    // get: sealed with @id = the CID, the document as exported.
    let got = run(&mut s, "store.get", json!({"cid": comp["cid"]}));
    assert_eq!(got["@id"], comp["cid"]);
    let mut doc = got.clone();
    doc.as_object_mut().unwrap().remove("@id");
    assert_eq!(doc, exported["document"]);
    let got = run(&mut s, "store.get", json!({"cid": CAFE_CID}));
    assert_eq!(got["@id"], CAFE_CID);
    let mut doc = got.clone();
    doc.as_object_mut().unwrap().remove("@id");
    assert_eq!(doc, cafe(), "a raw bundle is stored as received");
    let content_cid = comp["content"]["cid"].as_str().unwrap();
    let got = run(&mut s, "store.get", json!({"cid": content_cid}));
    assert_eq!(got["@type"], "ModelContent");
    assert_eq!(got["@id"], json!(content_cid));

    // list: a row per kind, with kind and about.
    let list = run(&mut s, "store.list", Value::Null);
    assert_eq!(list["total"], 4);
    let net_row = row(&list, &cid_of(&net));
    assert_eq!(
        (net_row["kind"].clone(), net_row["places"].clone()),
        (json!("net"), json!(3))
    );
    let comp_row = row(&list, &cid_of(&comp));
    assert_eq!(comp_row["kind"], "composition");
    assert_eq!(comp_row["title"], "Orders");
    assert_eq!(comp_row["places"], flat_counts.0);
    assert_eq!(comp_row["transitions"], flat_counts.1);
    assert_eq!(comp_row["about"], Value::Null);
    let cafe_row = row(&list, CAFE_CID);
    assert_eq!(cafe_row["kind"], "bundle");
    assert!(cafe_row["places"].as_u64().unwrap() > 0, "cafe flattens");
    let content_row = row(&list, content_cid);
    assert_eq!(content_row["kind"], "content");
    assert_eq!(content_row["title"], "orders notes");
    assert_eq!(
        content_row["about"], comp["cid"],
        "content is about its composition"
    );
    assert_eq!(
        (
            content_row["places"].clone(),
            content_row["transitions"].clone()
        ),
        (json!(0), json!(0))
    );
    // The table says so too.
    let art = s.artifact(list["artifact"].as_str().unwrap()).unwrap();
    let ArtifactData::Table { columns, rows } = &art.data else {
        panic!("a table")
    };
    let col = |name: &str| columns.iter().position(|c| c == name).unwrap();
    let r = rows
        .iter()
        .find(|r| r[col("cid")] == json!(content_cid))
        .unwrap();
    assert_eq!(r[col("kind")], "content");
    assert_eq!(r[col("about")], comp["cid"]);

    // delete each kind; the others stay.
    for (n, cid) in [content_cid.to_string(), CAFE_CID.to_string(), cid_of(&comp)]
        .iter()
        .enumerate()
    {
        let del = run(&mut s, "store.delete", json!({"cid": cid}));
        assert_eq!(del["deleted"], json!(cid));
        assert_eq!(run(&mut s, "store.list", Value::Null)["total"], 3 - n);
        assert!(refused(&mut s, "store.get", json!({"cid": cid})).contains("no model"));
    }
    assert_eq!(
        run(&mut s, "store.list", Value::Null)["rows"][0]["cid"],
        net["cid"]
    );
}

#[test]
fn storing_is_idempotent_by_cid_and_the_first_metadata_stands() {
    let mut s = orders_with_content();
    let first = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true, "title": "Orders"}),
    );
    let again = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true, "title": "Other"}),
    );
    assert_eq!(again["cid"], first["cid"]);
    assert_eq!(again["existed"], true);
    assert_eq!(again["title"], "Orders");
    assert_eq!(again["created"], first["created"]);
    assert_eq!(again["content"]["cid"], first["content"]["cid"]);
    assert_eq!(again["content"]["existed"], true);

    // The same document by another road is the same entry: `document`,
    // with or without the @id the store added.
    let got = run(&mut s, "store.get", json!({"cid": first["cid"]}));
    let by_doc = run(&mut s, "store.put", json!({"document": got}));
    assert_eq!(by_doc["cid"], first["cid"]);
    assert_eq!(by_doc["existed"], true);
    let exported = run(&mut s, "bundle.export", json!({"form": "composition"}))["document"].clone();
    let by_text = run(
        &mut s,
        "store.put",
        json!({"document": exported.to_string()}),
    );
    assert_eq!(by_text["cid"], first["cid"]);
    assert_eq!(by_text["existed"], true);

    // A raw bundle: by `document`, then as the open composition.
    let one = run(
        &mut s,
        "store.put",
        json!({"document": cafe(), "title": "Cafe"}),
    );
    run(&mut s, "bundle.load", json!({"document": cafe()}));
    let two = run(&mut s, "store.put", json!({"composition": true}));
    assert_eq!(
        (one["cid"].clone(), two["cid"].clone()),
        (json!(CAFE_CID), json!(CAFE_CID))
    );
    assert_eq!(two["existed"], true);
    assert_eq!(two["title"], "Cafe", "the first title stands");
    assert_eq!(two["kind"], "bundle");
    assert_eq!(run(&mut s, "store.list", Value::Null)["total"], 3);

    // Parents are recorded once; a cycle is not recorded.
    let parent = cid_of(&one);
    let child = run(
        &mut s,
        "store.put",
        json!({"document": orders_content(), "parents": [parent]}),
    );
    assert_eq!(child["parents"], json!([parent]));
    let back = run(
        &mut s,
        "store.put",
        json!({"document": cafe(), "parents": [child["cid"]]}),
    );
    assert_eq!(back["existed"], true);
    assert_eq!(back["parents"], json!([]));
    assert!(back["notes"][0].as_str().unwrap().contains("close a cycle"));
}

#[test]
fn a_stored_composition_opens_in_a_fresh_session_identically() {
    let mut s = session();
    run(&mut s, "bundle.open", json!({}));
    let before = run(&mut s, "bundle.state", Value::Null);
    let flat = flatten(&mut s);
    let put = run(&mut s, "store.put", json!({"composition": true}));
    assert_eq!(put["kind"], "composition");

    let mut t = fresh();
    assert!(t.composition.is_none());
    let opened = run(&mut t, "store.open", json!({"cid": put["cid"]}));
    assert_eq!(opened["kind"], "composition");
    assert_eq!(opened["form"], "composition");
    assert_eq!(opened["cid"], put["cid"]);
    assert_eq!(opened["flatten"], json!({"ok": true}));
    assert_eq!(flatten(&mut t), flat);
    let after = run(&mut t, "bundle.state", Value::Null);
    assert_eq!(shape(&after), shape(&before));
    assert_eq!(after["document"]["origin"], "document");

    // It is what bundle.load of the same document gives, key for key.
    let doc = run(&mut s, "bundle.export", json!({"form": "composition"}))["document"].clone();
    let mut u = fresh();
    run(&mut u, "bundle.load", json!({"document": doc}));
    assert_eq!(run(&mut u, "bundle.state", Value::Null), after);

    // Re-exporting gives the document that was stored, and its CID.
    let again = run(&mut t, "bundle.export", json!({"form": "composition"}));
    assert_eq!(again["cid"], put["cid"]);

    // bundle.load {cid} reads the same stored document (it has an @id the
    // loaders would refuse; the store hands them the document as sealed).
    let mut v = fresh();
    let loaded = run(&mut v, "bundle.load", json!({"cid": put["cid"]}));
    assert_eq!(loaded["cid"], put["cid"]);
    assert_eq!(run(&mut v, "bundle.state", Value::Null), after);

    // Opening is one undo step.
    run(&mut t, "edit.undo", Value::Null);
    assert!(t.composition.is_none());
}

#[test]
fn a_stored_raw_bundle_opens_as_the_bytes_received() {
    let mut s = session();
    let put = run(&mut s, "store.put", json!({"document": cafe()}));
    assert_eq!(put["cid"], CAFE_CID);

    let mut t = fresh();
    let opened = run(&mut t, "store.open", json!({"cid": CAFE_CID}));
    assert_eq!(opened["kind"], "bundle");
    assert_eq!(opened["form"], "bundle");
    assert_eq!(opened["cid"], CAFE_CID);
    let out = run(&mut t, "bundle.export", Value::Null);
    assert_eq!(out["form"], "bundle");
    assert_eq!(
        out["document"],
        cafe(),
        "unedited, the export is the loaded JSON"
    );
    assert_eq!(out["cid"], CAFE_CID);

    // What a load from the document gives, key for key.
    let mut u = fresh();
    run(&mut u, "bundle.load", json!({"document": cafe()}));
    assert_eq!(
        run(&mut u, "bundle.state", Value::Null),
        run(&mut t, "bundle.state", Value::Null)
    );
    assert_eq!(flatten(&mut u), flatten(&mut t));

    // Storing the composition form of an open bundle (form: composition) is
    // refused as bundle.export refuses it; form: bundle is the same entry.
    let e = refused(
        &mut t,
        "store.put",
        json!({"composition": true, "form": "composition"}),
    );
    assert!(e.contains("[EXPORT_NEEDS_PATTERNS]"), "{e}");
    let same = run(
        &mut t,
        "store.put",
        json!({"composition": true, "form": "bundle"}),
    );
    assert_eq!(
        (same["cid"].clone(), same["existed"].clone()),
        (json!(CAFE_CID), json!(true))
    );
}

#[test]
fn a_composition_stored_as_a_bundle_is_a_bundle_entry() {
    let mut s = session();
    run(&mut s, "bundle.open", json!({}));
    let as_bundle = run(
        &mut s,
        "store.put",
        json!({"composition": true, "form": "bundle"}),
    );
    let as_comp = run(
        &mut s,
        "store.put",
        json!({"composition": true, "form": "composition"}),
    );
    assert_eq!(as_bundle["kind"], "bundle");
    assert_eq!(as_comp["kind"], "composition");
    assert_ne!(as_bundle["cid"], as_comp["cid"]);
    assert_eq!(
        as_bundle["cid"], as_comp["bundle_cid"],
        "the bundle the composition flattens to"
    );

    let mut t = fresh();
    run(&mut t, "store.open", json!({"cid": as_bundle["cid"]}));
    assert_eq!(
        flatten(&mut t),
        flatten(&mut s),
        "the same flat net by either form"
    );
}

#[test]
fn content_is_about_its_composition() {
    let mut s = orders_with_content();
    // Alone, before the composition is stored: about is the open
    // composition's CID, and the result says it is not stored yet.
    let comp_cid = cid_of(&run(&mut s, "bundle.export", Value::Null));
    let alone = run(&mut s, "store.put", json!({"content": true}));
    assert_eq!(alone["kind"], "content");
    assert_eq!(alone["about"], json!(comp_cid));
    assert!(
        alone["note"]
            .as_str()
            .unwrap()
            .contains("not in the local store"),
        "{alone}"
    );
    assert_eq!(alone["title"], "orders notes");
    assert!(alone.get("content").is_none());
    let row0 = row(&run(&mut s, "store.list", Value::Null), &cid_of(&alone)).clone();
    assert_eq!(row0["about"], json!(comp_cid));

    // With the composition: the same content entry, now with its composition
    // stored and no note.
    let both = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    assert_eq!(both["cid"], json!(comp_cid));
    assert_eq!(both["content"]["cid"], alone["cid"]);
    assert_eq!(both["content"]["existed"], true);
    let again = run(&mut s, "store.put", json!({"content": true}));
    assert_eq!(again["existed"], true);
    assert!(again.get("note").is_none(), "{again}");

    // Through `document`, about is the content's own `composition`.
    let mut doc = orders_content();
    doc["name"] = json!("with a composition");
    doc["composition"] = json!(comp_cid);
    let given = run(&mut s, "store.put", json!({"document": doc}));
    assert_eq!(given["kind"], "content");
    assert_eq!(given["about"], json!(comp_cid));
    assert!(given.get("note").is_none());
    let stranger = "z4EBG9j78kMucqFkb2KMNrXjUrygdVYF3s1V7ueZseEhwRYvmBq";
    doc["name"] = json!("about a net");
    doc["composition"] = json!(stranger);
    let odd = run(&mut s, "store.put", json!({"document": doc}));
    assert_eq!(odd["about"], json!(stranger), "any model CID is accepted");
    assert!(odd["note"]
        .as_str()
        .unwrap()
        .contains("not in the local store"));
    doc["name"] = json!("placeholder");
    doc["composition"] = json!("z…");
    let placeholder = run(&mut s, "store.put", json!({"document": doc}));
    assert!(placeholder.get("about").is_none());
    assert!(placeholder["note"]
        .as_str()
        .unwrap()
        .contains("not a model CID"));
    // A content document with no composition is about nothing.
    let bare = run(&mut s, "store.put", json!({"document": orders_content()}));
    assert_eq!(
        bare["cid"], alone["cid"],
        "the same document is the same entry"
    );
    assert_eq!(
        bare["about"],
        json!(comp_cid),
        "and the first record of about stands"
    );

    // The listing can find what is about a composition.
    let list = run(&mut s, "store.list", json!({"kind": "content"}));
    assert_eq!(list["total"], 4);
}

#[test]
fn stored_content_attaches_to_the_open_composition() {
    let mut s = orders_with_content();
    let put = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    let content = put["content"]["cid"].clone();

    // Nothing open: refused clearly.
    let mut t = fresh();
    let e = refused(&mut t, "store.open", json!({"cid": content}));
    assert!(
        e.contains("[NO_COMPOSITION]") && e.contains("no composition is open"),
        "{e}"
    );

    // The composition, then the content: one undo step, strictly. The
    // stored composition is the orders example, so loading it brings the
    // library's content back (it records this CID); the stored content
    // replaces that.
    let mut t = fresh();
    run(&mut t, "store.open", json!({"cid": put["cid"]}));
    let before = run(&mut t, "bundle.state", Value::Null)["content"].clone();
    assert_eq!(before["source"], "library:orders");
    assert!(before["entries"].as_u64().unwrap() > 2);
    let opened = run(&mut t, "store.open", json!({"cid": content}));
    assert_eq!(opened["kind"], "content");
    assert_eq!(opened["about"], put["cid"]);
    assert_eq!(opened["content"]["entries"], 2);
    assert_eq!(opened["content"]["source"], "loaded");
    assert!(opened.get("note").is_none(), "{opened}");
    let state = run(&mut t, "bundle.state", Value::Null);
    assert_eq!(state["content"], opened["content"]);
    assert_eq!(
        run(&mut t, "bundle.export", json!({"content": true}))["content"]["cid"],
        content,
        "the attached content is the stored document"
    );
    run(&mut t, "edit.undo", Value::Null);
    assert_eq!(
        run(&mut t, "bundle.state", Value::Null)["content"],
        before,
        "undo brings back the content the composition had"
    );

    // Content whose ids do not resolve is refused, and nothing changes.
    let mut u = fresh();
    run(&mut u, "store.put", json!({"document": cafe()}));
    run(&mut u, "store.open", json!({"cid": CAFE_CID}));
    let e = refused(&mut u, "store.open", json!({"cid": content}));
    assert!(e.contains("[CONTENT_REFUSED]"), "{e}");
    assert_eq!(
        run(&mut u, "bundle.state", Value::Null)["content"]["entries"],
        0
    );

    // Attached to another composition than the one it is about: a note.
    let mut w = fresh();
    run(&mut w, "bundle.open", json!({}));
    run(&mut w, "bundle.clear_links", Value::Null);
    let elsewhere = run(&mut w, "store.open", json!({"cid": content}));
    assert_eq!(elsewhere["content"]["entries"], 2);
    let note = elsewhere["note"].as_str().unwrap();
    assert!(note.contains(put["cid"].as_str().unwrap()), "{note}");
}

#[test]
fn list_filters_by_kind() {
    let mut s = orders_with_content();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(&mut s, "store.put", json!({"title": "SIR"}));
    run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    run(&mut s, "store.put", json!({"document": cafe()}));
    for (kind, want) in [
        ("net", 1),
        ("composition", 1),
        ("bundle", 1),
        ("content", 1),
    ] {
        let l = run(&mut s, "store.list", json!({"kind": kind}));
        assert_eq!(l["total"], want, "{kind}");
        assert!(l["rows"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["kind"] == kind));
    }
    assert_eq!(run(&mut s, "store.list", Value::Null)["total"], 4);
    assert_eq!(run(&mut s, "store.list", json!({"kind": ""}))["total"], 4);
    // Filters compose.
    assert_eq!(
        run(
            &mut s,
            "store.list",
            json!({"kind": "bundle", "query": "cafe"})
        )["total"],
        1
    );
    assert_eq!(
        run(
            &mut s,
            "store.list",
            json!({"kind": "net", "query": "cafe"})
        )["total"],
        0
    );
    let e = refused(&mut s, "store.list", json!({"kind": "model"}));
    assert!(e.contains("net, bundle, composition, content"), "{e}");
}

#[test]
fn what_cannot_be_stored_is_refused_and_writes_nothing() {
    let mut s = session();
    let e = refused(
        &mut s,
        "store.put",
        json!({"document": {"@type": "Widget"}}),
    );
    assert!(
        e.contains("[DOCUMENT_KIND]") && e.contains("\"Widget\""),
        "{e}"
    );
    let e = refused(
        &mut s,
        "store.put",
        json!({"document": {"name": "no type"}}),
    );
    assert!(
        e.contains("[DOCUMENT_KIND]") && e.contains("no @type"),
        "{e}"
    );
    let e = refused(&mut s, "store.put", json!({"document": [1, 2]}));
    assert!(e.contains("[DOCUMENT_KIND]"), "{e}");
    let e = refused(&mut s, "store.put", json!({"document": "{not json"}));
    assert!(e.contains("not valid JSON"), "{e}");

    // Arguments that do not go together.
    let e = refused(
        &mut s,
        "store.put",
        json!({"document": cafe(), "composition": true}),
    );
    assert!(e.contains("document is the whole request"), "{e}");
    let e = refused(
        &mut s,
        "store.put",
        json!({"document": cafe(), "model": "x"}),
    );
    assert!(e.contains("document is the whole request"), "{e}");
    let e = refused(
        &mut s,
        "store.put",
        json!({"composition": true, "model": "x"}),
    );
    assert!(e.contains("model names a net"), "{e}");
    let e = refused(&mut s, "store.put", json!({"form": "bundle"}));
    assert!(e.contains("pass it with composition"), "{e}");
    let e = refused(
        &mut s,
        "store.put",
        json!({"content": true, "form": "bundle"}),
    );
    assert!(e.contains("pass it with composition"), "{e}");

    // No composition open.
    for args in [json!({"composition": true}), json!({"content": true})] {
        let e = refused(&mut s, "store.put", args);
        assert!(e.contains("no composition is open"), "{e}");
    }
    // A composition without content: asking for the content refuses all of
    // it. (The orders example itself loads with the library's content, which
    // records its CID; a renamed copy is a user's composition and is bare.)
    let mut doc = orders_document(&mut s);
    doc["name"] = json!("my orders");
    run(&mut s, "bundle.load", json!({"document": doc}));
    assert_eq!(
        run(&mut s, "bundle.state", Value::Null)["content"]["source"],
        "none"
    );
    let e = refused(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    assert!(e.contains("[NO_CONTENT]"), "{e}");
    let e = refused(
        &mut s,
        "store.put",
        json!({"composition": true, "form": "sideways"}),
    );
    assert!(e.contains("auto, composition, bundle"), "{e}");
    assert_eq!(
        run(&mut s, "store.list", Value::Null)["total"],
        0,
        "nothing was written"
    );

    // Opening what is not there, or not a kind.
    let e = refused(
        &mut s,
        "store.open",
        json!({"cid": "z4EBG9j78kMucqFkb2KMNrXjUrygdVYF3s1V7ueZseEhwRYvmBq"}),
    );
    assert!(e.contains("no model"), "{e}");
}

#[test]
fn the_store_checks_kind_and_about() {
    let st = Store::memory();
    let doc = cafe();
    let mismatch = |kind: DocKind| {
        st.put_document(&doc, kind, None, None, None, (0, 0))
            .unwrap_err()
            .to_string()
    };
    assert!(mismatch(DocKind::Composition).contains("[DOCUMENT_KIND]"));
    assert!(mismatch(DocKind::Net).contains("stored as net, but the document is bundle"));
    assert!(mismatch(DocKind::Unknown).contains("[DOCUMENT_KIND]"));
    assert!(st
        .put_document(
            &json!({"@type": "Widget"}),
            DocKind::Unknown,
            None,
            None,
            None,
            (0, 0)
        )
        .unwrap_err()
        .to_string()
        .contains("[DOCUMENT_KIND]"));
    // `about` belongs to content, and is a model CID.
    let e = st
        .put_document(&doc, DocKind::Bundle, None, None, Some(CAFE_CID), (0, 0))
        .unwrap_err()
        .to_string();
    assert!(e.contains("a bundle has none"), "{e}");
    let e = st
        .put_document(
            &orders_content(),
            DocKind::Content,
            None,
            None,
            Some("nope"),
            (0, 0),
        )
        .unwrap_err()
        .to_string();
    assert!(e.contains("not a model CID"), "{e}");
    assert!(st.snapshot().unwrap().models.is_empty());

    // The counts are the index's.
    let put = st
        .put_document(&doc, DocKind::Bundle, Some("Cafe"), None, None, (7, 5))
        .unwrap();
    assert_eq!(put["cid"], CAFE_CID);
    let meta = st.model_meta(CAFE_CID).unwrap().unwrap();
    assert_eq!((meta.places, meta.transitions), (7, 5));
    assert_eq!(meta.kind.as_deref(), Some("bundle"));
    assert_eq!(meta.about, None);
    // get_sealed is the document as put; get_model has the @id.
    assert_eq!(st.get_sealed(CAFE_CID).unwrap(), doc);
    assert_eq!(st.get_model(CAFE_CID).unwrap()["@id"], CAFE_CID);
}

#[test]
fn a_document_that_will_not_load_or_is_not_canonical_says_so() {
    let mut s = orders_with_content();
    let canonical =
        run(&mut s, "bundle.export", json!({"form": "composition"}))["document"].clone();

    // The prototype's shape: params as objects. It loads, and is stored as
    // received, under a CID other than the canonical document's.
    let mut loose = canonical.clone();
    for inst in loose["instances"].as_array_mut().unwrap() {
        let p: Value = serde_json::from_str(inst["params"].as_str().unwrap()).unwrap();
        inst["params"] = p;
    }
    let put = run(&mut s, "store.put", json!({"document": loose}));
    assert_eq!(put["kind"], "composition");
    assert_ne!(
        put["cid"],
        json!(cid_of(&run(&mut s, "bundle.export", Value::Null)))
    );
    assert!(
        put["note"]
            .as_str()
            .unwrap()
            .contains("not the form bundle.export writes"),
        "{put}"
    );
    assert!(put["bundle_cid"].as_str().unwrap().starts_with('z'));
    let want = run(&mut s, "bundle.export", json!({"form": "composition"}));
    assert!(put["note"]
        .as_str()
        .unwrap()
        .contains(want["cid"].as_str().unwrap()));
    assert!(
        row(&run(&mut s, "store.list", Value::Null), &cid_of(&put))["places"]
            .as_u64()
            .unwrap()
            > 0
    );

    // One that does not load is still a document of a known kind: stored
    // with no counts, and opening it is refused with the loader's reason.
    let mut broken = canonical.clone();
    broken["instances"][0]["pattern"] = json!("stack");
    let put = run(&mut s, "store.put", json!({"document": broken}));
    assert_eq!(put["kind"], "composition");
    let note = put["note"].as_str().unwrap();
    assert!(
        note.contains("does not load") && note.contains("[PATTERN_UNKNOWN]"),
        "{note}"
    );
    let r = row(&run(&mut s, "store.list", Value::Null), &cid_of(&put)).clone();
    assert_eq!(
        (r["places"].clone(), r["transitions"].clone()),
        (json!(0), json!(0))
    );
    let mut t = fresh();
    let e = refused(&mut t, "store.open", json!({"cid": put["cid"]}));
    assert!(e.contains("[PATTERN_UNKNOWN]"), "{e}");
    assert!(t.composition.is_none());
}

#[test]
fn every_kind_is_a_model_to_the_rest_of_the_store() {
    let mut s = orders_with_content();
    let a = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true, "title": "Orders"}),
    );
    let (a_cid, c_cid) = (
        cid_of(&a),
        a["content"]["cid"].as_str().unwrap().to_string(),
    );
    let b_cid = cid_of(&run(&mut s, "store.put", json!({"document": cafe()})));
    // A revised composition: the same instances without their links.
    run(&mut s, "bundle.clear_links", Value::Null);
    let a2 = run(
        &mut s,
        "store.put",
        json!({"composition": true, "title": "Orders v2"}),
    );
    let a2_cid = cid_of(&a2);
    assert_ne!(a2_cid, a_cid);

    // link / edges / neighbors, across kinds (a content document is a model).
    let l1 = run(
        &mut s,
        "store.link",
        json!({"subject": a_cid, "predicate": "related", "object": b_cid}),
    );
    assert_eq!(l1["existing"], false);
    let l2 = run(
        &mut s,
        "store.link",
        json!({"subject": c_cid, "predicate": "related", "object": a_cid}),
    );
    let again = run(
        &mut s,
        "store.link",
        json!({"subject": a_cid, "predicate": "related", "object": b_cid}),
    );
    assert_eq!(
        (again["existing"].clone(), again["id"].clone()),
        (json!(true), l1["id"].clone())
    );
    let edges = run(&mut s, "store.edges", json!({"id": a_cid}));
    assert_eq!(edges["edges"].as_array().unwrap().len(), 2);
    // `related` is symmetric: both directions count.
    let near = |s: &mut Session, id: &str| {
        let mut v: Vec<String> = run(
            s,
            "store.neighbors",
            json!({"id": id, "predicate": "related"}),
        )
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
        v.sort();
        v
    };
    let mut both = vec![b_cid.clone(), c_cid.clone()];
    both.sort();
    assert_eq!(near(&mut s, &a_cid), both);
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a_cid, "predicate": "related", "object": "z4EBG9j78kMucqFkb2KMNrXjUrygdVYF3s1V7ueZseEhwRYvmBq"}),
    );
    assert!(e.contains("not a stored entity"), "{e}");
    let graph = run(&mut s, "store.graph", json!({"around": a_cid, "depth": 1}));
    assert_eq!(graph["nodes"], 3);

    // collections
    run(
        &mut s,
        "store.collection_create",
        json!({"name": "orders docs"}),
    );
    for cid in [&a_cid, &b_cid, &c_cid] {
        run(
            &mut s,
            "store.collection_add",
            json!({"collection": "orders docs", "cid": cid}),
        );
    }
    let cols = run(&mut s, "store.collection_list", Value::Null);
    assert_eq!(cols["collections"][0]["members"], 3);
    let list = run(&mut s, "store.list", Value::Null);
    assert_eq!(row(&list, &c_cid)["collections"], json!(["orders docs"]));

    // delete refuses a model an edge names, unless forced; a license is final.
    let e = refused(&mut s, "store.delete", json!({"cid": b_cid}));
    assert!(e.contains("edge(s)"), "{e}");
    let lic = run(
        &mut s,
        "store.license",
        json!({"cid": a_cid, "license": "CC0-1.0"}),
    );
    assert_eq!(lic["license"], "CC0-1.0");
    let e = refused(
        &mut s,
        "store.license",
        json!({"cid": a_cid, "license": "CC0-1.0"}),
    );
    assert!(e.contains("irrevocable"), "{e}");
    let e = refused(&mut s, "store.delete", json!({"cid": a_cid, "force": true}));
    assert!(e.contains("dedicated to the commons"), "{e}");
    assert_eq!(
        row(&run(&mut s, "store.list", Value::Null), &a_cid)["license"],
        "CC0-1.0"
    );

    // supersede and migration offers: the edge from the old composition is
    // offered for the new one, and accepting it writes it there.
    let sup = run(
        &mut s,
        "store.supersede",
        json!({"old": a_cid, "new": a2_cid}),
    );
    assert_eq!(sup["chain"]["status"], "complete");
    assert_eq!(sup["chain"]["current"], json!(a2_cid));
    let list = run(&mut s, "store.list", Value::Null);
    assert_eq!(row(&list, &a_cid)["superseded_by"], json!(a2_cid));
    let offers = run(
        &mut s,
        "store.migration_offers",
        json!({"cid": a_cid, "predicate": "related"}),
    );
    assert_eq!(offers["current"], json!(a2_cid));
    let offered: Vec<&Value> = offers["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["disposition"] == "offered")
        .collect();
    assert_eq!(
        offered.len(),
        2,
        "both related edges touch the old composition"
    );
    assert!(offered.iter().all(|e| e["check"]["result"] == "passes"));
    // Every pending offer: the two related edges and the collection's
    // membership of the old composition.
    let done = run(&mut s, "store.accept_migration", json!({"old": a_cid}));
    assert_eq!(done["migrated"].as_array().unwrap().len(), 3);
    assert_eq!(
        near(&mut s, &a2_cid),
        both,
        "the edges now hold on the successor"
    );
    let cols = run(&mut s, "store.collection_list", Value::Null);
    assert_eq!(
        cols["collections"][0]["members"], 4,
        "the successor joined the collection"
    );
    assert_ne!(l2["id"], l1["id"]);

    // A content document can be superseded by another, too.
    let c2 = run(
        &mut s,
        "store.put",
        json!({"document": json!({
        "@context": "https://pflow.xyz/schema", "@type": "ModelContent", "name": "v2",
        "subject": {"kind": "composition", "name": "orders"}, "entries": []})}),
    );
    let sup = run(
        &mut s,
        "store.supersede",
        json!({"old": c_cid, "new": c2["cid"]}),
    );
    assert_eq!(sup["chain"]["status"], "complete");

    // After all that, the store is still consistent: dump and restore.
    let blobs = store::open().dump_blobs().unwrap();
    let copy = Store::memory();
    copy.replace_all(&blobs).unwrap();
    assert_eq!(copy.dump_blobs().unwrap(), blobs);
}

#[test]
fn dump_and_replace_all_round_trip_every_kind() {
    let mut s = orders_with_content();
    let flat = flatten(&mut s);
    let state = run(&mut s, "bundle.state", Value::Null);
    let put = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let net = run(&mut s, "store.put", json!({"title": "SIR"}));
    run(&mut s, "store.put", json!({"document": cafe()}));
    let list = run(&mut s, "store.list", Value::Null);
    let blobs = store::open().dump_blobs().unwrap();

    // The layout is the same for every kind: no new key.
    let keys: Vec<&str> = blobs.keys().map(String::as_str).collect();
    let models: Vec<&String> = blobs.keys().filter(|k| k.starts_with("models/")).collect();
    assert_eq!(models.len(), 4);
    assert!(models.iter().all(|k| k.ends_with(".jsonld")));
    assert!(keys.contains(&"index.json"));
    assert_eq!(keys.len(), 5, "index.json and four documents: {keys:?}");
    let index: Value = serde_json::from_str(&blobs["index.json"]).unwrap();
    assert_eq!(
        index["models"][put["cid"].as_str().unwrap()]["kind"],
        "composition"
    );
    assert_eq!(
        index["models"][put["content"]["cid"].as_str().unwrap()]["about"],
        put["cid"]
    );
    assert!(index["models"][net["cid"].as_str().unwrap()]
        .get("kind")
        .is_none());

    // Into a fresh store, and the commands see the same thing.
    let copy = Arc::new(Store::memory());
    assert_eq!(copy.replace_all(&blobs).unwrap(), blobs.len());
    assert_eq!(copy.dump_blobs().unwrap(), blobs);
    store::set_thread_store(Some(copy));
    let mut t = Session::default();
    let relisted = run(&mut t, "store.list", Value::Null);
    assert_eq!(relisted["rows"], list["rows"]);
    run(&mut t, "store.open", json!({"cid": put["cid"]}));
    assert_eq!(
        shape(&run(&mut t, "bundle.state", Value::Null)),
        shape(&state)
    );
    assert_eq!(flatten(&mut t), flat);
    let attached = run(&mut t, "store.open", json!({"cid": put["content"]["cid"]}));
    assert_eq!(attached["content"]["entries"], 2);
    let mut u = fresh();
    run(&mut u, "store.open", json!({"cid": CAFE_CID}));
    assert_eq!(
        run(&mut u, "bundle.export", Value::Null)["document"],
        cafe()
    );
}

#[test]
fn a_saved_state_carries_every_kind() {
    let mut s = orders_with_content();
    let flat = flatten(&mut s);
    let put = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    run(&mut s, "store.put", json!({"document": cafe()}));
    let dumped = run(&mut s, "session.dump_state", Value::Null);

    // The page's way: a new engine restores the whole state, memory store and all.
    let mut t = session();
    let out = run(&mut t, "session.restore_state", json!({"state": dumped}));
    assert_eq!(out["store"], 4, "index.json and three documents");
    assert_eq!(run(&mut t, "store.list", Value::Null)["total"], 3);
    let opened = run(&mut t, "store.open", json!({"cid": put["cid"]}));
    assert_eq!(opened["cid"], put["cid"]);
    assert_eq!(flatten(&mut t), flat);
    assert_eq!(
        run(&mut t, "session.dump_state", json!({"parts": ["store"]}))["store"],
        dumped["store"]
    );
}

#[test]
fn a_restored_state_must_hold_documents_of_the_kinds_the_index_says() {
    let mut s = orders_with_content();
    let put = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    let (comp, content) = (
        cid_of(&put),
        put["content"]["cid"].as_str().unwrap().to_string(),
    );
    let state = run(&mut s, "session.dump_state", json!({"parts": ["store"]}));
    let with = |f: &dyn Fn(&mut Value)| {
        let mut v = state.clone();
        f(&mut v);
        json!({"state": v})
    };
    let index_text = state["store"]["blobs"]["index.json"]
        .as_str()
        .unwrap()
        .to_string();
    let strange = json!({"@context": "https://pflow.xyz/schema", "@type": "Widget", "name": "w"});
    let strange_cid = pflow_native_core::cid::seal(&strange).unwrap();
    let cases = [
        (
            // The index calls the composition a bundle.
            with(&|v| {
                let t = index_text.replacen("\"kind\": \"composition\"", "\"kind\": \"bundle\"", 1);
                v["store"]["blobs"]["index.json"] = json!(t);
            }),
            "as a bundle, but",
        ),
        (
            // The index forgets that the content is content (so: a net).
            with(&|v| {
                let t = index_text.replace("\"kind\": \"content\",", "");
                v["store"]["blobs"]["index.json"] = json!(t);
            }),
            "as a net, but",
        ),
        (
            // A document of no kind the store keeps, under its own CID.
            with(&|v| {
                v["store"]["blobs"][format!("models/{strange_cid}.jsonld")] =
                    json!(strange.to_string());
            }),
            "no kind the store keeps",
        ),
        (
            // A composition under another document's CID.
            with(&|v| {
                let other = v["store"]["blobs"][format!("models/{content}.jsonld")].clone();
                v["store"]["blobs"][format!("models/{comp}.jsonld")] = other;
            }),
            "not the z",
        ),
    ];
    for (args, want) in cases {
        let e = refused(&mut s, "session.restore_state", args);
        assert!(e.contains(want), "{want:?} not in {e:?}");
        assert!(e.contains("store"), "{e}");
        assert_eq!(
            run(&mut s, "session.dump_state", json!({"parts": ["store"]})),
            state,
            "a refused state changes nothing"
        );
    }
}

#[test]
fn a_filesystem_store_keeps_every_kind_across_handles() {
    let dir = std::env::temp_dir().join(format!(
        "pflow-native-composition-store-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&dir).ok();
    store::set_thread_store(Some(Arc::new(Store::filesystem(&dir))));
    let mut s = Session::default();
    let doc = orders_document(&mut s);
    run(
        &mut s,
        "bundle.load",
        json!({"document": doc, "content": orders_content()}),
    );
    let put = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    run(&mut s, "store.put", json!({"document": cafe()}));

    // A new handle on the same directory sees them.
    store::set_thread_store(Some(Arc::new(Store::filesystem(&dir))));
    let mut t = Session::default();
    assert_eq!(run(&mut t, "store.list", Value::Null)["total"], 3);
    run(&mut t, "store.open", json!({"cid": put["cid"]}));
    run(&mut t, "store.open", json!({"cid": put["content"]["cid"]}));
    assert_eq!(
        run(&mut t, "bundle.state", Value::Null)["content"]["entries"],
        2
    );
    let on_disk: Vec<String> = std::fs::read_dir(dir.join("models"))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(on_disk.len(), 3);
    assert!(
        on_disk.iter().all(|f| f.ends_with(".jsonld")),
        "{on_disk:?}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn content_stored_for_a_composition_can_be_found_from_it_and_left_out_content_is_noted() {
    let mut s = orders_with_content();
    // Composition alone: the attached content is user-made (not the library's), so say it was left out.
    let put = run(&mut s, "store.put", json!({"composition": true}));
    assert!(
        put["note"]
            .as_str()
            .unwrap()
            .contains("content attached that was not stored"),
        "{put}"
    );
    let comp_cid = put["cid"].as_str().unwrap().to_string();
    // With the content, no such note, and the content is listed by the composition it is about.
    let both = run(
        &mut s,
        "store.put",
        json!({"composition": true, "content": true}),
    );
    assert!(both.get("note").is_none(), "{both}");
    let by_about = run(&mut s, "store.list", json!({"about": comp_cid}));
    let rows = by_about["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "{by_about}");
    assert_eq!(rows[0]["kind"], "content");
    assert_eq!(rows[0]["cid"], both["content"]["cid"]);
    assert_eq!(rows[0]["about"], json!(comp_cid));
    // Another composition's content is not in it.
    let none = run(&mut s, "store.list", json!({"about": CAFE_CID}));
    assert_eq!(none["rows"].as_array().unwrap().len(), 0, "{none}");
    // The library's content is a name, so it needs no storing and draws no note.
    let mut lib = session();
    run(&mut lib, "bundle.open", json!({}));
    let put = run(&mut lib, "store.put", json!({"composition": true}));
    assert!(put.get("note").is_none(), "{put}");
}

#[test]
fn a_bundle_with_an_id_of_its_own_is_stored_under_its_cid_and_says_the_id_goes() {
    let mut s = session();
    let mut b = cafe();
    b["@id"] = json!("urn:example:cafe");
    let put = run(&mut s, "store.put", json!({"document": b}));
    // The seal ignores a top-level @id: the CID is the cafe's.
    assert_eq!(put["cid"], CAFE_CID);
    assert!(
        put["note"].as_str().unwrap().contains("urn:example:cafe"),
        "{put}"
    );
    // Opened from the store it is the Bundle without the author's @id.
    let mut t = fresh();
    run(&mut t, "store.open", json!({"cid": CAFE_CID}));
    assert_eq!(
        run(&mut t, "bundle.export", Value::Null)["document"],
        cafe()
    );
    // A bundle with none says nothing.
    let mut u = session();
    let put = run(&mut u, "store.put", json!({"document": cafe()}));
    assert!(put.get("note").is_none(), "{put}");
}

#[test]
fn superseding_across_kinds_is_allowed_and_says_so() {
    let mut s = orders_with_content();
    let net = {
        run(&mut s, "file.open_example", json!({"name": "sir"}));
        run(&mut s, "store.put", json!({}))
    };
    let comp = {
        run(&mut s, "bundle.open", json!({}));
        run(&mut s, "store.put", json!({"composition": true}))
    };
    let across = run(
        &mut s,
        "store.supersede",
        json!({"old": comp["cid"], "new": net["cid"]}),
    );
    assert!(
        across["note"]
            .as_str()
            .unwrap()
            .contains("the superseded document is a composition and its successor a net"),
        "{across}"
    );
    // Same kind: nothing to say.
    run(&mut s, "bundle.add_instance", json!({"id": "payment2"}));
    let comp2 = run(&mut s, "store.put", json!({"composition": true}));
    let same = run(
        &mut s,
        "store.supersede",
        json!({"old": comp["cid"], "new": comp2["cid"]}),
    );
    assert!(
        !same["note"]
            .as_str()
            .unwrap_or("")
            .contains("the superseded document is a"),
        "{same}"
    );
}

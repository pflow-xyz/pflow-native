//! The local content-addressed store (`store.*`). Every test gives its own
//! thread a fresh store (memory, or a temporary directory), so nothing
//! touches the real user data directory.

use std::path::PathBuf;
use std::sync::Arc;

use pflow_native_core::store::{self, canon, Store};
use pflow_native_core::{ArtifactData, Session};
use serde_json::{json, Value};

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

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "pflow-native-store-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&d).ok();
    d
}

/// Stores a bundled example and returns its CID.
fn put_example(s: &mut Session, name: &str) -> String {
    run(s, "file.open_example", json!({"name": name}));
    run(s, "store.put", json!({}))["cid"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn put_is_idempotent_by_cid_and_the_cid_is_file_cid() {
    let mut s = session();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let cid = run(&mut s, "file.cid", Value::Null)["cid"]
        .as_str()
        .unwrap()
        .to_string();
    let first = run(&mut s, "store.put", json!({"title": "SIR"}));
    assert_eq!(first["cid"], json!(cid));
    assert_eq!(first["existed"], false);
    let again = run(&mut s, "store.put", json!({"title": "other"}));
    assert_eq!(again["cid"], json!(cid));
    assert_eq!(again["existed"], true);
    assert_eq!(again["title"], "SIR", "the first metadata stands");

    // A library model by name; parents recorded as provenance.
    run(
        &mut s,
        "model.import",
        json!({"name": "shop", "example": "coffee-shop"}),
    );
    let shop = run(
        &mut s,
        "store.put",
        json!({"model": "shop", "parents": [cid]}),
    );
    assert_eq!(shop["parents"], json!([cid]));
    assert_ne!(shop["cid"], json!(cid));

    let list = run(&mut s, "store.list", Value::Null);
    assert_eq!(list["total"], 2);
    let sir = list["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["cid"] == json!(cid))
        .unwrap();
    assert_eq!(sir["title"], "SIR");
    assert_eq!(sir["places"], 3);
    let filtered = run(&mut s, "store.list", json!({"query": "sir"}));
    assert_eq!(filtered["total"], 1);
    let art = s.artifact(filtered["artifact"].as_str().unwrap()).unwrap();
    assert_eq!(art.data.kind(), "table");
    art.data.validate().unwrap();

    // A model cannot list itself as its parent.
    let shop_cid = shop["cid"].as_str().unwrap();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let p = run(&mut s, "store.put", json!({"parents": [shop_cid]}));
    assert_eq!(p["existed"], true);
    assert_eq!(
        p["parents"],
        json!([]),
        "sir descends from nothing recorded; first record stands"
    );
}

#[test]
fn open_round_trips_the_document() {
    let mut s = session();
    let cid = put_example(&mut s, "coffee-shop");
    let original = s.doc.clone();
    let stored = run(&mut s, "store.get", json!({"cid": cid}));
    assert_eq!(stored["@id"], json!(cid), "stored sealed");
    run(&mut s, "file.new", Value::Null);
    assert_eq!(s.doc.places.len(), 0);
    let opened = run(&mut s, "store.open", json!({"cid": cid}));
    assert_eq!(opened["cid"], json!(cid));
    assert_eq!(s.doc.places, original.places);
    assert_eq!(s.doc.transitions, original.transitions);
    assert_eq!(s.doc.arcs, original.arcs);
    assert_eq!(s.path, None);
    assert_eq!(run(&mut s, "file.cid", Value::Null)["cid"], json!(cid));
    assert_eq!(run(&mut s, "file.cid", Value::Null)["sealed"], true);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.doc.places.len(), 0, "open is undoable");
    assert!(s
        .execute("store.open", json!({"cid": "zNotThere"}))
        .is_err());
}

#[test]
fn link_validates_as_sim_and_graph_reads_answer() {
    let mut s = session();
    let a = put_example(&mut s, "sir");
    let b = put_example(&mut s, "coffee-shop");
    let c = put_example(&mut s, "predator-prey");
    let col = run(&mut s, "store.collection_create", json!({"name": "epi"}))["id"]
        .as_str()
        .unwrap()
        .to_string();

    // sim's refusals, in sim's order.
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "knows", "object": b}),
    );
    assert!(
        e.contains("not registered") && e.contains("hasMember"),
        "{e}"
    );
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "narrower", "object": b}),
    );
    assert!(e.contains("inverse of \"broader\""), "{e}");
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "isomorphicTo", "object": b}),
    );
    assert!(e.contains("structural"), "{e}");
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "supersededBy", "object": b}),
    );
    assert!(e.contains("store.supersede"), "{e}");
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "hasMember", "object": b}),
    );
    assert!(
        e.contains("is a model") && e.contains("needs a collection"),
        "{e}"
    );
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "@type", "object": "Queue"}),
    );
    assert!(e.contains("QueueingSystem"), "{e}");
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": "slug", "predicate": "cites", "object": b}),
    );
    assert!(e.contains("not a content id"), "{e}");
    let missing = "0123456789abcdef01234567";
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "cites", "object": missing}),
    );
    assert!(e.contains("not a stored entity"), "{e}");
    let e = refused(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "cites", "object": a}),
    );
    assert!(e.contains("two different entities"), "{e}");

    let t = run(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "@type", "object": "QueueingSystem"}),
    );
    assert_eq!(t["axis"], "taxonomic");
    let l1 = run(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "broader", "object": b}),
    );
    let l2 = run(
        &mut s,
        "store.link",
        json!({"subject": b, "predicate": "broader", "object": c}),
    );
    let again = run(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "broader", "object": b}),
    );
    assert_eq!(again["id"], l1["id"]);
    assert_eq!(again["existing"], true);
    run(
        &mut s,
        "store.link",
        json!({"subject": c, "predicate": "related", "object": a}),
    );
    run(
        &mut s,
        "store.collection_add",
        json!({"collection": col, "cid": a}),
    );

    let edges = run(&mut s, "store.edges", json!({"id": a}));
    assert_eq!(edges["edges"].as_array().unwrap().len(), 4);
    let only = run(
        &mut s,
        "store.edges",
        json!({"id": a, "predicate": "broader"}),
    );
    assert_eq!(only["edges"].as_array().unwrap().len(), 1);
    assert!(s
        .execute("store.edges", json!({"id": a, "predicate": "narrower"}))
        .is_err());

    // Neighbors read through the registry.
    assert_eq!(
        run(
            &mut s,
            "store.neighbors",
            json!({"id": b, "predicate": "narrower"})
        ),
        json!([a])
    );
    assert_eq!(
        run(
            &mut s,
            "store.neighbors",
            json!({"id": a, "predicate": "related"})
        ),
        json!([c])
    );
    assert_eq!(
        run(
            &mut s,
            "store.neighbors",
            json!({"id": c, "predicate": "related"})
        ),
        json!([a])
    );
    assert_eq!(
        run(
            &mut s,
            "store.neighbors",
            json!({"id": c, "predicate": "broader", "direction": "in"})
        ),
        json!([b])
    );
    assert_eq!(
        run(
            &mut s,
            "store.neighbors",
            json!({"id": col, "predicate": "hasMember"})
        ),
        json!([a])
    );
    let both = run(
        &mut s,
        "store.neighbors",
        json!({"id": b, "direction": "both"}),
    );
    assert_eq!(both.as_array().unwrap().len(), 2);

    // Queries: an edge, a closure, a join, an ask, an inverse rewrite.
    let q = run(
        &mut s,
        "store.query",
        json!({"pattern": {"where": [{"subject": a, "predicate": "broader", "object": "?x", "mode": "closure"}]}}),
    );
    assert_eq!(q["status"], "complete");
    let xs: Vec<&str> = q["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["bindings"]["?x"].as_str().unwrap())
        .collect();
    let mut want = vec![b.as_str(), c.as_str()];
    want.sort();
    assert_eq!(xs, want);
    let far = q["rows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["bindings"]["?x"] == json!(c))
        .unwrap();
    assert_eq!(
        far["support"][0]["edges"],
        json!([l1["id"], l2["id"]]),
        "chain in order"
    );
    assert_eq!(far["support"][0]["reading"], "closure");

    let path = run(
        &mut s,
        "store.query",
        json!({"pattern": [{"subject": a, "predicate": "broader", "object": "?x", "mode": "path", "maxHops": 1}]}),
    );
    assert_eq!(path["rows"].as_array().unwrap().len(), 1);
    assert_eq!(path["hopLimitReached"], json!([0]));

    let join = run(
        &mut s,
        "store.query",
        json!({"pattern": {
        "select": ["?m"],
        "where": [
            {"subject": "?col", "predicate": "hasMember", "object": "?m"},
            {"subject": "?m", "predicate": "@type", "object": "QueueingSystem"}
        ]}}),
    );
    assert_eq!(
        join["rows"],
        json!([{"bindings": {"?m": a}, "support": join["rows"][0]["support"]}])
    );
    let table = s.artifact(join["table"].as_str().unwrap()).unwrap();
    assert_eq!(table.data.kind(), "table");
    let graph = s.artifact(join["graph"].as_str().unwrap()).unwrap();
    graph.data.validate().unwrap();
    match &graph.data {
        ArtifactData::Graph { nodes, edges } => {
            assert_eq!(edges.len(), 2);
            assert!(nodes.iter().any(|n| n.id == "type:QueueingSystem"));
        }
        other => panic!("{other:?}"),
    }

    let ask = run(
        &mut s,
        "store.query",
        json!({"pattern": [{"subject": c, "predicate": "narrower", "object": a, "mode": "closure"}]}),
    );
    assert_eq!(ask["answer"], "yes");
    assert!(ask["rewrites"][0]
        .as_str()
        .unwrap()
        .contains("narrower read as broader"));
    let no = run(
        &mut s,
        "store.query",
        json!({"pattern": [{"subject": c, "predicate": "broader", "object": a, "mode": "closure"}]}),
    );
    assert_eq!(no["answer"], "no");

    // Query refusals.
    let e = refused(
        &mut s,
        "store.query",
        json!({"pattern": [{"subject": "?a", "predicate": "related", "object": "?b", "mode": "closure"}]}),
    );
    assert!(e.contains("not transitive"), "{e}");
    let e = refused(
        &mut s,
        "store.query",
        json!({"pattern": [{"subject": "?a", "predicate": "broader", "object": "?b", "mode": "closure"}]}),
    );
    assert!(e.contains("needs one end bound"), "{e}");
    let e = refused(
        &mut s,
        "store.query",
        json!({"pattern": [
        {"subject": "?a", "predicate": "broader", "object": "?b"},
        {"subject": "?c", "predicate": "cites", "object": "?d"}]}),
    );
    assert!(e.contains("share no variable"), "{e}");
    let e = refused(
        &mut s,
        "store.query",
        json!({"pattern": {"where": [], "limit": 3}}),
    );
    assert!(e.contains("unknown field"), "{e}");

    let g = run(&mut s, "store.graph", json!({"around": "epi", "depth": 1}));
    let art = s.artifact(g["artifact"].as_str().unwrap()).unwrap();
    art.data.validate().unwrap();
    assert_eq!(g["nodes"], 2, "the collection and its member");
    let whole = run(&mut s, "store.graph", Value::Null);
    assert!(whole["edges"].as_u64().unwrap() >= 5);
}

#[test]
fn collections_hold_members() {
    let mut s = session();
    let a = put_example(&mut s, "sir");
    let b = put_example(&mut s, "coffee-shop");
    let c1 = run(
        &mut s,
        "store.collection_create",
        json!({"name": "favourites"}),
    );
    let c2 = run(
        &mut s,
        "store.collection_create",
        json!({"name": "favourites"}),
    );
    assert_eq!(c1["id"], c2["id"], "the name is the content id");
    assert_eq!(c2["existed"], true);
    // sim's id: sha24 of {"Name": name} as Go marshals it.
    assert_eq!(c1["id"], json!(canon::sha24(br#"{"Name":"favourites"}"#)));
    run(
        &mut s,
        "store.collection_add",
        json!({"collection": "favourites", "cid": a}),
    );
    run(
        &mut s,
        "store.collection_add",
        json!({"collection": c1["id"], "cid": b}),
    );
    let again = run(
        &mut s,
        "store.collection_add",
        json!({"collection": "favourites", "cid": a}),
    );
    assert_eq!(again["existing"], true);
    let list = run(&mut s, "store.collection_list", Value::Null);
    assert_eq!(list["collections"][0]["members"], 2);
    let rows = run(&mut s, "store.list", Value::Null)["rows"].clone();
    assert!(rows
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["collections"] == json!(["favourites"])));
    assert!(s
        .execute(
            "store.collection_add",
            json!({"collection": "nope", "cid": a})
        )
        .is_err());
}

#[test]
fn license_is_irrevocable() {
    let mut s = session();
    let a = put_example(&mut s, "sir");
    let e = refused(&mut s, "store.license", json!({"cid": a, "license": "MIT"}));
    assert!(e.contains("CC0-1.0"), "{e}");
    run(
        &mut s,
        "store.license",
        json!({"cid": a, "license": "CC0-1.0"}),
    );
    let e = refused(
        &mut s,
        "store.license",
        json!({"cid": a, "license": "CC-BY-4.0"}),
    );
    assert!(e.contains("irrevocable"), "{e}");
    let e = refused(
        &mut s,
        "store.license",
        json!({"cid": a, "license": "CC0-1.0"}),
    );
    assert!(e.contains("irrevocable"), "{e}");
    let e = refused(&mut s, "store.delete", json!({"cid": a, "force": true}));
    assert!(e.contains("irrevocable"), "{e}");
    let rows = run(&mut s, "store.list", Value::Null)["rows"].clone();
    assert_eq!(rows[0]["license"], "CC0-1.0");
}

#[test]
fn supersede_offers_and_accept_carry_edges_forward() {
    let mut s = session();
    let old = put_example(&mut s, "sir");
    let new = put_example(&mut s, "enzyme-kinetics");
    let other = put_example(&mut s, "coffee-shop");
    let col = run(&mut s, "store.collection_create", json!({"name": "epi"}))["id"].clone();
    run(
        &mut s,
        "store.collection_add",
        json!({"collection": col, "cid": old}),
    );
    run(
        &mut s,
        "store.link",
        json!({"subject": old, "predicate": "@type", "object": "ConservedResourceCycle"}),
    );
    let cites = run(
        &mut s,
        "store.link",
        json!({"subject": other, "predicate": "cites", "object": old}),
    );

    let e = refused(&mut s, "store.migration_offers", json!({"cid": old}));
    assert!(e.contains("not superseded"), "{e}");
    assert!(s
        .execute("store.supersede", json!({"old": old, "new": old}))
        .is_err());

    let sup = run(&mut s, "store.supersede", json!({"old": old, "new": new}));
    assert_eq!(sup["edges"], json!({"offered": 3, "stays": 1}));
    assert_eq!(sup["chain"]["current"], json!(new));
    let rows = run(&mut s, "store.list", Value::Null)["rows"].clone();
    let r = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["cid"] == json!(old))
        .unwrap();
    assert_eq!(r["superseded_by"], json!(new));
    assert_eq!(
        run(
            &mut s,
            "store.neighbors",
            json!({"id": old, "predicate": "supersededBy"})
        ),
        json!([new])
    );

    let offers = run(&mut s, "store.migration_offers", json!({"cid": old}));
    assert_eq!(offers["chainStatus"], "complete");
    assert_eq!(
        offers["counts"],
        json!({"offered": 3, "stays": 1, "total": 4})
    );
    let edges = offers["edges"].as_array().unwrap();
    assert!(edges
        .iter()
        .filter(|e| e["disposition"] == "offered")
        .all(|e| e["status"] == "pending" && e["check"]["result"] == "passes"));
    let stays = edges.iter().find(|e| e["disposition"] == "stays").unwrap();
    assert_eq!(stays["predicate"], "supersededBy");
    let cite_row = edges.iter().find(|e| e["id"] == cites["id"]).unwrap();
    assert_eq!(
        cite_row["proposed"],
        json!({"subject": other, "predicate": "cites", "object": new})
    );

    // A wrong successor, or an edge that stays, refuses the whole call.
    let e = refused(
        &mut s,
        "store.accept_migration",
        json!({"old": old, "new": other}),
    );
    assert!(e.contains("Nothing was migrated"), "{e}");
    let e = refused(
        &mut s,
        "store.accept_migration",
        json!({"old": old, "edges": [stays["id"], cites["id"]]}),
    );
    assert!(e.contains("stays with"), "{e}");
    assert_eq!(
        run(&mut s, "store.edges", json!({"id": new}))["edges"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "nothing written"
    );

    let acc = run(
        &mut s,
        "store.accept_migration",
        json!({"old": old, "new": new}),
    );
    assert_eq!(acc["successor"], json!(new));
    assert_eq!(acc["migrated"].as_array().unwrap().len(), 3);
    // The successor now carries the edges; the old ones stay.
    // Carried edges carry the zero time, so they read first (as on sim).
    assert_eq!(
        run(
            &mut s,
            "store.neighbors",
            json!({"id": col, "predicate": "hasMember"})
        ),
        json!([new, old])
    );
    let q = run(
        &mut s,
        "store.query",
        json!({"pattern": [{"subject": new, "predicate": "@type", "object": "ConservedResourceCycle"}]}),
    );
    assert_eq!(q["answer"], "yes");
    assert_eq!(
        run(
            &mut s,
            "store.neighbors",
            json!({"id": other, "predicate": "cites"})
        ),
        json!([new, old])
    );
    // migratedFrom points from each new edge back to the old one.
    let migrated = acc["migrated"][0]["migrated"].as_str().unwrap();
    let back = run(
        &mut s,
        "store.neighbors",
        json!({"id": migrated, "predicate": "migratedFrom"}),
    );
    assert_eq!(back, json!([acc["migrated"][0]["edge"]]));

    let offers = run(&mut s, "store.migration_offers", json!({"cid": old}));
    assert!(offers["edges"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["disposition"] == "offered")
        .all(|e| e["status"] == "accepted"));
    // Re-running converges on the same ids.
    let again = run(
        &mut s,
        "store.accept_migration",
        json!({"old": old, "edges": [cites["id"]]}),
    );
    let first = acc["migrated"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["edge"] == cites["id"])
        .unwrap();
    assert_eq!(again["migrated"][0]["migrated"], first["migrated"]);
    assert_eq!(again["migrated"][0]["provenance"], first["provenance"]);
    assert_eq!(again["migrated"][0]["existing"], true);
    let e = refused(&mut s, "store.accept_migration", json!({"old": old}));
    assert!(e.contains("no pending offer"), "{e}");

    // A chain: old → new → newest; offers target the newest.
    let newest = put_example(&mut s, "producer-consumer");
    run(
        &mut s,
        "store.supersede",
        json!({"old": new, "new": newest}),
    );
    let offers = run(&mut s, "store.migration_offers", json!({"cid": old}));
    assert_eq!(offers["current"], json!(newest));
    assert_eq!(offers["chain"], json!([old, new, newest]));
    // A cycle has no current successor.
    run(
        &mut s,
        "store.supersede",
        json!({"old": newest, "new": old}),
    );
    let offers = run(&mut s, "store.migration_offers", json!({"cid": old}));
    assert_eq!(offers["chainStatus"], "cycle");
    assert!(s
        .execute("store.accept_migration", json!({"old": old}))
        .is_err());
}

#[test]
fn maps_are_content_addressed() {
    let mut s = session();
    let m1 = run(
        &mut s,
        "store.map_put",
        json!({"data": {"b": 2, "a": [1.5, "x"]}}),
    );
    let m2 = run(
        &mut s,
        "store.map_put",
        json!({"data": "{\"a\": [1.5, \"x\"], \"b\": 2.0}"}),
    );
    assert_eq!(
        m1["id"], m2["id"],
        "key order and number spelling do not matter"
    );
    assert_eq!(m1["existed"], false);
    assert_eq!(m2["existed"], true);
    assert_eq!(
        m1["id"],
        json!(canon::sha24(br#"{"Data":{"a":[1.5,"x"],"b":2}}"#)),
        "sim's map id"
    );
    let got = run(&mut s, "store.map_get", json!({"id": m1["id"]}));
    assert_eq!(got["Data"], json!({"a": [1.5, "x"], "b": 2}));
    let list = run(&mut s, "store.map_list", Value::Null);
    assert_eq!(list["maps"], json!([{"id": m1["id"], "keys": 2}]));
    assert!(s.execute("store.map_put", json!({"data": [1, 2]})).is_err());
    assert!(s
        .execute("store.map_get", json!({"id": "0123456789abcdef01234567"}))
        .is_err());

    // A map can be cited by a model and collected.
    let a = put_example(&mut s, "sir");
    run(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "cites", "object": m1["id"]}),
    );
}

#[test]
fn delete_refuses_referenced_models_unless_forced() {
    let mut s = session();
    let a = put_example(&mut s, "sir");
    let b = put_example(&mut s, "coffee-shop");
    let lone = put_example(&mut s, "tic-tac-toe");
    run(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "cites", "object": b}),
    );
    let e = refused(&mut s, "store.delete", json!({"cid": b}));
    assert!(e.contains("force"), "{e}");
    run(&mut s, "store.delete", json!({"cid": lone}));
    assert!(s.execute("store.get", json!({"cid": lone})).is_err());
    let d = run(&mut s, "store.delete", json!({"cid": b, "force": true}));
    assert_eq!(d["dangling_edges"], 1);
    assert!(s.execute("store.get", json!({"cid": b})).is_err());
    assert_eq!(
        run(&mut s, "store.edges", json!({"id": a}))["edges"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "edges are kept"
    );
    let g = run(&mut s, "store.graph", Value::Null);
    s.artifact(g["artifact"].as_str().unwrap())
        .unwrap()
        .data
        .validate()
        .unwrap();
    // Storing the net again revives it, edges and all.
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    assert_eq!(run(&mut s, "store.put", Value::Null)["cid"], json!(b));
    assert_eq!(run(&mut s, "store.neighbors", json!({"id": a})), json!([b]));
}

#[test]
fn filesystem_store_persists_across_handles() {
    let dir = temp_dir("persist");
    store::set_thread_store(Some(Arc::new(Store::filesystem(&dir))));
    let mut s = Session::default();
    let a = put_example(&mut s, "sir");
    let b = put_example(&mut s, "coffee-shop");
    run(
        &mut s,
        "store.link",
        json!({"subject": a, "predicate": "related", "object": b}),
    );
    let col = run(&mut s, "store.collection_create", json!({"name": "kept"}))["id"].clone();
    run(
        &mut s,
        "store.collection_add",
        json!({"collection": col, "cid": a}),
    );
    let map = run(&mut s, "store.map_put", json!({"data": {"k": "v"}}))["id"].clone();
    run(
        &mut s,
        "store.license",
        json!({"cid": b, "license": "CC-BY-4.0"}),
    );

    // The layout on disk.
    assert!(dir.join("models").join(format!("{a}.jsonld")).is_file());
    assert!(dir.join("index.json").is_file());
    assert!(dir.join("edges.json").is_file());
    assert!(dir.join("collections.json").is_file());
    assert!(dir
        .join("maps")
        .join(format!("{}.json", map.as_str().unwrap()))
        .is_file());
    let model: Value = serde_json::from_slice(
        &std::fs::read(dir.join("models").join(format!("{a}.jsonld"))).unwrap(),
    )
    .unwrap();
    assert_eq!(
        pflow_native_core::cid::seal(&model).unwrap(),
        a,
        "the file seals to its name"
    );

    // A fresh handle on the same directory sees everything.
    let fresh = Store::filesystem(&dir);
    let snap = fresh.snapshot().unwrap();
    assert_eq!(snap.models.len(), 2);
    assert_eq!(snap.models[&b].license.as_deref(), Some("CC-BY-4.0"));
    assert_eq!(snap.relations.len(), 2);
    assert_eq!(
        fresh.get_map(map.as_str().unwrap()).unwrap(),
        json!({"k": "v"})
    );
    store::set_thread_store(Some(Arc::new(fresh)));
    let mut t = Session::default();
    assert_eq!(
        run(
            &mut t,
            "store.neighbors",
            json!({"id": col, "predicate": "hasMember"})
        ),
        json!([a])
    );
    run(&mut t, "store.open", json!({"cid": a}));
    assert_eq!(t.doc.places.len(), 3);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn default_store_lives_under_pflow_native_home() {
    // The only test that reads the environment: it points the data dir at
    // a temporary directory first.
    let home = temp_dir("home");
    std::env::set_var("PFLOW_NATIVE_HOME", &home);
    store::set_thread_store(None);
    let st = store::open();
    assert_eq!(PathBuf::from(st.location()), home.join("store"));
    assert!(Arc::ptr_eq(&st, &store::open()), "one handle per data dir");
    let mut s = Session::default();
    let a = put_example(&mut s, "sir");
    assert!(home
        .join("store/models")
        .join(format!("{a}.jsonld"))
        .is_file());
    let info = run(&mut s, "store.info", Value::Null);
    assert_eq!(info["models"], 1);
    assert_eq!(info["predicates"].as_array().unwrap().len(), 18);
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn store_commands_are_journaled_by_kind() {
    let s = session();
    for c in s
        .registry()
        .commands()
        .iter()
        .filter(|c| c.id.starts_with("store."))
    {
        let write = [
            "store.put",
            "store.open",
            "store.delete",
            "store.link",
            "store.collection_create",
            "store.collection_add",
            "store.license",
            "store.supersede",
            "store.accept_migration",
            "store.map_put",
        ]
        .contains(&c.id);
        assert_eq!(c.journal, write, "{}", c.id);
    }
}

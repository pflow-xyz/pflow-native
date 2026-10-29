//! `session.dump_state` / `session.restore_state`: the state that outlives
//! one document, saved and restored whole. Every test gives its thread its
//! own store (memory, or a temporary directory), so nothing touches the
//! real user data directory.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use pflow_native_core::commands::session::MAX_NEXT;
use pflow_native_core::store::{self, MemoryBackend, Store, StoreBackend};
use pflow_native_core::Session;
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

fn memory_store() -> Arc<Store> {
    let st = Arc::new(Store::memory());
    store::set_thread_store(Some(st.clone()));
    st
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "pflow-native-session-state-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    std::fs::remove_dir_all(&d).ok();
    d
}

/// Every file under `dir`, by relative path.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                let rel = p.strip_prefix(root).unwrap().to_string_lossy().into_owned();
                out.insert(rel, std::fs::read(&p).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

/// The one key of a dumped state's store under `dir`.
fn blob_key(state: &Value, dir: &str) -> String {
    let keys: Vec<&String> = state["store"]["blobs"]
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| k.starts_with(dir))
        .collect();
    assert_eq!(keys.len(), 1, "{keys:?}");
    keys[0].clone()
}

fn model_key(state: &Value) -> String {
    blob_key(state, "models/")
}

fn map_key(state: &Value) -> String {
    blob_key(state, "maps/")
}

/// Another net, as text, for a model file that does not match its key.
fn coffee_shop() -> String {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    run(&mut s, "file.export_json", Value::Null).to_string()
}

/// A session with something in every part: a library model with a port,
/// a binding, a macro, a recording in progress, a note on a result, and a
/// store with a model, a map and a collection.
fn populated() -> Session {
    let mut s = Session::default();
    run(
        &mut s,
        "model.import",
        json!({"name": "a", "example": "sir"}),
    );
    run(
        &mut s,
        "model.import",
        json!({"name": "b", "example": "coffee-shop"}),
    );
    run(
        &mut s,
        "system.set_port",
        json!({"model": "a", "id": "I", "direction": "out", "kind": "People"}),
    );
    run(
        &mut s,
        "system.set_port",
        json!({"model": "b", "id": "CustomerArrives", "direction": "in", "kind": "People"}),
    );
    run(
        &mut s,
        "system.bind",
        json!({"from_model": "a", "from_port": "I", "to_model": "b", "to_port": "CustomerArrives"}),
    );
    run(
        &mut s,
        "macro.define",
        json!({"name": "one", "steps": [{"command": "net.add_place", "args": {"x": 1, "y": 1}}]}),
    );
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let cid = run(&mut s, "store.put", json!({"title": "SIR"}))["cid"].clone();
    // Values whose canonical bytes are easy to get wrong, so a restore's
    // content-id check is seen to accept what store.map_put writes.
    run(
        &mut s,
        "store.map_put",
        json!({"data": {"k": "v", "x": 0.1, "big": 1e21, "small": -2.5e-7, "s": "é <&> \u{2028}", "deep": {"b": [1, 2.0, true, null]}}}),
    );
    let col = run(&mut s, "store.collection_create", json!({"name": "c"}))["id"].clone();
    run(
        &mut s,
        "store.collection_add",
        json!({"collection": col, "cid": cid}),
    );
    run(&mut s, "analysis.invariants", Value::Null);
    let art = s.artifacts[0].id.clone();
    run(&mut s, "note.add", json!({"text": "see", "artifact": art}));
    run(&mut s, "macro.record", Value::Null);
    run(&mut s, "net.add_place", json!({"x": 5, "y": 5}));
    s
}

#[test]
fn a_dump_restores_into_a_fresh_session_and_store() {
    memory_store();
    let mut s = populated();
    let state = run(&mut s, "session.dump_state", Value::Null);
    assert_eq!(state["format"], "pflow-native-session-state");
    assert_eq!(state["version"], 1);
    assert_eq!(state["store"]["location"], "memory");
    assert_eq!(state["recording"].as_array().unwrap().len(), 1);

    // Another store and session, with state of its own that the restore
    // replaces.
    let fresh = memory_store();
    let mut t = Session::default();
    run(
        &mut t,
        "model.import",
        json!({"name": "old", "example": "sir"}),
    );
    run(&mut t, "net.add_place", json!({"x": 1, "y": 1}));
    let (doc, revision) = (t.doc.clone(), t.revision);
    let out = run(&mut t, "session.restore_state", json!({"state": state}));
    assert_eq!(out["models"], 2, "{out}");
    assert_eq!(out["recording"], 1, "{out}");
    assert_eq!(run(&mut t, "session.dump_state", Value::Null), state);
    assert!(!t.models.contains_key("old"));
    assert_eq!(fresh.snapshot().unwrap().models.len(), 1);
    // The document and its history are the session's own.
    assert_eq!(t.doc, doc);
    assert_eq!(t.revision, revision);
    assert!(t.can_undo());
    // The recording carries on; neither command was journaled.
    run(&mut t, "net.add_place", json!({"x": 9, "y": 9}));
    run(&mut t, "macro.stop", json!({"name": "rec"}));
    assert_eq!(t.macros["rec"].len(), 2);
    assert!(t
        .journal
        .iter()
        .all(|st| !st.command.starts_with("session.")));
    // A state passed as a string works the same.
    let text = state.to_string();
    let mut u = Session::default();
    memory_store();
    run(&mut u, "session.restore_state", json!({"state": text}));
    assert_eq!(run(&mut u, "session.dump_state", Value::Null), state);
}

#[test]
fn parts_pick_what_is_dumped_and_absent_parts_are_kept() {
    memory_store();
    let mut s = populated();
    let some = run(
        &mut s,
        "session.dump_state",
        json!({"parts": ["macros", "models"]}),
    );
    let keys: Vec<&str> = some
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(
        keys,
        ["format", "version", "pflowNative", "models", "macros"]
    );
    let e = refused(&mut s, "session.dump_state", json!({"parts": ["model"]}));
    assert!(e.contains("unknown part \"model\""), "{e}");

    // Only the parts present change.
    let mut t = Session::default();
    run(&mut t, "note.add", json!({"text": "mine"}));
    run(&mut t, "session.restore_state", json!({"state": some}));
    assert_eq!(t.models.len(), 2);
    assert_eq!(t.macros.len(), 1);
    assert_eq!(t.notes.len(), 1);
    assert_eq!(t.notes[0].text, "mine");
}

#[test]
fn a_store_on_disk_is_never_overwritten() {
    memory_store();
    let mut s = populated();
    let state = run(&mut s, "session.dump_state", Value::Null);

    let dir = temp_dir("disk");
    store::set_thread_store(Some(Arc::new(Store::filesystem(&dir))));
    let mut t = Session::default();
    run(&mut t, "file.open_example", json!({"name": "coffee-shop"}));
    let cid = run(&mut t, "store.put", Value::Null)["cid"]
        .as_str()
        .unwrap()
        .to_string();
    let map = run(&mut t, "store.map_put", json!({"data": {"k": 1}}))["id"]
        .as_str()
        .unwrap()
        .to_string();
    let col = run(&mut t, "store.collection_create", json!({"name": "c"}))["id"].clone();
    run(
        &mut t,
        "store.collection_add",
        json!({"collection": col, "cid": cid}),
    );
    run(
        &mut t,
        "model.import",
        json!({"name": "keep", "example": "sir"}),
    );
    let on_disk = files(&dir);
    assert!(on_disk.contains_key(&format!("models/{cid}.jsonld")));

    let e = refused(&mut t, "session.restore_state", json!({"state": state}));
    assert!(e.contains("on disk"), "{e}");
    assert!(e.contains(&dir.display().to_string()), "{e}");
    assert_eq!(files(&dir), on_disk, "the directory is untouched");
    assert!(t.models.contains_key("keep"), "and so is the session");

    // A disk store dumps (root files and both directories): every file the
    // store wrote, so a new kind of file cannot be left out unnoticed. A
    // stray file in the directory is not part of the store.
    std::fs::write(dir.join("README"), "mine").unwrap();
    let dumped = run(&mut t, "session.dump_state", json!({"parts": ["store"]}));
    let keys: Vec<&String> = dumped["store"]["blobs"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(
        keys,
        [
            "collections.json".to_string(),
            "edges.json".to_string(),
            "index.json".to_string(),
            format!("maps/{map}.json"),
            format!("models/{cid}.jsonld"),
        ]
        .iter()
        .collect::<Vec<_>>()
    );
    assert_eq!(keys, on_disk.keys().collect::<Vec<_>>());
    std::fs::remove_file(dir.join("README")).unwrap();
    let mut rest = state.clone();
    rest.as_object_mut().unwrap().remove("store");
    run(&mut t, "session.restore_state", json!({"state": rest}));
    assert!(!t.models.contains_key("keep"));
    assert_eq!(files(&dir), on_disk);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_bad_part_refuses_the_whole_state() {
    memory_store();
    let mut s = populated();
    let state = run(&mut s, "session.dump_state", Value::Null);
    let with = |f: &dyn Fn(&mut Value)| {
        let mut v = state.clone();
        // A good part to replace, beside the damage.
        v["models"] = json!({});
        f(&mut v);
        json!({ "state": v })
    };
    let (mk, pk) = (model_key(&state), map_key(&state));
    let cases: Vec<(Value, &str)> = vec![
        (with(&|v| v["version"] = json!(2)), "version 2"),
        (with(&|v| v["version"] = json!(0)), "no version"),
        (
            with(&|v| v["format"] = json!("other")),
            "not a saved pflow-native session",
        ),
        (with(&|v| v["extra"] = json!(1)), "unknown part \"extra\""),
        (
            with(&|v| v["models"] = json!({"current": {}})),
            "invalid model name",
        ),
        (
            with(&|v| v["models"] = json!({"x": {"places": []}})),
            "state.models: x:",
        ),
        (
            with(&|v| v["macros"] = json!({"m": [{"args": {}}]})),
            "state.macros",
        ),
        (with(&|v| v["recording"] = json!(3)), "state.recording"),
        (
            with(&|v| v["notes"]["next"] = json!(0)),
            "next is 0, but n1 exists",
        ),
        (
            with(&|v| {
                let n = v["notes"]["items"][0].clone();
                v["notes"]["items"].as_array_mut().unwrap().push(n);
            }),
            "repeated",
        ),
        (
            with(&|v| v["notes"]["items"][0]["text"] = json!("x".repeat(501))),
            "longer than 500",
        ),
        (
            with(&|v| {
                v["notes"]["items"][0].as_object_mut().unwrap().remove("id");
            }),
            "item 0 has no string id",
        ),
        // A counter past MAX_NEXT would wrap round to ids in use.
        (
            with(&|v| v["notes"]["next"] = json!(u64::MAX)),
            "next is 18446744073709551615; a restored id counter is at most 4294967295",
        ),
        (
            with(&|v| v["artifacts"]["next"] = json!(MAX_NEXT + 1)),
            "state.artifacts: next is 4294967296",
        ),
        (
            with(&|v| {
                v["notes"]["items"][0]["id"] = json!("n4294967296");
                v["notes"]["next"] = Value::Null;
            }),
            "next is 4294967296; a restored id counter is at most 4294967295",
        ),
        (
            with(&|v| v["store"]["blobs"]["../escape"] = json!("{}")),
            "invalid store key",
        ),
        (
            with(&|v| v["store"]["blobs"]["maps/deep/x.json"] = json!("{}")),
            "not a file of the store's layout",
        ),
        (
            with(&|v| v["store"]["blobs"]["notes.txt"] = json!("hello")),
            "not a file of the store's layout",
        ),
        (
            with(&|v| v["store"]["blobs"]["models/not-a-cid.jsonld"] = json!("{}")),
            "not a file of the store's layout",
        ),
        (
            with(&|v| {
                v["store"]["blobs"][&format!("maps/{}.json", "0".repeat(24))] = json!("{not json")
            }),
            "store maps/000000000000000000000000.json",
        ),
        (
            with(&|v| v["store"]["blobs"]["index.json"] = json!(r#"{"models": []}"#)),
            "store index.json",
        ),
        (
            with(&|v| v["store"] = json!({"blobs": {}, "more": 1})),
            "unknown key",
        ),
        // The content-addressed files hold what their keys say.
        (
            with(&|v| v["store"]["blobs"][&mk] = json!(coffee_shop())),
            "not the z",
        ),
        (
            with(&|v| v["store"]["blobs"][&mk] = json!("[1,2]")),
            "not a JSON object",
        ),
        (
            with(&|v| {
                v["store"]["blobs"].as_object_mut().unwrap().remove(&mk);
            }),
            "index.json lists model z",
        ),
        (
            with(&|v| v["store"]["blobs"][&pk] = json!(r#"{"Data":{"k":"w"}}"#)),
            "holds the content of",
        ),
        (
            with(&|v| v["store"]["blobs"][&pk] = json!(r#"{"k":"v"}"#)),
            "a map file is",
        ),
        (
            with(&|v| {
                let t = v["store"]["blobs"]["edges.json"]
                    .as_str()
                    .unwrap()
                    .replace("hasMember", "related");
                v["store"]["blobs"]["edges.json"] = json!(t);
            }),
            "store edges.json: relation",
        ),
        (
            with(&|v| {
                let t = v["store"]["blobs"]["collections.json"]
                    .as_str()
                    .unwrap()
                    .replace("\"c\"", "\"d\"");
                v["store"]["blobs"]["collections.json"] = json!(t);
            }),
            "store collections.json: collection",
        ),
    ];
    for (args, want) in cases {
        let e = refused(&mut s, "session.restore_state", args.clone());
        assert!(e.contains(want), "{want:?} not in {e:?} for {args}");
        assert_eq!(
            run(&mut s, "session.dump_state", Value::Null),
            state,
            "nothing changed after {e}"
        );
    }
    let e = refused(&mut s, "session.restore_state", json!({"state": "{oops"}));
    assert!(e.contains("not JSON"), "{e}");
}

#[test]
fn a_file_that_is_not_text_cannot_be_dumped() {
    let b = MemoryBackend::new();
    let key = format!("maps/{}.json", "0".repeat(24));
    b.put(&key, &[0xff, 0xfe]).unwrap();
    store::set_thread_store(Some(Arc::new(Store::new(b))));
    let mut s = Session::default();
    let e = refused(&mut s, "session.dump_state", Value::Null);
    assert!(e.contains(&key) && e.contains("not UTF-8"), "{e}");
    // Without the store part, the rest still dumps.
    run(&mut s, "session.dump_state", json!({"parts": ["models"]}));
}

/// A result holding a number that is not finite (here a diverging ODE)
/// dumps with null in its place, which an `Artifact` does not read back.
/// The restore drops that result and names it rather than refuse the
/// library, macros, notes and store beside it.
#[test]
fn a_result_that_does_not_read_back_is_dropped_not_the_state() {
    memory_store();
    let mut s = populated();
    run(
        &mut s,
        "net.add_place",
        json!({"x": 0, "y": 90, "id": "P", "initial": [1]}),
    );
    run(
        &mut s,
        "net.add_transition",
        json!({"x": 50, "y": 90, "id": "grow"}),
    );
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "P", "target": "grow"}),
    );
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "grow", "target": "P", "weight": [3]}),
    );
    run(&mut s, "sim.set_rate", json!({"id": "grow", "rate": 1000}));
    run(
        &mut s,
        "sim.ode",
        json!({"t_end": 100, "method": "euler", "dt": 0.1}),
    );
    let ode = s.artifacts.last().unwrap().id.clone();
    let state = run(&mut s, "session.dump_state", Value::Null);
    let saved = state["artifacts"]["items"].as_array().unwrap();
    let with_null: Vec<&Value> = saved
        .iter()
        .filter(|a| a["data"].to_string().contains("null"))
        .map(|a| &a["id"])
        .collect();
    assert_eq!(with_null, [&json!(ode)], "the diverging run saves null");

    let fresh = memory_store();
    let mut t = Session::default();
    let out = run(&mut t, "session.restore_state", json!({"state": state}));
    assert_eq!(out["artifacts"], saved.len() - 1, "{out}");
    assert_eq!(out["artifacts_dropped"], json!([ode]), "{out}");
    let warnings = out["warnings"].to_string();
    assert!(
        warnings.contains(&format!("result {ode} was not restored"))
            && warnings.contains("null")
            && warnings.contains("sim.ode recomputes it"),
        "{warnings}"
    );
    // Everything else came back.
    assert_eq!(out["models"], 2, "{out}");
    assert_eq!(t.models.len(), 2);
    assert_eq!(t.macros.len(), 1);
    assert_eq!(t.notes.len(), 1);
    assert_eq!(fresh.snapshot().unwrap().models.len(), 1);
    assert!(t.artifact(&ode).is_none());
    // The dropped id is not handed out again.
    run(&mut t, "file.open_example", json!({"name": "sir"}));
    run(&mut t, "analysis.invariants", Value::Null);
    assert_ne!(t.artifacts.last().unwrap().id, ode);
    // The state the restored session dumps restores whole.
    let again = run(&mut t, "session.dump_state", Value::Null);
    let mut u = Session::default();
    memory_store();
    let out = run(&mut u, "session.restore_state", json!({"state": again}));
    assert!(out.get("warnings").is_none(), "{out}");

    // A result whose shape does not hold is dropped the same way.
    let mut bent = state.clone();
    bent["artifacts"]["items"][0]["data"] = json!({"kind": "timeseries", "x_label": "t", "x": [0, 1], "series": [{"name": "a", "values": [1]}]});
    let first = bent["artifacts"]["items"][0]["id"].clone();
    let out = run(&mut u, "session.restore_state", json!({"state": bent}));
    assert_eq!(out["artifacts"], saved.len() - 2, "{out}");
    assert_eq!(out["artifacts_dropped"], json!([first, ode]), "{out}");
    assert!(
        out["warnings"].to_string().contains("1 values for 2 x"),
        "{out}"
    );
}

/// A macro step naming a command this build lacks (a state saved by
/// another build: an older one, or a native one with `remote.*`) is kept as
/// saved and warned about; `macro.play` refuses it when it gets there. The
/// ids here are ones no build has, as this test's build may have
/// `remote.*` (the workspace's tests turn `services` on).
#[test]
fn a_macro_step_this_build_lacks_is_kept_and_warned_about() {
    memory_store();
    let mut s = Session::default();
    let state = json!({
        "format": "pflow-native-session-state",
        "version": 1,
        "macros": {"m": [
            {"command": "net.add_place", "args": {"x": 1, "y": 1}},
            {"command": "remote.retired", "args": {}},
        ]},
        "recording": [{"command": "rnn.retired", "args": {}}],
    });
    let out = run(&mut s, "session.restore_state", json!({"state": state}));
    assert_eq!(out["macros"], 1, "{out}");
    let w = out["warnings"].to_string();
    assert!(
        w.contains("macro \\\"m\\\", step 1: this build has no command \\\"remote.retired\\\""),
        "{w}"
    );
    assert!(w.contains("the recording, step 0"), "{w}");
    assert_eq!(s.macros["m"].len(), 2, "kept as saved");
    let e = refused(&mut s, "macro.play", json!({"name": "m"}));
    assert!(e.contains("rolled back"), "{e}");
    assert!(
        s.doc.places.is_empty(),
        "the macro's first step is rolled back"
    );
}

#[test]
fn the_open_composition_and_plan_round_trip() {
    memory_store();
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    run(&mut s, "bundle.add_instance", json!({"id": "payment2"}));
    run(
        &mut s,
        "bundle.add_link",
        json!({"kind": "token", "from": "payment:account:customer", "to": "payment2:account:customer"}),
    );
    run(&mut s, "plan.open", json!({}));
    run(
        &mut s,
        "plan.add_piece",
        json!({"piece": "threats", "slot": "order"}),
    );
    run(
        &mut s,
        "view.composition",
        json!({"view": "laws", "example": "connect4", "lens": "assume"}),
    );
    let state = run(&mut s, "session.dump_state", Value::Null);
    assert_eq!(
        state["composition"]["instances"].as_array().unwrap().len(),
        4
    );
    assert_eq!(
        state["plan"]["order"].as_array().unwrap().last().unwrap(),
        "threats"
    );
    assert_eq!(state["composition_view"]["view"], "laws");

    let mut t = Session::default();
    memory_store();
    let rev = t.revision;
    let out = run(&mut t, "session.restore_state", json!({"state": state}));
    assert_eq!(out["composition"], 4, "{out}");
    assert_eq!(out["plan"], true, "{out}");
    assert!(t.revision > rev, "the views re-read");
    assert_eq!(
        run(&mut t, "bundle.state", Value::Null)["links"],
        run(&mut s, "bundle.state", Value::Null)["links"]
    );
    assert_eq!(
        run(&mut t, "plan.state", Value::Null),
        run(&mut s, "plan.state", Value::Null)
    );
    assert_eq!(t.composition_view, s.composition_view);
    assert_eq!(run(&mut t, "session.dump_state", Value::Null), state);

    // A state saved before composition documents has `instances` and `links`
    // and no `document`: it still restores.
    let mut old = state.clone();
    old["composition"] = json!({
        "instances": state["composition"]["instances"],
        "links": state["composition"]["links"],
    });
    let mut u = Session::default();
    let out = run(&mut u, "session.restore_state", json!({"state": old}));
    assert_eq!(out["composition"], 4, "{out}");
    assert_eq!(
        run(&mut u, "bundle.state", Value::Null)["links"],
        run(&mut s, "bundle.state", Value::Null)["links"]
    );

    // Closed means closed: null restores as nothing open.
    run(&mut s, "bundle.close", Value::Null);
    run(&mut s, "plan.close", Value::Null);
    let closed = run(
        &mut s,
        "session.dump_state",
        json!({"parts": ["composition", "plan"]}),
    );
    assert!(closed["composition"].is_null() && closed["plan"].is_null());
    run(&mut t, "session.restore_state", json!({"state": closed}));
    assert!(t.composition.is_none() && t.search_plan.is_none());

    // A saved link this build's rules refuse refuses the whole state, in
    // either shape.
    let link = json!([{"kind": "token", "from": "orders:in", "to": "workers:available"}]);
    let mut bad = old.clone();
    bad["composition"]["links"] = link.clone();
    let e = refused(&mut t, "session.restore_state", json!({"state": bad}));
    assert!(
        e.contains("composition") && e.contains("schemas differ"),
        "{e}"
    );
    let mut bad_doc = state.clone();
    bad_doc["composition"]["document"]["links"] = link;
    let e = refused(&mut t, "session.restore_state", json!({"state": bad_doc}));
    assert!(
        e.contains("composition") && e.contains("schemas differ"),
        "{e}"
    );
    // An unknown instance refuses the whole state.
    let mut worse = old.clone();
    worse["composition"]["instances"] = json!(["nope"]);
    let e = refused(&mut t, "session.restore_state", json!({"state": worse}));
    assert!(e.contains("composition"), "{e}");
    // A state with neither a document nor instances is not a composition.
    let mut empty = state.clone();
    empty["composition"] = json!({});
    let e = refused(&mut t, "session.restore_state", json!({"state": empty}));
    assert!(e.contains("composition"), "{e}");
}

#[test]
fn a_loaded_raw_bundle_round_trips_as_its_document() {
    memory_store();
    let cafe: Value =
        serde_json::from_str(include_str!("fixtures/bundle/cafe.bundle.json")).unwrap();
    let mut s = Session::default();
    let loaded = run(&mut s, "bundle.load", json!({"document": cafe}));
    assert_eq!(loaded["form"], "bundle", "{loaded}");
    let state = run(
        &mut s,
        "session.dump_state",
        json!({"parts": ["composition"]}),
    );
    assert_eq!(state["composition"]["form"], "bundle");

    let mut t = Session::default();
    let out = run(&mut t, "session.restore_state", json!({"state": state}));
    assert_eq!(out["composition"], 3, "{out}");
    // The same document, so the same CID (an unedited raw load re-exports
    // as written), and the same dump.
    let (a, b) = (
        run(&mut s, "bundle.export", Value::Null),
        run(&mut t, "bundle.export", Value::Null),
    );
    assert_eq!(a["cid"], b["cid"]);
    assert_eq!(a["document"], b["document"]);
    assert_eq!(
        run(
            &mut t,
            "session.dump_state",
            json!({"parts": ["composition"]})
        ),
        state
    );
}

#[test]
fn attached_content_round_trips_with_the_composition() {
    memory_store();
    let mut s = Session::default();
    run(
        &mut s,
        "bundle.load",
        json!({"document": orders_document(), "library": "orders"}),
    );
    let state = run(
        &mut s,
        "session.dump_state",
        json!({"parts": ["composition"]}),
    );
    // The library's content is a name, not a copy.
    assert_eq!(state["composition"]["library"], "orders");
    assert!(state["composition"].get("content").is_none());

    let mut t = Session::default();
    run(&mut t, "session.restore_state", json!({"state": state}));
    assert_eq!(
        run(&mut t, "bundle.scenarios", Value::Null)["source"],
        run(&mut s, "bundle.scenarios", Value::Null)["source"]
    );
    // A name this build does not have refuses the state.
    let mut gone = state.clone();
    gone["composition"]["library"] = json!("nope");
    let e = refused(&mut t, "session.restore_state", json!({"state": gone}));
    assert!(e.contains("composition") && e.contains("nope"), "{e}");
}

/// The default orders composition, as `bundle.export` writes it.
fn orders_document() -> Value {
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    run(&mut s, "bundle.export", json!({"form": "composition"}))["document"].clone()
}

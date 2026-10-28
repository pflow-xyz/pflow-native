//! pflow-native-core for the browser.
//!
//! pflow.xyz loads this module lazily, when the user runs a command its JS
//! engines lack (analysis, verify, reach, diagnose, ...). The contract is
//! the registry's, as the CLI and MCP see it, over JSON strings:
//!
//! - [`Engine::list_commands`]: ids, argument schemas, help, enabled now.
//! - [`Engine::execute`]: one command, `Session::execute`.
//! - [`Engine::batch`]: several, as one undo step that rolls back on failure.
//! - [`Engine::load`]: replace the document with pflow.xyz JSON-LD. It is
//!   the page syncing its editor, not an edit of the user's, so it is not
//!   journaled (a macro recorded here never replays it). The library,
//!   store, macros, notes and systems stay; the results go, as they belong
//!   to the old document.
//! - [`Engine::dump_state`] / [`Engine::restore_state`]: everything that
//!   outlives one document (the library, systems, macros and a recording in
//!   progress, notes, results and the store) as one JSON string
//!   (`session.dump_state`), for the page to keep in browser storage.
//!
//! The composition views' commands (`bundle.*`, `plan.*`) are exposed the
//! same way as every other: there is no allowlist, so the page lists and
//! runs them like the rest.
//!
//! Every call goes through the registry (`Session::execute`; `load` through
//! `Session::execute_unjournaled`), as the ground rules require; this crate
//! adds no behaviour, only the JSON boundary. Errors come back as thrown JS
//! errors carrying the registry's message.
//!
//! Each engine has its own memory store, made the thread's store
//! (`store::set_thread_store`) for the length of each call: two engines
//! never share one, and the native tests never touch the user's data
//! directory.
//!
//! The logic lives in plain functions on [`Engine`] that return
//! `Result<String, String>`, so it is tested natively (`cargo test -p
//! pflow-native-wasm`); the `wasm_bindgen` methods only convert the error.

use std::sync::Arc;

use pflow_native_core::session::{Session, Step};
use pflow_native_core::store::{self, Store};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

/// One editing session: a document, its marking, results and undo history,
/// with the library, systems, macros, notes and store around it.
#[wasm_bindgen]
pub struct Engine {
    sess: Session,
    /// This engine's store, never shared (see [`StoreScope`]).
    store: Arc<Store>,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Engine {
    /// An empty document, library and store.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Engine {
        Engine {
            sess: Session::default(),
            store: Arc::new(Store::memory()),
        }
    }

    /// The registry, as `command.list`: pass `""` for every command or a
    /// prefix such as `"analysis."`.
    #[wasm_bindgen(js_name = listCommands)]
    pub fn list_commands(&mut self, prefix: &str) -> Result<String, JsError> {
        self.list_commands_json(prefix)
            .map_err(|e| JsError::new(&e))
    }

    /// Runs command `id` with `args_json` (an object, or `""` for none) and
    /// returns its result as JSON.
    pub fn execute(&mut self, id: &str, args_json: &str) -> Result<String, JsError> {
        self.execute_json(id, args_json)
            .map_err(|e| JsError::new(&e))
    }

    /// Runs `[{"command", "args"}, ...]` as one undo step.
    pub fn batch(&mut self, steps_json: &str) -> Result<String, JsError> {
        self.batch_json(steps_json).map_err(|e| JsError::new(&e))
    }

    /// Replaces the document with a pflow.xyz JSON-LD document
    /// (`file.load_json`, not journaled). The library, store, macros, notes
    /// and systems stay.
    pub fn load(&mut self, jsonld: &str) -> Result<String, JsError> {
        self.load_json(jsonld).map_err(|e| JsError::new(&e))
    }

    /// Everything that outlives one document, as JSON
    /// (`session.dump_state`): the library, systems, macros, a recording in
    /// progress, notes, results and the store. The page saves it in browser
    /// storage after each command that changes any of these.
    #[wasm_bindgen(js_name = dumpState)]
    pub fn dump_state(&mut self) -> Result<String, JsError> {
        self.dump_state_json().map_err(|e| JsError::new(&e))
    }

    /// Restores what [`Engine::dump_state`] saved (`session.restore_state`)
    /// and returns how much of each part came back. The page calls it once
    /// at startup, before anything else. All or nothing: a refused state (a
    /// newer version, a damaged part) changes nothing, and the page should
    /// set that saved copy aside rather than save this empty engine's
    /// `dumpState()` over it. A result this build cannot read back (a
    /// non-finite number, saved as null) is dropped instead of refusing the
    /// rest; the summary names it (`artifacts_dropped`, `warnings`).
    #[wasm_bindgen(js_name = restoreState)]
    pub fn restore_state(&mut self, state_json: &str) -> Result<String, JsError> {
        self.restore_state_json(state_json)
            .map_err(|e| JsError::new(&e))
    }

    /// The crate's version, for the vendored bundle's lock.
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

impl Engine {
    pub fn list_commands_json(&mut self, prefix: &str) -> Result<String, String> {
        let args = if prefix.is_empty() {
            Value::Null
        } else {
            json!({ "prefix": prefix })
        };
        self.run("command.list", args)
    }

    pub fn execute_json(&mut self, id: &str, args_json: &str) -> Result<String, String> {
        let args = if args_json.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(args_json)
                .map_err(|e| format!("{id}: arguments are not JSON: {e}"))?
        };
        self.run(id, args)
    }

    pub fn batch_json(&mut self, steps_json: &str) -> Result<String, String> {
        let steps: Vec<Step> = serde_json::from_str(steps_json)
            .map_err(|e| format!("batch: steps are not [{{command, args}}]: {e}"))?;
        self.call(|s| s.batch(&steps))
    }

    pub fn load_json(&mut self, jsonld: &str) -> Result<String, String> {
        self.call(|s| s.execute_unjournaled("file.load_json", json!({ "json": jsonld })))
    }

    pub fn dump_state_json(&mut self) -> Result<String, String> {
        self.run("session.dump_state", Value::Null)
    }

    pub fn restore_state_json(&mut self, state_json: &str) -> Result<String, String> {
        self.run("session.restore_state", json!({ "state": state_json }))
    }

    fn run(&mut self, id: &str, args: Value) -> Result<String, String> {
        self.call(|s| s.execute(id, args))
    }

    /// Runs `f` with this engine's store as the thread's.
    fn call(
        &mut self,
        f: impl FnOnce(&mut Session) -> pflow_native_core::Result<Value>,
    ) -> Result<String, String> {
        let _scope = StoreScope::enter(&self.store);
        f(&mut self.sess)
            .map(|v| v.to_string())
            .map_err(|e| e.to_string())
    }
}

/// Makes an engine's store the thread's (what `store::open` returns) until
/// dropped, then puts the previous override back.
struct StoreScope(Option<Arc<Store>>);

impl StoreScope {
    fn enter(store: &Arc<Store>) -> Self {
        StoreScope(store::set_thread_store(Some(store.clone())))
    }
}

impl Drop for StoreScope {
    fn drop(&mut self) {
        store::set_thread_store(self.0.take());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/../../examples/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .expect("example")
    }

    fn sir() -> String {
        example("sir")
    }

    fn ok(e: &mut Engine, id: &str, args: Value) -> Value {
        let out = e
            .execute_json(id, &args.to_string())
            .unwrap_or_else(|err| panic!("{id}: {err}"));
        serde_json::from_str(&out).unwrap()
    }

    /// A store query's answer without the id of the table it also files.
    fn answer(e: &mut Engine, id: &str, args: Value) -> Value {
        let mut v = ok(e, id, args);
        v.as_object_mut().map(|m| m.remove("artifact"));
        v
    }

    #[test]
    fn lists_the_registry_without_the_opt_in_features() {
        let mut e = Engine::new();
        let all: Value = serde_json::from_str(&e.list_commands_json("").unwrap()).unwrap();
        let text = all.to_string();
        assert!(text.contains("analysis.invariants"), "{text:.300}");
        assert!(
            !text.contains("rnn.imitate"),
            "the browser build has no rnn.*"
        );
        let some: Value =
            serde_json::from_str(&e.list_commands_json("analysis.").unwrap()).unwrap();
        assert!(some.to_string().len() < text.len());
    }

    #[test]
    fn plays_a_game_search() {
        let mut e = Engine::new();
        let all: Value = serde_json::from_str(&e.list_commands_json("game.").unwrap()).unwrap();
        assert!(all.to_string().contains("game.move"), "{all}");
        e.load_json(&example("tic-tac-toe")).unwrap();
        let rules = json!({"sides": {"x_": 1, "o_": -1}, "reward": {"win_x": 1, "win_o": -1},
                           "auto": ["x_win_", "o_win_"]});
        let m = ok(&mut e, "game.move", rules);
        assert_eq!(m["value"], json!(0.0), "{m}");
        assert_eq!(m["exact"], true, "{m}");
        assert!(m["transition"].as_str().unwrap().starts_with("x_"), "{m}");
    }

    #[test]
    fn loads_runs_and_reports_errors() {
        let mut e = Engine::new();
        e.load_json(&sir()).unwrap();
        let inv: Value =
            serde_json::from_str(&e.execute_json("analysis.invariants", "").unwrap()).unwrap();
        assert!(inv.is_object(), "{inv}");
        let err = e.execute_json("analysis.nonesuch", "{}").unwrap_err();
        assert!(err.contains("analysis."), "{err}");
        assert!(e
            .execute_json("analysis.invariants", "{not json")
            .unwrap_err()
            .contains("not JSON"));
    }

    #[test]
    fn a_batch_is_one_undo_step_and_rolls_back() {
        let mut e = Engine::new();
        let steps = r#"[{"command":"net.add_place","args":{"x":10,"y":10}},
                        {"command":"net.add_place","args":{"x":90,"y":10}}]"#;
        e.batch_json(steps).unwrap();
        let places = |e: &mut Engine| {
            let doc: Value =
                serde_json::from_str(&e.execute_json("file.export_json", "").unwrap()).unwrap();
            doc.to_string().matches("\"@type\":\"Place\"").count()
        };
        assert_eq!(places(&mut e), 2);
        e.execute_json("edit.undo", "").unwrap();
        assert_eq!(places(&mut e), 0);
        let bad =
            r#"[{"command":"net.add_place","args":{"x":1,"y":1}},{"command":"net.nonesuch"}]"#;
        assert!(e.batch_json(bad).unwrap_err().contains("rolled back"));
        assert_eq!(places(&mut e), 0);
    }

    #[test]
    fn load_is_undoable_but_not_journaled() {
        let mut e = Engine::new();
        let out: Value = serde_json::from_str(&e.load_json(&sir()).unwrap()).unwrap();
        assert_eq!(out["places"], 3, "{out}");
        assert_eq!(ok(&mut e, "command.journal", Value::Null), json!([]));
        ok(&mut e, "edit.undo", Value::Null);
        let doc = ok(&mut e, "file.export_json", Value::Null);
        assert_eq!(doc["places"], json!({}), "undo gives back the empty net");
        let err = e.load_json("{not json").unwrap_err();
        assert!(err.starts_with("parse:"), "the registry's message: {err}");
    }

    /// Fills every part of the state: two library models with ports and a
    /// binding, a macro, a store holding two models, a link, a collection
    /// and a map, and a result with a note on it. Returns the result's id
    /// and the first stored model's CID.
    fn populate(e: &mut Engine) -> (String, String) {
        e.load_json(&sir()).unwrap();
        ok(e, "model.import", json!({"name": "sir", "example": "sir"}));
        ok(
            e,
            "model.import",
            json!({"name": "shop", "example": "coffee-shop"}),
        );
        ok(
            e,
            "system.set_port",
            json!({"model": "sir", "id": "I", "direction": "out", "kind": "People"}),
        );
        ok(
            e,
            "system.set_port",
            json!({"model": "shop", "id": "CustomerArrives", "direction": "in", "kind": "People"}),
        );
        ok(
            e,
            "system.bind",
            json!({"from_model": "sir", "from_port": "I", "to_model": "shop", "to_port": "CustomerArrives"}),
        );
        ok(
            e,
            "macro.define",
            json!({"name": "two", "steps": [
                {"command": "net.add_place", "args": {"x": 10, "y": 10}},
                {"command": "net.add_place", "args": {"x": 90, "y": 10}},
            ]}),
        );
        let a = ok(e, "store.put", json!({"title": "SIR"}))["cid"]
            .as_str()
            .unwrap()
            .to_string();
        e.load_json(&example("coffee-shop")).unwrap();
        let b = ok(e, "store.put", Value::Null)["cid"].clone();
        ok(
            e,
            "store.link",
            json!({"subject": a, "predicate": "related", "object": b}),
        );
        let col = ok(e, "store.collection_create", json!({"name": "kept"}))["id"].clone();
        ok(
            e,
            "store.collection_add",
            json!({"collection": col, "cid": a}),
        );
        ok(e, "store.map_put", json!({"data": {"k": "v"}}));
        let art = ok(
            e,
            "artifact.put",
            json!({"title": "t", "data": {"kind": "table", "columns": ["a"], "rows": [[1]]}}),
        )["artifact"]
            .as_str()
            .unwrap()
            .to_string();
        ok(e, "note.add", json!({"text": "look", "artifact": art}));
        (art, a)
    }

    /// What a reload must keep, as the commands report it. The store
    /// queries file results of their own, so they come after the results
    /// are read.
    fn observe(e: &mut Engine, art: &str, a: &str) -> Value {
        json!({
            "models": ok(e, "model.list", Value::Null),
            "sir": ok(e, "model.get", json!({"name": "sir"})),
            "systems": ok(e, "system.dump_state", Value::Null),
            "bindings": ok(e, "system.bindings", Value::Null),
            "macros": ok(e, "macro.list", Value::Null),
            "two": ok(e, "macro.get", json!({"name": "two"})),
            "notes": ok(e, "note.list", Value::Null),
            "artifact": ok(e, "artifact.get", json!({"id": art})),
            "model": ok(e, "store.get", json!({"cid": a})),
            "edges": answer(e, "store.edges", json!({"id": a})),
            "collections": answer(e, "store.collection_list", Value::Null),
            "maps": answer(e, "store.map_list", Value::Null),
            "stored": answer(e, "store.list", Value::Null),
        })
    }

    #[test]
    fn state_survives_a_reload() {
        let mut e = Engine::new();
        let (art, a) = populate(&mut e);
        let saved = e.dump_state_json().unwrap();
        let before = observe(&mut e, &art, &a);
        assert_eq!(before["stored"]["rows"].as_array().unwrap().len(), 2);
        assert_eq!(before["bindings"].as_array().unwrap().len(), 1, "{before}");

        // The page reloads: a new engine, then the saved state.
        let mut f = Engine::new();
        assert_eq!(ok(&mut f, "model.list", Value::Null), json!([]));
        let summary: Value = serde_json::from_str(&f.restore_state_json(&saved).unwrap()).unwrap();
        assert_eq!(
            summary,
            json!({"store": 6, "models": 2, "systems": {"bindings": 1, "systems": 0},
                   "macros": 1, "recording": null, "notes": 1, "artifacts": 1})
        );
        assert_eq!(f.dump_state_json().unwrap(), saved, "it dumps the same");
        assert_eq!(observe(&mut f, &art, &a), before);

        // The ids handed out next are new ones.
        let mut g = Engine::new();
        g.restore_state_json(&saved).unwrap();
        let next = ok(
            &mut g,
            "artifact.put",
            json!({"title": "u", "data": {"kind": "table", "columns": [], "rows": []}}),
        );
        assert_ne!(next["artifact"], json!(art));
        let note = ok(&mut g, "note.add", json!({"text": "new"}));
        let notes = ok(&mut g, "note.list", Value::Null);
        assert_eq!(notes.as_array().unwrap().len(), 2);
        assert_eq!(notes[0]["target"], json!({"artifact": art}));
        assert_ne!(notes[0]["id"], note["id"], "{notes}");
        // The library model opens, and the macro plays on it.
        g.load_json(&sir()).unwrap();
        ok(&mut g, "macro.play", json!({"name": "two"}));
        let doc = ok(&mut g, "file.export_json", Value::Null);
        assert_eq!(doc["places"].as_object().unwrap().len(), 5);
        ok(&mut g, "store.open", json!({"cid": a}));
        assert_eq!(
            ok(&mut g, "file.export_json", Value::Null)["places"]
                .as_object()
                .unwrap()
                .len(),
            3
        );
    }

    /// A diverging ODE's result has null where its numbers overflowed,
    /// which a result cannot read back: a reload drops that one result and
    /// keeps the library, store and everything else.
    #[test]
    fn a_diverging_result_does_not_cost_the_rest_of_the_state() {
        let mut e = Engine::new();
        let (art, a) = populate(&mut e);
        let grow = r#"[
            {"command":"net.add_place","args":{"x":0,"y":90,"id":"P","initial":[1]}},
            {"command":"net.add_transition","args":{"x":50,"y":90,"id":"grow"}},
            {"command":"net.add_arc","args":{"source":"P","target":"grow"}},
            {"command":"net.add_arc","args":{"source":"grow","target":"P","weight":[3]}},
            {"command":"sim.set_rate","args":{"id":"grow","rate":1000}}]"#;
        e.batch_json(grow).unwrap();
        let ode = ok(
            &mut e,
            "sim.ode",
            json!({"t_end": 100, "method": "euler", "dt": 0.1}),
        );
        assert_eq!(
            ode["final"]["P"],
            Value::Null,
            "it diverges: {}",
            ode["final"]
        );
        let saved = e.dump_state_json().unwrap();
        let before = observe(&mut e, &art, &a);

        let mut f = Engine::new();
        let summary: Value = serde_json::from_str(&f.restore_state_json(&saved).unwrap()).unwrap();
        assert_eq!(
            summary["artifacts_dropped"],
            json!([ode["artifact"]]),
            "{summary}"
        );
        assert_eq!(summary["models"], 2, "{summary}");
        assert_eq!(observe(&mut f, &art, &a), before);
        assert!(f
            .execute_json("artifact.get", &json!({"id": ode["artifact"]}).to_string())
            .is_err());
    }

    #[test]
    fn load_keeps_the_library_store_macros_notes_and_systems() {
        let mut e = Engine::new();
        let (art, a) = populate(&mut e);
        ok(&mut e, "note.add", json!({"text": "on the net"}));
        let kept = |e: &mut Engine| {
            json!({
                "models": ok(e, "model.list", Value::Null),
                "systems": ok(e, "system.dump_state", Value::Null),
                "macros": ok(e, "macro.list", Value::Null),
                "notes": ok(e, "note.list", Value::Null).as_array().unwrap().len(),
                "model": ok(e, "store.get", json!({"cid": a})),
                "maps": answer(e, "store.map_list", Value::Null),
                "stored": answer(e, "store.list", Value::Null),
            })
        };
        let before = kept(&mut e);
        assert_eq!(before["notes"], 2);
        e.load_json(&example("producer-consumer")).unwrap();
        assert_eq!(kept(&mut e), before);
        assert!(
            e.execute_json("artifact.get", &json!({"id": art}).to_string())
                .is_err(),
            "the old document's results go"
        );
    }

    #[test]
    fn a_recording_survives_a_reload_without_the_document_loads() {
        let mut e = Engine::new();
        ok(&mut e, "macro.record", Value::Null);
        e.load_json(&sir()).unwrap();
        ok(&mut e, "net.add_place", json!({"x": 10, "y": 10}));
        e.load_json(&sir()).unwrap();
        let saved = e.dump_state_json().unwrap();
        let state: Value = serde_json::from_str(&saved).unwrap();
        assert_eq!(state["recording"].as_array().unwrap().len(), 1, "{state}");

        let mut f = Engine::new();
        f.restore_state_json(&saved).unwrap();
        f.load_json(&sir()).unwrap();
        ok(&mut f, "net.add_place", json!({"x": 90, "y": 10}));
        ok(&mut f, "macro.stop", json!({"name": "m"}));
        let steps = ok(&mut f, "macro.get", json!({"name": "m"}));
        let commands: Vec<&str> = steps
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["command"].as_str().unwrap())
            .collect();
        assert_eq!(commands, ["net.add_place", "net.add_place"]);
    }

    #[test]
    fn two_engines_do_not_share_a_store_and_the_thread_store_comes_back() {
        let outer = Arc::new(Store::memory());
        let prev = store::set_thread_store(Some(outer.clone()));
        let mut e = Engine::new();
        let mut f = Engine::new();
        e.load_json(&sir()).unwrap();
        ok(&mut e, "store.put", Value::Null);
        assert_eq!(ok(&mut e, "store.info", Value::Null)["models"], 1);
        assert_eq!(ok(&mut f, "store.info", Value::Null)["models"], 0);
        assert!(Arc::ptr_eq(&store::open(), &outer));
        assert!(outer.snapshot().unwrap().models.is_empty());
        store::set_thread_store(prev);
    }

    #[test]
    fn a_refused_state_changes_nothing() {
        let mut e = Engine::new();
        let (art, a) = populate(&mut e);
        let before = observe(&mut e, &art, &a);
        let saved = e.dump_state_json().unwrap();

        let mut newer: Value = serde_json::from_str(&saved).unwrap();
        newer["version"] = json!(2);
        newer["models"] = json!({});
        let err = e.restore_state_json(&newer.to_string()).unwrap_err();
        assert!(err.contains("version 2"), "{err}");
        assert!(e
            .restore_state_json("{not json")
            .unwrap_err()
            .contains("not JSON"));
        assert!(e
            .restore_state_json(r#"{"models": {}}"#)
            .unwrap_err()
            .contains("no format"));
        // Good parts beside a damaged one: none is applied.
        let mut damaged: Value = serde_json::from_str(&saved).unwrap();
        damaged["models"] = json!({});
        damaged["store"]["blobs"] = json!({});
        let first = damaged["artifacts"]["items"][0].clone();
        damaged["artifacts"]["items"]
            .as_array_mut()
            .unwrap()
            .push(first);
        let err = e.restore_state_json(&damaged.to_string()).unwrap_err();
        assert!(err.contains("repeated"), "{err}");
        assert_eq!(e.dump_state_json().unwrap(), saved);
        assert_eq!(observe(&mut e, &art, &a), before);
    }

    /// The composition views' commands (`bundle.*`, `plan.*`) are in the
    /// browser registry like every other: listed, run, and their edits
    /// undone, with no document loaded.
    #[test]
    fn composition_and_plan_commands_run_in_the_browser() {
        let mut e = Engine::new();
        let listed: Value =
            serde_json::from_str(&e.list_commands_json("bundle.").unwrap()).unwrap();
        assert!(listed.to_string().contains("bundle.candidates"));
        let listed: Value = serde_json::from_str(&e.list_commands_json("plan.").unwrap()).unwrap();
        assert!(listed.to_string().contains("plan.slot_candidates"));

        let open = ok(&mut e, "bundle.open", Value::Null);
        assert_eq!(open["instances"], json!(["orders", "workers", "payment"]));
        let c = ok(
            &mut e,
            "bundle.candidates",
            json!({"from": "orders:dequeue"}),
        );
        assert!(c["lit"].as_u64().unwrap() > 0);
        let flat = ok(&mut e, "bundle.flatten", Value::Null);
        assert_eq!(
            flat["fused_groups"]["fused:payment/commit+workers/release"],
            json!(["payment/commit", "workers/release"])
        );
        let run = ok(&mut e, "bundle.run", json!({"scenario": "refund"}));
        assert_eq!(run["completed"], true);
        let err = e
            .execute_json(
                "bundle.add_link",
                r#"{"kind": "event", "from": "orders:dequeue", "to": "workers:acquire"}"#,
            )
            .unwrap_err();
        assert_eq!(err, "[PAGE_DUPLICATE] already linked");

        let plan = ok(&mut e, "plan.open", Value::Null);
        assert_eq!(plan["check"]["ok"], true);
        let err = e
            .execute_json(
                "plan.add_piece",
                r#"{"piece": "static_eval", "slot": "heuristics"}"#,
            )
            .unwrap_err();
        assert!(err.contains("can change the value"), "{err}");
        ok(&mut e, "edit.undo", Value::Null);
        assert!(e.execute_json("plan.state", "").is_err());
    }
}

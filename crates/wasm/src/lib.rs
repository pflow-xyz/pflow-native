//! pflow-native-core for the browser.
//!
//! pflow.xyz loads this module lazily, when the user runs a command its JS
//! engines lack (analysis, verify, reach, diagnose, ...). The contract is
//! the registry's, as the CLI and MCP see it, over JSON strings:
//!
//! - [`Engine::list_commands`]: ids, argument schemas, help, enabled now.
//! - [`Engine::execute`]: one command, `Session::execute`.
//! - [`Engine::batch`]: several, as one undo step that rolls back on failure.
//! - [`Engine::load`]: replace the document with pflow.xyz JSON-LD.
//!
//! Every call goes through `Session::execute`, as the ground rules require;
//! this crate adds no behaviour, only the JSON boundary. Errors come back as
//! thrown JS errors carrying the registry's message.
//!
//! The logic lives in plain functions on [`Engine`] that return
//! `Result<String, String>`, so it is tested natively (`cargo test -p
//! pflow-native-wasm`); the `wasm_bindgen` methods only convert the error.

use pflow_native_core::session::{Session, Step};
use serde_json::{json, Value};
use wasm_bindgen::prelude::*;

/// One editing session: a document, its marking, results and undo history.
#[wasm_bindgen]
pub struct Engine {
    sess: Session,
}

impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}

#[wasm_bindgen]
impl Engine {
    /// An empty document.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Engine {
        Engine {
            sess: Session::default(),
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
    /// (`file.load_json`).
    pub fn load(&mut self, jsonld: &str) -> Result<String, JsError> {
        self.load_json(jsonld).map_err(|e| JsError::new(&e))
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
        let out = self.sess.batch(&steps).map_err(|e| e.to_string())?;
        Ok(out.to_string())
    }

    pub fn load_json(&mut self, jsonld: &str) -> Result<String, String> {
        self.run("file.load_json", json!({ "json": jsonld }))
    }

    fn run(&mut self, id: &str, args: Value) -> Result<String, String> {
        self.sess
            .execute(id, args)
            .map(|v| v.to_string())
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sir() -> String {
        std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../examples/sir.json"
        ))
        .expect("sir.json")
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
}

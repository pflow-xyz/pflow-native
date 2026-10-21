//! What the outbound layer can see: which credentials are configured and
//! where the data directory is. (Remote commands themselves live with
//! their services.)

use serde_json::{json, Value};

use crate::outbound;
use crate::registry::{always, CommandSpec};
use crate::session::Session;
use crate::Result;

/// Credential names the remote bridges read (env var or credentials.json).
pub const CREDENTIALS: &[&str] = &["PFLOW_XYZ_TOKEN", "SIM_PFLOW_TOKEN", "GITHUB_TOKEN"];

pub const SET: &[CommandSpec] = &[CommandSpec {
    id: "remote.credentials",
    label: "Remote Credentials",
    help: "Which remote credentials are configured (names and source only, never values) and where \
           pflow-native keeps its data. Set them as environment variables or in <data_dir>/credentials.json.",
    menu: "",
    shortcut: None,
    args: &[],
    enabled: always,
    run: credentials,
    journal: false,
}];

fn credentials(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({
        "data_dir": outbound::data_dir(),
        "credentials": outbound::credential_status(CREDENTIALS),
        "policy": "remote writes preview their request unless called with \"send\": true",
    }))
}

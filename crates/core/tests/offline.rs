//! A build without the `services` feature (`make build-offline`) has no
//! command that talks to pflow.xyz, sim.pflow.xyz or GitHub.

#![cfg(not(feature = "services"))]

use pflow_native_core::Session;

#[test]
fn no_service_commands_without_the_feature() {
    let s = Session::default();
    let ids: Vec<&str> = s.registry().commands().iter().map(|c| c.id).collect();
    for id in &ids {
        assert!(
            !id.starts_with("remote.") && !id.starts_with("auth."),
            "{id} is a service command"
        );
    }
    for gone in ["file.open_cid", "file.permalink"] {
        assert!(!ids.contains(&gone), "{gone} needs pflow.xyz");
    }
    // Local work is all still there, including opening a URL the user gives.
    for kept in [
        "file.open",
        "file.open_url",
        "file.cid",
        "sim.ode",
        "store.put",
    ] {
        assert!(ids.contains(&kept), "{kept} missing");
    }
}

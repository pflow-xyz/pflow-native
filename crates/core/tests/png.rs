//! export.png (feature `png`) and view.show_artifact.

use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn show_artifact_checks_the_id() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let ode = run(&mut s, "sim.ode", json!({}));
    let r = run(&mut s, "view.show_artifact", json!({"id": ode["artifact"]}));
    assert_eq!(r["ui"], false);
    assert!(s.ui_requests.is_empty(), "headless: nothing for a UI to do");
    s.has_ui = true;
    run(&mut s, "view.show_artifact", json!({"id": ode["artifact"]}));
    assert_eq!(s.ui_requests.len(), 1);
    assert!(s
        .execute("view.show_artifact", json!({"id": "a999"}))
        .is_err());
}

#[cfg(feature = "png")]
#[test]
fn export_png_renders_the_net_and_results() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let net = run(&mut s, "export.png", json!({"width": 400}));
    assert_eq!(net["rendered"], "net");
    assert_eq!(net["width"], 400);
    let b64 = net["png_base64"].as_str().unwrap();
    assert!(b64.starts_with("iVBORw0KGgo"), "a PNG signature");
    let ode = run(&mut s, "sim.ode", json!({}));
    let dir = std::env::temp_dir().join(format!("pflow-native-png-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ode.png");
    let r = run(
        &mut s,
        "export.png",
        json!({"id": ode["artifact"], "path": path.to_string_lossy()}),
    );
    assert_eq!(r["width"], 1200);
    assert!(std::fs::read(&path).unwrap().starts_with(b"\x89PNG"));
    // A report has no picture; bad widths are refused.
    let inv = run(&mut s, "analysis.validate", json!({}));
    let report = inv["report"]
        .as_str()
        .or(inv["artifact"].as_str())
        .unwrap()
        .to_string();
    let e = s
        .execute("export.png", json!({"id": report}))
        .unwrap_err()
        .to_string();
    assert!(e.contains("no picture"), "{e}");
    assert!(s.execute("export.png", json!({"width": 10})).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

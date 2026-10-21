use pflow_native_core::commands::file::EXAMPLES;
use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn every_gallery_example_loads_fires_and_simulates() {
    assert_eq!(EXAMPLES.len(), 7, "the six pflow.xyz gallery nets + SIR");
    for (name, _) in EXAMPLES {
        let mut s = Session::default();
        run(&mut s, "file.open_example", json!({ "name": name }));
        assert!(!s.doc.places.is_empty(), "{name}");
        let en = run(&mut s, "sim.enabled", Value::Null);
        if let Some(t) = en.as_array().and_then(|a| a.first()) {
            run(&mut s, "sim.fire", json!({ "id": t }));
        }
        run(&mut s, "sim.ode", json!({ "t_end": 1, "points": 20 }));
        assert!(!s.is_dirty(), "{name}: opening an example is not an edit");
    }
}

#[test]
fn rename_rewrites_arcs_marking_and_rates() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(&mut s, "sim.set_rate", json!({"id": "infect", "rate": 0.5}));
    run(
        &mut s,
        "net.rename",
        json!({"id": "infect", "new_id": "contact"}),
    );
    assert!(s.doc.transition("contact").is_some());
    assert!(s
        .doc
        .arcs
        .iter()
        .all(|a| a.source != "infect" && a.target != "infect"));
    assert_eq!(s.rates.get("contact"), Some(&0.5));
    run(&mut s, "net.rename", json!({"id": "S", "new_id": "Sus"}));
    assert_eq!(s.marking["Sus"], vec![999]);
    assert!(s
        .execute("net.rename", json!({"id": "I", "new_id": "R"}))
        .is_err());
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.doc.place("S").is_some());
}

#[test]
fn copy_paste_cut_duplicate() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(&mut s, "edit.select", json!({"ids": ["S", "infect", "I"]}));
    run(&mut s, "edit.copy", Value::Null);
    let r = run(&mut s, "edit.paste", json!({"dx": 0, "dy": 200}));
    assert_eq!(r["pasted"].as_array().unwrap().len(), 3);
    assert_eq!(s.doc.places.len(), 5);
    // S->infect, I->infect, infect->I copied; I->recover not (recover not copied).
    assert_eq!(s.doc.arcs.len(), 8);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.doc.places.len(), 3);
    run(&mut s, "edit.select", json!({"ids": ["R"]}));
    run(&mut s, "edit.cut", Value::Null);
    assert!(s.doc.place("R").is_none());
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.doc.place("R").is_some(), "cut is one undo step");
    run(&mut s, "edit.select", json!({"ids": ["R"]}));
    run(&mut s, "edit.duplicate", Value::Null);
    assert_eq!(s.doc.places.len(), 4);
}

#[test]
fn colors_and_details() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(&mut s, "net.set_colors", json!({"token": ["black", "red"]}));
    assert_eq!(s.doc.token[1], "https://pflow.xyz/tokens/red");
    assert_eq!(s.doc.place("S").unwrap().initial, vec![999.0, 0.0]);
    run(
        &mut s,
        "net.add_tokens",
        json!({"id": "I", "color": 1, "delta": 2}),
    );
    assert_eq!(s.marking["I"], vec![1, 2]);
    run(
        &mut s,
        "net.set_details",
        json!({"name": "SIR", "description": "epidemic"}),
    );
    assert_eq!(s.doc.to_value()["name"], "SIR");
    run(&mut s, "net.set_details", json!({"name": ""}));
    assert!(s.doc.to_value().get("name").is_none());
}

#[test]
fn live_json_edit_keeps_marking_and_view() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(&mut s, "sim.fire", json!({"id": "recover"}));
    run(&mut s, "view.zoom", json!({"factor": 2}));
    let view = s.view;
    let mut v = s.doc.to_value();
    v["places"]["R"]["label"] = json!("Removed");
    run(&mut s, "file.load_json", json!({"json": v, "live": true}));
    assert_eq!(s.doc.place("R").unwrap().label.as_deref(), Some("Removed"));
    assert_eq!(s.marking["R"], vec![1], "token game survives a source edit");
    assert_eq!(s.view, view);
}

#[test]
fn settings_toggle() {
    let mut s = Session::default();
    assert!(!s.view.settings.json_pane);
    run(&mut s, "view.toggle_json", Value::Null);
    assert!(s.view.settings.json_pane);
    run(&mut s, "view.set", json!({"theme": "light", "snap": false}));
    run(&mut s, "net.add_place", json!({"x": 13, "y": 17}));
    assert_eq!(s.doc.places[0].x, 13.0, "snap off");
    assert!(s.execute("view.set", json!({"theme": "neon"})).is_err());
}

#[test]
fn producer_consumer_gallery_net_runs_as_on_pflow_xyz() {
    let mut s = Session::default();
    run(
        &mut s,
        "file.open_example",
        json!({"name": "producer-consumer"}),
    );
    let en = run(&mut s, "sim.enabled", Value::Null);
    assert_eq!(
        en,
        json!(["produce"]),
        "the weight-0 read of `produced` is inert, as in pflow.xyz"
    );
    run(&mut s, "sim.fire", json!({"id": "produce"}));
    assert_eq!(s.marking["buffer"], vec![1]);
}

#[test]
fn session_rates_survive_save_and_open() {
    let dir = std::env::temp_dir().join(format!("pn-rates-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let with = dir.join("with.json");
    let without = dir.join("without.json");
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(
        &mut s,
        "sim.set_rate",
        json!({"id": "infect", "rate": 0.004}),
    );
    run(&mut s, "file.save", json!({"path": with.to_str().unwrap()}));
    run(
        &mut s,
        "file.save",
        json!({"path": without.to_str().unwrap(), "rates": false}),
    );
    assert!(std::fs::read_to_string(&with)
        .unwrap()
        .contains("\"rate\": 0.004"));
    assert!(!std::fs::read_to_string(&without)
        .unwrap()
        .contains("\"rate\""));

    let mut t = Session::default();
    run(&mut t, "file.open", json!({"path": with.to_str().unwrap()}));
    assert_eq!(t.rates.get("infect"), Some(&0.004));
    assert!(
        t.doc
            .transition("infect")
            .unwrap()
            .extra
            .get("rate")
            .is_none(),
        "the rate lives in the session, not the document"
    );
    run(
        &mut t,
        "file.open",
        json!({"path": without.to_str().unwrap()}),
    );
    run(&mut t, "edit.undo", Value::Null);
    assert_eq!(
        t.rates.get("infect"),
        Some(&0.004),
        "undoing an open restores its rates"
    );
    std::fs::remove_dir_all(dir).ok();
}

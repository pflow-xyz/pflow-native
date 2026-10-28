//! The token game on the flat composition (`bundle.game`, `bundle.fire`,
//! `bundle.reset_marking`, `bundle.scenario`, `bundle.step`): firing through
//! pflow-metamodel, scenario stepping agreeing with `bundle.run`, undo, and
//! a changed net starting a new game.

use pflow_native_core::composition as comp;
use pflow_native_core::Session;
use serde_json::{json, Value};

fn open() -> Session {
    let mut s = Session::default();
    s.execute("bundle.open", json!({})).unwrap();
    s
}

fn game(s: &mut Session) -> Value {
    s.execute("bundle.game", Value::Null).unwrap()
}

#[test]
fn starts_at_the_initial_marking_with_three_enabled() {
    let mut s = open();
    let g = game(&mut s);
    assert_eq!(g["ok"], true);
    assert_eq!(g["fired"], json!([]));
    assert_eq!(g["transitions"].as_array().unwrap().len(), 9);
    assert_eq!(g["enabled"].as_array().unwrap().len(), 3);
    let flat = comp::flatten(&s.composition.as_ref().unwrap().bundle).unwrap();
    let want: std::collections::BTreeMap<String, i64> =
        flat.model.initial_marking().into_iter().collect();
    assert_eq!(g["marking"], json!(want));
    assert!(g["scenario"].is_null());
}

#[test]
fn fire_goes_through_the_flat_net_and_refuses_with_the_reason() {
    let mut s = open();
    let err = s
        .execute("bundle.fire", json!({"transition": "payment/do_charge"}))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("not enabled") || err.contains("needs"),
        "{err}"
    );
    assert_eq!(
        game(&mut s)["fired"],
        json!([]),
        "a refusal changes nothing"
    );
    // a member names the fused flat transition it is in
    let g = s
        .execute("bundle.fire", json!({"transition": "orders/dequeue"}))
        .unwrap();
    let id = "fused:orders/dequeue+payment/do_reserve+workers/acquire";
    assert_eq!(g["fired"], json!([id]));
    assert_eq!(g["marking"]["orders/items"], 1);
    assert_eq!(g["marking"]["workers/available"], 1);
    assert_eq!(g["marking"]["workers/in_use"], 1);
    assert!(s
        .execute("bundle.fire", json!({"transition": "nope"}))
        .is_err());
}

#[test]
fn stepping_a_scenario_agrees_with_bundle_run() {
    let mut s = open();
    for sc in comp::scenarios() {
        s.execute("bundle.scenario", json!({"id": sc.id})).unwrap();
        let g = game(&mut s);
        assert_eq!(g["scenario"]["shown"], 0);
        assert_eq!(g["scenario"]["done"], false);
        let mut last = Value::Null;
        for _ in 0..sc.script.len() + 1 {
            last = s.execute("bundle.step", Value::Null).unwrap();
        }
        let run = s.execute("bundle.run", json!({"scenario": sc.id})).unwrap();
        let g = &last;
        assert_eq!(g["scenario"]["done"], true, "{}", sc.id);
        assert_eq!(g["scenario"]["completed"], run["completed"], "{}", sc.id);
        assert_eq!(
            g["scenario"]["stopped_step"].as_str(),
            run["stopped_step"].as_str(),
            "{}",
            sc.id
        );
        assert_eq!(g["marking"], run["final_marking"], "{}", sc.id);
        let got: Vec<_> = g["scenario"]["expect"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["place"].clone(), e["actual"].clone(), e["pass"].clone()))
            .collect();
        let want: Vec<_> = run["expect"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["place"].clone(), e["actual"].clone(), e["pass"].clone()))
            .collect();
        assert_eq!(got, want, "{}", sc.id);
    }
}

#[test]
fn step_all_runs_the_rest_and_the_refund_leaks_a_worker() {
    let mut s = open();
    s.execute("bundle.scenario", json!({"id": "refund"}))
        .unwrap();
    let g = s.execute("bundle.step", json!({"all": true})).unwrap();
    assert_eq!(g["scenario"]["completed"], true);
    let e = g["scenario"]["expect"].as_array().unwrap();
    assert_eq!(e.len(), 3);
    assert_eq!(e[2]["says"], "the worker returns to the pool");
    assert_eq!(e[2]["pass"], false, "release is fused only with commit");
    // conservation still holds: a stuck token keeps the sum
    let k = g["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["id"] == "workers/pool_conservation")
        .unwrap();
    assert_eq!(k["holds"], true);
    assert_eq!(k["lhs"], "1 + 1");
}

#[test]
fn a_manual_firing_leaves_the_scenario() {
    let mut s = open();
    s.execute("bundle.scenario", json!({"id": "commit"}))
        .unwrap();
    s.execute("bundle.step", Value::Null).unwrap();
    assert!(!game(&mut s)["scenario"].is_null());
    // the commit script's second step is enabled now; firing it by hand
    let g = s
        .execute("bundle.fire", json!({"transition": "payment/do_charge"}))
        .unwrap();
    assert!(g["scenario"].is_null());
    assert_eq!(g["fired"].as_array().unwrap().len(), 2);
}

#[test]
fn second_order_stops_at_the_fused_dequeue() {
    let mut s = open();
    s.execute("bundle.scenario", json!({"id": "second-order"}))
        .unwrap();
    let g = s.execute("bundle.step", json!({"all": true})).unwrap();
    assert_eq!(g["scenario"]["completed"], false);
    assert_eq!(g["scenario"]["stopped_step"], "orders/dequeue");
    assert_eq!(g["scenario"]["steps"].as_array().unwrap().len(), 4);
    // stepping a done scenario is a no-op
    let g2 = s.execute("bundle.step", Value::Null).unwrap();
    assert_eq!(g2["marking"], g["marking"]);
    assert!(s.execute("bundle.scenario", json!({"id": "nope"})).is_err());
}

#[test]
fn reset_and_undo_and_a_changed_net() {
    let mut s = open();
    let initial = game(&mut s)["marking"].clone();
    s.execute("bundle.fire", json!({"transition": "orders/dequeue"}))
        .unwrap();
    assert_ne!(game(&mut s)["marking"], initial);
    s.execute("bundle.reset_marking", Value::Null).unwrap();
    assert_eq!(game(&mut s)["marking"], initial);
    assert_eq!(game(&mut s)["fired"], json!([]));
    // undo brings the firing back, as sim.fire's does
    s.execute("edit.undo", Value::Null).unwrap();
    assert_eq!(game(&mut s)["fired"].as_array().unwrap().len(), 1);
    // changing the net (a link) starts a new game at its own initial marking
    s.execute("bundle.clear_links", Value::Null).unwrap();
    let g = game(&mut s);
    assert_eq!(g["fired"], json!([]));
    assert_eq!(g["transitions"].as_array().unwrap().len(), 12);
}

#[test]
fn constraints_are_evaluated_at_the_current_marking() {
    use comp::game::evaluate;
    let mk = [("a".to_string(), 2), ("b".to_string(), 0)]
        .into_iter()
        .collect();
    let e = evaluate(r#"tokens("a") + tokens("b") + 1 == 3"#, &mk).unwrap();
    assert!(e.holds);
    assert_eq!(e.lhs, "2 + 0 + 1");
    assert!(!evaluate(r#"tokens("a") >= 3"#, &mk).unwrap().holds);
    assert!(evaluate("all of them", &mk).is_none());
}

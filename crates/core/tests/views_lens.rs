//! The lens test (crate::lens), ported from pflow-views-prototype
//! `tests/lensfind.test.mjs`. The expectations for the two repairs are
//! what go-pflow c802f34 itself gave for the same wirings (recorded in the
//! prototype's commit message, not in a receipt).

use pflow_compose::{Bundle, Link, Subnet};
use pflow_native_core::composition as comp;
use pflow_native_core::lens::{self, Analysis, Run};
use pflow_native_core::Session;
use serde_json::{json, Value};

fn subs() -> Vec<Subnet> {
    ["orders", "workers", "payment"]
        .iter()
        .map(|id| comp::build(comp::instance(id).unwrap()).unwrap())
        .collect()
}

fn flat(subnets: &[&Subnet], links: &[Link]) -> Result<comp::Flat, String> {
    let mut b = Bundle::new("lens");
    for s in subnets {
        b.add_subnet((*s).clone());
    }
    b.links = links.to_vec();
    comp::flatten(&b)
}

fn pairs(s: &Subnet) -> Vec<String> {
    let mut v: Vec<String> = lens::backward_pairs(s)
        .iter()
        .map(|p| format!("{}->{}:{}", p.t, p.u, p.residuals.join("+")))
        .collect();
    v.sort();
    v
}

fn run<'a>(a: &'a Analysis, id: &str) -> &'a Run {
    a.runs.iter().find(|r| r.id == id).unwrap()
}

#[test]
fn pair_rule_finds_inverses_and_no_control_place() {
    let s = subs();
    assert_eq!(
        pairs(&s[2]),
        [
            "do_charge->undo_charge:charge_done+merchant",
            "do_reserve->undo_reserve:reserve_done",
            "undo_charge->do_charge:customer"
        ]
    );
    assert_eq!(
        pairs(&s[1]),
        ["acquire->release:in_use", "release->acquire:available"]
    );
    assert_eq!(
        pairs(&s[0]),
        ["dequeue->enqueue:slots", "enqueue->dequeue:items"]
    );
}

#[test]
fn strict_no_boundary_and_backward_maps_do_not_fire_together() {
    let s = subs();
    let refs: Vec<&Subnet> = s.iter().collect();
    let l = comp::default_links();
    let a = lens::analyse(&refs, &l, &flat(&refs, &l));
    assert!(!a.holds);
    assert_eq!(
        json!(a.legal),
        json!({"data": 0, "event": 72, "guard": 66, "token": 0})
    );
    assert_eq!(a.glued, 0);
    let first = &a.steps[0];
    assert_eq!(
        first.flat,
        "fused:orders/dequeue+payment/do_reserve+workers/acquire"
    );
    let mut back: Vec<&str> = first.back.iter().map(|b| b.u.as_str()).collect();
    back.sort();
    assert_eq!(back, ["orders/enqueue", "payment/undo_reserve"]);
    assert!(first.split);
    let trips: Vec<String> = a
        .round_trips
        .iter()
        .map(|r| format!("{}->{}", r.t, r.u))
        .collect();
    assert_eq!(trips, ["workers/acquire->workers/release"]);
    assert!(a.steps[1].composes); // do_charge / undo_charge, inside the saga
    let refund = run(&a, "refund");
    let avail = refund
        .expect
        .iter()
        .find(|e| e.place == "workers/available")
        .unwrap();
    assert_eq!(avail.actual, 1);
}

#[test]
fn repairs_add_deadlocks_move_leaks_on_commit_as_go_pflow() {
    let s = subs();
    let refs: Vec<&Subnet> = s.iter().collect();
    let l = comp::default_links();
    let a = lens::analyse(&refs, &l, &flat(&refs, &l));
    let patterns = [
        ("orders", "queue"),
        ("workers", "resource-pool"),
        ("payment", "saga"),
    ];
    let fixes = lens::try_fixes(&refs, &patterns, &l, &a);
    let find = |how: &str| {
        fixes
            .iter()
            .find(|x| x.how == how && x.u == "workers/release")
            .unwrap()
    };
    let add = find("add");
    assert!(add.legal);
    let added = add.result.as_ref().unwrap();
    let dead = added
        .steps
        .iter()
        .find(|s| s.dead_forward.is_some())
        .unwrap();
    assert_eq!(
        dead.flat,
        "fused:payment/commit+payment/undo_reserve+workers/release"
    );
    assert_eq!(
        dead.dead_forward.as_ref().unwrap().constraint,
        "payment/saga_control"
    );
    assert_eq!(
        run(added, "commit").stopped_step.as_deref(),
        Some("payment/commit")
    );
    assert_eq!(
        run(added, "refund").stopped_step.as_deref(),
        Some("payment/undo_reserve")
    );
    let moved = find("move").result.as_ref().unwrap();
    assert!(run(moved, "refund").expect.iter().all(|e| e.pass));
    assert_eq!(
        run(moved, "commit")
            .expect
            .iter()
            .find(|e| e.place == "workers/available")
            .unwrap()
            .actual,
        1
    );
    assert!(!fixes
        .iter()
        .any(|x| x.result.as_ref().is_some_and(|r| r.holds)));
}

#[test]
fn dead_by_reads_only_exclusive_constraints() {
    let s = subs();
    let refs: Vec<&Subnet> = s.iter().collect();
    let f = flat(&refs, &comp::default_links()).unwrap();
    for t in &f.model.transitions {
        assert_eq!(lens::dead_by(&f.model, &t.id), None, "{}", t.id);
    }
}

#[test]
fn lens_command_reports_and_stores() {
    let mut s = Session::default();
    s.execute("bundle.open", Value::Null).unwrap();
    let out = s.execute("bundle.lens", Value::Null).unwrap();
    assert_eq!(out["analysis"]["holds"], false);
    assert!(!out["fixes"].as_array().unwrap().is_empty());
    assert!(out["artifact"].is_string());
    let out = s.execute("bundle.lens", json!({"fixes": false})).unwrap();
    assert_eq!(out["fixes"], json!([]));
}

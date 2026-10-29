//! The ten composition views on a document that is not the orders example: a raw Bundle
//! (pflow-xyz's `cafe.bundle.json`), with and without a content document that declares scenarios.
//! Each view draws what the model carries and says what it lacks; none draws another model's
//! scenarios, or claims there are no links when links exist that it cannot anchor.

use pflow_native::compviews::{self, Headless};
use pflow_native_core::Session;
use serde_json::{json, Value};

const CAFE: &str = include_str!("../../core/tests/fixtures/bundle/cafe.bundle.json");
const CAFE_CONTENT: &str = include_str!("../../core/tests/fixtures/content/cafe.content.json");
const VIEWS: [&str; 10] = [
    "cospan",
    "spiders",
    "operad",
    "string",
    "flat",
    "boundaries",
    "lens",
    "incidence",
    "process",
    "laws",
];

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A session with the cafe Bundle open, bare or with its content document.
fn cafe(with_content: bool) -> Session {
    let mut s = Session::default();
    let mut args = json!({"document": serde_json::from_str::<Value>(CAFE).unwrap()});
    if with_content {
        args["content"] = serde_json::from_str(CAFE_CONTENT).unwrap();
    }
    run(&mut s, "bundle.load", args);
    s
}

fn ids(h: &Headless, kind: &str) -> Vec<String> {
    h.manifest["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == kind)
        .map(|e| e["id"].as_str().unwrap().to_string())
        .collect()
}

fn said(h: &Headless, needle: &str) -> bool {
    h.texts.iter().any(|t| t.contains(needle)) || h.notes.iter().any(|(_, t)| t.contains(needle))
}

#[test]
fn the_process_view_draws_the_scenarios_the_model_declares() {
    let s = cafe(true);
    let h = compviews::headless_frame(&s, "process", "orders");
    assert_eq!(ids(&h, "run"), ["espresso-order", "latte-order"]);
    assert!(said(&h, "espresso-order"));
    for orders_only in ["commit", "refund", "charge-fails", "second-order"] {
        assert!(
            !said(&h, orders_only),
            "{orders_only} is the orders example's"
        );
    }
    assert!(
        !h.notes.iter().any(|(_, t)| t.contains("does not fire")),
        "no run fails for want of another model's members: {:?}",
        h.notes
    );
}

#[test]
fn the_flat_view_offers_the_declared_scenarios_to_step() {
    let s = cafe(true);
    let h = compviews::headless_frame(&s, "flat", "orders");
    assert_eq!(ids(&h, "scenario"), ["espresso-order", "latte-order"]);
}

#[test]
fn a_model_with_no_declared_scenario_has_no_run_to_draw_and_says_so() {
    let s = cafe(false);
    for view in ["process", "flat"] {
        let h = compviews::headless_frame(&s, view, "orders");
        assert!(ids(&h, "run").is_empty(), "{view}");
        assert!(ids(&h, "scenario").is_empty(), "{view}");
    }
    let h = compviews::headless_frame(&s, "process", "orders");
    assert!(said(&h, "no scenario is declared"), "{:?}", h.texts);
    assert!(!said(&h, "refund"));
}

#[test]
fn the_orders_example_still_draws_its_four_scenarios() {
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    let h = compviews::headless_frame(&s, "process", "orders");
    // The manifest is sorted by (kind, id).
    assert_eq!(
        ids(&h, "run"),
        ["charge-fails", "commit", "refund", "second-order"]
    );
}

/// cafe's seven links end on places and transitions, not ports. The views that anchor links by port
/// cannot draw them, and must not say there are none.
#[test]
fn views_that_anchor_links_by_port_say_so_instead_of_claiming_there_are_none() {
    let s = cafe(true);
    let st = run(&mut cafe(true), "bundle.state", Value::Null);
    let links = st["links"].as_array().unwrap();
    assert_eq!(links.len(), 7);
    assert!(
        links
            .iter()
            .all(|l| l["from"]["port"] == "" || l["to"]["port"] == ""),
        "none of cafe's links is between two ports"
    );
    for view in ["cospan", "spiders", "operad", "string"] {
        let h = compviews::headless_frame(&s, view, "orders");
        assert!(
            h.notes
                .iter()
                .any(|(_, t)| t.contains("end on places or transitions, not ports")),
            "{view}: {:?}",
            h.notes
        );
        for false_claim in [
            "no links",
            "no two links share a member",
            "the composite is exactly the monoidal product",
        ] {
            assert!(!said(&h, false_claim), "{view} says {false_claim:?}");
        }
    }
    let h = compviews::headless_frame(&s, "spiders", "orders");
    assert!(said(&h, "not ports: not drawn here"), "{:?}", h.texts);
    let h = compviews::headless_frame(&s, "string", "orders");
    assert!(said(&h, "not ports, and"), "{:?}", h.texts);
}

/// The views that read the flat net draw what the links fuse, and agree with each other.
#[test]
fn the_views_that_read_the_flat_net_count_the_links_the_port_views_cannot_anchor() {
    let s = cafe(true);
    let flat = compviews::headless_frame(&s, "boundaries", "orders");
    assert!(
        !said(&flat, "from 0 binary event links"),
        "{:?}",
        flat.texts
    );
    assert!(said(&flat, "binary event link"), "{:?}", flat.texts);
}

#[test]
fn every_view_draws_a_loaded_bundle_without_panicking() {
    for with_content in [false, true] {
        let s = cafe(with_content);
        for view in VIEWS {
            let h = compviews::headless_frame(&s, view, "orders");
            assert!(h.manifest["elements"].is_array(), "{view}");
        }
    }
}

/// A model that flattens but declares no `commit` scenario has no lens verdict; the view says that,
/// not "the composition does not flatten", and does not leave the canvas blank.
#[test]
fn the_lens_view_of_a_model_with_no_commit_scenario_says_why_there_is_no_verdict() {
    let s = cafe(false);
    let h = compviews::headless_frame(&s, "lens", "orders");
    assert!(!said(&h, "does not flatten"), "{:?} {:?}", h.texts, h.notes);
    assert!(said(&h, "declares none"), "{:?}", h.notes);
    assert!(
        h.texts.iter().any(|t| t.contains("declares none")),
        "drawn in the canvas too: {:?}",
        h.texts
    );
    // And a composition that does not flatten is still said to be that.
    let mut s = Session::default();
    run(&mut s, "bundle.open", json!({}));
    run(&mut s, "content.clear", json!({}));
    let h = compviews::headless_frame(&s, "lens", "orders");
    assert!(
        said(&h, "declares none"),
        "an edited orders with its content cleared: {:?}",
        h.notes
    );
}

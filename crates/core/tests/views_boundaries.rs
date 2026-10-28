//! `bundle.boundaries`: the event classes with the composite's and each
//! member's own enablement at the flat initial marking.

use pflow_native_core::Session;
use serde_json::{json, Value};

fn open() -> Session {
    let mut s = Session::default();
    s.execute("bundle.open", json!({})).unwrap();
    s
}

fn classes(s: &mut Session) -> Vec<Value> {
    let b = s.execute("bundle.boundaries", Value::Null).unwrap();
    assert_eq!(b["ok"], true);
    b["classes"].as_array().unwrap().clone()
}

#[test]
fn the_three_idioms_make_two_classes() {
    let mut s = open();
    let cl = classes(&mut s);
    assert_eq!(cl.len(), 2);
    let a = &cl[0];
    assert_eq!(
        a["id"],
        "fused:orders/dequeue+payment/do_reserve+workers/acquire"
    );
    assert_eq!(a["enabled"], true);
    assert_eq!(a["members"].as_array().unwrap().len(), 3);
    assert!(a["members"]
        .as_array()
        .unwrap()
        .iter()
        .all(|m| m["enabled_alone"] == true));
    let b = &cl[1];
    assert_eq!(b["id"], "fused:payment/commit+workers/release");
    assert_eq!(b["enabled"], false);
    assert!(b["why"].as_str().unwrap().contains("needs"));
    // each member waits on its own as well, with go-pflow's wording
    for m in b["members"].as_array().unwrap() {
        assert_eq!(m["enabled_alone"], false, "{m}");
        assert!(m["why"].as_str().unwrap().starts_with("needs"), "{m}");
    }
    // the composite's enablement is the flat net's own
    let flat = s.execute("bundle.flatten", Value::Null).unwrap();
    for c in &cl {
        let e = flat["initial_enablement"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["transition"] == c["id"])
            .unwrap();
        assert_eq!(e["enabled"], c["enabled"]);
        assert_eq!(e["why"], c["why"]);
    }
}

#[test]
fn no_event_links_no_classes() {
    let mut s = open();
    s.execute("bundle.clear_links", Value::Null).unwrap();
    assert!(classes(&mut s).is_empty());
}

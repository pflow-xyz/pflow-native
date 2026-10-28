//! The composition views' parity manifests: drawn headless by the egui
//! renderers (crates/app/src/compviews.rs), they must equal the committed
//! goldens (`parity/composition-views/goldens.json`), which pflow.xyz's web
//! renderers are held to. Regenerate with
//! `cargo run -p pflow-native -- --composition-goldens` only when a native
//! renderer change is intended; never to make a parity test pass.

use pflow_native::compviews;
use serde_json::Value;

fn committed() -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/../../parity/composition-views/goldens.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("parity/composition-views/goldens.json");
    serde_json::from_str(&text).unwrap()
}

#[test]
fn goldens_are_what_the_renderers_draw() {
    let now = compviews::goldens().unwrap();
    let was = committed();
    let (a, b) = (
        now["manifests"].as_array().unwrap(),
        was["manifests"].as_array().unwrap(),
    );
    assert_eq!(a.len(), b.len());
    assert_eq!(a.len(), 10 * 2 * 5);
    for (x, y) in a.iter().zip(b) {
        assert_eq!(x, y, "{} / {} / {}", x["view"], x["example"], x["state"]);
    }
    assert_eq!(now["pflow_rs"], was["pflow_rs"]);
}

#[test]
fn manifests_follow_the_contract() {
    let g = compviews::goldens().unwrap();
    for m in g["manifests"].as_array().unwrap() {
        let els = m["elements"].as_array().unwrap();
        // An empty list is legitimate (spiders with no links fuses nothing).
        let keys: Vec<(String, String)> = els
            .iter()
            .map(|e| {
                (
                    e["kind"].as_str().unwrap().into(),
                    e["id"].as_str().unwrap().into(),
                )
            })
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(keys, sorted, "sorted by (kind, id), no duplicates");
        for e in els {
            for v in e["attrs"].as_object().unwrap().values() {
                assert!(!v.is_f64(), "numbers are integers: {e}");
            }
        }
    }
}

#[test]
fn spec_examples_hold() {
    let s = compviews::session_in("default").unwrap();
    let m = compviews::headless_manifest(&s, "lens", "orders", "default");
    let v = m["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "verdict")
        .unwrap()
        .clone();
    assert_eq!(v["attrs"]["holds"], false);
    assert_eq!(v["attrs"]["legal_token"], 0);
    let s = compviews::session_in("payment2-pushout").unwrap();
    let m = compviews::headless_manifest(&s, "cospan", "orders", "payment2-pushout");
    let c = m["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "composite")
        .unwrap()
        .clone();
    assert_eq!(c["attrs"]["form"], "pushout");
    let s = compviews::session_in("default").unwrap();
    let m = compviews::headless_manifest(&s, "boundaries", "connect4", "default");
    assert!(m["elements"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "na"));
}

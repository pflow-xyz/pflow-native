//! The string view's two levels (docs/cv-notes/b.md): the transition level's
//! manifests come from the same drawing as the pattern level's, say so in a
//! `level` field, and agree with `diagram.transition_layout`, the query the
//! layout is read from.

use pflow_native::compviews;
use pflow_native_core::Session;
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn ids<'a>(m: &'a Value, kind: &str) -> BTreeSet<&'a str> {
    m["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == kind)
        .map(|e| e["id"].as_str().unwrap())
        .collect()
}

#[test]
fn transition_manifests_carry_their_level_and_the_pattern_ones_do_not() {
    let s = compviews::session_in("default").unwrap();
    let pattern = compviews::headless_manifest(&s, "string", "orders", "default");
    assert!(
        pattern.get("level").is_none(),
        "the default level names none"
    );
    let t = compviews::headless_manifest_at(&s, "string", "orders", "default", "transition");
    assert_eq!(t["level"], "transition");
    assert_eq!(
        (&t["view"], &t["example"], &t["state"]),
        (&json!("string"), &json!("orders"), &json!("default"))
    );
    // pattern kinds are not emitted at the transition level, and vice versa
    assert!(ids(&t, "box").is_empty() && ids(&t, "fusion").is_empty());
    assert!(ids(&pattern, "tbox").is_empty() && ids(&pattern, "twire").is_empty());
}

#[test]
fn orders_transition_manifest_is_the_layout_of_the_flat_net() {
    for state in ["default", "payment2-pushout", "no-links", "release-undo"] {
        let mut s = compviews::session_in(state).unwrap();
        let lay = s.execute("diagram.transition_layout", json!({})).unwrap();
        let flat = s.execute("bundle.flatten", json!({})).unwrap();
        let m = compviews::headless_manifest_at(&s, "string", "orders", state, "transition");
        let boxes: BTreeSet<&str> = lay["boxes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids(&m, "tbox"), boxes, "{state}: one tbox per box");
        let flat_t: BTreeSet<&str> = flat["transitions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["id"].as_str().unwrap())
            .collect();
        assert_eq!(
            ids(&m, "tbox"),
            flat_t,
            "{state}: boxes are the flat transitions"
        );
        let backs: BTreeSet<String> = lay["back_edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| format!("{}->{}", e[0].as_str().unwrap(), e[1].as_str().unwrap()))
            .collect();
        let got: BTreeSet<String> = ids(&m, "tback").into_iter().map(String::from).collect();
        assert_eq!(got, backs, "{state}: tback per back edge");
        let bound: BTreeSet<&str> = lay["boundary_inputs"]
            .as_array()
            .unwrap()
            .iter()
            .chain(lay["boundary_outputs"].as_array().unwrap())
            .map(|p| p.as_str().unwrap())
            .collect();
        assert_eq!(
            ids(&m, "tboundary"),
            bound,
            "{state}: tboundary per boundary place"
        );
        // fused is the flat net's own flag
        for e in m["elements"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["kind"] == "tbox")
        {
            let id = e["id"].as_str().unwrap();
            let fused = flat["transitions"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["id"] == id)
                .unwrap()["fused"]
                .clone();
            assert_eq!(e["attrs"]["fused"], fused, "{state}: {id}");
        }
        assert!(ids(&m, "twire").len() <= lay["wire_count"].as_u64().unwrap() as usize);
    }
}

#[test]
fn event_links_fuse_boxes_at_the_transition_level() {
    let none = compviews::session_in("no-links").unwrap();
    let some = compviews::session_in("default").unwrap();
    let a = compviews::headless_manifest_at(&none, "string", "orders", "no-links", "transition");
    let b = compviews::headless_manifest_at(&some, "string", "orders", "default", "transition");
    assert!(
        ids(&a, "tbox").len() > ids(&b, "tbox").len(),
        "fusion merges boxes"
    );
    assert!(ids(&b, "tbox").iter().any(|i| i.starts_with("fused:")));
    assert!(!ids(&a, "tbox").iter().any(|i| i.starts_with("fused:")));
}

#[test]
fn connect4_transition_manifest_is_one_net_entry() {
    let mut s = Session::default();
    let lay = s
        .execute(
            "diagram.transition_layout",
            json!({"source": "connect4", "wires": false}),
        )
        .unwrap();
    let st = compviews::session_in("default").unwrap();
    let m = compviews::headless_manifest_at(&st, "string", "connect4", "default", "transition");
    let els = m["elements"].as_array().unwrap();
    assert_eq!(els.len(), 1);
    assert_eq!(
        (els[0]["kind"].as_str(), els[0]["id"].as_str()),
        (Some("tnet"), Some("connect-4"))
    );
    let a = &els[0]["attrs"];
    assert_eq!(a["boxes"], lay["boxes"].as_array().unwrap().len());
    assert_eq!(a["wires"], lay["wire_count"]);
    assert_eq!(a["back_wires"], lay["back_wire_count"]);
    assert_eq!(a["crossing_wires"], lay["crossing_wire_count"]);
    assert_eq!(
        a["boundary_in"],
        lay["boundary_inputs"].as_array().unwrap().len()
    );
}

#[test]
fn transition_goldens_cover_both_examples_and_every_state() {
    let g = compviews::goldens_transition().unwrap();
    let ms = g["manifests"].as_array().unwrap();
    assert_eq!(ms.len(), 2 * 5);
    for m in ms {
        assert_eq!(m["view"], "string");
        assert_eq!(m["level"], "transition");
        for e in m["elements"].as_array().unwrap() {
            for v in e["attrs"].as_object().unwrap().values() {
                assert!(!v.is_f64(), "numbers are integers: {e}");
            }
        }
    }
}

#[test]
fn level_and_lens_are_registry_state() {
    let mut s = Session::default();
    let v = s.execute("view.composition", json!({})).unwrap();
    assert_eq!(v["level"], "pattern");
    assert_eq!(v["lens_y"], 0.0);
    let v = s
        .execute(
            "view.composition",
            json!({"level": "transition", "lens_y": 1234.5}),
        )
        .unwrap();
    assert_eq!(
        (v["level"].as_str(), v["lens_y"].as_f64()),
        (Some("transition"), Some(1234.5))
    );
    assert!(s
        .execute("view.composition", json!({"level": "nope"}))
        .is_err());
    // other fields leave them alone
    let v = s
        .execute("view.composition", json!({"view": "string"}))
        .unwrap();
    assert_eq!(v["level"], "transition");
    assert!(s.journal.iter().all(|j| j.command != "view.composition"));
}

/// Draws `view` at every stage of a merge animation: nothing panics and the
/// manifest is the resting one (animations never change what is drawn).
#[test]
fn merge_animations_run_and_leave_the_manifest_alone() {
    use compviews::{draw, Data, Event};
    use eframe::egui::{self, Pos2, Rect, Vec2};
    let events = [
        Event::LinkAdded {
            key: "event:payment:undo:reserve->workers:release".into(),
            kind: "event".into(),
        },
        Event::LinkAdded {
            key: "token:payment:account:customer->payment2:account:customer".into(),
            kind: "token".into(),
        },
        Event::InstanceAdded {
            id: "payment2".into(),
        },
        Event::PieceAdded {
            name: "threats".into(),
            slot: "order".into(),
        },
        Event::PieceAdded {
            name: "memo".into(),
            slot: "memo".into(),
        },
        Event::PieceAdded {
            name: "budget".into(),
            slot: "drivers".into(),
        },
    ];
    for state in [
        "default",
        "release-undo",
        "payment2-pushout",
        "plan-threats",
    ] {
        let sess = compviews::session_in(state).unwrap();
        let mut data = Data::default();
        data.refresh(&sess);
        for (example, level) in [
            ("orders", "pattern"),
            ("orders", "transition"),
            ("connect4", "pattern"),
            ("connect4", "transition"),
        ] {
            let mut cv = sess.composition_view.clone();
            cv.view = "string".into();
            cv.example = example.into();
            cv.level = level.into();
            let run = |anim: Option<(Event, f32)>| {
                let ctx = egui::Context::default();
                let raw = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1000.0))),
                    ..Default::default()
                };
                let mut m = Value::Null;
                ctx.run_ui(raw, |ui| {
                    m = draw(ui, &data, None, &cv, false, anim.clone())
                        .scene
                        .manifest("string", example, state);
                })
                .drop_without_applying_deltas();
                m
            };
            let rest = run(None);
            for e in &events {
                for t in [0.0f32, 0.2, 0.45, 0.7, 1.0] {
                    assert_eq!(
                        run(Some((e.clone(), t))),
                        rest,
                        "{state} {example} {level} {e:?} at {t}"
                    );
                }
            }
        }
    }
}

/// The transition level slides the boxes a new event link fused from where
/// the previous flat net had them (`Data::prev_flat`).
#[test]
fn fusion_ghosts_need_the_previous_flat_net() {
    use compviews::{draw, Data, Event};
    use eframe::egui::{self, Pos2, Rect, Vec2};
    let before = compviews::session_in("default").unwrap();
    let after = compviews::session_in("release-undo").unwrap();
    let mut data = Data::default();
    data.refresh(&before);
    data.refresh(&after);
    assert!(data.prev_flat.is_some(), "the revision before is kept");
    let mut cv = after.composition_view.clone();
    cv.view = "string".into();
    cv.example = "orders".into();
    cv.level = "transition".into();
    let ctx = egui::Context::default();
    for t in [0.1f32, 0.5, 0.9] {
        let raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 1000.0))),
            ..Default::default()
        };
        let e = Event::LinkAdded {
            key: "event:payment:undo:reserve->workers:release".into(),
            kind: "event".into(),
        };
        let mut n = 0;
        ctx.run_ui(raw, |ui| {
            n = draw(ui, &data, None, &cv, false, Some((e.clone(), t)))
                .scene
                .manifest("string", "orders", "release-undo")["elements"]
                .as_array()
                .unwrap()
                .len();
        })
        .drop_without_applying_deltas();
        assert!(n > 0);
    }
}

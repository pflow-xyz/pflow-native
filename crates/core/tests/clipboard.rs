//! The pflow clipboard format (docs/clipboard-format.md): edit.copy / cut
//! write it, edit.paste reads it, against the fixture the spec shares with
//! pflow.xyz. Nothing here touches the store or the data dir.

use pflow_native_core::commands::clipboard::{parse_clip, ClipError};
use pflow_native_core::commands::file::EXAMPLES;
use pflow_native_core::Session;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Byte-identical in both repos; the spec names its sha256.
const FIXTURE: &str = include_str!("fixtures/clip-v1.json");
const FIXTURE_SHA256: &str = "ba8ff2db4b2dca16baa83bbe9c35145ea99e0887d4512a68e8b1a0ef5d146276";
const SPEC: &str = include_str!("../../../docs/clipboard-format.md");

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fixture() -> Value {
    serde_json::from_str(FIXTURE).unwrap()
}

fn sir() -> String {
    EXAMPLES
        .iter()
        .find(|(n, _)| *n == "sir")
        .unwrap()
        .1
        .to_string()
}

/// p0 -(t0)-> p1, 2 tokens in p0, t0 at rate 0.7.
fn chain() -> Session {
    let mut s = Session::default();
    run(
        &mut s,
        "net.add_place",
        json!({"x": 100, "y": 100, "initial": 2}),
    );
    run(&mut s, "net.add_transition", json!({"x": 200, "y": 100}));
    run(&mut s, "net.add_place", json!({"x": 300, "y": 100}));
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "p0", "target": "t0"}),
    );
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "t0", "target": "p1"}),
    );
    run(&mut s, "sim.set_rate", json!({"id": "t0", "rate": 0.7}));
    s
}

/// A clip with ids replaced by positions and coordinates shifted by
/// (dx, dy): what must survive a paste and a copy back.
fn shape(clip: &Value, dx: f64, dy: f64) -> Value {
    let ids: Vec<&String> = clip["places"]
        .as_object()
        .unwrap()
        .keys()
        .chain(clip["transitions"].as_object().unwrap().keys())
        .collect();
    let at = |id: &Value| ids.iter().position(|i| *i == id.as_str().unwrap()).unwrap();
    let xy = |n: &Value| json!([n["x"].as_f64().unwrap() + dx, n["y"].as_f64().unwrap() + dy]);
    json!({
        "places": clip["places"].as_object().unwrap().values().map(|p| json!({
            "label": p["label"], "initial": p["initial"], "capacity": p["capacity"], "xy": xy(p),
        })).collect::<Vec<_>>(),
        "transitions": clip["transitions"].as_object().unwrap().values().map(|t| json!({
            "label": t["label"], "rate": t["rate"], "xy": xy(t),
        })).collect::<Vec<_>>(),
        "arcs": clip["arcs"].as_array().unwrap().iter().map(|a| json!({
            "source": at(&a["source"]), "target": at(&a["target"]),
            "weight": a["weight"], "inhibit": a["inhibitTransition"],
        })).collect::<Vec<_>>(),
    })
}

#[test]
fn the_fixture_is_the_one_the_spec_names() {
    let got = format!("{:x}", Sha256::digest(FIXTURE.as_bytes()));
    assert_eq!(got, FIXTURE_SHA256, "clip-v1.json must stay byte-identical");
    assert!(SPEC.contains(FIXTURE_SHA256), "the spec names this hash");
}

#[test]
fn the_fixture_pastes_and_copies_back() {
    let mut s = Session::default();
    let r = run(&mut s, "edit.paste", json!({ "clip": FIXTURE }));
    assert_eq!(r["pasted"], json!(["p0", "p1", "t0"]), "fresh ids");
    assert_eq!(s.doc.places.len(), 2);
    assert_eq!(s.doc.transitions.len(), 1);
    assert_eq!(s.doc.arcs.len(), 2);
    for id in ["Queue", "Done", "Serve"] {
        assert!(s.doc.kind_of(id).is_none(), "{id} is not reused");
    }
    let (queue, done) = (s.doc.place("p0").unwrap(), s.doc.place("p1").unwrap());
    assert_eq!(queue.label.as_deref(), Some("Queue"));
    assert_eq!(done.label, None);
    assert_eq!(
        (queue.initial.clone(), queue.capacity.clone()),
        (vec![2.0], vec![0.0])
    );
    assert_eq!((queue.x, queue.y), (140.0, 160.0), "+40, +40");
    assert_eq!((done.x, done.y), (340.0, 160.0));
    let serve = s.doc.transition("t0").unwrap();
    assert_eq!(serve.label.as_deref(), Some("Serve"));
    assert_eq!((serve.x, serve.y), (240.0, 160.0));
    assert_eq!(
        s.rates.get("t0"),
        Some(&2.5),
        "the rate became a session rate"
    );
    assert!(serve.extra.get("rate").is_none());
    let arcs: Vec<_> = s
        .doc
        .arcs
        .iter()
        .map(|a| (a.source.as_str(), a.target.as_str()))
        .collect();
    assert_eq!(arcs, [("p0", "t0"), ("t0", "p1")]);
    assert_eq!(s.marking["p0"], vec![2]);
    assert_eq!(
        s.selection,
        ["p0", "p1", "t0"],
        "the pasted nodes are selected"
    );

    // Copy that selection: the clip reads back as the same nodes and arcs.
    let r = run(&mut s, "edit.copy", Value::Null);
    assert_eq!(
        (
            r["places"].clone(),
            r["transitions"].clone(),
            r["arcs"].clone()
        ),
        (json!(2), json!(1), json!(2))
    );
    let text = r["clip"].as_str().expect("clip text");
    let clip: Value = serde_json::from_str(text).unwrap();
    assert_eq!(
        clip["pflowClip"],
        json!({"version": 1, "origin": "pflow-native"})
    );
    assert!(clip.get("@id").is_none(), "a clip is not sealed");
    assert_eq!(clip["@context"], "https://pflow.xyz/schema");
    assert_eq!(clip["@type"], "PetriNet");
    assert_eq!(clip["@version"], "1.1");
    assert_eq!(clip["transitions"]["t0"]["rate"], json!(2.5));
    assert_eq!(clip["arcs"][0]["source"], "p0");
    assert_eq!(clip["arcs"][1]["target"], "p1");
    assert_eq!(shape(&clip, 0.0, 0.0), shape(&fixture(), 40.0, 40.0));
    let mut back = parse_clip(text).expect("a pflow-native clip parses");
    assert_eq!(back.places, s.doc.places);
    assert_eq!(back.arcs, s.doc.arcs);
    assert_eq!(
        back.transitions[0].extra.shift_remove("rate"),
        Some(json!(2.5))
    );
    assert_eq!(back.transitions, s.doc.transitions);
}

#[test]
fn paste_is_one_undo_step_and_takes_its_rates_along() {
    let mut s = Session::default();
    run(&mut s, "edit.paste", json!({ "clip": FIXTURE }));
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.doc.places.is_empty() && s.doc.transitions.is_empty() && s.doc.arcs.is_empty());
    assert!(s.rates.is_empty(), "undo removes the pasted rate");
    assert!(!s.can_undo(), "one step");
    run(&mut s, "edit.redo", Value::Null);
    assert_eq!(s.doc.places.len(), 2);
    assert_eq!(s.rates.get("t0"), Some(&2.5), "redo restores it");
}

#[test]
fn paste_takes_the_clip_as_an_object_and_needs_no_session_clipboard() {
    let mut s = Session::default();
    assert!(s.clipboard.is_none());
    let spec = s.registry().find("edit.paste").unwrap();
    assert_eq!(spec.describe(&s)["enabled"], json!(true));
    let e = s
        .execute("edit.paste", Value::Null)
        .unwrap_err()
        .to_string();
    assert!(e.contains("clipboard is empty"), "{e}");
    let r = run(
        &mut s,
        "edit.paste",
        json!({ "clip": fixture(), "dx": 0, "dy": 0 }),
    );
    assert_eq!(r["pasted"].as_array().unwrap().len(), 3);
    assert_eq!(s.doc.place("p0").map(|p| (p.x, p.y)), Some((100.0, 120.0)));
    assert!(
        s.clipboard.is_none(),
        "a clip argument leaves the session clipboard alone"
    );
}

#[test]
fn a_later_format_version_is_refused() {
    let mut v = fixture();
    v["pflowClip"]["version"] = json!(2);
    let text = v.to_string();
    match parse_clip(&text) {
        Err(ClipError::TooNew(n)) => assert_eq!(n.as_u64(), Some(2)),
        other => panic!("expected TooNew, got {other:?}"),
    }
    let mut s = Session::default();
    let e = s
        .execute("edit.paste", json!({ "clip": text }))
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("version 2") && e.contains("reads version 1"),
        "{e}"
    );
    assert!(s.doc.places.is_empty() && !s.can_undo());
}

#[test]
fn a_plain_pflow_net_pastes_whole() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(
        &mut s,
        "net.set_details",
        json!({"name": "Open", "description": "the open net"}),
    );
    let details = s.doc.extra.clone();
    let mut net: Value = serde_json::from_str(&sir()).unwrap();
    net["transitions"]["infect"]["rate"] = json!(0.3);
    net["name"] = json!("Pasted");
    net["description"] = json!("the pasted net");
    net["@id"] = json!("zb2rhexample");
    let r = run(&mut s, "edit.paste", json!({ "clip": net.to_string() }));
    assert_eq!(r["pasted"], json!(["p0", "p1", "p2", "t0", "t1"]));
    assert_eq!(s.doc.places.len(), 6);
    assert_eq!(s.doc.transitions.len(), 4);
    assert_eq!(s.doc.arcs.len(), 10);
    assert_eq!(
        s.doc.place("p0").unwrap().label.as_deref(),
        Some("Susceptible")
    );
    assert_eq!(s.rates.get("t0"), Some(&0.3));
    assert!(s
        .doc
        .arcs
        .iter()
        .any(|a| a.source == "t0" && a.target == "p1" && a.weight == [2.0]));
    // The pasted net's own fields (name, description, @id) are not.
    assert_eq!(s.doc.extra, details);
    assert_eq!(s.doc.extra["name"], "Open");
}

#[test]
fn other_text_is_not_a_clip() {
    let not_clips = [
        "hello",
        "",
        "[1, 2]",
        "{}",
        r#"{"places": {}, "transitions": {}}"#,
        r#"{"@context": "https://pflow.xyz/schema", "@type": "PetriNet"}"#,
        r#"{"pflowClip": {"version": 0}, "places": {"a": {"x": 0, "y": 0}}}"#,
        r#"{"pflowClip": {"version": "1"}, "places": {"a": {"x": 0, "y": 0}}}"#,
        r#"{"pflowClip": true, "places": {"a": {"x": 0, "y": 0}}}"#,
    ];
    let mut s = chain();
    run(&mut s, "edit.select", json!({"ids": ["p0"]}));
    run(&mut s, "edit.copy", Value::Null);
    let before = s.doc.clone();
    for text in not_clips {
        assert!(
            matches!(parse_clip(text), Err(ClipError::NotAClip(_))),
            "{text:?}: {:?}",
            parse_clip(text)
        );
        let e = s
            .execute("edit.paste", json!({ "clip": text }))
            .unwrap_err()
            .to_string();
        assert!(e.contains("not a pflow clip"), "{text:?}: {e}");
        assert_eq!(
            s.doc, before,
            "{text:?}: a refused clip does not fall back to the session clipboard"
        );
    }
}

/// A change to the fixture that breaks it.
type Breakage = fn(&mut Value);

#[test]
fn malformed_clips_are_refused_without_panicking() {
    let edits: Vec<(&str, Breakage)> = vec![
        ("joins two places", |v| {
            v["arcs"][0]["target"] = json!("Done")
        }),
        ("joins two transitions", |v| {
            v["transitions"]["Also"] = json!({"x": 0, "y": 0});
            v["arcs"][0]["source"] = json!("Also");
        }),
        ("names two nodes", |v| {
            v["transitions"]["Queue"] = json!({"x": 0, "y": 0})
        }),
        ("negative weight", |v| v["arcs"][0]["weight"] = json!([-1])),
        ("negative initial", |v| {
            v["places"]["Queue"]["initial"] = json!([-2])
        }),
        ("arc without source", |v| {
            v["arcs"][0].as_object_mut().unwrap().remove("source");
        }),
        ("\"places\" must be an object", |v| {
            v["places"] = json!([{"id": "Queue"}])
        }),
        // Not pasted partly, without the transitions or the arcs.
        ("\"transitions\" must be an object", |v| {
            v["transitions"] = json!([{"id": "Serve"}])
        }),
        ("\"transitions\" must be an object", |v| {
            v["transitions"] = json!(null)
        }),
        ("\"arcs\" must be an array", |v| {
            v["arcs"] = json!({"a": v["arcs"][0].clone()})
        }),
        ("place \"Done\" is not an object", |v| {
            v["places"]["Done"] = json!(5)
        }),
        ("transition \"Serve\" is not an object", |v| {
            v["transitions"]["Serve"] = json!("Serve")
        }),
        ("no places or transitions", |v| {
            v["places"] = json!({});
            v["transitions"] = json!({});
            v["arcs"] = json!([]);
        }),
    ];
    for (why, edit) in edits {
        let mut v = fixture();
        edit(&mut v);
        let text = v.to_string();
        match parse_clip(&text) {
            Err(ClipError::Malformed(m)) => assert!(m.contains(why), "{why}: {m}"),
            other => panic!("{why}: expected Malformed, got {other:?}"),
        }
        let mut s = Session::default();
        let e = s
            .execute("edit.paste", json!({ "clip": text }))
            .unwrap_err()
            .to_string();
        assert!(
            e.starts_with("bad pflow clip") && e.contains(why),
            "{why}: {e}"
        );
        assert!(
            s.doc.places.is_empty() && s.doc.transitions.is_empty() && !s.can_undo(),
            "{why}"
        );
        assert!(s.rates.is_empty(), "{why}");
    }
}

/// A clip holds the arcs whose two ends were copied; one with an end
/// outside it is skipped, as pflow.xyz's paste skips it, and the result
/// says how many were.
#[test]
fn an_arc_to_a_node_outside_the_clip_is_skipped() {
    let mut s = Session::default();
    let r = run(&mut s, "edit.paste", json!({ "clip": fixture() }));
    assert!(r.get("skippedArcs").is_none(), "{r}");
    for edit in [
        (|v: &mut Value| v["arcs"][0]["source"] = json!("Elsewhere")) as Breakage,
        |v| v["arcs"][1]["target"] = json!(""),
    ] {
        let mut v = fixture();
        edit(&mut v);
        let clip = parse_clip(&v.to_string()).unwrap();
        assert_eq!(clip.arcs.len(), 2, "parsed as it is");
        let mut s = Session::default();
        let r = run(&mut s, "edit.paste", json!({ "clip": v.to_string() }));
        assert_eq!(r["skippedArcs"], 1, "{r}");
        assert_eq!(r["pasted"].as_array().unwrap().len(), 3, "{r}");
        assert_eq!((s.doc.places.len(), s.doc.transitions.len()), (2, 1));
        assert_eq!(s.doc.arcs.len(), 1);
        let ids: Vec<&str> = s
            .doc
            .places
            .iter()
            .map(|p| p.id.as_str())
            .chain(s.doc.transitions.iter().map(|t| t.id.as_str()))
            .collect();
        let a = &s.doc.arcs[0];
        assert!(ids.contains(&a.source.as_str()) && ids.contains(&a.target.as_str()));
        assert!(s.can_undo());
    }
}

#[test]
fn the_session_clipboard_still_works_and_keeps_rates() {
    let mut s = chain();
    run(&mut s, "edit.select", json!({"ids": ["p0", "t0"]}));
    let r = run(&mut s, "edit.copy", Value::Null);
    assert_eq!(
        (r["places"].clone(), r["arcs"].clone()),
        (json!(1), json!(1))
    );
    let r = run(&mut s, "edit.paste", Value::Null);
    assert_eq!(r["pasted"], json!(["p2", "t1"]));
    assert!(s.doc.arc_index("p2", "t1").is_some(), "arc remapped");
    assert_eq!(s.doc.place("p2").map(|p| (p.x, p.y)), Some((140.0, 140.0)));
    assert_eq!(s.rates.get("t1"), Some(&0.7), "the copied rate came along");
    assert!(s.doc.transition("t1").unwrap().extra.get("rate").is_none());
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.doc.transition("t1").is_none());
    assert!(!s.rates.contains_key("t1"), "undone with the paste");
    assert_eq!(s.rates.get("t0"), Some(&0.7));
    // Pasting again from the same clipboard.
    run(&mut s, "edit.paste", json!({"dx": 0, "dy": 200}));
    assert_eq!(s.doc.place("p2").map(|p| (p.x, p.y)), Some((100.0, 300.0)));
}

#[test]
fn cut_returns_the_clip_and_deletes() {
    let mut s = chain();
    run(&mut s, "edit.select", json!({"ids": ["t0"]}));
    let r = run(&mut s, "edit.cut", Value::Null);
    let clip = parse_clip(r["clip"].as_str().unwrap()).unwrap();
    assert_eq!(clip.transitions.len(), 1);
    assert!(
        clip.places.is_empty() && clip.arcs.is_empty(),
        "no arc has both ends cut"
    );
    assert_eq!(clip.transitions[0].extra.get("rate"), Some(&json!(0.7)));
    assert!(s.doc.transition("t0").is_none());
    assert!(s.doc.arcs.is_empty(), "the arcs touching it went too");
    run(&mut s, "edit.paste", Value::Null);
    assert_eq!(s.rates.get("t0"), Some(&0.7), "pasted back with its rate");
    run(&mut s, "edit.undo", Value::Null);
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.doc.transition("t0").is_some(), "cut is one undo step");
    assert_eq!(s.doc.arcs.len(), 2);
}

#[test]
fn duplicate_keeps_rates_and_the_clipboard() {
    let mut s = chain();
    run(&mut s, "edit.select", json!({"ids": ["t0"]}));
    let r = run(&mut s, "edit.duplicate", Value::Null);
    assert_eq!(r["pasted"], json!(["t1"]));
    assert_eq!(s.rates.get("t1"), Some(&0.7));
    assert!(
        s.clipboard.is_none(),
        "duplicate leaves the clipboard as it was"
    );
}

/// pflow.xyz draws same-direction parallel arcs (a normal and an inhibitor
/// arc on one pair, say), and file.load keeps them: copy, paste and
/// duplicate carry them as they are.
#[test]
fn parallel_arcs_copy_paste_and_duplicate() {
    let mut s = Session::default();
    let net = json!({
        "places": {"a": {"x": 0, "y": 0, "initial": [1]}, "b": {"x": 100, "y": 0}},
        "transitions": {"t": {"x": 50, "y": 0}},
        "arcs": [
            {"source": "a", "target": "t"},
            {"source": "a", "target": "t", "inhibitTransition": true},
            {"source": "t", "target": "b"},
        ],
    });
    run(&mut s, "file.load_json", json!({ "json": net }));
    run(&mut s, "edit.select_all", Value::Null);
    let r = run(&mut s, "edit.duplicate", Value::Null);
    assert_eq!(r["pasted"], json!(["p0", "p1", "t0"]));
    let inhibit = |s: &Session, src: &str, dst: &str| {
        s.doc
            .arcs
            .iter()
            .filter(|a| a.source == src && a.target == dst)
            .map(|a| a.inhibit)
            .collect::<Vec<_>>()
    };
    assert_eq!(inhibit(&s, "p0", "t0"), [false, true], "both arcs kept");
    run(&mut s, "edit.select_all", Value::Null);
    let r = run(&mut s, "edit.copy", Value::Null);
    assert_eq!(r["arcs"], json!(6));
    let text = r["clip"].as_str().unwrap().to_string();
    assert_eq!(
        parse_clip(&text).expect("its own clip parses").arcs.len(),
        6
    );
    run(&mut s, "edit.paste", Value::Null);
    assert_eq!(s.doc.arcs.len(), 12, "the session clip pastes");
    run(&mut s, "edit.paste", json!({ "clip": text }));
    assert_eq!(s.doc.arcs.len(), 18, "and so does its text");
}

/// A `rate` that is not a usable one pastes as no rate, rather than
/// riding along in the transition into file.save and the next copy.
#[test]
fn an_unusable_rate_is_dropped() {
    for bad in [json!(-1), json!("2"), json!(null)] {
        let mut v = fixture();
        v["transitions"]["Serve"]["rate"] = bad.clone();
        let mut s = Session::default();
        run(&mut s, "edit.paste", json!({ "clip": v }));
        assert!(s.rates.is_empty(), "{bad}");
        let t = s.doc.transition("t0").unwrap();
        assert!(t.extra.get("rate").is_none(), "{bad}: {:?}", t.extra);
        run(&mut s, "edit.select", json!({"ids": ["t0"]}));
        let r = run(&mut s, "edit.copy", Value::Null);
        let clip: Value = serde_json::from_str(r["clip"].as_str().unwrap()).unwrap();
        assert!(clip["transitions"]["t0"].get("rate").is_none(), "{bad}");
    }
}

/// Fresh ids are the first free ones, skipping ids any node uses, as
/// `Document::fresh_id` picks them; and a large paste stays linear.
#[test]
fn pasted_ids_fill_the_gaps() {
    let mut s = Session::default();
    let net = json!({
        "places": {"p0": {"x": 0, "y": 0}, "p2": {"x": 0, "y": 0}},
        "transitions": {"t1": {"x": 0, "y": 0}, "p1": {"x": 0, "y": 0}},
    });
    run(&mut s, "file.load_json", json!({ "json": net }));
    let mut clip = fixture();
    clip["places"]["More"] = json!({"x": 0, "y": 0});
    clip["transitions"]["Again"] = json!({"x": 0, "y": 0});
    let r = run(&mut s, "edit.paste", json!({ "clip": clip }));
    assert_eq!(r["pasted"], json!(["p3", "p4", "p5", "t0", "t2"]));
    assert_eq!(
        s.doc
            .arcs
            .iter()
            .map(|a| (a.source.as_str(), a.target.as_str()))
            .collect::<Vec<_>>(),
        [("p3", "t0"), ("t0", "p4")]
    );

    let n = 3000;
    let places: serde_json::Map<String, Value> = (0..n)
        .map(|i| (format!("q{i}"), json!({"x": i, "y": 0})))
        .collect();
    let big = json!({"pflowClip": {"version": 1}, "places": places});
    let mut s = Session::default();
    let r = run(&mut s, "edit.paste", json!({ "clip": big }));
    let ids = r["pasted"].as_array().unwrap();
    assert_eq!(ids.len(), n);
    assert_eq!(
        (ids[0].as_str(), ids[n - 1].as_str()),
        (Some("p0"), Some("p2999"))
    );
}

#[test]
fn pasting_into_more_colors_pads_the_vectors() {
    let mut s = Session::default();
    run(&mut s, "net.set_colors", json!({"token": ["black", "red"]}));
    run(&mut s, "edit.paste", json!({ "clip": FIXTURE }));
    assert_eq!(s.doc.place("p0").unwrap().initial, vec![2.0, 0.0]);
    assert_eq!(s.marking["p0"], vec![2, 0]);
    assert!(s.doc.arcs.iter().all(|a| a.weight == [1.0, 0.0]));
}

/// The spec's color rule the other way: a clip with more colors than the net
/// loses its extra ones, tokens included, and the net's token list stays.
#[test]
fn pasting_into_fewer_colors_cuts_the_vectors() {
    let mut clip = fixture();
    clip["token"] = json!(["black", "red"]);
    clip["places"]["Queue"]["initial"] = json!([2, 5]);
    clip["places"]["Queue"]["capacity"] = json!([0, 9]);
    clip["arcs"][0]["weight"] = json!([1, 3]);
    clip["arcs"][1].as_object_mut().unwrap().remove("weight");
    let mut s = Session::default();
    let token = s.doc.token.clone();
    run(&mut s, "edit.paste", json!({ "clip": clip.to_string() }));
    let p0 = s.doc.place("p0").unwrap();
    assert_eq!(
        (p0.initial.clone(), p0.capacity.clone()),
        (vec![2.0], vec![0.0])
    );
    assert_eq!(s.marking["p0"], vec![2]);
    assert!(
        s.doc.arcs.iter().all(|a| a.weight == [1.0]),
        "cut, and a missing weight is [1]"
    );
    assert_eq!(s.doc.token, token);
}

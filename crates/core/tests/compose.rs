use pflow_native_core::artifact::{ArtifactData, DrawShape};
use pflow_native_core::Session;
use serde_json::{json, Value};

fn run(s: &mut Session, id: &str, args: Value) -> Value {
    s.execute(id, args).unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn ids(v: &Value) -> Vec<String> {
    serde_json::from_value(v.clone()).unwrap()
}

/// Undo steps available.
fn undo_depth(s: &mut Session) -> usize {
    let mut n = 0;
    let doc = s.doc.clone();
    while s.can_undo() {
        run(s, "edit.undo", Value::Null);
        n += 1;
    }
    for _ in 0..n {
        run(s, "edit.redo", Value::Null);
    }
    assert_eq!(s.doc, doc);
    n
}

#[test]
fn components_list_sim_registry_and_templates() {
    let mut s = Session::default();
    let list = run(&mut s, "compose.components", Value::Null);
    let names: Vec<&str> = list
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["name"].as_str().unwrap())
        .collect();
    for want in [
        "arrivals",
        "service",
        "hazard",
        "inventory",
        "decision",
        "mailbox",
        "datastore",
        "sir",
        "seir",
        "queue",
        "producer-consumer",
        "workflow",
        "bounded-queue",
        "resource-pool",
    ] {
        assert!(names.contains(&want), "{want} missing from {names:?}");
    }
    let svc = list
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "service")
        .unwrap();
    assert_eq!(
        svc["params"][0],
        json!({"name": "staff", "description": "servers in the pool", "default": 2.0})
    );
    assert_eq!(svc["ports"][0]["role"], "in");
    assert!(s.journal.is_empty(), "a query is not journaled");
}

#[test]
fn fuse_sir_into_an_empty_net() {
    let mut s = Session::default();
    let out = run(
        &mut s,
        "compose.fuse",
        json!({"component": "sir", "params": {"population": 100, "initial_infected": 4}}),
    );
    assert_eq!(ids(&out["places"]), ["sir_S", "sir_I", "sir_R"]);
    assert_eq!(ids(&out["transitions"]), ["sir_infection", "sir_recovery"]);
    assert_eq!(out["arcs"], 5);
    assert_eq!(s.doc.places.len(), 3);
    assert_eq!(s.doc.arcs.len(), 5);
    assert_eq!(s.doc.place("sir_S").unwrap().initial, vec![96.0]);
    assert_eq!(
        s.doc.place("sir_I").unwrap().label.as_deref(),
        Some("Infected")
    );
    assert_eq!(s.tokens("sir_I"), 4);
    // Fires through the metamodel like any drawn net.
    run(&mut s, "sim.fire", json!({"id": "sir_infection"}));
    assert_eq!(s.tokens("sir_I"), 5);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(undo_depth(&mut s), 1, "one undo step");
    run(&mut s, "edit.undo", Value::Null);
    assert!(s.doc.places.is_empty());
    // A second instance needs its own prefix.
    run(&mut s, "edit.redo", Value::Null);
    assert!(s
        .execute("compose.fuse", json!({"component": "sir"}))
        .is_err());
    run(
        &mut s,
        "compose.fuse",
        json!({"component": "sir", "prefix": "b"}),
    );
    assert_eq!(s.doc.places.len(), 6);
}

#[test]
fn fuse_service_into_coffee_shop_with_attach() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "coffee-shop"}));
    let (np, nt, na) = (
        s.doc.places.len(),
        s.doc.transitions.len(),
        s.doc.arcs.len(),
    );
    let depth = undo_depth(&mut s);
    let args = json!({"component": "service", "prefix": "desk",
        "attach": {"queue": "WaitingCustomers"}, "params": {"staff": 3}});
    let err = s
        .execute("compose.fuse", args.clone())
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("token colors"),
        "colored net refused clearly: {err}"
    );
    let mut colored = args.clone();
    colored["color"] = json!(0);
    let out = run(&mut s, "compose.fuse", colored);
    assert_eq!(
        out["fused"],
        json!([{"port": "queue", "role": "in", "place": "WaitingCustomers"}])
    );
    assert_eq!(
        ids(&out["places"]),
        ["desk_staff_free", "desk_busy", "desk_served"]
    );
    assert_eq!(s.doc.places.len(), np + 3, "queue fused, not copied");
    assert_eq!(s.doc.transitions.len(), nt + 2);
    assert_eq!(s.doc.arcs.len(), na + 6);
    assert!(s.doc.arc_index("WaitingCustomers", "desk_pickup").is_some());
    assert_eq!(
        s.doc.place("desk_staff_free").unwrap().initial,
        vec![3.0, 0.0, 0.0]
    );
    assert_eq!(
        s.doc.arcs[s.doc.arc_index("desk_finish", "desk_served").unwrap()].weight,
        vec![1.0, 0.0, 0.0]
    );
    assert_eq!(s.rates["desk_pickup"], 720.0);
    assert_eq!(s.rates["desk_finish"], 4.0);
    assert!(out["notes"][0].as_str().unwrap().contains("non-kinetic"));
    // Placed to the right of the net.
    let max_x = 640.0_f64.max(
        s.doc
            .places
            .iter()
            .take(np)
            .map(|p| p.x)
            .fold(0.0, f64::max),
    );
    assert!(s.doc.place("desk_busy").unwrap().x > max_x - 1.0);
    assert_eq!(undo_depth(&mut s), depth + 1);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.doc.places.len(), np);
    assert_eq!(s.doc.arcs.len(), na);

    // A color by name, then the refusals sim makes.
    let cname = s.doc.token[0].rsplit('/').next().unwrap().to_string();
    run(
        &mut s,
        "compose.fuse",
        json!({"component": "hazard", "color": cname,
        "attach": {"queue": "WaitingCustomers"}, "prefix": "giveup"}),
    );
    assert!(s.doc.transition("giveup_abandon").is_some());
    for bad in [
        json!({"component": "nope"}),
        json!({"component": "hazard", "color": 0, "params": {"typo": 1}}),
        json!({"component": "hazard", "color": 0, "attach": {"queue": "nowhere"}}),
        json!({"component": "hazard", "color": 0, "attach": {"noport": "WaitingCustomers"}}),
        json!({"component": "hazard", "color": 7}),
        json!({"component": "datastore", "color": 0, "attach": {"store": "CoffeeBeans"}, "params": {"capacity": 5}}),
    ] {
        assert!(
            s.execute("compose.fuse", bad.clone()).is_err(),
            "{bad} accepted"
        );
    }
    run(
        &mut s,
        "compose.fuse",
        json!({"component": "datastore", "color": 0,
        "attach": {"store": "CoffeeBeans"}, "params": {"write_per_hour": 60}}),
    );
}

#[test]
fn fuse_helpdesk_like_sim() {
    let mut s = Session::default();
    run(
        &mut s,
        "compose.fuse",
        json!({"component": "arrivals", "prefix": "tickets", "params": {"per_hour": 12}}),
    );
    run(
        &mut s,
        "compose.fuse",
        json!({"component": "service", "prefix": "desk",
        "attach": {"queue": "tickets_queue"}, "params": {"staff": 2, "per_hour": 5}}),
    );
    run(
        &mut s,
        "compose.fuse",
        json!({"component": "hazard", "prefix": "giveup",
        "attach": {"queue": "tickets_queue"}, "params": {"patience_per_hour": 1.5}}),
    );
    let places: Vec<&str> = s.doc.places.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(
        places,
        [
            "tickets_queue",
            "desk_staff_free",
            "desk_busy",
            "desk_served",
            "giveup_lost"
        ]
    );
    assert_eq!(s.doc.transitions.len(), 4);
    assert_eq!(s.doc.arcs.len(), 1 + 6 + 2);
    // Nodes do not overlap.
    let mut pts: Vec<(i64, i64)> = s
        .doc
        .places
        .iter()
        .map(|p| (p.x as i64, p.y as i64))
        .chain(s.doc.transitions.iter().map(|t| (t.x as i64, t.y as i64)))
        .collect();
    pts.sort();
    pts.dedup();
    assert_eq!(pts.len(), 9);
}

#[test]
fn extend_applies_ops_as_one_undo_step() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    let before = s.doc.clone();
    let depth = undo_depth(&mut s);
    let out = run(
        &mut s,
        "compose.extend",
        json!({"ops": [
            {"op": "add_place", "id": "D", "initial": 0, "label": "Dead"},
            {"op": "add_transition", "id": "die", "rate": 0.1},
            {"op": "add_arc", "from": "I", "to": "die"},
            {"op": "add_arc", "from": "die", "to": "D", "weight": 1},
            {"op": "set_initial", "id": "S", "initial": 500},
            {"op": "set_capacity", "id": "R", "capacity": 2000},
            {"op": "add_arc", "from": "R", "to": "infect", "type": "read"},
            {"op": "rename", "id": "recover", "new_id": "heal"},
        ]}),
    );
    assert_eq!(out["applied"].as_array().unwrap().len(), 8);
    let d = &out["diff"];
    assert_eq!(d["places_added"], json!(["D"]));
    assert_eq!(d["transitions_added"], json!(["die", "heal"]));
    assert_eq!(d["transitions_removed"], json!(["recover"]));
    assert_eq!(
        d["places_modified"],
        json!([
            "R: capacity 0 → capacity 2000",
            "S: initial 999 → initial 500"
        ])
    );
    assert!(d["arcs_added"]
        .as_array()
        .unwrap()
        .contains(&json!("infect->R (read)")));
    assert_eq!(s.rates["die"], 0.1);
    assert!(
        s.doc.arcs[s.doc.arc_index("infect", "R").unwrap()].inhibit,
        "read arc in document orientation"
    );
    assert_eq!(undo_depth(&mut s), depth + 1);
    run(&mut s, "edit.undo", Value::Null);
    assert_eq!(s.doc, before);
    assert!(s.journal.iter().any(|j| j.command == "compose.extend"));
    assert!(
        !s.journal.iter().any(|j| j.command == "net.add_place"),
        "inner steps not journaled"
    );

    // Removal cascades; a later remove_arc of a cascaded arc is fine.
    run(
        &mut s,
        "compose.extend",
        json!({"ops": "[{\"op\":\"remove_place\",\"id\":\"R\"},{\"op\":\"remove_arc\",\"from\":\"recover\",\"to\":\"R\"}]"}),
    );
    assert!(s.doc.place("R").is_none());
    assert_eq!(s.doc.arcs.len(), 4);
    run(&mut s, "edit.undo", Value::Null);

    // A failing op rolls everything back; bad ops are all reported up front.
    let n = s.doc.places.len();
    assert!(s
        .execute(
            "compose.extend",
            json!({"ops": [
                {"op": "add_place", "id": "Z"},
                {"op": "add_arc", "from": "Z", "to": "nowhere"},
            ]})
        )
        .is_err());
    assert_eq!(s.doc.places.len(), n);
    let err = s.execute("compose.extend", json!({"ops": [
        {"op": "add_event", "id": "e"}, {"op": "bogus"}, {"op": "add_transition", "id": "x", "guard": "a > 1"}
    ]})).unwrap_err().to_string();
    assert!(
        err.contains("operation 0") && err.contains("operation 1") && err.contains("operation 2"),
        "{err}"
    );
    assert!(s.execute("compose.extend", json!({"ops": []})).is_err());
}

#[test]
fn diff_reports_exactly_the_changes() {
    let mut s = Session::default();
    run(&mut s, "file.open_example", json!({"name": "sir"}));
    run(&mut s, "model.save", json!({"name": "sir"}));
    run(&mut s, "net.set_place", json!({"id": "I", "initial": 3}));
    run(&mut s, "edit.delete", json!({"ids": ["R"]}));
    run(
        &mut s,
        "net.add_place",
        json!({"id": "Q", "x": 600, "y": 260}),
    );
    run(
        &mut s,
        "net.add_arc",
        json!({"source": "recover", "target": "Q"}),
    );
    run(
        &mut s,
        "net.set_arc",
        json!({"source": "infect", "target": "I", "weight": 3}),
    );
    let out = run(&mut s, "compose.diff", json!({"a": "sir", "b": "current"}));
    let d = &out["diff"];
    assert_eq!(d["places_added"], json!(["Q"]));
    assert_eq!(d["places_removed"], json!(["R"]));
    assert_eq!(d["places_modified"], json!(["I: initial 1 → initial 3"]));
    assert_eq!(d["arcs_added"], json!(["recover->Q"]));
    assert_eq!(d["arcs_removed"], json!(["recover->R"]));
    assert_eq!(
        d["arcs_modified"],
        json!(["infect->I: weight 2 → weight 3"])
    );
    assert_eq!(d["transitions_added"], json!([]));
    assert_eq!(d["transitions_removed"], json!([]));
    assert_eq!(out["added"], 2);
    assert_eq!(out["removed"], 2);
    assert_eq!(out["modified"], 2);

    let table = s.artifact(out["table"].as_str().unwrap()).unwrap();
    match &table.data {
        ArtifactData::Table { columns, rows } => {
            assert_eq!(columns, &["kind", "id", "change", "before", "after"]);
            assert_eq!(rows.len(), 6);
        }
        other => panic!("{other:?}"),
    }
    // Sets name only nodes of the open net, and the canvas can show them.
    let sets_id = out["sets"].as_str().unwrap().to_string();
    match &s.artifact(&sets_id).unwrap().data {
        ArtifactData::Sets { sets } => {
            assert_eq!(sets[0].label, "added");
            assert_eq!(sets[0].nodes, ["Q", "recover"]);
            assert_eq!(
                sets[1].nodes,
                ["recover"],
                "R is gone; its arc's live endpoint remains"
            );
            for set in sets {
                for n in &set.nodes {
                    assert!(s.doc.kind_of(n).is_some(), "{n} not in the open net");
                }
            }
        }
        other => panic!("{other:?}"),
    }
    for i in 0..3 {
        run(
            &mut s,
            "artifact.show_set",
            json!({"id": sets_id, "index": i}),
        );
    }
    // The drawing is finite and inside its box.
    match &s.artifact(out["drawing"].as_str().unwrap()).unwrap().data {
        ArtifactData::Drawing {
            width,
            height,
            shapes,
        } => {
            let inside = |x: f64, y: f64| {
                x.is_finite()
                    && y.is_finite()
                    && (0.0..=*width).contains(&x)
                    && (0.0..=*height).contains(&y)
            };
            assert!(shapes.len() > 20);
            let mut outlined = 0;
            for sh in shapes {
                match sh {
                    DrawShape::Line { points, .. } | DrawShape::Polygon { points, .. } => {
                        assert!(points.iter().all(|&(x, y)| inside(x, y)), "{sh:?}");
                    }
                    DrawShape::Rect { x, y, w, h, .. } => {
                        assert!(inside(*x, *y) && inside(x + w, y + h), "{sh:?}");
                    }
                    DrawShape::Circle {
                        x,
                        y,
                        r,
                        stroke_width,
                        ..
                    } => {
                        assert!(inside(x - r, y - r) && inside(x + r, y + r), "{sh:?}");
                        if *stroke_width == 3.0 {
                            outlined += 1;
                        }
                    }
                    DrawShape::Text { x, y, .. } => assert!(inside(*x, *y), "{sh:?}"),
                }
            }
            // R (left), Q (right), I on both sides, plus three legend dots.
            assert_eq!(outlined, 4 + 3);
        }
        other => panic!("{other:?}"),
    }
    let svg = run(
        &mut s,
        "export.artifact",
        json!({"id": out["drawing"], "format": "svg"}),
    );
    assert!(svg.as_str().unwrap().contains("<circle"));

    // No change, and two library models (no sets).
    let same = run(&mut s, "compose.diff", json!({"b": "current"}));
    assert_eq!(same["has_changes"], false);
    run(&mut s, "model.save", json!({"name": "edited"}));
    let lib = run(&mut s, "compose.diff", json!({"a": "sir", "b": "edited"}));
    assert_eq!(lib["sets"], Value::Null);
    assert_eq!(lib["diff"], out["diff"]);
    assert!(s.execute("compose.diff", json!({"b": "missing"})).is_err());
}

#[test]
fn every_component_fuses_standalone_and_compiles() {
    let mut s = Session::default();
    let list = run(&mut s, "compose.components", Value::Null);
    for c in list.as_array().unwrap() {
        let mut t = Session::default();
        let name = c["name"].as_str().unwrap();
        run(&mut t, "compose.fuse", json!({"component": name}));
        assert!(!t.doc.places.is_empty(), "{name}");
        // Compiles for the token game.
        let _ = run(&mut t, "sim.enabled", Value::Null);
        assert!(t
            .execute("compose.diff", json!({"a": "current", "b": "current"}))
            .is_ok());
    }
}

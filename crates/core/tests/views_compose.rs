//! The composition commands against go-pflow's own verdicts: the
//! prototype's receipt `tests/fixtures/views/gopflow.json` (go-pflow
//! c802f34 via pflow-views-prototype tools/gocheck; see the README there).
//!
//! The pin is pflow-rs a0a2bbc (branch feat/saga, pflow-rs#11: 29e6a34
//! plus pflow-compose's port of go-pflow's saga), so every receipt case
//! is compared, the saga instances included. One known gap, with its
//! counts asserted so it cannot grow silently:
//!
//! - pflow-compose's `validate` lacks go-pflow's `E_SCHEMA_MISMATCH` and
//!   `E_PORT_DIRECTION` checks, and writes `E_ENDPOINT_KIND`'s link kind
//!   with `{:?}` ("a Token link"). The fix is upstream work (pflow-rs branch
//!   `fix/validate-link-go-parity`, with which all 968 queue and pool verdicts match
//!   the receipt exactly); until the pin has it, the receipt's rows are
//!   compared with those two codes taken out and the kind's case changed.
//!   [`pflow_rs_gap_at_the_pin`] holds the gap itself, and fails when the
//!   pin moves past it. The flatten config "rejected" (refused by go-pflow
//!   with E_PORT_DIRECTION) is excluded for the same reason.

use std::collections::BTreeMap;

use pflow_compose::{Bundle, Link};
use pflow_native_core::composition::{self as comp};
use pflow_native_core::Session;
use serde_json::{json, Value};

fn receipt() -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/views/gopflow.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// The codes pflow-compose does not check at the pin.
const GAP_CODES: [&str; 2] = ["E_SCHEMA_MISMATCH", "E_PORT_DIRECTION"];

/// go-pflow's row as pflow-compose at the pin gives it: without the
/// [`GAP_CODES`] (and their messages), the kind in E_ENDPOINT_KIND as
/// `{:?}` writes it. Returns (ok, codes, messages, changed).
fn at_the_pin(w: &Value) -> (bool, Vec<String>, Vec<String>, bool) {
    let codes: Vec<String> = w["codes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap().to_string())
        .collect();
    let msgs: Vec<String> = w["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_str().unwrap().to_string())
        .collect();
    // Errors' messages come first, one per code; then the warnings'.
    let (err_msgs, warn_msgs) = msgs.split_at(codes.len());
    let mut kept_codes = vec![];
    let mut kept = vec![];
    for (c, m) in codes.iter().zip(err_msgs) {
        if !GAP_CODES.contains(&c.as_str()) {
            kept_codes.push(c.clone());
            kept.push(
                m.replace(
                    "a token link connects places",
                    "a Token link connects places",
                )
                .replace("a data link connects places", "a Data link connects places"),
            );
        }
    }
    kept.extend(warn_msgs.iter().cloned());
    let changed = kept_codes != codes || kept != msgs;
    (kept_codes.is_empty(), kept_codes, kept, changed)
}

fn built(ids: &[&str]) -> Vec<pflow_compose::Subnet> {
    ids.iter()
        .map(|id| comp::build(comp::instance(id).unwrap()).unwrap())
        .collect()
}

#[test]
fn subnets_match_go_pflow_constructors() {
    let r = receipt();
    let mut compared = 0;
    for (id, want) in r["subnets"].as_object().unwrap() {
        let spec = comp::instance(id).unwrap();
        let got = comp::subnet_json(spec, &comp::build(spec).unwrap());
        for k in [
            "id",
            "net_type",
            "model_name",
            "places",
            "transitions",
            "arcs",
            "constraints",
        ] {
            assert_eq!(got[k], want[k], "{id}.{k}");
        }
        let ports: Vec<Value> = got["ports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                let mut p = p.clone();
                p.as_object_mut().unwrap().remove("pattern_port");
                p
            })
            .collect();
        assert_eq!(json!(ports), want["ports"], "{id}.ports");
        compared += 1;
    }
    assert_eq!(compared, 6);
}

#[test]
fn verdicts_match_go_pflow_validate() {
    let r = receipt();
    let want: Vec<&Value> = r["verdicts"].as_array().unwrap().iter().collect();
    let subs = built(&[
        "orders", "orders2", "payment", "payment2", "workers", "workers2",
    ]);
    let refs: Vec<&pflow_compose::Subnet> = subs.iter().collect();
    let got = comp::all_verdicts(&refs);
    assert_eq!(got.len(), want.len());
    assert_eq!(got.len(), 5576, "6 instances' ordered port pairs × 4 kinds");
    let (mut gap_rows, mut flipped, mut reworded) = (0, 0, 0);
    for (g, w) in got.iter().zip(&want) {
        let label = format!("{} {} -> {}", g.kind, g.from, g.to);
        assert_eq!(g.kind, w["kind"].as_str().unwrap(), "{label}");
        assert_eq!(g.from, w["from"].as_str().unwrap(), "{label}");
        assert_eq!(g.to, w["to"].as_str().unwrap(), "{label}");
        let (ok, codes, msgs, changed) = at_the_pin(w);
        let gap = w["codes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| GAP_CODES.contains(&c.as_str().unwrap()));
        gap_rows += usize::from(gap);
        flipped += usize::from(json!(ok) != w["ok"]);
        reworded += usize::from(changed && !gap);
        assert_eq!(g.ok, ok, "{label}");
        assert_eq!(g.codes, codes, "{label}");
        assert_eq!(json!(g.warnings), w["warnings"], "{label}");
        assert_eq!(g.messages, msgs, "{label}");
    }
    // The gap, pinned: rows with a check pflow-compose lacks, of which
    // these are legal at the pin and refused by go-pflow; and the
    // E_ENDPOINT_KIND rows worded "a Token link".
    assert_eq!((gap_rows, flipped, reworded), (470, 246, 2256));
}

#[test]
fn pflow_rs_gap_at_the_pin() {
    // When this fails, the pin has pflow-compose's schema and port-direction
    // checks: drop GAP_CODES / at_the_pin and this test, and compare the
    // receipt rows (and the "rejected" flatten) as they are.
    let subs = built(&["orders", "orders2", "workers"]);
    let refs: Vec<&pflow_compose::Subnet> = subs.iter().collect();
    let v = |k, a: &str, b: &str| {
        comp::validate_link(
            &refs,
            &comp::link(
                k,
                comp::parse_endpoint(&json!(a)).unwrap(),
                comp::parse_endpoint(&json!(b)).unwrap(),
            ),
        )
    };
    use pflow_compose::LinkKind::*;
    assert!(
        v(Token, "orders:in", "orders2:in").ok,
        "go-pflow: E_PORT_DIRECTION"
    );
    assert!(
        v(Token, "orders:out", "workers:available").ok,
        "go-pflow: E_SCHEMA_MISMATCH"
    );
    assert_eq!(
        v(Token, "orders:dequeue", "orders2:in").errors[0].message,
        "links[0]: a Token link connects places, but an endpoint names a transition"
    );
}

fn flat_compare(got: &Value, want: &Value, id: &str) {
    assert_eq!(got["ok"], want["ok"], "{id}.ok");
    assert_eq!(got["error"], want["error"], "{id}.error");
    if want["ok"] != json!(true) {
        return;
    }
    let strip = |v: &Value, keys: &[&str]| -> Value {
        json!(v
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                let mut x = x.clone();
                for k in keys {
                    x.as_object_mut().unwrap().remove(*k);
                }
                x
            })
            .collect::<Vec<_>>())
    };
    assert_eq!(
        strip(&got["places"], &["members"]),
        want["places"],
        "{id}.places"
    );
    assert_eq!(
        strip(&got["transitions"], &["members", "fused"]),
        want["transitions"],
        "{id}.transitions"
    );
    for (g, w) in [
        ("arcs", "arcs"),
        ("constraints", "constraints"),
        ("wires", "wires"),
        ("fused_groups", "fusedGroups"),
        ("place_map", "placeMap"),
        ("transition_map", "transitionMap"),
        ("warnings", "warnings"),
        ("initial_enablement", "initialEnablement"),
    ] {
        assert_eq!(got[g], want[w], "{id}.{g}");
    }
}

#[test]
fn flattens_match_go_pflow_flatten_with_map() {
    let r = receipt();
    let mut compared = vec![];
    let mut excluded = vec![];
    for cfg in r["flatten"].as_array().unwrap() {
        let id = cfg["id"].as_str().unwrap();
        let ids: Vec<&str> = cfg["subnets"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap())
            .collect();
        if id == "rejected" {
            // go-pflow refuses its token link with E_PORT_DIRECTION, which
            // pflow-compose does not check at the pin (see the module doc):
            // it flattens. Excluded until the pin has the check.
            excluded.push(id.to_string());
            continue;
        }
        let mut b = Bundle::new("composition");
        for s in built(&ids) {
            b.add_subnet(s);
        }
        for l in cfg["links"].as_array().unwrap() {
            let l: Link = comp::parse_link(l).unwrap();
            b.add_link(l);
        }
        flat_compare(&comp::flatten_json(&b), cfg, id);
        compared.push(id.to_string());
    }
    assert_eq!(
        compared,
        [
            "default",
            "idiom-0",
            "idiom-1",
            "idiom-2",
            "queue-chain",
            "shared-accounts",
            "guard-alone",
            "pool-share",
            "merge-sum",
            "identity"
        ]
    );
    assert_eq!(
        excluded,
        ["rejected"],
        "until the pin checks port direction"
    );
}

#[test]
fn scenarios_match_go_pflow_runs() {
    let r = receipt();
    let mut s = Session::default();
    let open = s.execute("bundle.open", Value::Null).unwrap();
    assert_eq!(open["instances"], json!(["orders", "workers", "payment"]));
    assert_eq!(open["links"].as_array().unwrap().len(), 3);
    let mut n = 0;
    for want in r["scenarios"].as_array().unwrap() {
        assert_eq!(want["config"], "default");
        let id = want["id"].as_str().unwrap();
        let got = s.execute("bundle.run", json!({ "scenario": id })).unwrap();
        let steps: Vec<Value> = got["steps"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| json!({"member": x["member"], "flat": x["flat"], "enabled": x["enabled"], "why": x["why"]}))
            .collect();
        assert_eq!(json!(steps), want["steps"], "{id}.steps");
        assert_eq!(got["completed"], want["completed"], "{id}");
        assert_eq!(got["stopped_at"], want["stoppedAt"], "{id}");
        if !want["finalMarking"].is_null() {
            assert_eq!(got["final_marking"], want["finalMarking"], "{id}");
        }
        let expect: Vec<Value> = got["expect"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| json!({"place": e["place"], "flat": e["flat"], "op": e["op"], "n": e["n"], "actual": e["actual"], "pass": e["pass"]}))
            .collect();
        assert_eq!(json!(expect), want["expect"], "{id}.expect");
        n += 1;
    }
    assert_eq!(n, 4);
}

// ------------------------------------------------------------ commands

fn open_extras(s: &mut Session) {
    s.execute(
        "bundle.open",
        json!({"instances": ["orders", "workers", "orders2", "workers2"]}),
    )
    .unwrap();
}

#[test]
fn add_link_validates_first_and_refuses_with_go_pflow_codes() {
    let mut s = Session::default();
    open_extras(&mut s);
    // orders:dequeue ⊗ workers:acquire came in with the default links.
    let st = s.execute("bundle.state", Value::Null).unwrap();
    assert_eq!(st["links"].as_array().unwrap().len(), 1);
    assert_eq!(st["links"][0]["idiom"], "idiom-queue-dequeue-pool-acquire");

    let err = s
        .execute(
            "bundle.add_link",
            json!({"kind": "data", "from": "orders:out", "to": "orders2:in"}),
        )
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "[E_DATALINK_CONSUMES] links[0]: observer orders2/items has arc enqueue -> items on the \
         observed place; a data link is read-only. Express the dependency as a read arc \
         (tokens >= n), an inhibitor arc, or a guard (tokens(\"items\") > 0)."
    );
    let err = s
        .execute(
            "bundle.add_link",
            json!({"kind": "event", "from": "workers:acquire", "to": "orders:dequeue"}),
        )
        .unwrap_err()
        .to_string();
    assert_eq!(err, "[PAGE_DUPLICATE] already linked");
    let err = s
        .execute(
            "bundle.add_link",
            json!({"kind": "event", "from": "orders:enqueue", "to": "orders:dequeue"}),
        )
        .unwrap_err()
        .to_string();
    assert!(err.starts_with("[PAGE_SAME_SUBNET]"), "{err}");

    let ok = s
        .execute(
            "bundle.add_link",
            json!({"kind": "token", "from": {"subnet": "orders", "port": "out"}, "to": {"subnet": "orders2", "port": "in"}}),
        )
        .unwrap();
    assert_eq!(ok["index"], 1);
    assert_eq!(ok["rule"]["kind"], "token");
    assert_eq!(
        ok["rule"]["iri"],
        "https://ontology.pflow.xyz/linkkind/token"
    );
    assert!(ok["rule"]["passed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["text"] == "schemas agree: queue:items"));

    // Undo takes the link back, redo restores it.
    s.execute("edit.undo", Value::Null).unwrap();
    assert_eq!(
        s.execute("bundle.state", Value::Null).unwrap()["links"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    s.execute("edit.redo", Value::Null).unwrap();
    assert_eq!(
        s.execute("bundle.state", Value::Null).unwrap()["links"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    // Edits are journaled, queries are not.
    let cmds: Vec<&str> = s.journal.iter().map(|j| j.command.as_str()).collect();
    assert_eq!(
        cmds,
        ["bundle.open", "bundle.add_link", "edit.undo", "edit.redo"]
    );
}

#[test]
fn candidates_light_what_validate_allows() {
    let mut s = Session::default();
    open_extras(&mut s);
    let c = s
        .execute("bundle.candidates", json!({"from": "orders:dequeue"}))
        .unwrap();
    let lit: Vec<(String, String)> = c["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["ok"] == json!(true))
        .map(|x| {
            (
                x["key"].as_str().unwrap().to_string(),
                x["link"]["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    // Transition ports of the other instances (event), place ports (guard);
    // workers:acquire is already linked (PAGE_DUPLICATE), the queue's own
    // ports are the page's same-subnet rule.
    let want: Vec<(String, String)> = [
        ("workers:available", "guard"),
        ("workers:in_use", "guard"),
        ("workers:release", "event"),
        ("orders2:in", "guard"),
        ("orders2:out", "guard"),
        ("orders2:depth", "guard"),
        ("orders2:enqueue", "event"),
        ("orders2:dequeue", "event"),
        ("workers2:available", "guard"),
        ("workers2:in_use", "guard"),
        ("workers2:acquire", "event"),
        ("workers2:release", "event"),
    ]
    .iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(lit, want);
    let by_key: BTreeMap<&str, &Value> = c["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| (x["key"].as_str().unwrap(), x))
        .collect();
    let dup = &by_key["workers:acquire"]["verdict"]["errors"][0];
    assert_eq!(dup["code"], "PAGE_DUPLICATE");
    assert_eq!(dup["rule"], "page");
    let same = &by_key["orders:enqueue"]["verdict"]["errors"][0];
    assert_eq!(same["code"], "PAGE_SAME_SUBNET");
    assert_eq!(same["rule"], "page");

    // A place port with token preferred. go-pflow lights only the second
    // queue's in-port (the pools' schema differs, and out/depth face the
    // wrong way); pflow-compose at the pin lacks those two checks (see the
    // module doc), so the pools' place ports and the queue's other two
    // light too.
    let c = s
        .execute(
            "bundle.candidates",
            json!({"from": "orders:out", "prefer": "token"}),
        )
        .unwrap();
    let lit: Vec<&str> = c["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["ok"] == json!(true))
        .map(|x| x["key"].as_str().unwrap())
        .collect();
    assert_eq!(
        lit,
        [
            "workers:available",
            "workers:in_use",
            "orders2:in",
            "orders2:out",
            "orders2:depth",
            "workers2:available",
            "workers2:in_use"
        ]
    );
    let pool = &by_key["workers:available"];
    assert_eq!(pool["link"]["kind"], "guard");
    assert_eq!(pool["link"]["from"]["port"], "dequeue");
}

#[test]
fn instances_edits_and_queries_round_trip() {
    let mut s = Session::default();
    let cat = s.execute("bundle.instances", Value::Null).unwrap();
    let avail: Vec<(&str, bool)> = cat["instances"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| (i["id"].as_str().unwrap(), i["available"] == json!(true)))
        .collect();
    assert_eq!(
        avail,
        [
            ("orders", true),
            ("workers", true),
            ("payment", true),
            ("payment2", true),
            ("orders2", true),
            ("workers2", true)
        ]
    );
    assert!(s.execute("bundle.state", Value::Null).is_err());
    s.execute("bundle.open", Value::Null).unwrap();
    s.execute("bundle.add_instance", json!({"id": "payment2"}))
        .unwrap();
    s.execute(
        "bundle.add_link",
        json!({"kind": "token", "from": "payment:account:customer", "to": "payment2:account:customer"}),
    )
    .unwrap();
    s.execute("bundle.add_instance", json!({"id": "workers2"}))
        .unwrap();
    s.execute(
        "bundle.add_link",
        json!({"kind": "token", "from": "workers:available", "to": "workers2:available"}),
    )
    .unwrap();
    let flat = s.execute("bundle.flatten", Value::Null).unwrap();
    assert_eq!(flat["ok"], true);
    assert_eq!(
        flat["wires"]["wire:workers/available"],
        json!(["workers/available", "workers2/available"])
    );
    assert!(flat["artifact"].is_string());
    let rm = s
        .execute("bundle.remove_instance", json!({"id": "workers2"}))
        .unwrap();
    assert_eq!(rm["links_removed"].as_array().unwrap().len(), 1);
    s.execute("bundle.clear_links", Value::Null).unwrap();
    s.execute("bundle.reset_links", Value::Null).unwrap();
    let st = s.execute("bundle.state", Value::Null).unwrap();
    assert_eq!(st["links"].as_array().unwrap().len(), 3);
    s.execute("bundle.remove_link", json!({"index": 0}))
        .unwrap();
    let v = s.execute("bundle.validate", Value::Null).unwrap();
    assert_eq!(v["valid"], true);
    let onto = s.execute("bundle.ontology", Value::Null).unwrap();
    assert!(onto["provenance"]["source"]
        .as_str()
        .unwrap()
        .contains("ns/patterns.ttl@bc4a996"));
    assert_eq!(onto["idioms"].as_array().unwrap().len(), 3);
    let sc = s.execute("bundle.scenarios", Value::Null).unwrap();
    assert_eq!(sc["scenarios"].as_array().unwrap().len(), 4);
    let ver = s
        .execute(
            "bundle.verdicts",
            json!({"instances": ["orders", "workers", "payment"]}),
        )
        .unwrap();
    assert_eq!(ver["instances"], json!(["orders", "workers", "payment"]));
    assert_eq!(ver["unavailable"], json!([]));
    // Queue 5 ports, pool 4, saga 12; ordered pairs on two subnets × 4 kinds.
    assert_eq!(ver["total"], (5 * 4 + 5 * 12 + 4 * 12) * 2 * 4);
    s.execute("bundle.close", Value::Null).unwrap();
    assert!(s.composition.is_none());
    s.execute("edit.undo", Value::Null).unwrap();
    assert!(s.composition.is_some());
}

#[test]
fn ports_carry_their_ontology_keys() {
    let mut s = Session::default();
    open_extras(&mut s);
    let st = s.execute("bundle.state", Value::Null).unwrap();
    let keys: Vec<&str> = st["subnets"][0]["ports"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["pattern_port"].as_str().unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "queue-port-in",
            "queue-port-out",
            "queue-port-depth",
            "queue-port-enqueue",
            "queue-port-dequeue"
        ]
    );
    assert_eq!(
        comp::pattern_port("saga", "do:reserve").as_deref(),
        Some("saga-port-do")
    );
    assert_eq!(
        comp::pattern_port("saga", "account:customer").as_deref(),
        Some("saga-port-account")
    );
}

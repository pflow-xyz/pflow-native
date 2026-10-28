//! The search-plan commands against pflow-rs's own verdicts: the
//! prototype's receipt `tests/fixtures/views/plancheck.json` (pflow-rs
//! c396d46 via pflow-views-prototype tools/plancheck; see the README there),
//! 289 plans.
//!
//! The pin is after pflow-rs 7ba0824 ("a bounded memo and sound move
//! filters"), which changed some verdicts on purpose. Each change is named
//! below and its count asserted:
//!
//! - `KNOWN_LIST` (55): "unknown … (known: …)" now also lists the filters.
//! - `BUILT_FILTERS` (16): `must_win` and `non_losing` are built filters:
//!   legal in `filters`, "is a filter, not a …" elsewhere.
//! - `FILTERS_NULL` (1): `filters` is optional, so `"filters": null` is the
//!   default rather than a parse error.
//! - `PRESERVE_ORDER` (3): pflow-native turns on serde_json's
//!   `preserve_order`, which (features unify) pflow-rs's serde_json gets
//!   too: a JSON object keeps its key order, so plan.rs's "not {v}" prints
//!   keys as written and serde meets a budget's fields in that order. A
//!   pflow-rs build alone sorts them. Wording of malformed plans only.
//!
//! The receipt's solver prints (values and node counts on probe positions)
//! are not compared: `must_win` is on by default since 7ba0824, which
//! changes node counts by design.

use pflow_metamodel::game::Game;
use pflow_native_core::search_plan as sp;
use pflow_native_core::Session;
use serde_json::{json, Value};

fn receipt() -> Value {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/views/plancheck.json",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    serde_json::from_str(&text).unwrap()
}

/// pflow-rs `tests/common/mod.rs` `goalless()`: two moves, a reward place
/// no transition fills, no goal.
fn goalless() -> Game {
    use pflow_metamodel::{Arc, Model, Place, Transition};
    let place = |id: &str, initial| Place {
        id: id.into(),
        initial,
        ..Default::default()
    };
    let arc = |from: &str, to: &str| Arc {
        from: from.into(),
        to: to.into(),
        ..Default::default()
    };
    let m = Model {
        places: vec![place("a", 1), place("b", 0), place("won", 0)],
        transitions: ["x_go", "o_go"]
            .map(|t| Transition {
                id: t.into(),
                ..Default::default()
            })
            .to_vec(),
        arcs: vec![arc("a", "x_go"), arc("x_go", "b"), arc("b", "o_go")],
        ..Default::default()
    };
    let ix = m.indexed();
    let won = ix.places.iter().position(|p| p == "won").unwrap();
    Game::new(ix, vec![false; 2], vec![1.0, -1.0], vec![(won, 1.0)])
}

const KNOWN_OLD: &str = r#"(known: ["tt_best", "immediate_win", "history", "centrality", "threats"], ["null_window", "budget"]"#;
const KNOWN_NEW: &str = r#"(known: ["tt_best", "immediate_win", "history", "centrality", "threats"], ["must_win", "non_losing"], ["null_window", "budget"]"#;

fn noun(slot: &str) -> &str {
    match slot {
        "drivers" => "driver",
        "order" => "order piece",
        "heuristics" => "heuristic",
        _ => "filter",
    }
}

type Verdict = (bool, Option<String>, Option<String>);

/// What pflow-rs at the pin says of a receipt case, and the name of the
/// change that applies, if any.
fn expected(c: &Value) -> (Verdict, &'static str) {
    let id = c["id"].as_str().unwrap();
    let want = (
        c["ok"].as_bool().unwrap(),
        c["error"].as_str().map(String::from),
        c["step"].as_str().map(String::from),
    );
    let refuse = |e: &str| (false, Some(e.to_string()), Some("check".to_string()));
    if let Some(rest) = id.strip_prefix("page/drop/") {
        let (_, drop) = rest.split_once('/').unwrap();
        let (piece, slot) = drop.split_once('@').unwrap();
        if piece == "must_win" || piece == "non_losing" {
            return if slot == "filters" {
                ((true, None, None), "BUILT_FILTERS")
            } else {
                (
                    refuse(&format!(
                        "search plan: {piece:?} is a filter, not a {}",
                        noun(slot)
                    )),
                    "BUILT_FILTERS",
                )
            };
        }
    }
    match id {
        "edge/serde-filters-null" => return ((true, None, None), "FILTERS_NULL"),
        "edge/driver-two-keys" => {
            return (
                refuse(
                    r#"search plan: a driver is a name or an object with one key, not {"null_window":{},"budget":{"nodes":1}}"#,
                ),
                "PRESERVE_ORDER",
            )
        }
        "edge/driver-two-keys-nested" => {
            return (
                refuse(
                    r#"search plan: a driver is a name or an object with one key, not {"z":{"b":1,"a":[2,{"y":null,"x":"s"}]},"a":0}"#,
                ),
                "PRESERVE_ORDER",
            )
        }
        "edge/budget-unknown-before-bad-nodes" => {
            return (
                refuse(r#"search plan: driver "budget": invalid type: string "x", expected usize"#),
                "PRESERVE_ORDER",
            )
        }
        _ => {}
    }
    match &want.1 {
        Some(e) if e.contains(KNOWN_OLD) => (
            (want.0, Some(e.replace(KNOWN_OLD, KNOWN_NEW)), want.2),
            "KNOWN_LIST",
        ),
        _ => (want, ""),
    }
}

#[test]
fn plans_match_pflow_rs_verdicts() {
    let r = receipt();
    assert_eq!(r["pflowRs"]["rev"], "c396d46");
    let gl = goalless();
    let c4 = sp::connect4().unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for c in r["cases"].as_array().unwrap() {
        let g = if c["net"] == "goalless" { &gl } else { c4 };
        let got = sp::check_text(c["json"].as_str().unwrap(), g);
        let ((ok, error, step), change) = expected(c);
        let id = c["id"].as_str().unwrap();
        assert_eq!(got.ok, ok, "{id}");
        assert_eq!(got.error, error, "{id}");
        assert_eq!(got.step.map(String::from), step, "{id}");
        *counts.entry(change).or_insert(0) += 1;
    }
    assert_eq!(
        counts,
        [
            ("", 214),
            ("BUILT_FILTERS", 16),
            ("FILTERS_NULL", 1),
            ("KNOWN_LIST", 55),
            ("PRESERVE_ORDER", 3)
        ]
        .into_iter()
        .collect()
    );
}

fn plan_of(c: &Value) -> Value {
    serde_json::from_str(c["json"].as_str().unwrap()).unwrap()
}

/// The page's edits make the receipt's plans from the base plan. A drop in
/// `filters` adds to pflow-rs's default there (`["must_win"]` since
/// 7ba0824), where the receipt's page (before it) started from `[]`.
#[test]
fn page_edits_make_the_receipt_plans() {
    let r = receipt();
    let cases = r["cases"].as_array().unwrap();
    let base = |mode: &str| -> Value {
        plan_of(
            cases
                .iter()
                .find(|c| c["id"] == format!("page/base/{mode}"))
                .unwrap(),
        )
    };
    assert_eq!(base("exact"), sp::connect4_exact());
    let mut n = 0;
    for c in cases {
        let id = c["id"].as_str().unwrap();
        let Some(rest) = id.strip_prefix("page/") else {
            continue;
        };
        let (op, rest) = rest.split_once('/').unwrap();
        if op == "base" {
            continue;
        }
        let (mode, edit) = rest.split_once('/').unwrap();
        let b = base(mode);
        let mut want = plan_of(c);
        let got = match op {
            "drop" => {
                let (piece, slot) = edit.split_once('@').unwrap();
                if slot == "filters" {
                    let list = want["filters"].as_array().unwrap().clone();
                    want["filters"] = json!([vec![json!("must_win")], list].concat());
                }
                sp::with_piece(&b, piece, slot, None).unwrap()
            }
            "remove" => match edit.split_once('[') {
                Some((slot, i)) => {
                    sp::without_piece(&b, slot, i.trim_end_matches(']').parse().unwrap()).unwrap()
                }
                None => sp::without_piece(&b, edit, 0).unwrap(),
            },
            "move" => {
                let (slot, ix) = edit.split_once('[').unwrap();
                let (from, to) = ix.trim_end_matches(']').split_once("->").unwrap();
                sp::move_piece(&b, slot, from.parse().unwrap(), to.parse().unwrap()).unwrap()
            }
            other => panic!("{other}"),
        };
        assert_eq!(sp::canonical(&got), want, "{id}");
        n += 1;
    }
    assert_eq!(n, 130 + 14 + 28);
}

/// The page's own refusals (`slotCandidates`) for drops it cannot say, as
/// the receipt's corpus lists them.
#[test]
fn page_rules_match_the_receipt() {
    let r = receipt();
    let mut n = 0;
    for s in r["corpus"]["skipped"].as_array().unwrap() {
        let id = s["id"].as_str().unwrap();
        let rest = id.strip_prefix("page/drop/").unwrap();
        let (mode, drop) = rest.split_once('/').unwrap();
        let (piece, slot) = drop.split_once('@').unwrap();
        let mut plan = sp::connect4_exact();
        plan["mode"] = json!(mode);
        let cands = sp::slot_candidates(&plan, piece);
        let got = cands.iter().find(|c| c["slot"] == slot).unwrap();
        let want = s["page"]["why"]
            .as_str()
            .unwrap()
            .replace("at c396d46", &format!("at {}", sp::PLAN_RS_REV));
        assert_eq!(got["ok"], false, "{id}");
        assert_eq!(got["why"], want, "{id}");
        assert_eq!(got["rule"], "page", "{id}");
        n += 1;
    }
    assert_eq!(n, 94);
}

#[test]
fn palette_status_is_read_off_pflow_rs() {
    let built: Vec<&str> = sp::pieces()
        .iter()
        .filter(|p| p.status == "built")
        .map(|p| p.name)
        .collect();
    assert_eq!(
        built,
        [
            "tt_best",
            "immediate_win",
            "history",
            "centrality",
            "threats",
            "null_window",
            "budget",
            "memo",
            "must_win",
            "non_losing"
        ]
    );
    for p in sp::pieces() {
        let field = match p.kind {
            "Order" => "order",
            "Driver" => "drivers",
            "Filter" => "filters",
            "Heuristic" => "heuristics",
            _ => continue,
        };
        let mut plan = json!({ "mode": "play" });
        plan[field] = json!([p.name]);
        let c = sp::check(&plan);
        // A proposed name is refused as unknown; no other is.
        let unknown = c.error.as_deref().is_some_and(|e| e.contains("unknown"));
        assert_eq!(unknown, p.status == "proposed", "{}: {c:?}", p.name);
    }
}

#[test]
fn plan_commands_edit_with_pflow_rs_verdicts() {
    let mut s = Session::default();
    assert!(s.execute("plan.state", Value::Null).is_err());
    let open = s.execute("plan.open", Value::Null).unwrap();
    assert_eq!(open["check"]["ok"], true);
    assert_eq!(open["plan"], sp::connect4_exact());

    let c = s
        .execute("plan.slot_candidates", json!({"piece": "threats"}))
        .unwrap();
    let lit: Vec<&str> = c["slots"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|x| x["ok"] == true)
        .map(|x| x["slot"].as_str().unwrap())
        .collect();
    assert_eq!(lit, ["order"]);

    let err = s
        .execute(
            "plan.add_piece",
            json!({"piece": "beam", "slot": "filters"}),
        )
        .unwrap_err()
        .to_string();
    assert_eq!(
        err,
        "search plan: filter \"beam\" can change the value, which exact mode does not allow"
    );
    let err = s
        .execute(
            "plan.add_piece",
            json!({"piece": "mtd_f", "slot": "drivers"}),
        )
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("search plan: unknown driver \"mtd_f\""),
        "{err}"
    );

    let add = s
        .execute(
            "plan.add_piece",
            json!({"piece": "threats", "slot": "order", "index": 0}),
        )
        .unwrap();
    assert_eq!(add["plan"]["order"][0], "threats");
    let add = s
        .execute(
            "plan.add_piece",
            json!({"piece": "non_losing", "slot": "filters"}),
        )
        .unwrap();
    assert_eq!(add["plan"]["filters"], json!(["must_win", "non_losing"]));
    let mv = s
        .execute(
            "plan.move_piece",
            json!({"slot": "filters", "from": 1, "to": 0}),
        )
        .unwrap();
    assert_eq!(mv["plan"]["filters"], json!(["non_losing", "must_win"]));
    let rm = s
        .execute("plan.remove_piece", json!({"slot": "memo"}))
        .unwrap();
    assert!(rm["plan"].get("memo").is_none());
    let mode = s.execute("plan.set_mode", json!({"mode": "play"})).unwrap();
    assert_eq!(mode["check"]["ok"], true);

    // Each edit is one undo step.
    for _ in 0..5 {
        s.execute("edit.undo", Value::Null).unwrap();
    }
    let st = s.execute("plan.state", Value::Null).unwrap();
    assert_eq!(st["plan"], sp::connect4_exact());

    // A plan given as text goes to from_json verbatim.
    let bad = s
        .execute("plan.check", json!({"plan": "{\"ordr\": []}"}))
        .unwrap();
    assert_eq!(bad["step"], "parse");
    assert!(bad["error"]
        .as_str()
        .unwrap()
        .starts_with("search plan: unknown field `ordr`"));

    let pieces = s.execute("plan.pieces", Value::Null).unwrap();
    assert_eq!(pieces["slots"].as_array().unwrap().len(), 7);
    assert_eq!(pieces["plan_rs"], sp::PLAN_RS_REV);
}

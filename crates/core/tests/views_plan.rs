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
//!
//! Then pflow-rs main (7532a79 root pieces and an aspiration driver,
//! 484017f, dfb9067, 890b380 the mtd_f driver; in the aacb7eb pin):
//!
//! - `KNOWN_LIST` (46, as above): the list also names the drivers `mtd_f`
//!   and `aspiration` and the estimate `ode`.
//! - `MTD_F_BUILT` (9): `mtd_f` is a built driver: "is a driver, not a …"
//!   elsewhere, and next to `null_window` the two are refused together.
//! - `DRIVER_ONCE` (6): each driver at most once ("listed twice"), where
//!   a second `null_window` was a no-op and a second `budget` replaced
//!   the first.
//! - `BUDGET_MS` (10): `budget` takes `nodes`, `ms` or both, so its
//!   serde words and refusals changed.
//! - `ROOT_FIELD` (1): a plan has a `root` field, which serde lists.
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
const KNOWN_NEW: &str = r#"(known: ["tt_best", "immediate_win", "history", "centrality", "threats"], ["must_win", "non_losing"], ["null_window", "mtd_f", "aspiration", "budget"], ["ode"]"#;

/// pflow-rs main's budget (`{"nodes": N, "ms": T}`, either or both): the
/// receipt case and what plan.rs says now.
const BUDGET_MS: [(&str, Option<&str>); 10] = [
    (
        "rs/budget-nodes-0",
        Some(r#"search plan: driver "budget": nodes and ms must be at least 1"#),
    ),
    (
        "edge/budget-args-empty",
        Some(r#"search plan: driver "budget" takes "nodes", "ms" or both"#),
    ),
    (
        "edge/budget-args-seq",
        Some(
            r#"search plan: driver "budget": invalid length 1, expected struct Budget with 2 elements"#,
        ),
    ),
    (
        "edge/budget-args-seq-0",
        Some(
            r#"search plan: driver "budget": invalid length 1, expected struct Budget with 2 elements"#,
        ),
    ),
    (
        "edge/budget-args-seq-empty",
        Some(
            r#"search plan: driver "budget": invalid length 0, expected struct Budget with 2 elements"#,
        ),
    ),
    ("edge/budget-args-seq-two", None),
    (
        "edge/budget-unknown-field",
        Some("search plan: driver \"budget\": unknown field `extra`, expected `nodes` or `ms`"),
    ),
    (
        "edge/budget-unknown-field-first",
        Some("search plan: driver \"budget\": unknown field `extra`, expected `nodes` or `ms`"),
    ),
    (
        "edge/budget-only-unknown-field",
        Some("search plan: driver \"budget\": unknown field `zz`, expected `nodes` or `ms`"),
    ),
    (
        "edge/budget-nodes-null",
        Some(r#"search plan: driver "budget" takes "nodes", "ms" or both"#),
    ),
];

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
    if let Some((_, e)) = BUDGET_MS.iter().find(|(x, _)| *x == id) {
        return (
            match e {
                Some(e) => refuse(e),
                None => (true, None, None),
            },
            "BUDGET_MS",
        );
    }
    let twice = |n: &str| {
        (
            refuse(&format!("search plan: driver {n:?} is listed twice")),
            "DRIVER_ONCE",
        )
    };
    match id {
        "edge/null-window-twice" => return twice("null_window"),
        "edge/budget-twice-last-wins" => return twice("budget"),
        "edge/proposed-in-order" => return (refuse(r#"search plan: "mtd_f" is a driver, not a order piece"#), "MTD_F_BUILT"),
        "rs/field-ordr" => return ((false, Some("search plan: unknown field `ordr`, expected one of `mode`, `order`, `drivers`, `filters`, `root`, `heuristics`, `memo` at line 1 column 7".into()), Some("parse".into())), "ROOT_FIELD"),
        _ => {}
    }
    if let Some(rest) = id.strip_prefix("page/drop/") {
        let (_, drop) = rest.split_once('/').unwrap();
        let (piece, slot) = drop.split_once('@').unwrap();
        if slot == "drivers" && (piece == "null_window" || piece == "budget") {
            return twice(piece);
        }
        if piece == "mtd_f" {
            return (
                refuse(&if slot == "drivers" {
                    r#"search plan: drivers "null_window" and "mtd_f" both find the value by yes/no searches: name one"#.to_string()
                } else {
                    format!("search plan: \"mtd_f\" is a driver, not a {}", noun(slot))
                }),
                "MTD_F_BUILT",
            );
        }
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
            ("", 197),
            ("BUDGET_MS", 10),
            ("BUILT_FILTERS", 16),
            ("DRIVER_ONCE", 6),
            ("FILTERS_NULL", 1),
            ("KNOWN_LIST", 46),
            ("MTD_F_BUILT", 9),
            ("PRESERVE_ORDER", 3),
            ("ROOT_FIELD", 1)
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
            "non_losing",
            "ode",
            "mtd_f",
            "aspiration"
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
    assert_eq!(
        err,
        "search plan: drivers \"null_window\" and \"mtd_f\" both find the value by yes/no searches: name one"
    );
    let err = s
        .execute(
            "plan.add_piece",
            json!({"piece": "lmr", "slot": "heuristics"}),
        )
        .unwrap_err()
        .to_string();
    assert!(
        err.starts_with("search plan: unknown heuristic \"lmr\""),
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
    assert_eq!(pieces["slots"].as_array().unwrap().len(), 8);
    assert_eq!(pieces["plan_rs"], sp::PLAN_RS_REV);
}

/// pflow-rs aacb7eb's `root` field (7532a79): `{"order": ["ode", "non_losing"]}`,
/// an estimate (at most one) and the root's `non_losing`. A root slot takes
/// both; every verdict is pflow-rs's own message.
#[test]
fn the_root_slot_takes_ode_and_non_losing_with_pflow_rs_verdicts() {
    let mut s = Session::default();
    s.execute("plan.open", json!({})).unwrap();
    let cands = |s: &mut Session, name: &str| {
        s.execute("plan.slot_candidates", json!({"piece": name}))
            .unwrap()["slots"]
            .clone()
    };
    let slot = |c: &Value, id: &str| {
        c.as_array()
            .unwrap()
            .iter()
            .find(|x| x["slot"] == id)
            .unwrap()
            .clone()
    };

    // ode: legal at the root, refused elsewhere by pflow-rs.
    let c = cands(&mut s, "ode");
    assert_eq!(slot(&c, "root")["ok"], true, "{c}");
    let in_order = slot(&c, "order");
    assert_eq!(in_order["ok"], false);
    assert!(
        in_order["why"]
            .as_str()
            .unwrap()
            .contains("is a estimate, not a order piece"),
        "{in_order}"
    );
    // Something that is not a root piece is the page's refusal, not pflow-rs's.
    let c = cands(&mut s, "history");
    let r = slot(&c, "root");
    assert_eq!(
        (r["ok"].clone(), r["rule"].clone()),
        (json!(false), json!("page"))
    );

    s.execute("plan.add_piece", json!({"piece": "ode", "slot": "root"}))
        .unwrap();
    assert_eq!(
        s.execute("plan.state", Value::Null).unwrap()["plan"]["root"],
        json!({"order": ["ode"]})
    );
    // The check ran the solver with pflow-solver's ode estimate.
    assert_eq!(s.execute("plan.check", Value::Null).unwrap()["ok"], true);
    s.execute(
        "plan.add_piece",
        json!({"piece": "non_losing", "slot": "root"}),
    )
    .unwrap();
    s.execute(
        "plan.move_piece",
        json!({"slot": "root", "from": 1, "to": 0}),
    )
    .unwrap();
    assert_eq!(
        s.execute("plan.state", Value::Null).unwrap()["plan"]["root"],
        json!({"order": ["non_losing", "ode"]})
    );
    // A second estimate is pflow-rs's refusal (only one exists, so: twice).
    let e = s
        .execute("plan.add_piece", json!({"piece": "ode", "slot": "root"}))
        .unwrap_err();
    assert!(
        e.to_string().contains("listed twice") || e.to_string().contains("second estimate"),
        "{e}"
    );

    // aspiration needs the estimate: refused without it, accepted with it.
    s.execute(
        "plan.add_piece",
        json!({"piece": "aspiration", "slot": "drivers"}),
    )
    .unwrap();
    s.execute("plan.remove_piece", json!({"slot": "root", "index": 1}))
        .unwrap();
    let c = s.execute("plan.check", Value::Null).unwrap();
    assert_eq!(c["ok"], false, "{c}");
    assert!(
        c["error"]
            .as_str()
            .unwrap()
            .contains("starts from the root's estimate"),
        "{c}"
    );
    // Removing the last root piece drops the field, as absent is the same.
    s.execute("plan.remove_piece", json!({"slot": "root", "index": 0}))
        .unwrap();
    assert!(s.execute("plan.state", Value::Null).unwrap()["plan"]
        .get("root")
        .is_none());
}

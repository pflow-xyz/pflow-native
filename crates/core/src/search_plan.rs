//! The open search plan of the composition views (the Connect-4 exact plan
//! of pflow-rs `crates/pflow-metamodel/src/plan.rs`), its piece palette and
//! the solver template's slots: the prototype's `js/model/plan.js`, except
//! that every verdict is pflow-rs's own. A plan is checked by
//! [`pflow_metamodel::SearchPlan::from_json`], `check()` and
//! `solver(&game)` on the Connect-4 net, and refused with their messages.
//!
//! What is this crate's (the page's): the palette (which names exist, with
//! their status: `built` from pflow-rs's own name lists, `reserved`,
//! `proposed`), the slots, and how a drop edits the plan ([`with_piece`],
//! [`without_piece`], [`move_piece`]). An absent list field stands for
//! pflow-rs's default for it (order: `Chain::default`; drivers:
//! `null_window`; filters: `Filter::DEFAULT`), so a drop adds to that
//! default rather than replacing it.
//!
//! Since pflow-rs 7ba0824 (in the pin) `filters` is a real field:
//! `must_win` and `non_losing` are built, an absent `filters` means
//! `["must_win"]` and `[]` runs none. The prototype's receipts are from
//! c396d46, before that; `tests/views_plan.rs` names each case that changed.

use std::sync::OnceLock;

use pflow_metamodel::game::Game;
use pflow_metamodel::{Chain, Filter, Piece, SearchPlan};
use serde::Serialize;
use serde_json::{json, Map, Value};

/// The plan.rs module-doc example, which the ontology ROADMAP names
/// `plan:connect4-exact`.
pub fn connect4_exact() -> Value {
    json!({
        "mode": "exact",
        "order": ["tt_best", "immediate_win", "centrality", "history"],
        "drivers": ["null_window", {"budget": {"nodes": 1000000}}],
        "memo": {"entries": 500000},
    })
}

/// The open plan (`plan.*`): the plan's JSON, on the Connect-4 net.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenPlan {
    pub plan: Value,
}

/// The pflow-rs revision whose `plan.rs` decides (the pinned one).
pub const PLAN_RS_REV: &str = "aacb7eb";

/// The Connect-4 net the plan is for: pflow-rs's fixture
/// `crates/pflow-metamodel/tests/fixtures/connect-4.json`, byte-identical
/// to `examples/connect-4.json` here.
pub const CONNECT4_JSON: &str = include_str!("../../../examples/connect-4.json");

/// The Connect-4 game, declared as pflow-rs's search tests declare it
/// (`tests/common/mod.rs` `game`): moves of `x_*` transitions are x's,
/// the rest o's; `*win*` transitions fire automatically; the reward is
/// `win_x` minus `win_o`.
pub fn connect4() -> Result<&'static Game, String> {
    static G: OnceLock<Result<Game, String>> = OnceLock::new();
    G.get_or_init(|| {
        let (model, _) =
            pflow_parser::model_from_json(CONNECT4_JSON.as_bytes()).map_err(|e| e.to_string())?;
        let ix = model.indexed();
        let auto = ix.transitions.iter().map(|t| t.contains("win")).collect();
        let sign = ix
            .transitions
            .iter()
            .map(|t| if t.starts_with("x_") { 1.0 } else { -1.0 })
            .collect();
        let at = |id: &str| {
            ix.places
                .iter()
                .position(|p| p == id)
                .ok_or_else(|| format!("connect-4 has no place {id:?}"))
        };
        let reward = vec![(at("win_x")?, 1.0), (at("win_o")?, -1.0)];
        Ok(Game::new(ix, auto, sign, reward))
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// The Connect-4 net as pflow-metamodel reads it.
pub fn connect4_model() -> Result<&'static pflow_metamodel::Model, String> {
    static M: OnceLock<Result<pflow_metamodel::Model, String>> = OnceLock::new();
    M.get_or_init(|| {
        pflow_parser::model_from_json(CONNECT4_JSON.as_bytes())
            .map(|(m, _)| m)
            .map_err(|e| e.to_string())
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// The prototype's fixed opening (process view): both sides into the centre
/// column, then its neighbours.
pub const OPENING: [&str; 4] = ["x_play_3_0", "o_play_3_1", "x_play_2_0", "o_play_4_0"];

/// `plan.opening`'s result: the opening fired from the initial marking
/// through pflow-metamodel's enabled_why_not / fire, stopping at the first
/// drop that cannot fire.
pub fn opening_json() -> Value {
    let m = match connect4_model() {
        Ok(m) => m,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    let mut mk = m.initial_marking();
    let mut steps = vec![];
    let mut fired = 0;
    for t in OPENING {
        match m.enabled_why_not(t, &mk) {
            Ok(()) => {
                mk = m.fire(t, &mk);
                fired += 1;
                steps.push(json!({"transition": t, "enabled": true, "why": ""}));
            }
            Err(e) => {
                steps.push(json!({"transition": t, "enabled": false, "why": e.to_string()}));
                break;
            }
        }
    }
    json!({"ok": true, "id": "opening", "steps": steps, "fired": fired})
}

/// `plan.net`'s result: the Connect-4 net's size.
pub fn net_json() -> Value {
    match connect4_model() {
        Ok(m) => json!({
            "id": "connect-4",
            "places": m.places.len(),
            "transitions": m.transitions.len(),
            "arcs": m.arcs.len(),
        }),
        Err(e) => json!({"error": e}),
    }
}

/// pflow-rs's verdict on a plan.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Check {
    pub ok: bool,
    /// The first refusal's message, verbatim.
    pub error: Option<String>,
    /// Where it was refused: `"parse"` (`SearchPlan::from_json`), `"check"`
    /// or `"solver"` (what the net cannot support).
    pub step: Option<&'static str>,
}

/// `SearchPlan::from_json(text)`, then `check()`, then `solver(g)`.
pub fn check_text(text: &str, g: &Game) -> Check {
    let refuse = |step, e: pflow_metamodel::Error| Check {
        ok: false,
        error: Some(e.to_string()),
        step: Some(step),
    };
    let plan = match SearchPlan::from_json(text) {
        Ok(p) => p,
        Err(e) => return refuse("parse", e),
    };
    if let Err(e) = plan.check() {
        return refuse("check", e);
    }
    // pflow_solver::game::solver is plan.solver(g) plus the `ode` estimate
    // a root may name (pflow-metamodel builds none).
    if let Err(e) = pflow_solver::game::solver(&plan, g) {
        return refuse("solver", e);
    }
    Check {
        ok: true,
        error: None,
        step: None,
    }
}

/// [`check_text`] of a plan on the Connect-4 net.
pub fn check(plan: &Value) -> Check {
    match connect4() {
        Ok(g) => check_text(&plan.to_string(), g),
        Err(e) => Check {
            ok: false,
            error: Some(format!("the Connect-4 net does not load: {e}")),
            step: Some("solver"),
        },
    }
}

// ------------------------------------------------------------ palette

/// A name the palette offers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PieceInfo {
    pub name: &'static str,
    /// `Order`, `Driver`, `Filter`, `Heuristic` or `Memo`.
    pub kind: &'static str,
    /// `built` (pflow-rs takes it), `reserved` (named only to be refused)
    /// or `proposed` (ROADMAP Phase 5, not built: refused as unknown).
    pub status: &'static str,
    /// `order-only`, `value-preserving`, `partial`, `sound-pruning` or
    /// `value-changing`.
    pub contract: &'static str,
    pub doc: &'static str,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub args: Value,
    pub needs_goals: bool,
    /// An order piece whose key puts moves in a top tier (order.rs:
    /// decisive pieces are compared first); the prototype's brackets.
    pub decisive: bool,
    /// What an order piece reads (the prototype's `reads`), or "".
    pub reads: &'static str,
}

fn piece(
    name: &'static str,
    kind: &'static str,
    contract: &'static str,
    doc: &'static str,
) -> PieceInfo {
    PieceInfo {
        name,
        kind,
        status: "",
        contract,
        doc,
        args: Value::Null,
        needs_goals: false,
        decisive: false,
        reads: "",
    }
}

/// The drivers plan.rs takes (its private `DRIVERS`).
pub const DRIVERS: [&str; 4] = ["null_window", "mtd_f", "aspiration", "budget"];

/// The estimates plan.rs knows (its private `ESTIMATES`); pflow-solver
/// supplies `ode`.
pub const ESTIMATES: [&str; 1] = ["ode"];

/// What a root's `order` takes besides an estimate: the `non_losing`
/// filter, run once at the position searched.
pub const ROOT_FILTER: &str = "non_losing";

/// The palette, in the prototype's order (`PIECES`), with each status
/// read off pflow-rs where it can be (`Piece::NAMES`, `Filter::NAMES`).
pub fn pieces() -> &'static [PieceInfo] {
    static P: OnceLock<Vec<PieceInfo>> = OnceLock::new();
    P.get_or_init(|| {
        let mut v = vec![
            piece("tt_best", "Order", "order-only", "The move the transposition table remembers as best."),
            piece("immediate_win", "Order", "order-only", "A move that ends the game in the mover's favour."),
            piece("history", "Order", "order-only", "Moves that caused cutoffs before, per transition."),
            PieceInfo { needs_goals: true, ..piece("centrality", "Order", "order-only", "Moves that play towards more of the mover's goals: the goals its output places feed, fixed per transition. Needs goals.") },
            PieceInfo { needs_goals: true, ..piece("threats", "Order", "order-only", "Moves after which more of the mover's goals that its output places feed are one token short of enabled. Needs goals.") },
            piece("null_window", "Driver", "value-preserving", "Finds a value by yes/no searches, one per gap between adjacent rewards. On by default; off when drivers is given without it."),
            PieceInfo { args: json!({"nodes": 1000000}), ..piece("budget", "Driver", "partial", "Gives a value up on a position past N nodes: the exact value or None, never a wrong value.") },
            PieceInfo { args: json!({"entries": 500000}), ..piece("memo", "Memo", "value-preserving", "Keeps the transposition table to N entries.") },
            piece("must_win", "Filter", "sound-pruning", "Drops moves only where the value is already known: a winning move found, the rest need not be searched. On by default (Filter::DEFAULT)."),
            piece("non_losing", "Filter", "sound-pruning", "Drops moves that lose at once, checking replies at every node: for a few positions, not a whole search."),
            piece("beam", "Filter", "value-changing", "Reserved name. Can change the value; refused in exact mode, and not built in play mode."),
            piece("static_eval", "Heuristic", "value-changing", "Reserved name. Can change the value; refused in exact mode, and not built in play mode."),
            piece("ode", "Estimate", "order-only", "The root's best guess: the mass-action ODE of the net from the position searched (pflow_solver::game::OdeEstimate). It orders the root's moves, best guess first, and the aspiration driver starts where it points; it never changes a value. At most one estimate in root.order."),
            piece("mtd_f", "Driver", "value-preserving", "Finds the value by yes/no searches, choosing each next one from the bound the last proved (Solver::with_mtd_f). Not together with null_window."),
            piece("aspiration", "Driver", "value-preserving", "Starts the search from the root's estimate, which root.order must name (Solver::with_aspiration): drop ode in the root slot first, or the check refuses it."),
            piece("forced_moves", "Filter", "sound-pruning", "Falls out of non_losing (approved candidate). Not built."),
            piece("lmr", "Heuristic", "value-changing", "Late move reductions, play mode only (approved candidate). Not built."),
            piece("futility", "Filter", "value-changing", "Futility pruning, play mode only (approved candidate). Not built."),
        ];
        for p in &mut v {
            (p.decisive, p.reads) = match p.name {
                "tt_best" => (true, "memo (Entry.best)"),
                "immediate_win" => (true, "the next position"),
                "history" => (false, "history counts, written by cutoff"),
                "centrality" => (false, "Game::centrality"),
                "threats" => (false, "Game::fed, deficit()"),
                _ => (false, ""),
            };
            p.status = if Piece::NAMES.contains(&p.name)
                || Filter::NAMES.contains(&p.name)
                || DRIVERS.contains(&p.name)
                || ESTIMATES.contains(&p.name)
                || p.name == "memo"
            {
                "built"
            } else if p.name == "beam" || p.name == "static_eval" {
                "reserved"
            } else {
                "proposed"
            };
        }
        v
    })
}

pub fn piece_info(name: &str) -> Option<&'static PieceInfo> {
    pieces().iter().find(|p| p.name == name)
}

/// A slot of the solver template (ROADMAP Phase 6: Solver = drivers ∘
/// core[order chain, filters, bounds, memo]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Slot {
    pub id: &'static str,
    pub accepts: &'static str,
    /// The plan field it edits; `None` for a slot designed in the ROADMAP
    /// and not in plan.rs (shown, never fillable).
    pub field: Option<&'static str>,
    pub label: &'static str,
    pub designed: bool,
}

pub const SLOTS: [Slot; 8] = [
    Slot {
        id: "drivers",
        accepts: "Driver",
        field: Some("drivers"),
        label: "drivers",
        designed: false,
    },
    Slot {
        id: "order",
        accepts: "Order",
        field: Some("order"),
        label: "order chain",
        designed: false,
    },
    Slot {
        id: "filters",
        accepts: "Filter",
        field: Some("filters"),
        label: "filters",
        designed: false,
    },
    Slot {
        id: "root",
        accepts: "Root",
        field: Some("root"),
        label: "root",
        designed: false,
    },
    Slot {
        id: "heuristics",
        accepts: "Heuristic",
        field: Some("heuristics"),
        label: "heuristics",
        designed: false,
    },
    Slot {
        id: "memo",
        accepts: "Memo",
        field: Some("memo"),
        label: "memo",
        designed: false,
    },
    Slot {
        id: "bounds",
        accepts: "Bound",
        field: None,
        label: "bounds",
        designed: true,
    },
    Slot {
        id: "engine",
        accepts: "Engine",
        field: None,
        label: "engine",
        designed: true,
    },
];

pub fn slot(id: &str) -> Option<&'static Slot> {
    SLOTS.iter().find(|s| s.id == id)
}

// ------------------------------------------------------------ edits

/// The plan's JSON with keys in plan.rs's field order. `filters` is kept
/// when present, even empty (`[]` runs none, absent runs the default);
/// `heuristics` only when non-empty (absent and `[]` are the same).
pub fn canonical(plan: &Value) -> Value {
    let mut out = Map::new();
    let o = plan.as_object().cloned().unwrap_or_default();
    for k in [
        "mode",
        "order",
        "drivers",
        "filters",
        "root",
        "heuristics",
        "memo",
    ] {
        match o.get(k) {
            None | Some(Value::Null) => {}
            Some(Value::Array(a)) if a.is_empty() && k == "heuristics" => {}
            Some(v) => {
                out.insert(k.into(), v.clone());
            }
        }
    }
    // Anything else (an unknown field) is kept, for plan.rs to refuse.
    for (k, v) in o {
        out.entry(k).or_insert(v);
    }
    Value::Object(out)
}

/// A list field as the solver reads it: explicit, or the default an absent
/// field stands for.
pub fn field_or_default(plan: &Value, field: &str) -> Vec<Value> {
    if let Some(a) = plan.get(field).and_then(Value::as_array) {
        return a.clone();
    }
    if field == "root" {
        return plan
            .pointer("/root/order")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
    }
    match field {
        "order" => Chain::default()
            .pieces()
            .iter()
            .map(|p| json!(p.name()))
            .collect(),
        "drivers" => vec![json!("null_window")],
        "filters" => Filter::DEFAULT.iter().map(|f| json!(f.name())).collect(),
        _ => vec![],
    }
}

/// The plan with `name` added to `slot` (at `index`, default the end), or
/// `None` when a drop there cannot be said (a designed slot, a name in
/// `memo`).
pub fn with_piece(plan: &Value, name: &str, slot_id: &str, index: Option<usize>) -> Option<Value> {
    let s = slot(slot_id)?;
    let field = s.field?;
    let mut p = plan.clone();
    if field == "memo" {
        if name != "memo" {
            return None;
        }
        let entries = piece_info("memo").map_or(json!(500000), |x| x.args["entries"].clone());
        p["memo"] = json!({ "entries": entries });
        return Some(p);
    }
    // budget carries its arguments only as a driver; in a list of names it
    // is named, so plan.rs refuses it by name instead of serde by type.
    let entry = if name == "budget" && field == "drivers" {
        let nodes = piece_info("budget").map_or(json!(1000000), |x| x.args["nodes"].clone());
        json!({ "budget": { "nodes": nodes } })
    } else {
        json!(name)
    };
    let mut list = field_or_default(plan, field);
    match index {
        Some(i) if i <= list.len() => list.insert(i, entry),
        _ => list.push(entry),
    }
    set_list(&mut p, field, list);
    Some(p)
}

/// Writes a slot's list back: `root` holds it as `{"order": [...]}` (and
/// is dropped when empty, which is the same as absent); the others as the
/// array itself.
fn set_list(p: &mut Value, field: &str, list: Vec<Value>) {
    if field == "root" {
        if list.is_empty() {
            if let Some(o) = p.as_object_mut() {
                o.remove("root");
            }
        } else {
            p["root"] = json!({ "order": list });
        }
    } else {
        p[field] = Value::Array(list);
    }
}

/// The plan with entry `index` of `slot` removed (`memo`: the memo).
pub fn without_piece(plan: &Value, slot_id: &str, index: usize) -> Result<Value, String> {
    let s = slot(slot_id).ok_or_else(|| format!("no slot {slot_id:?}"))?;
    let field = s
        .field
        .ok_or_else(|| format!("slot {slot_id:?} is designed in the ROADMAP, not a plan field"))?;
    let mut p = plan.clone();
    if field == "memo" {
        if p.get("memo").is_none_or(Value::is_null) {
            return Err("the plan has no memo".into());
        }
        p.as_object_mut()
            .expect("a plan is an object")
            .remove("memo");
        return Ok(p);
    }
    let mut list = field_or_default(plan, field);
    if index >= list.len() {
        return Err(format!(
            "slot {slot_id:?} has {} entries, no index {index}",
            list.len()
        ));
    }
    list.remove(index);
    // An empty list stays explicit: an empty chain is the order monoid's
    // identity, `"drivers": []` turns null_window off and `"filters": []`
    // runs none; none of these is the default. (An empty root is no root.)
    set_list(&mut p, field, list);
    Ok(p)
}

/// The plan with entry `from` of a list slot moved to `to` (an absent list
/// is its default, made explicit).
pub fn move_piece(plan: &Value, slot_id: &str, from: usize, to: usize) -> Result<Value, String> {
    let s = slot(slot_id).ok_or_else(|| format!("no slot {slot_id:?}"))?;
    let field = match s.field {
        Some(f) if f != "memo" => f,
        _ => return Err(format!("slot {slot_id:?} holds no list")),
    };
    let mut list = field_or_default(plan, field);
    if from >= list.len() || to >= list.len() {
        return Err(format!(
            "slot {slot_id:?} has {} entries; cannot move {from} to {to}",
            list.len()
        ));
    }
    let x = list.remove(from);
    list.insert(to, x);
    let mut p = plan.clone();
    set_list(&mut p, field, list);
    Ok(p)
}

/// Where `name` may go: one verdict per slot (`slotCandidates`). A drop is
/// legal only when the resulting plan passes pflow-rs's check and solver
/// for Connect-4; `why` is then pflow-rs's message, or the page's when the
/// drop cannot be said at all.
pub fn slot_candidates(plan: &Value, name: &str) -> Vec<Value> {
    SLOTS
        .iter()
        .map(|s| {
            let no = |why: String| json!({"slot": s.id, "ok": false, "why": why, "rule": "page", "plan": null});
            if s.designed {
                return no(format!(
                    "designed in the ROADMAP, not a field of SearchPlan at {PLAN_RS_REV}"
                ));
            }
            if s.field == Some("memo") {
                if name != "memo" {
                    return no(format!("memo takes {{\"entries\": N}}, not {name:?}"));
                }
                if plan.get("memo").is_some_and(|m| !m.is_null()) {
                    return no("the plan already has a memo".into());
                }
            }
            if s.field == Some("root") && !ESTIMATES.contains(&name) && name != ROOT_FILTER {
                return no(format!(
                    "the root takes an estimate ({}) or {ROOT_FILTER:?}, not {name:?}",
                    ESTIMATES.join(", ")
                ));
            }
            if name == "memo" && s.field != Some("memo") {
                return no(format!("memo is not a {} entry", s.label));
            }
            let Some(next) = with_piece(plan, name, s.id, None) else {
                return no("the page has no way to say this drop".into());
            };
            let c = check(&canonical(&next));
            json!({"slot": s.id, "ok": c.ok, "why": c.error, "rule": if c.ok { Value::Null } else { json!("pflow-rs") }, "step": c.step, "plan": canonical(&next)})
        })
        .collect()
}

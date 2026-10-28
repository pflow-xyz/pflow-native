//! A net as a two-sided game (`game.*`): the exact value of the session's
//! position and a best move there, by pflow-metamodel's alpha-beta search
//! run as a [`SearchPlan`] names it. The game is read from the `sides`,
//! `reward` and `auto` arguments ([`crate::game`]); the search fires through
//! the metamodel's firing rule and changes neither the net nor the marking.
//! Each result is also stored as a Report artifact.

use pflow_metamodel::{Book, SearchPlan};
use serde_json::{json, Value};

use crate::analysis::Structure;
use crate::artifact::ArtifactData;
use crate::game::{game_rules, Dense};
use crate::registry::{always, arg, n, opt, s, ArgSpec, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::{Error, Result};

const MENU: &str = "Analyze";

/// Nodes searched when neither the plan nor `nodes` sets a budget.
pub const NODES: usize = 1_000_000;

/// The largest node budget taken.
pub const MAX_NODES: usize = 100_000_000;

/// Transposition-table entries when the plan names none: small enough for
/// a browser tab.
pub const MEMO: usize = 1 << 20;

/// The largest table taken (pflow-metamodel's own default).
pub const MAX_MEMO: usize = 1 << 23;

const ARGS: &[ArgSpec] = &[
    arg("sides", Json, "{transition-id prefix: sign}: who moves, e.g. {\"x_\": 1, \"o_\": -1} (+1 maximizes the reward)"),
    arg("reward", Json, "{place: weight} read from the final marking, from the +1 side's view, e.g. {\"win_x\": 1, \"win_o\": -1}"),
    opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled, before anyone chooses (e.g. [\"x_win_\", \"o_win_\"]: win claims)"),
    opt("plan", Json, "a SearchPlan (pflow-metamodel), or a JSON string of one: {mode, order, filters, root, drivers, memo}; default {}. drivers: \"null_window\" (default), \"mtd_f\" (not together with null_window), {\"budget\": {\"nodes\": n}}. A time budget (budget.ms) is refused. Root estimate \"ode\" solves the mass-action ODE with every transition at rate 1, not the session's rates"),
    opt("nodes", Int, "the node budget when the plan's drivers set none (default 1,000,000; at most 100,000,000)"),
    opt("book", Str, "an opening book: the text of a label cache as the experimental rnn commands write it to `labels` (JSONL, a header line naming the net and rules, then one position per line); its positions are pinned in the table, exact"),
];

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "game.value",
        label: "Game Value…",
        help: "The exact value of the current position, from the +1 side's view, with both sides playing perfectly: \
               pflow-metamodel's alpha-beta search over the firing rule, run as `plan` names it. The game is the net read \
               through `sides` (who owns each transition), `reward` (read off the final marking) and `auto` (transitions \
               nobody chooses, fired first). The value is null when the search runs past its node budget (`nodes` then \
               reports the budget). A plan and budget give the same result on every machine, native or browser, the \
               \"ode\" root estimate included: it solves the mass-action ODE with every transition at rate 1, not the \
               session's rates, and its step control uses pflow-solver's correctly rounded `crmath::pow`, not the \
               platform's `powf`. Changes neither the net nor the marking; the result is stored as a Report.",
        menu: MENU,
        shortcut: None,
        args: ARGS,
        enabled: always,
        run: value,
        journal: false,
    },
    CommandSpec {
        id: "game.move",
        label: "Best Move…",
        help: "A best move at the current position and its exact value (pflow-metamodel's Solver::choose, run as `plan` \
               names it; the game as for game.value). It is a best move: another plan may pick another, equally good one. \
               Past the node budget there is no move (transition null, reason \"past the budget\"), unless the plan's \
               mode is \"play\", where the move is a guess (exact false): the table's best move at the position if it \
               has one, else the first the plan's root pieces rank, which is the net's first enabled move when the root \
               names none, so give a root order (e.g. [\"ode\"]) for a guess worth playing. Deterministic as \
               game.value is: the \"ode\" estimate runs every transition at rate 1, not the session's rates, and can \
               search a different number of nodes in the browser build than natively. The move is not fired: sim.fire \
               it. The result is stored as a Report.",
        menu: MENU,
        shortcut: None,
        args: ARGS,
        enabled: always,
        run: choose,
        journal: false,
    },
];

/// The plan `a` gives, with the defaults filled in: the node budget (`nodes`
/// when the plan's drivers set none) and the table size. Returns the plan
/// and its node budget.
fn plan(a: &Value) -> Result<(SearchPlan, usize)> {
    let bad = |m: String| Error::BadArgs(format!("plan: {m}"));
    let mut p = match a.get("plan").filter(|x| !x.is_null()) {
        None => json!({}),
        Some(Value::String(text)) => {
            serde_json::from_str::<Value>(text).map_err(|e| bad(e.to_string()))?
        }
        Some(x) => x.clone(),
    };
    let obj = p
        .as_object_mut()
        .ok_or_else(|| bad("a SearchPlan is a JSON object".into()))?;
    let nodes = match n(a, "nodes") {
        None => None,
        Some(x) if x >= 1.0 && x <= MAX_NODES as f64 => Some(x as usize),
        Some(x) => {
            return Err(Error::BadArgs(format!(
                "nodes must be in 1..{MAX_NODES}, got {x}"
            )))
        }
    };
    // Null window is on by default and off when `drivers` is given without
    // it, so a budget added to a plan without drivers keeps it.
    let drivers = obj
        .entry("drivers")
        .or_insert_with(|| json!(["null_window"]));
    if drivers.is_null() {
        *drivers = json!(["null_window"]);
    }
    let mut budget = None;
    if let Some(list) = drivers.as_array_mut() {
        for d in list.iter() {
            let Some(b) = d.get("budget") else { continue };
            if b.get("ms").is_some_and(|x| !x.is_null()) {
                return Err(bad(
                    "a time budget (budget.ms) is refused: the result would depend on the machine, and the \
                     clock it needs is not there in the browser (wasm32); give a node budget"
                        .into(),
                ));
            }
            // The last budget is the one the plan runs.
            budget = b.get("nodes").and_then(Value::as_u64);
        }
        match (budget, nodes) {
            (Some(_), Some(_)) => {
                return Err(Error::BadArgs(
                    "give the node budget once: in the plan's drivers or as nodes".into(),
                ))
            }
            (Some(b), None) if b > MAX_NODES as u64 => {
                return Err(bad(format!(
                    "budget.nodes must be at most {MAX_NODES}, got {b}"
                )))
            }
            (Some(_), None) => {}
            (None, _) => {
                let b = nodes.unwrap_or(NODES);
                list.push(json!({"budget": {"nodes": b}}));
                budget = Some(b as u64);
            }
        }
    }
    match obj.get("memo") {
        None | Some(Value::Null) => {
            obj.insert("memo".into(), json!({ "entries": MEMO }));
        }
        Some(m) => {
            if let Some(e) = m.get("entries").and_then(Value::as_u64) {
                if e > MAX_MEMO as u64 {
                    return Err(bad(format!(
                        "memo.entries must be at most {MAX_MEMO}, got {e}"
                    )));
                }
            }
        }
    }
    let plan = SearchPlan::from_json(&p.to_string())?;
    plan.check()?;
    // `check` refused drivers that are not a list, so a budget was set.
    Ok((plan, budget.map_or(NODES, |b| b as usize)))
}

/// What both commands search: the game, its plan, the settled position
/// and its moves.
struct Search {
    st: Structure,
    d: Dense,
    plan: SearchPlan,
    budget: usize,
    book: Option<Book>,
    mk: Vec<i64>,
    moves: Vec<usize>,
}

impl Search {
    fn new(sess: &Session, a: &Value) -> Result<Self> {
        let st = Structure::new(&sess.doc, &sess.marking)?;
        let rules = game_rules(&st, a)?;
        let (plan, budget) = plan(a)?;
        let d = Dense::new(&st, &rules)?;
        let book = s(a, "book").map(|text| book(&d, text)).transpose()?;
        let mk = d.settled(&st, &sess.marking)?;
        let moves = d.rules.moves(&mk);
        Ok(Self {
            st,
            d,
            plan,
            budget,
            book,
            mk,
            moves,
        })
    }

    /// A solver running the plan, the book pinned in its table. The book
    /// goes in after the solver is built: building it sizes the table.
    fn solver(&self) -> Result<pflow_metamodel::game::Solver<'_>> {
        let solver = pflow_solver::game::solver(&self.plan, &self.d.rules)?;
        Ok(match &self.book {
            Some(book) => solver.with_book(book),
            None => solver,
        })
    }

    /// The nodes `solver` searched. Past the budget it counts the node that
    /// stopped it, one more than the budget: that one is not reported.
    fn nodes(&self, solver: &pflow_metamodel::game::Solver<'_>) -> usize {
        solver.nodes().min(self.budget)
    }

    fn transition(&self, t: usize) -> &str {
        &self.st.transitions[self.d.to_view[t]]
    }

    /// What both results carry.
    fn common(&self) -> Value {
        json!({
            "budget": self.budget,
            "book": self.book.as_ref().map(Book::len),
        })
    }
}

/// The book a label cache `text` holds for `d`. A line with no values (a
/// final position) is refused here: pflow-metamodel's reader panics on one.
fn book(d: &Dense, text: &str) -> Result<Book> {
    for (n, line) in text.lines().enumerate().skip(1) {
        let Ok(x) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if x.get("vals")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
        {
            return Err(Error::BadArgs(format!(
                "book line {}: no values; a label line values the moves of a position that has some",
                n + 1
            )));
        }
    }
    Ok(Book::from_labels(&d.rules, text)?)
}

/// `out` stored as a Report artifact, its id added.
fn stored(sess: &mut Session, id: &str, title: &str, a: &Value, mut out: Value) -> Value {
    let source = Step {
        command: id.into(),
        args: a.clone(),
    };
    let art = sess.add_artifact(title.into(), source, ArtifactData::Report(out.clone()));
    out["artifact"] = json!(art);
    out
}

fn merge(mut out: Value, more: Value) -> Value {
    if let (Some(o), Value::Object(m)) = (out.as_object_mut(), more) {
        o.extend(m);
    }
    out
}

fn value(sess: &mut Session, a: &Value) -> Result<Value> {
    let x = Search::new(sess, a)?;
    if x.moves.is_empty() {
        let score = x.d.rules.score(&x.mk)?;
        let out = merge(
            json!({"value": score, "exact": true, "nodes": 0, "mover": null, "moves": 0}),
            x.common(),
        );
        return Ok(stored(sess, "game.value", "Game value", a, out));
    }
    let mover = x.d.mover(&x.moves)?;
    let mut solver = x.solver()?;
    let v = solver.value(&x.mk)?;
    let out = merge(
        json!({
            "value": v, "exact": v.is_some(), "nodes": x.nodes(&solver), "mover": mover,
            "moves": x.moves.len(),
        }),
        x.common(),
    );
    Ok(stored(sess, "game.value", "Game value", a, out))
}

fn choose(sess: &mut Session, a: &Value) -> Result<Value> {
    let x = Search::new(sess, a)?;
    if x.moves.is_empty() {
        let score = x.d.rules.score(&x.mk)?;
        let out = merge(
            json!({
                "transition": null, "value": score, "exact": true, "nodes": 0,
                "reason": "no move: the position is final",
            }),
            x.common(),
        );
        return Ok(stored(sess, "game.move", "Best move", a, out));
    }
    x.d.mover(&x.moves)?;
    let mut solver = x.solver()?;
    let out = match solver.choose(&x.mk)? {
        Some(c) => {
            let mut out = json!({
                "transition": x.transition(c.t), "value": c.value, "exact": c.exact, "nodes": x.nodes(&solver),
            });
            if !c.exact {
                out["reason"] = json!("past the budget: a guess (mode \"play\"), not proven best");
            }
            out
        }
        None => json!({
            "transition": null, "value": null, "exact": false, "nodes": x.nodes(&solver),
            "reason": "past the budget: no move was proven best within it (mode \"play\" guesses one)",
        }),
    };
    let out = merge(out, x.common());
    Ok(stored(sess, "game.move", "Best move", a, out))
}

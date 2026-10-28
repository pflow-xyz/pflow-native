//! The open search plan (`plan.*`) for the composition views: the
//! Connect-4 exact plan of pflow-rs `plan.rs`, its piece palette and the
//! solver template's slots. Every verdict is pflow-rs's
//! (`SearchPlan::from_json`, `check`, `solver` on Connect-4), with its own
//! message; the logic lives in [`crate::search_plan`].

use serde_json::{json, Value};

use crate::registry::{always, arg, opt, req, ArgType::*, CommandSpec};
use crate::search_plan::{self as sp, OpenPlan};
use crate::session::Session;
use crate::{Error, Result};

fn need(sess: &Session) -> std::result::Result<(), String> {
    if sess.search_plan.is_some() {
        Ok(())
    } else {
        Err("no search plan is open (plan.open)".into())
    }
}

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "plan.pieces",
        label: "Plan Pieces",
        help: "The piece palette (name, kind, status built/reserved/proposed, contract, doc, args, \
               needs_goals) and the solver template's slots (drivers, order, filters, heuristics, \
               memo; bounds and engine are designed, never fillable).",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: always,
        run: pieces,
        journal: false,
    },
    CommandSpec {
        id: "plan.open",
        label: "Open Search Plan",
        help: "Open a search plan on the Connect-4 net, replacing any open one: `plan` (an object), \
               or the plan.rs module-doc example (the Connect-4 exact plan). Opened even when \
               pflow-rs refuses it; the result says so. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[opt("plan", Json, "a SearchPlan object (default: the Connect-4 exact plan)")],
        enabled: always,
        run: open,
        journal: true,
    },
    CommandSpec {
        id: "plan.close",
        label: "Close Search Plan",
        help: "Close the open search plan. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[],
        enabled: need,
        run: close,
        journal: true,
    },
    CommandSpec {
        id: "plan.state",
        label: "Search Plan State",
        help: "The open plan (keys in plan.rs order), its JSON text, and pflow-rs's verdict on it.",
        menu: "",
        shortcut: None,
        args: &[],
        enabled: need,
        run: state,
        journal: false,
    },
    CommandSpec {
        id: "plan.check",
        label: "Check Search Plan",
        help: "pflow-rs's verdict on a plan for Connect-4: SearchPlan::from_json, check(), then \
               solver(); {ok, error (pflow-rs's message), step: parse|check|solver}. `plan` is an \
               object, or a string passed to from_json verbatim; default the open plan.",
        menu: "Compose",
        shortcut: None,
        args: &[opt("plan", Json, "a plan object or JSON text (default: the open plan)")],
        enabled: always,
        run: check,
        journal: false,
    },
    CommandSpec {
        id: "plan.slot_candidates",
        label: "Slot Candidates",
        help: "Where a piece may go in the open plan: per slot, ok and why (pflow-rs's message when \
               it refuses the resulting plan, rule \"pflow-rs\"; the page's when the drop cannot be \
               said, rule \"page\"), and the plan the drop would make.",
        menu: "",
        shortcut: None,
        args: &[arg("piece", Str, "a palette name (plan.pieces)")],
        enabled: need,
        run: slot_candidates,
        journal: false,
    },
    CommandSpec {
        id: "plan.add_piece",
        label: "Add Piece…",
        help: "Put a piece in a slot (at `index`, default the end) if pflow-rs accepts the resulting \
               plan for Connect-4; refused with its message otherwise. An absent list stands for its \
               default, which the drop adds to. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[
            arg("piece", Str, "a palette name"),
            arg("slot", Str, "drivers, order, filters, heuristics or memo"),
            opt("index", Int, "position in the slot's list (default: the end)"),
        ],
        enabled: need,
        run: add_piece,
        journal: true,
    },
    CommandSpec {
        id: "plan.remove_piece",
        label: "Remove Piece…",
        help: "Remove entry `index` of a slot (memo: the memo). An emptied list stays explicit (\"order\": \
               [] is the empty chain, \"drivers\": [] turns null_window off, \"filters\": [] runs none). \
               Applied whatever pflow-rs says; the result carries its verdict. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[
            arg("slot", Str, "drivers, order, filters, heuristics or memo"),
            opt("index", Int, "entry to remove (default 0)"),
        ],
        enabled: need,
        run: remove_piece,
        journal: true,
    },
    CommandSpec {
        id: "plan.move_piece",
        label: "Move Piece…",
        help: "Move entry `from` of a list slot to `to` (order changes nodes, never the value). \
               Applied whatever pflow-rs says; the result carries its verdict. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[
            arg("slot", Str, "drivers, order, filters or heuristics"),
            arg("from", Int, "index to move"),
            arg("to", Int, "index to move it to"),
        ],
        enabled: need,
        run: move_piece,
        journal: true,
    },
    CommandSpec {
        id: "plan.set_mode",
        label: "Set Plan Mode…",
        help: "Set the plan's mode: \"exact\" (only pieces that cannot change the value) or \"play\". \
               Applied whatever pflow-rs says; the result carries its verdict. One undo step.",
        menu: "Compose",
        shortcut: None,
        args: &[arg("mode", Str, "exact or play")],
        enabled: need,
        run: set_mode,
        journal: true,
    },
];

fn pieces(_: &mut Session, _: &Value) -> Result<Value> {
    Ok(json!({
        "pieces": sp::pieces(),
        "slots": sp::SLOTS,
        "plan_rs": sp::PLAN_RS_REV,
        "default_plan": sp::connect4_exact(),
    }))
}

fn plan_ref(sess: &Session) -> Result<&Value> {
    sess.search_plan
        .as_ref()
        .map(|p| &p.plan)
        .ok_or_else(|| Error::Invalid("no search plan is open (plan.open)".into()))
}

fn view(plan: &Value) -> Value {
    let c = sp::canonical(plan);
    json!({ "plan": c, "json": c.to_string(), "check": sp::check(&c) })
}

fn open(sess: &mut Session, a: &Value) -> Result<Value> {
    let plan = match a.get("plan") {
        None | Some(Value::Null) => sp::connect4_exact(),
        Some(v @ Value::Object(_)) => v.clone(),
        Some(other) => {
            return Err(Error::BadArgs(format!(
                "plan must be an object, got {other}"
            )))
        }
    };
    let plan = sp::canonical(&plan);
    sess.edit("open search plan", |s| {
        s.search_plan = Some(OpenPlan { plan: plan.clone() });
        Ok(view(&plan))
    })
}

fn close(sess: &mut Session, _: &Value) -> Result<Value> {
    sess.edit("close search plan", |s| {
        s.search_plan = None;
        Ok(json!({ "closed": true }))
    })
}

fn state(sess: &mut Session, _: &Value) -> Result<Value> {
    Ok(view(plan_ref(sess)?))
}

fn check(sess: &mut Session, a: &Value) -> Result<Value> {
    let (text, plan) = match a.get("plan") {
        None | Some(Value::Null) => {
            let p = sp::canonical(plan_ref(sess)?);
            (p.to_string(), p)
        }
        Some(Value::String(t)) => (t.clone(), Value::String(t.clone())),
        Some(v) => (v.to_string(), v.clone()),
    };
    let g = sp::connect4().map_err(Error::Invalid)?;
    let c = sp::check_text(&text, g);
    let mut out = json!(c);
    out["plan"] = plan;
    Ok(out)
}

fn slot_candidates(sess: &mut Session, a: &Value) -> Result<Value> {
    let name = req(a, "piece");
    let plan = plan_ref(sess)?.clone();
    Ok(json!({
        "piece": name,
        "info": sp::piece_info(name),
        "slots": sp::slot_candidates(&plan, name),
    }))
}

fn index(a: &Value, k: &str) -> Option<usize> {
    a.get(k)
        .and_then(Value::as_f64)
        .map(|x| x.max(0.0) as usize)
}

/// Replaces the open plan as one undoable edit and returns it with its
/// verdict.
fn set_plan(sess: &mut Session, label: &str, plan: Value) -> Result<Value> {
    let plan = sp::canonical(&plan);
    sess.edit(label, |s| {
        s.search_plan = Some(OpenPlan { plan: plan.clone() });
        Ok(view(&plan))
    })
}

fn add_piece(sess: &mut Session, a: &Value) -> Result<Value> {
    let (name, slot) = (req(a, "piece"), req(a, "slot"));
    let plan = plan_ref(sess)?.clone();
    let cand = sp::slot_candidates(&plan, name)
        .into_iter()
        .find(|c| c["slot"] == json!(slot))
        .ok_or_else(|| Error::BadArgs(format!("no slot {slot:?}")))?;
    if cand["ok"] != json!(true) {
        return Err(Error::Invalid(
            cand["why"].as_str().unwrap_or("refused").to_string(),
        ));
    }
    let next = match index(a, "index") {
        None => cand["plan"].clone(),
        Some(i) => {
            let p = sp::with_piece(&plan, name, slot, Some(i))
                .ok_or_else(|| Error::Invalid(format!("{name:?} cannot go in {slot:?}")))?;
            let c = sp::check(&sp::canonical(&p));
            if !c.ok {
                return Err(Error::Invalid(c.error.unwrap_or_default()));
            }
            p
        }
    };
    set_plan(sess, &format!("add {name} to {slot}"), next)
}

fn remove_piece(sess: &mut Session, a: &Value) -> Result<Value> {
    let slot = req(a, "slot");
    let plan = plan_ref(sess)?.clone();
    let next =
        sp::without_piece(&plan, slot, index(a, "index").unwrap_or(0)).map_err(Error::BadArgs)?;
    set_plan(sess, &format!("remove from {slot}"), next)
}

fn move_piece(sess: &mut Session, a: &Value) -> Result<Value> {
    let slot = req(a, "slot");
    let plan = plan_ref(sess)?.clone();
    let next = sp::move_piece(
        &plan,
        slot,
        index(a, "from").unwrap_or(0),
        index(a, "to").unwrap_or(0),
    )
    .map_err(Error::BadArgs)?;
    set_plan(sess, &format!("move in {slot}"), next)
}

fn set_mode(sess: &mut Session, a: &Value) -> Result<Value> {
    let mode = req(a, "mode");
    if mode != "exact" && mode != "play" {
        return Err(Error::BadArgs(format!(
            "mode is \"exact\" or \"play\", not {mode:?}"
        )));
    }
    let mut plan = plan_ref(sess)?.clone();
    plan["mode"] = json!(mode);
    set_plan(sess, &format!("plan mode {mode}"), plan)
}

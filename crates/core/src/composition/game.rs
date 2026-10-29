//! The token game on the flattened composition (`bundle.game`, `bundle.fire`,
//! `bundle.reset_marking`, `bundle.scenario`, `bundle.step`): the prototype's
//! flat view (`js/views/flat.js`), held in the session so an agent and a
//! window drive it identically.
//!
//! The marking, and whether a transition is enabled, are
//! `pflow_metamodel::Model::initial_marking` / `enabled_why_not` / `fire`
//! on the flat net ([`super::Flat`]); this module only remembers the marking,
//! the transitions fired since the last reset, and how far a scenario has
//! been stepped. The state lives in the [`super::Composition`] (so it is part
//! of undo) and belongs to one flat net: any edit that changes the net
//! (a link, an instance) makes [`FlatGame::of`] start afresh.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::content::DeclaredScenario;
use super::{flatten, Flat};
use pflow_compose::Bundle;
use pflow_metamodel::{Marking, Model};

/// How far a scenario has been stepped: its script's members fired in order
/// from the initial marking, stopping at the first one that is not enabled.
#[derive(Debug, Clone, PartialEq)]
pub struct Stepping {
    pub id: String,
    /// Steps taken: fired ones, plus the one that was refused (the last).
    pub shown: usize,
    /// Index of the step that could not fire.
    pub stopped_at: Option<usize>,
}

/// The state of the game on the open composition's flat net.
#[derive(Debug, Clone, Default)]
pub struct FlatGame {
    /// The flat net this belongs to ([`shape_key`]); empty before the first use.
    key: String,
    pub marking: BTreeMap<String, i64>,
    /// Flat transitions fired since the last reset, in order.
    pub fired: Vec<String>,
    pub scenario: Option<Stepping>,
}

/// What identifies a flat net for the game: its places (with their initial
/// tokens), transitions and arcs. A change to any of them starts a new game.
pub fn shape_key(m: &Model) -> String {
    json!([
        m.places
            .iter()
            .map(|p| json!([p.id, p.initial, p.capacity, p.is_token()]))
            .collect::<Vec<_>>(),
        m.transitions.iter().map(|t| &t.id).collect::<Vec<_>>(),
        m.arcs
            .iter()
            .map(|a| json!([a.from, a.to, a.weight, super::arc_type(a.typ)]))
            .collect::<Vec<_>>(),
    ])
    .to_string()
}

fn to_marking(m: &BTreeMap<String, i64>) -> Marking {
    m.iter().map(|(k, v)| (k.clone(), *v)).collect()
}

impl FlatGame {
    /// The game for `flat`: `self` when it belongs to this net, else a fresh
    /// one at the initial marking.
    pub fn of(&self, flat: &Flat) -> FlatGame {
        let key = shape_key(&flat.model);
        if self.key == key {
            return self.clone();
        }
        FlatGame {
            key,
            marking: flat.model.initial_marking().into_iter().collect(),
            fired: vec![],
            scenario: None,
        }
    }

    /// Back to the initial marking, no scenario.
    pub fn reset(&self, flat: &Flat) -> FlatGame {
        FlatGame::default().of(flat)
    }

    /// Fires flat transition `t` through `Model::enabled_why_not` / `fire`;
    /// the reason when it is not enabled. A manual firing leaves any
    /// scenario (the marking is no longer the scenario's).
    pub fn fire(&mut self, flat: &Flat, t: &str) -> Result<(), String> {
        if !flat.model.transitions.iter().any(|x| x.id == t) {
            return Err(format!("no flat transition {t:?}"));
        }
        let mk = to_marking(&self.marking);
        flat.model
            .enabled_why_not(t, &mk)
            .map_err(|e| e.to_string())?;
        self.marking = flat.model.fire(t, &mk).into_iter().collect();
        self.fired.push(t.to_string());
        self.scenario = None;
        Ok(())
    }

    /// Selects scenario `id` of `scenarios` (the ones the open composition's
    /// content declares): back to the initial marking with nothing of its
    /// script fired.
    pub fn select(
        &self,
        flat: &Flat,
        scenarios: &[DeclaredScenario],
        id: &str,
    ) -> Result<FlatGame, String> {
        if !scenarios.iter().any(|s| s.id == id) {
            return Err(no_scenario(scenarios, id));
        }
        let mut g = self.reset(flat);
        g.scenario = Some(Stepping {
            id: id.into(),
            shown: 0,
            stopped_at: None,
        });
        Ok(g)
    }

    /// Takes the next step of the selected scenario: resolves its member to a
    /// flat transition, fires it when enabled, else records where the script
    /// stopped. `Ok(true)` while there is more to do.
    pub fn step(&mut self, flat: &Flat, scenarios: &[DeclaredScenario]) -> Result<bool, String> {
        let st = self
            .scenario
            .clone()
            .ok_or("no scenario is selected (bundle.scenario)")?;
        let sc = scenarios
            .iter()
            .find(|s| s.id == st.id)
            .ok_or_else(|| no_scenario(scenarios, &st.id))?;
        if st.stopped_at.is_some() || st.shown >= sc.script.len() {
            return Ok(false);
        }
        let member = sc.script[st.shown].as_str();
        let mk = to_marking(&self.marking);
        let t = flat.transition_of(member).map(str::to_string);
        let ok = match &t {
            None => false,
            Some(t) => flat.model.enabled_why_not(t, &mk).is_ok(),
        };
        let next = Stepping {
            id: st.id.clone(),
            shown: st.shown + 1,
            stopped_at: (!ok).then_some(st.shown),
        };
        if ok {
            let t = t.expect("resolved");
            self.marking = flat.model.fire(&t, &mk).into_iter().collect();
            self.fired.push(t);
        }
        let more = ok && next.shown < sc.script.len();
        self.scenario = Some(next);
        Ok(more)
    }
}

// ------------------------------------------------------------ constraints at a marking

/// The refusal for a scenario the composition does not declare: its content's list says
/// what there is.
fn no_scenario(scenarios: &[DeclaredScenario], id: &str) -> String {
    if scenarios.is_empty() {
        format!("no scenario {id:?}: this composition declares none (content.load adds them)")
    } else {
        let ids: Vec<&str> = scenarios.iter().map(|s| s.id.as_str()).collect();
        format!("no scenario {id:?}; this composition declares {ids:?}")
    }
}

/// A declared constraint read at a marking.
#[derive(Debug, Clone, PartialEq)]
pub struct Evaluated {
    pub holds: bool,
    /// The left side's terms as numbers: `"1 + 0 + 2"`.
    pub lhs: String,
    pub op: String,
    pub rhs: i64,
}

fn side(s: &str, mk: &BTreeMap<String, i64>) -> Option<(i64, Vec<i64>)> {
    let mut total = 0;
    let mut parts = vec![];
    for term in s.split('+').map(str::trim) {
        let v = if let Some(p) = term
            .strip_prefix("tokens(\"")
            .and_then(|t| t.strip_suffix("\")"))
        {
            *mk.get(p).unwrap_or(&0)
        } else {
            term.parse::<i64>().ok()?
        };
        total += v;
        parts.push(v);
    }
    Some((total, parts))
}

/// A constraint of the form `tokens("a") + tokens("b") + 3 == N` (the form
/// the constructors emit; any of `== != >= <= > <`) evaluated at `mk`; `None`
/// for any other expression.
pub fn evaluate(expr: &str, mk: &BTreeMap<String, i64>) -> Option<Evaluated> {
    let ops = ["==", "!=", ">=", "<=", ">", "<"];
    let (at, op) = ops
        .iter()
        .filter_map(|o| expr.find(o).map(|i| (i, *o)))
        .min_by_key(|(i, _)| *i)?;
    let (l, r) = (&expr[..at], &expr[at + op.len()..]);
    let (lt, parts) = side(l, mk)?;
    let (rt, _) = side(r, mk)?;
    let holds = match op {
        "==" => lt == rt,
        "!=" => lt != rt,
        ">=" => lt >= rt,
        "<=" => lt <= rt,
        ">" => lt > rt,
        _ => lt < rt,
    };
    Some(Evaluated {
        holds,
        lhs: parts
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(" + "),
        op: op.into(),
        rhs: rt,
    })
}

// ------------------------------------------------------------ the query

/// `bundle.game`'s result for the composition's bundle and its stored game;
/// `scenarios` are the ones its content declares.
pub fn game_json(b: &Bundle, stored: &FlatGame, scenarios: &[DeclaredScenario]) -> Value {
    let flat = match flatten(b) {
        Ok(f) => f,
        Err(e) => return json!({"ok": false, "error": e}),
    };
    let g = stored.of(&flat);
    let mk = to_marking(&g.marking);
    let transitions: Vec<Value> = flat
        .model
        .transitions
        .iter()
        .map(|t| match flat.model.enabled_why_not(&t.id, &mk) {
            Ok(()) => json!({"id": t.id, "enabled": true, "why": ""}),
            Err(e) => json!({"id": t.id, "enabled": false, "why": e.to_string()}),
        })
        .collect();
    let enabled: Vec<&str> = transitions
        .iter()
        .filter(|t| t["enabled"] == true)
        .filter_map(|t| t["id"].as_str())
        .collect();
    let constraints: Vec<Value> = flat
        .model
        .constraints
        .iter()
        .map(|k| match evaluate(&k.expr, &g.marking) {
            Some(e) => json!({"id": k.id, "expr": k.expr, "evaluated": true, "holds": e.holds,
                              "lhs": e.lhs, "op": e.op, "rhs": e.rhs}),
            None => json!({"id": k.id, "expr": k.expr, "evaluated": false}),
        })
        .collect();
    json!({
        "ok": true,
        "marking": g.marking,
        "fired": g.fired,
        "enabled": enabled,
        "transitions": transitions,
        "scenario": g
            .scenario
            .as_ref()
            .and_then(|s| scenarios.iter().find(|x| x.id == s.id).map(|sc| scenario_json(&flat, &g, s, sc))),
        "constraints": constraints,
    })
}

/// The scenario's progress: the steps taken, and when it is done the
/// predicates at the marking reached.
fn scenario_json(flat: &Flat, g: &FlatGame, st: &Stepping, sc: &DeclaredScenario) -> Value {
    // The steps are replayed from the initial marking, as `run` does; the
    // game's own marking is what they led to.
    let mut mk = flat.model.initial_marking();
    let mut steps = vec![];
    for member in sc.script.iter().take(st.shown) {
        let t = flat.transition_of(member).map(str::to_string);
        let why = match &t {
            None => Some(format!("no member {member:?} in the composition")),
            Some(t) => flat
                .model
                .enabled_why_not(t, &mk)
                .err()
                .map(|e| e.to_string()),
        };
        steps.push(
            json!({"member": member, "flat": t, "enabled": why.is_none(),
                          "why": why.clone().unwrap_or_default()}),
        );
        if why.is_some() {
            break;
        }
        mk = flat.model.fire(t.as_deref().expect("resolved"), &mk);
    }
    let done = st.stopped_at.is_some() || st.shown >= sc.script.len();
    let expect: Vec<Value> = if done {
        sc.expect
            .iter()
            .map(|e| {
                let f = flat.place_of(&e.place).map(str::to_string);
                let actual = f
                    .as_ref()
                    .and_then(|p| g.marking.get(p))
                    .copied()
                    .unwrap_or(0);
                json!({"place": e.place, "flat": f, "op": e.op, "n": e.n, "says": e.says,
                       "actual": actual, "pass": super::content::compare(&e.op, actual, e.n)})
            })
            .collect()
    } else {
        vec![]
    };
    json!({
        "id": sc.id,
        "title": sc.title,
        "note": sc.note,
        "script": sc.script,
        "steps": steps,
        "shown": st.shown,
        "done": done,
        "completed": done && st.stopped_at.is_none(),
        "stopped_at": st.stopped_at.map_or(-1, |i| i as i64),
        "stopped_step": st.stopped_at.map(|i| sc.script[i].as_str()),
        "expect": expect,
    })
}

//! Playing the Connect-4 net the open plan searches (`plan.game`,
//! `plan.play`, `plan.new_game`): the flat view's connect4 example
//! (the prototype's `renderConnect4`), held in the session so an agent and
//! a window drive it identically.
//!
//! The net is pflow-rs's fixture read as a game exactly as its search tests
//! declare it ([`crate::search_plan::connect4`]); a move is fired by
//! `pflow_metamodel::game::Game::play` (the move through the firing rule,
//! then the automatic `*win*` transitions settled, lowest index first), and a
//! refusal is `Model::enabled_why_not`'s.

use std::collections::BTreeMap;

use serde_json::{json, Value};

use crate::search_plan::{connect4, connect4_model};

/// The position played so far.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayState {
    /// The settled marking (nonzero places); `None` is the settled initial one.
    marking: Option<BTreeMap<String, i64>>,
    /// The moves played, in order.
    pub log: Vec<String>,
    /// The automatic transitions the last move settled.
    pub last: Vec<String>,
}

fn dense(st: &PlayState) -> Result<Vec<i64>, String> {
    let g = connect4()?;
    Ok(match &st.marking {
        Some(m) => {
            g.ix.dense(&m.iter().map(|(k, v)| (k.clone(), *v)).collect())
        }
        None => {
            let mut mk = g.ix.dense(&connect4_model()?.initial_marking());
            g.settle(&mut mk).map_err(|e| e.to_string())?;
            mk
        }
    })
}

fn sparse(mk: &[i64]) -> Result<BTreeMap<String, i64>, String> {
    let g = connect4()?;
    Ok(g.ix
        .places
        .iter()
        .zip(mk)
        .filter(|(_, v)| **v != 0)
        .map(|(p, v)| (p.clone(), *v))
        .collect())
}

/// Plays move `t`: through the firing rule, then the automatic transitions
/// settle. The reason when `t` is not an enabled move.
pub fn play(st: &PlayState, t: &str) -> Result<PlayState, String> {
    let (g, m) = (connect4()?, connect4_model()?);
    let ti =
        g.ix.transitions
            .iter()
            .position(|x| x == t)
            .ok_or_else(|| format!("no transition {t:?} in the Connect-4 net"))?;
    if g.auto[ti] {
        return Err(format!(
            "{t} is automatic (its name contains \"win\"): it fires by itself when enabled (Game::settle)"
        ));
    }
    let mk = dense(st)?;
    let marking = g.ix.sparse(&mk);
    m.enabled_why_not(t, &marking).map_err(|e| e.to_string())?;
    let next = g.play(&mk, ti).map_err(|e| e.to_string())?;
    // which automatic transitions settled: the same walk Game::settle makes,
    // for the report only (the marking is Game::play's)
    let mut walk = g.ix.fire(ti, &mk);
    let mut last = vec![];
    while let Some(a) = (0..g.auto.len()).find(|&a| g.auto[a] && g.ix.enabled(a, &walk)) {
        walk = g.ix.fire(a, &walk);
        last.push(g.ix.transitions[a].clone());
        if last.len() > 1000 {
            break;
        }
    }
    debug_assert_eq!(walk, next);
    let mut log = st.log.clone();
    log.push(t.to_string());
    Ok(PlayState {
        marking: Some(sparse(&next)?),
        log,
        last,
    })
}

/// `plan.game`'s result.
pub fn game_json(st: &PlayState) -> Value {
    let run = || -> Result<Value, String> {
        let g = connect4()?;
        let mk = dense(st)?;
        let moves: Vec<&str> = g
            .moves(&mk)
            .into_iter()
            .map(|t| g.ix.transitions[t].as_str())
            .collect();
        let sides: std::collections::BTreeSet<i64> =
            g.moves(&mk).into_iter().map(|t| g.sign[t] as i64).collect();
        let over = moves.is_empty();
        Ok(json!({
            "ok": true,
            "marking": sparse(&mk)?,
            "moves": moves,
            "side": match (sides.len(), sides.iter().next()) {
                (1, Some(1)) => "x_ (sign +1)",
                (1, _) => "o_ (sign -1)",
                (0, _) => "",
                _ => "mixed",
            },
            "final": over,
            "score": if over { g.score(&mk).ok() } else { None },
            "ply": st.log.len(),
            "log": st.log,
            "last": st.last,
        }))
    };
    run().unwrap_or_else(|e| json!({"ok": false, "error": e}))
}

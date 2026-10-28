//! A net read as a two-sided game: which side each transition belongs to
//! (`sides`), the reward read off a final marking (`reward`) and the
//! transitions nobody chooses (`auto`), all given as command arguments, and
//! the dense, index-resolved form [`pflow_metamodel::game`] searches. Shared
//! by `game.*` and the experimental `rnn.*`.

use pflow_metamodel::game::Game;
use serde_json::{json, Value};

use crate::analysis::Structure;
use crate::registry::strs;
use crate::sim::Marking;
use crate::{Error, Result};

/// `auto` resolved to a flag per transition (prefixes of transition ids).
pub fn auto(st: &Structure, a: &Value) -> Result<Vec<bool>> {
    let prefixes = strs(a, "auto").unwrap_or_default();
    for p in &prefixes {
        if !st.transitions.iter().any(|t| t.starts_with(p.as_str())) {
            return Err(Error::BadArgs(format!(
                "auto names {p}, which no transition id starts with"
            )));
        }
    }
    Ok(st
        .transitions
        .iter()
        .map(|t| prefixes.iter().any(|p| t.starts_with(p.as_str())))
        .collect())
}

/// `reward` resolved to (index into `st.places`, weight).
pub fn reward(st: &Structure, a: &Value) -> Result<Vec<(usize, f64)>> {
    let obj = match a.get("reward").filter(|x| !x.is_null()) {
        Some(Value::String(s)) => {
            serde_json::from_str::<Value>(s).map_err(|e| Error::BadArgs(format!("reward: {e}")))?
        }
        Some(x) => x.clone(),
        None => {
            return Err(Error::BadArgs(
                "reward is required: {place: weight}, e.g. {\"win_x\": 1, \"win_o\": -1}".into(),
            ))
        }
    };
    let obj = obj
        .as_object()
        .ok_or_else(|| Error::BadArgs("reward must be an object {place: weight}".into()))?;
    let mut out = vec![];
    for (k, w) in obj {
        let w = w
            .as_f64()
            .ok_or_else(|| Error::BadArgs(format!("reward weight for {k} must be a number")))?;
        let idx: Vec<usize> = (0..st.places.len())
            .filter(|&i| st.display[i] == *k || st.base[i] == *k)
            .collect();
        if idx.is_empty() {
            return Err(Error::BadArgs(format!(
                "reward names {k}, which is not a place of this net"
            )));
        }
        out.extend(idx.into_iter().map(|i| (i, w)));
    }
    Ok(out)
}

/// `sides` resolved to (transition index → sign); None without `sides`.
pub fn sides(st: &Structure, a: &Value) -> Result<Option<Vec<f64>>> {
    let obj = match a.get("sides").filter(|x| !x.is_null()) {
        None => return Ok(None),
        Some(Value::String(s)) => {
            serde_json::from_str::<Value>(s).map_err(|e| Error::BadArgs(format!("sides: {e}")))?
        }
        Some(x) => x.clone(),
    };
    let obj = obj.as_object().ok_or_else(|| {
        Error::BadArgs("sides must be an object {transition-id prefix: sign}".into())
    })?;
    let mut prefixes = vec![];
    for (k, w) in obj {
        let w = w
            .as_f64()
            .ok_or_else(|| Error::BadArgs(format!("sides sign for {k} must be a number")))?;
        if !st.transitions.iter().any(|t| t.starts_with(k.as_str())) {
            return Err(Error::BadArgs(format!(
                "sides names {k}, which no transition id starts with"
            )));
        }
        prefixes.push((k.as_str(), w));
    }
    Ok(Some(
        st.transitions
            .iter()
            .map(|t| {
                prefixes
                    .iter()
                    .filter(|(p, _)| t.starts_with(p))
                    .max_by_key(|(p, _)| p.len())
                    .map_or(0.0, |&(_, w)| w)
            })
            .collect(),
    ))
}

/// The rules of a two-sided game: each transition's side, the reward
/// read from the final marking, and the automatic transitions.
pub struct Rules {
    pub sign: Vec<f64>,
    pub reward: Vec<(usize, f64)>,
    pub auto: Vec<bool>,
}

/// The rules `a` gives (`sides` and `reward` required, `auto` optional).
pub fn game_rules(st: &Structure, a: &Value) -> Result<Rules> {
    let sign = sides(st, a)?.ok_or_else(|| {
        Error::BadArgs(
            "sides is required: {transition-id prefix: sign}, e.g. {\"x_\": 1, \"o_\": -1}".into(),
        )
    })?;
    Ok(Rules {
        sign,
        reward: reward(st, a)?,
        auto: auto(st, a)?,
    })
}

/// The session's view of a net and the dense, index-resolved form
/// [`pflow_metamodel::game`] searches, side by side.
pub struct Dense {
    pub rules: Game,
    /// Per [`Structure`] place: its index in a dense marking.
    pub place: Vec<usize>,
    /// Per [`Structure`] transition: its index in `rules.ix`; and back.
    pub to_ix: Vec<usize>,
    pub to_view: Vec<usize>,
}

impl Dense {
    pub fn new(st: &Structure, r: &Rules) -> Result<Self> {
        let ix = st.net.model.indexed();
        let missing = || {
            Error::Invalid(
                "the exact search needs every token place and transition of the net (a coloured net with unused \
                 colour copies is not supported)"
                    .into(),
            )
        };
        if ix.places.len() != st.places.len() {
            return Err(missing());
        }
        let place = st
            .places
            .iter()
            .map(|p| ix.places.iter().position(|q| q == p).ok_or_else(missing))
            .collect::<Result<Vec<_>>>()?;
        let to_ix = st
            .transitions
            .iter()
            .map(|t| {
                ix.transitions
                    .iter()
                    .position(|u| u == t)
                    .ok_or_else(missing)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut to_view = vec![usize::MAX; ix.transitions.len()];
        for (i, &j) in to_ix.iter().enumerate() {
            to_view[j] = i;
        }
        if to_view.contains(&usize::MAX) {
            return Err(missing());
        }
        let (mut auto, mut sign) = (
            vec![false; ix.transitions.len()],
            vec![0.0; ix.transitions.len()],
        );
        for (i, &j) in to_ix.iter().enumerate() {
            auto[j] = r.auto[i];
            sign[j] = r.sign[i];
        }
        let reward: Vec<(usize, f64)> = r.reward.iter().map(|&(p, w)| (place[p], w)).collect();
        Ok(Self {
            rules: Game::new(ix, auto, sign, reward),
            place,
            to_ix,
            to_view,
        })
    }

    /// A session marking as a dense one.
    pub fn dense(&self, st: &Structure, mk: &Marking) -> Vec<i64> {
        let x = st.marking(mk);
        let mut d = vec![0; self.rules.ix.places.len()];
        for (i, &p) in self.place.iter().enumerate() {
            d[p] = x[i];
        }
        d
    }

    /// A session marking as a dense one, with the automatic transitions
    /// fired until none is enabled: the position a search starts from.
    pub fn settled(&self, st: &Structure, mk: &Marking) -> Result<Vec<i64>> {
        let mut d = self.dense(st, mk);
        self.rules.settle(&mut d)?;
        Ok(d)
    }

    /// A dense marking as the policy's input (the [`Structure`]'s place
    /// order).
    pub fn input(&self, d: &[i64]) -> Vec<f64> {
        self.place.iter().map(|&p| d[p] as f64).collect()
    }

    /// A dense marking as a session marking, shaped like `template`.
    pub fn marking(&self, st: &Structure, template: &Marking, d: &[i64]) -> Marking {
        let mut mk = template.clone();
        for (i, &p) in self.place.iter().enumerate() {
            let (base, c) = st.net.base(&st.places[i]);
            if let Some(slot) = mk.get_mut(&base).and_then(|x| x.get_mut(c)) {
                *slot = d[p];
            }
        }
        mk
    }

    /// The side to move at a settled position with moves `moves`: the sign
    /// every one shares.
    pub fn mover(&self, moves: &[usize]) -> Result<f64> {
        self.rules.mover(moves).map_err(|e| {
            Error::Invalid(format!(
                "{e}; give every transition a side in `sides`, and mark transitions nobody chooses \
                 (e.g. win claims) `auto`"
            ))
        })
    }

    /// The first line of a label cache (`rnn.imitate` and `rnn.verify`'s
    /// `labels`, `game.*`'s `book`): what the labels depend on, the net, the
    /// rules and the search budget.
    pub fn label_header(&self, budget: usize) -> Value {
        let r = &self.rules;
        json!({
            "labels": 1,
            "places": r.ix.places,
            "transitions": r.ix.transitions,
            "auto": r.auto,
            "sign": r.sign,
            "reward": r.reward,
            "budget": budget,
        })
    }
}

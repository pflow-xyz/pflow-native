//! Exact game values for nets too big for the `rnn.*` teacher's full
//! solve: alpha-beta over the firing rule, from one position at a time,
//! with a node budget.
//!
//! The rules are the teacher's (`commands/rnn.rs`): `auto` transitions
//! fire as soon as they are enabled, the side to move is the sign every
//! enabled move shares, and a position with no move is worth the reward
//! read from its marking. Firing goes through pflow-metamodel's
//! [`IndexedModel`], the reference rule resolved to indices once, so a
//! node costs a scan of the transitions rather than of every arc.
//!
//! A value [`Solver::value`] returns is exact. When a search runs past its
//! budget it returns `None`, never a guess: a caller sampling positions
//! skips and counts those (as the sim.pflow.xyz ODE Connect-4 experiment's oracle does).

use std::collections::HashMap;

use pflow_metamodel::IndexedModel;

use crate::{Error, Result};

/// A two-sided game on a net, over dense markings in `ix.places` order.
pub(crate) struct Rules {
    pub ix: IndexedModel,
    /// Per transition (in `ix` order): fires as soon as it is enabled.
    pub auto: Vec<bool>,
    /// Per transition: the side that chooses it (+1 maximizes the reward).
    pub sign: Vec<f64>,
    /// (dense place index, weight): the reward, read from a final marking.
    pub reward: Vec<(usize, f64)>,
    /// The least and greatest reward a final marking can have. Searching
    /// inside them lets a win end a search early.
    pub bounds: (f64, f64),
}

impl Rules {
    /// Fires automatic transitions until none is enabled.
    pub fn settle(&self, mk: &mut [i64]) -> Result<()> {
        for _ in 0..10_000 {
            let Some(t) = (0..self.auto.len()).find(|&t| self.auto[t] && self.ix.enabled(t, mk))
            else {
                return Ok(());
            };
            self.ix.fire_in_place(t, mk);
        }
        Err(Error::Invalid(
            "automatic transitions never stop firing".into(),
        ))
    }

    /// The moves at a settled marking.
    pub fn moves(&self, mk: &[i64]) -> Vec<usize> {
        self.ix.enabled_transitions(mk)
    }

    pub fn score(&self, mk: &[i64]) -> Result<f64> {
        let x: f64 = self.reward.iter().map(|&(p, w)| w * mk[p] as f64).sum();
        if x < self.bounds.0 || x > self.bounds.1 {
            return Err(Error::Invalid(format!(
                "a final marking scores {x}, outside the reward's assumed range {:?} (each reward place ending \
                 with at most one token)",
                self.bounds
            )));
        }
        Ok(x)
    }

    /// The side to move at a settled marking with moves `en`.
    pub fn mover(&self, en: &[usize]) -> Result<f64> {
        let s = self.sign[en[0]];
        if s == 0.0 || en.iter().any(|&t| self.sign[t] != s) {
            let names: Vec<&str> = en
                .iter()
                .map(|&t| self.ix.transitions[t].as_str())
                .collect();
            return Err(Error::Invalid(format!(
                "the enabled moves {names:?} do not all belong to one side of `sides`"
            )));
        }
        Ok(s)
    }

    /// The settled marking after move `t`.
    pub fn play(&self, mk: &[i64], t: usize) -> Result<Vec<i64>> {
        let mut next = self.ix.fire(t, mk);
        self.settle(&mut next)?;
        Ok(next)
    }
}

/// A 128-bit hash of a marking: the transposition table's key.
fn key(mk: &[i64]) -> u128 {
    let (mut a, mut b) = (0x9E37_79B9_7F4A_7C15u64, 0xC2B2_AE3D_27D4_EB4Fu64);
    for (i, &v) in mk.iter().enumerate() {
        let x = (v as u64).wrapping_add((i as u64) << 40);
        a = (a ^ x).wrapping_mul(0xFF51_AFD7_ED55_8CCD).rotate_left(29);
        b = (b ^ x.rotate_left(17))
            .wrapping_mul(0xC4CE_B9FE_1A85_EC53)
            .rotate_left(31);
    }
    (a as u128) << 64 | b as u128
}

/// What a finished search knows about a position: its value lies in
/// `[lo, hi]` (equal when exact), and the move that did best.
#[derive(Clone, Copy)]
struct Entry {
    lo: f64,
    hi: f64,
    best: u32,
}

const NO_MOVE: u32 = u32::MAX;

/// Alpha-beta with a transposition table that persists across calls (so
/// solving the positions of one game reuses work) and a history heuristic
/// for move order. One per thread.
pub(crate) struct Solver<'a> {
    r: &'a Rules,
    tt: HashMap<u128, Entry>,
    history: Vec<u32>,
    nodes: usize,
    budget: usize,
}

impl<'a> Solver<'a> {
    /// Entries kept before the table is cleared.
    const TABLE: usize = 2_000_000;

    pub fn new(r: &'a Rules, budget: usize) -> Self {
        Self {
            r,
            tt: HashMap::new(),
            history: vec![0; r.ix.transitions.len()],
            nodes: 0,
            budget,
        }
    }

    /// The exact value (+1 side's view) of a settled position with both
    /// sides playing perfectly, or `None` past `budget` nodes.
    pub fn value(&mut self, mk: &[i64]) -> Result<Option<f64>> {
        if self.tt.len() > Self::TABLE {
            self.tt.clear();
        }
        self.nodes = 0;
        let (lo, hi) = self.r.bounds;
        self.search(mk, lo, hi)
    }

    /// Fail-soft alpha-beta in the +1 side's view. Inside the bounds every
    /// result is a bound on the true value; at the root, whose window is
    /// the whole range, it is the value.
    fn search(&mut self, mk: &[i64], mut a: f64, mut b: f64) -> Result<Option<f64>> {
        self.nodes += 1;
        if self.nodes > self.budget {
            return Ok(None);
        }
        let k = key(mk);
        let mut first = NO_MOVE;
        if let Some(e) = self.tt.get(&k) {
            if e.lo == e.hi || e.lo >= b {
                return Ok(Some(e.lo));
            }
            if e.hi <= a {
                return Ok(Some(e.hi));
            }
            a = a.max(e.lo);
            b = b.min(e.hi);
            first = e.best;
        }
        let en = self.r.moves(mk);
        if en.is_empty() {
            let x = self.r.score(mk)?;
            self.tt.insert(
                k,
                Entry {
                    lo: x,
                    hi: x,
                    best: NO_MOVE,
                },
            );
            return Ok(Some(x));
        }
        let s = self.r.mover(&en)?;
        // Children, best first: the remembered best move, then moves that
        // end the game in the mover's favour, then by history.
        let mut kids = Vec::with_capacity(en.len());
        for &t in &en {
            let next = self.r.play(mk, t)?;
            let over = self.r.moves(&next).is_empty();
            let rank = if t as u32 == first {
                u64::MAX
            } else if over && s * self.r.score(&next)? > 0.0 {
                u64::MAX - 1
            } else {
                self.history[t] as u64
            };
            kids.push((rank, t, next));
        }
        kids.sort_by_key(|x| std::cmp::Reverse(x.0));
        let (a0, b0) = (a, b);
        let mut best = (
            if s > 0.0 {
                f64::NEG_INFINITY
            } else {
                f64::INFINITY
            },
            NO_MOVE,
        );
        for (_, t, next) in &kids {
            let Some(v) = self.search(next, a, b)? else {
                return Ok(None);
            };
            if (s > 0.0 && v > best.0) || (s < 0.0 && v < best.0) {
                best = (v, *t as u32);
            }
            if s > 0.0 {
                a = a.max(v);
            } else {
                b = b.min(v);
            }
            if a >= b {
                self.history[*t] += 1;
                break;
            }
        }
        let v = best.0;
        let (lo, hi) = if v <= a0 {
            (self.r.bounds.0, v)
        } else if v >= b0 {
            (v, self.r.bounds.1)
        } else {
            (v, v)
        };
        // Merge with what was known before this search narrowed the window.
        let e = match self.tt.get(&k) {
            Some(old) => Entry {
                lo: lo.max(old.lo),
                hi: hi.min(old.hi),
                best: best.1,
            },
            None => Entry {
                lo,
                hi,
                best: best.1,
            },
        };
        self.tt.insert(k, e);
        Ok(Some(v))
    }
}

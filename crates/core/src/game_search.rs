//! Exact game values for nets too big for the `rnn.*` teacher's full
//! solve: alpha-beta over the firing rule, from one position at a time,
//! with a node budget.
//!
//! The rules are the teacher's (`commands/rnn.rs`): `auto` transitions
//! fire as soon as they are enabled, the side to move is the sign every
//! enabled move shares, and a position with no move is worth the reward
//! read from its marking. Firing goes through pflow-metamodel. The search
//! runs over one of two engines:
//!
//! - [`BitModel`], when every count is 0 or 1 (a board's cells, a turn
//!   token): a marking is a few words, enabled is a mask test and firing
//!   an XOR. A firing that would put a second token on a place is refused,
//!   and the position is searched again by counts.
//! - [`IndexedModel`] with its [`Enabled`] sets otherwise, rechecking after
//!   a firing only the conditions on the places it changed
//!   ([`IndexedModel::fire_tracked`]).
//!
//! A value [`Solver::value`] returns is exact. When a search runs past its
//! budget it returns `None`, never a guess: a caller sampling positions
//! skips and counts those (as the sim.pflow.xyz ODE Connect-4 experiment's oracle does).

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use pflow_metamodel::{BitModel, Enabled, IndexedModel, TransitionSet};

use crate::{Error, Result};

/// A two-sided game on a net, over dense markings in `ix.places` order.
pub(crate) struct Rules {
    pub ix: IndexedModel,
    /// `ix` over 0/1 markings.
    pub bits: BitModel,
    /// Per transition (in `ix` order): fires as soon as it is enabled.
    pub auto: Vec<bool>,
    /// Per transition: the side that chooses it (+1 maximizes the reward).
    pub sign: Vec<f64>,
    /// (dense place index, weight): the reward, read from a final marking.
    pub reward: Vec<(usize, f64)>,
    /// The least and greatest reward a final marking can have. Searching
    /// inside them lets a win end a search early.
    pub bounds: (f64, f64),
    /// `auto` as a set, and its complement (the moves).
    pub auto_set: TransitionSet,
    pub move_set: TransitionSet,
    /// Per transition: the automatic ones it can enable
    /// ([`BitModel::wakes`]).
    auto_wakes: Vec<Vec<usize>>,
}

/// How long settling may run before the automatic transitions count as
/// never stopping.
const SETTLE: usize = 10_000;

fn endless() -> Error {
    Error::Invalid("automatic transitions never stop firing".into())
}

impl Rules {
    /// Rules over `ix`; `auto` and the rest as the fields.
    pub fn new(
        ix: IndexedModel,
        auto: Vec<bool>,
        sign: Vec<f64>,
        reward: Vec<(usize, f64)>,
        bounds: (f64, f64),
    ) -> Self {
        let n = auto.len();
        let bits = ix.bits();
        let auto_wakes = (0..n)
            .map(|t| bits.wakes(t).iter().copied().filter(|&u| auto[u]).collect())
            .collect();
        Self {
            bits,
            auto_wakes,
            auto_set: ix.set_of((0..n).filter(|&t| auto[t])),
            move_set: ix.set_of((0..n).filter(|&t| !auto[t])),
            ix,
            auto,
            sign,
            reward,
            bounds,
        }
    }

    /// Fires automatic transitions until none is enabled.
    pub fn settle(&self, mk: &mut [i64]) -> Result<()> {
        for _ in 0..SETTLE {
            let Some(t) = (0..self.auto.len()).find(|&t| self.auto[t] && self.ix.enabled(t, mk))
            else {
                return Ok(());
            };
            self.ix.fire_in_place(t, mk);
        }
        Err(endless())
    }

    /// The moves at a settled marking.
    pub fn moves(&self, mk: &[i64]) -> Vec<usize> {
        self.ix.enabled_transitions(mk)
    }

    pub fn score(&self, mk: &[i64]) -> Result<f64> {
        self.check(self.reward.iter().map(|&(p, w)| w * mk[p] as f64).sum())
    }

    /// A reward, refused outside the bounds.
    fn check(&self, x: f64) -> Result<f64> {
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

/// A settled marking the search plays moves from.
trait Walk: Clone {
    /// The moves here, into `out`.
    fn moves(&self, r: &Rules, out: &mut Vec<usize>);
    /// Whether no move is enabled.
    fn over(&self, r: &Rules) -> bool;
    /// Plays move `t` and settles. False when this engine cannot represent
    /// the result, which leaves `self` meaning nothing.
    fn play(&mut self, r: &Rules, t: usize) -> Result<bool>;
    fn score(&self, r: &Rules) -> Result<f64>;
    /// A 128-bit hash: the transposition table's key.
    fn key(&self) -> u128;
}

/// Mixes words into a 128-bit hash.
fn key_of(xs: impl Iterator<Item = u64>) -> u128 {
    let (mut a, mut b) = (0x9E37_79B9_7F4A_7C15u64, 0xC2B2_AE3D_27D4_EB4Fu64);
    for (i, v) in xs.enumerate() {
        let x = v.wrapping_add((i as u64) << 40);
        a = (a ^ x).wrapping_mul(0xFF51_AFD7_ED55_8CCD).rotate_left(29);
        b = (b ^ x.rotate_left(17))
            .wrapping_mul(0xC4CE_B9FE_1A85_EC53)
            .rotate_left(31);
    }
    (a as u128) << 64 | b as u128
}

/// Counts, with the enabled set kept current.
#[derive(Clone)]
struct Counts {
    mk: Vec<i64>,
    en: Enabled,
}

impl Walk for Counts {
    fn moves(&self, _: &Rules, out: &mut Vec<usize>) {
        out.extend(self.en.iter());
    }

    fn over(&self, _: &Rules) -> bool {
        self.en.is_empty()
    }

    fn play(&mut self, r: &Rules, t: usize) -> Result<bool> {
        r.ix.fire_tracked(t, &mut self.mk, &mut self.en);
        for _ in 0..SETTLE {
            let Some(u) = self.en.first_in(&r.auto_set) else {
                return Ok(true);
            };
            r.ix.fire_tracked(u, &mut self.mk, &mut self.en);
        }
        Err(endless())
    }

    fn score(&self, r: &Rules) -> Result<f64> {
        r.score(&self.mk)
    }

    fn key(&self) -> u128 {
        key_of(self.mk.iter().map(|&v| v as u64))
    }
}

/// The most words a bit marking has here (256 places), so a child is a
/// copy of a few words rather than an allocation.
const BIT_WORDS: usize = 4;

/// A 0/1 marking as bits.
#[derive(Clone, Copy)]
struct Bits([u64; BIT_WORDS]);

impl Walk for Bits {
    fn moves(&self, r: &Rules, out: &mut Vec<usize>) {
        out.extend(r.bits.enabled_in(&self.0, &r.move_set));
    }

    fn over(&self, r: &Rules) -> bool {
        r.bits.enabled_in(&self.0, &r.move_set).next().is_none()
    }

    fn play(&mut self, r: &Rules, t: usize) -> Result<bool> {
        if !r.bits.fire(t, &mut self.0) {
            return Ok(false);
        }
        // No automatic transition was enabled before `t`, so the only ones
        // that can be now are those the firings since have woken. Past
        // WOKE firings, check them all.
        const WOKE: usize = 8;
        let mut woke: [&[usize]; WOKE] = [&[]; WOKE];
        woke[0] = &r.auto_wakes[t];
        for n in 1..SETTLE {
            let next = if n <= WOKE {
                woke[..n]
                    .iter()
                    .filter_map(|ts| ts.iter().copied().find(|&u| r.bits.enabled(u, &self.0)))
                    .min()
            } else {
                r.bits.enabled_in(&self.0, &r.auto_set).next()
            };
            let Some(u) = next else {
                return Ok(true);
            };
            if !r.bits.fire(u, &mut self.0) {
                return Ok(false);
            }
            if n < WOKE {
                woke[n] = &r.auto_wakes[u];
            }
        }
        Err(endless())
    }

    fn score(&self, r: &Rules) -> Result<f64> {
        let has = |p: usize| self.0[p / 64] >> (p % 64) & 1 == 1;
        r.check(
            r.reward
                .iter()
                .map(|&(p, w)| if has(p) { w } else { 0.0 })
                .sum(),
        )
    }

    fn key(&self) -> u128 {
        // Apart from the counts' keys: a bit marking is the same position
        // as its counts, but hashes differently.
        key_of(self.0.iter().copied().chain([u64::MAX]))
    }
}

/// The table's keys are hashes already: use their low bits as they are.
#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = self.0.rotate_left(8) ^ b as u64;
        }
    }
    fn write_u128(&mut self, k: u128) {
        self.0 = k as u64;
    }
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
    tt: HashMap<u128, Entry, BuildHasherDefault<KeyHasher>>,
    history: Vec<u32>,
    nodes: usize,
    budget: usize,
    /// The bit engine met a marking it cannot hold.
    spilled: bool,
}

impl<'a> Solver<'a> {
    /// Entries kept before the table is cleared.
    const TABLE: usize = 2_000_000;

    pub fn new(r: &'a Rules, budget: usize) -> Self {
        Self {
            r,
            tt: HashMap::default(),
            history: vec![0; r.ix.transitions.len()],
            nodes: 0,
            budget,
            spilled: false,
        }
    }

    /// The exact value (+1 side's view) of a settled position with both
    /// sides playing perfectly, or `None` past `budget` nodes.
    pub fn value(&mut self, mk: &[i64]) -> Result<Option<f64>> {
        if self.tt.len() > Self::TABLE {
            self.tt.clear();
        }
        let (lo, hi) = self.r.bounds;
        if let Some(b) = self.r.bits.marking(mk).filter(|b| b.len() <= BIT_WORDS) {
            let mut w = [0; BIT_WORDS];
            w[..b.len()].copy_from_slice(&b);
            self.nodes = 0;
            self.spilled = false;
            let got = self.search(&Bits(w), lo, hi)?;
            if !self.spilled {
                return Ok(got);
            }
        }
        self.nodes = 0;
        let en = self.r.ix.enabled_set(mk);
        self.search(
            &Counts {
                mk: mk.to_vec(),
                en,
            },
            lo,
            hi,
        )
    }

    /// Fail-soft alpha-beta in the +1 side's view. Inside the bounds every
    /// result is a bound on the true value; at the root, whose window is
    /// the whole range, it is the value.
    fn search<W: Walk>(&mut self, w: &W, mut a: f64, mut b: f64) -> Result<Option<f64>> {
        self.nodes += 1;
        if self.nodes > self.budget {
            return Ok(None);
        }
        let k = w.key();
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
        let mut moves = Vec::with_capacity(8);
        w.moves(self.r, &mut moves);
        if moves.is_empty() {
            let x = w.score(self.r)?;
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
        let s = self.r.mover(&moves)?;
        // Children, best first: the remembered best move, then moves that
        // end the game in the mover's favour, then by history.
        let mut kids = Vec::with_capacity(moves.len());
        for &t in &moves {
            let mut next = w.clone();
            if !next.play(self.r, t)? {
                self.spilled = true;
                return Ok(None);
            }
            let rank = if t as u32 == first {
                u64::MAX
            } else if next.over(self.r) && s * next.score(self.r)? > 0.0 {
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

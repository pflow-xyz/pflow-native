//! Minimal siphons and traps, found from the arc structure alone: a port of
//! sim.pflow.xyz's `siphons.go` (FEATURES.md §D gap 2; its home upstream is
//! pflow-reachability).
//!
//! For a place set `S`, `•S` is every transition that puts a token into `S`
//! and `S•` every transition that needs a token in `S` to fire, by consuming
//! it or by a read arc (an inhibitor arc blocks when `S` is high, so it never
//! counts). `S` is a **siphon** when `•S ⊆ S•`: once empty it stays empty.
//! It is a **trap** when `S• ⊆ •S`: once marked it stays marked (Murata
//! 1989, §III.C).
//!
//! The search is a branch and bound like sim's: prune a branch when some
//! transition already forced into the set can never be answered for (dead
//! end) or when the set already contains a found minimal one, and search
//! each weakly connected component on its own, since a minimal siphon or
//! trap never spans two. It branches on violated constraints rather than on
//! places in a fixed order (see `minimal`): the same sets, in far fewer
//! nodes. Sets are `u128` bitmasks (sim uses `u64`, so this takes nets up to
//! 128 token places where sim stops at 63), and the search is bounded by
//! [`NODE_BUDGET`] nodes per component.
//!
//! Beyond sim, [`Io::max_trap_within`] finds the largest trap inside a siphon
//! (the standard fixpoint), which is what Commoner's property asks about.

/// Token places a search takes (one `u128` bitmask per set).
pub const PLACE_CAP: usize = 128;
/// Search nodes per connected component before the search refuses (sim's
/// `siphonTrapNodeBudget`, though a node here does more than one of sim's).
pub const NODE_BUDGET: usize = 20_000_000;

pub type Mask = u128;

pub fn bit(i: usize) -> Mask {
    1u128 << i
}

/// Indices of the set bits of `s`, ascending.
pub fn members(s: Mask) -> Vec<usize> {
    (0..128).filter(|&i| s & bit(i) != 0).collect()
}

/// The arc structure the closure tests read: per transition, the places it
/// writes to and the places it needs marked.
#[derive(Debug, Clone)]
pub struct Io {
    pub places: usize,
    /// `produces[t]`: places some output arc of `t` writes to.
    pub produces: Vec<Mask>,
    /// `needs[t]`: places `t` consumes from or reads (not inhibitors).
    pub needs: Vec<Mask>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiphonsTraps {
    /// Minimal siphons, smallest first.
    pub siphons: Vec<Mask>,
    pub traps: Vec<Mask>,
}

impl Io {
    /// `•S ⊆ S•`.
    pub fn is_siphon(&self, s: Mask) -> bool {
        s != 0
            && self
                .produces
                .iter()
                .zip(&self.needs)
                .all(|(p, n)| p & s == 0 || n & s != 0)
    }

    /// `S• ⊆ •S`.
    pub fn is_trap(&self, s: Mask) -> bool {
        s != 0
            && self
                .produces
                .iter()
                .zip(&self.needs)
                .all(|(p, n)| n & s == 0 || p & s != 0)
    }

    /// Weakly connected components of the place graph: two places are
    /// joined when one transition touches both, in either role.
    pub fn components(&self) -> Vec<Vec<usize>> {
        let n = self.places;
        let mut parent: Vec<usize> = (0..n).collect();
        fn find(parent: &mut [usize], mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }
        for (p, q) in self.produces.iter().zip(&self.needs) {
            let mut first = None;
            for i in members(p | q) {
                match first {
                    None => first = Some(i),
                    Some(f) => {
                        let (a, b) = (find(&mut parent, f), find(&mut parent, i));
                        if a != b {
                            parent[a] = b;
                        }
                    }
                }
            }
        }
        let mut by_root: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for i in 0..n {
            let r = find(&mut parent, i);
            by_root.entry(r).or_default().push(i);
        }
        let mut out: Vec<Vec<usize>> = by_root.into_values().collect();
        out.sort_by_key(|c| c[0]);
        out
    }

    /// Every minimal siphon and trap, or an error naming the component
    /// whose search ran out of budget (`names` labels places in it).
    pub fn minimal_sets(&self, names: &[String]) -> Result<SiphonsTraps, String> {
        if self.places > PLACE_CAP {
            return Err(format!(
                "{} token places exceeds the {PLACE_CAP}-place cap of the siphon/trap search \
                 (one 128-bit mask per candidate set)",
                self.places
            ));
        }
        let mut siphons = vec![];
        let mut traps = vec![];
        for comp in self.components() {
            let label = || {
                comp.iter()
                    .map(|&i| names.get(i).cloned().unwrap_or_else(|| i.to_string()))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let s = minimal(&comp, &self.produces, &self.needs, |m| self.is_siphon(m))
                .ok_or_else(|| {
                    format!(
                        "the minimal-siphon search over {{{}}} exceeded its {NODE_BUDGET}-node budget before completing",
                        label()
                    )
                })?;
            let t = minimal(&comp, &self.needs, &self.produces, |m| self.is_trap(m))
                .ok_or_else(|| {
                    format!(
                        "the minimal-trap search over {{{}}} exceeded its {NODE_BUDGET}-node budget before completing",
                        label()
                    )
                })?;
            siphons.extend(s);
            traps.extend(t);
        }
        let order = |v: &mut Vec<Mask>| {
            v.sort_by_key(|&m| (m.count_ones(), members(m)));
        };
        order(&mut siphons);
        order(&mut traps);
        Ok(SiphonsTraps { siphons, traps })
    }

    /// The largest trap contained in `s` (0 if none): repeatedly drop a
    /// place some transition needs without putting anything back into
    /// what is left.
    pub fn max_trap_within(&self, s: Mask) -> Mask {
        let mut q = s;
        loop {
            let mut changed = false;
            for (p, n) in self.produces.iter().zip(&self.needs) {
                let drained = n & q;
                if drained != 0 && p & q == 0 {
                    q &= !drained;
                    changed = true;
                }
            }
            if !changed {
                return q;
            }
        }
    }
}

/// Minimal closed sets over `bits` (one component), `None` when the budget
/// runs out. For siphons `drives = produces`, `rescue = needs`; for traps
/// the roles swap. A set is closed when every transition it drives (a
/// transition writing into a siphon) is rescued (needs a place in it).
///
/// sim decides places in a fixed order, in or out, and only prunes dead
/// ends, which costs millions of decisions on a 31-place board game. This
/// branches on the constraints instead: fix the set's smallest place, then
/// repeatedly pick a violated transition (fewest candidates first) and
/// branch on which of its rescuing places joins, excluding the earlier
/// candidates in later branches. Every closed set extends one such path, so
/// every minimal one is found; supersets of a found set are pruned, and the
/// few non-minimal closed sets the order lets through are filtered at the
/// end. Same sets as sim, far fewer nodes.
fn minimal(
    bits: &[usize],
    drives: &[Mask],
    rescue: &[Mask],
    test: impl Fn(Mask) -> bool,
) -> Option<Vec<Mask>> {
    struct Search<'a> {
        comp: Mask,
        drives: &'a [Mask],
        rescue: &'a [Mask],
        found: Vec<Mask>,
        nodes: usize,
        over: bool,
    }
    impl Search<'_> {
        fn go(&mut self, inn: Mask, out: Mask) {
            if self.over {
                return;
            }
            self.nodes += 1;
            if self.nodes > NODE_BUDGET {
                self.over = true;
                return;
            }
            if self.found.iter().any(|&f| f & !inn == 0) {
                return;
            }
            // The violated transition with the fewest ways to fix it.
            let mut pick: Option<Mask> = None;
            for (d, r) in self.drives.iter().zip(self.rescue) {
                if d & inn != 0 && r & inn == 0 {
                    let cand = r & self.comp & !out;
                    if cand == 0 {
                        return; // dead end: nothing left can rescue it
                    }
                    if pick.is_none_or(|p| cand.count_ones() < p.count_ones()) {
                        pick = Some(cand);
                    }
                }
            }
            let Some(cand) = pick else {
                self.found.push(inn); // closed
                return;
            };
            let mut out = out;
            for c in members(cand) {
                self.go(inn | bit(c), out);
                out |= bit(c);
            }
        }
    }
    let comp = bits.iter().fold(0, |m, &b| m | bit(b));
    let mut s = Search {
        comp,
        drives,
        rescue,
        found: vec![],
        nodes: 0,
        over: false,
    };
    let mut out = 0;
    for &b in bits {
        s.go(bit(b), out);
        out |= bit(b);
    }
    if s.over {
        return None;
    }
    let found = &s.found;
    let minimal: Vec<Mask> = found
        .iter()
        .copied()
        .filter(|&m| !found.iter().any(|&f| f != m && m & f == f))
        .collect();
    debug_assert!(minimal.iter().all(|&m| test(m)));
    Some(minimal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// p0 -t0-> p1 -t1-> p0.
    fn cycle() -> Io {
        Io {
            places: 2,
            produces: vec![0b10, 0b01],
            needs: vec![0b01, 0b10],
        }
    }

    #[test]
    fn two_cycle_needs_both_places() {
        let io = cycle();
        let r = io.minimal_sets(&[]).unwrap();
        assert_eq!(r.siphons, vec![0b11]);
        assert_eq!(r.traps, vec![0b11]);
        assert_eq!(io.max_trap_within(0b11), 0b11);
    }

    #[test]
    fn source_place_is_a_trap_not_a_siphon() {
        // t0 produces p0 from nothing; t1 consumes p0.
        let io = Io {
            places: 1,
            produces: vec![0b1, 0],
            needs: vec![0, 0b1],
        };
        let r = io.minimal_sets(&[]).unwrap();
        assert!(r.siphons.is_empty());
        assert!(r.traps.is_empty()); // t1 drains it without refilling
        let sink = Io {
            places: 1,
            produces: vec![0b1],
            needs: vec![0],
        };
        let r = sink.minimal_sets(&[]).unwrap();
        assert_eq!(r.traps, vec![0b1]);
        assert!(r.siphons.is_empty());
    }

    /// The search against brute force over every subset, on random nets.
    #[test]
    fn matches_brute_force() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        for _ in 0..300 {
            let np = 1 + rnd(9) as usize;
            let nt = 1 + rnd(8) as usize;
            let mut io = Io {
                places: np,
                produces: vec![0; nt],
                needs: vec![0; nt],
            };
            for t in 0..nt {
                for p in 0..np {
                    match rnd(6) {
                        0 => io.produces[t] |= bit(p),
                        1 => io.needs[t] |= bit(p),
                        _ => {}
                    }
                }
            }
            let brute = |closed: &dyn Fn(Mask) -> bool| -> Vec<Mask> {
                let all: Vec<Mask> = (1..(1u128 << np)).filter(|&m| closed(m)).collect();
                let mut min: Vec<Mask> = all
                    .iter()
                    .copied()
                    .filter(|&m| !all.iter().any(|&f| f != m && m & f == f))
                    .collect();
                min.sort_by_key(|&m| (m.count_ones(), members(m)));
                min
            };
            let r = io.minimal_sets(&[]).unwrap();
            assert_eq!(r.siphons, brute(&|m| io.is_siphon(m)), "{io:?}");
            assert_eq!(r.traps, brute(&|m| io.is_trap(m)), "{io:?}");
            for &s in &r.siphons {
                let t = io.max_trap_within(s);
                assert!(t == 0 || io.is_trap(t));
                let best = (1..(1u128 << np))
                    .filter(|&m| m & s == m && io.is_trap(m))
                    .fold(0, |a, m| a | m);
                assert_eq!(t, best, "{io:?}");
            }
        }
    }
}

//! Minimal-support semi-positive invariants by Farkas elimination
//! (Martínez & Silva 1982): every `y ≥ 0`, `y ≠ 0` with `y·M = 0` whose
//! support contains no other solution's support. Over the incidence matrix
//! (places × transitions) these are the P-invariants; over its transpose,
//! the T-invariants.
//!
//! pflow-reachability has the same elimination (`InvariantAnalyzer`), but
//! over its own sorted, token-place-only reading of a model and with
//! unchecked arithmetic (a large weight overflows, which panics in a debug
//! build). This one takes any integer matrix, so callers keep their own
//! row order (document order, colour copies), and treats an overflowing
//! combination as truncation rather than a crash. It is shared by
//! `analysis.invariants` and `sim.crosscheck`'s conservation laws.

/// The outcome of [`farkas`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FarkasResult {
    /// Minimal-support solutions, each indexed like the matrix rows, in
    /// lowest terms, sorted lexicographically.
    pub basis: Vec<Vec<i64>>,
    /// True if the row cap was hit or a combination overflowed: every
    /// returned vector is still an invariant, but some may be missing.
    pub truncated: bool,
}

/// Minimal-support semi-positive solutions of `y·M = 0` for an `r × c`
/// matrix `m` (rows of equal length). `max_rows` caps the intermediate rows
/// the elimination may hold.
pub fn farkas(m: &[Vec<i64>], max_rows: usize) -> FarkasResult {
    let n = m.len();
    let cols = m.first().map_or(0, Vec::len);
    type Row = (Vec<i64>, Vec<i64>);
    let mut rows: Vec<Row> = (0..n)
        .map(|i| {
            let mut y = vec![0; n];
            y[i] = 1;
            (m[i].clone(), y)
        })
        .collect();
    let mut truncated = false;
    let combine = |p: &Row, q: &Row, j: usize| -> Option<Row> {
        let a = -q.0[j];
        let b = p.0[j];
        let mix = |x: &[i64], y: &[i64]| -> Option<Vec<i64>> {
            x.iter()
                .zip(y)
                .map(|(u, v)| u.checked_mul(a)?.checked_add(v.checked_mul(b)?))
                .collect()
        };
        let mut d = mix(&p.0, &q.0)?;
        let mut y = mix(&p.1, &q.1)?;
        let g = d.iter().chain(y.iter()).fold(0, |g, &x| gcd(g, x));
        if g > 1 {
            d.iter_mut().for_each(|x| *x /= g);
            y.iter_mut().for_each(|x| *x /= g);
        }
        Some((d, y))
    };
    'cols: for j in 0..cols {
        let mut next: Vec<Row> = rows.iter().filter(|r| r.0[j] == 0).cloned().collect();
        let mut seen: std::collections::HashSet<Row> = next.iter().cloned().collect();
        let pos: Vec<&Row> = rows.iter().filter(|r| r.0[j] > 0).collect();
        let neg: Vec<&Row> = rows.iter().filter(|r| r.0[j] < 0).collect();
        for p in &pos {
            for q in &neg {
                match combine(p, q, j) {
                    Some(r) => {
                        if seen.insert(r.clone()) {
                            next.push(r);
                        }
                    }
                    None => truncated = true,
                }
                if next.len() > max_rows {
                    truncated = true;
                    rows = next;
                    break 'cols;
                }
            }
        }
        rows = prune(next);
    }
    if truncated {
        // A cut elimination leaves rows that are not invariants; keep only
        // the ones that are (every remaining entry zero).
        rows.retain(|r| r.0.iter().all(|&x| x == 0));
    }
    let mut ys: Vec<Vec<i64>> = rows
        .into_iter()
        .map(|r| r.1)
        .filter(|y| y.iter().any(|&x| x != 0))
        .collect();
    ys.sort();
    ys.dedup();
    let supports: Vec<Vec<usize>> = ys.iter().map(|y| support(y)).collect();
    let basis = ys
        .iter()
        .enumerate()
        .filter(|(i, _)| !strictly_contains_another(&supports, *i))
        .map(|(_, y)| y.clone())
        .collect();
    FarkasResult { basis, truncated }
}

/// Drops rows whose annotation support strictly contains another row's:
/// such a row can only lead to non-minimal invariants (the minimality
/// criterion of the elimination), and keeping it is what makes the row
/// count explode.
fn prune(rows: Vec<(Vec<i64>, Vec<i64>)>) -> Vec<(Vec<i64>, Vec<i64>)> {
    if rows.len() < 2 {
        return rows;
    }
    let supports: Vec<Vec<usize>> = rows.iter().map(|r| support(&r.1)).collect();
    rows.into_iter()
        .enumerate()
        .filter(|(i, _)| !strictly_contains_another(&supports, *i))
        .map(|(_, r)| r)
        .collect()
}

fn support(y: &[i64]) -> Vec<usize> {
    (0..y.len()).filter(|&i| y[i] != 0).collect()
}

fn strictly_contains_another(supports: &[Vec<usize>], i: usize) -> bool {
    supports.iter().enumerate().any(|(k, s)| {
        k != i && s.len() < supports[i].len() && s.iter().all(|x| supports[i].contains(x))
    })
}

pub fn gcd(a: i64, b: i64) -> i64 {
    let (mut a, mut b) = (a.abs(), b.abs());
    while b != 0 {
        (a, b) = (b, a % b);
    }
    a
}

/// `m` transposed (`r × c` → `c × r`); `cols` is used when `m` has no rows.
pub fn transpose(m: &[Vec<i64>], cols: usize) -> Vec<Vec<i64>> {
    let mut t = vec![vec![0i64; m.len()]; cols];
    for (i, row) in m.iter().enumerate() {
        for (j, &v) in row.iter().enumerate() {
            t[j][i] = v;
        }
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_cycle() {
        // p0 -t0-> p1 -t1-> p0
        let c = vec![vec![-1, 1], vec![1, -1]];
        let r = farkas(&c, 100);
        assert_eq!(r.basis, vec![vec![1, 1]]);
        assert!(!r.truncated);
        let t = farkas(&transpose(&c, 2), 100);
        assert_eq!(t.basis, vec![vec![1, 1]]);
    }

    #[test]
    fn weighted_and_lowest_terms() {
        // 2A -> B: A + 2B is conserved.
        let c = vec![vec![-2], vec![1]];
        assert_eq!(farkas(&c, 100).basis, vec![vec![1, 2]]);
    }

    #[test]
    fn source_has_none() {
        let c = vec![vec![1]];
        assert!(farkas(&c, 100).basis.is_empty());
    }

    #[test]
    fn overflow_is_truncation_not_a_panic() {
        let big = i64::MAX / 2;
        let c = vec![vec![big, 0], vec![-big + 1, 0], vec![0, 1]];
        let r = farkas(&c, 100);
        assert!(r.basis.iter().all(|y| {
            (0..2).all(|j| (0..3).map(|i| y[i] as i128 * c[i][j] as i128).sum::<i128>() == 0)
        }));
    }
}

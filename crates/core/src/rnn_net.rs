//! The Elman RNN behind `rnn.*`, laid out for speed: pflow-tropical's
//! `ElmanRnn` keeps each weight matrix as `Vec<Vec<f64>>` and allocates
//! on every step; this one keeps them flat, reuses its buffers, skips the
//! zero entries of the (mostly zero) marking vector, and runs the backward
//! pass as row updates the compiler vectorizes.
//!
//! It computes exactly what `ElmanRnn` computes: every sum adds the same
//! terms in the same order, so a model trains to the same weights bit for
//! bit (the tests check this against `ElmanRnn::train`, `train_weighted`
//! and `train_weighted_per_move`). `ElmanRnn::new` still makes the seeded
//! initial weights.

use pflow_tropical::rnn::ElmanRnn;

/// Gradient entries are clipped to ±CLIP before each update, as in ElmanRnn.
const CLIP: f64 = 5.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Rnn {
    pub inp: usize,
    pub hs: usize,
    pub out: usize,
    /// Row-major, `hs × inp`: `w_xh[i * inp + j]` is ElmanRnn's `w_xh[i][j]`.
    pub w_xh: Vec<f64>,
    /// `hs × hs`.
    pub w_hh: Vec<f64>,
    pub b_h: Vec<f64>,
    /// `out × hs`.
    pub w_hy: Vec<f64>,
    pub b_y: Vec<f64>,
}

/// One training sequence: the input at each move, the move made, and the
/// weight of its cross-entropy term (1 to imitate; a reward for REINFORCE).
pub(crate) struct Seq {
    pub inputs: Vec<Vec<f64>>,
    pub targets: Vec<usize>,
    pub weights: Vec<f64>,
    /// Per move, the legal moves its softmax ranges over; empty for the
    /// whole output layer. Only the MLP reads it (`positions: "sampled"`):
    /// over every output, the illegal moves' logits are pushed down without
    /// end, and long runs diverge. A sequence with masks also has its
    /// gradient clipped by length ([`clip_norm`]) rather than entry-wise.
    pub legal: Vec<Vec<usize>>,
}

impl Seq {
    /// Every move weighted 1 (`ElmanRnn::train`).
    pub fn plain(inputs: Vec<Vec<f64>>, targets: Vec<usize>) -> Self {
        let weights = vec![1.0; targets.len()];
        Self {
            inputs,
            targets,
            weights,
            legal: vec![],
        }
    }
}

fn flat(m: &[Vec<f64>]) -> Vec<f64> {
    m.iter().flatten().copied().collect()
}

/// `acc[i] += Σ_j w[i·cols + j] · v[j]` for every row `i`, adding the terms
/// in `j` order (only the `j` in `idx`, when given). Four rows at a time,
/// so four independent sums are in flight; each row's own sum keeps its
/// order.
pub(crate) fn rows_dot(acc: &mut [f64], w: &[f64], cols: usize, v: &[f64], idx: Option<&[usize]>) {
    let n = acc.len();
    let mut i = 0;
    while i + 4 <= n {
        let r = |k: usize| &w[(i + k) * cols..(i + k + 1) * cols];
        let (r0, r1, r2, r3) = (r(0), r(1), r(2), r(3));
        let (mut a0, mut a1, mut a2, mut a3) = (acc[i], acc[i + 1], acc[i + 2], acc[i + 3]);
        match idx {
            Some(idx) => {
                for &j in idx {
                    let x = v[j];
                    a0 += r0[j] * x;
                    a1 += r1[j] * x;
                    a2 += r2[j] * x;
                    a3 += r3[j] * x;
                }
            }
            None => {
                for j in 0..cols {
                    let x = v[j];
                    a0 += r0[j] * x;
                    a1 += r1[j] * x;
                    a2 += r2[j] * x;
                    a3 += r3[j] * x;
                }
            }
        }
        acc[i..i + 4].copy_from_slice(&[a0, a1, a2, a3]);
        i += 4;
    }
    for i in i..n {
        let row = &w[i * cols..(i + 1) * cols];
        let mut a = acc[i];
        match idx {
            Some(idx) => idx.iter().for_each(|&j| a += row[j] * v[j]),
            None => (0..cols).for_each(|j| a += row[j] * v[j]),
        }
        acc[i] = a;
    }
}

/// `dst[j] += w_row[j] · s` for every `j`.
pub(crate) fn axpy(dst: &mut [f64], w_row: &[f64], s: f64) {
    for (d, &w) in dst.iter_mut().zip(w_row) {
        *d += w * s;
    }
}

/// `dst[j] += s · v[j]` for every `j` (the product in that order).
pub(crate) fn axpy_s(dst: &mut [f64], s: f64, v: &[f64]) {
    for (d, &x) in dst.iter_mut().zip(v) {
        *d += s * x;
    }
}

/// `ElmanRnn::softmax`, into `out`.
pub(crate) fn softmax_into(logits: &[f64], out: &mut [f64]) {
    let max = logits.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    for (o, &x) in out.iter_mut().zip(logits) {
        *o = (x - max).exp();
    }
    let sum: f64 = out.iter().sum();
    out.iter_mut().for_each(|o| *o /= sum);
}

pub(crate) fn nonzero(x: &[f64], idx: &mut Vec<usize>) {
    idx.clear();
    idx.extend((0..x.len()).filter(|&j| x[j] != 0.0));
}

pub(crate) fn clip(v: &mut [f64]) {
    v.iter_mut().for_each(|x| *x = x.clamp(-CLIP, CLIP));
}

/// Scales `v` down to Euclidean length CLIP if it is longer, keeping its
/// direction. Entry-wise clipping lets every weight move by the full CLIP at
/// once; on 100,000 Connect-4 games that pushed an MLP's hidden units to
/// silence after 10–20 epochs, at every learning rate tried.
pub(crate) fn clip_norm(v: &mut [f64]) {
    let n = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if n > CLIP {
        v.iter_mut().for_each(|x| *x *= CLIP / n);
    }
}

pub(crate) fn descend(w: &mut [f64], g: &[f64], lr: f64) {
    for (w, &g) in w.iter_mut().zip(g) {
        *w -= lr * g;
    }
}

impl Rnn {
    /// Seeded small random weights, as `ElmanRnn::new` makes them.
    pub fn new(inp: usize, hs: usize, out: usize, seed: u64) -> Self {
        Self::from_elman(&ElmanRnn::new(inp, hs, out, seed))
    }

    pub fn from_elman(e: &ElmanRnn) -> Self {
        Self {
            inp: e.input_size,
            hs: e.hidden_size,
            out: e.output_size,
            w_xh: flat(&e.w_xh),
            w_hh: flat(&e.w_hh),
            b_h: e.b_h.clone(),
            w_hy: flat(&e.w_hy),
            b_y: e.b_y.clone(),
        }
    }

    /// The weights as ElmanRnn's nested rows (the model artifact's format).
    pub fn rows(&self) -> [Vec<Vec<f64>>; 3] {
        [
            self.w_xh
                .chunks(self.inp.max(1))
                .map(<[f64]>::to_vec)
                .collect(),
            self.w_hh.chunks(self.hs).map(<[f64]>::to_vec).collect(),
            self.w_hy.chunks(self.hs).map(<[f64]>::to_vec).collect(),
        ]
    }

    /// Forward one step: `h_pre = b_h + W_xh·x + W_hh·h_prev`, `h = tanh`,
    /// `logits = b_y + W_hy·h`. `zero_h` says `h_prev` is all zeros, whose
    /// terms are then skipped; `idx` lists the nonzero entries of `x`.
    fn forward(
        &self,
        x: &[f64],
        idx: &[usize],
        h_prev: &[f64],
        zero_h: bool,
        h: &mut [f64],
        logits: &mut [f64],
    ) {
        h.copy_from_slice(&self.b_h);
        rows_dot(h, &self.w_xh, self.inp, x, Some(idx));
        if !zero_h {
            rows_dot(h, &self.w_hh, self.hs, h_prev, None);
        }
        h.iter_mut().for_each(|v| *v = v.tanh());
        logits.copy_from_slice(&self.b_y);
        rows_dot(logits, &self.w_hy, self.hs, h, None);
    }

    /// One step of play: updates `h` and fills `logits` (`ElmanRnn::step`).
    pub fn step(&self, x: &[f64], h: &mut Vec<f64>, logits: &mut [f64]) {
        let mut idx = Vec::with_capacity(x.len());
        nonzero(x, &mut idx);
        let zero_h = h.iter().all(|&v| v == 0.0);
        let mut next = vec![0.0; self.hs];
        self.forward(x, &idx, h, zero_h, &mut next, logits);
        *h = next;
    }

    /// Backpropagation through time with weighted cross-entropy, for
    /// `epochs` passes. Returns the last pass's weighted loss summed over
    /// each sequence's moves and averaged over sequences. Empty sequences
    /// are skipped (and still counted).
    ///
    /// `batch` 1 is ElmanRnn's SGD, one update per sequence. A larger batch
    /// works out every sequence's (clipped, `lr / len`-scaled) step against
    /// the same weights, on all cores, and then applies them in sequence
    /// order: the result depends on `batch` but not on the number of cores.
    pub fn train(&mut self, seqs: &[Seq], lr: f64, epochs: usize, batch: usize) -> f64 {
        let longest = seqs.iter().map(|s| s.inputs.len()).max().unwrap_or(0);
        let mut w = Work::new(self, longest);
        let batch = batch.max(1);
        // Sequences per parallel task: about 16 tasks per batch.
        let sub = batch.div_ceil(16);
        let mut loss = 0.0;
        for _ in 0..epochs {
            loss = 0.0;
            if batch == 1 {
                for s in seqs {
                    if s.inputs.is_empty() {
                        continue;
                    }
                    self.grads(s, &mut w, &mut loss);
                    let lr = lr / s.inputs.len() as f64;
                    descend(&mut self.w_xh, &w.g_xh, lr);
                    descend(&mut self.w_hh, &w.g_hh, lr);
                    descend(&mut self.b_h, &w.g_bh, lr);
                    descend(&mut self.w_hy, &w.g_hy, lr);
                    descend(&mut self.b_y, &w.g_by, lr);
                }
            } else {
                for chunk in seqs.chunks(batch) {
                    let steps = crate::par::map(chunk.chunks(sub).collect(), |part: &[Seq]| {
                        let mut w = Work::new(self, longest);
                        let mut step = Step::zeros(self);
                        let mut loss = 0.0;
                        for s in part.iter().filter(|s| !s.inputs.is_empty()) {
                            self.grads(s, &mut w, &mut loss);
                            step.add(&w, lr / s.inputs.len() as f64);
                        }
                        (step, loss)
                    });
                    for (step, l) in steps {
                        loss += l;
                        descend(&mut self.w_xh, &step.xh, 1.0);
                        descend(&mut self.w_hh, &step.hh, 1.0);
                        descend(&mut self.b_h, &step.bh, 1.0);
                        descend(&mut self.w_hy, &step.hy, 1.0);
                        descend(&mut self.b_y, &step.by, 1.0);
                    }
                }
            }
            loss /= seqs.len().max(1) as f64;
        }
        loss
    }

    /// One sequence's clipped gradients into `w.g_*`; subtracts its
    /// log-likelihood terms from `loss` one by one, as ElmanRnn does.
    fn grads(&self, s: &Seq, w: &mut Work, loss: &mut f64) {
        let (inp, hs, out) = (self.inp, self.hs, self.out);
        let len = s.inputs.len();
        for t in 0..len {
            nonzero(&s.inputs[t], &mut w.idx[t]);
            let (prev, rest) = w.h[t * hs..(t + 2) * hs].split_at_mut(hs);
            self.forward(
                &s.inputs[t],
                &w.idx[t],
                prev,
                t == 0,
                rest,
                &mut w.logits[t * out..(t + 1) * out],
            );
        }
        for g in [
            &mut w.g_xh,
            &mut w.g_hh,
            &mut w.g_bh,
            &mut w.g_hy,
            &mut w.g_by,
            &mut w.dh_next,
        ] {
            g.fill(0.0);
        }
        let Work {
            h,
            logits,
            idx,
            g_xh,
            g_hh,
            g_bh,
            g_hy,
            g_by,
            dl,
            dh,
            dh_pre,
            dh_next,
        } = w;
        for t in (0..len).rev() {
            let (h_prev, h_post) = (&h[t * hs..(t + 1) * hs], &h[(t + 1) * hs..(t + 2) * hs]);
            softmax_into(&logits[t * out..(t + 1) * out], dl);
            let (target, r) = (s.targets[t], s.weights[t]);
            *loss -= r * dl[target].max(1e-10).ln();
            dl.iter_mut().for_each(|p| *p *= r);
            dl[target] -= r;

            for (i, &d) in dl.iter().enumerate() {
                axpy_s(&mut g_hy[i * hs..(i + 1) * hs], d, h_post);
                g_by[i] += d;
            }
            dh.fill(0.0);
            for (row, &d) in self.w_hy.chunks_exact(hs).zip(dl.iter()) {
                axpy(dh, row, d);
            }
            for j in 0..hs {
                dh[j] += dh_next[j];
                dh_pre[j] = dh[j] * (1.0 - h_post[j] * h_post[j]);
            }
            let x = &s.inputs[t];
            for (i, &d) in dh_pre.iter().enumerate() {
                let row = &mut g_xh[i * inp..(i + 1) * inp];
                for &j in &idx[t] {
                    row[j] += d * x[j];
                }
                if t > 0 {
                    axpy_s(&mut g_hh[i * hs..(i + 1) * hs], d, h_prev);
                }
                g_bh[i] += d;
            }
            if t > 0 {
                dh_next.fill(0.0);
                for (row, &d) in self.w_hh.chunks_exact(hs).zip(dh_pre.iter()) {
                    axpy(dh_next, row, d);
                }
            }
        }
        for g in [g_xh, g_hh, g_bh, g_hy, g_by] {
            clip(g);
        }
    }
}

/// Buffers for one sequence's forward and backward pass.
struct Work {
    /// h[t] for t = 0..=len: h[0] = 0, h[t + 1] the state after move t.
    h: Vec<f64>,
    logits: Vec<f64>,
    idx: Vec<Vec<usize>>,
    g_xh: Vec<f64>,
    g_hh: Vec<f64>,
    g_bh: Vec<f64>,
    g_hy: Vec<f64>,
    g_by: Vec<f64>,
    dl: Vec<f64>,
    dh: Vec<f64>,
    dh_pre: Vec<f64>,
    dh_next: Vec<f64>,
}

impl Work {
    fn new(r: &Rnn, longest: usize) -> Self {
        let (inp, hs, out) = (r.inp, r.hs, r.out);
        Self {
            h: vec![0.0; (longest + 1) * hs],
            logits: vec![0.0; longest * out],
            idx: vec![vec![]; longest],
            g_xh: vec![0.0; hs * inp],
            g_hh: vec![0.0; hs * hs],
            g_bh: vec![0.0; hs],
            g_hy: vec![0.0; out * hs],
            g_by: vec![0.0; out],
            dl: vec![0.0; out],
            dh: vec![0.0; hs],
            dh_pre: vec![0.0; hs],
            dh_next: vec![0.0; hs],
        }
    }
}

/// The summed weight steps of part of a batch.
struct Step {
    xh: Vec<f64>,
    hh: Vec<f64>,
    bh: Vec<f64>,
    hy: Vec<f64>,
    by: Vec<f64>,
}

impl Step {
    fn zeros(r: &Rnn) -> Self {
        Self {
            xh: vec![0.0; r.w_xh.len()],
            hh: vec![0.0; r.w_hh.len()],
            bh: vec![0.0; r.b_h.len()],
            hy: vec![0.0; r.w_hy.len()],
            by: vec![0.0; r.b_y.len()],
        }
    }

    fn add(&mut self, w: &Work, lr: f64) {
        for (d, g) in [
            (&mut self.xh, &w.g_xh),
            (&mut self.hh, &w.g_hh),
            (&mut self.bh, &w.g_bh),
            (&mut self.hy, &w.g_hy),
            (&mut self.by, &w.g_by),
        ] {
            axpy_s(d, lr, g);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pflow_tropical::rnn::SimpleRng;

    #[test]
    fn clip_norm_keeps_direction_and_short_vectors() {
        let mut long = vec![30.0, -40.0];
        clip_norm(&mut long);
        assert!((long[0] - 3.0).abs() < 1e-12 && (long[1] + 4.0).abs() < 1e-12);
        let mut short = vec![1.0, -2.0];
        clip_norm(&mut short);
        assert_eq!(short, vec![1.0, -2.0]);
    }

    /// (inputs, targets, per-move weights), ElmanRnn's per-move format.
    type Data = Vec<(Vec<Vec<f64>>, Vec<usize>, Vec<f64>)>;

    /// Sequences shaped like markings: small token counts, mostly zero.
    fn data(seed: u64, inp: usize, out: usize, n: usize) -> Data {
        let mut rng = SimpleRng::new(seed);
        (0..n)
            .map(|_| {
                let len = 1 + (rng.next_u64() % 9) as usize;
                let inputs = (0..len)
                    .map(|_| {
                        (0..inp)
                            .map(|_| [0.0, 0.0, 0.0, 1.0, 2.0][(rng.next_u64() % 5) as usize])
                            .collect()
                    })
                    .collect();
                let targets = (0..len)
                    .map(|_| (rng.next_u64() % out as u64) as usize)
                    .collect();
                let weights = (0..len)
                    .map(|_| [1.0, -1.0, 0.5][(rng.next_u64() % 3) as usize])
                    .collect();
                (inputs, targets, weights)
            })
            .collect()
    }

    fn same(fast: &Rnn, e: &ElmanRnn) {
        assert_eq!(fast, &Rnn::from_elman(e), "weights differ from ElmanRnn's");
    }

    #[test]
    fn trains_exactly_like_elman_rnn() {
        for (inp, hs, out) in [(7, 5, 4), (12, 16, 9), (29, 33, 28)] {
            let d = data(inp as u64, inp, out, 40);

            let mut e = ElmanRnn::new(inp, hs, out, 3);
            let mut f = Rnn::new(inp, hs, out, 3);
            let plain: Vec<_> = d.iter().map(|(x, t, _)| (x.clone(), t.clone())).collect();
            let seqs: Vec<_> = plain
                .iter()
                .map(|(x, t)| Seq::plain(x.clone(), t.clone()))
                .collect();
            assert_eq!(e.train(&plain, 0.05, 3), f.train(&seqs, 0.05, 3, 1));
            same(&f, &e);

            let per_seq: Vec<_> = d
                .iter()
                .map(|(x, t, w)| (x.clone(), t.clone(), w[0]))
                .collect();
            let seqs: Vec<_> = per_seq
                .iter()
                .filter(|(_, _, r)| r.abs() >= 1e-10)
                .map(|(x, t, r)| Seq {
                    inputs: x.clone(),
                    targets: t.clone(),
                    weights: vec![*r; t.len()],
                    legal: vec![],
                })
                .collect();
            assert_eq!(
                e.train_weighted(&per_seq, 0.05, 2),
                f.train(&seqs, 0.05, 2, 1)
            );
            same(&f, &e);

            let seqs: Vec<_> = d
                .iter()
                .map(|(x, t, w)| Seq {
                    inputs: x.clone(),
                    targets: t.clone(),
                    weights: w.clone(),
                    legal: vec![],
                })
                .collect();
            assert_eq!(
                e.train_weighted_per_move(&d, 0.05, 2),
                f.train(&seqs, 0.05, 2, 1)
            );
            same(&f, &e);
        }
    }

    #[test]
    fn steps_exactly_like_elman_rnn() {
        let e = ElmanRnn::new(10, 12, 6, 9);
        let f = Rnn::from_elman(&e);
        let (mut he, mut hf) = (vec![0.0; 12], vec![0.0; 12]);
        let mut logits = vec![0.0; 6];
        for x in &data(5, 10, 6, 1)[0].0 {
            let (h2, y) = e.step(x, &he);
            he = h2;
            f.step(x, &mut hf, &mut logits);
            assert_eq!(he, hf);
            assert_eq!(y, logits);
        }
        let mut p = vec![0.0; 6];
        softmax_into(&logits, &mut p);
        assert_eq!(p, ElmanRnn::softmax(&logits));
    }
}

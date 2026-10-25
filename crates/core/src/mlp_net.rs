//! A feed-forward policy for `rnn.*` (`arch: "mlp"`): ReLU hidden layers
//! over the marking, then a softmax over the transitions. A Petri net's
//! marking is its whole state, so a policy needs no memory of earlier
//! moves; this one has none, and depth (`layers`) is where its capacity
//! goes.
//!
//! It trains like [`crate::rnn_net::Rnn`]: each game's moves give a
//! cross-entropy term weighted 1 (imitation) or by reward (REINFORCE),
//! gradients are summed over the game, clipped to ±5 and applied with
//! `learning_rate / moves`, and `batch` games' steps are worked out on every
//! core and applied in game order. So `learning_rate`, `batch` and the loss
//! mean the same for both architectures.

use pflow_tropical::rnn::SimpleRng;

use crate::rnn_net::{axpy, axpy_s, clip, descend, nonzero, rows_dot, softmax_into, Seq};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Mlp {
    /// Layer widths: the input, each hidden layer, the output.
    pub sizes: Vec<usize>,
    /// Every weight and bias, layer by layer: layer `l`'s `sizes[l] ×
    /// sizes[l-1]` weights (row-major), then its `sizes[l]` biases.
    pub params: Vec<f64>,
}

impl Mlp {
    /// He-initialized weights (normal, sd √(2 / fan-in)) and zero biases,
    /// from a seed.
    pub fn new(sizes: Vec<usize>, seed: u64) -> Self {
        let mut rng = SimpleRng::new(seed);
        let mut params = vec![];
        for l in 1..sizes.len() {
            let sd = (2.0 / sizes[l - 1].max(1) as f64).sqrt();
            params.extend((0..sizes[l] * sizes[l - 1]).map(|_| rng.normal() * sd));
            params.extend(std::iter::repeat_n(0.0, sizes[l]));
        }
        Self { sizes, params }
    }

    /// Number of weight layers.
    pub fn depth(&self) -> usize {
        self.sizes.len() - 1
    }

    /// Where layer `l`'s (1-based) weights and biases start in `params`.
    fn offset(&self, l: usize) -> usize {
        (1..l)
            .map(|k| self.sizes[k] * (self.sizes[k - 1] + 1))
            .sum()
    }

    /// Layer `l`'s weights and biases.
    pub fn layer(&self, l: usize) -> (&[f64], &[f64]) {
        let (o, n, m) = (self.offset(l), self.sizes[l], self.sizes[l - 1]);
        (
            &self.params[o..o + n * m],
            &self.params[o + n * m..o + n * m + n],
        )
    }

    /// Builds one from its layers' weights (rows) and biases; checks shapes.
    pub fn from_layers(layers: &[(Vec<Vec<f64>>, Vec<f64>)]) -> Option<Self> {
        let first = layers.first()?;
        let mut sizes = vec![first.0.first()?.len()];
        let mut params = vec![];
        for (w, b) in layers {
            let fan_in = *sizes.last().expect("sizes");
            if w.len() != b.len() || w.is_empty() || w.iter().any(|r| r.len() != fan_in) {
                return None;
            }
            sizes.push(b.len());
            params.extend(w.iter().flatten());
            params.extend(b);
        }
        Some(Self { sizes, params })
    }

    /// Forward pass into `acts[1..=depth]` (the last is the logits); the
    /// input is `x`, with nonzero entries `idx`.
    fn forward(&self, x: &[f64], idx: &[usize], acts: &mut [Vec<f64>]) {
        let depth = self.depth();
        for l in 1..=depth {
            let (w, b) = self.layer(l);
            let (before, after) = acts.split_at_mut(l);
            let a = &mut after[0];
            a.copy_from_slice(b);
            if l == 1 {
                rows_dot(a, w, self.sizes[0], x, Some(idx));
            } else {
                rows_dot(a, w, self.sizes[l - 1], &before[l - 1], None);
            }
            if l < depth {
                a.iter_mut().for_each(|v| *v = v.max(0.0));
            }
        }
    }

    fn buffers(&self) -> Vec<Vec<f64>> {
        self.sizes.iter().map(|&n| vec![0.0; n]).collect()
    }

    /// The logits at input `x`.
    pub fn logits(&self, x: &[f64]) -> Vec<f64> {
        let mut idx = vec![];
        nonzero(x, &mut idx);
        let mut acts = self.buffers();
        self.forward(x, &idx, &mut acts);
        acts.pop().expect("output layer")
    }

    /// One game's gradients (clipped) into `w.g`; subtracts its weighted
    /// log-likelihood terms from `loss`.
    fn grads(&self, s: &Seq, w: &mut Work, loss: &mut f64) {
        let depth = self.depth();
        w.g.fill(0.0);
        for (t, x) in s.inputs.iter().enumerate() {
            nonzero(x, &mut w.idx);
            self.forward(x, &w.idx, &mut w.acts);
            let out = &mut w.d[depth];
            match s.legal.get(t).filter(|l| !l.is_empty()) {
                None => softmax_into(&w.acts[depth], out),
                Some(legal) => {
                    // Over the legal moves only; the rest get no gradient.
                    let z = &w.acts[depth];
                    let top = legal
                        .iter()
                        .map(|&i| z[i])
                        .fold(f64::NEG_INFINITY, f64::max);
                    out.fill(0.0);
                    let mut sum = 0.0;
                    for &i in legal {
                        out[i] = (z[i] - top).exp();
                        sum += out[i];
                    }
                    for &i in legal {
                        out[i] /= sum;
                    }
                }
            }
            let (target, r) = (s.targets[t], s.weights[t]);
            *loss -= r * out[target].max(1e-10).ln();
            out.iter_mut().for_each(|p| *p *= r);
            out[target] -= r;
            for l in (1..=depth).rev() {
                let (o, n, m) = (self.offset(l), self.sizes[l], self.sizes[l - 1]);
                let (weights, _) = self.layer(l);
                let (d_prev, d_here) = w.d.split_at_mut(l);
                let dz = &d_here[0];
                let (gw, gb) = w.g[o..o + n * m + n].split_at_mut(n * m);
                for (i, &d) in dz.iter().enumerate().filter(|(_, &d)| d != 0.0) {
                    let row = &mut gw[i * m..(i + 1) * m];
                    if l == 1 {
                        w.idx.iter().for_each(|&j| row[j] += d * x[j]);
                    } else {
                        axpy_s(row, d, &w.acts[l - 1]);
                    }
                    gb[i] += d;
                }
                if l > 1 {
                    // Back through layer l's weights and layer l-1's ReLU.
                    let da = &mut d_prev[l - 1];
                    da.fill(0.0);
                    for (i, &d) in dz.iter().enumerate().filter(|(_, &d)| d != 0.0) {
                        axpy(da, &weights[i * m..(i + 1) * m], d);
                    }
                    for (g, &a) in da.iter_mut().zip(&w.acts[l - 1]) {
                        if a <= 0.0 {
                            *g = 0.0;
                        }
                    }
                }
            }
        }
        clip(&mut w.g);
    }

    /// Trains for `epochs` passes; returns the last pass's loss, summed over
    /// each game's moves and averaged over games (as `Rnn::train`).
    pub fn train(&mut self, seqs: &[Seq], lr: f64, epochs: usize, batch: usize) -> f64 {
        let batch = batch.max(1);
        let sub = batch.div_ceil(16);
        let mut w = Work::new(self);
        let mut loss = 0.0;
        for _ in 0..epochs {
            loss = 0.0;
            if batch == 1 {
                for s in seqs.iter().filter(|s| !s.inputs.is_empty()) {
                    self.grads(s, &mut w, &mut loss);
                    descend(&mut self.params, &w.g, lr / s.inputs.len() as f64);
                }
            } else {
                for chunk in seqs.chunks(batch) {
                    let steps = crate::par::map(chunk.chunks(sub).collect(), |part: &[Seq]| {
                        let mut w = Work::new(self);
                        let mut step = vec![0.0; self.params.len()];
                        let mut loss = 0.0;
                        for s in part.iter().filter(|s| !s.inputs.is_empty()) {
                            self.grads(s, &mut w, &mut loss);
                            axpy_s(&mut step, lr / s.inputs.len() as f64, &w.g);
                        }
                        (step, loss)
                    });
                    for (step, l) in steps {
                        loss += l;
                        descend(&mut self.params, &step, 1.0);
                    }
                }
            }
            loss /= seqs.len().max(1) as f64;
        }
        loss
    }
}

/// Buffers for one game's passes.
struct Work {
    acts: Vec<Vec<f64>>,
    /// Gradient with respect to each layer's pre-activation.
    d: Vec<Vec<f64>>,
    idx: Vec<usize>,
    g: Vec<f64>,
}

impl Work {
    fn new(m: &Mlp) -> Self {
        Self {
            acts: m.buffers(),
            d: m.buffers(),
            idx: vec![],
            g: vec![0.0; m.params.len()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loss_of(m: &Mlp, s: &Seq) -> f64 {
        let mut loss = 0.0;
        for (t, x) in s.inputs.iter().enumerate() {
            let y = m.logits(x);
            let p = match s.legal.get(t).filter(|l| !l.is_empty()) {
                None => {
                    let mut p = vec![0.0; y.len()];
                    softmax_into(&y, &mut p);
                    p[s.targets[t]]
                }
                Some(legal) => {
                    let sum: f64 = legal.iter().map(|&i| y[i].exp()).sum();
                    y[s.targets[t]].exp() / sum
                }
            };
            loss -= s.weights[t] * p.max(1e-10).ln();
        }
        loss
    }

    #[test]
    fn gradients_match_finite_differences() {
        gradients_check(vec![]);
    }

    #[test]
    fn gradients_over_legal_moves_match_finite_differences() {
        gradients_check(vec![vec![1, 2], vec![0, 1, 3], vec![3]]);
    }

    fn gradients_check(legal: Vec<Vec<usize>>) {
        let m = Mlp::new(vec![5, 7, 6, 4], 11);
        let s = Seq {
            legal,
            inputs: vec![
                vec![1.0, 0.0, 2.0, 0.0, 1.0],
                vec![0.0, 1.0, 1.0, 0.0, 0.0],
                vec![2.0, 0.0, 0.0, 1.0, 1.0],
            ],
            targets: vec![2, 0, 3],
            weights: vec![1.0, -0.5, 0.75],
        };
        let mut w = Work::new(&m);
        let mut loss = 0.0;
        m.grads(&s, &mut w, &mut loss);
        assert!((loss - loss_of(&m, &s)).abs() < 1e-12);
        assert!(
            w.g.iter().all(|g| g.abs() < 5.0),
            "no clipping in this check"
        );
        let eps = 1e-6;
        for k in 0..m.params.len() {
            let (mut up, mut down) = (m.clone(), m.clone());
            up.params[k] += eps;
            down.params[k] -= eps;
            let numeric = (loss_of(&up, &s) - loss_of(&down, &s)) / (2.0 * eps);
            assert!(
                (numeric - w.g[k]).abs() < 1e-5,
                "param {k}: numeric {numeric}, backprop {}",
                w.g[k]
            );
        }
    }

    #[test]
    fn batched_training_does_not_depend_on_threads_and_learns() {
        // A rule a single layer cannot fit: target = parity of x0 + x1.
        let seqs: Vec<Seq> = (0..64)
            .map(|i| {
                let (a, b) = ((i & 1) as f64, ((i >> 1) & 1) as f64);
                Seq::plain(vec![vec![a, b, 1.0]], vec![((a + b) as usize) % 2])
            })
            .collect();
        let mut m = Mlp::new(vec![3, 16, 16, 2], 4);
        let first = m.clone().train(&seqs, 0.1, 1, 8);
        let last = m.train(&seqs, 0.1, 300, 8);
        assert!(last < 0.05 && last < first, "loss {first} -> {last}");
        let one_thread = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("pool");
        let mut again = Mlp::new(vec![3, 16, 16, 2], 4);
        one_thread.install(|| again.train(&seqs, 0.1, 300, 8));
        assert_eq!(again, m, "one thread and many give the same weights");
    }
}

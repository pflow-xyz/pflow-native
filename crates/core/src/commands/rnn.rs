//! Experimental (cargo feature `rnn`, `make build-rnn`): an Elman RNN that
//! learns which transition fires next from the marking, using pflow-tropical's
//! `ElmanRnn` for the network and its training (BPTT, REINFORCE-weighted).
//!
//! Every trajectory and every game goes through the metamodel firing rule
//! (`sim::Compiled`), so colours, inhibitor and read arcs and capacities mean
//! what they mean everywhere else; pflow-tropical never fires anything here.
//! A trained model is a report artifact holding its weights. `rnn.play`
//! stores the game it plays as a trace, which `sim.goto` replays and
//! `provenance.check_witness` re-checks. A learned policy is a guess, not a
//! verdict: these commands prove nothing about the net.

use pflow_tropical::rnn::{ElmanRnn, SimpleRng};
use serde_json::{json, Value};

use crate::analysis::Structure;
use crate::artifact::{ArtifactData, Series, TraceStep};
use crate::registry::{always, b, n, opt, s, ArgType::*, CommandSpec};
use crate::session::{Session, Step};
use crate::sim::{self, Compiled, Marking};
use crate::{Error, Result};

const MENU: &str = "Learn";
const KIND: &str = "rnn-model";

pub const SET: &[CommandSpec] = &[
    CommandSpec {
        id: "rnn.train",
        label: "Train RNN Policy (experimental)",
        help: "Experimental. Play `games` random games (each picks uniformly among the enabled transitions, through the metamodel \
               firing rule, until none is enabled or `max_steps`), then train an Elman RNN (pflow-tropical) to predict the next \
               transition from the marking. Stores the loss per epoch, with the floor a perfect imitator of uniform play would \
               reach (Σ ln of the number of enabled transitions over a game's moves, averaged over games, the same scale as the \
               loss), and the model as a report artifact for rnn.play and \
               rnn.self_play. Seeded and reproducible.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("hidden", Int, "hidden units (default 32, 1..256)"),
            opt("games", Int, "random games to learn from (default 200, 1..5000)"),
            opt("epochs", Int, "passes over the games (default 40, 1..1000)"),
            opt("learning_rate", Num, "step size (default 0.01)"),
            opt("max_steps", Int, "firings per game at most (default 200, 1..10000)"),
            opt("seed", Int, "seed for the games and the initial weights (default 1)"),
            opt("from_current", Bool, "start games from the canvas marking instead of the initial marking"),
        ],
        enabled: always,
        run: train,
        journal: true,
    },
    CommandSpec {
        id: "rnn.play",
        label: "Play RNN Policy (experimental)",
        help: "Experimental. Let a trained RNN pick every firing, among the enabled transitions only, until none is enabled or \
               `max_steps`. Greedy unless `stochastic`. Stores the game as a trace (sim.goto replays it on the canvas; \
               provenance.check_witness re-checks it) and a report with each move's probability and the runners-up.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("model", Str, "rnn model artifact id (default: the newest)"),
            opt("stochastic", Bool, "sample moves from the policy instead of taking the most likely"),
            opt("seed", Int, "seed for stochastic play (default 1)"),
            opt("max_steps", Int, "firings at most (default 200, 1..10000)"),
            opt("from_current", Bool, "start from the canvas marking instead of the initial marking"),
        ],
        enabled: always,
        run: play,
        journal: true,
    },
    CommandSpec {
        id: "rnn.self_play",
        label: "Self-Play RNN Policy (experimental)",
        help: "Experimental. Improve a trained RNN by self-play (REINFORCE): each round plays `games` games sampled from the policy, \
               scores each by `reward` (Σ weight × tokens at the end, e.g. {\"win_x\": 1, \"win_o\": -1}) and trains on the games \
               weighted by their score. One network plays every move, so with only `reward` it learns to make the rewarded \
               outcome happen from both sides (on tic-tac-toe, O learns to lose); `sides` ({transition-id prefix: sign}, e.g. \
               {\"x_\": 1, \"o_\": -1}) gives each move the reward times its side's sign, so both sides learn to play for \
               themselves (moves matching no prefix are not trained). Stores the mean reward and the share of positive, zero and negative games per round, and \
               the improved model as a new report artifact.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("model", Str, "rnn model artifact id to start from (default: the newest)"),
            opt("reward", Json, "{place: weight}, read from the final marking (a place id sums its colours; place.color names one)"),
            opt("sides", Json, "{transition-id prefix: sign}: zero-sum self-play, each move rewarded by its side (longest prefix wins)"),
            opt("rounds", Int, "rounds (default 20, 1..500)"),
            opt("games", Int, "games per round (default 50, 1..2000)"),
            opt("epochs", Int, "training passes per round (default 1, 1..100)"),
            opt("learning_rate", Num, "step size (default 0.01)"),
            opt("max_steps", Int, "firings per game at most (default 200, 1..10000)"),
            opt("seed", Int, "seed (default 1)"),
            opt("from_current", Bool, "start games from the canvas marking instead of the initial marking"),
        ],
        enabled: always,
        run: self_play,
        journal: true,
    },
];

fn source(id: &str, a: &Value) -> Step {
    Step {
        command: id.into(),
        args: a.clone(),
    }
}

fn int(a: &Value, k: &str, default: usize, lo: usize, hi: usize) -> Result<usize> {
    let v = match n(a, k) {
        None => return Ok(default),
        Some(x) if x.fract() == 0.0 && x >= lo as f64 && x <= hi as f64 => x as usize,
        Some(x) => {
            return Err(Error::BadArgs(format!(
                "{k} must be an integer in {lo}..{hi}, got {x}"
            )))
        }
    };
    Ok(v)
}

fn rate(a: &Value) -> Result<f64> {
    match n(a, "learning_rate") {
        None => Ok(0.01),
        Some(x) if x > 0.0 && x.is_finite() => Ok(x),
        Some(x) => Err(Error::BadArgs(format!(
            "learning_rate must be > 0, got {x}"
        ))),
    }
}

/// The open net as the policy sees it: the firing rule, the token places
/// (the input vector) and the transitions (the output classes).
struct View {
    compiled: Compiled,
    st: Structure,
}

impl View {
    fn new(sess: &Session) -> Result<Self> {
        let st = Structure::new(&sess.doc, &sess.marking)?;
        if st.places.is_empty() || st.transitions.is_empty() {
            return Err(Error::Invalid(
                "the net needs at least one place and one transition".into(),
            ));
        }
        Ok(Self {
            compiled: Compiled::new(&sess.doc)?,
            st,
        })
    }

    fn input(&self, mk: &Marking) -> Vec<f64> {
        self.st.marking(mk).into_iter().map(|v| v as f64).collect()
    }

    fn enabled(&self, mk: &Marking) -> Vec<usize> {
        let on = self.compiled.enabled(mk);
        (0..self.st.transitions.len())
            .filter(|&i| on.contains(&self.st.transitions[i]))
            .collect()
    }

    fn fire(&self, mk: &Marking, t: usize) -> Result<Marking> {
        self.compiled.fire(&self.st.transitions[t], mk)
    }

    /// Checks a stored model was trained on a net of this shape.
    fn check(&self, m: &Value) -> Result<()> {
        let names = |k: &str| -> Vec<String> {
            m[k].as_array()
                .map(|v| {
                    v.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        };
        if names("places") != self.st.display || names("transitions") != self.st.transitions {
            return Err(Error::Invalid(
                "that model was trained on a net with different places or transitions; train one on this net (rnn.train)".into(),
            ));
        }
        Ok(())
    }
}

fn start(sess: &Session, a: &Value) -> Marking {
    if b(a, "from_current").unwrap_or(false) {
        sess.marking.clone()
    } else {
        sim::initial_marking(&sess.doc)
    }
}

/// One game: the marking before each move, the move, and the enabled moves.
struct Game {
    inputs: Vec<Vec<f64>>,
    moves: Vec<usize>,
    probs: Vec<Vec<f64>>,
    markings: Vec<Marking>,
    end: Marking,
    capped: bool,
}

/// Plays one game. `policy` None picks uniformly among the enabled moves;
/// Some((rnn, sample)) asks the RNN, masked to the enabled moves, and takes
/// the most likely move unless `sample`.
fn game(
    v: &View,
    from: &Marking,
    max_steps: usize,
    policy: Option<(&ElmanRnn, bool)>,
    rng: &mut SimpleRng,
) -> Result<Game> {
    let mut mk = from.clone();
    let mut h = policy
        .map(|(r, _)| vec![0.0; r.hidden_size])
        .unwrap_or_default();
    let mut g = Game {
        inputs: vec![],
        moves: vec![],
        probs: vec![],
        markings: vec![],
        end: Marking::new(),
        capped: false,
    };
    loop {
        let enabled = v.enabled(&mk);
        if enabled.is_empty() {
            break;
        }
        if g.moves.len() >= max_steps {
            g.capped = true;
            break;
        }
        let input = v.input(&mk);
        let (t, probs) = match policy {
            None => {
                let t = enabled[(rng.next_u64() % enabled.len() as u64) as usize];
                (t, vec![])
            }
            Some((rnn, sample)) => {
                let (h2, logits) = rnn.step(&input, &h);
                h = h2;
                let mut masked = vec![f64::NEG_INFINITY; logits.len()];
                for &t in &enabled {
                    masked[t] = logits[t];
                }
                let p = ElmanRnn::softmax(&masked);
                let t = if sample {
                    let r = rng.uniform();
                    let mut acc = 0.0;
                    let mut pick = *enabled.last().expect("non-empty");
                    for &t in &enabled {
                        acc += p[t];
                        if r < acc {
                            pick = t;
                            break;
                        }
                    }
                    pick
                } else {
                    *enabled
                        .iter()
                        .max_by(|&&x, &&y| p[x].total_cmp(&p[y]))
                        .expect("non-empty")
                };
                (t, p)
            }
        };
        g.inputs.push(input);
        g.moves.push(t);
        g.probs.push(probs);
        g.markings.push(mk.clone());
        mk = v.fire(&mk, t)?;
    }
    g.end = mk;
    Ok(g)
}

fn to_json(rnn: &ElmanRnn, v: &View, mode: &str, training: Value) -> Value {
    json!({
        "kind": KIND,
        "mode": mode,
        "places": v.st.display,
        "transitions": v.st.transitions,
        "hidden": rnn.hidden_size,
        "training": training,
        "weights": {
            "w_xh": rnn.w_xh, "w_hh": rnn.w_hh, "b_h": rnn.b_h,
            "w_hy": rnn.w_hy, "b_y": rnn.b_y,
        },
    })
}

fn from_json(m: &Value) -> Result<ElmanRnn> {
    let bad = || Error::Invalid("not an rnn model artifact (rnn.train makes one)".into());
    let w = &m["weights"];
    let mat = |k: &str| -> Result<Vec<Vec<f64>>> {
        serde_json::from_value(w[k].clone()).map_err(|_| bad())
    };
    let vec =
        |k: &str| -> Result<Vec<f64>> { serde_json::from_value(w[k].clone()).map_err(|_| bad()) };
    let (w_xh, w_hh, b_h, w_hy, b_y) = (
        mat("w_xh")?,
        mat("w_hh")?,
        vec("b_h")?,
        mat("w_hy")?,
        vec("b_y")?,
    );
    let hidden_size = b_h.len();
    let output_size = b_y.len();
    let input_size = w_xh.first().map_or(0, |r| r.len());
    if hidden_size == 0
        || w_xh.len() != hidden_size
        || w_hh.len() != hidden_size
        || w_hy.len() != output_size
    {
        return Err(bad());
    }
    Ok(ElmanRnn {
        w_xh,
        w_hh,
        b_h,
        w_hy,
        b_y,
        hidden_size,
        input_size,
        output_size,
    })
}

/// The model artifact `model` names, or the newest one.
fn model(sess: &Session, a: &Value) -> Result<(String, Value)> {
    let is_model = |d: &ArtifactData| matches!(d, ArtifactData::Report(r) if r["kind"] == KIND);
    let art = match s(a, "model") {
        Some(id) => sess
            .artifact(id)
            .ok_or_else(|| Error::NotFound(format!("no artifact {id}")))?,
        None => sess
            .artifacts
            .iter()
            .rev()
            .find(|x| is_model(&x.data))
            .ok_or_else(|| Error::NotFound("no rnn model yet: run rnn.train first".into()))?,
    };
    match &art.data {
        ArtifactData::Report(r) if r["kind"] == KIND => Ok((art.id.clone(), r.clone())),
        _ => Err(Error::Invalid(format!(
            "{} is not an rnn model artifact",
            art.id
        ))),
    }
}

// ================================================================ commands

fn train(sess: &mut Session, a: &Value) -> Result<Value> {
    let hidden = int(a, "hidden", 32, 1, 256)?;
    let games = int(a, "games", 200, 1, 5000)?;
    let epochs = int(a, "epochs", 40, 1, 1000)?;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let lr = rate(a)?;
    let v = View::new(sess)?;
    let from = start(sess, a);

    let mut rng = SimpleRng::new(seed);
    let mut seqs = vec![];
    let (mut moves, mut floor, mut capped) = (0usize, 0.0f64, 0usize);
    for _ in 0..games {
        let g = game(&v, &from, max_steps, None, &mut rng)?;
        if g.moves.is_empty() {
            continue;
        }
        for mk in &g.markings {
            floor += (v.enabled(mk).len() as f64).ln();
        }
        moves += g.moves.len();
        capped += g.capped as usize;
        seqs.push((g.inputs, g.moves));
    }
    if seqs.is_empty() {
        return Err(Error::Invalid(
            "nothing is enabled at the start marking, so there are no games to learn from".into(),
        ));
    }
    // ElmanRnn::train sums the cross-entropy over a game's moves and
    // averages over games, so the floor is on that scale too.
    let floor = floor / seqs.len() as f64;

    let mut rnn = ElmanRnn::new(v.st.places.len(), hidden, v.st.transitions.len(), seed);
    let mut losses = Vec::with_capacity(epochs);
    for _ in 0..epochs {
        losses.push(rnn.train(&seqs, lr, 1));
    }
    let loss = *losses.last().expect("epochs ≥ 1");

    let training = json!({
        "games": seqs.len(), "moves": moves, "capped_games": capped, "epochs": epochs,
        "learning_rate": lr, "max_steps": max_steps, "seed": seed,
        "start": if b(a, "from_current").unwrap_or(false) { "current" } else { "initial" },
        "loss": loss, "floor": floor,
    });
    let curve = sess.add_artifact(
        format!("RNN training loss (hidden {hidden}, seed {seed})"),
        source("rnn.train", a),
        ArtifactData::TimeSeries {
            x_label: "epoch".into(),
            x: (1..=epochs).map(|e| e as f64).collect(),
            series: vec![
                Series::new("loss", losses),
                Series::new("floor (uniform play)", vec![floor; epochs]),
            ],
        },
    );
    let m = sess.add_artifact(
        format!("RNN model (hidden {hidden}, seed {seed})"),
        source("rnn.train", a),
        ArtifactData::Report(to_json(&rnn, &v, "imitation", training.clone())),
    );
    Ok(json!({
        "model": m, "artifact": curve, "artifacts": [curve, m],
        "training": training,
        "note": "loss is the cross-entropy over every transition, summed over a game's moves and averaged over games; the floor \
                 is on the same scale. Imitating uniform random play can at best learn which moves are legal, so expect the \
                 loss to approach the floor, not zero.",
    }))
}

fn play(sess: &mut Session, a: &Value) -> Result<Value> {
    let (mid, m) = model(sess, a)?;
    let v = View::new(sess)?;
    v.check(&m)?;
    let rnn = from_json(&m)?;
    let stochastic = b(a, "stochastic").unwrap_or(false);
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let from = start(sess, a);
    let g = game(
        &v,
        &from,
        max_steps,
        Some((&rnn, stochastic)),
        &mut SimpleRng::new(seed),
    )?;

    let mut steps = vec![TraceStep {
        transition: String::new(),
        marking: from.clone(),
    }];
    let mut moves = vec![];
    for (i, &t) in g.moves.iter().enumerate() {
        let after = g
            .markings
            .get(i + 1)
            .cloned()
            .unwrap_or_else(|| g.end.clone());
        steps.push(TraceStep {
            transition: v.st.transitions[t].clone(),
            marking: after,
        });
        let p = &g.probs[i];
        let mut alt: Vec<usize> = (0..p.len()).filter(|&j| j != t && p[j] > 0.0).collect();
        alt.sort_by(|&x, &y| p[y].total_cmp(&p[x]));
        moves.push(json!({
            "transition": v.st.transitions[t],
            "p": p[t],
            "runners_up": alt.iter().take(3).map(|&j| json!({"transition": v.st.transitions[j], "p": p[j]})).collect::<Vec<_>>(),
        }));
    }
    let end: serde_json::Map<String, Value> = g
        .end
        .iter()
        .filter(|(_, c)| c.iter().any(|&x| x != 0))
        .map(|(k, c)| {
            (
                k.clone(),
                json!(if c.len() == 1 { json!(c[0]) } else { json!(c) }),
            )
        })
        .collect();
    let how = if stochastic {
        format!("sampled, seed {seed}")
    } else {
        "greedy".into()
    };
    let trace = sess.add_artifact(
        format!("RNN game ({mid}, {how})"),
        source("rnn.play", a),
        ArtifactData::Trace { steps },
    );
    let out = json!({
        "model": mid, "trace": trace, "steps": g.moves.len(), "capped": g.capped,
        "end": end, "moves": moves,
        "note": "a learned policy's game: legal by construction (every firing went through the firing rule), not optimal by any proof",
    });
    let report = sess.add_artifact(
        format!("RNN game report ({mid}, {how})"),
        source("rnn.play", a),
        ArtifactData::Report(out.clone()),
    );
    let mut out = out;
    out["report"] = json!(report);
    out["artifacts"] = json!([trace, report]);
    Ok(out)
}

/// `reward` resolved to (index into the input vector, weight).
fn reward(v: &View, a: &Value) -> Result<Vec<(usize, f64)>> {
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
        let idx: Vec<usize> = (0..v.st.places.len())
            .filter(|&i| v.st.display[i] == *k || v.st.base[i] == *k)
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
fn sides(v: &View, a: &Value) -> Result<Option<Vec<f64>>> {
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
        if !v.st.transitions.iter().any(|t| t.starts_with(k.as_str())) {
            return Err(Error::BadArgs(format!(
                "sides names {k}, which no transition id starts with"
            )));
        }
        prefixes.push((k.as_str(), w));
    }
    Ok(Some(
        v.st.transitions
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

fn self_play(sess: &mut Session, a: &Value) -> Result<Value> {
    let (mid, m) = model(sess, a)?;
    let v = View::new(sess)?;
    v.check(&m)?;
    let mut rnn = from_json(&m)?;
    let reward = reward(&v, a)?;
    let sides = sides(&v, a)?;
    let rounds = int(a, "rounds", 20, 1, 500)?;
    let games = int(a, "games", 50, 1, 2000)?;
    let epochs = int(a, "epochs", 1, 1, 100)?;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let lr = rate(a)?;
    let from = start(sess, a);

    let mut rng = SimpleRng::new(seed);
    let (mut mean, mut pos, mut zero, mut neg) = (vec![], vec![], vec![], vec![]);
    let mut loss = 0.0;
    for _ in 0..rounds {
        let mut seqs = Vec::with_capacity(games);
        let mut per_move = Vec::with_capacity(games);
        let (mut total, mut p, mut z, mut q) = (0.0, 0usize, 0usize, 0usize);
        for _ in 0..games {
            let g = game(&v, &from, max_steps, Some((&rnn, true)), &mut rng)?;
            let end = v.input(&g.end);
            let r: f64 = reward.iter().map(|&(i, w)| w * end[i]).sum();
            total += r;
            match r.partial_cmp(&0.0) {
                Some(std::cmp::Ordering::Greater) => p += 1,
                Some(std::cmp::Ordering::Less) => q += 1,
                _ => z += 1,
            }
            if g.moves.is_empty() {
                continue;
            }
            match &sides {
                Some(sign) => {
                    let rs = g.moves.iter().map(|&t| r * sign[t]).collect();
                    per_move.push((g.inputs, g.moves, rs));
                }
                None => seqs.push((g.inputs, g.moves, r)),
            }
        }
        if !per_move.is_empty() {
            loss = rnn.train_weighted_per_move(&per_move, lr, epochs);
        } else if !seqs.is_empty() {
            loss = rnn.train_weighted(&seqs, lr, epochs);
        }
        let k = games as f64;
        mean.push(total / k);
        pos.push(p as f64 / k);
        zero.push(z as f64 / k);
        neg.push(q as f64 / k);
    }

    let mut training = m["training"].clone();
    training["self_play"] = json!({
        "from_model": mid, "reward": a.get("reward"), "sides": a.get("sides"), "rounds": rounds, "games": games,
        "epochs": epochs, "learning_rate": lr, "max_steps": max_steps, "seed": seed, "loss": loss,
        "final": {"mean_reward": mean.last(), "positive": pos.last(), "zero": zero.last(), "negative": neg.last()},
    });
    let curve = sess.add_artifact(
        format!("RNN self-play ({mid}, seed {seed})"),
        source("rnn.self_play", a),
        ArtifactData::TimeSeries {
            x_label: "round".into(),
            x: (1..=rounds).map(|r| r as f64).collect(),
            series: vec![
                Series::new("mean reward", mean.clone()),
                Series::new("share positive", pos.clone()),
                Series::new("share zero", zero.clone()),
                Series::new("share negative", neg.clone()),
            ],
        },
    );
    let out_model = sess.add_artifact(
        format!("RNN model (self-play from {mid}, seed {seed})"),
        source("rnn.self_play", a),
        ArtifactData::Report(to_json(&rnn, &v, "self-play", training)),
    );
    Ok(json!({
        "model": out_model, "artifact": curve, "artifacts": [curve, out_model], "from_model": mid,
        "first": {"mean_reward": mean[0], "positive": pos[0], "zero": zero[0], "negative": neg[0]},
        "last": {"mean_reward": mean.last(), "positive": pos.last(), "zero": zero.last(), "negative": neg.last()},
        "loss": loss,
    }))
}

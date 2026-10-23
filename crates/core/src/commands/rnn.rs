//! Experimental (cargo feature `rnn`, `make build-rnn`): an Elman RNN that
//! learns which transition fires next from the marking: pflow-tropical's
//! `ElmanRnn` (BPTT, REINFORCE-weighted), run by [`crate::rnn_net`], which
//! computes the same thing with flat buffers.
//!
//! Every trajectory and every game goes through the metamodel firing rule
//! (`sim::Compiled`), so colours, inhibitor and read arcs and capacities mean
//! what they mean everywhere else; pflow-tropical never fires anything here.
//! A trained model is a report artifact holding its weights. `rnn.play`
//! stores the game it plays as a trace, which `sim.goto` replays and
//! `provenance.check_witness` re-checks. A learned policy is a guess, not a
//! verdict: these commands prove nothing about the net.

use std::collections::HashMap;
use std::sync::RwLock;

use pflow_tropical::rnn::SimpleRng;
use serde_json::{json, Value};

use crate::analysis::Structure;
use crate::artifact::{ArtifactData, Bar, Series, TraceStep};
use crate::mlp_net::Mlp;
use crate::registry::{always, b, n, nums, opt, s, strs, ArgType::*, CommandSpec};
use crate::rnn_net::{softmax_into, Rnn, Seq};
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
            opt("arch", Str, "rnn (default): an Elman RNN with `hidden` units; mlp: a feed-forward ReLU network over the marking with `layers`"),
            opt("hidden", Int, "rnn hidden units (default 32, 1..256)"),
            opt("layers", NumList, "mlp hidden layer widths, input side first (default [64, 64]; 1..4 layers of 1..512)"),
            opt("games", Int, "random games to learn from (default 200, 1..5000)"),
            opt("epochs", Int, "passes over the games (default 40, 1..1000)"),
            opt("learning_rate", Num, "step size (default 0.01)"),
            opt("batch", Int, "games per weight update (default 16, 1..5000): their gradients are worked out on every core at once and their steps summed, so a large batch takes a large step and can diverge (the loss climbs): lower learning_rate with it; 1 is one update per game"),
            opt("max_steps", Int, "firings per game at most (default 200, 1..10000)"),
            opt("seed", Int, "seed for the games and the initial weights (default 1)"),
            opt("from_current", Bool, "start games from the canvas marking instead of the initial marking"),
            opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled, before anyone chooses (e.g. [\"x_win_\", \"o_win_\"]: a line ends the game when it is made)"),
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
            opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled, before anyone chooses (e.g. [\"x_win_\", \"o_win_\"]: a line ends the game when it is made)"),
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
            opt("batch", Int, "games per weight update (default 16, 1..5000): their gradients are worked out on every core at once and their steps summed, so a large batch takes a large step and can diverge (the loss climbs): lower learning_rate with it; 1 is one update per game"),
            opt("max_steps", Int, "firings per game at most (default 200, 1..10000)"),
            opt("seed", Int, "seed (default 1)"),
            opt("from_current", Bool, "start games from the canvas marking instead of the initial marking"),
            opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled, before anyone chooses (e.g. [\"x_win_\", \"o_win_\"]: a line ends the game when it is made)"),
        ],
        enabled: always,
        run: self_play,
        journal: true,
    },
    CommandSpec {
        id: "rnn.imitate",
        label: "Train RNN from Minimax Teacher (experimental)",
        help: "Experimental. Train an RNN to play like an exact minimax teacher: the teacher searches the whole game (two sides by \
               `sides`, scored by `reward`, automatic transitions by `auto`; small games only, ≤ 200k positions) and names the \
               best move at every position, ties broken by the move that does best against random play. Games follow the \
               teacher, with a random move a share `explore` of the time so the model sees mistakes too. Stores the loss per \
               epoch, the teacher's values for the opening moves, and the model (for rnn.play, rnn.evaluate, rnn.policy).",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("sides", Json, "{transition-id prefix: sign}: who moves, e.g. {\"x_\": 1, \"o_\": -1} (+1 maximizes the reward)"),
            opt("reward", Json, "{place: weight} read from the final marking, from the +1 side's view, e.g. {\"win_x\": 1, \"win_o\": -1}"),
            opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled (e.g. win claims)"),
            opt("arch", Str, "rnn (default): an Elman RNN with `hidden` units; mlp: a feed-forward ReLU network over the marking with `layers`"),
            opt("hidden", Int, "rnn hidden units (default 32, 1..256)"),
            opt("layers", NumList, "mlp hidden layer widths, input side first (default [64, 64]; 1..4 layers of 1..512)"),
            opt("games", Int, "teacher games to learn from (default 300, 1..5000)"),
            opt("epochs", Int, "passes over the games (default 40, 1..1000)"),
            opt("explore", Num, "share of moves played at random (default 0.3, 0..1)"),
            opt("learning_rate", Num, "step size (default 0.01)"),
            opt("batch", Int, "games per weight update (default 16, 1..5000): their gradients are worked out on every core at once and their steps summed, so a large batch takes a large step and can diverge (the loss climbs): lower learning_rate with it; 1 is one update per game"),
            opt("max_steps", Int, "firings per game at most (default 200, 1..10000)"),
            opt("seed", Int, "seed (default 1)"),
        ],
        enabled: always,
        run: imitate,
        journal: true,
    },
    CommandSpec {
        id: "rnn.evaluate",
        label: "Evaluate RNN Policy (experimental)",
        help: "Experimental. Play a model against an opponent from each side of `sides`: `games` games per side, the model greedy \
               unless `stochastic`, the opponent random or the minimax teacher. Stores wins, draws and losses per side as a bar \
               chart (shares of games) and a table.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("model", Str, "rnn model artifact id (default: the newest)"),
            opt("sides", Json, "{transition-id prefix: sign}: who moves, e.g. {\"x_\": 1, \"o_\": -1} (+1 maximizes the reward)"),
            opt("reward", Json, "{place: weight} read from the final marking, from the +1 side's view, e.g. {\"win_x\": 1, \"win_o\": -1}"),
            opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled (e.g. win claims)"),
            opt("opponent", Str, "random (default) or minimax"),
            opt("games", Int, "games per side (default 200, 1..5000)"),
            opt("stochastic", Bool, "sample the model's moves instead of taking the most likely"),
            opt("max_steps", Int, "firings per game at most (default 200, 1..10000)"),
            opt("seed", Int, "seed (default 1)"),
        ],
        enabled: always,
        run: evaluate,
        journal: true,
    },
    CommandSpec {
        id: "rnn.policy",
        label: "Show RNN Policy Here (experimental)",
        help: "Experimental. What a model would play at the canvas marking: its probability for each enabled move, as badges on \
               the canvas (view.highlight) and a bar chart. With `teacher`, the minimax teacher's value of each move instead \
               (+1: the +1 side wins with best play, 0: draw, −1: loses; needs sides and reward). The model sees the marking \
               fresh (hidden state zero), which is exact at the start of a game.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("model", Str, "rnn model artifact id (default: the newest)"),
            opt("teacher", Bool, "show the minimax teacher's move values instead of a model's probabilities"),
            opt("sides", Json, "{transition-id prefix: sign}: who moves, e.g. {\"x_\": 1, \"o_\": -1} (+1 maximizes the reward)"),
            opt("reward", Json, "{place: weight} read from the final marking, from the +1 side's view, e.g. {\"win_x\": 1, \"win_o\": -1}"),
            opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled (e.g. win claims)"),
        ],
        enabled: always,
        run: policy,
        journal: false,
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

fn batch(a: &Value) -> Result<usize> {
    int(a, "batch", 16, 1, 5000)
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
///
/// Games revisit the same positions over and over, so the firing rule's
/// answers are remembered per position (up to [`View::CACHE`] each): what
/// is enabled at a marking, and what firing a transition there gives. The
/// answers still come from `sim::Compiled`; they are just not recomputed.
/// The caches are shared by the threads playing games at once.
struct View {
    compiled: Compiled,
    st: Structure,
    enabled: RwLock<HashMap<Vec<i64>, Vec<usize>>>,
    fired: RwLock<HashMap<(Vec<i64>, usize), Marking>>,
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
            enabled: RwLock::default(),
            fired: RwLock::default(),
        })
    }

    const CACHE: usize = 200_000;

    fn input(&self, mk: &Marking) -> Vec<f64> {
        self.st.marking(mk).into_iter().map(|v| v as f64).collect()
    }

    fn enabled(&self, mk: &Marking) -> Vec<usize> {
        let key = self.st.marking(mk);
        if let Some(en) = self.enabled.read().expect("cache").get(&key) {
            return en.clone();
        }
        let on = self.compiled.enabled(mk);
        let en: Vec<usize> = (0..self.st.transitions.len())
            .filter(|&i| on.contains(&self.st.transitions[i]))
            .collect();
        let mut cache = self.enabled.write().expect("cache");
        if cache.len() < Self::CACHE {
            cache.insert(key, en.clone());
        }
        en
    }

    fn fire(&self, mk: &Marking, t: usize) -> Result<Marking> {
        let key = (self.st.marking(mk), t);
        if let Some(next) = self.fired.read().expect("cache").get(&key) {
            return Ok(next.clone());
        }
        let next = self.compiled.fire(&self.st.transitions[t], mk)?;
        let mut cache = self.fired.write().expect("cache");
        if cache.len() < Self::CACHE {
            cache.insert(key, next.clone());
        }
        Ok(next)
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

/// One game: the input, choice and policy probabilities at each chosen
/// move, every firing (automatic ones too) with the marking after it, and
/// the end.
struct Game {
    inputs: Vec<Vec<f64>>,
    moves: Vec<usize>,
    probs: Vec<Vec<f64>>,
    markings: Vec<Marking>,
    /// (transition, automatic, marking after), in firing order.
    fired: Vec<(usize, bool, Marking)>,
    end: Marking,
    capped: bool,
}

/// Picks a move: given the marking, its input vector and the enabled
/// moves (never automatic ones), the transition to fire and the policy's
/// probabilities over every transition (empty when not an RNN).
type Chooser<'a> =
    dyn FnMut(&Marking, &[f64], &[usize], &mut SimpleRng) -> Result<(usize, Vec<f64>)> + 'a;

/// Plays one game from `from`. A transition marked `auto` fires as soon as
/// it is enabled, before anyone chooses (a win claim ends the game the
/// moment the line is made); otherwise `choose` picks among the enabled
/// moves. Stops when nothing is enabled or after `max_steps` firings.
fn game(
    v: &View,
    from: &Marking,
    max_steps: usize,
    auto: &[bool],
    rng: &mut SimpleRng,
    choose: &mut Chooser,
) -> Result<Game> {
    let mut mk = from.clone();
    let mut g = Game {
        inputs: vec![],
        moves: vec![],
        probs: vec![],
        markings: vec![],
        fired: vec![],
        end: Marking::new(),
        capped: false,
    };
    loop {
        let enabled = v.enabled(&mk);
        if enabled.is_empty() {
            break;
        }
        if g.fired.len() >= max_steps {
            g.capped = true;
            break;
        }
        if let Some(&t) = enabled.iter().find(|&&t| auto[t]) {
            mk = v.fire(&mk, t)?;
            g.fired.push((t, true, mk.clone()));
            continue;
        }
        let input = v.input(&mk);
        let (t, probs) = choose(&mk, &input, &enabled, rng)?;
        g.inputs.push(input);
        g.moves.push(t);
        g.probs.push(probs);
        g.markings.push(mk.clone());
        mk = v.fire(&mk, t)?;
        g.fired.push((t, false, mk.clone()));
    }
    g.end = mk;
    Ok(g)
}

/// Game `i` of `stream` (a round, a side)'s own random numbers under
/// `seed`, so games can run on any core in any order with the same result.
fn game_rng(seed: u64, stream: u64, i: usize) -> SimpleRng {
    SimpleRng::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (stream << 32) ^ i as u64)
}

/// `f(i)` for every game `i < n`, on every core, in game order; the first
/// error in game order is the one returned.
fn each_game<T: Send>(n: usize, f: impl Fn(usize) -> Result<T> + Sync + Send) -> Result<Vec<T>> {
    crate::par::map((0..n).collect(), f).into_iter().collect()
}

fn uniform(enabled: &[usize], rng: &mut SimpleRng) -> usize {
    enabled[(rng.next_u64() % enabled.len() as u64) as usize]
}

/// A trained policy: the Elman RNN (state carried from move to move) or
/// the MLP (none: the marking is the whole state).
enum Policy {
    Rnn(Rnn),
    Mlp(Mlp),
}

impl Policy {
    /// Builds an untrained one from `arch`, `hidden` and `layers`.
    fn new(v: &View, a: &Value, seed: u64) -> Result<Self> {
        let (inp, out) = (v.st.places.len(), v.st.transitions.len());
        match s(a, "arch").unwrap_or("rnn") {
            "rnn" => {
                if a.get("layers").is_some_and(|x| !x.is_null()) {
                    return Err(Error::BadArgs(
                        "layers is for arch mlp; an rnn has `hidden`".into(),
                    ));
                }
                Ok(Self::Rnn(Rnn::new(
                    inp,
                    int(a, "hidden", 32, 1, 256)?,
                    out,
                    seed,
                )))
            }
            "mlp" => {
                if a.get("hidden").is_some_and(|x| !x.is_null()) {
                    return Err(Error::BadArgs(
                        "hidden is for arch rnn; an mlp has `layers`".into(),
                    ));
                }
                let widths = nums(a, "layers").unwrap_or_else(|| vec![64.0, 64.0]);
                if widths.is_empty()
                    || widths.len() > 4
                    || widths
                        .iter()
                        .any(|&w| w.fract() != 0.0 || !(1.0..=512.0).contains(&w))
                {
                    return Err(Error::BadArgs(format!(
                        "layers must be 1..4 integer widths in 1..512, got {widths:?}"
                    )));
                }
                let mut sizes = vec![inp];
                sizes.extend(widths.iter().map(|&w| w as usize));
                sizes.push(out);
                Ok(Self::Mlp(Mlp::new(sizes, seed)))
            }
            other => Err(Error::BadArgs(format!(
                "arch must be rnn or mlp, got {other}"
            ))),
        }
    }

    /// "RNN" or "MLP", for titles.
    fn name(&self) -> &'static str {
        match self {
            Self::Rnn(_) => "RNN",
            Self::Mlp(_) => "MLP",
        }
    }

    /// Its shape, for titles: "hidden 32" or "layers 64-64".
    fn shape(&self) -> String {
        match self {
            Self::Rnn(r) => format!("hidden {}", r.hs),
            Self::Mlp(m) => {
                let w: Vec<String> = m.sizes[1..m.sizes.len() - 1]
                    .iter()
                    .map(|n| n.to_string())
                    .collect();
                format!("layers {}", w.join("-"))
            }
        }
    }

    /// The state a game starts from (empty for the MLP).
    fn state(&self) -> Vec<f64> {
        match self {
            Self::Rnn(r) => vec![0.0; r.hs],
            Self::Mlp(_) => vec![],
        }
    }

    /// The logits at `input`, advancing the RNN's state `h`.
    fn logits(&self, input: &[f64], h: &mut Vec<f64>) -> Vec<f64> {
        match self {
            Self::Rnn(r) => {
                let mut logits = vec![0.0; r.out];
                r.step(input, h, &mut logits);
                logits
            }
            Self::Mlp(m) => m.logits(input),
        }
    }

    fn train(&mut self, seqs: &[Seq], lr: f64, epochs: usize, batch: usize) -> f64 {
        match self {
            Self::Rnn(r) => r.train(seqs, lr, epochs, batch),
            Self::Mlp(m) => m.train(seqs, lr, epochs, batch),
        }
    }
}

/// The policy's next step: updates `h` and returns its probabilities over
/// every transition, masked to `enabled`.
fn rnn_probs(rnn: &Policy, h: &mut Vec<f64>, input: &[f64], enabled: &[usize]) -> Vec<f64> {
    let mut logits = rnn.logits(input, h);
    let mut masked = vec![f64::NEG_INFINITY; logits.len()];
    for &t in enabled {
        masked[t] = logits[t];
    }
    softmax_into(&masked, &mut logits);
    logits
}

/// The most likely enabled move, or one sampled from `p`.
fn pick(p: &[f64], enabled: &[usize], sample: bool, rng: &mut SimpleRng) -> usize {
    if sample {
        let r = rng.uniform();
        let mut acc = 0.0;
        for &t in enabled {
            acc += p[t];
            if r < acc {
                return t;
            }
        }
        *enabled.last().expect("non-empty")
    } else {
        *enabled
            .iter()
            .max_by(|&&x, &&y| p[x].total_cmp(&p[y]))
            .expect("non-empty")
    }
}

/// A chooser that asks the policy at every move.
fn rnn_chooser<'a>(rnn: &'a Policy, sample: bool) -> Box<Chooser<'a>> {
    let mut h = rnn.state();
    Box::new(move |_, input, enabled, rng| {
        let p = rnn_probs(rnn, &mut h, input, enabled);
        Ok((pick(&p, enabled, sample, rng), p))
    })
}

/// `auto` resolved to a flag per transition (prefixes of transition ids).
fn auto(v: &View, a: &Value) -> Result<Vec<bool>> {
    let prefixes = strs(a, "auto").unwrap_or_default();
    for p in &prefixes {
        if !v.st.transitions.iter().any(|t| t.starts_with(p.as_str())) {
            return Err(Error::BadArgs(format!(
                "auto names {p}, which no transition id starts with"
            )));
        }
    }
    Ok(v.st
        .transitions
        .iter()
        .map(|t| prefixes.iter().any(|p| t.starts_with(p.as_str())))
        .collect())
}

/// The side to move: the sign every enabled move shares.
fn mover(v: &View, sign: &[f64], enabled: &[usize]) -> Result<f64> {
    let s = sign[enabled[0]];
    if s == 0.0 || enabled.iter().any(|&t| sign[t] != s) {
        let names: Vec<&str> = enabled
            .iter()
            .map(|&t| v.st.transitions[t].as_str())
            .collect();
        return Err(Error::Invalid(format!(
            "the enabled moves {names:?} do not all belong to one side of `sides`; give every transition a side, and mark \
             transitions nobody chooses (e.g. win claims) `auto`"
        )));
    }
    Ok(s)
}

/// Exact minimax over the token game, for two sides that alternate by the
/// net's own rules: the side with sign +1 maximizes the reward, −1
/// minimizes it. Values are memoized per marking; `cap` bounds the search.
struct Teacher<'a> {
    v: &'a View,
    auto: &'a [bool],
    sign: &'a [f64],
    reward: &'a [(usize, f64)],
    value: HashMap<Vec<i64>, f64>,
    random: HashMap<Vec<i64>, f64>,
    cap: usize,
}

impl<'a> Teacher<'a> {
    const CAP: usize = 200_000;

    fn new(v: &'a View, auto: &'a [bool], sign: &'a [f64], reward: &'a [(usize, f64)]) -> Self {
        Self {
            v,
            auto,
            sign,
            reward,
            value: HashMap::new(),
            random: HashMap::new(),
            cap: Self::CAP,
        }
    }

    /// Fires automatic transitions until none is enabled.
    fn settle(&self, mk: &Marking) -> Result<Marking> {
        let mut mk = mk.clone();
        for _ in 0..10_000 {
            match self.v.enabled(&mk).into_iter().find(|&t| self.auto[t]) {
                Some(t) => mk = self.v.fire(&mk, t)?,
                None => return Ok(mk),
            }
        }
        Err(Error::Invalid(
            "automatic transitions never stop firing".into(),
        ))
    }

    fn score(&self, mk: &Marking) -> f64 {
        let x = self.v.input(mk);
        self.reward.iter().map(|&(i, w)| w * x[i]).sum()
    }

    fn check_cap(&self) -> Result<()> {
        if self.value.len() + self.random.len() > self.cap {
            return Err(Error::Invalid(format!(
                "the game has more than {} positions; the minimax teacher is for small games like tic-tac-toe",
                self.cap
            )));
        }
        Ok(())
    }

    /// The game's value from `mk` with both sides playing perfectly.
    fn value(&mut self, mk: &Marking) -> Result<f64> {
        let mk = self.settle(mk)?;
        let key = self.v.st.marking(&mk);
        if let Some(&x) = self.value.get(&key) {
            return Ok(x);
        }
        self.check_cap()?;
        let en = self.v.enabled(&mk);
        let x = if en.is_empty() {
            self.score(&mk)
        } else {
            let s = mover(self.v, self.sign, &en)?;
            let mut best = f64::NEG_INFINITY;
            for &t in &en {
                best = best.max(s * self.value(&self.v.fire(&mk, t)?)?);
            }
            s * best
        };
        self.value.insert(key, x);
        Ok(x)
    }

    /// The expected reward from `mk` when both sides play uniformly at
    /// random: the tie-break between moves of equal value.
    fn random(&mut self, mk: &Marking) -> Result<f64> {
        let mk = self.settle(mk)?;
        let key = self.v.st.marking(&mk);
        if let Some(&x) = self.random.get(&key) {
            return Ok(x);
        }
        self.check_cap()?;
        let en = self.v.enabled(&mk);
        let x = if en.is_empty() {
            self.score(&mk)
        } else {
            let mut sum = 0.0;
            for &t in &en {
                sum += self.random(&self.v.fire(&mk, t)?)?;
            }
            sum / en.len() as f64
        };
        self.random.insert(key, x);
        Ok(x)
    }

    /// Solves every position reachable from `from`, so the games that
    /// follow can ask [`Teacher::advise_solved`] from any thread.
    fn solve(&mut self, from: &Marking) -> Result<()> {
        self.value(from)?;
        self.random(from)?;
        Ok(())
    }

    /// Each enabled move's (perfect-play value, random-play value) from the
    /// mover's side, and the best move: highest value, ties broken by the
    /// random-play value.
    fn advise(&mut self, mk: &Marking, en: &[usize]) -> Result<(Vec<(f64, f64)>, usize)> {
        let s = mover(self.v, self.sign, en)?;
        let mut vals = Vec::with_capacity(en.len());
        for &t in en {
            let next = self.v.fire(mk, t)?;
            vals.push((s * self.value(&next)?, s * self.random(&next)?));
        }
        Ok(best_move(en, vals))
    }

    /// [`Teacher::advise`] in a position [`Teacher::solve`] reached.
    fn advise_solved(&self, mk: &Marking, en: &[usize]) -> Result<(Vec<(f64, f64)>, usize)> {
        let s = mover(self.v, self.sign, en)?;
        let known = |memo: &HashMap<Vec<i64>, f64>, mk: &Marking| -> Result<f64> {
            let key = self.v.st.marking(&self.settle(mk)?);
            memo.get(&key)
                .copied()
                .ok_or_else(|| Error::Invalid("the teacher has not solved this position".into()))
        };
        let mut vals = Vec::with_capacity(en.len());
        for &t in en {
            let next = self.v.fire(mk, t)?;
            vals.push((
                s * known(&self.value, &next)?,
                s * known(&self.random, &next)?,
            ));
        }
        Ok(best_move(en, vals))
    }
}

/// The move with the highest value, ties broken by the random-play value.
fn best_move(en: &[usize], vals: Vec<(f64, f64)>) -> (Vec<(f64, f64)>, usize) {
    let best = (0..en.len())
        .max_by(|&i, &j| {
            vals[i]
                .0
                .total_cmp(&vals[j].0)
                .then(vals[i].1.total_cmp(&vals[j].1))
        })
        .expect("non-empty");
    (vals, en[best])
}

fn to_json(net: &Policy, v: &View, mode: &str, training: Value) -> Value {
    let mut out = json!({
        "kind": KIND,
        "mode": mode,
        "places": v.st.display,
        "transitions": v.st.transitions,
        "training": training,
    });
    match net {
        Policy::Rnn(rnn) => {
            let [w_xh, w_hh, w_hy] = rnn.rows();
            out["arch"] = json!("rnn");
            out["hidden"] = json!(rnn.hs);
            out["weights"] = json!({
                "w_xh": w_xh, "w_hh": w_hh, "b_h": rnn.b_h,
                "w_hy": w_hy, "b_y": rnn.b_y,
            });
        }
        Policy::Mlp(mlp) => {
            out["arch"] = json!("mlp");
            out["layers"] = json!(mlp.sizes[1..mlp.sizes.len() - 1]);
            let layers: Vec<Value> = (1..=mlp.depth())
                .map(|l| {
                    let (w, b) = mlp.layer(l);
                    let rows: Vec<&[f64]> = w.chunks(mlp.sizes[l - 1]).collect();
                    json!({ "w": rows, "b": b })
                })
                .collect();
            out["weights"] = json!({ "layers": layers });
        }
    }
    out
}

fn from_json(m: &Value, v: &View) -> Result<Policy> {
    let net = parse_model(m)?;
    let (inp, out) = match &net {
        Policy::Rnn(r) => (r.inp, r.out),
        Policy::Mlp(p) => (p.sizes[0], *p.sizes.last().expect("sizes")),
    };
    if inp != v.st.places.len() || out != v.st.transitions.len() {
        return Err(Error::Invalid(
            "that model's weights do not fit this net's places and transitions".into(),
        ));
    }
    Ok(net)
}

fn parse_model(m: &Value) -> Result<Policy> {
    match m["arch"].as_str().unwrap_or("rnn") {
        "rnn" => rnn_from_json(m).map(Policy::Rnn),
        "mlp" => {
            let bad = || Error::Invalid("not an mlp model artifact (rnn.train makes one)".into());
            let layers: Vec<(Vec<Vec<f64>>, Vec<f64>)> = m["weights"]["layers"]
                .as_array()
                .ok_or_else(bad)?
                .iter()
                .map(|l| {
                    Ok((
                        serde_json::from_value(l["w"].clone()).map_err(|_| bad())?,
                        serde_json::from_value(l["b"].clone()).map_err(|_| bad())?,
                    ))
                })
                .collect::<Result<_>>()?;
            Mlp::from_layers(&layers).map(Policy::Mlp).ok_or_else(bad)
        }
        other => Err(Error::Invalid(format!("unknown model arch {other}"))),
    }
}

fn rnn_from_json(m: &Value) -> Result<Rnn> {
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
        || w_xh.iter().any(|r| r.len() != input_size)
        || w_hh.iter().chain(&w_hy).any(|r| r.len() != hidden_size)
    {
        return Err(bad());
    }
    let flat = |m: Vec<Vec<f64>>| m.into_iter().flatten().collect();
    Ok(Rnn {
        inp: input_size,
        hs: hidden_size,
        out: output_size,
        w_xh: flat(w_xh),
        w_hh: flat(w_hh),
        b_h,
        w_hy: flat(w_hy),
        b_y,
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
    let games = int(a, "games", 200, 1, 5000)?;
    let epochs = int(a, "epochs", 40, 1, 1000)?;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let lr = rate(a)?;
    let batch = batch(a)?;
    let v = View::new(sess)?;
    let auto = auto(&v, a)?;
    let from = start(sess, a);

    let played = each_game(games, |i| {
        let mut random = |_: &Marking, _: &[f64], en: &[usize], rng: &mut SimpleRng| {
            Ok((uniform(en, rng), vec![]))
        };
        let g = game(
            &v,
            &from,
            max_steps,
            &auto,
            &mut game_rng(seed, 0, i),
            &mut random,
        )?;
        let floor: f64 = g
            .markings
            .iter()
            .map(|mk| (v.enabled(mk).len() as f64).ln())
            .sum();
        Ok((g, floor))
    })?;
    let mut seqs = vec![];
    let (mut moves, mut floor, mut capped) = (0usize, 0.0f64, 0usize);
    for (g, f) in played {
        if g.moves.is_empty() {
            continue;
        }
        floor += f;
        moves += g.moves.len();
        capped += g.capped as usize;
        seqs.push(Seq::plain(g.inputs, g.moves));
    }
    if seqs.is_empty() {
        return Err(Error::Invalid(
            "nothing is enabled at the start marking, so there are no games to learn from".into(),
        ));
    }
    // Training sums the cross-entropy over a game's moves and
    // averages over games, so the floor is on that scale too.
    let floor = floor / seqs.len() as f64;

    let mut rnn = Policy::new(&v, a, seed)?;
    let (name, shape) = (rnn.name(), rnn.shape());
    let mut losses = Vec::with_capacity(epochs);
    for _ in 0..epochs {
        losses.push(rnn.train(&seqs, lr, 1, batch));
    }
    let loss = *losses.last().expect("epochs ≥ 1");

    let training = json!({
        "games": seqs.len(), "moves": moves, "capped_games": capped, "epochs": epochs,
        "learning_rate": lr, "batch": batch, "max_steps": max_steps, "seed": seed,
        "start": if b(a, "from_current").unwrap_or(false) { "current" } else { "initial" },
        "loss": loss, "floor": floor,
    });
    let curve = sess.add_artifact(
        format!("{name} training loss ({shape}, seed {seed})"),
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
        format!("{name} model ({shape}, seed {seed})"),
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
    let rnn = from_json(&m, &v)?;
    let stochastic = b(a, "stochastic").unwrap_or(false);
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let auto = auto(&v, a)?;
    let from = start(sess, a);
    let g = game(
        &v,
        &from,
        max_steps,
        &auto,
        &mut SimpleRng::new(seed),
        &mut *rnn_chooser(&rnn, stochastic),
    )?;

    let mut steps = vec![TraceStep {
        transition: String::new(),
        marking: from.clone(),
    }];
    for (t, _, after) in &g.fired {
        steps.push(TraceStep {
            transition: v.st.transitions[*t].clone(),
            marking: after.clone(),
        });
    }
    let mut moves = vec![];
    for (i, &t) in g.moves.iter().enumerate() {
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
        format!("{} game ({mid}, {how})", rnn.name()),
        source("rnn.play", a),
        ArtifactData::Trace { steps },
    );
    let out = json!({
        "model": mid, "trace": trace, "steps": g.fired.len(), "moves_chosen": g.moves.len(), "capped": g.capped,
        "end": end, "moves": moves,
        "note": "a learned policy's game: legal by construction (every firing went through the firing rule), not optimal by any proof",
    });
    let report = sess.add_artifact(
        format!("{} game report ({mid}, {how})", rnn.name()),
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
    let mut rnn = from_json(&m, &v)?;
    let reward = reward(&v, a)?;
    let sides = sides(&v, a)?;
    let auto = auto(&v, a)?;
    let rounds = int(a, "rounds", 20, 1, 500)?;
    let games = int(a, "games", 50, 1, 2000)?;
    let epochs = int(a, "epochs", 1, 1, 100)?;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let lr = rate(a)?;
    let batch = batch(a)?;
    let from = start(sess, a);

    let (mut mean, mut pos, mut zero, mut neg) = (vec![], vec![], vec![], vec![]);
    let mut loss = 0.0;
    for round in 0..rounds {
        let rnn_now = &rnn;
        let played_games = each_game(games, |i| {
            game(
                &v,
                &from,
                max_steps,
                &auto,
                &mut game_rng(seed, round as u64, i),
                &mut *rnn_chooser(rnn_now, true),
            )
        })?;
        let mut seqs = Vec::with_capacity(games);
        let mut per_move = Vec::with_capacity(games);
        let mut played = 0usize;
        let (mut total, mut p, mut z, mut q) = (0.0, 0usize, 0usize, 0usize);
        for g in played_games {
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
            played += 1;
            match &sides {
                Some(sign) => {
                    let weights = g.moves.iter().map(|&t| r * sign[t]).collect();
                    per_move.push(Seq {
                        inputs: g.inputs,
                        targets: g.moves,
                        weights,
                    });
                }
                // REINFORCE skips games with no reward (and does not count them).
                None if r.abs() < 1e-10 => {}
                None => seqs.push(Seq {
                    weights: vec![r; g.moves.len()],
                    inputs: g.inputs,
                    targets: g.moves,
                }),
            }
        }
        if !per_move.is_empty() {
            loss = rnn.train(&per_move, lr, epochs, batch);
        } else if sides.is_none() && played > 0 {
            loss = rnn.train(&seqs, lr, epochs, batch);
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
        "epochs": epochs, "learning_rate": lr, "batch": batch, "max_steps": max_steps, "seed": seed, "loss": loss,
        "final": {"mean_reward": mean.last(), "positive": pos.last(), "zero": zero.last(), "negative": neg.last()},
    });
    let curve = sess.add_artifact(
        format!("{} self-play ({mid}, seed {seed})", rnn.name()),
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
        format!("{} model (self-play from {mid}, seed {seed})", rnn.name()),
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

/// The rules of a two-sided game: each transition's side, the reward
/// read from the final marking, and the automatic transitions.
struct Rules {
    sign: Vec<f64>,
    reward: Vec<(usize, f64)>,
    auto: Vec<bool>,
}

fn game_rules(v: &View, a: &Value) -> Result<Rules> {
    let sign = sides(v, a)?.ok_or_else(|| {
        Error::BadArgs(
            "sides is required: {transition-id prefix: sign}, e.g. {\"x_\": 1, \"o_\": -1}".into(),
        )
    })?;
    Ok(Rules {
        sign,
        reward: reward(v, a)?,
        auto: auto(v, a)?,
    })
}

fn imitate(sess: &mut Session, a: &Value) -> Result<Value> {
    let games = int(a, "games", 300, 1, 5000)?;
    let epochs = int(a, "epochs", 40, 1, 1000)?;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let lr = rate(a)?;
    let batch = batch(a)?;
    let explore = match n(a, "explore") {
        None => 0.3,
        Some(x) if (0.0..=1.0).contains(&x) => x,
        Some(x) => return Err(Error::BadArgs(format!("explore must be in 0..1, got {x}"))),
    };
    let v = View::new(sess)?;
    let mut rnn = Policy::new(&v, a, seed)?;
    let (name, shape) = (rnn.name(), rnn.shape());
    let Rules { sign, reward, auto } = game_rules(&v, a)?;
    let from = sim::initial_marking(&sess.doc);
    let mut teacher = Teacher::new(&v, &auto, &sign, &reward);
    teacher.solve(&from)?;
    let teacher = &teacher;

    let played = each_game(games, |i| {
        let mut labels = vec![];
        let mut follow = |mk: &Marking, _: &[f64], en: &[usize], rng: &mut SimpleRng| {
            let (_, best) = teacher.advise_solved(mk, en)?;
            labels.push(best);
            let t = if rng.uniform() < explore {
                uniform(en, rng)
            } else {
                best
            };
            Ok((t, vec![]))
        };
        let g = game(
            &v,
            &from,
            max_steps,
            &auto,
            &mut game_rng(seed, 0, i),
            &mut follow,
        )?;
        Ok((g, labels))
    })?;
    let mut seqs = vec![];
    let mut moves = 0usize;
    for (g, labels) in played {
        if !g.inputs.is_empty() {
            moves += labels.len();
            seqs.push(Seq::plain(g.inputs, labels));
        }
    }
    if seqs.is_empty() {
        return Err(Error::Invalid(
            "nothing to choose at the start marking".into(),
        ));
    }
    // What the teacher thinks of the opening.
    let start = teacher.settle(&from)?;
    let en = v.enabled(&start);
    let (vals, best) = teacher.advise_solved(&start, &en)?;
    let opening: Vec<Value> = en
        .iter()
        .zip(&vals)
        .map(|(&t, &(value, random))| json!({ "transition": v.st.transitions[t], "value": value, "vs_random": random }))
        .collect();
    let positions = teacher.value.len();

    let mut losses = Vec::with_capacity(epochs);
    for _ in 0..epochs {
        losses.push(rnn.train(&seqs, lr, 1, batch));
    }
    let training = json!({
        "teacher": "minimax", "games": seqs.len(), "moves": moves, "epochs": epochs, "explore": explore,
        "learning_rate": lr, "batch": batch, "max_steps": max_steps, "seed": seed, "loss": losses.last(),
        "sides": a.get("sides"), "reward": a.get("reward"), "auto": a.get("auto"),
        "positions_solved": positions,
    });
    let curve = sess.add_artifact(
        format!("{name} imitation loss ({shape}, seed {seed})"),
        source("rnn.imitate", a),
        ArtifactData::TimeSeries {
            x_label: "epoch".into(),
            x: (1..=epochs).map(|e| e as f64).collect(),
            series: vec![Series::new("loss", losses)],
        },
    );
    let m = sess.add_artifact(
        format!("{name} model (minimax teacher, {shape}, seed {seed})"),
        source("rnn.imitate", a),
        ArtifactData::Report(to_json(&rnn, &v, "minimax-imitation", training.clone())),
    );
    Ok(json!({
        "model": m, "artifact": curve, "artifacts": [curve, m], "training": training,
        "teacher_opening": { "best": v.st.transitions[best], "moves": opening },
    }))
}

fn evaluate(sess: &mut Session, a: &Value) -> Result<Value> {
    let (mid, m) = model(sess, a)?;
    let v = View::new(sess)?;
    v.check(&m)?;
    let rnn = from_json(&m, &v)?;
    let Rules { sign, reward, auto } = game_rules(&v, a)?;
    let games = int(a, "games", 200, 1, 5000)?;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let stochastic = b(a, "stochastic").unwrap_or(false);
    let opponent = s(a, "opponent").unwrap_or("random");
    if !matches!(opponent, "random" | "minimax") {
        return Err(Error::BadArgs(format!(
            "opponent must be random or minimax, got {opponent}"
        )));
    }
    let from = sim::initial_marking(&sess.doc);
    let mut teacher = Teacher::new(&v, &auto, &sign, &reward);
    let prefixes: Vec<(String, f64)> = match a.get("sides") {
        Some(Value::Object(o)) => o
            .iter()
            .map(|(k, w)| (k.clone(), w.as_f64().unwrap_or(0.0)))
            .collect(),
        Some(Value::String(t)) => serde_json::from_str::<serde_json::Map<String, Value>>(t)
            .map(|o| {
                o.iter()
                    .map(|(k, w)| (k.clone(), w.as_f64().unwrap_or(0.0)))
                    .collect()
            })
            .unwrap_or_default(),
        _ => vec![],
    };
    if opponent == "minimax" {
        teacher.solve(&from)?;
    }
    let teacher = &teacher;
    let (mut bars, mut rows) = (vec![], vec![]);
    for (k, (side, seat)) in prefixes.iter().enumerate() {
        let rewards = each_game(games, |i| {
            let mut h = rnn.state();
            let mut choose = |mk: &Marking, input: &[f64], en: &[usize], rng: &mut SimpleRng| {
                // The model's hidden state follows every move, both sides'.
                let p = rnn_probs(&rnn, &mut h, input, en);
                if mover(&v, &sign, en)? == *seat {
                    return Ok((pick(&p, en, stochastic, rng), vec![]));
                }
                let t = match opponent {
                    "minimax" => teacher.advise_solved(mk, en)?.1,
                    _ => uniform(en, rng),
                };
                Ok((t, vec![]))
            };
            let g = game(
                &v,
                &from,
                max_steps,
                &auto,
                &mut game_rng(seed, k as u64, i),
                &mut choose,
            )?;
            let x = v.input(&g.end);
            Ok(seat * reward.iter().map(|&(i, wt)| wt * x[i]).sum::<f64>())
        })?;
        let (mut w, mut d, mut l) = (0usize, 0usize, 0usize);
        for r in rewards {
            match r.partial_cmp(&0.0) {
                Some(std::cmp::Ordering::Greater) => w += 1,
                Some(std::cmp::Ordering::Less) => l += 1,
                _ => d += 1,
            }
        }
        let k = games as f64;
        for (what, c) in [("win", w), ("draw", d), ("loss", l)] {
            bars.push(Bar {
                name: format!("as {side} · {what}"),
                value: c as f64 / k,
                lo: None,
                hi: None,
            });
        }
        rows.push(vec![
            json!(side),
            json!(w),
            json!(d),
            json!(l),
            json!(w as f64 / k),
        ]);
    }
    let how = if stochastic { "sampled" } else { "greedy" };
    let chart = sess.add_artifact(
        format!("{} vs {opponent} ({mid}, {how}, seed {seed})", rnn.name()),
        source("rnn.evaluate", a),
        ArtifactData::Bars {
            value_label: "share of games".into(),
            bars,
        },
    );
    let table = sess.add_artifact(
        format!(
            "{} vs {opponent} table ({mid}, {how}, seed {seed})",
            rnn.name()
        ),
        source("rnn.evaluate", a),
        ArtifactData::Table {
            columns: ["side", "wins", "draws", "losses", "win share"]
                .map(String::from)
                .to_vec(),
            rows: rows.clone(),
        },
    );
    Ok(json!({
        "model": mid, "opponent": opponent, "games_per_side": games, "artifact": chart, "artifacts": [chart, table],
        "results": rows.iter().map(|r| json!({"side": r[0], "wins": r[1], "draws": r[2], "losses": r[3]})).collect::<Vec<_>>(),
    }))
}

fn policy(sess: &mut Session, a: &Value) -> Result<Value> {
    let v = View::new(sess)?;
    let teacher_mode = b(a, "teacher").unwrap_or(false);
    let mk = sess.marking.clone();
    let (weights, title, label, mid): (Vec<(usize, f64)>, String, String, Option<String>) =
        if teacher_mode {
            let Rules { sign, reward, auto } = game_rules(&v, a)?;
            let mut t = Teacher::new(&v, &auto, &sign, &reward);
            let here = t.settle(&mk)?;
            let en = v.enabled(&here);
            if en.is_empty() {
                return Err(Error::Invalid("the game is over at this marking".into()));
            }
            let (vals, _) = t.advise(&here, &en)?;
            let s = mover(&v, &sign, &en)?;
            (
                en.iter()
                    .zip(&vals)
                    .map(|(&t, &(val, _))| (t, s * val))
                    .collect(),
                "Minimax value per move".into(),
                "minimax value per move (+1: the +1 side wins with best play)".into(),
                None,
            )
        } else {
            let (mid, m) = model(sess, a)?;
            v.check(&m)?;
            let rnn = from_json(&m, &v)?;
            let auto = auto(&v, a)?;
            let en: Vec<usize> = v.enabled(&mk).into_iter().filter(|&t| !auto[t]).collect();
            if en.is_empty() {
                return Err(Error::Invalid("no move to choose at this marking".into()));
            }
            let mut h = rnn.state();
            let p = rnn_probs(&rnn, &mut h, &v.input(&mk), &en);
            (
                en.iter().map(|&t| (t, p[t])).collect(),
                format!("{} policy here ({mid})", rnn.name()),
                format!("{} policy ({mid}): probability per move", rnn.name()),
                Some(mid),
            )
        };
    let mut sorted = weights.clone();
    sorted.sort_by(|x, y| y.1.total_cmp(&x.1));
    let chart = sess.add_artifact(
        title,
        source("rnn.policy", a),
        ArtifactData::Bars {
            value_label: if teacher_mode {
                "value".into()
            } else {
                "probability".into()
            },
            bars: sorted
                .iter()
                .map(|&(t, w)| Bar {
                    name: v.st.transitions[t].clone(),
                    value: w,
                    lo: None,
                    hi: None,
                })
                .collect(),
        },
    );
    let badges: serde_json::Map<String, Value> = weights
        .iter()
        .map(|&(t, w)| {
            (
                v.st.transitions[t].clone(),
                json!((w * 100.0).round() / 100.0),
            )
        })
        .collect();
    sess.execute(
        "view.highlight",
        json!({ "nodes": weights.iter().map(|&(t, _)| v.st.transitions[t].clone()).collect::<Vec<_>>(), "label": label, "weights": badges }),
    )?;
    Ok(json!({
        "model": mid, "artifact": chart,
        "moves": sorted.iter().map(|&(t, w)| json!({"transition": v.st.transitions[t], "value": w})).collect::<Vec<_>>(),
    }))
}

#[allow(dead_code)]
fn _shared_across_threads() {
    fn sync<T: Sync + Send>() {}
    sync::<View>();
    sync::<Policy>();
}

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
use pflow_metamodel::game::{self, Solver};

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
            opt("games", Int, "teacher games to learn from, or random-play games to sample (default 300, 1..100,000)"),
            opt("epochs", Int, "passes over the games (default 40, 1..1000)"),
            opt("explore", Num, "share of moves played at random (default 0.3, 0..1)"),
            opt("positions", Str, "games (default): learn from `games` teacher games; all: from every reachable position once, labelled with the teacher's move; sampled: for games too large to solve whole, from the distinct positions of `games` random-play games (from `min_moves` in), each labelled by an exact alpha-beta search with every best move as the target (all and sampled: arch mlp only, since an RNN's move depends on the path to a position)"),
            opt("min_moves", Int, "sampled: only positions at least this many moves in (default 14)"),
            opt("budget", Int, "sampled: search nodes per move valued (default 800,000); positions past it are skipped and counted"),
            opt("labels", Str, "sampled: a JSONL file of exact labels, read before searching and appended to as positions are labelled, so a later run (more games, another seed, imitate or verify) searches only positions it has not seen; it records the net, rules and budget it was made with and refuses others"),
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
        help: "Experimental. Play `player` (default the model) against `opponent` from each side of `sides`, `games` games per \
               side, and store wins, draws and losses per side as a bar chart (shares of games) and a table. Each is one of: model \
               (greedy unless `stochastic`), random, minimax (a random choice among the moves of best value, from the exact teacher) \
               or ode (sim_evaluate's play scorer: fire each move, solve the mass-action ODE `horizon` ahead at the session's \
               rates, take the best expected reward for its side; deterministic, no search). player ode against opponent minimax \
               measures the ODE evaluator itself.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("model", Str, "rnn model artifact id (default: the newest)"),
            opt("sides", Json, "{transition-id prefix: sign}: who moves, e.g. {\"x_\": 1, \"o_\": -1} (+1 maximizes the reward)"),
            opt("reward", Json, "{place: weight} read from the final marking, from the +1 side's view, e.g. {\"win_x\": 1, \"win_o\": -1}"),
            opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled (e.g. win claims)"),
            opt("player", Str, "who plays each side in turn: model (default), random, minimax or ode"),
            opt("opponent", Str, "who plays against it: random (default), minimax or ode"),
            opt("horizon", Num, "ode: model time to solve ahead (default 3)"),
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
        id: "rnn.verify",
        label: "Verify Policy Exhaustively (experimental)",
        help: "Experimental. The exhaustive referee (sim-pflow-xyz experiments/ode-minimax): for each side of `sides`, walk \
               every legal line of the other side (optimal or not) from the initial marking, and at each of `player`'s \
               decisions check its move against the exact game value. A value-losing move turns a won or drawn position into a \
               loss; a missed win turns a won position into a draw. Reports, per side, the decisions checked (distinct \
               positions, and paths: an RNN's move depends on the path), both counts and the first failures (marking, the move, \
               the values, the best moves). Zero of both on every side is minimax equivalence over the whole game, not a sample. \
               player: model (greedy; default), ode or minimax (a check of the referee). Small games only (the teacher's \
               200,000 positions, `max_nodes` paths). With `games`, a sampled referee for larger games (the sim.pflow.xyz \
               ODE Connect-4 experiment): distinct positions from random-play games, `min_moves` in; every legal move is \
               valued by an exact alpha-beta search over the firing rule, and positions it cannot finish within `budget` \
               nodes are skipped and counted. Exact per position, a sample of the game.",
        menu: MENU,
        shortcut: None,
        args: &[
            opt("player", Str, "model (default), ode or minimax"),
            opt("model", Str, "model artifact id (default: the newest)"),
            opt("sides", Json, "{transition-id prefix: sign}: who moves, e.g. {\"x_\": 1, \"o_\": -1} (+1 maximizes the reward)"),
            opt("reward", Json, "{place: weight} read from the final marking, from the +1 side's view, e.g. {\"win_x\": 1, \"win_o\": -1}"),
            opt("auto", StrList, "transition-id prefixes that fire as soon as they are enabled (e.g. win claims)"),
            opt("horizon", Num, "ode: model time to solve ahead (default 3)"),
            opt("max_nodes", Int, "positions visited along all paths, at most (default 1,000,000, ..10,000,000)"),
            opt("games", Int, "sampled referee instead (for games too large to walk): positions from this many random-play games (e.g. 250)"),
            opt("min_moves", Int, "sampled: only positions at least this many moves in (default 14)"),
            opt("budget", Int, "sampled: search nodes per move valued (default 800,000); positions past it are skipped and counted"),
            opt("seed", Int, "sampled: the random games' seed (default 13)"),
            opt("labels", Str, "sampled: a JSONL file of exact labels, read before searching and appended to as positions are labelled, so a later run (more games, another seed, imitate or verify) searches only positions it has not seen; it records the net, rules and budget it was made with and refuses others"),
        ],
        enabled: always,
        run: verify,
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
        settle(self.v, self.auto, mk)
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

    /// The perfect-play value (+1 side's view) of a position
    /// [`Teacher::solve`] reached.
    fn solved_value(&self, mk: &Marking) -> Result<f64> {
        let key = self.v.st.marking(&self.settle(mk)?);
        self.value
            .get(&key)
            .copied()
            .ok_or_else(|| Error::Invalid("the teacher has not solved this position".into()))
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

/// Fires automatic transitions until none is enabled.
fn settle(v: &View, auto: &[bool], mk: &Marking) -> Result<Marking> {
    let mut mk = mk.clone();
    for _ in 0..10_000 {
        match v.enabled(&mk).into_iter().find(|&t| auto[t]) {
            Some(t) => mk = v.fire(&mk, t)?,
            None => return Ok(mk),
        }
    }
    Err(Error::Invalid(
        "automatic transitions never stop firing".into(),
    ))
}

/// sim_evaluate's play scorer as a player: fire each enabled move, solve
/// the mass-action ODE `horizon` ahead from the position it leaves, and
/// take the move whose expected reward is best for the side to move (the
/// first on ties). Deterministic; the choice is remembered per position.
struct OdePlayer<'a> {
    v: &'a View,
    doc: &'a crate::doc::Document,
    rates: &'a std::collections::BTreeMap<String, f64>,
    horizon: f64,
    /// (state variable, weight): the reward, read from the ODE's final state.
    reward: Vec<(String, f64)>,
    chosen: RwLock<HashMap<Vec<i64>, usize>>,
}

impl OdePlayer<'_> {
    fn choose(&self, auto: &[bool], sign: &[f64], mk: &Marking, en: &[usize]) -> Result<usize> {
        let key = self.v.st.marking(mk);
        if let Some(&t) = self.chosen.read().expect("cache").get(&key) {
            return Ok(t);
        }
        let s = mover(self.v, sign, en)?;
        let mut best = (f64::NEG_INFINITY, en[0]);
        for &t in en {
            let next = settle(self.v, auto, &self.v.fire(mk, t)?)?;
            let end =
                sim::ode(self.doc, &next, self.rates, self.horizon, "tsit5", 2)?.final_state();
            let score = s * self
                .reward
                .iter()
                .map(|(k, w)| w * end.get(k).copied().unwrap_or(0.0))
                .sum::<f64>();
            if score > best.0 {
                best = (score, t);
            }
        }
        self.chosen.write().expect("cache").insert(key, best.1);
        Ok(best.1)
    }
}

/// The moves of the highest perfect-play value (ignoring the tie-break).
fn optimal(en: &[usize], vals: &[(f64, f64)]) -> Vec<usize> {
    let top = vals.iter().map(|x| x.0).fold(f64::NEG_INFINITY, f64::max);
    en.iter()
        .zip(vals)
        .filter(|(_, x)| x.0 == top)
        .map(|(&t, _)| t)
        .collect()
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
                        legal: vec![],
                    });
                }
                // REINFORCE skips games with no reward (and does not count them).
                None if r.abs() < 1e-10 => {}
                None => seqs.push(Seq {
                    weights: vec![r; g.moves.len()],
                    inputs: g.inputs,
                    targets: g.moves,
                    legal: vec![],
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

/// What [`imitate`] trains on, and what it reports about it.
type Imitation = (
    Vec<Seq>,
    usize,
    Option<Vec<Value>>,
    Option<String>,
    usize,
    Value,
);

/// [`imitate`] with `positions: "sampled"`, for games too large for the
/// teacher's full solve: positions from `games` random-play games, each
/// labelled by the exact search ([`sample_labelled`]). A position's
/// example asks for every move of the best value at once (an even share
/// each), so equally good moves don't compete.
fn imitate_sampled(
    v: &View,
    a: &Value,
    rules: &Rules,
    from: &Marking,
    games: usize,
    seed: u64,
) -> Result<Imitation> {
    let min_moves = int(a, "min_moves", 14, 0, 10_000)?;
    let budget = int(a, "budget", 800_000, 1, 1_000_000_000)?;
    let d = Dense::new(v, rules)?;
    let (labelled, found, skipped, cached) = sample_labelled(
        v,
        &d,
        from,
        None,
        games,
        min_moves,
        budget,
        seed,
        s(a, "labels"),
    )?;
    let mut seqs: Vec<Seq> = labelled
        .iter()
        .map(|p| {
            let best = p.vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let targets: Vec<usize> = p
                .moves
                .iter()
                .zip(&p.vals)
                .filter(|(_, &x)| x == best)
                .map(|(&t, _)| d.to_view[t])
                .collect();
            let input = d.input(&p.mk);
            let legal: Vec<usize> = p.moves.iter().map(|&t| d.to_view[t]).collect();
            let mut seq = Seq::plain(vec![input; targets.len()], targets);
            seq.legal = vec![legal; seq.targets.len()];
            seq
        })
        .collect();
    let mut rng = SimpleRng::new(seed);
    for i in (1..seqs.len()).rev() {
        seqs.swap(i, (rng.next_u64() % (i as u64 + 1)) as usize);
    }
    let n = seqs.len();
    let sample = json!({"games": games, "min_moves": min_moves, "budget": budget,
                        "positions": found, "labelled": n, "skipped": skipped, "cached": cached});
    Ok((seqs, n, None, None, n, sample))
}

fn imitate(sess: &mut Session, a: &Value) -> Result<Value> {
    let games = int(a, "games", 300, 1, 100_000)?;
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
    let rules = game_rules(&v, a)?;
    let from = sim::initial_marking(&sess.doc);
    let mode = s(a, "positions").unwrap_or("games");
    match mode {
        "games" => {}
        "all" | "sampled" if matches!(rnn, Policy::Mlp(_)) => {}
        "all" | "sampled" => {
            return Err(Error::BadArgs(format!(
                "positions {mode} needs arch mlp: an RNN's move depends on the path to a position"
            )))
        }
        other => {
            return Err(Error::BadArgs(format!(
                "positions must be games, all or sampled, got {other}"
            )))
        }
    }
    let all = mode == "all";
    let (seqs, moves, opening, best_opening, positions, sample) = if mode == "sampled" {
        imitate_sampled(&v, a, &rules, &from, games, seed)?
    } else {
        let Rules { sign, reward, auto } = &rules;
        let mut teacher = Teacher::new(&v, auto, sign, reward);
        teacher.solve(&from)?;
        let teacher = &teacher;
        let auto = auto.clone();
        let played = each_game(if all { 0 } else { games }, |i| {
            let mut labels = vec![];
            let mut follow = |mk: &Marking, _: &[f64], en: &[usize], rng: &mut SimpleRng| {
                let (vals, best) = teacher.advise_solved(mk, en)?;
                labels.push(best);
                // The label is the teacher's move; the move played is any move
                // of the same value (or, `explore` of the time, any move), so
                // the games reach what every optimal line leads to.
                let t = if rng.uniform() < explore {
                    uniform(en, rng)
                } else {
                    uniform(&optimal(en, &vals), rng)
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
        if all {
            // Every reachable decision position once, in a seeded order.
            let mut seen = std::collections::HashSet::new();
            let mut stack = vec![settle(&v, &auto, &from)?];
            while let Some(mk) = stack.pop() {
                if !seen.insert(v.st.marking(&mk)) {
                    continue;
                }
                let en = v.enabled(&mk);
                if en.is_empty() {
                    continue;
                }
                let (_, best) = teacher.advise_solved(&mk, &en)?;
                seqs.push(Seq::plain(vec![v.input(&mk)], vec![best]));
                for &t in &en {
                    stack.push(settle(&v, &auto, &v.fire(&mk, t)?)?);
                }
            }
            let mut rng = SimpleRng::new(seed);
            for i in (1..seqs.len()).rev() {
                seqs.swap(i, (rng.next_u64() % (i as u64 + 1)) as usize);
            }
            moves = seqs.len();
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
        (
            seqs,
            moves,
            Some(opening),
            Some(v.st.transitions[best].clone()),
            positions,
            Value::Null,
        )
    };
    let mut losses = Vec::with_capacity(epochs);
    for _ in 0..epochs {
        losses.push(rnn.train(&seqs, lr, 1, batch));
    }
    let training = json!({
        "teacher": "minimax", "positions": mode,
        "games": if mode == "games" { seqs.len() } else { 0 }, "moves": moves, "sample": sample, "epochs": epochs, "explore": explore,
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
        "teacher_opening": opening.map(|o| json!({ "best": best_opening, "moves": o })),
    }))
}

fn evaluate(sess: &mut Session, a: &Value) -> Result<Value> {
    let player = s(a, "player").unwrap_or("model");
    let opponent = s(a, "opponent").unwrap_or("random");
    if !matches!(player, "model" | "random" | "minimax" | "ode") {
        return Err(Error::BadArgs(format!(
            "player must be model, random, minimax or ode, got {player}"
        )));
    }
    if !matches!(opponent, "random" | "minimax" | "ode") {
        return Err(Error::BadArgs(format!(
            "opponent must be random, minimax or ode, got {opponent}"
        )));
    }
    let horizon = match n(a, "horizon") {
        None => 3.0,
        Some(x) if x > 0.0 && x <= 10_000.0 => x,
        Some(x) => {
            return Err(Error::BadArgs(format!(
                "horizon must be in (0, 10000], got {x}"
            )))
        }
    };
    let v = View::new(sess)?;
    let (mid, net) = if player == "model" {
        let (mid, m) = model(sess, a)?;
        v.check(&m)?;
        let net = from_json(&m, &v)?;
        (Some(mid), Some(net))
    } else {
        (None, None)
    };
    let Rules { sign, reward, auto } = game_rules(&v, a)?;
    let games = int(a, "games", 200, 1, 5000)?;
    let max_steps = int(a, "max_steps", 200, 1, 10000)?;
    let seed = int(a, "seed", 1, 0, u32::MAX as usize)? as u64;
    let stochastic = b(a, "stochastic").unwrap_or(false);
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
    if player == "minimax" || opponent == "minimax" {
        teacher.solve(&from)?;
    }
    let teacher = &teacher;
    let ode = OdePlayer {
        v: &v,
        doc: &sess.doc,
        rates: &sess.rates,
        horizon,
        reward: reward
            .iter()
            .map(|&(i, w)| (v.st.display[i].clone(), w))
            .collect(),
        chosen: RwLock::default(),
    };
    let ode = &ode;
    // One agent's move.
    let act =
        |who: &str, p: &[f64], mk: &Marking, en: &[usize], rng: &mut SimpleRng| -> Result<usize> {
            Ok(match who {
                "model" => pick(p, en, stochastic, rng),
                "minimax" => {
                    let (vals, _) = teacher.advise_solved(mk, en)?;
                    uniform(&optimal(en, &vals), rng)
                }
                "ode" => ode.choose(&auto, &sign, mk, en)?,
                _ => uniform(en, rng),
            })
        };
    let (mut bars, mut rows) = (vec![], vec![]);
    for (k, (side, seat)) in prefixes.iter().enumerate() {
        let rewards = each_game(games, |i| {
            let mut h = net.as_ref().map(Policy::state).unwrap_or_default();
            let mut choose = |mk: &Marking, input: &[f64], en: &[usize], rng: &mut SimpleRng| {
                // The model's hidden state follows every move, both sides'.
                let p = match &net {
                    Some(net) => rnn_probs(net, &mut h, input, en),
                    None => vec![],
                };
                let who = if mover(&v, &sign, en)? == *seat {
                    player
                } else {
                    opponent
                };
                Ok((act(who, &p, mk, en, rng)?, vec![]))
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
    let name = match (&net, &mid) {
        (Some(net), Some(mid)) => format!("{} {mid}", net.name()),
        _ => player.to_string(),
    };
    let how = if player == "model" && stochastic {
        ", sampled"
    } else {
        ""
    };
    let chart = sess.add_artifact(
        format!("{name} vs {opponent} ({games} games per side{how}, seed {seed})"),
        source("rnn.evaluate", a),
        ArtifactData::Bars {
            value_label: "share of games".into(),
            bars,
        },
    );
    let table = sess.add_artifact(
        format!("{name} vs {opponent} table (seed {seed})"),
        source("rnn.evaluate", a),
        ArtifactData::Table {
            columns: ["side", "wins", "draws", "losses", "win share"]
                .map(String::from)
                .to_vec(),
            rows: rows.clone(),
        },
    );
    Ok(json!({
        "model": mid, "player": player, "opponent": opponent, "games_per_side": games, "artifact": chart, "artifacts": [chart, table],
        "results": rows.iter().map(|r| json!({"side": r[0], "wins": r[1], "draws": r[2], "losses": r[3]})).collect::<Vec<_>>(),
    }))
}

/// What [`verify`] counts for one side.
#[derive(Default)]
struct Referee {
    nodes: usize,
    paths: usize,
    /// Distinct decision positions, and those where some path's move lost
    /// value or missed a win.
    decisions: std::collections::HashSet<Vec<i64>>,
    losing: std::collections::HashSet<Vec<i64>>,
    missed: std::collections::HashSet<Vec<i64>>,
    failures: Vec<Value>,
}

/// The pieces [`walk`] needs.
struct Walk<'a> {
    v: &'a View,
    auto: &'a [bool],
    sign: &'a [f64],
    teacher: &'a Teacher<'a>,
    ode: &'a OdePlayer<'a>,
    net: Option<&'a Policy>,
    player: &'a str,
    seat: f64,
    max_nodes: usize,
}

/// Depth-first over every line of the other side; `h` is the model's
/// state along this path.
fn walk(w: &Walk, mk: &Marking, h: &[f64], r: &mut Referee) -> Result<()> {
    r.nodes += 1;
    if r.nodes > w.max_nodes {
        return Err(Error::Invalid(format!(
            "more than {} positions along the paths; raise max_nodes or use rnn.evaluate on a game this large",
            w.max_nodes
        )));
    }
    let mk = settle(w.v, w.auto, mk)?;
    let en: Vec<usize> = w.v.enabled(&mk);
    if en.is_empty() {
        r.paths += 1;
        return Ok(());
    }
    // The model's state follows every move, both sides' (as in rnn.evaluate).
    let mut h = h.to_vec();
    let p = match w.net {
        Some(net) => rnn_probs(net, &mut h, &w.v.input(&mk), &en),
        None => vec![],
    };
    let s = mover(w.v, w.sign, &en)?;
    if s != w.seat {
        for &t in &en {
            walk(w, &w.v.fire(&mk, t)?, &h, r)?;
        }
        return Ok(());
    }
    let t = match w.player {
        "model" => pick(&p, &en, false, &mut SimpleRng::new(0)),
        "ode" => w.ode.choose(w.auto, w.sign, &mk, &en)?,
        _ => w.teacher.advise_solved(&mk, &en)?.1,
    };
    let next = w.v.fire(&mk, t)?;
    let key = w.v.st.marking(&mk);
    // (+ 0.0 turns −0 into 0 for the report.)
    let (here, after) = (
        s * w.teacher.solved_value(&mk)? + 0.0,
        s * w.teacher.solved_value(&next)? + 0.0,
    );
    r.decisions.insert(key.clone());
    let lost = after < here && after < 0.0;
    let missed = after < here && here > 0.0 && after >= 0.0;
    if (lost || missed)
        && r.failures.len() < 5
        && !r.losing.contains(&key)
        && !r.missed.contains(&key)
    {
        let (vals, _) = w.teacher.advise_solved(&mk, &en)?;
        let best: Vec<&str> = optimal(&en, &vals)
            .iter()
            .map(|&b| w.v.st.transitions[b].as_str())
            .collect();
        let marking: serde_json::Map<String, Value> =
            w.v.st
                .display
                .iter()
                .zip(w.v.st.marking(&mk))
                .filter(|(_, n)| *n != 0)
                .map(|(k, n)| (k.clone(), json!(n)))
                .collect();
        r.failures.push(json!({
            "kind": if lost { "value-losing" } else { "missed win" },
            "marking": marking, "move": w.v.st.transitions[t], "value_before": here, "value_after": after,
            "best": best,
        }));
    }
    if lost {
        r.losing.insert(key);
    } else if missed {
        r.missed.insert(key);
    }
    walk(w, &next, &h, r)
}

fn verify(sess: &mut Session, a: &Value) -> Result<Value> {
    let player = s(a, "player").unwrap_or("model");
    if !matches!(player, "model" | "minimax" | "ode") {
        return Err(Error::BadArgs(format!(
            "player must be model, ode or minimax, got {player}"
        )));
    }
    let horizon = match n(a, "horizon") {
        None => 3.0,
        Some(x) if x > 0.0 && x <= 10_000.0 => x,
        Some(x) => {
            return Err(Error::BadArgs(format!(
                "horizon must be in (0, 10000], got {x}"
            )))
        }
    };
    let max_nodes = int(a, "max_nodes", 1_000_000, 1, 10_000_000)?;
    let v = View::new(sess)?;
    let (mid, net) = if player == "model" {
        let (mid, m) = model(sess, a)?;
        v.check(&m)?;
        (Some(mid), Some(from_json(&m, &v)?))
    } else {
        (None, None)
    };
    let rules = game_rules(&v, a)?;
    let Rules { sign, reward, auto } = &rules;
    let (sign, reward, auto) = (sign.clone(), reward.clone(), auto.clone());
    let from = sim::initial_marking(&sess.doc);
    let ode = OdePlayer {
        v: &v,
        doc: &sess.doc,
        rates: &sess.rates,
        horizon,
        reward: reward
            .iter()
            .map(|&(i, w)| (v.st.display[i].clone(), w))
            .collect(),
        chosen: RwLock::default(),
    };
    let mut seats: Vec<(String, f64)> = vec![];
    if let Some(Value::Object(o)) = a.get("sides") {
        seats = o
            .iter()
            .map(|(k, w)| (k.clone(), w.as_f64().unwrap_or(0.0)))
            .collect();
    } else if let Some(Value::String(t)) = a.get("sides") {
        if let Ok(o) = serde_json::from_str::<serde_json::Map<String, Value>>(t) {
            seats = o
                .iter()
                .map(|(k, w)| (k.clone(), w.as_f64().unwrap_or(0.0)))
                .collect();
        }
    }
    if a.get("games").is_some() {
        let out = verify_sampled(&v, a, &rules, &from, player, net.as_ref(), &ode, &seats)?;
        return finish_verify(sess, a, out, net.as_ref(), mid);
    }
    let mut teacher = Teacher::new(&v, &auto, &sign, &reward);
    teacher.solve(&from)?;
    let state = net.as_ref().map(Policy::state).unwrap_or_default();
    let (mut rows, mut sides_out) = (vec![], vec![]);
    let mut clean = true;
    for (side, seat) in &seats {
        let w = Walk {
            v: &v,
            auto: &auto,
            sign: &sign,
            teacher: &teacher,
            ode: &ode,
            net: net.as_ref(),
            player,
            seat: *seat,
            max_nodes,
        };
        let mut r = Referee::default();
        walk(&w, &from, &state, &mut r)?;
        clean &= r.losing.is_empty() && r.missed.is_empty();
        rows.push(vec![
            json!(side),
            json!(r.decisions.len()),
            json!(r.paths),
            json!(r.losing.len()),
            json!(r.missed.len()),
        ]);
        sides_out.push(json!({
            "side": side, "decisions": r.decisions.len(), "paths": r.paths,
            "value_losing": r.losing.len(), "missed_wins": r.missed.len(), "failures": r.failures,
        }));
    }
    let name = match (&net, &mid) {
        (Some(net), Some(mid)) => format!("{} {mid}", net.name()),
        _ => player.to_string(),
    };
    let table = sess.add_artifact(
        format!("{name}: exhaustive referee"),
        source("rnn.verify", a),
        ArtifactData::Table {
            columns: ["side", "decisions", "paths", "value-losing", "missed wins"]
                .map(String::from)
                .to_vec(),
            rows,
        },
    );
    let out = json!({
        "model": mid, "player": player, "minimax_equivalent": clean, "sides": sides_out, "artifact": table,
        "note": "every line of the opponent was walked; counts are distinct decision positions (an RNN's move can differ by path; a position counts once if any path's move failed)",
    });
    let report = sess.add_artifact(
        format!("{name}: exhaustive referee report"),
        source("rnn.verify", a),
        ArtifactData::Report(out.clone()),
    );
    let mut out = out;
    out["report"] = json!(report);
    out["artifacts"] = json!([table, report]);
    Ok(out)
}

/// The session's view of a net and the dense, index-resolved form
/// [`pflow_metamodel::game`] searches, side by side.
struct Dense {
    rules: game::Game,
    /// Per [`View`] place: its index in a dense marking.
    place: Vec<usize>,
    /// Per [`View`] transition: its index in `rules.ix`; and back.
    to_ix: Vec<usize>,
    to_view: Vec<usize>,
}

impl Dense {
    fn new(v: &View, r: &Rules) -> Result<Self> {
        let ix = v.st.net.model.indexed();
        let missing = || {
            Error::Invalid(
                "the exact search needs every token place and transition of the net (a coloured net with unused \
                 colour copies is not supported)"
                    .into(),
            )
        };
        if ix.places.len() != v.st.places.len() {
            return Err(missing());
        }
        let place =
            v.st.places
                .iter()
                .map(|p| ix.places.iter().position(|q| q == p).ok_or_else(missing))
                .collect::<Result<Vec<_>>>()?;
        let to_ix =
            v.st.transitions
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
            rules: game::Game::new(ix, auto, sign, reward),
            place,
            to_ix,
            to_view,
        })
    }

    fn dense(&self, v: &View, mk: &Marking) -> Vec<i64> {
        let x = v.st.marking(mk);
        let mut d = vec![0; self.rules.ix.places.len()];
        for (i, &p) in self.place.iter().enumerate() {
            d[p] = x[i];
        }
        d
    }

    /// A dense marking as the policy's input (the [`View`]'s place order).
    fn input(&self, d: &[i64]) -> Vec<f64> {
        self.place.iter().map(|&p| d[p] as f64).collect()
    }

    /// A dense marking as a session marking, shaped like `template`.
    fn marking(&self, v: &View, template: &Marking, d: &[i64]) -> Marking {
        let mut mk = template.clone();
        for (i, &p) in self.place.iter().enumerate() {
            let (base, c) = v.st.net.base(&v.st.places[i]);
            if let Some(slot) = mk.get_mut(&base).and_then(|x| x.get_mut(c)) {
                *slot = d[p];
            }
        }
        mk
    }
}

/// A sampled decision position, labelled by the exact search.
struct Labelled {
    mk: Vec<i64>,
    /// The model's state on arriving here along its game.
    h: Vec<f64>,
    side: f64,
    /// Moves (`rules.ix` order) and each one's value from the mover's view.
    moves: Vec<usize>,
    vals: Vec<f64>,
}

/// [`verify`] with `games`: the sampled referee of the
/// sim.pflow.xyz ODE Connect-4 experiment, for games too large to walk. Random-play
/// games give the positions (distinct, from `min_moves` moves in); the
/// exact search values every legal move at each, and positions it cannot
/// finish within `budget` nodes are skipped and counted, never guessed.
#[allow(clippy::too_many_arguments)]
fn verify_sampled(
    v: &View,
    a: &Value,
    rules: &Rules,
    from: &Marking,
    player: &str,
    net: Option<&Policy>,
    ode: &OdePlayer,
    seats: &[(String, f64)],
) -> Result<Value> {
    let games = int(a, "games", 250, 1, 100_000)?;
    let min_moves = int(a, "min_moves", 14, 0, 10_000)?;
    let budget = int(a, "budget", 800_000, 1, 1_000_000_000)?;
    let seed = int(a, "seed", 13, 0, u32::MAX as usize)? as u64;
    let d = Dense::new(v, rules)?;
    let r = &d.rules;
    let (labelled, found, skipped, cached) = sample_labelled(
        v,
        &d,
        from,
        net,
        games,
        min_moves,
        budget,
        seed,
        s(a, "labels"),
    )?;
    let (mut rows, mut sides_out) = (vec![], vec![]);
    let mut clean = true;
    for (side, seat) in seats {
        let mine: Vec<&Labelled> = labelled.iter().filter(|p| p.side == *seat).collect();
        let checks = crate::par::map(mine.clone(), |p| -> Result<(usize, f64, f64)> {
            let best = p.vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let t = match player {
                "model" => {
                    let net = net.expect("a model");
                    let en_view: Vec<usize> = p.moves.iter().map(|&t| d.to_view[t]).collect();
                    let probs = rnn_probs(net, &mut p.h.clone(), &d.input(&p.mk), &en_view);
                    d.to_ix[pick(&probs, &en_view, false, &mut SimpleRng::new(0))]
                }
                "ode" => {
                    let en_view: Vec<usize> = p.moves.iter().map(|&t| d.to_view[t]).collect();
                    let mk = d.marking(v, from, &p.mk);
                    d.to_ix[ode.choose(&rules.auto, &rules.sign, &mk, &en_view)?]
                }
                _ => p.moves[p.vals.iter().position(|&x| x == best).expect("a best move")],
            };
            let k = p.moves.iter().position(|&m| m == t).expect("a legal move");
            Ok((t, best, p.vals[k]))
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
        let (mut losing, mut missed, mut failures) = (0, 0, vec![]);
        for (p, &(t, best, after)) in mine.iter().zip(&checks) {
            let lost = after < best && after < 0.0;
            let miss = after < best && best > 0.0 && after >= 0.0;
            losing += lost as usize;
            missed += miss as usize;
            if (lost || miss) && failures.len() < 5 {
                let marking: serde_json::Map<String, Value> =
                    v.st.display
                        .iter()
                        .zip(d.input(&p.mk))
                        .filter(|(_, n)| *n != 0.0)
                        .map(|(k, n)| (k.clone(), json!(n)))
                        .collect();
                let bests: Vec<&str> = p
                    .moves
                    .iter()
                    .zip(&p.vals)
                    .filter(|(_, &x)| x == best)
                    .map(|(&m, _)| r.ix.transitions[m].as_str())
                    .collect();
                failures.push(json!({
                    "kind": if lost { "value-losing" } else { "missed win" },
                    "marking": marking, "move": r.ix.transitions[t], "value_before": best, "value_after": after,
                    "best": bests,
                }));
            }
        }
        let must_play = mine
            .iter()
            .filter(|p| {
                p.vals
                    .iter()
                    .any(|&x| x < p.vals.iter().copied().fold(f64::NEG_INFINITY, f64::max))
            })
            .count();
        clean &= losing == 0 && missed == 0;
        rows.push(vec![
            json!(side),
            json!(mine.len()),
            json!(must_play),
            json!(losing),
            json!(missed),
        ]);
        sides_out.push(json!({
            "side": side, "decisions": mine.len(), "must_play": must_play,
            "value_losing": losing, "missed_wins": missed, "failures": failures,
        }));
    }
    Ok(json!({
        "player": player, "mode": "sampled", "no_losses_in_sample": clean, "sides": sides_out,
        "sample": {"games": games, "min_moves": min_moves, "seed": seed, "budget": budget,
                   "positions": found, "labelled": labelled.len(), "skipped": skipped, "cached": cached},
        "rows": rows,
        "note": "a sampled referee, not an exhaustive one: every legal move at every sampled position is checked against \
                 the exact value, which is exact per position, but the positions are a sample of the game; skipped \
                 positions ran past the search budget and are not scored",
    }))
}

/// Distinct decision positions from `games` random-play games (from
/// `min_moves` moves in), each with every legal move valued by the exact
/// search: (the labelled positions, how many were found, how many ran past
/// `budget` and were skipped). `net`'s state is carried along each game.
#[allow(clippy::too_many_arguments)]
fn sample_labelled(
    v: &View,
    d: &Dense,
    from: &Marking,
    net: Option<&Policy>,
    games: usize,
    min_moves: usize,
    budget: usize,
    seed: u64,
    labels: Option<&str>,
) -> Result<(Vec<Labelled>, usize, usize, usize)> {
    let r = &d.rules;
    let mut start = d.dense(v, from);
    r.settle(&mut start)?;
    let state = net.map(Policy::state).unwrap_or_default();
    let sampled = each_game(games, |i| {
        let mut rng = game_rng(seed, 9, i);
        let (mut mk, mut h, mut out) = (start.clone(), state.clone(), vec![]);
        for ply in 0..10_000 {
            let en = r.moves(&mk);
            if en.is_empty() {
                break;
            }
            if ply >= min_moves {
                out.push((mk.clone(), h.clone()));
            }
            if let Some(net) = net {
                // The model's state follows every move, as in rnn.evaluate.
                let en_view: Vec<usize> = en.iter().map(|&t| d.to_view[t]).collect();
                rnn_probs(net, &mut h, &d.input(&mk), &en_view);
            }
            mk = r.play(&mk, uniform(&en, &mut rng))?;
        }
        Ok(out)
    })?;
    let mut seen = std::collections::HashSet::new();
    let positions: Vec<(Vec<i64>, Vec<f64>)> = sampled
        .into_iter()
        .flatten()
        .filter(|(mk, _)| seen.insert(mk.clone()))
        .collect();
    let found = positions.len();
    let mut cache = match labels {
        Some(path) => LabelCache::open(path, d, budget)?,
        None => LabelCache::default(),
    };
    let cached = positions
        .iter()
        .filter(|(mk, _)| cache.known.contains_key(mk))
        .count();
    let todo: Vec<Vec<i64>> = positions
        .iter()
        .filter(|(mk, _)| !cache.known.contains_key(mk))
        .map(|(mk, _)| mk.clone())
        .collect();
    // Label in batches, appending each to the cache, so a long run that
    // stops keeps what it has done.
    for batch in todo.chunks(LabelCache::BATCH) {
        let chunks: Vec<Vec<Vec<i64>>> = batch
            .chunks(batch.len().div_ceil(64).max(1))
            .map(<[_]>::to_vec)
            .collect();
        // One solver (and transposition table) per chunk, on every core.
        let done: Vec<(Vec<i64>, Option<Vec<f64>>)> = crate::par::map(chunks, |chunk| {
            let mut solver = Solver::new(r, budget);
            chunk
                .into_iter()
                .map(|mk| -> Result<(Vec<i64>, Option<Vec<f64>>)> {
                    let moves = r.moves(&mk);
                    let side = r.mover(&moves)?;
                    let mut vals = Vec::with_capacity(moves.len());
                    for &t in &moves {
                        match solver.value(&r.play(&mk, t)?)? {
                            Some(x) => vals.push(side * x + 0.0),
                            None => return Ok((mk, None)),
                        }
                    }
                    Ok((mk, Some(vals)))
                })
                .collect::<Result<Vec<_>>>()
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
        cache.add(done)?;
    }
    let mut skipped = 0;
    let mut labelled = vec![];
    for (mk, h) in positions {
        match &cache.known[&mk] {
            None => skipped += 1,
            Some(vals) => {
                let moves = r.moves(&mk);
                let side = r.mover(&moves)?;
                labelled.push(Labelled {
                    vals: vals.clone(),
                    mk,
                    h,
                    side,
                    moves,
                });
            }
        }
    }
    Ok((labelled, found, skipped, cached))
}

/// Exact labels by position: the values of each move (`rules.ix` order,
/// mover's view), or `None` where the search ran past its budget. With a
/// file, it is read when opened and every [`LabelCache::add`] appends to
/// it, one JSON object per line after a header naming what the labels
/// depend on.
#[derive(Default)]
struct LabelCache {
    known: HashMap<Vec<i64>, Option<Vec<f64>>>,
    file: Option<std::fs::File>,
    path: String,
}

impl LabelCache {
    const BATCH: usize = 20_000;

    /// What a label depends on: the net, the rules and the budget.
    fn header(d: &Dense, budget: usize) -> Value {
        let r = &d.rules;
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

    fn open(path: &str, d: &Dense, budget: usize) -> Result<Self> {
        use std::io::{BufRead, Write};
        let io = |e: std::io::Error| Error::Io(format!("{path}: {e}"));
        let want = Self::header(d, budget);
        let mut known = HashMap::new();
        let fresh = match std::fs::File::open(path) {
            Ok(f) => {
                let mut lines = std::io::BufReader::new(f).lines();
                let head: Value = match lines.next() {
                    Some(line) => serde_json::from_str(&line.map_err(io)?)
                        .map_err(|e| Error::Invalid(format!("{path}: header: {e}")))?,
                    None => Value::Null,
                };
                if !head.is_null() && head != want {
                    return Err(Error::Invalid(format!(
                        "{path} holds labels for another net, other rules or another budget (budget {})",
                        head["budget"]
                    )));
                }
                for (n, line) in lines.enumerate() {
                    let line = line.map_err(io)?;
                    // A run stopped mid-write leaves at most a partial last line.
                    let Ok(x) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    let mk: Vec<i64> = serde_json::from_value(x["mk"].clone())
                        .map_err(|e| Error::Invalid(format!("{path}: line {}: {e}", n + 2)))?;
                    let vals: Option<Vec<f64>> = serde_json::from_value(x["vals"].clone())
                        .map_err(|e| Error::Invalid(format!("{path}: line {}: {e}", n + 2)))?;
                    known.insert(mk, vals);
                }
                head.is_null()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => true,
            Err(e) => return Err(io(e)),
        };
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(io)?;
        if fresh {
            writeln!(file, "{want}").map_err(io)?;
        }
        Ok(Self {
            known,
            file: Some(file),
            path: path.to_string(),
        })
    }

    fn add(&mut self, done: Vec<(Vec<i64>, Option<Vec<f64>>)>) -> Result<()> {
        if let Some(f) = &mut self.file {
            use std::io::Write;
            let mut buf = String::new();
            for (mk, vals) in &done {
                buf.push_str(&json!({"mk": mk, "vals": vals}).to_string());
                buf.push('\n');
            }
            f.write_all(buf.as_bytes())
                .and_then(|()| f.flush())
                .map_err(|e| Error::Io(format!("{}: {e}", self.path)))?;
        }
        self.known.extend(done);
        Ok(())
    }
}

/// Stores a sampled referee's table and report.
fn finish_verify(
    sess: &mut Session,
    a: &Value,
    mut out: Value,
    net: Option<&Policy>,
    mid: Option<String>,
) -> Result<Value> {
    let name = match (net, &mid) {
        (Some(net), Some(mid)) => format!("{} {mid}", net.name()),
        _ => out["player"].as_str().unwrap_or("player").to_string(),
    };
    let rows: Vec<Vec<Value>> = serde_json::from_value(out["rows"].take()).unwrap_or_default();
    out.as_object_mut().expect("an object").remove("rows");
    out["model"] = json!(mid);
    let n = &out["sample"];
    let title = format!(
        "{name}: sampled referee ({} positions from {} games, {} skipped)",
        n["labelled"], n["games"], n["skipped"]
    );
    let table = sess.add_artifact(
        title.clone(),
        source("rnn.verify", a),
        ArtifactData::Table {
            columns: [
                "side",
                "decisions",
                "must-play",
                "value-losing",
                "missed wins",
            ]
            .map(String::from)
            .to_vec(),
            rows,
        },
    );
    out["artifact"] = json!(table);
    let report = sess.add_artifact(
        format!("{title} report"),
        source("rnn.verify", a),
        ArtifactData::Report(out.clone()),
    );
    out["report"] = json!(report);
    out["artifacts"] = json!([table, report]);
    Ok(out)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The alpha-beta search (pflow_metamodel::game) against the exhaustive teacher:
    /// the same value at every `step`-th position the teacher solved.
    fn search_agrees_with_teacher(open: (&str, Value), step: usize) {
        let mut sess = Session::default();
        sess.execute(open.0, open.1).expect("open");
        let v = View::new(&sess).expect("view");
        let a = json!({"sides": {"x_": 1, "o_": -1}, "reward": {"win_x": 1, "win_o": -1}, "auto": ["x_win_", "o_win_"]});
        let rules = game_rules(&v, &a).expect("rules");
        let mut teacher = Teacher::new(&v, &rules.auto, &rules.sign, &rules.reward);
        teacher
            .solve(&sim::initial_marking(&sess.doc))
            .expect("solve");
        let d = Dense::new(&v, &rules).expect("dense");
        let mut solver = Solver::new(&d.rules, usize::MAX);
        let mut checked = 0;
        for (key, &want) in teacher.value.iter().step_by(step) {
            let mut mk = vec![0; d.rules.ix.places.len()];
            for (i, &p) in d.place.iter().enumerate() {
                mk[p] = key[i];
            }
            let got = solver.value(&mk).expect("search").expect("no budget");
            assert_eq!(got, want, "at {key:?}");
            checked += 1;
        }
        assert!(checked > 1000, "{checked} positions");
    }

    #[test]
    fn search_agrees_with_the_teacher_on_tic_tac_toe() {
        search_agrees_with_teacher(("file.open_example", json!({"name": "tic-tac-toe"})), 1);
    }

    #[test]
    fn cached_labels_match_fresh_ones() {
        let mut sess = Session::default();
        sess.execute("file.open_example", json!({"name": "tic-tac-toe"}))
            .expect("open");
        let v = View::new(&sess).expect("view");
        let a = json!({"sides": {"x_": 1, "o_": -1}, "reward": {"win_x": 1, "win_o": -1}, "auto": ["x_win_", "o_win_"]});
        let rules = game_rules(&v, &a).expect("rules");
        let d = Dense::new(&v, &rules).expect("dense");
        let from = sim::initial_marking(&sess.doc);
        let path = std::env::temp_dir().join(format!("pflow-labels-{}.jsonl", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let file = path.to_str().expect("a path");
        let run = |games, labels| {
            sample_labelled(&v, &d, &from, None, games, 2, 1_000_000, 5, labels).expect("sample")
        };
        let (fresh, found, _, _) = run(40, None);
        // A first run with a file labels everything; a smaller run then
        // finds all of it cached, and a larger one only the new part.
        assert_eq!(run(20, Some(file)).3, 0);
        let (small, small_found, _, small_cached) = run(10, Some(file));
        assert_eq!(small_cached, small_found);
        let (again, _, _, cached) = run(40, Some(file));
        assert!(
            cached >= small.len() && cached < found,
            "{cached} of {found}"
        );
        let key = |l: &[Labelled]| {
            l.iter()
                .map(|p| (p.mk.clone(), p.moves.clone(), p.vals.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(key(&again), key(&fresh));
        // Labels made with another budget are refused.
        assert!(sample_labelled(&v, &d, &from, None, 10, 2, 5, 5, Some(file)).is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn search_agrees_with_the_teacher_on_connect_3() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/connect-3.json");
        search_agrees_with_teacher(("file.open", json!({ "path": path })), 7);
    }
}

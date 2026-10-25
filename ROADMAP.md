# Learned policies: roadmap of investigations

Investigations for the experimental learned policies (`--features rnn`). FEATURES.md stays the plan of record for shipped commands.

## Where things stand

On tic-tac-toe, an MLP trained on every reachable position once (`positions: "all"`, 4,520 positions) is **minimax-equivalent on both seats**, checked by `rnn.verify`'s exhaustive referee on 4 of 4 seeds (`layers` [64, 64] or [128, 64], 300 epochs, `learning_rate` 0.005–0.01). It clears the same bar sim-pflow-xyz `experiments/ode-minimax` cleared. That experiment needed hand-calibrated structure; the MLP learns it from the teacher's labels.

How it got there, in order of effect:
1. **Coverage.** The first imitation games followed one tie-broken line, and were graded against a minimax that played that same line, so "draws every game" was an artifact. Once games played any of the teacher's best moves, RNN 64 dropped from 104 to 8 losses in 1,000 as O, with no change to the model.
2. **Architecture.** On the same data, under the referee (value-losing / missed wins): RNN 64 scores 2 / 18 as X and 13 / 32 as O; MLP [64, 64] scores 0 / 2 and 0 / 0.
3. **Full coverage.** `positions: "all"` removed the MLP's last 2 missed wins. It is MLP-only: an RNN's move depends on the path to a position, so "each position once" isn't defined for it.

The plain ODE player (sim_evaluate's play scorer) is exact as X and has 12 value-losing positions as O (finding 10).

On Connect-3 (23,930 decision positions), the ODE player matches the sim.pflow.xyz experiment's referee exactly. The same MLP recipe, with [256, 128] layers trained for 200 epochs at lr 0.005, is minimax-equivalent on 4 of 4 seeds. At 100 epochs, seeds 2–4 leave 2–3 O positions wrong; see §6.

## What is and isn't new

Nothing so far is new as learning:
- Imitating an exact minimax teacher on tic-tac-toe is supervised learning on a solved game. With 4,520 decision positions, a lookup table is minimax-equivalent too. The MLP reaching that checks the pipeline; it is not a result.
- An MLP beating an RNN when the input is the full state is the Markov property at work.
- Coverage beating architecture is the usual lesson about training data.
- The ODE findings are ode-minimax's, reproduced here in a weaker form.
- Checking a small learned policy exhaustively against a solver has been done before.

What the representation buys is a harness. Every part of the learning setup comes from the net, with no game-specific code:
- the input (the marking) and the actions (the transitions)
- which moves are legal (enablement)
- the reward (read from the final marking) and the sides (transition prefixes)
- the teacher and the referee (both search over the firing rule)
- replayable, checkable games (`sim.goto`, `provenance.check_witness`)

OpenSpiel or Gym make you write each of those per game. Here they come from one formal model, and the ODE evaluator runs on the same model. So learned, search and continuous-relaxation players are all graded by the same exact referee. That is real leverage, but not a new method.

The claim gets tested past the solver's cap (§6):
- Does the raw marking (token counts per place) still make a good input on connect-4, or does a generic encoding lose to one written for the game?
- With no exact teacher and no exhaustive referee, can the pieces that come from the net, the ODE player especially, take the solver's place as teacher or referee?

Either that is where something new shows up, or the harness turns out to be only convenient. Both are worth knowing.

## Why the MLP is the right default

A fully modelled Petri net's marking is its whole state. Anything the future depends on has to be a place, or the firing rule couldn't work. History a game needs (castling rights, a repetition count) is modelled as places, and then it is in the marking. So a policy over the marking needs no memory, and an RNN's hidden state is something it has to learn to ignore. This is ode-minimax's lesson again: put what matters into the net's structure, not into the evaluator.

Memory only earns its place where the player can't see the whole marking, where modelling the history is too costly, or where there is no net yet (§3).

Each item below gives a question, a method, and what would settle it.

## 0. Measurement ✅ mostly
- `rnn.verify` is the headline metric: distinct value-losing positions and missed wins, per side. Tournaments are a sanity check.
- Still to do: report the median and range over 3–5 seeds for every row of the README's referee table, not only the last one.

## 1. Coverage ✅ answered on tic-tac-toe
- `positions: "all"` closes the gap. Open: how far `games` gets with more games or a higher `explore`, which matters once a game is too big to enumerate (§6).

## 2. Defaults and naming
- Make `arch: "mlp"` the default for `rnn.imitate` and `rnn.train`; keep `arch: "rnn"` for §3.
- The `rnn.*` namespace no longer fits. Consider `policy.*` once the current work settles. It is a rename across commands, tests, README, FEATURES.md and the Learn menu.
- Narrow the README's "it doesn't need one, because the marking is the net's whole state" to: true for a fully modelled net whose player can see every place.
- Optional ablation, to finish the memory-versus-depth story: MLP [64] (one layer), RNN 128, and the RNN with He/Xavier init, scored by `rnn.verify`. Low priority now that the default is settled.

## 3. Where memory is real
Not "hide the marking from the RNN": that throws away state the net already provides. Three honest cases:
- **What a player may see.** A hidden-information game (cards, fog of war) where the net models everything, but a fair policy for one player reads only that player's places. It has to infer the rest from the moves it has seen. Needs an `observe` option (place prefixes a side may read) in imitate, evaluate and verify, and a teacher for imperfect information. Exact minimax over the full marking cheats here.
- **Cost.** Modelling history as places multiplies the reachable markings. Measure how much, on a game that needs history, and compare it with a policy that carries memory instead.
- **No net yet.** From event logs before a net has been discovered, a sequence model over events is all there is. This belongs with `data.*`.
- Done when: one of these has a measured case where an RNN beats an MLP given the same input.

## 4. The ODE player as O (finding 10)
- First, reproduce sim's plain play scorer: 93 draws, 7 losses as O in ode-minimax, against 35 losses here. The likely causes are the net (`examples/tic-tac-toe.json` against sim's model with `game_active`, `move_tokens` and the weight-9 draw), how the draw is scored (ode-minimax saw O's losses swing from 7 to 84 on that alone), rates and `horizon`.
- Then tune with `opt.distill` (a hinge rank loss against exact minimax) and score with `rnn.verify`. Compare with ode-minimax's two-knob policy bias (finding 9).
- Done when: both numbers are reproduced, or the difference is explained.

## 5. Learning without a teacher
- Can `rnn.self_play` reach the imitation result, or keep it from slipping? Run self-play from scratch and from an imitation-trained start, and score each round with `rnn.verify`, since the exact teacher can still referee tic-tac-toe.
- Done when: a curve of value-losing positions per round, from both starts.

## 6. Bigger games (connect-3, connect-4)
This is the test of the harness claim above, and where the MLP result and the ODE approach part ways: imitation needs a teacher that has solved the game, and the exact teacher is capped at 200k positions. The ODE player needs no solved game.
- **Connect-3 (`examples/connect-3.json`, 52 places, 80 transitions) fits under the cap.** The teacher solves it in about 7.6 s: every opening is a forced X win, 23,930 decision positions.
  - **The port is faithful.** `rnn.verify` on the ODE player reproduces the sim.pflow.xyz experiment's referee table exactly. At `horizon` 0.5 the ODE player scores X 45 decisions / 0 value-losing / 0 missed wins and O 462 / 6 / 2. The `connect_3_matches_the_sim_referee` test pins that table. (At `horizon` 3 it scores O 454 / 8 / 2 against the experiment's 462 / 8 / 2.)
  - **The MLP on every position is minimax-equivalent once trained long enough.**

    | layers | epochs | lr | seed | equivalent | X value-losing + missed wins | O value-losing + missed wins |
    |---|---|---|---|---|---|---|
    | [128, 64] | 100 | 0.01 | 1 | no | 1 + 0 | 8 + 0 |
    | [256, 128] | 100 | 0.01 | 1 | **yes** | 0 + 0 | 0 + 0 |
    | [256, 128] | 100 | 0.01 | 2 | no | 0 + 0 | 2 + 1 |
    | [256, 128] | 100 | 0.01 | 3 | no | 0 + 0 | 2 + 0 |
    | [256, 128] | 100 | 0.01 | 4 | no | 0 + 0 | 2 + 0 |
    | [256, 128] | 200 | 0.005 | 1–4 | **yes, 4 of 4** (loss 0.0002) | 0 + 0 | 0 + 0 |
    | [512, 256] | 100 | 0.01 | 2–4 | **yes, 3 of 3** (loss 0.0001) | 0 + 0 | 0 + 0 |

    At 100 epochs, X is perfect on every seed and beats the ODE player every game. The misses are 2–3 O positions out of about 450. They give up value against perfect play but don't change any result against ODE, since O loses with best play.
  - **Longer training closes the gap.** [256, 128] at 200 epochs and lr 0.005 is minimax-equivalent on seeds 1–4, about 150–410 s each while sharing the cores with another run. So does doubling the width at 100 epochs: [512, 256] is equivalent on seeds 2–4, at about 310–470 s each under the same sharing. Every equivalent model faces the same 62 X and 449 O decisions.
  - Connect-3 stays out of the built-in gallery because `analysis.siphons` doesn't finish on it (§8).
- **Connect-4 (`examples/connect-4.json`, 131 places, 222 transitions, about 4.5 × 10¹² positions) is past any exhaustive teacher.** The referee and the teacher are sampled there:
  - `rnn.verify {games}` values every legal move at the distinct positions of random-play games by exact alpha-beta (`game_search.rs`, over pflow-metamodel's `IndexedModel`, added upstream in pflow-rs `70caba8` because `Model::enabled`/`fire` cost about 1 ms per position here). Its values were cross-checked move for move against the original experiment's oracle on 373 positions, which also exposed a sign error there on moves that end the game.
  - On the default sample (250 games, seed 13, 1,819 positions): random play loses value at about 33% of decisions, the `ode` player at 1.8% (X 17/940, O 17/879).
  - `rnn.imitate {positions: "sampled"}` with the MLP fits its training positions exactly (0 / 0 on both seats when trained on the referee's own positions) but on held-out positions (none shared with training) it scores 241/940 and 256/879 after 1,000 games (7,064 positions) 166/940 and 161/879 after 5,000 (36,227), and 81/940 and 88/879 after 25,000 (182,820). It generalizes, slowly: each 5× of data cut mistakes by a third, then by half, still about 5× the `ode` player's.
  - Training restricts the softmax to legal moves for `positions: "sampled"`. Over every output, the illegal moves' logits are pushed down without end; on 5,000 games that diverged after 30–50 epochs at lr 0.005 and 0.002 and at batch 1 alike. With the mask the loss falls steadily (7.12 → 6.48).
  - Open: the 100,000-game point: how held-out mistakes fall with more positions (the scaling curve, with `labels` caching the labels across runs). If 100k games still leaves it far from the `ode` player, the raw marking is the wrong input at this size, which answers the harness question above.
- Cheaper teachers, for more positions than exact search can label: depth-limited alpha-beta with the ODE player as the leaf evaluator, the distilled ODE player itself, or self-play (§5).
- Done when: MLP imitation is measured on one game too large to solve exhaustively, against a stated referee. ✅ measured (Connect-4, sampled referee); the open part is the scaling curve above.

## 7. Training stability
- Batches of 64 or more diverge because the update sums the steps of every game in the batch. With `positions: "all"` each example is one move and gets the full rate, so 0.02 is already too large. Try averaging the steps instead (a rate that doesn't scale with batch size or game length) and/or Adam, then re-measure speed against batch size.
- Done when: batch size and `positions` change speed, not correctness, at the default learning rate.

## 8. Engineering, alongside
- Upstream to pflow-tropical: a firing callback and a step cap (FEATURES.md §D 11), and the flat-buffer layout.
- Models are session artifacts: decide whether a trained model can be saved with the document and re-verified later.
- Re-check wasm (`make wasm-check`) whenever a new `rnn` option is added.
- `analysis.siphons` doesn't finish on Connect-3: it runs past 60 s against its 20M-node budget. Fix it before Connect-3 joins the built-in examples.
- The teacher's `solve` is single-threaded, about 7.6 s on Connect-3, and every `rnn.*` command pays it again. Parallelize it, or cache the solution per document.

## Order
2 (defaults, small) → 4 (reproduce the ODE baseline) → 5 → 7 when batch size starts limiting speed → 3 once there is a hidden-information game to test on.

# Learned policies: roadmap of investigations

Investigations for the experimental learned policies (`--features rnn`). FEATURES.md stays the plan of record for shipped commands.

Where things stand (tic-tac-toe, 2,000 imitation games, 150 epochs): MLP [64, 64]
draws every sampled game against a minimax that varies its moves. RNN 64 loses 8 of
1,000 as O. The plain ODE player loses 35 of 100 as O. `rnn.verify` walks every
opponent line. It finds the MLP has no value-losing moves but misses 2 wins as X, and
the ODE player makes value-losing moves at 12 positions as O. Every result is one
seed.

Each item below gives a question, a method, and what would settle it.

## 0. Measurement first
- **Headline metric = `rnn.verify`**, not tournaments: distinct value-losing positions
  and missed wins, per side. Tournaments stay as a sanity check.
- **Seeds:** every configuration gets 3–5 seeds; report the median and range. Re-run the
  current table that way before drawing conclusions.
- **Done when:** the README table is filled from `rnn.verify`, over several seeds.

## 1. Coverage: what the model never sees
- Q: Are the MLP's 2 missed wins a coverage gap (positions that only follow an
  opponent blunder)?
- Method: `positions: "all"` (every reachable position once) vs `games` at `explore`
  0, 0.1, 0.3, 0.5. Optionally, a mix: all positions plus teacher games.
- Done when: some setting gives 0 value-losing moves and 0 missed wins over every seed,
  or the residual errors are explained.

## 2. Architecture: separate memory from depth
At 64 units both nets have about 8.4k parameters, and the only structural difference is
what the 64×64 matrix does: recurrence (W_hh) or a second layer. Also mixed in: tanh vs
ReLU, and 0.1-scale init vs He.
- Grid (about 4 s per run): RNN 64, RNN 128, MLP [64], MLP [64, 64], MLP [32].
- Follow-ups if the grid points at init or activation: an RNN with Xavier/He init, and
  an RNN with W_hh fixed at 0 (a one-layer tanh MLP). The first may belong upstream in
  pflow-tropical.
- Done when: the RNN's deficit is attributed to memory, depth, init, or seed noise.

## 3. A task where memory is actually needed
On a fully observed game the marking is the whole state, so an RNN shouldn't win. Test
it where history matters:
- **Moves-only input:** the input is a one-hot of the last fired transition, not the
  marking, so the RNN has to rebuild the board itself. Needs an `input` option in
  imitate/evaluate/verify.
- **Hidden places:** some places are left out of the input (partial observation).
- **Path-dependent reward:** a reward that depends on the path, not only the final
  marking. The README's "doesn't need memory" claim fails here.
- Done when: an RNN beats an MLP that gets the same input on one of these.

## 4. The ODE player as O (finding 10)
- Q: How much of the gap do tuned rates close in this implementation?
- Method: `opt.distill` (already here: a hinge rank loss against exact minimax), then
  `rnn.evaluate player: ode` and `rnn.verify`. Compare with ode-minimax's two-knob
  policy bias (finding 9), and sweep `horizon`.
- Done when: the ODE player's value-losing positions as O are counted after
  distillation, and checked against the sim-pflow-xyz result.

## 5. Learning without a teacher
- Q: Can `rnn.self_play` reach the imitation result, or keep it from slipping?
- Method: self-play from scratch, and from an imitation-trained start; score each
  round with `rnn.verify`, since the exact teacher can still referee tic-tac-toe.
- Done when: a curve of value-losing positions per round, from both starts.

## 6. Bigger games (connect-3, connect-4)
The exact teacher is capped at 200k positions.
- Find out which petri-pilot `ode-connect3`/`ode-connect4` nets fit under the cap.
- Above it, candidate teachers: depth-limited alpha-beta with the ODE player as the
  leaf evaluator, the distilled ODE player itself, or self-play. `rnn.verify` stops
  being exhaustive, so there it falls back to sampled tournaments plus spot checks.
- Done when: MLP imitation is measured on one game too large to solve exhaustively,
  against a stated referee.

## 7. Training stability
- Batches of 64 or more diverge because the update sums the steps of every game in the
  batch. Try averaging them (an lr that doesn't scale with batch size) and/or Adam,
  then re-measure speed against batch size.
- Done when: batch size changes speed, not correctness, at the default learning rate.

## 8. Engineering, alongside
- Upstream to pflow-tropical: a firing callback and a step cap (FEATURES.md §D 11), and
  the flat-buffer layout.
- Models are session artifacts: decide whether a trained model can be saved and
  re-verified later.
- Re-check wasm (`make wasm-check`) whenever a new `rnn` option is added.

## Order
0 → 1 and 2 (cheap, in parallel) → 4 → 3 (new input modes) → 5 → 6 (new games) → 7
when batch size starts to block speed.

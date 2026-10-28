# Connect-4 search: measurements

This folder records how the exact game search behind `rnn.imitate` and
`rnn.verify` got faster on Connect-4, checked against Pascal Pons' solver
at every step. It is kept as source material for a write-up. The numbers
are for pflow-rs `aa8cde3` and pflow-native `4e98a2a`, measured 2026-10-22
to 2026-10-27 on an M4 Pro MacBook (14 cores, 36 GB).

The search knows nothing about Connect-4. `pflow_metamodel::game` reads any
net as a game:

- Automatic transitions fire until none is enabled.
- Every other transition is a move of the side its sign names.
- A final marking is worth the reward read from its places.

`examples/connect-4.json` declares the wins as automatic `x_win_*` /
`o_win_*` transitions paying `win_x` / `win_o`. The rule for this work was
that performance goes into pflow-rs as generic Petri net tooling, and
anything that could not be made generic was not built (pflow-rs
`CLAUDE.md`, "Performance work").

## The benchmark

- **Referee:** 1,942 positions from 250 sampled games (seed 13).
- **Budget:** 800,000 nodes per move value. A position whose value is not
  found within the budget is skipped.
- **Oracle:** Pascal Pons' solver (github.com/PascalPons/connect4 at
  `d6ba50d`, `c4solver -a`, AGPL v3, not vendored here). Each label is
  compared move by move with the sign of Pons' score.
- **Nodes per value:** the mean over every legal move of the first 60
  referee positions (393 values).

## Results

| Step | Commit | µs per node | Nodes per value, mean (median) | 40 games, one thread | Referee, all cores |
|---|---|---|---|---|---|
| Starting point: every transition rechecked after each move | before `0c74b7d` | ~2.4 | 42,144 | 130.8 s | ~11 positions/s |
| Enabled sets kept current across firings (failing-condition counts) | pflow-rs `0c74b7d` | 1.60 | 42,144 (2) | | |
| 0/1 markings searched as bits (`BitModel`) | pflow-rs `fa7b407`, pflow-native `053a56c` | 0.65 | 42,144 (2) | 37.0 s | 72 positions/s |
| Narrow windows (yes/no search per reward gap, mover's worst first) | pflow-rs `aa8cde3` (step 1) | 0.68 | 33,492 (3) | | |
| Bit enabled sets carried and updated incrementally | `aa8cde3` (step 2) | 0.58 | 33,492 (3) | | |
| Condition masks packed as (word, care, one) | `aa8cde3` (final) | **0.39** | **33,492 (3)** | **21.3 s** | **131 positions/s** |

The intermediate `aa8cde3` rows were measured on the working tree before
the commit, so they have no commit of their own.

### Labels

| | Labelled | Skipped (past budget) | Agrees with Pons |
|---|---|---|---|
| Before narrow windows | 1,819 | 123 | all |
| After | 1,835 | 107 | all 1,835 (11,784 move values) |

No value changed at any step. Every row is a speedup only.

Of the 107 skipped positions, Pons scores 73 as wins for the side to move,
10 as draws and 24 as losses. Wins are over-represented, because a deep
forced win is the hardest kind of value to prove within the budget.

Earlier, a larger check also agreed with Pons on every value: 4,635,258
move values over 734,548 training positions, plus the 11,672 referee
values. That run is in the README.

For scale, Pons labels about 1,000 positions a second on 8 cores. It is
Connect-4-specific: a bitboard, a hand-tuned move order and an opening
book.

## What worked and why

- **Keeping enabled sets current.** Each transition keeps a count of the
  conditions the current marking fails. After a firing, only conditions on
  the places it changed are re-evaluated. This is generic and exact.
- **Bits for 0/1 nets.** Many nets never put two tokens on a place: board
  cells and turn tokens are examples. On such a net:
  - enabledness is `mk & care == one`, a mask test;
  - firing is `mk ^= flip`, and firing again undoes it.

  A firing that would leave a place outside 0/1 is refused, and the search
  falls back to counts. `BitModel::wakes(t)` lists the transitions `t`
  could enable, so settling rechecks only those automatic transitions.
- **Narrow windows.** A final marking can only be worth a subset sum of the
  reward weights, so the possible values are few (≤16; here −1, 0, +1). A
  value is found by yes/no searches, one per gap between adjacent possible
  values. The order of those searches matters:

  | First yes/no search | Change in nodes per value |
  |---|---|
  | Mover's worst gap | −21% |
  | Mover's best gap | −3% |

## What didn't, and what wasn't built

- **Make/unmake instead of copying children.** Undo in place
  (`IndexedModel::unfire_tracked`) is exact, but it was slower here: 3.69
  µs per node against 1.6 µs for copying, because every child is played
  twice. Bit markings are four inline `u64` words, so copying is cheap. The
  undo functions are kept in pflow-rs for other callers.
- **Skipping the hub rechecks.** Incremental move generation gave only
  about 15%, because the turn tokens are a hub. Every move sets a turn
  token that 111 transitions read: all of the other side's moves and all
  of its own win detectors.

  Avoiding those rechecks would need knowledge of which markings are
  reachable. For example, a line that is already complete would have
  fired its detector already. Static net structure does not give that, and
  a Connect-4 shortcut would break the generic-only rule, so it was not
  built.

## Files

| File | What |
|---|---|
| `referee-labels.jsonl` | Our labels for the 1,942 referee positions at `aa8cde3`. Line 1 is the header (place order, net, rules, budget). Each later line is a dense marking `mk` and per-move `vals`, `null` when skipped. It is also a valid `labels` cache for `rnn.imitate`. |
| `pons-scores.txt` | Pons' `c4solver -a` output for the same positions: the move string, then one score per column (−1000 = full column). |
| `pons_input.py` | Converts the labels to Pons move strings (one per line) and writes a meta file that maps each string to our values. |
| `pons_compare.py` | Counts agreements between our values and Pons' signs. |
| `label_games.py` | Labels N sampled games through `pflow-native-cli control` and times it. Use 250 for the referee and 40 for the one-thread timing. |
| `nodes_per_value.rs` | An ignored test that prints nodes per value and µs per node over the first N referee positions. |

## Reproducing

```bash
# Pons check
python3 docs/connect4-search/pons_input.py docs/connect4-search/referee-labels.jsonl seqs.txt meta.jsonl
c4solver -a < seqs.txt > pons.txt     # or use pons-scores.txt
python3 docs/connect4-search/pons_compare.py - pons.txt meta.jsonl

# Throughput (fresh labels file; RAYON_NUM_THREADS=1 for one thread)
cargo build --release -p pflow-native-cli --features rnn
python3 docs/connect4-search/label_games.py target/release/pflow-native-cli /tmp/new.jsonl 40

# Nodes per value (copy nodes_per_value.rs into crates/core/tests/ first)
C4_LABELS=$PWD/docs/connect4-search/referee-labels.jsonl C4_N=60 \
  cargo test --release -p pflow-native-core --test nodes_per_value -- --ignored --nocapture
```

The last command was rerun against the pinned pflow-rs `aa8cde3` when this
file was written. It reproduced 33,492 nodes per value (median 3, 8 of 393
past budget) at 0.38 µs per node.

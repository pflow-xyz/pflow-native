# cv-a: spiders view notes

## Manifest

No new kinds, no changed manifests. `spider`, `leak` (orders) and `junction` `tt` / `history` (connect4) are
emitted exactly as before; `--composition-goldens` output is byte-identical to
`parity/composition-views/goldens.json` (100 manifests).

## pflow-rs cites re-checked against aacb7eb (the prototype's were read at c396d46)

The solver's table moved to `memo.rs` and became bounded since c396d46, so several prototype cites and two
statements of fact were stale. Now (all `crates/pflow-metamodel/src/`):

| what | prototype @c396d46 | now @aacb7eb |
|---|---|---|
| one table shared by the yes/no searches | game.rs:23-28 | game.rs:25-29 |
| Entry { lo, hi, best } | game.rs:440-444 | memo.rs:53-60 |
| Solver.tt / Solver.history | game.rs:454-455 | game.rs:566-567 |
| default table size | game.rs:469-470 (cleared past 2,000,000) | game.rs:588-594 (`Solver::MEMO` = 1<<23, bounded, weakest entry replaced; memo.rs:15-24) |
| history init | game.rs:485 | game.rs:619 |
| with_memo | game.rs:495-505 | game.rs:633-642 |
| store | game.rs:514-519, 616-623, 679-691 | game.rs:1181-1185, 1082-1085, 1168-1177 |
| tt.get / first = e.best | game.rs:601-611 | game.rs:1069-1079 |
| Ctx { first, history } | game.rs:628-633 | game.rs:1090-1095 |
| cutoff | game.rs:666 | game.rs:1164 |
| search | game.rs:590-693 | game.rs:1050-1179 |
| solve, one search when null_window off | game.rs:564-566 | game.rs:943-959 |
| Chain::cutoff | order.rs:267-271 | order.rs:277-281 |
| Chain::default | order.rs:205-216 | unchanged |
| plan drivers default | plan.rs:17-19 | plan.rs:29-30 |
| plan memo | plan.rs:247-249 | plan.rs:425-427 |
| exact mode refuses unsound pieces | plan.rs:216-238 / 221-236 | plan.rs:378-392 |

Dropped as no longer true: "the table is cleared past 2,000,000 entries between calls" and "cap: usize::MAX".
The filters slot now shows the plan's real default (`Filter::DEFAULT`, plan.rs:41-43) as a ghost chip.
go-pflow (69257b3) and ontology/ROADMAP (bc4a996 / f854116) lines were re-read and are unchanged.

## Gaps

- No "replay fusion" button (the prototype's is an HTML button above the SVG; replay is not a registry command).
  The fusion plays on link-added only.
- A `root` plan slot (pflow-rs `root`) has no rectangle in the connect4 layout; `SLOTS` is iterated, so an id
  without a rect is skipped, not drawn.
- The piece palette strip under the drawing is mod.rs's `palette()` (its chip colours are not theme.js's).

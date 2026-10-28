# pflow-rs cites re-checked at aacb7eb (agent e)

The prototype (`pflow-views-prototype/js`) cites pflow-rs at **c396d46**. The app pins **aacb7eb**, where `plan.rs`, `game.rs` and `order.rs` moved and changed (a bounded memo in `memo.rs`, move filters in `filter.rs`, a `root` field with `root.rs`, `mtd_f` / `aspiration` drivers, a `ms` budget, `plan.net`). Every cite was re-read in the aacb7eb source (a difflib line map was only a hint) and the result is one table, `crates/core/src/composition/pflow_rs_cites.json`, read through `search_plan::pflow_rs_cite(key)` (core) and `compviews::pflow_rs_cites::pflow_rs(key)` (the app's label).

## Counts (the prototype's 178 cites, plus 20 new)

| status | count | meaning |
|---|---|---|
| same | 47 | prototype lines unchanged and still say it |
| moved | 79 | same thing, other lines (includes ranges the prototype had a line or two off) |
| rewritten | 45 | code changed: the note was reworded or the cite re-aimed |
| gone | 7 | the cited thing no longer exists; the entry points at what replaced it |
| new | 20 | not in the prototype (root field, ode, mtd_f, aspiration, filters, engine) |

Keys: `<prototype-file-stem>.<name>` (the property name in the prototype, e.g. `lens.value`, `plan.driversFlags`, `spiders.ttShared`), `<file>.L<line>` for a cite with no property (`string.L1300b`; a letter when a line has several), `piece.<name>` and `slot.<id>` for the palette (`.2`, `.3` for a second and third cite of the same piece). `was` is where the prototype put it. `needle` is checked against lines `from..=to` by `crates/core/tests/pflow_rs_cites.rs` (tree: `PFLOW_RS_SRC`, else `$CARGO_HOME/git/checkouts/pflow-rs-*/aacb7eb*`).

## Gone (7)

| key | was (c396d46) | now (aacb7eb) | what is true now |
|---|---|---|---|
| `spiders.tableDefault` | game.rs:469-470 | game.rs:589-594 | there is no TABLE const: the default is Solver::MEMO = 2^23 entries, and a full table replaces its weakest entries instead of being cleared |
| `spiders.capDefault` | game.rs:489-490 | memo.rs:15-24 | there is no cap field: the table is bounded inside a search too (a new key takes an empty slot of its two buckets, else the weakest; the table never grows past its entries) |
| `spiders.clear` | game.rs:529-531 | game.rs:895-900 | there is no clearing: start() ages the table (a new generation), and entries of earlier calls are the first replaced once the table is full |
| `lens.tableDefault` | game.rs:469-470 | game.rs:589-594 | there is no TABLE const: the default is Solver::MEMO = 2^23 entries, and a full table replaces its weakest entries instead of being cleared |
| `lens.capDefault` | game.rs:489-490 | memo.rs:15-24 | there is no cap field: the table is bounded inside a search too (a new key takes an empty slot of its two buckets, else the weakest; the table never grows past its entries) |
| `lens.clear` | game.rs:529-531 | game.rs:895-900 | there is no clearing: start() ages the table (a new generation), and entries of earlier calls are the first replaced once the table is full |
| `string.L1300b` | game.rs:469-470 | game.rs:589-594 | there is no TABLE const: the default is Solver::MEMO = 2^23 entries, and a full table replaces its weakest entries instead of being cleared |

## Rewritten (45)

| key | was (c396d46) | now (aacb7eb) | what is true now |
|---|---|---|---|
| `plan.connect4Exact` | plan.rs:6-11 | plan.rs:6-13 | the module-doc example; at the pin it also lists filters, a root and the aspiration driver (connect4_exact() here is the smaller, older form) |
| `piece.budget` | plan.rs:18-19 | plan.rs:35-37 | budget: {"budget": {"nodes": N, "ms": T}} gives a value up past N nodes or T milliseconds (either or both) |
| `piece.budget.2` | game.rs:593-597 | game.rs:1057-1064 | search: nodes > budget, or the clock ran out (every 1,024 nodes): give up (None) |
| `piece.memo.2` | game.rs:495-505 | game.rs:634-641 | with_memo: a fresh table bounded to `entries`, inside a search and across calls; a full table replaces its weakest entries (there is no table/cap pair) |
| `slot.filters` | plan.rs:222-236 | plan.rs:341-358 | filters(): the Filter names (must_win, non_losing; absent means Filter::DEFAULT = [must_win], [] runs none); a name listed twice is refused |
| `slot.heuristics` | plan.rs:222-236 | plan.rs:378-393 | check(): heuristics are read as heuristics; the reserved static_eval (and beam, a filter) is refused: exact mode "can change the value", play mode "is not built" |
| `plan.value` | game.rs:526-554 | game.rs:902-935 | Solver::value calls run(): nodes reset to 0 (bit engine, then again by counts if it spilled), then solve |
| `plan.store` | game.rs:514-519 | game.rs:1181-1185 | store: remember what a search found; the table (memo.rs) merges it with what it knew and, once full, replaces its weakest entry (there is no cap check) |
| `spiders.withMemo` | game.rs:495-505 | game.rs:634-641 | with_memo: a fresh table bounded to `entries`, inside a search and across calls; a full table replaces its weakest entries (there is no table/cap pair) |
| `spiders.store` | game.rs:514-519 | game.rs:1181-1185 | store: remember what a search found; the table (memo.rs) merges it with what it knew and, once full, replaces its weakest entry (there is no cap check) |
| `spiders.solveOne` | game.rs:564-566 | game.rs:947-959 | solve: with null_window and mtd_f both off (or fewer than 3 rewards), one search over the whole reward range (after one over a window around the root's guess, when aspiration gives one) |
| `spiders.storeAfter` | game.rs:679-691 | memo.rs:289-292 | the merge with the old entry is Table::keep's (lo = max, hi = min); the search only calls store |
| `spiders.planCheck` | plan.rs:216-238 | plan.rs:378-393 | check: exact mode refuses only the reserved beam and static_eval; the filters must_win and non_losing are built and legal in either mode |
| `laws.planDrivers` | plan.rs:146-194 | plan.rs:205-281 | drivers(): the list is read into flags (null_window, mtd_f, aspiration) and a budget (nodes, ms) (driver-wrap.chyp reads the nesting), each at most once |
| `laws.planFilters` | plan.rs:222-235 | plan.rs:377-393 | check(): the filters are built (must_win, non_losing: legal in either mode); only the reserved beam and static_eval are refused |
| `lens.value` | game.rs:526-554 | game.rs:902-935 | Solver::value calls run(): nodes reset to 0 (bit engine, then again by counts if it spilled), then solve |
| `lens.engine` | game.rs:533-553 | game.rs:918-934 | run() (called by value()): the bit engine when the marking fits, else counts |
| `lens.driversDoc` | plan.rs:17-19 | plan.rs:29-37 | drivers: null_window on by default (off when drivers is given without it); budget gives a value up past N nodes or T milliseconds |
| `lens.window` | game.rs:573-577 | game.rs:970-981 | the value is one of vals[i..=j]; the first window is (vals[k], vals[k+1]) of the mover's worst gap (or of the gap nearest the root's guess, under aspiration) |
| `lens.fullRange` | game.rs:564-566 | game.rs:947-959 | solve: with null_window and mtd_f both off (or fewer than 3 rewards), one search over the whole reward range (after one over a window around the root's guess, when aspiration gives one) |
| `lens.children` | game.rs:626-643 | game.rs:1087-1141 | children keyed by the chain (after must_win and non_losing drop what they can, and the root's pieces rank the root's moves), sorted best first |
| `lens.recurse` | game.rs:654-654 | game.rs:1152-1152 | self.search(next, a, b, false): the core again, one level down (root = false: the root's pieces run only at the position searched) |
| `lens.boundsStore` | game.rs:670-691 | game.rs:1168-1177 | the result as [lo, hi], stored; the table merges it with the old entry (memo.rs, Table::keep) |
| `lens.store` | game.rs:514-519 | game.rs:1181-1185 | store: remember what a search found; the table (memo.rs) merges it with what it knew and, once full, replaces its weakest entry (there is no cap check) |
| `lens.withMemo` | game.rs:495-505 | game.rs:634-641 | with_memo: a fresh table bounded to `entries`, inside a search and across calls; a full table replaces its weakest entries (there is no table/cap pair) |
| `lens.solver` | plan.rs:240-251 | plan.rs:407-440 | solver_with(): budget (nodes, ms), chain, null_window, mtd_f, filters, the root's non_losing, aspiration, play (the mode is read), memo; the root's estimate is given by the caller |
| `lens.modeCheck` | plan.rs:222-236 | plan.rs:385-392 | filters / heuristics: only the reserved names (beam, static_eval) are refused: exact mode "can change the value", play mode "is not built"; must_win and non_losing are built |
| `lens.L606` | plan.rs:21-23 | plan.rs:41-47 | filters: must_win and non_losing drop moves only where the value is already known, so they are legal in either mode (absent means must_win, [] runs none); heuristics, and the reserved beam, can change the value: none is built |
| `lens.L709a` | plan.rs:21-23 | plan.rs:41-47 | filters: must_win and non_losing drop moves only where the value is already known, so they are legal in either mode (absent means must_win, [] runs none); heuristics, and the reserved beam, can change the value: none is built |
| `string.value` | game.rs:526-554 | game.rs:902-935 | Solver::value calls run(): nodes reset, then solve (by bits, or again by counts) |
| `string.L1159` | game.rs:541-548 | game.rs:926-934 | run() (called by value()): nodes = 0 before the bit-engine run, and again before the retry by counts |
| `string.L1231` | game.rs:612-643 | game.rs:1080-1141 | moves, keyed by the chain (after the filters), sorted |
| `string.L1278b` | game.rs:533-553 | game.rs:918-934 | run() (called by value()): the bit engine when the marking fits, else counts |
| `string.L1300a` | game.rs:448-466 | game.rs:559-586 | Solver: tt (a bounded memo::Table) and history; there are no table/cap fields |
| `string.L1300c` | game.rs:495-505 | game.rs:634-641 | with_memo: a fresh table bounded to `entries`, inside a search and across calls; a full table replaces its weakest entries (there is no table/cap pair) |
| `string.L1329` | game.rs:678-691 | memo.rs:289-292 | merge with what was known, then store: the merge is Table::keep's (lo = max, hi = min) |
| `operad.planDoc` | plan.rs:6-11 | plan.rs:6-13 | the module-doc example; at the pin it also lists filters, a root and the aspiration driver (connect4_exact() here is the smaller, older form) |
| `operad.driversFlags` | plan.rs:134-138 | plan.rs:184-191 | Drivers: flags null_window, mtd_f, aspiration, plus the budget (nodes) and ms: still flags and a budget, not a list |
| `operad.drivers` | plan.rs:146-194 | plan.rs:205-281 | drivers(): the list is read into flags (null_window, mtd_f, aspiration) and a budget (nodes, ms), each at most once |
| `operad.check` | plan.rs:216-238 | plan.rs:364-395 | check(): every name in its place; aspiration needs a root estimate; exact mode refuses only the reserved names (beam, static_eval), not the built filters |
| `operad.exactRefuses` | plan.rs:229-234 | plan.rs:385-392 | a reserved filter or heuristic (beam, static_eval): refused in exact mode, not built in play mode |
| `operad.solver` | plan.rs:240-251 | plan.rs:407-440 | solver_with(): budget (nodes, ms), chain, null_window, mtd_f, filters, the root's non_losing, aspiration, play (set by mode), memo; the root's estimate is given by the caller, not built |
| `operad.value` | game.rs:526-554 | game.rs:902-935 | run() (under value): nodes reset once per call (twice when the bit engine spills and the run is redone by counts) |
| `operad.run` | game.rs:526-597 | game.rs:926-934 | run(): nodes reset once per call (and again before a retry by counts); search counts them, so one node count covers the whole null-window run |
| `operad.fullWindow` | game.rs:564-565 | game.rs:947-959 | solve: with null_window and mtd_f both off (or fewer than 3 rewards), one search over the whole reward range (after one over a window around the root's guess, when aspiration gives one) |

## Things the views must not keep saying

- **Table cleared past 2,000,000 entries / `cap` / `TABLE` / `table` (spiders memo slot, lens, string memo).** None of it exists. The table is `memo::Table`: bounded to `entries` (default `Solver::MEMO` = 2^23) *inside* a search as well as across calls; a full table replaces its weakest entry (earlier-call entries first, via `age()`); it is never cleared and never grows past its bound. Keys: `spiders.tableDefault`, `spiders.capDefault`, `spiders.clear`, `lens.tableDefault/capDefault/clear`, `string.L1300b`.
- **`Entry` is in `memo.rs`**, and the merge with the old entry is `Table::keep`'s (`spiders.entry`, `spiders.storeAfter`, `string.L1329`).
- **"exact mode refuses every filter and heuristic"; "check() refuses every named filter"; "mode is not read"** (spiders.planCheck, laws.planFilters, lens.modeCheck, operad.check/exactRefuses, lens.solver). `must_win` and `non_losing` are built filters, legal in either mode (`filters` absent means `["must_win"]`); only the reserved `beam` / `static_eval` are refused; `solver_with` reads `mode` (`play`).
- **"Drivers: two flags"** (operad.driversFlags, laws.planDrivers, operad.drivers): there are three flags (`null_window`, `mtd_f`, `aspiration`) and a budget of `nodes` and/or `ms`; `null_window` and `mtd_f` exclude each other; `aspiration` needs a root estimate.
- **`value()` resets nodes**: it is `run()` (called by `value()` and `choose()`) that does; `value()` first ages the table, starts the clock and runs the root's pieces (`rank`).
- **The module-doc example** now lists `filters`, `root` and the `aspiration` driver; `search_plan::connect4_exact()` is the smaller, older plan (`plan.connect4Exact`, `operad.planDoc`).
- **Palette note:** `budget` takes `nodes` and/or `ms` at the pin (`piece.budget`); the native palette only offers `nodes`.

## Inline `file.rs:N-M` strings in the prototype's views

Hover and legend text in the views quotes line numbers directly, besides the `cites` arrays; they are c396d46 numbers too (several were already a line or two off, e.g. `plan.rs:146-194`, `game.rs:556-591`). Each is matched to the table entry (same file) whose prototype range overlaps it and is closest to it; `-` means no entry covers it, so replace the string by a `pflow_rs(key)` label or drop the number. A match marked (near) is a pointer: read the entry's note before reusing its numbers.

| prototype string | table entry | now |
|---|---|---|
| game.rs:23-28 | `spiders.ttShared` | game.rs:23-28 (same) |
| game.rs:101-103 | `lens.valueErr` | game.rs:117-119 (moved) |
| game.rs:105-122 | `lens.rewards` | game.rs:121-138 (moved) |
| game.rs:221-232 | `flat.settle` | game.rs:277-288 (moved) |
| game.rs:440-444 | `spiders.entry` | memo.rs:53-60 (moved) |
| game.rs:448-450 | `string.L1300a` (near) | game.rs:559-586 (rewritten) |
| game.rs:454 | `spiders.fields` (near) | game.rs:566-567 (moved) |
| game.rs:455 | `spiders.fields` (near) | game.rs:566-567 (moved) |
| game.rs:469-470 | `lens.tableDefault` | game.rs:589-594 (gone) |
| game.rs:485 | `spiders.histInit` | game.rs:619-619 (moved) |
| game.rs:489-490 | `lens.capDefault` | memo.rs:15-24 (gone) |
| game.rs:495-500 | `lens.withMemo` (near) | game.rs:634-641 (rewritten) |
| game.rs:495-505 | `lens.withMemo` | game.rs:634-641 (rewritten) |
| game.rs:514-519 | `lens.store` | game.rs:1181-1185 (rewritten) |
| game.rs:515-519 | `lens.store` (near) | game.rs:1181-1185 (rewritten) |
| game.rs:515-520 | `lens.store` (near) | game.rs:1181-1185 (rewritten) |
| game.rs:516 | `lens.store` (near) | game.rs:1181-1185 (rewritten) |
| game.rs:524-555 | `lens.value` (near) | game.rs:902-935 (rewritten) |
| game.rs:526-527 | `lens.value` (near) | game.rs:902-935 (rewritten) |
| game.rs:526-528 | `lens.value` (near) | game.rs:902-935 (rewritten) |
| game.rs:526-597 | `operad.run` | game.rs:926-934 (rewritten) |
| game.rs:526-693 | `lens.search` (near) | game.rs:1046-1179 (moved) |
| game.rs:533-553 | `lens.engine` | game.rs:918-934 (rewritten) |
| game.rs:541 | `string.L1159` (near) | game.rs:926-934 (rewritten) |
| game.rs:544-553 | `lens.spill` | game.rs:929-934 (moved) |
| game.rs:548 | `string.L1159` (near) | game.rs:926-934 (rewritten) |
| game.rs:556-588 | `lens.solve` | game.rs:937-996 (moved) |
| game.rs:556-591 | `lens.solve` (near) | game.rs:937-996 (moved) |
| game.rs:561-588 | `lens.solve` (near) | game.rs:937-996 (moved) |
| game.rs:564-565 | `operad.fullWindow` | game.rs:947-959 (rewritten) |
| game.rs:564-566 | `lens.fullRange` | game.rs:947-959 (rewritten) |
| game.rs:573-586 | `lens.narrow` (near) | game.rs:985-993 (moved) |
| game.rs:573-587 | `lens.narrow` (near) | game.rs:985-993 (moved) |
| game.rs:575-586 | `lens.narrow` (near) | game.rs:985-993 (moved) |
| game.rs:576-577 | `lens.window` (near) | game.rs:970-981 (rewritten) |
| game.rs:578-580 | `lens.upSolve` | game.rs:982-984 (moved) |
| game.rs:590-592 | `operad.run` (near) | game.rs:926-934 (rewritten) |
| game.rs:590-693 | `lens.search` | game.rs:1046-1179 (moved) |
| game.rs:593-597 | `lens.budgetCount` | game.rs:1057-1060 (moved) |
| game.rs:593-690 | `lens.search` (near) | game.rs:1046-1179 (moved) |
| game.rs:593-693 | `lens.search` (near) | game.rs:1046-1179 (moved) |
| game.rs:594 | `plan.budget` (near) | game.rs:1057-1060 (moved) |
| game.rs:594-597 | `plan.budget` | game.rs:1057-1060 (moved) |
| game.rs:599 | `lens.ttGet` (near) | game.rs:1067-1079 (moved) |
| game.rs:599-611 | `lens.ttGet` | game.rs:1067-1079 (moved) |
| game.rs:601-609 | `spiders.ttRead` (near) | game.rs:1069-1079 (moved) |
| game.rs:601-611 | `spiders.ttRead` | game.rs:1069-1079 (moved) |
| game.rs:601-612 | `spiders.ttRead` (near) | game.rs:1069-1079 (moved) |
| game.rs:610 | `spiders.ttRead` (near) | game.rs:1069-1079 (moved) |
| game.rs:612-613 | `lens.leaf` (near) | game.rs:1080-1086 (moved) |
| game.rs:612-625 | `lens.leaf` | game.rs:1080-1086 (moved) |
| game.rs:612-642 | `string.L1231` (near) | game.rs:1080-1141 (rewritten) |
| game.rs:614-623 | `spiders.storeLeaf` (near) | game.rs:1082-1086 (moved) |
| game.rs:616-623 | `spiders.storeLeaf` | game.rs:1082-1086 (moved) |
| game.rs:626-643 | `lens.children` | game.rs:1087-1141 (rewritten) |
| game.rs:628-633 | `spiders.ctxBuild` | game.rs:1090-1095 (moved) |
| game.rs:634-643 | `lens.children` (near) | game.rs:1087-1141 (rewritten) |
| game.rs:653-668 | `lens.loop` (near) | game.rs:1151-1167 (moved) |
| game.rs:653-669 | `lens.loop` | game.rs:1151-1167 (moved) |
| game.rs:654 | `lens.recurse` | game.rs:1152-1152 (rewritten) |
| game.rs:654-656 | `lens.upSearch` | game.rs:1152-1154 (moved) |
| game.rs:657-664 | `lens.loop` (near) | game.rs:1151-1167 (moved) |
| game.rs:665-668 | `lens.cutoff` | game.rs:1163-1166 (moved) |
| game.rs:666 | `plan.cutoff` | game.rs:1164-1164 (moved) |
| game.rs:670-691 | `lens.boundsStore` | game.rs:1168-1177 (rewritten) |
| game.rs:679-691 | `spiders.storeAfter` | memo.rs:289-292 (rewritten) |
| order.rs:1-7 | `lens.orderOnly` | order.rs:1-7 (same) |
| order.rs:4-6 | `lens.orderOnly` (near) | order.rs:1-7 (same) |
| order.rs:13 | `slot.order.2` (near) | order.rs:1-2 (moved) |
| order.rs:20-21 | `lens.ctxRead` (near) | order.rs:15-24 (same) |
| order.rs:26-31 | `operad.decisive` | order.rs:26-31 (same) |
| order.rs:36-37 | `lens.cutoffNoop` | order.rs:36-37 (same) |
| order.rs:52-58 | `lens.ttBest` | order.rs:52-58 (same) |
| order.rs:68-71 | `lens.histKey` | order.rs:68-71 (same) |
| order.rs:69-71 | `lens.histKey` (near) | order.rs:68-71 (same) |
| order.rs:73-75 | `lens.histCut` | order.rs:73-75 (same) |
| order.rs:194-265 | `plan.chainKey` (near) | order.rs:197-282 (moved) |
| order.rs:197-199 | `spiders.chainTies` | order.rs:197-199 (same) |
| order.rs:205-216 | `lens.chainDefault` | order.rs:205-216 (same) |
| order.rs:205-217 | `lens.chainDefault` (near) | order.rs:205-216 (same) |
| order.rs:220-230 | `string.L1220b` | order.rs:220-230 (same) |
| order.rs:221-226 | `string.L1220b` (near) | order.rs:220-230 (same) |
| order.rs:249-265 | `operad.chainKey` | order.rs:259-275 (moved) |
| order.rs:267-271 | `lens.chainCutoff` | order.rs:277-281 (moved) |
| plan.rs:3-4 | `spiders.planFixed` | plan.rs:3-4 (same) |
| plan.rs:8 | `operad.planDoc` (near) | plan.rs:6-13 (rewritten) |
| plan.rs:15-16 | - | - |
| plan.rs:15-18 | `lens.driversDoc` (near) | plan.rs:29-37 (rewritten) |
| plan.rs:17-18 | `lens.driversDoc` (near) | plan.rs:29-37 (rewritten) |
| plan.rs:17-19 | `lens.driversDoc` | plan.rs:29-37 (rewritten) |
| plan.rs:18-19 | `piece.budget` | plan.rs:35-37 (rewritten) |
| plan.rs:21-23 | `lens.L606` | plan.rs:41-47 (rewritten) |
| plan.rs:70-71 | `lens.L709b` | plan.rs:105-106 (moved) |
| plan.rs:113-118 | - | - |
| plan.rs:134-138 | `operad.driversFlags` | plan.rs:184-191 (rewritten) |
| plan.rs:146-193 | `lens.driversFlags` | plan.rs:205-281 (moved) |
| plan.rs:146-194 | `laws.planDrivers` | plan.rs:205-281 (rewritten) |
| plan.rs:147-152 | `operad.driversAbsent` | plan.rs:206-214 (moved) |
| plan.rs:196-210 | `laws.planChain` | plan.rs:283-298 (moved) |
| plan.rs:197-199 | `laws.planChain` (near) | plan.rs:283-298 (moved) |
| plan.rs:204-206 | `string.L1220a` | plan.rs:292-294 (moved) |
| plan.rs:216-238 | `operad.check` | plan.rs:364-395 (rewritten) |
| plan.rs:221-236 | `lens.modeCheck` (near) | plan.rs:385-392 (rewritten) |
| plan.rs:222-235 | `laws.planFilters` | plan.rs:377-393 (rewritten) |
| plan.rs:229-234 | `operad.exactRefuses` | plan.rs:385-392 (rewritten) |
| plan.rs:242-251 | `lens.solver` (near) | plan.rs:407-440 (rewritten) |
| plan.rs:245 | `lens.budgetDefault` | plan.rs:415-415 (moved) |
| plan.rs:247-249 | `spiders.planMemo` | plan.rs:425-427 (moved) |

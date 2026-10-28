# Agent c: lens, process, laws

Branch `feat/cv-c`. Files: `crates/app/src/compviews/{lens_view,c_lens_orders,c_lens_plan,process_view,c_process,laws_view,c_laws_live,c_chyp,c_hypergraph,c_draw}.rs`.

## Manifests

None changed: `cargo run --release -p pflow-native -- --composition-goldens <file>` is byte-identical to
`parity/composition-views/goldens.json`. No new kinds are proposed.

- **lens (orders)**: `verdict`, `pair`, `step`, `fix` are emitted in both modes (strict, assume), as D5/D13
  say. The strict card draws every pair as a chip, every forward rendezvous as a row and every repair as a
  table row; the assume drawing draws the same as lens boxes, rings and tables. The Connect-4 plan
  emits `frame` for every driver the plan lists (budget, null_window, and also mtd_f / aspiration when
  the plan has them), `lens` per order piece, `memo`. Filters are drawn (a card per filter the plan runs,
  `must_win` by default at the pin) but, as D23 lists no kind for them, not emitted.
- **process**: `run` per scenario and `opening`, unchanged (the scenario shown is view state, not a manifest).
- **laws**: `theory` (own tabs), `rewrite` (the selected theory's chips), `cover`: unchanged. The step shown
  is not in the manifest (`view.composition {step}`), nor are the "other:" theory tabs (the prototype shows
  them; D26 lists own tabs only), nor the live card.

## Core changes (small, additive)

- `view.composition` takes `scenario` (Process) and `step` (Laws, 0-based; `-1` resets). `rewrite` also
  takes a rule name (the Laws view shows its two sides). Setting `rewrite` resets `step`.
  `CompositionView` gains `scenario: Option<String>` and `step: Option<usize>` (both `#[serde(default)]`).
- `plan.opening` reports `markings`: the non-zero places, the moves and the automatic transitions enabled at
  the initial marking and after each fired drop (pflow-metamodel's `enabled_why_not`).
- `bundle.run` / `Run` reports `markings`: the marking before the first step and after each step that fired.
  Nothing in the app fires: the Process view reads what fired, why a step is refused and the markings from
  these, and assigns token identities (first in, first out) itself.

## Gaps

- Laws: the `.chyp` files' comments ("As written", the Sources block, the comment above each rule and
  rewrite) are not shown: only the JSON export is vendored, and the files' inline cites are c396d46 lines.
  Theory-level cites come from the verified table (`laws.orderKey`, `laws.planChain`, `laws.planDrivers`,
  `laws.planFilters`) and go-pflow's `compose_fuse.go:9-20`, `compose_flatten.go:72-88` (@69257b3, checked).
- Laws: live cards exist for `orderer-monoid` (Connect-4) and `sync-spiders` (orders). `filter-meet` and
  `driver-wrap` have no live card (the prototype's draw the plan's drivers/filters as Search boxes).
- Laws: the whole chain is drawn at once with the selected step emphasised (the prototype hovers a "="
  instead); a click on a "=" selects its step. The card for a rule (a chip) shows its two sides.
- Process: the prototype's per-link animations are the replay sweep (wires drawn left to right, boxes
  appearing) and the fused box's members converging; the token/data link wire pulse is not ported.
- Lens (Connect-4): no play dot (the prototype's is illustrative and runs no search); the lane texts are
  paraphrases re-read at aacb7eb, including what changed there (bounded table, `must_win`, `mtd_f`,
  `aspiration`, the root slot).
- Lens (orders, assume): the other instances' pairs are placed above the step they fuse with exactly as
  the prototype does; added instances (payment2, ...) count in the check but only those fused with the
  first saga are drawn.
- `make clippy` stops on `spiders_view.rs:95` (an empty line after a doc comment: on the base branch,
  not mine); everything of mine is clean under `-A clippy::empty_line_after_doc_comments`.

## Where the pieces live

- `c_process.rs`: token identities, idle tokens, the lane layout search (`choose_layout`), with tests.
- `c_hypergraph.rs`: `js/layout/hypergraph.js` as an egui-free module; tests hold it to cvxpy's optimum
  on `assets/chyp/receipts/chyp-layout.json`.
- `c_chyp.rs`: one exported Chyp diagram at a pixel scale (box widths widened to fit labels, ports on
  the box edges), Chyp's tentacle cubic for wires; tests on two real graphs and, in `laws_view.rs`, on
  every graph of the four exports.

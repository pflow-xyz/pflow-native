# cv-d: flat, boundaries, incidence

Branch `feat/cv-d`. Views: `flat_view.rs`, `boundaries_view.rs`, `incidence_view.rs`, with
helper files `d_draw.rs` (text with halo/wrapped, arrows, tokens, badges, prototype colours,
cite labels), `d_flat_layout.rs` (port of flat.js `layoutOrders`) and `d_bnd_layout.rs` (port of
boundaries.js `layoutOrders`).

## Manifests

None changed. `--composition-goldens` regenerated and diffed against
`parity/composition-views/goldens.json`: 0 of 100 manifests differ. No new kinds are proposed.
The token game's marking is not in the manifest: `place.initial` stays the flat initial
marking, `scenario` stays the frozen `bundle.run` per scenario.

## New registry commands (core, additive) and proposed spec text

All fire through `pflow_metamodel` (`Model::enabled_why_not` / `fire`, `Game::play`); the state
is held in the session, so an agent and the window drive the same game. Tests:
`crates/core/tests/views_game.rs`, `views_plan_game.rs`, `views_boundaries.rs`.

| command | journal | what |
|---|---|---|
| `bundle.game` | no | token game on the flat net: marking, fired, enabled and why-not per flat transition, selected scenario's progress (steps; predicates once done), declared constraints evaluated at the marking |
| `bundle.fire {transition}` | yes | fire a flat transition (flat id or a member `subnet/transition`); refused with `enabled_why_not`'s reason; leaves any scenario |
| `bundle.reset_marking` | yes | back to the initial marking |
| `bundle.scenario {id}` | yes | select a scenario (reset, nothing fired) |
| `bundle.step {all?}` | yes | take the next step (or the rest); the first step not enabled stops the script |
| `bundle.boundaries` | no | event classes: composite enablement and each member's own (one-transition model through `enabled_why_not`) |
| `plan.game` / `plan.play {transition}` / `plan.new_game` | no / yes / yes | play the Connect-4 net by hand: `Game::play` (move, then automatic `*win*` transitions settle, lowest index first) |

`bundle.incidence`'s invariants gained `y`, `n`, `y_c` (y·C per column) and `y_m0`
(`holds` unchanged). The state lives in `Composition.game` / `OpenPlan.game`, so it is part of undo,
and an edit that changes the flat net starts a new game.

Spec text proposal (docs/composition-views-scene.md, new Decision, no manifest effect):

> D27. The flat view's token game is the registry's (`bundle.fire`, `bundle.step`,
> `bundle.scenario`, `bundle.reset_marking`; `plan.play`, `plan.new_game` for connect4). A renderer
> never fires on its own. The manifest is collected in the resting state: `place.initial` is the
> flat initial marking whatever the current marking, and `scenario` entries are `bundle.run`'s.

## Shared-file changes (all additive)

* `compviews/mod.rs`: `mod d_*` lines; `Data` gained `game`, `boundaries`, `c4game` and `prev`
  (flat + incidence of the previous revision, for the incidence merge animation), filled in `refresh`.
* core: `composition/game.rs`, `composition/boundaries.rs`, `plan_game.rs` (new files); `Composition.game`,
  `OpenPlan.game` fields; `commands/bundle.rs` and `commands/plan.rs` command entries (`set_plan`
  keeps the game); `invariant_check` next to `invariant_holds` in `composition/mod.rs`.

## Where the drawing follows the prototype

* flat: cluster layout with the per-pattern homes, label placement, fused elements in the gap
  (refineFused), RenderFlatDOT shapes (double borders, read arc dashed with a dot, inhibitor head,
  weight pills), legend, click-to-fire green transitions, scenario chips + Step / Run all, the PASS/FAIL list,
  the pool-leak explanation (`leakWhy`), constraints at the current marking, link-added merge
  animation (members slide together, fused element rings in; a guard link draws its read arc in).
  connect4: the playable fixture net at its own coordinates, group boxes and captions, preset/postset
  arcs on hover, status line, automatic detectors settling.
* boundaries: instances as `N : k -> l`, rows wired to the outer or shared edge, event wires into one
  `⊗` port per class, cards (members' own enablement, EnabledWhyNot, rendezvous rule, binary links, pre/postset bar,
  pool-leak caveat), faint ghosts for token/data/guard links, the merge animation (ports and rects slide
  to the shared edge, absorbed boundary ports travel to the new one, new wire draws in, card slides/fades in),
  instance-added slide-in. connect4: the closed box `N : 0 -> 0` (manifest: `na` only).
* incidence: block-diagonal C beside the flat C, class combs and wire combs, tints per pattern, the
  crosshair, rotated column headers, read-arc diamonds and pre = post rings, y·C strips with
  `y·C = 0 ✓` / `y·M₀ = n ✓`, hovered invariant lights its rows, bottom legend and the
  `189/189` agreement check, link-added animation (columns/rows slide, old members ghost into the sum, Σ badge,
  comb draws in; guard ring). connect4: 131 x 222 matrix grouped by prefix with the readout line and hover.

## Gaps (honest)

* No token-travel animation when a transition fires (the prototype slides dots along the arcs); the state
  changes at once. It needs a `Fired` event in `mod.rs` `Event`.
* The prototype's "go-pflow's own firing" card (compares with a recorded go-pflow run) is not drawn: the receipt
  lives in `crates/core/tests/fixtures/views/gopflow.json`, a test fixture, not app data.
* Info has no `bib` field, so the bibliography entries the prototype attaches to hovers are folded into facts.
* The scenario / constraints cards clip rather than scroll when a long scenario note does not fit.
* `mod.rs`'s shared `pattern_tint` / `schema_color` differ from the prototype's `theme.js` (queue is orange there, blue in the prototype);
  my views use the prototype's colours from `d_draw.rs`. The lead may want to correct `mod.rs` for the others.
* Pre-existing at the base: clippy `empty_line_after_doc_comments` in `spiders_view.rs:95` fails `-D warnings`;
  not touched here.

## Cites

pflow-rs cites now come from the verified table (`pflow_rs("flat.settle")` etc., cherry-picked fb52908):
flat.c4fixture, flat.c4names, flat.c4decl, flat.gameDoc, flat.settle (prototype 221-232 -> 277-288 at aacb7eb),
flat.score (239-242 -> 311-314), flat.idOrder, boundaries.fixture, incidence.* , plan.connect4Exact.
go-pflow cites were re-read at 69257b3 (composedot.go, compose_flatten.go, compose_fuse.go, firing.go,
unionfind.go, schema.go, compose.go, reachability/invariants.go, petri/net.go, docs/MODEL-CORRECTNESS.md) and the
ontology/ROADMAP cites at bc4a996 / f854116: all matched, none moved.

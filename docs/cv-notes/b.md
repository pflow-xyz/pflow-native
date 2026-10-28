# The string view (agent b)

Two levels, one toggle: `view.composition {level: "pattern" | "transition"}` (default `pattern`),
and the transition level's lens on the Connect-4 net, `view.composition {lens_y}`. Both are
session state (`CompositionView.level`, `.lens_y`, serde defaults, not journaled).

## What was ported

| | prototype | here |
|---|---|---|
| layout (translatePetriNet) | `vendor/diagram-viewer.js` (pflow-xyz `public/diagram-viewer.js`, MIT, blob e9bb675e…, at 954b75c and 6241e4b) | `core::string_diagram::translate` (it was already a port; now also `isCrossing`, `Wire::forward_curve`, `Wire::back_curves`, `Diagram::stubs`, `flat_document`), query `diagram.transition_layout` |
| pattern level, orders | `ordersPattern` (string.js:248-924) | `compviews/b_orders.rs` |
| pattern level, Connect-4 | `c4Pattern` (string.js:1096-1377) | `compviews/b_c4.rs` |
| transition level | `ordersTransition`, `c4Transition` | `compviews/b_transition.rs` |

The layout is tested against the JS: `crates/core/tests/diagram_viewer_fixtures.rs` compares boxes
(x, y, width, layer, posInLayer, ports), every wire (ends, `isCrossing`, `isBack`, inhibit, weight),
boundaries, back edges and the size with `translatePetriNet`'s output (fixtures and provenance in
`crates/core/tests/fixtures/diagram-viewer/`). `Wire.crossing` was missing and is now computed.

## Cites

Every pflow-rs line cite in the string view is a key of the verified table (`pflow_rs_cites.json`,
agent e): `string.value`, `string.solve`, `string.search`, `string.ttRead`, `plan.cutoff`,
`plan.store`, `string.L1300b/c` (the memo: there is no TABLE const at the pin; `Solver::MEMO = 2^23`
and a bounded table), `string.chain`, `string.decisive`, `plan.chainKey`, ... The short forms drawn on the
figure (`game.rs:1046-1179`) are read from the same table. The prototype's text that is false at
aacb7eb was reworded: "cleared past 2M between calls, one search may grow without limit" (the table is
bounded inside a search and across calls), "play parses but solver() never reads the mode"
(`with_play(self.mode == Mode::Play)`), "filters are refused in exact mode" (`must_win` and
`non_losing` are legal; the default filters are `[must_win]`, drawn as a piece on the wire with
"default" on it), "decisive pieces are a top tier" is kept as ROADMAP's words but the code reads
"a nonzero key of a decisive piece settles the rank". diagram-viewer.js cites (dv:L-L) were re-read
against the file at 954b75c: all of the prototype's lines still say what it says. go-pflow cites carry
`@69257b3` (`composedot.go:28-33`, `compose_flatten.go:72-80, 424-482`, re-read there).
ROADMAP lines were re-read at f854116 (valoper).

## Drawn that the pattern manifest does not mention (unchanged, by D20 and D10)

Pattern level: nothing is added to or removed from the manifest. D20 says `backedge` `cutoff` is
emitted always; the prototype draws that edge only when `history` is in the chain, otherwise a label.
Here the edge is always drawn (game.rs calls `chain.cutoff` whatever the chain is; with no `history`
it ends at the chain's border, labelled "no piece records them"), so the always-emit rule and the drawing agree.
`mtd_f` is drawn as the inner frame and `aspiration` as a tab (no manifest entries: D20 lists
`budget` and `null_window` only).

## Proposed spec Decisions: the transition level's manifest

The transition level draws diagram-viewer.js's output, so its elements are exactly what
`translatePetriNet` returns. Proposal for D27 onwards (the lead decides; the goldens would gain 10
manifests):

- **D27.** A manifest has an optional `level` field. It is absent for every existing manifest (the
  pattern level, the default), so the 100 manifests in `goldens.json` stay byte-identical. The string
  view's transition level is a second manifest per example and state, `{view:"string", example,
  state, level:"transition", elements}`, sorted and deduplicated as ever. No other view has levels.
  `headless_manifest_at(sess, view, example, state, level)` makes them;
  `pflow-native --composition-goldens-transition OUT.json` writes the 10 (they are not in
  `goldens.json` until adopted).
- **D28 (orders, `translatePetriNet` on `bundle.flatten`).** The net is `bundle.flatten` as the prototype's
  `dvModel` reads it: read arcs left out, an arc weighs `weight || 1`, an inhibitor arc is an inhibitor.
  - `tbox`, id the flat transition (`fused:orders/dequeue+...`, `payment/commit`): attrs `layer`
    (int), `fused` (bool, the flat net's flag), `inputs` and `outputs` (port counts).
  - `twire`, id `{producer box}->{consumer box}:{place}` (the wire's two ends and the place it
    carries; the prototype labels the wire with the place): attrs `back` (bool), `inhibit` (bool),
    `weight` (int, the producer arc's first weight). Parallel arcs make one entry.
  - `tback`, id `{box}->{box}` for each pair the DFS marks as a back edge (`backEdges`, in discovery order
    in the JS; sorted here): no attrs.
  - `tboundary`, id the place: attr `side` `in` (enters the top: consumed, never produced) or `out`
    (leaves at the bottom: produced, never consumed).
- **D29 (Connect-4).** One entry: `tnet`, id `connect-4`, attrs `boxes`, `layers`, `wires`,
  `back_wires` (wires flagged `isBack`), `crossing_wires` (`isCrossing`), `boundary_in`,
  `boundary_out`. 222 boxes in 150 layers and 14,296 wires are too many to list (D21 treats the flat
  view's Connect-4 the same way). For the pin: 222, 150, 14296, 3663, 10519, 8, 2.
- **D30.** Layers and wires come from `translatePetriNet`'s own fields, so a web renderer that runs
  diagram-viewer.js emits them from its `diagram` object (`b.layer`, `w.isBack`, `w.isCrossing`,
  `diagram.boundaryInputs`, `diagram.backEdges`); only `fused` needs the flat model.

The numbers in a manifest are integers; positions are not in it.

## The toggle and the lens

- The toggle is two chips in the view, each queuing `view.composition {level}`. The hint line under them
  says what a drag does at that level, as the prototype's does.
- Connect-4 at the transition level: a scaled overview in five columns (16.8%) with the lens rectangle in
  each, and a 1:1 lens (the real boxes, ports and wires, hover info on each box). Click or drag the
  overview, or scroll over the lens: each queues `view.composition {lens_y}`. The overview draws wires as
  straight segments (14,296 of them; at 17% the curve is invisible), back edges in a faint red.

## Merge animations (never headless)

- Orders pattern, link added: the boxes the new link moved slide from the layout without that link (the
  layout is a pure function of subnets and links, so "before" is recomputed, not remembered) over the first
  45% of the animation; then the link draws itself on (an event link from both ends to the middle, a
  token/data/guard link end to end), its glyph pops, the two ports pulse, the two boxes flash in the
  link's colour. Instance added: the box drops in.
- Orders transition, event link added: dashed ghosts of the member boxes (from the previous flat net,
  `Data.prev_flat`) slide into the fused box, which flashes.
- Connect-4: an order piece slides in from the left and the chain re-flows; the memo drops in from the top;
  a driver frame's tab pops; the back edges fade in last.

`merge_animations_run_and_leave_the_manifest_alone` draws every view/level/state at five stages of
each event and checks the manifest equals the resting one.

## Gaps (honest)

- Hover on links is by distance to the polyline (6 px), not an SVG hit-line; the leak note, glyph, port and
  box regions are rectangles.
- Text is egui's, not the prototype's SVG sans-serif: widths differ a little, so the layout (which measures
  text, as the prototype does with canvas) differs by a few px from the web page. Positions are not in
  the manifest, so parity is unaffected.
- The pattern-level Connect-4 does not draw the plan's `root` slot (agent e added it to `SLOTS`; the
  prototype's picture has no root) (a `ms` budget is printed in the budget frame's tab and is otherwise not modelled).
- The transition-level orders view puts the diagram at 1:1 design scale and fits the whole stage; a net
  that grows (payment2, orders2, workers2) shrinks to fit rather than scrolling.
- Schema colours here follow the prototype's theme.js (`queue:items` blue, `resource` amber, `account`
  purple, `saga:outcome` teal); mod.rs's `schema_color` (used by the other views) has other values.
  Not changed there (another agent's file).

# Composition views: the scene manifest (parity contract)

The ten composition views are drawn twice: natively in pflow-native (egui) and in
pflow.xyz (SVG, JS). Both draw from the same registry commands (pflow-native core;
on the web through the vendored wasm shim). The parity check asks: **do the two
renderers draw the same things for the same model?** Coordinates, colours and fonts
are out of scope; the elements drawn and what each one says are in scope.

## How a manifest is made

Each renderer collects its manifest **while it draws**: every call that draws an
element listed below also appends that element's entry. A manifest is never
recomputed from the model on the side (that would test the model twice and the
renderer never).

    manifest = { "view": <id>, "example": "orders" | "connect4", "state": <state id>,
                 "elements": [ <entry>, ... ] }       // sorted by (kind, id), no duplicates
    entry    = { "kind": <kind>, "id": <string>, "attrs": { <string>: <string|number|bool|[string]> } }

Attrs hold only the fields listed for the kind (no extras), arrays sorted. Numbers are
integers. A view that does not apply to an example draws its "not applicable" note
and emits exactly one entry `{kind:"na", id:<view>}` (plus anything it still draws).

## States (both sides must reproduce them by commands only)

| state id | commands from a fresh session |
|---|---|
| `default` | `bundle.open {}` ; `plan.open {}` |
| `payment2-pushout` | default, then `bundle.add_instance {id:"payment2"}`, `bundle.add_link {kind:"token", from:"payment:account:customer", to:"payment2:account:customer"}` |
| `no-links` | default, then `bundle.clear_links` |
| `release-undo` | default, then `bundle.add_link {kind:"event", from:"payment:undo:reserve", to:"workers:release"}` |
| `plan-threats` | default, then `plan.add_piece {piece:"threats", slot:"order"}` |

## Views, ids and kinds

Ids use the registry's own names: subnet ids, `subnet:port` keys, link `key`s from
`bundle.state`, flat ids from `bundle.flatten`, piece and slot names from `plan.*`.

| # | view id | orders example: kinds (attrs) | connect4 example: kinds (attrs) |
|---|---|---|---|
| 1 | `cospan` | `subnet` (pattern); `port` id `s:p` (side: left\|right\|both\|top, target); `link` id key (kind, style: pushout\|ghost\|guard); `composite` id `bundle` (form: tensor\|pushout) | `net` id `connect-4` (places, transitions: counts), `na`-style note entry `{kind:"note", id:"closed"}` |
| 2 | `spiders` | `spider` id flat transition or wire id (colour: event\|token, legs: sorted members); `leak` id link key | `junction` id `tt`\|`history` (legs: pieces in the plan using it) |
| 3 | `operad` | `hole` id subnet; `junction` id link key (kind); `exposed` id `s:p` (port no link uses) | `frame` id driver; `hole` id slot (accepts); `piece` id `slot/name` |
| 4 | `string` | `box` id subnet; `wire` id `s:p` (schema); `fusion` id link key | `box` id `core`; `chain` id piece (index); `frame` id driver; `backedge` id `nullwindow`\|`recursion`\|`tt`\|`cutoff` |
| 5 | `flat` | `place` id flat (initial, fused: bool); `transition` id flat (fused: bool); `scenario` id (pass: bool, stopped_step) | `place`/`transition` counts only: one entry `{kind:"net", id:"connect-4", attrs:{places, transitions, arcs}}` |
| 6 | `boundaries` | `boundary` id event-class flat id (legs); `composite` id flat transition (members) | `na` |
| 7 | `lens` | from `bundle.lens`: `verdict` id `strict` (holds: bool, glued, legal_token, legal_data); `pair` id `s/t->u`; `step` id flat (composes: bool, split: bool, dead_forward: constraint id or ""); `fix` id `how:u` (holds: bool) | `frame` id driver; `lens` id piece; `memo` id `memo` (present: bool) |
| 8 | `incidence` | `matrix` id `flat` (rows, cols); `block` id subnet (rows, cols); `invariant` id constraint id (holds: bool) | `matrix` id `connect-4` (rows, cols) |
| 9 | `process` | `run` id scenario (fired: sorted flat ids, stopped_step) | `run` id `opening` (fired count) |
| 10 | `laws` | `theory` id `sync-spiders`; `rewrite` id name (status: valid\|invalid\|error); `cover` id rewrite name covering the live link order | `theory` id per plan theory; `rewrite` id name (status) |

Where a cell above is vague for the view as the prototype draws it, follow the
prototype's drawing (`~/Workspace/pflow-views-prototype/js/views/<view>.js`) and write
the choice into this file in the same commit, in a "Decisions" section below, so the
other renderer can match it. Both renderers then emit identical manifests or the parity
test fails.

## Where the goldens live

pflow-native generates `parity/composition-views/goldens.json` (all views × examples ×
states) from its egui renderer's manifest collector, run headless; pflow.xyz vendors it
next to its existing `parity/native/` goldens and a deno test compares the web
renderers' manifests with it, the same way `parity/native/native_parity_test.ts` does.
Besides `manifests` (10 views × 2 examples × 5 states = 100) the file has
`transition_manifests` (D27). A failing parity test is a finding about one renderer; it is
never fixed by regenerating the goldens.

## Decisions

Recorded by pflow-native (the egui renderer, `crates/app/src/compviews/`), which owns
this file. The readings pflow.xyz#122 proposed are adopted where they follow the
prototype (D7 transition ports `bottom`, D10, D14 y·M₀, D15 repeats kept, D16 L3 cover,
D19 six holes, D26 selected theory); its others already agree with D1–D26. Each decision follows what the prototype draws; where the prototype draws
more than the table names, only the table's kinds are emitted.

**General**

- D1. Ids are the registry's own strings, unaltered: subnet ids, `subnet:port` keys
  (`payment:account:customer`), link `key`s from `bundle.state`
  (`event:orders:dequeue->workers:acquire`), flat ids from `bundle.flatten`.
- D2. Every listed kind is emitted from the registry data alone: `bundle.state`,
  `bundle.flatten`, `bundle.incidence`, `bundle.lens` (with fixes), `bundle.run` for each
  of `bundle.scenarios`, `plan.state`, `plan.net`, `plan.opening`, and the Chyp export.
  Positions, colours and the inspector are not in the manifest.
- D3. "As the solver reads it": a plan list field that is absent stands for its default
  (`order`: `["tt_best","immediate_win","history"]`; `drivers`: `["null_window"]`;
  `filters`: `["must_win"]`, pflow-rs 7ba0824). Every c4 element that lists pieces or
  drivers uses those lists. A driver's name is the string, or the one key of
  `{"budget": …}`.
- D4. Notes (honesty / not applicable / analogy / judgment / warning) are drawn but not
  in the manifest, except the two entries the table names (`note` `closed`, `na`
  `boundaries`).
- D5. All states are drawn with the lens view in `strict` mode (`view.composition
  {lens}`); the mode changes the lens view's layout, not its manifest (D17).
- D6. Arrays are sorted as strings; a numeric attr is a JSON integer; an absent string
  attr is `""`, never null.

**orders**

- D7. `cospan` `port`: one entry per port (an `inout` port is drawn on both sides but is
  one entry, side `both`); side: a transition port → `bottom` (cospan.js `portSides`:
  event ports on the bottom edge, outside the cospan); a place port `in`→`left`,
  `out`→`right`, `inout`→`both`, `observe`→`top`. `link` style: token and data →
  `pushout`, event → `ghost`, guard → `guard`. `composite` form is `pushout` when any
  token or data link exists (two or more instances glued), else `tensor`.
- D8. `spiders` `spider`: one per class with two or more members — each
  `fused_groups` entry (colour `event`) and each `wires` entry (colour `token`); legs
  are the class's members (`subnet/local`). Singletons are not spiders. `leak`: every
  link whose `leak` is true in `bundle.state`.
- D9. `operad` `hole`: one per subnet. `junction`: one per link (all kinds, guard
  included; attr kind). `exposed`: every port (`s:p`) that no link's from or to names.
- D10. `string` `box`: one per subnet; `wire`: one per place port (schema-typed; a
  transition port is drawn as a stub, not a wire); `fusion`: one per link, every kind.
- D11. `flat` `place`: initial = the flat initial marking, fused = more than one member;
  `transition` fused = `bundle.flatten`'s `fused`. `scenario`: one per
  `bundle.scenarios` entry; pass = every predicate passes (a run that stops can still
  pass); stopped_step = the member it stopped at, or `""`.
- D12. `boundaries`: `boundary` and `composite` are one each per `fused_groups` entry
  (same id), both with the members (`legs`, `members`). Singleton transitions are not
  drawn as boundary ports.
- D13. `lens`: `verdict` `strict` from `bundle.lens`'s analysis (legal_token and
  legal_data are its `legal.token` / `legal.data`; glued its `glued`). `pair` id is
  `{subnet}/{t}->{u}` with t and u local (`payment/do_charge->undo_charge`). `step`
  id is the flat transition; dead_forward is `dead_forward.constraint` or `""`. `fix`
  id `{how}:{u}` with u qualified (`add:workers/release`); holds = the re-run
  analysis's `holds`, false when there is none (refused or not flattening).
- D14. `incidence`: `matrix` `flat` and `block` per subnet from `bundle.incidence`
  (rows = places, cols = transitions); `invariant` per flat constraint, holds =
  `bundle.incidence`'s `holds`: y·C = 0 **and** y·M₀ = n, y the 1-vector on the places
  the constraint names (incidence.js: "the P-invariant check (y·C = 0 and y·M₀ = n)").
  `null` never occurs for these patterns. A token link can break one
  (`payment/saga_conservation` is false in `payment2-pushout`).
- D15. `process` `run`: one per scenario; fired = the flat ids of the steps that fired,
  sorted, repeats kept; stopped_step as D11.
- D16. `laws` (orders): `theory` `sync-spiders`; `rewrite` for each of its rewrites
  (status = the export's `status`). `cover` (laws.js `liveDefaultRewrite` and :1029),
  with L1 = `orders:dequeue~workers:acquire`, L2 = `orders:dequeue~payment:do:reserve`,
  L3 = `payment:commit~workers:release` among the event links in order: `fuse_L1_L2`
  when L1 comes before L2, `fuse_L2_L1` when after; plus `L3_position_irrelevant` when L1,
  L2 and L3 are all there; no `cover` entry when L1 or L2 is missing.

**connect4**

- D17. `cospan`: `net` `connect-4` (places, transitions from `plan.net`) and
  `{kind:"note", id:"closed"}`; no `na` entry (the table lists the note instead).
- D18. `spiders`: two `junction`s. `tt` legs: `tt_best` if in the order chain (D3) and
  `memo` if the plan has a memo; `history` legs: `history` if in the chain.
- D19. `operad`: `frame` per driver that is on (D3); `hole` per core hole: `order`,
  `filters`, `root`, `heuristics`, `memo` and the designed `bounds`, `engine` (seven; attr
  accepts; `root` accepts `Root`, pflow-rs's `root` field, D31). Drivers are frames, not a hole
  (the drivers drop target is drawn but not emitted). `piece` `{slot}/{name}` for each entry of
  `order`, `filters`, `root`, `heuristics` as the solver reads them (D3, so `filters/must_win` by
  default; `root` has no default, so none unless the plan names one), and `memo/memo` when the
  plan has a memo.
- D20. `string`: `box` `core`; `chain` per order piece (D3) with its index; `frame` per
  driver that is `budget` or `null_window`; `backedge` `recursion`, `tt` and `cutoff`
  always, and `nullwindow` when `null_window` is on.
- D21. `flat`: one `net` entry (places, transitions, arcs from `plan.net`).
- D22. `boundaries`: `{kind:"na", id:"boundaries"}` only.
- D23. `lens`: `frame` per driver (D3); `lens` per order piece (D3), no attrs; `memo`
  `memo` (present).
- D24. `incidence`: `matrix` `connect-4` (rows = places, cols = transitions).
- D25. `process`: `run` `opening` with fired = `plan.opening`'s `fired` (4 at the pin).
- D26. `laws` (connect4): `theory` per own tab (`orderer-monoid`, `filter-meet`,
  `driver-wrap`); `rewrite` only for the selected theory's chips (the prototype draws one
  theory at a time). With no rewrite chosen (`view.composition {rewrite}`), the selected
  theory is the first, `orderer-monoid`, so the states emit its 8 rewrites. No `cover`.

**Added with the quality pass (pflow-native feat/composition-views-quality)**

- D27. A manifest has an optional `level` field. It is absent for every manifest above (the
  pattern level, the default), so those stay as they were. The string view's transition level is
  a second manifest per example and state, `{view:"string", example, state, level:"transition",
  elements}`, sorted and deduplicated as ever. No other view has levels. In `goldens.json` they
  are the array `transition_manifests` (10 entries: orders and connect4 × the five states), apart
  from `manifests` (still 100) so a consumer keyed by (view, example, state) is unaffected.
- D28 (string, transition level, orders: `translatePetriNet` of diagram-viewer.js on
  `bundle.flatten`). The net is `bundle.flatten` as the prototype's `dvModel` reads it: read arcs
  left out, an arc weighs `weight || 1`, an inhibitor arc is an inhibitor.
  - `tbox`, id the flat transition (`fused:orders/dequeue+...`, `payment/commit`): attrs `layer`
    (int), `fused` (bool, the flat net's flag), `inputs` and `outputs` (port counts).
  - `twire`, id `{producer box}->{consumer box}:{place}`: attrs `back` (bool), `inhibit` (bool),
    `weight` (int, the producer arc's first weight). Parallel arcs make one entry.
  - `tback`, id `{box}->{box}` for each pair the DFS marks as a back edge: no attrs.
  - `tboundary`, id the place: attr `side` `in` (consumed, never produced: enters the top) or
    `out` (produced, never consumed: leaves at the bottom).
- D29 (string, transition level, connect4). One entry: `tnet`, id `connect-4`, attrs `boxes`,
  `layers`, `wires`, `back_wires` (wires flagged `isBack`), `crossing_wires` (`isCrossing`),
  `boundary_in`, `boundary_out`. 222 boxes in 150 layers and 14,296 wires are too many to list
  (D21 treats the flat view's Connect-4 the same way). At the pin: 222, 150, 14296, 3663, 10519,
  8, 2.
- D30. Layers and wires come from `translatePetriNet`'s own fields, so a web renderer that runs
  diagram-viewer.js emits them from its `diagram` object (`b.layer`, `w.isBack`, `w.isCrossing`,
  `diagram.boundaryInputs`, `diagram.backEdges`); only `fused` needs the flat model. pflow-native
  runs the same algorithm in Rust (`core::string_diagram`, `diagram.transition_layout`), checked
  number for number against diagram-viewer.js's own output
  (`crates/core/tests/fixtures/diagram-viewer/`).
- D31. pflow-rs's plan `root` field (`{"order": ["ode", "non_losing"]}`, an estimate and the
  root's `non_losing`; at the pin aacb7eb) is a slot of the solver template: `plan.pieces`
  lists eight slots, `plan.add_piece {slot:"root"}` takes `ode` or `non_losing`, and every
  verdict is pflow-rs's (`pflow_solver::game::solver` builds the `ode` estimate). The operad
  view draws it as a hole (D19).
- D32. The flat view's token game is the registry's (`bundle.fire`, `bundle.step`,
  `bundle.scenario`, `bundle.reset_marking`; `plan.play`, `plan.new_game` for connect4). A
  renderer never fires on its own. The manifest is collected in the resting state:
  `place.initial` is the flat initial marking whatever the current marking, and `scenario`
  entries are `bundle.run`'s. The same holds for the process view's chosen scenario and the laws
  view's chosen step (`view.composition {scenario, step}`): what is selected is drawn, the
  manifest keeps D15 and D16/D26.

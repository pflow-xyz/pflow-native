# cv-a: operad view notes

No new manifest kinds. `operad` emits what it emitted before (hole / junction / exposed for
orders; frame / hole / piece for connect4): the six goldens-states x two examples are
byte-identical (`--composition-goldens` diffed against `parity/composition-views/goldens.json`).

Drawing notes the web renderer may want to match (not in the manifest):

- orders: one junction dot per fused event class (3 event links in the default state draw
  2 dots), but `junction` is still emitted once per link key (D9). Token/data/guard links are
  orthogonal cables, not junctions, and are emitted per link as before.
- connect4: the drivers are frames, drawn budget (outer), aspiration (if on), then null_window
  or mtd_f, then the core. A frame is *emitted* only when the driver is on (D19); the dashed
  "driver off" frames are drawn, not emitted. The nesting of `aspiration` is this view's
  reading (plan.rs `normalized()` lists null_window, aspiration, budget).
- connect4: holes are drawn from `sp::SLOTS`: `order` alone in row 1, the non-designed slots
  other than drivers/order in row 2, the designed slots in row 3. A `root` slot added to
  SLOTS later appears in row 2 and is emitted as a `hole` by the same loop; the lead should
  decide whether D19's "six holes" becomes seven then.
- The prototype's text about filters ("exact mode refuses every filter") is out of date at the
  pinned pflow-rs: `must_win` (default) and `non_losing` are built and legal in either mode,
  `beam` is the reserved filter, and `static_eval` the only heuristic name (refused). The
  filters hole therefore draws its pieces (`filters/must_win` by default).

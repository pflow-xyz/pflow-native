# diagram-viewer.js layout fixtures

Golden outputs of pflow.xyz's own `public/diagram-viewer.js`
(`translatePetriNet`, run unmodified under node) for the nets the string
view draws at its transition level. `crates/core/tests/diagram_viewer_fixtures.rs`
compares `core::string_diagram::translate` with them number for number (box
x/y/width/layer/position, port lists, every wire's ends and `isCrossing` /
`isBack`, boundaries, back edges, size).

## Provenance

| | |
|---|---|
| generator | `gen.mjs` (this directory): `node gen.mjs <pflow-xyz>/public/diagram-viewer.js` |
| source | `pflow-xyz/public/diagram-viewer.js`, last changed in pflow-xyz commit `6241e4bf169244196574471e9306f8c90a5b4e7c` (2026-02-27); sha256 `e9bb675e2011d971e26ba311178a4db8f35546de57d0c3e489f5e81ee12b2384`; byte-identical to `pflow-views-prototype/vendor/diagram-viewer.js` |
| node | v23.9.0 |
| run on | 2026-10-28 |
| licence | pflow-xyz is MIT (Copyright (c) 2025 pflow); the *output* here is data computed from the nets below, no JS is vendored |

## Inputs

| fixture | input | what it is |
|---|---|---|
| `orders-default` | `orders-default.flat.json` | `bundle.flatten` after `bundle.open {}` (the flat orders net: 21 places, 9 transitions) |
| `orders-payment2-pushout` | `orders-payment2-pushout.flat.json` | `bundle.flatten` after `bundle.add_instance {id:"payment2"}` and the token link `payment:account:customer` to `payment2:account:customer` (the spec's `payment2-pushout` state) |
| `connect-4` | `examples/connect-4.json` | the Connect-4 net (222 transitions, 14,296 wires, 3,406 back edges); `input.bytes` guards against drift |
| `sir`, `coffee-shop` | `sir.net.json`, `coffee-shop.net.json` | copies of `examples/` |
| `mixed` | `mixed.net.json` | array-index ids, a cycle through a parallel arc, inhibitors, a weight-3 arc |

A `.flat.json` is converted to diagram-viewer.js's model exactly as
`pflow-views-prototype/js/views/string.js` `dvModel` does (read arcs left out,
weight `[a.weight || 1]`, inhibitor flag); `core::string_diagram::flat_document`
is the Rust twin of that conversion. The tests also check that
`bundle.flatten` still returns the `.flat.json` inputs.

## Regenerating

```
node gen.mjs ~/Workspace/pflow-xyz/public/diagram-viewer.js
```

A failing comparison is a finding about the port, never fixed by
regenerating (unless the inputs changed on purpose: then say so in the commit).

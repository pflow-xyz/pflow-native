# Chyp export (precomputed)

The Laws view draws Chyp's verdicts and rewrite diagrams from these files. **Nothing here is
re-checked at run time**: pflow-native does not reimplement Chyp's checker (Chyp is Python).

- Source: `pflow-views-prototype` `data/chyp/` (commit `91695ad`), copied unmodified.
- Made by `chyp_headless 0.1.0` (the prototype's headless wrapper, which imports Chyp's core
  unmodified) from the theories in the prototype's `chyp/*.chyp`, with
  [Chyp](https://github.com/akissinger/chyp) commit `3d2848361e261b07a86daaacf70b2b4b018affcf`
  (version 0.5.2). `index.json` and each file's `provenance` record the generator, the Chyp
  modules used and the layout engine.
- Chyp is Copyright (C) 2022-2023 Aleks Kissinger, licensed under the Apache License 2.0
  (`LICENSE-chyp`, copied from the prototype's `js/layout/LICENSE-chyp`). Chyp ships no NOTICE
  file at 3d28483. The diagrams' coordinates are Chyp's own layout output
  (`chyp/layout.py` convex layout); the rewrite statuses are Chyp's checker's verdicts.

| file | sha256 |
|---|---|
| `driver-wrap.json` | `74d988e3f1a49a7e76ef0cfd86c7cf4547353a8087e2c927ba3ab6dcef06da29` |
| `filter-meet.json` | `130bf908e93bf8849b9f43f490977ebdeac45275a92f848a70c451e94ca00445` |
| `index.json` | `96ae257fd44ff7f682d01a3e8d46ffd27f0709802fe442966225d6f0a1744697` |
| `orderer-monoid.json` | `877ea9f247c3c2444c0d4cc10e0ac232cda3e30c2aef7fc6878b44d08e64bd1c` |
| `sync-spiders.json` | `a84dc3fad84c35411cef1515ae28c280a2804a20a9470441c9e1bc903c60fd68` |

## Layout receipts (`receipts/`)

`receipts/chyp-layout.json` records what Chyp's own layout does on ten diagrams (Chyp's README and
examples, plus shapes from the prototype): the hypergraph, the layers `layer_decomp` made, Chyp's
`convex_layout` coordinates and its objective (`problem.value`, recomputed by the script). It is
copied unmodified from `pflow-views-prototype` `js/layout/receipts/` (made by
`chyp_layout_receipt.py`, copied beside it, with Chyp 3d28483, cvxpy 1.9.3, lark 1.3.1). The app does
not read it at run time: `crates/app/src/compviews/c_hypergraph.rs` (a port of the prototype's
`js/layout/hypergraph.js`) is held to it by tests (same layers as Chyp; given Chyp's layers and order,
the L1 objective reaches cvxpy's optimum within 0.5%).

| file | sha256 |
|---|---|
| `receipts/chyp-layout.json` | `ce37ba04af507643b201a9043cb1a84c25f5f3542c177ecfa23f66296eba3115` |
| `receipts/chyp_layout_receipt.py` | `56537c26a4a32d1e6949441c5773b4b7726dd5fde1959a38b8dfa598b4ed62ae` |

The data is Chyp's output (Copyright (C) 2022-2023 Aleks Kissinger, Apache-2.0; `LICENSE-chyp`).

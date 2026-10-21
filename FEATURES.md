# Feature parity matrix

This file tracks what pflow-native must reach:

- every user-facing feature of the **pflow.xyz editor**, from `pflow-xyz/public/petri-view.js` v1.28.0 and its sibling modules;
- every tool of the **sim.pflow.xyz MCP server**, from `sim-pflow-xyz/pkg/simmcp` (56 tools);
- the **visualizations and exports** for the data both produce.

How to read it:

- **Source:** where the feature lives today. `pv:` means `petri-view.js` line numbers; `sim_*` is the MCP tool name.
- **Native:** the registry command(s) and/or the view that will carry the feature (see `ARCHITECTURE.md`).
- **Backing:** the pflow-rs crate that does the work. **port** means pflow-rs has no equivalent yet; **remote** means it calls sim.pflow.xyz.
- **Status:** ✅ done, 🟡 partial, blank = planned. Phase 1 shipped in v0.1.0.

## Phases

| Phase | Theme | Exit criterion |
|---|---|---|
| **1 MVP** | Editor core, token game, ODE, registry + CLI/control/MCP, macros, artifact/view skeleton | Draw, fire and simulate a net on macOS. An agent drives the same commands headless |
| **2 Editor parity** | Everything else in the pflow.xyz editor, plus the web build | A pflow.xyz user loses nothing by switching |
| **3 Simulation workbench** | Rates, scans, sweeps, phase plots, SSA/SDE scenarios, compare, heatmap, optimize | Matches the editor's Analysis workbench and sim's scenario family |
| **4 Analysis & verification** | Incidence, invariants, siphons/traps, reachability, verify, diagnose, canonical/classify | Matches sim's analysis family; results shown on the canvas |
| **5 Data & learning** | Event logs, conformance, calibrate, mining, distill | Matches sim's data family |
| **6 Composition & systems** | Components, compose, extend, diff, bindings, systems, pipelines | Matches sim's composition family |
| **7 Store, share, provenance** | Local CID store, pflow.xyz/sim bridges, publish, knowledge graph, receipts, ZK, LLM tools | Matches sim's store family; signed or proven results |

## A. pflow.xyz editor

### A1. Editing

| Feature | Source | Native | Backing | Phase | Status |
|---|---|---|---|---|---|
| Tool modes select/place/transition/arc/token/delete (keys 1–6) | pv:5–49, 3171, 3426 | `tool.set {tool}`, toolbar | core | 1 | ✅ |
| Click to add place / transition | pv:3599 | `net.add_place`, `net.add_transition {x,y}` | core | 1 | ✅ |
| Click-click arc; reject same-kind/self-loop | pv:4789 | `net.add_arc {source,target,weight,inhibit}` | core | 1 | ✅ |
| Inhibitor arc (input) / read arc (output-side inhibit) | pv:2841, 2960; petri-sim.js:94 | `net.add_arc {inhibit}`, `net.set_arc` | pflow-parser → metamodel | 1 | ✅ |
| Toggle inhibitor on an existing arc | pv:2971 | `net.set_arc {inhibit}` (inspector) | core | 1 | ✅ |
| Arc weight, per-color vector | pv:2971, 3020 | `net.set_arc {weight}` | core | 1 | ✅ |
| Token tool: +1 / −1 on click / right-click | pv:2787, 2841 | `net.add_tokens {id, delta, color}` | core | 1 | ✅ |
| Capacity per color (0 = unbounded); clamp initial | pv:1936, 4685 | `net.set_place {capacity}` (inspector); full places outlined red | core | 1 | 🟡 token tool refuses over capacity; no clamp on load |
| Delete node (and its arcs), delete arc, Delete/Backspace | pv:3051, 3072, 2978 | `edit.delete`, `net.delete_arc` | core | 1 | ✅ |
| Labels, validated (≤100 chars, single line) | pv:3545, 3558 | `net.set_label` | core | 1 | ✅ |
| Shift-click multi-select | pv:2798 | `edit.select {ids, add}` | core | 1 | ✅ |
| Box select (shift-drag) | pv:3512, 7838 | gesture → `edit.select {ids}` | app | 1 | ✅ |
| Group drag, 10px grid snap | pv:3665–3929 | `net.move_by {ids,dx,dy}` (snapped) | core | 1 | ✅ |
| Undo/redo (full snapshots) | pv:2144–2168 | `edit.undo`, `edit.redo` | core | 1 | ✅ |
| Token colors: `token[]`, palette, arc color by weight, radial breakdown | pv:4246–4710; petri-colors.js | `net.set_colors`, `net.add_tokens {color}`; canvas | core + pflow-core colors | 2 | 🟡 read/write/fire, named colors, per-color counts, arc colors; no editing of `token[]`, no radial |
| Parallel arcs as curves | pv:4356 | canvas | app | 2 | 🟡 opposite-direction pairs curve |
| Edit details (name, description) | pv:6945 | `net.set_details` | core | 2 | |
| Rename node id | — | `net.rename` | core | 2 | |
| Copy/paste/duplicate (not in pflow.xyz) | — | `edit.copy/paste/duplicate` | core | 2 | |

### A2. Views & panels

| Feature | Source | Native | Backing | Phase | Status |
|---|---|---|---|---|---|
| Canvas: places, transitions, arcs, tokens, enabled highlight | pv:4421, 4668, 4695 | Net view | app | 1 | ✅ |
| Wheel zoom, pan (drag empty / middle / space), pinch | pv:7594, 7878, 7611 | `view.zoom`, `view.pan`; gestures | app | 1 | ✅ |
| Fit to view, center on node, zoom readout | pv:4186, 4231, 4849 | `view.fit`, `view.center {id}`; status bar % | core | 1 | ✅ |
| JSON-LD editor pane, two-way live sync | pv:360–400, 7462 | JSON view → `file.load_json` | app | 2 | |
| Split layout / toggle editor pane | pv:913–1124 | dock tabs, `view.open/close` | app | 2 | |
| Dark/light theme | pv:965 | `view.theme` | app | 2 | |
| Fireable list + locate, step counter, reset | pv:3220–3338 | `sim.enabled`, `view.center`; Run panel | core | 1 | ✅ |
| Display settings (hide uniform weights) | pv:2500 | `view.set {hide_unit_weights}` | app | 2 | |
| Toasts, help dialog | pv:5171, 4961 | status bar, Help → `command.list` | app | 1 | 🟡 |
| String-diagram view + SVG | pv:6324; diagram-viewer.js | String-diagram view | port | 4 | |

### A3. Simulation (editor)

| Feature | Source | Native | Backing | Phase | Status |
|---|---|---|---|---|---|
| Token game: fire on click, blocked feedback, baseline reset | pv:2215–2911 | `sim.fire {id}`, `sim.reset`, Fire tool, double-click | pflow-metamodel `Model::enabled/fire` | 1 | ✅ |
| ODE (Tsit5) + plot + initial/final/Δ table | pv:5268; petri-solver.js | `sim.ode {t_end, method}` → TimeSeries view | pflow-solver | 1 | ✅ plot + final values in legend; no Δ table |
| Per-transition rates (session), rate 0 disables | pv:5287 | `sim.set_rate {id, rate}` | core | 1 | ✅ |
| Solver options dt/abstol/reltol | pv:5287–5300 | `sim.ode {dt, abstol, reltol}` | pflow-solver Options | 3 | |
| Place checkboxes on plot, legend toggle, hover | pv:593 | TimeSeries view | app | 3 | 🟡 legend toggle + hover |
| Rate scan (steady state vs rate) | pv:5268 | `opt.rate_scan` → TimeSeries(x=rate) | pflow-solver equilibrium | 3 | |
| Sweep (one place across rates) | pv:5268 | `opt.sweep` → overlay | pflow-solver | 3 | |
| Phase plot | pv:5268 | Phase view over a TimeSeries | app | 3 | |
| CSV / JSON download, "copy MCP call" | pv:5615–5620 | `export.artifact {format}`, `command.copy_call` | core | 1 | 🟡 csv/json |
| SSA / SDE engines (library only in JS) | petri-ssa.js, petri-sde.js | `sim.ssa`, `sim.sde` | pflow-solver ssa / ssa::sde | 3 | |
| Rate fitting (library only in JS) | petri-learn.js:1863 | `opt.fit` | pflow-learn | 5 | |

### A4. I/O & sharing

| Feature | Source | Native | Backing | Phase | Status |
|---|---|---|---|---|---|
| Open/save pflow JSON-LD | pv:1320 | `file.open`, `file.save`, `file.load_json`, `file.export_json` | core | 1 | ✅ |
| CID (URDNA2015 → CIDv1) as `@id`, `{cid}.jsonld` | seal-cid.mjs | `file.seal_cid` | port (or pflow-tokenmodel) | 2 | |
| Open from URL, `?data=` / `?cid=` permalink | pv:661, 2008 | `file.open_url`, `file.open_permalink` | app (http) | 2 | |
| Autosave | pv:2117 | app autosave → recovery file | app | 2 | |
| Example gallery | index.html:766 | `file.open_example {name}`, File → Examples | bundled | 2 | 🟡 2 of 6 bundled |
| Server save / delete / My Diagrams (`/api/save`, `/o/{cid}`) | pv:1345, 1415, 6603 | `remote.pflow.save/list/delete` | remote | 7 | |
| Share dialog (badge, `/img/{cid}.svg`), Gist, GitHub login | pv:1506, 1709, 214 | `remote.pflow.share`, `remote.gist` | remote | 7 | |
| Layout algorithms: Sugiyama, grid, bipartite, circular | pv:5837–6320 | `net.layout {algorithm}` (`core::layout`, matches the JS output exactly) | port | 2 | ✅ |
| SVG export of the net | server svg.go | `export.svg` | core (no egui) | 1 | ✅ |
| PNG export | — (not in pflow.xyz) | `--screenshot`, `view.screenshot` | app | 1 | ✅ |

## B. sim.pflow.xyz tool surface

| Tool(s) | What | Native | Backing | Output view | Phase |
|---|---|---|---|---|---|
| create_model, get_model, list_models, delete_model | content-addressed store | `store.put/get/list/delete` | pflow-tokenmodel CID + local store | — | 7 |
| extend | structural edit ops + diff | `compose.extend {ops}` (each op = a `net.*` command) | pflow-compose | Net diff overlay | 6 |
| diff | structural diff | `compose.diff {a,b}` | port (pflow-mcp diff.rs) | Diff overlay, Table | 6 |
| components, compose | template library, port fusion | `compose.components`, `compose.fuse` | pflow-compose templates | Net | 6 |
| code_to_flow, prompt, reroll, get_artifact | LLM derivation | `remote.sim.prompt` … | remote | Net | 7 |
| scenario | seeded SSA/ODE what-if | `sim.scenario {scenario}` | pflow-solver ssa + **port** (contention/depletion) | TimeSeries + bands, Table | 3 |
| compare | N scenarios, shared seed | `sim.compare {scenarios}` | same | TimeSeries overlay | 3 |
| receipt | signed run certificate | `sim.receipt` | port (ed25519) | Report | 7 |
| evaluate | score moves by rollouts | `sim.evaluate {player}` | pflow-solver ssa | Table (ranked) | 3 |
| crosscheck | SSA vs ODE vs conservation | `sim.crosscheck` | pflow-solver + pflow-reachability | TimeSeries overlay, Report | 3 |
| dataset | synthetic event log | `data.generate {cases, seed}` | pflow-solver ssa + pflow-eventlog | Table, CSV export | 5 |
| run_pipeline | bound models, loop fixed point | `compose.run_pipeline` | port | TimeSeries per model, Graph | 6 |
| diagnose | resource/rate influence, knee, lumpability | `analysis.diagnose` | pflow-results sensitivity + port | Tornado (bars), curve, Report | 4 |
| invariants | P/T invariants, siphons/traps, deadlock witnesses | `analysis.invariants`, `analysis.siphons` | pflow-reachability (+ **port** siphons/traps) | Matrix, canvas overlay | 4 |
| — (incidence) | incidence matrix | `analysis.incidence` | pflow-zk IncidenceMatrix / metamodel | Matrix | 4 |
| verify | deadlock-free, bounded, live, reachable, invariant, mutex | `analysis.verify {properties}` | pflow-verify | Report + counterexample Trace | 4 |
| reach | reachability with firing trace | `analysis.reach {from,to}` | pflow-reachability | Trace (replay on canvas) | 4 |
| — (graph) | full reachability / coverability graph | `analysis.reachability_graph` | pflow-reachability | Graph view | 4 |
| check_witness | re-check witnesses and derivations | `analysis.check_witness` | port (morphism) / remote | Report | 7 |
| canonical, classify, refine, propose_types | automorphisms, orbits, parameter classes | `analysis.canonical`, `analysis.classify`, `net.refine` | **port** (orbits.go, classify.go) | canvas orbit coloring | 4 |
| prove | isomorphic / subnet / refines / preserves invariant | `analysis.prove` | **port** (pkg/morphism) / remote | side-by-side nets + mapping | 7 |
| query, edges, neighbors, link | knowledge-graph edges | `store.query`, `store.link` | local store / remote | Graph | 7 |
| create_collection, license_model, supersede_model, migration_offers, accept_migration | collections, licenses, versioning | `store.*` / `remote.sim.*` | remote | Table | 7 |
| map_put, map_get, map_list | content-addressed tables | `store.map_*` | local store | Table | 7 |
| bind, get_binding, list_bindings | checked port bindings (units, delay) | `compose.bind` | **port** | Graph | 6 |
| create_system, system | stored model+binding sets | `compose.system` | **port** | Graph (system) | 6 |
| conformance | fitness/precision/generalization/simplicity | `data.conformance {log}` | pflow-mining, pflow-eventlog | Table + residual on canvas | 5 |
| conformance_stream | online replay, first divergence | `data.replay {events}` | pflow-mining | Trace + running-fitness TimeSeries | 5 |
| calibrate | learn rates/delays from a log | `data.calibrate {log}` | pflow-learn, pflow-mining | Table (evidence), new model | 5 |
| optimize | Monte Carlo sweep, Pareto | `opt.optimize {parameters, objectives}` | pflow-solver | Scatter + Pareto | 3 |
| param_heatmap | two-rate grid → final value | `opt.heatmap {param_x,param_y,observable}` | pflow-solver | Heatmap | 3 |
| distill | fit rate multipliers to minimax | `opt.distill` | pflow-learn | Report | 5 |
| publish, publish_compare, my_sheets | Google Sheets | `remote.sim.publish` | remote | URL | 7 |
| publish_app | host an HTML app | `remote.sim.publish_app` (wasm build) | remote | URL | 7 |

## C. Visualizations and exports

| View | Fed by | Export | Phase | Status |
|---|---|---|---|---|
| Net graph (+ marking, enabled, overlays) | document | SVG (core), PNG (app), JSON | 1 | ✅ (overlays 4) |
| Time series (lines; mean±sd bands; overlays) | ODE, SSA, scenario, compare, pipeline, crosscheck | CSV, JSON, SVG | 1 | ✅ lines, CSV/JSON |
| Phase plot (X vs Y) | any TimeSeries | SVG, CSV | 3 | |
| Heatmap | param_heatmap | CSV, JSON, SVG | 3 | |
| Scatter / Pareto | optimize | CSV, JSON | 3 | |
| Matrix table (incidence, P/T invariants) | analysis | CSV, JSON | 4 | |
| Reachability / coverability graph | analysis | JSON, DOT, SVG | 4 | |
| Trace / timeline (replayable on canvas) | reach, verify, replay, token game history | JSON, CSV | 4 | |
| Tornado / ranked bars | diagnose, evaluate | CSV | 4 | |
| Table (conformance, calibrate, dataset) | data | CSV | 5 | |
| System / pipeline / knowledge graph | compose, store | JSON, SVG | 6–7 | |
| Side-by-side mapping | prove, diff | SVG | 6–7 | |

## D. Gaps in pflow-rs (work that lands upstream, not here)

1. Stochastic analysis over `ssa`: contention, depletion, supply kinds, schedules/stages. Source: go-pflow `stochastic`.
2. Siphon and trap search. Home: pflow-reachability.
3. Exact automorphisms, canonical labelling, color refinement. Source: sim `orbits.go`, `classify.go`.
4. Morphism prover/checker. Source: sim `pkg/morphism`.
5. Port bindings, pipelines, loop fixed point. Source: sim `bindcheck`, `pipeline`, `loop`.
6. pflow.xyz layout algorithms (Sugiyama, grid, bipartite, circular) and URDNA2015 CID sealing.

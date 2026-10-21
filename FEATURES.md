# Feature parity matrix

This file tracks what pflow-native must reach:

- every user-facing feature of the **pflow.xyz editor**, from `pflow-xyz/public/petri-view.js` v1.28.0 and its sibling modules;
- every tool of the **sim.pflow.xyz MCP server**, from `sim-pflow-xyz/pkg/simmcp` (56 tools);
- the **visualizations and exports** for the data both produce.

How to read it:

- **Source:** where the feature lives today. `pv:` means `petri-view.js` line numbers; `sim_*` is the MCP tool name.
- **Native:** the registry command(s) and/or the view that will carry the feature (see `ARCHITECTURE.md`).
- **Backing:** the pflow-rs crate that does the work. **port** means pflow-rs has no equivalent yet; **remote** means it calls sim.pflow.xyz.
- **Status:** ✅ done, 🟡 partial, blank = planned. Phase 1 shipped in v0.1.0, phase 2 in v0.2.0, phase 3 in v0.3.0.

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
| Token colors: `token[]`, palette, arc color by weight, radial breakdown | pv:4246–4710; petri-colors.js | `net.set_colors`, `net.add_tokens {color}`; token-tool color picker; canvas | core + pflow-core colors | 2 | ✅ per-color counts instead of radial breakdown |
| Parallel arcs as curves | pv:4356 | canvas | app | 2 | ✅ (duplicate same-direction arcs are refused; opposite pairs curve) |
| Edit details (name, description) | pv:6945 | `net.set_details`; Net → Edit Details | core | 2 | ✅ |
| Rename node id | — | `net.rename`; inspector Id field | core | 2 | ✅ |
| Copy/paste/duplicate (not in pflow.xyz) | — | `edit.copy/cut/paste/duplicate` | core | 2 | ✅ |

### A2. Views & panels

| Feature | Source | Native | Backing | Phase | Status |
|---|---|---|---|---|---|
| Canvas: places, transitions, arcs, tokens, enabled highlight | pv:4421, 4668, 4695 | Net view | app | 1 | ✅ |
| Wheel zoom, pan (drag empty / middle / space), pinch | pv:7594, 7878, 7611 | `view.zoom`, `view.pan`; gestures | app | 1 | ✅ |
| Fit to view, center on node, zoom readout | pv:4186, 4231, 4849 | `view.fit`, `view.center {id}`; status bar % | core | 1 | ✅ |
| JSON-LD editor pane, two-way live sync | pv:360–400, 7462 | JSON pane (⌘J) → `file.load_json {live}`, debounced | app | 2 | ✅ (no find/fullscreen) |
| Split layout / toggle editor pane | pv:913–1124 | `view.toggle_*`, `view.set {json_pane, inspector, bottom_panel}` | app | 2 | ✅ panel toggles; no dock tabs yet |
| Dark/light theme | pv:965 | `view.set {theme}`, `view.cycle_theme` | app | 2 | ✅ |
| Fireable list + locate, step counter, reset | pv:3220–3338 | `sim.enabled`, `view.center`; Run panel | core | 1 | ✅ |
| Display settings (hide uniform weights) | pv:2500 | `view.set {show_unit_weights, snap}` | app | 2 | ✅ |
| Toasts, help dialog | pv:5171, 4961 | status bar, Help → Keyboard & Mouse, ⌘K palette | app | 1 | ✅ |
| Command palette (not in pflow.xyz) | — | ⌘K over the whole registry; args form generated from each command's `ArgSpec`s | app | 2 | ✅ |
| Agents drive the live window (not in pflow.xyz) | — | app control port `127.0.0.1:7979` (JSON lines); `pflow-native-cli mcp` attaches to it, `screenshot` tool | app + cli | 2 | ✅ |
| Web build | — | Trunk (`make web`), eframe WebRunner, browser-storage autosave | app | 2 | ✅ (no file dialogs or HTTP fetch yet) |
| String-diagram view + SVG | pv:6324; diagram-viewer.js | `view.string_diagram` → drawing artifact (`core::string_diagram`), SVG via `export.artifact`; the pv:6381 net layout is `string_diagram::net_layout` | port | 4 | 🟡 core: boxes, wires, ports, labels, back edges match the JS element for element (Béziers flattened, arrowhead outlined); net layout matches but is not yet a `net.layout` algorithm |

### A3. Simulation (editor)

| Feature | Source | Native | Backing | Phase | Status |
|---|---|---|---|---|---|
| Token game: fire on click, blocked feedback, baseline reset | pv:2215–2911 | `sim.fire {id}`, `sim.reset`, Fire tool, double-click | pflow-metamodel `Model::enabled/fire` | 1 | ✅ |
| ODE (Tsit5) + plot + initial/final/Δ table | pv:5268; petri-solver.js | `sim.ode {t_end, method, places}` → TimeSeries view; result carries `initial`/`final`/`delta` | pflow-solver | 1 | ✅ plot + final values in legend; Δ in the result (no table view yet) |
| Per-transition rates (session), rate 0 disables | pv:5287 | `sim.set_rate {id, rate}` | core | 1 | ✅ |
| Solver options dt/abstol/reltol | pv:5287–5300 | `sim.ode {dt, abstol, reltol, dtmax}`; also on every `opt.*` | pflow-solver Options (`sim::OdeOptions`) | 3 | ✅ |
| Place checkboxes on plot, legend toggle, hover | pv:593 | TimeSeries view; `sim.ode {places}` / `opt.rate_scan {places}` filter | app | 3 | 🟡 legend toggle + hover; places filter in core |
| Rate scan (steady state vs rate) | pv:5268 | `opt.rate_scan {transition, from, to, steps≤25, t_end=50}` → TimeSeries(x=rate) | pflow-solver | 3 | ✅ value at t_end, as the editor (not detected equilibrium) |
| Sweep (one place across rates) | pv:5268 | `opt.sweep {transition, place, from, to, steps≤25}` → TimeSeries overlay, group `k=<rate>` | pflow-solver | 3 | ✅ runs resampled onto one time grid |
| Phase plot | pv:5268 | Results → Phase (any TimeSeries) | app | 3 | ✅ |
| CSV / JSON download, "copy MCP call" | pv:5615–5620 | `export.artifact {format}`, `command.copy_call` | core | 1 | 🟡 csv/json |
| SSA / SDE engines (library only in JS) | petri-ssa.js, petri-sde.js | `sim.ssa {t_end, seed, runs, points}`, `sim.sde {…, dt}` → TimeSeries, mean ± sd band when runs > 1 | pflow-solver ssa (portable, byte-exact) / ssa::sde | 3 | ✅ SDE refuses gated nets, as go-pflow |
| Rate fitting (library only in JS) | petri-learn.js:1863 | `opt.fit` | pflow-learn | 5 | |
| Equilibrium (not in the editor; pflow-rs) | — | `sim.equilibrium {t_max, tolerance}` → Report | pflow-solver `solve_until_equilibrium` | 3 | ✅ |

### A4. I/O & sharing

| Feature | Source | Native | Backing | Phase | Status |
|---|---|---|---|---|---|
| Open/save pflow JSON-LD | pv:1320 | `file.open`, `file.save`, `file.load_json`, `file.export_json` | core | 1 | ✅ |
| CID (URDNA2015 → CIDv1) as `@id`, `{cid}.jsonld` | seal-cid.mjs, internal/seal | `file.cid`, `file.seal` | core `cid` (port of json-gold; byte-identical on pflow-xyz's parity goldens) | 2 | ✅ (`{cid}.jsonld` naming: save with `path`) |
| Open from URL, `?data=` / `?cid=` permalink | pv:661, 2008 | `file.open_url`, `file.open_cid`, `file.permalink` | app (http) | 2 | ✅ native (http feature); web: permalink only |
| Autosave | pv:2117 | eframe persistence (doc, path, settings); `--fresh` skips | app | 2 | ✅ native + web |
| Example gallery | index.html:766 | `file.open_example {name}`, File → Examples | bundled | 2 | ✅ all 6 pflow.xyz gallery nets + SIR |
| Server save / delete / My Diagrams (`/api/save`, `/o/{cid}`) | pv:1345, 1415, 6603 | `remote.pflow.save/list/delete` | remote | 7 | |
| Share dialog (badge, `/img/{cid}.svg`), Gist, GitHub login | pv:1506, 1709, 214 | `remote.pflow.share`, `remote.gist` | remote | 7 | |
| Layout algorithms: Sugiyama, grid, bipartite, circular | pv:5837–6320 | `net.layout {algorithm}` (`core::layout`, matches the JS output exactly) | port | 2 | ✅ coordinates identical to pflow.xyz |
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
| scenario | seeded SSA/ODE what-if | `sim.scenario {scenario}`: marking, rates, schedule, hours, samples, realizations, seed, summary, engine; `params` refused | pflow-solver stochastic (contention, depletion, metrics, schedules) | TimeSeries + bands, Table | 3 ✅ (no `neverFired`; starts from the current marking) |
| compare | N scenarios, shared seed | `sim.compare {scenarios, seed}` | same | TimeSeries overlay (group = scenario), Table | 3 ✅ (scenarios must share hours/samples) |
| receipt | signed run certificate | `sim.receipt` | port (ed25519) | Report | 7 |
| evaluate | score moves by rollouts | `sim.evaluate {player, objective, rollouts, seed, horizon, depth}` (play method) | metamodel firing + pflow-solver stochastic | Table (ranked) | 3 ✅ (player = role or id prefix, objective inferred from `win_*`; no eliminate/incidence/search) |
| crosscheck | SSA vs ODE vs conservation | `sim.crosscheck {t_end, runs, seed, points}` | pflow-solver ssa + ODE; Farkas P-invariants in core | TimeSeries overlay (ssa/ode), Report | 3 ✅ (no move-ranking check) |
| dataset | synthetic event log | `data.generate {cases, seed}` | pflow-solver ssa + pflow-eventlog | Table, CSV export | 5 |
| run_pipeline | bound models, loop fixed point | `system.run {name \| bindings, hours, engine: ode\|ssa, seed, realizations, sweeps, tolerance, graph}` | port (`core::systems`) | TimeSeries overlay (group = model), Report (order, loop verdict, per-binding flows, assumptions), Graph | 6 ✅ topological order, delays, sim's sweep fixed point (bit-exact) + `tolerance`/`sweeps`; default engine `ode` (piecewise mass-action), `ssa` is sim's scenario engine |
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
| bind, get_binding, list_bindings | checked port bindings (units, delay) | `system.set_port`, `system.ports`, `system.bind`, `system.binding`, `system.bindings`, `system.unbind` | port (`core::systems`, binds-port-v1/v2/v3) | Report | 6 ✅ ports in the document's `"ports"` field; bindings by library name, not CID |
| create_system, system | stored model+binding sets | `system.create`, `system.show`, `system.list`, `system.delete` | port | Graph (system) + Report | 6 🟡 checks, plan, loops, maxHours, per-port bound/candidates; no isomorphicTo/subnetOf edges or one-net shape (needs the phase-7 prover), no collection snapshots or `naming` |
| conformance | fitness/precision/generalization/simplicity | `data.conformance {log}` | pflow-mining, pflow-eventlog | Table + residual on canvas | 5 |
| conformance_stream | online replay, first divergence | `data.replay {events}` | pflow-mining | Trace + running-fitness TimeSeries | 5 |
| calibrate | learn rates/delays from a log | `data.calibrate {log}` | pflow-learn, pflow-mining | Table (evidence), new model | 5 |
| optimize | Monte Carlo sweep, Pareto | `opt.optimize {parameters, objectives, samples≤2000, seed}` ✅ | pflow-solver | Scatter + Pareto, Table | 3 |
| param_heatmap | two-rate grid → final value | `opt.heatmap {param_x,param_y,observable}` (≤ 40 × 40, `log`) ✅ | pflow-solver | Heatmap | 3 |
| distill | fit rate multipliers to minimax | `opt.distill` | pflow-learn | Report | 5 |
| publish, publish_compare, my_sheets | Google Sheets | `remote.sim.publish` | remote | URL | 7 |
| publish_app | host an HTML app | `remote.sim.publish_app` (wasm build) | remote | URL | 7 |

## C. Visualizations and exports

| View | Fed by | Export | Phase | Status |
|---|---|---|---|---|
| Net graph (+ marking, enabled, overlays) | document | SVG (core), PNG (app), JSON | 1 | ✅ (overlays 4) |
| Time series (lines; mean±sd bands; overlays) | ODE, SSA, scenario, compare, pipeline, crosscheck | CSV, JSON, SVG | 1 | ✅ lines, CSV/JSON | ✅ bands, overlay groups (sweeps colored by group), hover, legend toggle; CSV/JSON/SVG 
| Phase plot (X vs Y) | any TimeSeries | SVG, CSV | 3 | | ✅ view (export the source series as CSV) 
| Heatmap | param_heatmap | CSV, JSON, SVG | 3 | 🟡 artifact + CSV/JSON (`opt.heatmap`) | ✅ view with color bar + hover; CSV/JSON/SVG 
| Scatter / Pareto | optimize | CSV, JSON | 3 | 🟡 artifact + CSV/JSON (`opt.optimize`) | ✅ view with Pareto front + hover; CSV/JSON/SVG 
| Matrix table (incidence, P/T invariants) | analysis | CSV, JSON | 4 | | 🟡 view ready (phase 4 produces the data) 
| Reachability / coverability graph | analysis | JSON, DOT, SVG | 4 | |
| Trace / timeline (replayable on canvas) | reach, verify, replay, token game history | JSON, CSV | 4 | |
| Tornado / ranked bars | diagnose, evaluate | CSV | 4 | |
| Table (conformance, calibrate, dataset) | data | CSV | 5 | | ✅ generic table view (scenario/compare/evaluate tables today) 
| System / pipeline / knowledge graph | compose, store | JSON, SVG | 6–7 | 🟡 system + pipeline Graph artifacts (`system.show`, `system.run {graph}`); knowledge graph is phase 7 |
| Side-by-side mapping | prove, diff | SVG | 6–7 | |

## D. Gaps in pflow-rs (work that lands upstream, not here)

1. ~~Stochastic analysis over `ssa`: contention, depletion, supply kinds, schedules/stages.~~ Landed as `pflow_solver::stochastic`; `sim.scenario` uses it.
2. Siphon and trap search. Home: pflow-reachability.
3. Exact automorphisms, canonical labelling, color refinement. Source: sim `orbits.go`, `classify.go`.
4. Morphism prover/checker. Source: sim `pkg/morphism`.
5. Port bindings, pipelines, loop fixed point. Source: sim `bindcheck`, `pipeline`, `loop`. Landed here as `crates/core/src/systems.rs` (no `Session` dependency, ready to move upstream).
6. pflow.xyz layout algorithms and URDNA2015 CID sealing. Both landed here instead (`crates/core/src/layout.rs`, `crates/core/src/cid.rs`; the latter is a port of pflow-xyz's json-gold behaviour, not a general JSON-LD library). Worth upstreaming.
7. Deterministic ODE summation. `pflow_core::PetriNet` keeps transitions in a std `HashMap` with a random hasher, so `pflow_solver::Problem` sums derivatives in a different order each run, and results differ in the last bits between runs. Iterate in sorted order (or use a `BTreeMap`) upstream. Until then, tests compare ODE results with a tolerance.

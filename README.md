# pflow-native

A native Petri-net editor and simulator built on [pflow-rs](https://github.com/pflow-xyz/pflow-rs), using egui/eframe.

- It opens and saves the [pflow.xyz](https://pflow.xyz) editor JSON-LD.
- It plays the token game through pflow-rs's firing rule and integrates the mass-action ODE with `pflow-solver`.
- Every action is a command in one registry, so the window, the CLI, a JSON-lines control channel and an MCP server all drive the same thing.

![SIR net with ODE plot](docs/screenshot-sir.png)

## Run

```bash
make run                                   # cargo run --release -p pflow-native
cargo run -p pflow-native -- examples/coffee-shop.json
cargo run -p pflow-native -- --example sir --cmd 'sim.ode {"t_end":15}' \
    --screenshot out.png --size 1400x900   # headless-ish capture, then exit
make test && make lint && make wasm-check
```

**Offline build.** `make build-offline` builds the app and CLI without the `services` feature (`--no-default-features`). That build never contacts pflow.xyz, sim.pflow.xyz or GitHub:
- Gone: the Remote and Share menus, Accounts, `remote.*`, `auth.*`, `remote.credentials`, `file.open_cid` and `file.permalink`.
- Still there: everything local (editing, simulation, analysis, the store, provenance), plus `file.open_url` for a URL you give it.

`make test-offline` runs the tests for that build.

## Using the editor

| Key | Tool / action |
|---|---|
| 1 select · 2 place · 3 transition · 4 arc · 5 token · 6 delete | tools, numbered as in pflow.xyz |
| 8 / X | play mode (click a green transition to fire it) |
| double-click a transition | fire it from select mode |
| right-click the target in arc mode | make the arc inhibitor/read |
| token tool click / right-click | +1 / −1 |
| shift-click, shift-drag | multi-select, box select |
| drag empty space, middle-drag, space+drag, wheel | pan |
| pinch, ⌘+wheel | zoom |
| ⌘Z / ⌘⇧Z · Delete · Esc · F · R · ⌘R | undo/redo · delete · cancel · fit · reset marking · run ODE |
| ⌘C ⌘X ⌘V ⌘D | copy · cut · paste · duplicate; copy, cut and paste go through the system clipboard in the format shared with pflow.xyz ([format](docs/clipboard-format.md)), duplicate does not |
| ⌘K · ⌘J · ⌘I · ⌘B | command palette · JSON pane · inspector · bottom panel |

The canvas snaps nodes to the 10-unit grid used by pflow.xyz (View → Snap to Grid). Menus are generated from the registry. Any command that takes arguments opens a form built from its argument spec, and **⌘K** searches every command. The app autosaves the document and settings, and restores them on the next start (`--fresh` skips that). The panels are:
- **Inspector** (right) edits labels, tokens, capacity, arc weight, inhibitor and rate.
- **Plot** shows the ODE time series with hover readout, a legend toggle, and CSV/SVG export.
- **Run** lists the fireable transitions, the marking table and seeded random runs.
- **Console** shows the journal and has a command line for any command.

![Tic-tac-toe from the pflow.xyz gallery, sealed, with the JSON-LD pane](docs/screenshot-ttt.png)

**Content IDs.** `file.cid` computes a document's pflow.xyz CID and `file.seal` stores it as `@id`. Both use URDNA2015 → CIDv1 and are byte-identical to pflow.xyz's Go sealer on its parity goldens. Opening and saving a file keeps its CID.

## Simulation workbench

Results land in the **Results** tab, one view per kind:
- time series with ± bands and overlays, plus phase-plot and Δ-table modes
- heatmaps
- Pareto scatters
- tables
- reports

Each result exports as JSON, CSV or SVG. The **Workbench** tab lists every simulation command. Its forms fill transition and place arguments from what you have selected.

| Command | What |
|---|---|
| `sim.ode` | mass-action ODE (pflow-solver); solver options, place filter, initial/final/Δ |
| `sim.equilibrium` | integrate to steady state |
| `sim.ssa`, `sim.sde` | Gillespie SSA / chemical Langevin, seeded, with mean ± sd over runs |
| `sim.scenario`, `sim.compare` | sim.pflow.xyz-style what-ifs: marking/rate/schedule overrides, metrics, contention; N scenarios on one seed |
| `sim.crosscheck` | SSA vs ODE vs conservation laws (P-invariants), with a verdict |
| `sim.evaluate` | rank the moves enabled for a player by rollouts (games such as tic-tac-toe) |
| `opt.rate_scan`, `opt.sweep` | pflow.xyz's Rate Scan and Sweep |
| `opt.heatmap` | two rates on a grid, showing one place's final value |
| `opt.optimize` | Monte-Carlo over rate ranges, with a Pareto front over objectives |

![Scenario compare with ± bands](docs/screenshot-compare.png)
![Rate heatmap](docs/screenshot-heatmap.png)

## Analysis and data

| Area | Commands |
|---|---|
| Structure | `analysis.incidence`, `analysis.invariants` (P/T, highlighted on the net), `analysis.siphons` (siphons/traps, Commoner, deadlock witnesses), `analysis.validate`, `analysis.canonical` / `analysis.classify` (symmetry orbits, identical to sim.pflow.xyz on the bundled examples) |
| State space | `analysis.reachability` (graph; click a state to load it), `analysis.verify` (counterexamples become traces you can step through), `analysis.reach` (shortest witness or invariant certificate), `analysis.diagnose` (tornado) |
| Event logs | `data.load_log` (CSV/JSONL), `data.generate` (seeded synthetic logs), `data.conformance` (fitness/precision/…, residual on the net), `data.replay` (first divergence + repair), `data.discover` (alpha/heuristic mining → new net) |
| Games | `game.value` (the exact value of the current position, from `sides`, `reward` and `auto` as the `rnn.*` commands take them), `game.move` (a best move there, not fired); both run a pflow-metamodel `SearchPlan` under a node budget and keep the result as a Report. A plan and budget give the same result on any machine, except that a plan naming the `ode` root estimate can search a different number of nodes in the browser build (its ODE step control uses the platform's `powf`), so near the budget one build can finish where the other gives up |
| Learning | `data.calibrate` (rates from a log), `opt.fit` (ODE rates to observed series), `opt.distill` (game scorer vs exact minimax). With `apply`, the learned rates are one undo step |
| Views | `view.string_diagram` (pflow.xyz's string diagram), `net.layout {algorithm: "string"}` |

![Reachability graph](docs/screenshot-reachability.png)
![Reach witness replayed on tic-tac-toe](docs/screenshot-reach.png)

## Composition and systems

| Area | Commands |
|---|---|
| Library | `model.save / list / get / open / import / delete`, the Models tab ("current" = the open net) |
| Composition | `compose.components` (sim's 7 components + pflow-compose templates), `compose.fuse` (attach ports onto your places), `compose.extend` (sim's edit ops as one undo step), `compose.diff` (change table, canvas highlight, side-by-side drawing) |
| Systems | `system.set_port`, `system.bind` (unit / conversion / delay checks), `system.create` / `system.show` (graph), `system.run` (pipeline in topological order; delayed loops solved to a fixed point by sweeps) |

![Fusing a service component onto the coffee shop](docs/screenshot-diff.png)

## Store, sharing and provenance

| Area | Commands |
|---|---|
| Local store | `store.put / get / open / list / delete` (keyed by the pflow.xyz CID, under `<data_dir>/store`; nets, and also composition documents, raw Bundles and model content, each with its `kind`), `store.link / edges / neighbors / query` (sim's predicate registry and query language), collections, irrevocable licenses, `store.supersede` / migration offers, content-addressed maps, `store.graph` |
| pflow.xyz | `remote.pflow.save / list / delete`, `remote.pflow.share` (page, `/img/{cid}.svg`, badge), `remote.gist` |
| sim.pflow.xyz | `remote.sim.tools`, `remote.sim.call` and wrappers (`create_model`, `publish`, `publish_app`, `prompt`, `prove`, `check_witness`) over MCP |
| Provenance | `sim.receipt` / `provenance.verify_receipt` (Ed25519, sim's receipt format, verified by replay; also a composition run, a content check and a content, from `bundle.receipt` / `content.receipt`, see [docs/composition-documents.md](docs/composition-documents.md)), `provenance.check_witness` (traces, invariant certificates, morphism maps), `zk.setup / prove_fire / prove_trace / verify` (Groth16 via pflow-zk-arkworks; opt-in: `make build-zk`, since the upstream circuit does not yet prove enabledness) |

**Nothing leaves the machine by accident.** Every remote write returns a preview of the exact request, with credentials redacted, unless it is called with `"send": true`. In the app, a preview opens a confirmation window, and only **Send** sends. Credentials come from environment variables or `<data_dir>/credentials.json`, never from documents or the journal; `remote.credentials` shows which are configured. They are:
- `PFLOW_XYZ_TOKEN`: a GitHub token with `read:user`.
- `GITHUB_TOKEN`: a GitHub token with the `gist` scope.
- `SIM_PFLOW_TOKEN`: a sim OAuth access token, valid for 24 h.

Each credential goes only to its own hosts, over HTTPS: `PFLOW_XYZ_TOKEN` to `pflow.xyz`; `GITHUB_TOKEN` to `api.github.com` and `pflow.xyz`; `SIM_PFLOW_TOKEN` to `sim.pflow.xyz`. A caller-supplied `server` or `endpoint` that is not one of them is refused before anything is built or sent.

**Signing in.** Rather than pasting tokens, sign in from **Remote → Accounts…**, with `pflow-native-cli login sim|pflow`, or with `auth.login {"service": …}`:
- **sim.pflow.xyz** opens your browser on sim's own sign-in page, which uses Google. Underneath, this is sim's OAuth server: pflow-native registers itself as a client, uses PKCE and a one-off `127.0.0.1` redirect, and stores the access token as `SIM_PFLOW_TOKEN`. That token lasts a day and is renewed automatically with the refresh token, which lasts 90 days.
- **pflow.xyz** uses a GitHub sign-in, since pflow.xyz checks its bearer against GitHub. This is GitHub's device flow with pflow.xyz's OAuth app: you enter the code shown at github.com/login/device. The token is stored as `GITHUB_TOKEN`, so Gists use it too. The app needs **Enable Device Flow** turned on in its GitHub settings; until then the sign-in says so. `PFLOW_NATIVE_GITHUB_CLIENT_ID` points it at another OAuth app.

Tokens are written to `credentials.json` (owner-only) and never returned by a command. `auth.status` (`pflow-native-cli accounts`) shows what you are signed in to, and `auth.logout` forgets a sign-in. An environment variable still takes precedence over a sign-in.

**Who can send.**
- **The app** asks you every time.
- **The control port** is open only to whoever holds the per-launch token, which the app writes to `<data_dir>/control.json` (owner-only). Sends that arrive there still wait for your **Send** in the app. Batches over the port may not contain sends.
- **`pflow-native-cli control` and headless `mcp`** run over stdio and are started by you. They are trusted: a `send: true` there sends without a dialog, still only to the allowlisted hosts.

**Datasets stay server-computed.** sim stores only results it computed itself. pflow-native can ask it to compute (scenario, dataset, calibrate, publish), but never uploads locally computed datasets; `remote.sim.call` refuses `sim_map_put`. Models are not datasets and can be published.

![Confirm before sending](docs/screenshot-confirm-send.png)

## Experimental: learned policies

`make build-rnn` (or `--features rnn`) adds a **Learn** menu with a policy that learns which transition fires next. There are two kinds:
- **`arch: "rnn"`** (the default) is an Elman RNN from pflow-tropical, with `hidden` units.
- **`arch: "mlp"`** is a feed-forward ReLU network over the marking, with `layers` hidden layers (e.g. `[64, 64]`). It has no memory of earlier moves. It doesn't need one, because the marking is the net's whole state.

`rnn.train` and `rnn.imitate` take `arch`; the other commands work with either kind of model. The commands:
- `rnn.train` plays seeded random games through the firing rule and trains on them. It plots the loss against the floor a perfect imitator of random play would reach.
- `rnn.play` lets a model choose each firing. The game is stored as a trace, so `sim.goto` replays it and `provenance.check_witness` re-checks it.
- `rnn.self_play` improves a model by self-play against a reward read from the final marking, e.g. `{"win_x": 1, "win_o": -1}` on tic-tac-toe.
- `rnn.imitate` trains a model on an exact minimax teacher, which solves the whole game (small games only). Ties between moves of equal value go to the move that does best against random play.
- `rnn.evaluate` plays a `player` (the model by default) against an `opponent` from each side, and charts wins, draws and losses. Either can be the model, `random`, `minimax` (a random choice among the teacher's best moves, so games cover many lines) or `ode`. The `ode` player is sim_evaluate's play scorer: it fires each move, solves the mass-action ODE `horizon` ahead, and takes the best expected reward. `player: "ode", opponent: "minimax"` measures the ODE evaluator itself.
- `rnn.verify` is the exhaustive referee from sim-pflow-xyz `experiments/ode-minimax`. For each side it walks every legal line of the opponent, optimal or not. At each of the player's decisions it checks the move against the exact game value, and counts value-losing moves (a drawn or won position thrown into a loss) and missed wins (a won position let slip to a draw). Zero of both on every side is minimax equivalence over the whole game, not a sample. The player can be a model, `ode` or `minimax`.
- `rnn.imitate` with `positions: "all"` (MLP only) learns from every reachable position once, instead of from sampled games.
- `rnn.policy` shows what a model would play at the canvas marking, or the teacher's value for each move with `teacher`, as badges on the canvas and a bar chart.
- Games and training use every core. Each game has its own seed, so a run gives the same result on any machine. `batch` (default 16) is how many games' gradients are computed at once per update; `batch: 1` is one update per game.
- `auto` (on every rnn command) names transitions that fire as soon as they are enabled, before anyone chooses. On tic-tac-toe, `["x_win_", "o_win_"]` makes a line end the game when it is made.

On tic-tac-toe with `sides {"x_": 1, "o_": -1}`, the teacher solves all 5,478 positions (the empty board included). Every opening draws with best play, and the centre is the tie-break. During imitation the teacher plays any of its best moves, so the games cover every optimal line, but the label is always its tie-broken choice.

Results with 2,000 games, 150 epochs, the default `explore` of 0.3, and 1,000 games per side against minimax:

| player | vs minimax as X | as O | vs random as O (500) |
|---|---|---|---|
| `ode` (the plain net, no tuning) | 100 draws of 100 | 35 losses of 100 | 14 losses |
| RNN, `hidden: 64` | 1,000 draws | 8 losses | 12 losses |
| MLP, `layers: [64, 64]` | 1,000 draws | **1,000 draws** | **0 losses** |

The ODE player's weak seat is the one predicted by sim-pflow-xyz `experiments/ode-minimax` (finding 10). Defending, the forks it falls into only show up in move order, and an order-free flow integral can't see that. That experiment closes the gap by adding transitions that bias play toward wins and blocks; the plain net here has no such tuning. Those are sampled tournaments. Under `rnn.verify`, which checks every opponent line including blunders:

| player | X: value-losing / missed wins | O: value-losing / missed wins |
|---|---|---|
| `minimax` (a check of the referee) | 0 / 0 (99 decisions) | 0 / 0 (308 decisions) |
| `ode` | 0 / 0 | 12 / 0 |
| RNN `hidden: 64`, games | 2 / 18 | 13 / 32 |
| MLP `[64, 64]`, games | 0 / 2 | 0 / 0 |
| **MLP `[128, 64]` or `[64, 64]`, `positions: "all"`, 300 epochs, `learning_rate` 0.005–0.01** | **0 / 0** | **0 / 0** |

The last row held on all 4 seeds tried, so a trained MLP is minimax-equivalent on tic-tac-toe. Training on games misses a few wins after an opponent blunders, because sampled games rarely reach those positions. Each example in `positions: "all"` is a single move, so it gets the full learning rate: 0.02 is too large.

`examples/connect-3.json` is the Connect-3 net from the sim.pflow.xyz ODE experiments: a 4×4 board with gravity, 52 places and 80 transitions. It uses the same `sides`, `reward` and `auto` as tic-tac-toe. The teacher solves it in a few seconds: every opening is a forced X win, over 23,930 decision positions. `rnn.verify` on the `ode` player at `horizon` 0.5 reproduces that experiment's referee table exactly: X 0 value-losing / 0 missed wins over 45 decisions, O 6 / 2 over 462. With `positions: "all"`:

| MLP | epochs, `learning_rate` | seeds minimax-equivalent | O value-losing + missed wins otherwise |
|---|---|---|---|
| `[128, 64]` | 100, 0.01 | 0 of 1 | 8 + 0 (and 1 for X) |
| `[256, 128]` | 100, 0.01 | 1 of 4 | 2 + 0 to 2 + 1 |
| **`[256, 128]`** | **200, 0.005** | **4 of 4** | — |
| **`[512, 256]`** | **100, 0.01** | **3 of 3** | — |

Undertrained models only slip as O, in 2–3 of about 450 positions. Those moves give up value against perfect play but don't change a result, because O loses with best play anyway. On 14 cores a `[256, 128]` run takes 2–5 minutes.

`examples/connect-4.json` is Connect Four: 131 places, 222 transitions, about 4.5 trillion positions. That is far past the teacher's 200,000-position cap, so `rnn.verify` and `rnn.imitate` take `games` and sample instead. Positions come from random-play games, at least `min_moves` (default 14) moves in, and every legal move at each is valued by an exact alpha-beta search over the firing rule (`budget` nodes per move; positions past it are skipped and counted, never guessed). `labels` keeps those values in a JSONL file, so later runs only search new positions. On the default sample (250 games, seed 13, 1,942 distinct positions): at the pflow-rs `9ce3d43` pin, whose search runs the `must_win` filter by default, the default 800,000-node budget labels 1,860 and skips 82 (1,819 and 123 before it). The `ode` row is measured at the pin; the MLP rows were measured on the earlier 1,819 positions, so they count out of 940 / 879:

| player | X game-losing | O game-losing |
|---|---|---|
| random legal move | about 33% | about 33% |
| `ode`, `horizon` 3 | 19 / 964 | 18 / 896 |
| MLP `[256, 128]` trained on these same positions | 0 / 940 | 0 / 879 |
| MLP `[256, 128]`, 7,064 positions from other games (seed 1, 1,000 games) | 245 / 940 | 235 / 879 |
| MLP `[256, 128]`, 36,227 positions (5,000 games) | 137 / 940 | 139 / 879 |
| MLP `[256, 128]`, 182,820 positions (25,000 games) | 67 / 940 | 74 / 879 |
| **MLP `[256, 128]`, 734,548 positions (100,000 games)** | **19 / 940** | **26 / 879** |

None of the referee's positions appear in any training set. The MLP learns every position it is shown, ties included, and generalizes slowly: each fivefold step in data takes a third to a half off its held-out mistakes (a quarter from 25,000 to 100,000 games), which at 100,000 games brings it level with the `ode` player. Connect-3 hid this because there it saw every position. (100 epochs, `learning_rate` 0.005. For `positions: "sampled"` the softmax ranges over each position's legal moves; over all 222 outputs the illegal moves' logits are pushed down without end, and runs on 5,000 games diverged. Each position's gradient is also clipped by its length rather than entry by entry: with entry-wise clipping, 100,000 games silenced all but a handful of the second layer's units within 10–20 epochs at every learning rate tried, and the model fell back to 265 / 940 and 250 / 879. The training loss is no guide here: it rewards spreading probability over tied best moves, so the run that plays best has the highest loss.)

Every label above was checked against Pascal Pons' Connect-4 solver (github.com/PascalPons/connect4): all 4,635,258 move values of the 734,548 training positions and the referee's 11,672 agree. The positions skipped for budget are not a random sample: 72% of the 123 skipped before the pin are wins for the side to move, against about a third of those labelled.

It is off by default. A learned policy is a guess, not a verdict like the rest of the app's results, and pflow-tropical is research code.

## Agents: CLI, control channel, MCP

```bash
pflow-native-cli list net.                          # commands + JSON Schemas
pflow-native-cli exec -i examples/sir.json sim.ode '{"t_end":10}'
pflow-native-cli batch -i in.json -o out.json steps.json   # one undo step, atomic
pflow-native-cli control                            # stdin/stdout JSON lines:
#   {"id":1,"command":"net.add_place","args":{"x":100,"y":100}}
#   {"id":1,"ok":true,"result":{"id":"p0"}}
pflow-native-cli mcp                                # MCP over stdio (attaches to a running app)
```

**Driving the live window.** The app listens on `127.0.0.1:7979` for the same JSON-lines protocol. `--control ADDR` moves it and `--no-control` turns it off.
- `pflow-native-cli mcp` attaches to that port when the app is running, so the agent and you share one window. Otherwise it runs a private headless session. Use `--headless` or `--attach ADDR` to force either.
- Commands are answered while the window is minimized, hidden or covered too.
- `view.show_artifact {id}` opens a result in the Results tab, so you and the agent look at the same thing.
- `view.focus` tells the agent what you are looking at: the selected nodes (tokens, enablement, rate), the result open in the Results tab, the bottom tab, the nodes in view, the highlight, whether the window is on screen, and the last commands.
- `note.add {text, node | artifact | x,y}` pins a note where you will see it: a sticky note on the canvas, a card above a result, and a list in the Inspector with delete buttons (`note.list / delete / clear`). Notes stay in the session and are never saved into the document.
- In attached mode the `screenshot` tool returns the window as a PNG. It needs the window on screen; a hidden window refuses at once, so export instead (`export.svg`, `export.artifact`).

![Driven over MCP](docs/screenshot-mcp.png)

The MCP server's tools are:
- `list_commands` lists commands, optionally filtered by prefix.
- `execute` runs one command. In `execute`, `batch` and `run_job` results, arrays of more than 40 numbers (time series, matrix rows) are summarized as length, min, max and first and last values; pass `"full": true`, or read the stored result with `artifact.get`, for all of it.
- `batch` runs `[{command,args}]` as one undo step and rolls back on failure.
- `run_job` runs a long command (simulation, optimization, analysis, data, rnn) as a background job and waits up to `wait_s` for it. The window stays responsive, and the results join the session when it finishes; `job.status` follows one that is still running.
- `get_state` returns the whole state.
- `screenshot` returns the window as a PNG (attached mode only).
- `render` returns a PNG of the net or of one stored result (`artifact`), drawn without the window (`export.png`), so it works headless and while the app is hidden.

To register it with Claude Code:

```bash
claude mcp add pflow-native -- /path/to/pflow-native-cli mcp
```

## Design

- [`ARCHITECTURE.md`](ARCHITECTURE.md) covers the layers, the registry, artifacts and views.
- [`FEATURES.md`](FEATURES.md) is the parity matrix against pflow.xyz and sim.pflow.xyz, with the phase plan.
- The command-registry, macro and MCP patterns were borrowed from storytold's [vectorcraft](https://github.com/storytold/vectorcraft) and [photocraft](https://github.com/storytold/photocraft) (MIT/Apache-2.0). They were re-implemented here; no code was copied.

pflow-rs is pinned to a git revision. To build against a sibling checkout instead, copy `.cargo/config.toml.example` to `.cargo/config.toml`.

## Web

`make wasm-check` keeps the core and the app compiling for `wasm32-unknown-unknown`. To build or serve the browser version with Trunk (`cargo install trunk`; see `crates/app/index.html`):
- `make web-build` writes `crates/app/dist/`.
- `make web` serves it.

`crates/wasm` is a second, UI-free browser build for pflow.xyz: the registry behind a small wasm-bindgen API (`listCommands`, `execute`, `batch`, `load`, `dumpState`, `restoreState`), without the `services`, `zk` and `rnn` features. `make wasm-shim` writes the `.wasm` and its JS glue to `target/wasm-shim/` (needs `cargo install wasm-bindgen-cli --version 0.2.129`). The library, systems, macros, notes, results and store live in the engine, so the page keeps them across reloads itself: it calls `restoreState` once at startup and saves `dumpState()` after each command that changes them.

```js
import init, { Engine } from "./pflow_native_wasm.js";
await init();
const e = new Engine();
const saved = localStorage.getItem("pflow-native-state");
if (saved) e.restoreState(saved);
e.load(jsonld);
const invariants = JSON.parse(e.execute("analysis.invariants", ""));
e.execute("model.save", JSON.stringify({ name: "mine" }));
localStorage.setItem("pflow-native-state", e.dumpState());
```

On the web the autosave goes to browser storage, and `?data=` permalinks open. File dialogs, URL fetches and screenshots are native-only for now.

![Web build](docs/screenshot-web.png)

## Contributing

We don't merge outside code, so pull requests won't be merged as-is.
Feature requests and bug reports are welcome: please open an
[issue](https://github.com/pflow-xyz/pflow-native/issues).

## License

MIT

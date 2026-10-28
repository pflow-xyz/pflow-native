# pflow-native architecture

pflow-native is a native (and later web) Petri-net workbench built on pflow-rs.
Its long-run scope is parity with the **pflow.xyz editor** and the
**sim.pflow.xyz MCP tool surface**, plus views for all the data those produce.
`FEATURES.md` is the feature-by-feature matrix. This file describes the
structure designed to hold all of it.

The patterns are borrowed from storytold/vectorcraft and photocraft (MIT/Apache).
They were re-implemented here, not copied.

## Layers

Nothing below `app` may depend on egui, eframe, winit or rfd, so everything an
agent can do runs headless and builds for `wasm32-unknown-unknown`.

```
pflow-rs crates (pinned git rev; sibling-path [patch] for development)
  └─ pflow-native-core   (crates/core)  Document, Session, Registry, Artifacts, and every
       │                                 command set: file/edit/net, sim/opt/analysis, data,
       │                                 compose/system, store/remote/provenance/zk, … (one
       │                                 module per area under src/commands/; the engines
       │                                 beside them under src/). Features: `http` (remote
       │                                 sends, URL loading), `zk` (Groth16 proofs),
       │                                 `rnn` (experimental RNN and MLP policies).
       ├─ pflow-native       (crates/app)  eframe UI: views render artifacts, every action -> registry;
       │                                   control port for agents
       ├─ pflow-native-cli   (crates/cli)  list / exec / batch / control (JSON lines) / mcp (stdio)
       └─ pflow-native-wasm  (crates/wasm) the registry for pflow.xyz's browser: listCommands /
                                           execute / batch / load over JSON (wasm-bindgen), and
                                           dumpState / restoreState for the page's storage
```

Areas once planned as separate crates (analysis, remote) live as modules of
pflow-native-core: they share `Session` and the registry, and splitting them
would only add plumbing.

## The document is pure data

`Document` is the pflow.xyz editor JSON ("Shape A"). It is held in ordered `Vec`s so a
load/save round trip keeps the file's own order and its unknown fields, top-level and
per element (`offset`, an element's `@type`, ...): each one is a triple of the net's
content identifier (`cid.rs`, pflow.xyz's URDNA2015 → CIDv1 sealing), so loading and
saving a pflow.xyz net keeps its CID.
It knows nothing about selection, views or simulation state.

`Session` wraps it with the following:

| Field | What |
|---|---|
| `doc`, `path`, `revision`, `saved_revision` | the net and its dirty tracking |
| `marking` | runtime token-game state (per place, per color) |
| `selection`, `tool`, `view` | editing state, as plain data the CLI and MCP can read and set |
| `undo` / `redo` | whole-snapshot history (`doc`, `marking`, `selection`). Nets are small, so snapshots beat inverse ops for simplicity and correctness |
| `journal` | every top-level journaled command, i.e. what macros are cut from |
| `macros` | named step lists (`macro.*`) |
| `artifacts` | typed results of analysis commands (see below) |
| `models`, `systems`, `notes` | the model library, port bindings and systems, notes for the person at the window |

What outlives one document (the library, systems, macros and a recording in
progress, notes, results, and the store, which is a process-wide handle rather
than a field) is saved and restored whole by `session.dump_state` /
`session.restore_state`, for a front end with no data directory to keep it in.
A restore checks every part before it changes any (the store's files against
their content ids), and refuses to overwrite a store on disk. Results are the one
part it does not hold to that: a result it cannot read back (JSON has no NaN, so a
diverging run saves null) is dropped and named, as its command recomputes it. The
document, marking, selection and history are not part of it.

## One command registry

Every action is a `CommandSpec`:

```rust
CommandSpec {
    id: "net.add_place",          // <namespace>.<verb>
    label: "Add Place",
    menu: &["Net"],               // where the app puts it ("" = not in a menu)
    shortcut: Some("P"),
    args: &[ArgSpec { name, ty, required, help }],  // validated; unknown keys refused
    enabled: fn(&Session) -> Result<(), String>,    // greys out menus, refuses calls
    run: fn(&mut Session, &Value) -> Result<Value>,
    journal: bool,                // false for queries: not journaled or recorded
}
```

- `Session::execute(id, args)` is the only entry point. The same call runs a menu
  click, a keyboard shortcut, a canvas gesture (on release), a CLI `exec`, a
  control-channel line and an MCP `execute`.
- The app adds no command of its own. Native file dialogs only *collect a path*,
  then run `file.open {path}`.
- Command sets are contributed per namespace (`fn commands() -> &'static [CommandSpec]`)
  and assembled into one `Registry`. A crate that adds an area such as
  `analysis.*` exports its set. The assembling binary passes the full registry
  to `Session`.
- `command.batch {steps}` runs steps as **one undo step** and rolls everything
  back if any step fails. MCP `batch` uses it.

| Namespace | Holds (phase) |
|---|---|
| `file.*` | new/open/save/load_json/export_json (1); examples gallery, permalink/CID import (2) |
| `edit.*` | undo/redo/select/delete (1); copy/paste/duplicate/align/snap (2) |
| `net.*` | add/move/connect/delete/set_* (1); colors/roles/arc kinds/layout/rename (2) |
| `view.*`, `tool.*` | zoom/pan/fit/tool selection, open/close views (1) |
| `sim.*` | token game: enabled/fire/reset/step/auto (1); ODE (1); SSA, scenarios, compare, equilibrium, crosscheck, dataset (3) |
| `analysis.*` | reachability graph, invariants, incidence, verify, validate, diagnose, sensitivity, siphons/traps (4) |
| `game.*` | a net as a two-sided game: exact value and a best move of the current position, by a pflow-metamodel `SearchPlan` |
| `data.*` | event logs, conformance, conformance stream, calibrate, mining (5) |
| `opt.*` | param heatmap, optimize/Pareto, fit (3, 5) |
| `compose.*` | components, compose, extend, diff, bind, systems, pipelines (6) |
| `store.*`, `remote.*` | local CID store, sim.pflow.xyz bridge (publish, collections, links, prove, LLM prompt) (7) |
| `export.*` | json/svg/csv (1), png (app), artifact export in every format (3+) |
| `macro.*` | record/stop/play/list/get/delete (1) |
| `command.*` | batch, list (1) |
| `session.*` | dump_state/restore_state: what outlives one document, for the browser's storage |

## Artifacts → views

Anything that produces data deposits a typed **artifact** in the session, then
returns its JSON:

```rust
enum ArtifactData {
    TimeSeries { t, series: [(name, values, band?)] },  // ODE, SSA mean±sd, scenario overlays
    Heatmap    { x_label, y_label, xs, ys, z },         // param_heatmap
    Matrix     { rows, cols, values },                  // incidence, P/T invariants
    Graph      { nodes, edges },                        // reachability graph, systems, knowledge graph
    Trace      { steps: [(transition, marking)] },      // firing traces, witnesses, conformance replay
    Table      { columns, rows },                       // conformance per trace, calibrate evidence
    Scatter    { points, pareto },                      // optimize
    Report     (Value),                                 // verify verdicts, diagnose readings
}
```

In the app, a **view** is a renderer for one artifact kind (plus the net canvas,
inspector, JSON editor and console). Views open as tabs via `view.open {kind,
artifact?}`, so layout is driven by commands like everything else.

| View | Renders | Phase |
|---|---|---|
| Net canvas | the document + marking, overlays (enabled, invariants, siphons, orbits) | 1 (overlays 4) |
| Inspector | selected node/arc properties → `net.set_*` | 1 |
| Time series | `TimeSeries` (lines, bands, overlays) | 1 |
| Phase plot | two series of a `TimeSeries` against each other | 3 |
| Heatmap | `Heatmap` | 3 |
| Matrix table | `Matrix` | 4 |
| Graph | `Graph` (force/layered layout) | 4 |
| Trace / timeline | `Trace`, replayable on the canvas | 4 |
| Table | `Table` | 5 |
| Scatter | `Scatter` with Pareto front | 3 |
| JSON | document source, editable through `file.load_json` | 2 |
| Console | journal, last results, command palette | 1 |

`export.artifact {id, format}` writes any artifact as JSON or CSV, and as SVG
where it has a picture. SVG is written by core with no egui, so a headless agent
gets the same picture the app shows. PNG comes from the app's own window
screenshot (`--screenshot`).

## Front ends

- **App** (`crates/app`, binary `pflow-native`):
  - Menus and shortcuts are generated from the registry.
  - Canvas gestures become commands. A drag shows a live offset and commits
    one `net.move_by` on release.
  - `--cmd ID[=JSON]` runs commands at startup. `--screenshot PATH` saves a
    PNG after the first frames and exits.
  - The wasm entry point is cfg-gated and built with Trunk (`make web-build`).
    Autosave goes through eframe persistence, i.e. a file natively and browser storage on the web.
  - Commands that take arguments open a form generated from their `ArgSpec`s,
    and the ⌘K palette lists every command, so new commands need no UI code to
    become usable.
  - Copy and Cut put the `clip` that `edit.copy` / `edit.cut` return on the
    system clipboard, and Paste hands its text to `edit.paste {clip}`, in the
    format shared with pflow.xyz (`docs/clipboard-format.md`). `clipboard.rs`
    turns each platform's clipboard input into one shape: natively eframe runs
    with `clipboard_shortcuts: false` and the app asks for the clipboard text
    itself, so ⌘V with no text on the clipboard still pastes the session clip.
- **CLI** (`crates/cli`, binary `pflow-native-cli`):
  - `list` and `exec` run commands one-shot against a file.
  - `batch` runs a JSON step list as one undo step.
  - `control` is the JSON-lines channel: `{"id","command","args"}` →
    `{"id","ok","result"|"error"}`.
  - `mcp` is a hand-rolled JSON-RPC 2.0 server over stdio. Its tools are
    `list_commands`, `execute`, `batch` (one long-lived session), `get_state`
    and `screenshot` (attached mode only).
- **Control port** (`crates/app/src/control.rs`): the app serves the same JSON-lines
  protocol on `127.0.0.1:7979`. Requests are handed to the UI thread and run between
  frames (`App::handle_control`).
  - **Authentication:** every request carries the per-launch token from
    `<data_dir>/control.json`, which is owner-only (`core::control_auth`).
    A missing or wrong token closes the connection, and so does anything that
    looks like HTTP.
  - **Confirmation:** a command with a `send` argument, called with `send: true`,
    runs as a preview and waits for the user's Send in the app. Batches may not
    contain sends.
  - **Credentials:** `outbound::credential_for` attaches a credential only for its
    allowlisted HTTPS hosts, so no path can deliver a token to a caller-chosen
    server.
  - **Sign-in** (`core::auth`, `auth.*`): an OAuth flow writes its token into
    `credentials.json` under one of those same names, so the allowlist covers it
    unchanged. The flow is sim's OAuth server with PKCE and a loopback redirect,
    or GitHub's device flow for pflow.xyz. Flows run on a background thread; the
    commands return at once, and `auth::wait` blocks only for the CLI's `login`.
  - `mcp` attaches to that port when it is up, falling back to headless. The
    `Backend` enum in `crates/cli/src/mcp.rs` holds both cases.
  - Attached, the `screenshot` tool returns the live window as a PNG.
- **Browser engine** (`crates/wasm`, `pflow-native-wasm`): the registry in
  pflow.xyz's page, one `Engine` per page, over JSON strings.
  - `load(jsonld)` is how the page syncs its editor's document before each
    call. It runs `file.load_json` through `Session::execute_unjournaled`, so
    a macro recorded in the browser never replays it; the library, store,
    macros, notes and systems stay, and only the results go.
  - Each engine owns a memory store and makes it the thread's
    (`store::set_thread_store`) for every call, so engines never share one.
  - `dumpState()` / `restoreState(json)` are `session.dump_state` /
    `session.restore_state`: the page saves the dump in browser storage after
    each command that changes state, and restores it once at startup. One
    synchronous snapshot, rather than a store backend calling into JS:
    `StoreBackend` is `Send + Sync` and synchronous, a JS callback is neither
    and IndexedDB is asynchronous, and the store holds only part of the state
    anyway (the library, macros, notes, systems and results are `Session`'s).

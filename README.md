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
| ⌘C ⌘X ⌘V ⌘D | copy · cut · paste · duplicate |
| ⌘K · ⌘J · ⌘I · ⌘B | command palette · JSON pane · inspector · bottom panel |

The canvas snaps nodes to the 10-unit grid used by pflow.xyz (View → Snap to Grid). Menus are generated from the registry. Any command that takes arguments opens a form built from its argument spec, and **⌘K** searches every command. The app autosaves the document and settings, and restores them on the next start (`--fresh` skips that). The panels are:
- **Inspector** (right) edits labels, tokens, capacity, arc weight, inhibitor and rate.
- **Plot** shows the ODE time series with hover readout, a legend toggle, and CSV/SVG export.
- **Run** lists the fireable transitions, the marking table and seeded random runs.
- **Console** shows the journal and has a command line for any command.

![Tic-tac-toe from the pflow.xyz gallery, sealed, with the JSON-LD pane](docs/screenshot-ttt.png)

**Content IDs.** `file.cid` computes a document's pflow.xyz CID and `file.seal` stores it as `@id`. Both use URDNA2015 → CIDv1 and are byte-identical to pflow.xyz's Go sealer on its parity goldens. Opening and saving a file keeps its CID.

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
- In attached mode the `screenshot` tool returns the window as a PNG.

![Driven over MCP](docs/screenshot-mcp.png)

The MCP server's tools are:
- `list_commands` lists commands, optionally filtered by prefix.
- `execute` runs one command.
- `batch` runs `[{command,args}]` as one undo step and rolls back on failure.
- `get_state` returns the whole state.
- `screenshot` returns the window as a PNG (attached mode only).

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

On the web the autosave goes to browser storage, and `?data=` permalinks open. File dialogs, URL fetches and screenshots are native-only for now.

![Web build](docs/screenshot-web.png)

## License

MIT

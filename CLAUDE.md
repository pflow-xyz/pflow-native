# pflow-native

Native (egui/eframe; wasm later) Petri-net editor and simulator on pflow-rs.

- `ARCHITECTURE.md` describes the structure.
- `FEATURES.md` is the plan of record: the parity matrix against pflow.xyz and sim.pflow.xyz, plus the phases. Update its Status column in the same commit as the code.

## Ground rules

- **Every action is a registry command.** `crates/core/src/commands/<namespace>.rs` holds them.
  - The app, CLI, control channel and MCP must not mutate `Session` fields directly. They call `Session::execute(id, args)`.
  - The app may *collect* arguments (file dialogs, text fields), then queue the command.
- **Nothing below `crates/app` depends on egui, eframe, winit or rfd.** `make wasm-check` must pass.
- **Firing goes through `pflow_metamodel::Model::enabled/fire`** (via `sim::Compiled`). Never reimplement the firing rule here.
- **The ODE goes through `pflow_solver`.** If a capability is missing in pflow-rs, add it upstream (FEATURES.md §D).
- **Data-producing commands store an `Artifact`** (`crates/core/src/artifact.rs`). Views render artifacts by kind, and `export.artifact` writes them out.
- **Undo is whole-snapshot.** Use `Session::edit` for document edits. A batch, or a macro played as a batch, is one undo step.
- **Queries and view commands set `journal: false`**, so they are not recorded into macros.
- **Anything that talks to pflow.xyz, sim.pflow.xyz or GitHub sits behind the `services` feature.** That covers commands, modules and app UI. The app and CLI turn it on by default, and `make build-offline` turns it off.

## Build & test

```bash
make test        # cargo test --workspace (core unit + command tests, CLI MCP stdio test)
make lint        # fmt --check + clippy -D warnings
make wasm-check  # core + app + wasm shim for wasm32-unknown-unknown
make wasm-shim   # crates/wasm → target/wasm-shim/ (.wasm + JS glue; needs wasm-bindgen-cli 0.2.129)
make test-offline  # the build without the `services` feature (no pflow.xyz / sim / GitHub)
make screenshot  # regenerate docs/screenshot-sir.png
```

To build against a sibling `../pflow-rs`, copy `.cargo/config.toml.example` to `.cargo/config.toml`. Otherwise the pinned git rev in the root `Cargo.toml` is used.

This repo lives on workbit (the Mac), because the app needs a display to run and screenshot.

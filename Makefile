.PHONY: build build-zk run test test-zk lint wasm-check web web-build screenshot mcp

build:
	cargo build --release

run:
	cargo run --release -p pflow-native -- $(ARGS)

test:
	cargo test --workspace

lint:
	cargo fmt --all --check
	cargo clippy --workspace --all-targets -- -D warnings
	cargo clippy --workspace --all-targets --features pflow-native-core/zk -- -D warnings

# ZK firing proofs are opt-in (FEATURES.md §D 9).
build-zk:
	cargo build --release --features pflow-native/zk,pflow-native-cli/zk

test-zk:
	cargo test -p pflow-native-core --features zk --test provenance

# Core + app must keep compiling for the browser.
wasm-check:
	cargo check -p pflow-native-core -p pflow-native --target wasm32-unknown-unknown

# Needs `cargo install trunk`; serves on :8080.
web:
	cd crates/app && trunk serve --release

web-build:
	cd crates/app && trunk build --release

screenshot:
	cargo run --release -p pflow-native -- --example sir \
	  --cmd 'sim.set_rate {"id":"infect","rate":0.005}' --cmd 'sim.ode {"t_end":15}' \
	  --screenshot docs/screenshot-sir.png --size 1400x900

mcp:
	cargo run --release -p pflow-native-cli -- mcp

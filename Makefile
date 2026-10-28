.PHONY: screenshot-views composition-goldens wasm-publish build build-zk build-rnn build-offline run test test-zk test-rnn test-offline lint wasm-check wasm-shim web web-build screenshot mcp

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
	cargo clippy --workspace --all-targets --features pflow-native-core/rnn -- -D warnings
	cargo clippy --workspace --all-targets --no-default-features -- -D warnings

# Without pflow.xyz, sim.pflow.xyz or GitHub: no remote.*, auth.*, open by
# CID or permalinks. Everything local, and opening a URL you give, remains.
build-offline:
	cargo build --release -p pflow-native -p pflow-native-cli --no-default-features

test-offline:
	cargo test --workspace --no-default-features

# ZK firing proofs are opt-in (FEATURES.md §D 9).
build-zk:
	cargo build --release --features pflow-native/zk,pflow-native-cli/zk

test-zk:
	cargo test -p pflow-native-core --features zk --test provenance

# Experimental RNN policies (rnn.*) are opt-in: a learned policy is a guess,
# not a verdict, and pflow-tropical is research code.
build-rnn:
	cargo build --release --features pflow-native/rnn,pflow-native-cli/rnn

test-rnn:
	cargo test -p pflow-native-core --features rnn --test rnn

# Core + app must keep compiling for the browser.
wasm-check:
	cargo check -p pflow-native-core -p pflow-native -p pflow-native-wasm --target wasm32-unknown-unknown

# The registry for pflow.xyz's browser (crates/wasm): a .wasm plus JS glue in
# target/wasm-shim/. Needs `cargo install wasm-bindgen-cli --version 0.2.129`.
wasm-shim:
	cargo build --profile wasm-release -p pflow-native-wasm --target wasm32-unknown-unknown
	wasm-bindgen --target web --out-dir target/wasm-shim \
	  target/wasm32-unknown-unknown/wasm-release/pflow_native_wasm.wasm
	@ls -l target/wasm-shim/*.wasm && gzip -9 -c target/wasm-shim/pflow_native_wasm_bg.wasm | wc -c | xargs echo gzip bytes:

# Publish the wasm-shim build to cdn.stackdump.com under its content address
# (scripts/publish-wasm.sh); prints the URL and sha256s a page pins.
#   make wasm-publish [DRAFT=0] [DRY_RUN=1] [PFLOW_XYZ=../pflow-xyz]
wasm-publish: wasm-shim
	@./scripts/publish-wasm.sh

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

# The composition views: one PNG per view x example (docs/composition-views/),
# and the parity manifests drawn headless (no window).
VIEWS := cospan spiders operad string flat boundaries lens incidence process laws
screenshot-views:
	cargo build --release -p pflow-native
	@mkdir -p docs/composition-views
	@n=0; for v in $(VIEWS); do n=$$((n+1)); for ex in orders connect4; do \
	  ./target/release/pflow-native --fresh --no-control \
	    --cmd 'view.set {"bottom_panel":false,"inspector":false}' \
	    --cmd "view.composition {\"view\":\"$$v\",\"example\":\"$$ex\"}" \
	    --screenshot docs/composition-views/$$(printf %02d $$n)-$$v-$$ex.png --size 1600x1000 || exit 1; \
	done; done

composition-goldens:
	cargo run --release -p pflow-native -- --composition-goldens parity/composition-views/goldens.json

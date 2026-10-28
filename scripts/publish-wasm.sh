#!/usr/bin/env bash
#
# publish-wasm.sh: publish the browser build (`make wasm-shim`) to
# cdn.stackdump.com, content-addressed, so pages load one engine by its hash
# instead of each vendoring a copy.
#
#   make wasm-publish                 build, then publish (as a draft: reachable by CID, not in the feed)
#   DRAFT=0 make wasm-publish         ... listed in the CDN's feed and search
#   DRY_RUN=1 make wasm-publish       print the CID and the landing page, write nothing
#   SHIM=<dir> COMMIT=<sha> VERSION=<v> ./scripts/publish-wasm.sh
#                                     publish a build made elsewhere (a .wasm and its glue),
#                                     e.g. the one a page already vendors, recording that commit
#
# What is published, one directory per build:
#
#   pflow_native_wasm_bg.wasm      the engine; its sha256 is the CID (bafyrei + first 32 hex)
#   pflow_native_wasm_bg.wasm.gz   the same bytes, gzip -9 -n (the CDN's nginx does not compress
#                                  wasm, so pages fetch this and decompress it themselves)
#   pflow_native_wasm.js           the wasm-bindgen glue, which only works with this .wasm
#
# The landing page's frontmatter records the commit, the version and every
# sha256. A page pins the URL and the .wasm's sha256 (pflow-xyz:
# pflow-native-wasm.lock) and refuses bytes that do not hash to it, so the
# CDN is a transport, never trusted for content. Paths under a CID never
# change; there is no "latest".
#
# The CDN's master is valoper (~/Workspace/cdn-stackdump-com/data/blobs); from
# any other machine this copies the build there over ssh (CDN_HOST, default
# valoper.local) and publishes with pflow-xyz's publish/cdn.py (PFLOW_XYZ,
# default ../pflow-xyz), which stages, reindexes and mirrors to pflow.dev.
# A dirty tree is refused: the commit recorded must be the one built.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SHIM="${SHIM:-$ROOT/target/wasm-shim}"
PFLOW_XYZ="${PFLOW_XYZ:-$ROOT/../pflow-xyz}"
CDN_HOST="${CDN_HOST:-valoper.local}"
CDN_TREE="$HOME/Workspace/cdn-stackdump-com/data/blobs"
DRAFT="${DRAFT:-1}"

die() { printf 'publish-wasm: %s\n' "$*" >&2; exit 1; }
sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

[ -f "$SHIM/pflow_native_wasm_bg.wasm" ] || die "no build in $SHIM: run make wasm-shim"
[ -f "$PFLOW_XYZ/publish/cdn.py" ] || die "no $PFLOW_XYZ/publish/cdn.py (set PFLOW_XYZ to a pflow-xyz checkout)"
if [ -z "${COMMIT:-}" ]; then
  git -C "$ROOT" diff --quiet HEAD -- crates Cargo.toml Cargo.lock || die "the tree differs from HEAD; commit first"
  COMMIT="$(git -C "$ROOT" rev-parse HEAD)"
fi
git -C "$ROOT" cat-file -e "$COMMIT^{commit}" 2>/dev/null || die "no commit $COMMIT here"
COMMIT="$(git -C "$ROOT" rev-parse "$COMMIT")"
VERSION="${VERSION:-$(git -C "$ROOT" show "$COMMIT:Cargo.toml" | sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' | head -1)}"
[ -n "$VERSION" ] || die "no version for $COMMIT"
git -C "$ROOT" branch -r --contains "$COMMIT" | grep -q . ||
  printf 'publish-wasm: note: %s is not on any remote branch yet\n' "${COMMIT:0:7}" >&2

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
mkdir "$STAGE/pflow-native-wasm"
OUT="$STAGE/pflow-native-wasm"
cp "$SHIM/pflow_native_wasm_bg.wasm" "$SHIM/pflow_native_wasm.js" "$OUT/"
# -n: no name or mtime in the header, so the same .wasm gives the same .gz.
gzip -9 -n -c "$OUT/pflow_native_wasm_bg.wasm" > "$OUT/pflow_native_wasm_bg.wasm.gz"
cp "$PFLOW_XYZ/publish/cdn.py" "$STAGE/cdn.py"

WASM_SHA="$(sha "$OUT/pflow_native_wasm_bg.wasm")"
GZ_SHA="$(sha "$OUT/pflow_native_wasm_bg.wasm.gz")"
JS_SHA="$(sha "$OUT/pflow_native_wasm.js")"
CID="bafyrei${WASM_SHA:0:32}"

cat > "$STAGE/body.md" <<EOF
The pflow-native engine compiled to wasm (crates/wasm, \`make wasm-shim\`),
version $VERSION, from [pflow-native ${COMMIT:0:7}](https://github.com/pflow-xyz/pflow-native/commit/$COMMIT).

Load the glue as an ES module and hand it the engine; check the bytes first:

\`\`\`js
const base = "https://cdn.stackdump.com/ipfs/$CID/";
const gz = await fetch(base + "pflow_native_wasm_bg.wasm.gz");
const bytes = await new Response(gz.body.pipeThrough(new DecompressionStream("gzip"))).arrayBuffer();
// sha256(bytes) must be $WASM_SHA
const glue = await import(base + "pflow_native_wasm.js");
await glue.default({ module_or_path: await WebAssembly.compile(bytes) });
const engine = new glue.Engine();
\`\`\`

| file | sha256 |
|---|---|
| pflow_native_wasm_bg.wasm | \`$WASM_SHA\` |
| pflow_native_wasm_bg.wasm.gz | \`$GZ_SHA\` |
| pflow_native_wasm.js | \`$JS_SHA\` |
EOF

ARGS=(pflow-native-wasm --source pflow-native --schema WasmModule/v1
  --title "pflow-native wasm engine $VERSION (${COMMIT:0:7})"
  --tag wasm --tag pflow-native --body body.md --wait
  --meta "commit=$COMMIT" --meta "version=$VERSION"
  --meta "wasm_sha256=$WASM_SHA" --meta "gzip_sha256=$GZ_SHA" --meta "js_sha256=$JS_SHA")
[ "$DRAFT" = 1 ] && ARGS+=(--draft)
[ "${DRY_RUN:-0}" = 1 ] && ARGS+=(--dry-run)

if [ -d "$CDN_TREE" ]; then
  (cd "$STAGE" && python3 cdn.py "${ARGS[@]}")
else
  REMOTE="$(ssh "$CDN_HOST" mktemp -d)"
  scp -q -r "$STAGE/." "$CDN_HOST:$REMOTE/"
  ssh "$CDN_HOST" "cd $REMOTE && python3 cdn.py $(printf '%q ' "${ARGS[@]}"); s=\$?; rm -rf $REMOTE; exit \$s"
fi

[ "${DRY_RUN:-0}" = 1 ] && exit 0
# What a page pins (pflow-xyz: scripts/sync-native-wasm.mjs --cdn <url>).
printf '{"url": "https://cdn.stackdump.com/ipfs/%s/", "commit": "%s", "version": "%s", "wasm_sha256": "%s", "gzip_sha256": "%s", "js_sha256": "%s"}\n' \
  "$CID" "$COMMIT" "$VERSION" "$WASM_SHA" "$GZ_SHA" "$JS_SHA"

#!/usr/bin/env bash
# Bundle the anonymous-voting island (assets/vote-island.js) with its one
# dependency into a single self-hosted file, static/vote.min.js. Run from the
# repo root; commit the result. Vendored like htmx: built here, not at runtime.
# No WASM; the bundle calls only the same-origin token/cast endpoints.
set -euo pipefail
cd "$(dirname "$0")/.."
REPO="$(pwd)"
SRC="$REPO/crates/server/assets/vote-island.js"
OUT="$REPO/crates/server/static/vote.min.js"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"
npm init -y >/dev/null 2>&1
npm install --no-audit --no-fund @cloudflare/blindrsa-ts@0.4.6 >/dev/null 2>&1
cp "$SRC" island.js
npx --yes esbuild@0.23.1 island.js --bundle --format=esm --minify --outfile="$OUT"
echo "wrote $OUT ($(wc -c < "$OUT") bytes)"

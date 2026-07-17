#!/usr/bin/env bash
# Build the Tailwind stylesheet with the standalone Tailwind CLI.
#
# Downloads a pinned CLI on first run (cached in .bin/), then compiles the
# input into the served stylesheet. Extra flags pass through, e.g.
#   ./scripts/tailwind.sh --watch
set -euo pipefail

cd "$(dirname "$0")/.."

TAILWIND_VERSION="v4.3.2"
BIN_DIR=".bin"
BIN="${BIN_DIR}/tailwindcss"
INPUT="crates/server/assets/tailwind.css"
OUTPUT="crates/server/static/app.css"

if [ ! -x "$BIN" ]; then
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)  target="linux-x64" ;;
    Linux-aarch64) target="linux-arm64" ;;
    Darwin-x86_64) target="macos-x64" ;;
    Darwin-arm64)  target="macos-arm64" ;;
    *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
  esac
  url="https://github.com/tailwindlabs/tailwindcss/releases/download/${TAILWIND_VERSION}/tailwindcss-${target}"
  echo "downloading tailwindcss ${TAILWIND_VERSION} (${target})..."
  mkdir -p "$BIN_DIR"
  curl -fsSL "$url" -o "$BIN"
  chmod +x "$BIN"
fi

exec "./${BIN}" -i "$INPUT" -o "$OUTPUT" --minify "$@"

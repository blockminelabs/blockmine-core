#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
MINER_ROOT="$ROOT/miner-client"
DIST_DIR="$ROOT/dist"

if [[ "$OSTYPE" != darwin* ]]; then
  echo "This packaging flow must be run on macOS."
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is not installed. Install it first with:"
  echo "curl https://sh.rustup.rs -sSf | sh"
  exit 1
fi

if ! command -v hdiutil >/dev/null 2>&1; then
  echo "hdiutil is missing. This script must be run on a Mac."
  exit 1
fi

mkdir -p "$DIST_DIR"

echo "Building unified Blockmine Miner for macOS ..."
pushd "$MINER_ROOT" >/dev/null
cargo build --release --bin blockmine-studio --features opencl
BIN_PATH="$MINER_ROOT/target/release/blockmine-studio"
popd >/dev/null

"$SCRIPT_DIR/package-macos.sh" "$BIN_PATH"

echo
echo "Done."
echo "Artifacts:"
echo "  $DIST_DIR/Blockmine Miner.app"
echo "  $DIST_DIR/Blockmine Miner.dmg"

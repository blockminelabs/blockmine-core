#!/bin/bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$DIR/../.." && pwd)"
cd "$ROOT/miner-client"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Rust is not installed. Install it first with:"
  echo "curl https://sh.rustup.rs -sSf | sh"
  exit 1
fi

cargo run --release --features opencl -- list-devices

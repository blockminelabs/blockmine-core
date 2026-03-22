#!/usr/bin/env bash

set -euo pipefail

source "$HOME/.cargo/env"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.local/anchor-0.30.1/bin:$PATH"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MINER_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OPENCL_LIB_DIR="$MINER_ROOT/native/windows-opencl"
export LIBRARY_PATH="$OPENCL_LIB_DIR:${LIBRARY_PATH:-}"
export RUSTFLAGS="-L native=$OPENCL_LIB_DIR ${RUSTFLAGS:-}"

cd "$MINER_ROOT"

cargo build --release --target x86_64-pc-windows-gnu "$@"

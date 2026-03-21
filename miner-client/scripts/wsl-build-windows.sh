#!/usr/bin/env bash

set -euo pipefail

source "$HOME/.cargo/env"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.local/anchor-0.30.1/bin:$PATH"
export LIBRARY_PATH="/mnt/c/Users/drums/Desktop/BLOC/miner-client/native/windows-opencl:${LIBRARY_PATH:-}"
export RUSTFLAGS="-L native=/mnt/c/Users/drums/Desktop/BLOC/miner-client/native/windows-opencl ${RUSTFLAGS:-}"

cd /mnt/c/Users/drums/Desktop/BLOC/miner-client

cargo build --release --target x86_64-pc-windows-gnu "$@"

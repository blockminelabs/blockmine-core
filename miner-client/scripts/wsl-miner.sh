#!/usr/bin/env bash

set -euo pipefail

source "$HOME/.cargo/env"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"

cd /mnt/c/Users/drums/Desktop/BLOC/miner-client

./target/debug/blockmine-miner "$@"

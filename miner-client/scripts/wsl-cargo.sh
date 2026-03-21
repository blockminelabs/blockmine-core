#!/usr/bin/env bash

set -euo pipefail

source "$HOME/.cargo/env"
export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.local/anchor-0.30.1/bin:$PATH"

cd /mnt/c/Users/drums/Desktop/BLOC/miner-client

cargo "$@"

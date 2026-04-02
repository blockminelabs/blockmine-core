# Packaging

The shared source of truth is:

- `miner-client/`
- `onchain/`

This folder only contains platform packaging wrappers.

- `windows/` builds the Windows desktop miner
- `macos/` builds the macOS desktop miner

Product logic belongs in `miner-client/`. Packaging scripts should not be used as the primary place to modify miner behavior.

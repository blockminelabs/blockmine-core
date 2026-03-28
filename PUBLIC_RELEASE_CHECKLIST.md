# Public Release Checklist

This checklist is for the moment when the core repository is opened publicly after the whale phase and after the final immutable posture is chosen.

## Contract posture

- remove the admin instruction surface from the public mainline if the deployed contract is meant to be immutable
- remove these public entrypoints from `onchain/programs/blockmine/src/lib.rs` if no longer needed:
  - `set_paused`
  - `rotate_admin`
  - `update_difficulty_params`
  - `update_runtime_params`
  - `update_treasury_accounts`
  - `reset_protocol`
- remove `onchain/programs/blockmine/src/instructions/admin.rs`
- remove `pub mod admin;` and `pub use admin::*;` from `onchain/programs/blockmine/src/instructions/mod.rs`
- regenerate the IDL and verify the admin surface is gone
- remove upgrade authority on the deployed mainnet program
- revoke mint authority
- revoke freeze authority

## Public repo hygiene

- remove `miner-client/src/bin/devnet-admin.rs`
- remove or archive devnet-only helpers under `onchain/scripts/`:
  - `devnet-bootstrap.mjs`
  - `update-difficulty-devnet.mjs`
  - `update-runtime-devnet.mjs`
  - `update-treasury-devnet.mjs`
  - `verify-devnet.mjs`
- decide whether `[programs.devnet]` should remain in `onchain/Anchor.toml`
- keep only public-safe scripts and packaging flows

## Documentation

- ensure README describes the immutable public posture, not the tuning posture
- add the final verified build / source verification instructions
- keep public mainnet references aligned:
  - program ID
  - mint
  - treasury authority
- remove any remaining devnet/testnet language from user-facing docs

## Distribution and verification

- tag the exact public release commit
- publish the open-source repo at that exact commit
- generate and publish the verified build flow for the deployed program
- publish desktop binaries that match the public source

## Final review

- no private keys, seeds, wallets, runbooks, passwords, or server notes in repo
- no dead scripts referencing folders that do not exist
- no private operational paths baked into public scripts
- no stale docs describing pre-mainnet behavior

# BlockMine Architecture

## Monorepo

- `onchain/`: Anchor workspace and Solana program
- `miner-client/`: Rust CLI miner and lightweight admin/bootstrap commands
- `web/`: Next.js dashboard, explorer, and wallet UX
- `scripts/`: PowerShell wrappers for local bootstrap and Devnet flows
- `docs/`: technical notes for protocol, launch, and operations

## Core components

### On-chain program

The Solana program owns the protocol state and reward release logic. It does not mine and does not use the GPU. It only:

- stores protocol configuration
- stores the current logical block
- verifies `SHA256(challenge || miner_pubkey || nonce)`
- compares the hash against a target
- records solved block history
- transfers BLOC rewards from the reward vault to the winner and treasury
- opens the next logical block
- periodically adjusts difficulty

### Miner client

The miner is a Rust CLI that:

- reads protocol accounts from RPC
- searches nonces off-chain on CPU, GPU, or both
- can enumerate generic OpenCL platforms and devices
- submits winning nonces on-chain
- can register miner metadata
- can initialize the protocol during Devnet bootstrap

The engine abstraction is intentionally simple so CPU and GPU backends can share the same RPC, signing, and submission flow.

### Web app

The dashboard is a read-focused interface:

- landing page
- protocol HUD
- block explorer
- leaderboard
- mining page with wallet connect and CLI guidance

The web app uses direct RPC reads and manual decoding of Anchor accounts, so it can work without a separate backend in V1.

## Account model

### `ProtocolConfig` PDA

Single global protocol config. Stores:

- admin authority
- BLOC mint
- reward vault
- treasury authority and treasury vault
- max supply
- reward schedule
- treasury fee schedule
- difficulty schedule
- current block number
- block timing config
- paused flag

### `CurrentBlock` PDA

Single mutable account for the open logical block. Stores:

- block number
- current challenge
- current target and difficulty bits
- reward for the current block
- timestamps
- winner fields for the last solved state before rotation

### `MinerStats` PDA

One PDA per miner wallet. Stores:

- winning submissions recorded on-chain
- valid blocks found
- total rewards earned
- last submission time
- nickname bytes

### `BlockHistory` PDA

One PDA per solved block. Stores:

- block number
- winner
- reward
- nonce
- hash
- timestamp
- difficulty snapshot
- challenge snapshot

## Data flow

1. Admin creates BLOC mint on Devnet.
2. Admin initializes the BlockMine program with the mint.
3. The program creates the reward vault ATA owned by the vault PDA.
4. Admin mints the full `21,000,000 BLOC` supply to the reward vault.
5. Admin revokes mint authority.
6. Miners fetch challenge and target through RPC.
7. Miners search nonces off-chain.
8. Winner submits a solution transaction.
9. Program verifies, pays the reward, records history, and rotates the block.

## Why this shape

This design keeps V1 realistic for Solana:

- only one mutable current-block account, so races are naturally serialized by account locks
- immediate reward transfer, so there is no claim backlog to manage
- fixed-supply rewards are enforced by vault depletion plus mint-authority revocation
- difficulty uses `difficulty_bits -> target` because full 256-bit dynamic math is heavier than necessary for V1

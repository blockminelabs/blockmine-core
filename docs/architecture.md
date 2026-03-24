# Blockmine Architecture

## Repository layout

- `onchain/`: Anchor workspace and Solana program
- `miner-client/`: Rust desktop miner and devnet/bootstrap tooling
- `docs/`: technical protocol notes and supporting documentation

## Core components

### On-chain program

The Solana program owns the canonical protocol state and reward release logic. It does not mine and it does not use the GPU. It only:

- stores protocol configuration
- stores the current logical block
- verifies `SHA256(challenge || miner_pubkey || nonce)`
- compares the hash against a live target
- records solved block history
- transfers BLOC rewards from the reward vault to the miner and treasury
- retargets the next block difficulty
- opens the next logical block
- restores liveness through permissionless stale rotation

The reward schedule is keyed to **successfully settled blocks**, so a stale rotation does not silently burn emissions.

### Miner client

The desktop miner is a Rust application that:

- reads protocol accounts from RPC
- searches nonces off-chain on CPU, GPU, or both
- supports Windows and macOS desktop mining
- submits winning nonces on-chain
- can register miner metadata
- can authorize delegated mining sessions
- includes devnet/bootstrap commands while the protocol is still tuning

The engine abstraction is intentionally simple so CPU and GPU backends can share the same RPC, signing, and submission flow.

## Account model

### `ProtocolConfig` PDA

Single global protocol config. Stores:

- admin authority during devnet tuning
- BLOC mint
- reward vault
- treasury authority and treasury vault
- max supply metadata
- treasury fee configuration
- difficulty configuration
- live block counters
- timing configuration
- paused flag

### `CurrentBlock` PDA

Single mutable account for the open logical block. Stores:

- block number
- current challenge
- current target and difficulty bits
- reward for the current block
- timestamps
- status and winner metadata

### `MinerStats` PDA

One PDA per miner wallet. Stores:

- accepted submissions recorded on-chain
- valid blocks found
- total rewards earned
- last submission time
- nickname bytes

### `MiningSession` PDA

One PDA per delegated mining session. Stores:

- canonical miner
- delegate
- expiry
- submission cap
- active flag

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

1. Create the SPL mint.
2. Initialize the Blockmine protocol with the canonical mint, vault, and treasury accounts.
3. Fund the reward vault with the `20,000,000 BLOC` mining allocation.
4. Hold the `550,000 BLOC` launch LP allocation separately.
5. Hold the `450,000 BLOC` treasury reserve separately.
6. Revoke mint authority after full allocation.
7. Miners fetch challenge and target through RPC.
8. Miners search nonces off-chain.
9. A winner submits a solution transaction.
10. The program verifies, pays the reward, records history, retargets difficulty, and rotates the block.

## Why this shape

This design keeps the protocol realistic for Solana:

- only one mutable current-block account, so races are naturally serialized by account locks
- immediate reward transfer, so there is no claim backlog to manage
- fixed-supply rewards are enforced by pre-funding plus mint-authority revocation
- difficulty uses a full 256-bit target with bounded retargeting instead of trusting client throughput claims
- stale recovery preserves liveness without silently skipping scheduled emissions

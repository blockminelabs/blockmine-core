# Blockmine Architecture

## Repository layout

- `onchain/`: Anchor workspace and Solana program
- `miner-client/`: Rust miner CLI and desktop client
- `docs/`: public technical notes for the core repo
- `packaging/`: Windows and macOS packaging helpers

## System shape

Blockmine splits mining into two layers:

- **off-chain search**
  - miners fetch the live block snapshot
  - CPU and GPU hardware iterate nonces
  - only a winning solution is sent on-chain
- **on-chain settlement**
  - the program stores canonical protocol state
- the program verifies the submitted proof
- the program routes rewards and fees
- the program opens the next logical block
- the program emits a canonical solved-block event trail

This keeps brute-force hashing off-chain while preserving deterministic settlement on Solana.

## On-chain program

The Solana program does not mine. It owns the canonical state machine:

- stores protocol configuration
- stores the currently open block
- verifies `SHA256(challenge || miner_pubkey || nonce)`
- compares the result against the live target
- transfers miner and treasury rewards from the reward vault
- transfers the fixed accepted-block `SOL` fee to the treasury authority
- emits solved block history as canonical events
- retargets difficulty
- rotates stale blocks to preserve liveness

Era progression advances on **successfully settled blocks**, not on raw block openings. A stale rotation does not burn scheduled emissions.

## Miner client

The Rust miner stack includes:

- CLI mining commands for CPU, GPU, and hybrid execution
- protocol inspection commands
- registration and test submission flows
- a desktop app for Windows and macOS
- local wallet management
- QR-assisted manual funding for the desktop wallet

The desktop app and the CLI use the same RPC, signing, and submission path.

## Account model

### `ProtocolConfig` PDA

Global protocol configuration:

- admin authority
- BLOC mint
- reward vault
- treasury authority and treasury vault
- timing configuration
- fee configuration
- difficulty configuration
- aggregate counters

### `CurrentBlock` PDA

Mutable live block state:

- block number
- challenge
- target
- difficulty bits
- reward
- open and expiry timestamps
- winner metadata

### `MinerStats` PDA

One PDA per miner:

- accepted submissions recorded on-chain
- valid blocks found
- total rewards earned
- nickname
- last submission time

### `MiningSession` PDA

Delegated session state:

- canonical miner
- delegate
- expiry
- submission cap
- active flag

### Solved-block event trail

Accepted blocks are still traceable, but that history now lives in the protocol event stream instead of a rent-bearing PDA created for every solved block.

The emitted solved-block data includes:

- block number
- winner
- reward
- nonce
- hash
- challenge snapshot
- difficulty snapshot

## Token and vault flow

Mainnet supply is fixed at `21,000,000 BLOC`:

- `20,000,000 BLOC` in the reward vault
- `450,000 BLOC` in the treasury vault
- `550,000 BLOC` held separately for LP provisioning

The reward schedule only applies to the `20,000,000 BLOC` mining allocation.

## Deployment posture

This public repo is meant to describe the open technical core.

Operational items stay outside the repo:

- launch wallet bundles
- private runbooks
- SSH material
- production server secrets

Mainnet hardening remains a deployment concern on top of this codebase:

- revoke mint authority after allocation
- revoke freeze authority
- remove upgrade authority
- remove or lock down admin controls if an immutable posture is required

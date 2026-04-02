# Architecture

## Model

Blockmine separates proof search from settlement.

- search happens on local hardware
- settlement happens on Solana

The chain is used as a deterministic state machine. It does not waste compute units on brute-force work.

## Data flow

### 1. Read path

The miner fetches:

- `ProtocolConfig`
- `CurrentBlock`
- the miner's `MinerStats`

These are normal RPC reads. The protocol does not need to be notified that a miner is hashing.

### 2. Local search

The miner constructs the preimage:

```text
challenge || miner_pubkey || nonce_le_u64
```

and iterates nonces on CPU, GPU, or both. The search loop is entirely local.

### 3. Local validation

When a candidate hash is found, the miner can compare it locally against the current 256-bit target before building a transaction.

### 4. Submission

Only a winning candidate is sent to the chain. The transaction does not contain a large work trace. It contains the nonce and the accounts required for settlement.

### 5. On-chain verification

The program recomputes the hash, compares it to the target, charges the accepted-block fee, routes BLOC rewards, records the winner, emits events, and opens the next block.

## Program structure

### State

- `ProtocolConfig` - global constants and aggregate counters
- `CurrentBlock` - the single live block
- `MinerStats` - per-miner statistics
- `MiningSession` - optional delegate authorization

### Math

- `math/difficulty.rs` - full-target retarget logic
- `math/rewards.rs` - era schedule and exact capped emissions

### Instructions

- `initialize_protocol`
- `register_miner`
- `update_nickname`
- `submit_solution`
- `authorize_mining_session`
- `submit_solution_with_session`
- `rotate_stale_block`

## Canonical public history

Accepted block history is recorded in events:

- `BlockOpened`
- `BlockSolved`
- `DifficultyAdjusted`
- `BlockStaleRotated`

This design keeps the public history readable without forcing a new rent-bearing account on every solved block.

## Wallet and reward flow

The protocol uses three token destinations:

- reward vault - pre-funded mining inventory
- miner ATA - beneficiary destination for accepted rewards
- treasury ATA - protocol treasury share of accepted rewards

The SOL submit fee is routed to the treasury wallet, not to the reward vault.

## Desktop miner

The desktop client is a thin execution layer on top of the same protocol.

It manages:

- local wallets
- CPU and GPU device selection
- mining mode selection
- telemetry rendering
- submission to the canonical program

The desktop client does not maintain its own private accounting system. It reads on-chain state and submits transactions against the same public program.

## Emission indexing

Two counters matter:

- `current_block_number`
- `total_blocks_mined`

They are not the same quantity.

- `current_block_number` tracks the current logical tip
- `total_blocks_mined` tracks successfully settled blocks only

The reward schedule is indexed by `total_blocks_mined`. Stale rotations therefore preserve emissions instead of silently burning them.

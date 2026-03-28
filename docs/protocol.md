# Blockmine Protocol

## Goal

Blockmine implements a proof-of-work block race on top of Solana.

Miners compete off-chain for `BLOC`. Solana acts as the canonical state, settlement, and accounting layer.

## Proof rule

V1 uses:

`hash = SHA256(challenge || miner_pubkey || nonce)`

Validity rule:

`hash < target`

Including the miner pubkey in the proof prevents the simplest form of nonce theft because a copied nonce will not validate for a different wallet.

## Block lifecycle

1. the protocol opens one canonical logical block
2. miners fetch the challenge, target, reward, and expiry
3. hardware searches nonces off-chain
4. a winner submits a valid solution
5. the program verifies the hash and target
6. the fixed `0.01 SOL` accepted-block fee is routed to the treasury authority
7. the `BLOC` reward is split:
   - `99%` to the miner
   - `1%` to the treasury vault
8. block history is written
9. the next block is opened with a fresh challenge and target

If a block expires before a valid solve arrives, anyone can rotate the stale block and keep the protocol live.

## Challenge rotation

The next challenge is derived from live settlement context, including:

- previous winning hash
- previous challenge
- winner pubkey
- winning nonce
- next block number
- slot and timestamp data

This prevents cross-block replay because every logical block has a new challenge.

## Difficulty

The protocol stores:

- `difficulty_bits`
- `difficulty_target`

`difficulty_bits` is a compact display value. The actual acceptance check uses the full `256-bit` target stored on-chain.

Retargeting happens on every solved block:

- expected duration = configured target block time
- observed duration = `solved_at - opened_at`
- the full target is scaled
- extreme outliers are clamped
- min and max bounds are enforced

This keeps the protocol responsive to changing hashrate without trusting any client-reported throughput.

## Reward model

Blockmine does not use an infinite tail and does not use a simple binary halving schedule.

Instead, V1 uses a named era schedule over the `20,000,000 BLOC` mining allocation:

- Genesis
- Aurum
- Phoenix
- Horizon
- Quasar
- Pulsar
- Voidfall
- Eclipse
- Mythos
- Paragon
- Hyperion
- Singularity
- Eternal I
- Eternal II
- Scarcity

Era progression advances on **successfully settled blocks**. Stale rotations do not burn scheduled emissions.

The final Scarcity tail is capped so the mining schedule stops exactly at `20,000,000 BLOC`.

## Settlement guarantees

The core V1 guarantees come from structure:

- one mutable current-block account
- one deterministic history record per solved block
- one reward vault owned by the program PDA
- one canonical treasury route loaded from config

That means the contract is not a passive faucet. It is the state machine that defines:

- who won
- what reward was due
- where the treasury fee went
- what block opens next

## Sessions

The protocol also supports delegated mining sessions:

- a canonical miner can authorize a delegate wallet
- the delegate submits solutions on behalf of the miner
- limits can be set through expiry and submission cap

This is useful for browser-to-desktop handoff and wallet-delegated mining flows, while keeping settlement tied to the canonical miner identity.

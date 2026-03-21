# BlockMine Protocol

## Goal

BlockMine implements a Bitcoin-like block race on top of Solana. Solana is the state and settlement layer. Miners are competing for `BLOC`, not for `SOL`.

## Proof of work

V1 uses:

`hash = SHA256(challenge || miner_pubkey || nonce)`

Validity rule:

`hash < target`

## Why the miner pubkey is included

Binding the wallet into the hash prevents the simplest form of solution theft:

- Miner A finds `(nonce, hash)`.
- Miner B sees the nonce in the mempool and copies it.
- Because Miner B has a different pubkey, the recomputed hash is different and invalid.

This does not remove every mempool concern, but it is a solid V1 mitigation without commit-reveal complexity.

## Challenge rotation

The next challenge is derived from:

- previous winning hash
- previous challenge
- winner pubkey
- winning nonce
- next block number
- current slot
- current timestamp

This prevents cross-block replay because the challenge changes every block.

## Difficulty

V1 stores:

- `difficulty_bits`
- `difficulty_target`

The target is derived from `difficulty_bits` by forcing a prefix of zero bits and filling the remaining bytes with `0xff`.

This gives a cheap on-chain model with a real target comparison, while keeping adjustment math easy to reason about and test.

## Difficulty adjustment

The protocol now retargets on every solved block.

Inputs:

- expected duration = `target_block_time_sec`
- observed duration = `solved_at - opened_at`

Rules:

- smooth the observed block time toward the target before reacting
- clamp extreme outliers so one weird block does not overreact
- if blocks are faster than target, increase difficulty by 1-2 bits
- if blocks are slower than target, decrease difficulty by 1-2 bits
- clamp within configured min/max bits

This keeps the system converging toward the configured average block time without using a hard timer or cooldown.

## Rewards

Reward per block is derived by:

`initial_block_reward >> halvings`

Where:

- `halvings = block_number / halving_interval`

This mirrors the spirit of Bitcoin while staying cheap on-chain.

## Reward model

V1 uses immediate payout:

- reward is split inside one instruction
- winner receives `99%` of the scheduled block reward in their ATA
- treasury receives `1%` of the scheduled block reward in its ATA
- no claim instruction is required in the happy path

This is simpler than a pending-claim ledger and reduces the amount of user-facing state.

## Rejections and stats

One implementation caveat matters:

- on Solana, a failed transaction rolls back account mutations
- because of that, V1 miner stats only persist successful winning submissions

If you want durable invalid-attempt telemetry, V2 should add a separate accepted-but-rejected accounting path or an indexer.

## Lifecycle

1. `initialize_protocol`
2. block `0` is opened
3. miners read challenge and target
4. off-chain nonce search begins
5. a valid solution is found
6. `submit_solution`
7. program verifies hash and target
8. reward is transferred
9. block history is written
10. next block is opened

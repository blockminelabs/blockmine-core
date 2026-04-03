<p align="center">
  <a href="../README.md#overview"><img alt="Home" src="https://img.shields.io/badge/Home-151515?style=for-the-badge"></a>
  <a href="protocol.md"><img alt="Protocol" src="https://img.shields.io/badge/Protocol-f7931a?style=for-the-badge"></a>
  <a href="architecture.md"><img alt="Architecture" src="https://img.shields.io/badge/Architecture-1f2937?style=for-the-badge"></a>
  <a href="miner-client.md"><img alt="Miner" src="https://img.shields.io/badge/Miner-374151?style=for-the-badge"></a>
  <a href="vast-ai.md"><img alt="Vast.ai" src="https://img.shields.io/badge/Vast.ai-0f766e?style=for-the-badge"></a>
  <a href="security-notes.md"><img alt="Security" src="https://img.shields.io/badge/Security-7c2d12?style=for-the-badge"></a>
  <a href="tokenomics.md"><img alt="Tokenomics" src="https://img.shields.io/badge/Tokenomics-92400e?style=for-the-badge"></a>
  <a href="../MINING_CURVE.md"><img alt="Mining Curve" src="https://img.shields.io/badge/Mining_Curve-b45309?style=for-the-badge"></a>
  <a href="../LIVE_CONFIG_NOTES.md"><img alt="Live Config" src="https://img.shields.io/badge/Live_Config-4b5563?style=for-the-badge"></a>
</p>

# Protocol

This document describes the on-chain state machine implemented by the Blockmine program.

## Canonical accounts

### `ProtocolConfig`

Global configuration:

- `admin`
- `bloc_mint`
- `reward_vault`
- `treasury_authority`
- `treasury_vault`
- `max_supply`
- `current_block_number`
- `total_blocks_mined`
- `total_rewards_distributed`
- `total_treasury_fees_distributed`
- difficulty, timing, and fee parameters

The important distinction is:

- `current_block_number` advances whenever the protocol opens a new logical block
- `total_blocks_mined` advances only when a block is successfully settled

Those counters may diverge whenever stale rotation occurs.

### `CurrentBlock`

The single mutable block in play:

- `block_number`
- `challenge`
- `difficulty_bits`
- `difficulty_target`
- `block_reward`
- `opened_at`
- `expires_at`
- `winner`
- `winning_nonce`
- `winning_hash`
- `solved_at`

### `MinerStats`

One record per miner:

- `total_submissions`
- `valid_blocks_found`
- `total_rewards_earned`
- `last_submission_time`
- `nickname`

### `MiningSession`

Optional delegated session keyed by canonical miner:

- `miner`
- `delegate`
- `expires_at`
- `max_submissions`
- `submissions_used`
- `active`

## Initialization

`initialize_protocol` accepts the mint, the reward vault, the treasury wallet, and the treasury ATA, then enforces a set of invariants.

The important invariants are:

- `treasury_fee_bps == 100`
- `submit_fee_lamports == 10_000_000`
- `max_supply == 20_000_000 * 10^9`
- `initial_block_reward == Genesis reward`
- `reward_vault.amount == TOTAL_PROTOCOL_EMISSIONS`
- `reward_vault != treasury_vault`

Genesis opens with:

- `block_number = 0`
- `status = OPEN`
- `challenge = SHA256("blockmine-genesis" || admin || mint || slot || timestamp)`
- `difficulty_target = target_from_difficulty_bits(initial_difficulty_bits)`
- `block_reward = Genesis reward`

## Proof rule

For a given `CurrentBlock`, the accepted proof is:

```text
H = SHA256(challenge || miner_pubkey || nonce_le_u64)
```

Acceptance rule:

```text
H < difficulty_target
```

This rule is deterministic and miner-bound. Replaying the same nonce under another pubkey changes the preimage and therefore changes the hash.

## Submit path

`submit_solution` and `submit_solution_with_session` both converge on the same settlement routine.

The program checks, in order:

1. protocol is not paused
2. block is open
3. block is not expired
4. reward state matches `total_blocks_mined`
5. candidate hash satisfies the target
6. reward vault holds enough BLOC

If the candidate is valid, settlement proceeds in this order:

1. transfer `submit_fee_lamports` to `treasury_authority`
2. compute:
   - `treasury_reward = gross_reward * treasury_fee_bps / 10000`
   - `miner_reward = gross_reward - treasury_reward`
3. transfer `miner_reward` from `reward_vault` to the miner ATA
4. transfer `treasury_reward` from `reward_vault` to `treasury_vault`
5. update `MinerStats`
6. mark the block solved
7. emit `BlockSolved`
8. increment `total_blocks_mined`
9. increment aggregate reward counters
10. derive and open the next block

If any step fails, the instruction aborts and the state transition is not partially committed.

## Delegated sessions

Delegated sessions authorize one delegate wallet to submit on behalf of a canonical miner.

The delegated path still binds rewards and proof ownership to the canonical miner:

- the proof uses `miner_pubkey`, not the delegate key
- the miner ATA receives the BLOC reward
- the delegate only pays fees and submits the transaction

This is the mechanism used for delegated mining flows without surrendering miner identity.

## Next challenge

After a solved block, the next challenge is:

```text
SHA256(
  "blockmine-next" ||
  winning_hash ||
  previous_challenge ||
  winner_pubkey ||
  winning_nonce_le_u64 ||
  next_block_number_le_u64 ||
  slot_le_u64 ||
  unix_timestamp_le_i64
)
```

The previous winning proof is therefore part of the next challenge seed.

## Difficulty retarget

Difficulty is stored as a full target and as a display bit count.

The retarget routine:

- measures the observed block duration
- smooths the observation
- clamps outliers
- scales the full target
- enforces bounds from `min_difficulty_bits` and `max_difficulty_bits`

The target is the canonical acceptance threshold. `difficulty_bits` is derived from the target and exists as a readable compression.

## Stale rotation

If `block_ttl_sec` is enabled and a block expires unsolved, anyone may call `rotate_stale_block`.

That path:

- requires the current block to be open and expired
- increments `current_block_number`
- derives a fresh challenge
- resets difficulty to `min_difficulty_bits`
- preserves reward indexing by using `reward_era_for_open_block(total_blocks_mined)`
- emits `BlockStaleRotated`
- emits `BlockOpened`

Important consequence:

- stale rotation advances the live block number
- stale rotation does not advance emissions

This is why `current_block_number` can be greater than `total_blocks_mined`.

## Event surface

The program emits the canonical public trace of state transitions:

- `ProtocolInitialized`
- `BlockOpened`
- `BlockSolved`
- `DifficultyAdjusted`
- `BlockStaleRotated`
- `MinerRegistered`
- `MiningSessionAuthorized`

Accepted block history therefore exists in the event stream rather than as a rent-bearing PDA per solved block.

## Monetary flow

For each accepted block:

- the miner pays `0.01 SOL`
- the treasury wallet receives that `0.01 SOL`
- the reward vault pays the gross BLOC reward
- the treasury ATA receives `1%` of that BLOC reward
- the miner ATA receives the remaining `99%`

The reward vault is a pre-funded inventory account. The protocol does not mint per block.

# Blockmine Core

Blockmine is a proof-of-work settlement protocol on Solana.

The chain does not brute-force hashes. The chain publishes one canonical challenge, one canonical target, one canonical reward, and one canonical settlement path. CPU and GPU hardware search nonces off-chain. The program verifies a valid proof, settles rewards, routes fees, and opens the next block.

## Mainnet references

- Program ID: `FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv`
- Mint: `9AJa38FiS8kD2n2Ztubrk6bCSYt55Lz2fBye3Comu1mg`
- Treasury wallet: `8DVGdWLzDu8mXV8UuTPtqMpdST6PY2eoEAypK1fARCMb`
- Default RPC in the public miner: `https://api.mainnet-beta.solana.com`

## Repository layout

- `onchain/` - Anchor workspace and Solana program
- `miner-client/` - Rust CLI miner and desktop client
- `docs/` - protocol, miner, architecture, security, and tokenomics notes
- `packaging/` - Windows and macOS packaging helpers
- `scripts/` - local build wrappers for the public binaries

## Proof function

The accepted proof is computed as:

```text
H = SHA256(challenge || miner_pubkey || nonce_le_u64)
```

where:

- `challenge` is the 32-byte challenge stored in `CurrentBlock`
- `miner_pubkey` is the 32-byte public key of the beneficiary miner
- `nonce_le_u64` is the 8-byte little-endian nonce

A candidate is valid iff:

```text
H < target
```

`target` is the full 256-bit target stored on-chain. The comparison is a raw big-endian byte comparison, which is equivalent to an unsigned 256-bit integer comparison.

The same rule is enforced by the miner and by the program. The program recomputes the hash during settlement and does not trust client-side claims.

## Canonical state

The protocol uses four principal state objects.

### `ProtocolConfig`

Global configuration and aggregate counters:

- mint
- reward vault
- treasury route
- difficulty bounds
- timing parameters
- fee parameters
- total settled blocks mined
- total rewards distributed

### `CurrentBlock`

The single mutable live block:

- block number
- challenge
- 256-bit target
- display difficulty bits
- gross block reward
- open and expiry timestamps
- winning metadata after settlement

### `MinerStats`

Per-miner accounting:

- accepted submissions
- valid blocks found
- total rewards earned
- last submission time
- nickname

### `MiningSession`

Optional delegated session state:

- canonical miner
- delegate
- expiry
- submission cap
- active flag

## Settlement path

For each accepted block, the program performs the following sequence:

1. Read the canonical `CurrentBlock`.
2. Recompute `SHA256(challenge || miner_pubkey || nonce_le_u64)`.
3. Reject if the block is closed, expired, invalid, or out of reward state.
4. Transfer the fixed accepted-block fee of `0.01 SOL` to the treasury wallet.
5. Split the BLOC block reward:
   - `treasury_reward = gross_reward * treasury_fee_bps / 10000`
   - `miner_reward = gross_reward - treasury_reward`
6. Transfer `miner_reward` from the reward vault to the miner ATA.
7. Transfer `treasury_reward` from the reward vault to the treasury ATA.
8. Emit `BlockSolved`.
9. Increment `total_blocks_mined`.
10. Derive the next challenge.
11. Retarget difficulty.
12. Open the next block.

There is one writable `CurrentBlock`. Competing winners race on the same mutable state. Solana account locking serializes the settlement path, so only one settlement can close a given block.

## Challenge derivation

Genesis is opened from:

```text
SHA256(
  "blockmine-genesis" ||
  admin_pubkey ||
  mint_pubkey ||
  slot_le_u64 ||
  unix_timestamp_le_i64
)
```

Each subsequent solved block derives the next challenge from:

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

Each stale rotation derives the next challenge from:

```text
SHA256(
  "blockmine-stale-rotate" ||
  previous_challenge ||
  caller_pubkey ||
  stale_block_number_le_u64 ||
  next_block_number_le_u64 ||
  slot_le_u64 ||
  unix_timestamp_le_i64
)
```

The result is a fresh challenge for every logical block. A proof for one block does not replay onto the next.

## Difficulty

Difficulty is stored in two forms:

- `difficulty_bits` - compact display value
- `difficulty_target` - full 256-bit acceptance target

The acceptance rule always uses the full target. `difficulty_bits` is derived from the target and exists as a readable scalar.

Retargeting occurs after every solved block. The program:

- measures `observed_seconds = solved_at - opened_at`
- compares it to the configured target block time
- scales the full 256-bit target
- clamps extreme outliers
- enforces minimum and maximum difficulty bounds

If a block goes stale, `rotate_stale_block` preserves liveness and advances `current_block_number`, but it does not increment `total_blocks_mined`. Emissions therefore advance on settled work, not on expired timers.

## Emissions

Blockmine has a fixed supply of `21,000,000 BLOC`.

- `20,000,000 BLOC` are locked into the reward vault and emitted by the protocol
- `550,000 BLOC` are reserved for initial liquidity
- `450,000 BLOC` are treasury reserve inventory

The reward schedule is not a binary halving table. It is a named era schedule capped exactly at `20,000,000 BLOC` of mining emissions. Era progression is indexed by `total_blocks_mined`, not by raw block openings. The exact schedule is documented in [MINING_CURVE.md](MINING_CURVE.md).

## Event trail

Accepted block history is carried by protocol events, not by rent-bearing per-block PDAs.

The main events are:

- `ProtocolInitialized`
- `BlockOpened`
- `BlockSolved`
- `DifficultyAdjusted`
- `BlockStaleRotated`
- `MinerRegistered`
- `MiningSessionAuthorized`

`BlockSolved` includes the block number, winner, nonce, winning hash, challenge snapshot, difficulty snapshot, gross reward, miner reward, treasury reward, and SOL submit fee.

## Miner stack

The public miner ships in two forms.

### CLI

The Rust CLI exposes:

- protocol inspection
- miner registration
- CPU, GPU, and hybrid mining
- wallet stats
- benchmarking

### Desktop client

The desktop miner provides:

- local wallet management
- wallet import by mnemonic or private key
- QR-assisted manual funding
- CPU, GPU, and hybrid execution
- live protocol telemetry
- live hashrate statistics

Both interfaces use the same proof rule, RPC read path, and submission path.

## Build

See:

- [docs/protocol.md](docs/protocol.md)
- [docs/architecture.md](docs/architecture.md)
- [docs/miner-client.md](docs/miner-client.md)
- [docs/security-notes.md](docs/security-notes.md)
- [docs/tokenomics.md](docs/tokenomics.md)
- [packaging/README.md](packaging/README.md)

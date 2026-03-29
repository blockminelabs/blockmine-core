# BlockMine

**BlockMine** is a proof-of-work protocol built on top of Solana.

Its core idea is simple, but technically powerful:

- hashing happens **off-chain**
- settlement happens **on-chain**
- Solana is treated as the canonical state and payment layer
- miners use real CPU and GPU hardware to search nonces
- the program only verifies valid solutions, settles rewards, collects treasury fees, and opens the next block

Internally we call this model **SMART MINING**.

That name matters.

BlockMine does **not** try to make Solana perform brute-force mining on-chain. Instead, it uses the chain for exactly what the chain is good at:

- global state
- deterministic settlement
- token accounting
- permissionless block lifecycle management
- transparent treasury routing

And it uses miner hardware for what hardware is good at:

- raw SHA-256 search
- high-throughput nonce iteration
- parallel CPU and GPU execution

This repository is the **public technical core** for BlockMine. It covers the protocol, the miner stack, the reward curve, the fee model, and the main security assumptions as they exist in the codebase today.

---

## Mainnet References

- Program ID: `FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv`
- BLOC mint: `9AJa38FiS8kD2n2Ztubrk6bCSYt55Lz2fBye3Comu1mg`
- Treasury authority: `8DVGdWLzDu8mXV8UuTPtqMpdST6PY2eoEAypK1fARCMb`

Launch runbooks, operational wallets, SSH material, and private deployment checklists stay outside this repository.

---

## 1. Executive Summary

BlockMine is a **real proof-of-work protocol on Solana**.

The protocol exposes a live block challenge, a difficulty target, and a reward schedule. Miners fetch that state, hash off-chain, and only touch Solana when they have something meaningful to do:

- register a miner
- authorize a delegated mining session
- submit a valid block solution
- rotate a stale block after the TTL expires

This architecture creates four important properties:

1. **Search is cheap for the chain**  
   Solana does not waste compute units on brute-force hashing.

2. **Settlement is canonical**  
   When a solution is submitted, the contract verifies it against the current block state and settles the reward deterministically.

3. **The proof is miner-bound**  
   A copied nonce is not enough to steal another miner's reward, because the miner pubkey is part of the proof.

4. **Difficulty is dynamic**  
   The protocol retargets the full 256-bit target so block production converges toward a target rhythm instead of relying on a fixed timer.

BlockMine is therefore not "browser clicks pretending to mine." It is a real off-chain search process with on-chain verification and settlement.

---

## 2. SMART MINING

**SMART MINING** means the protocol cleanly separates the two worlds of mining:

### Off-chain work

- fetch the current challenge and target
- iterate nonces on CPU or GPU
- measure local brute-force throughput
- keep searching until a valid hash is found

### On-chain work

- store the canonical current block
- store the canonical target and block reward
- verify a submitted solution
- pay BLOC rewards
- collect protocol fees
- open the next block
- recover liveness when a block goes stale

That separation is the technical heart of BlockMine.

The protocol does **not** attempt to be a blockchain that hashes inside the contract. It is a settlement engine for proof-of-work performed by external hardware.

That is what makes the design efficient.

---

## 3. End-to-End Block Lifecycle

Every block in BlockMine follows the same lifecycle.

### 3.1 A live block is open

The protocol maintains a single canonical `CurrentBlock` account that stores:

- block number
- challenge
- full 256-bit target
- display difficulty bits
- block reward
- open timestamp
- expiry timestamp
- winner metadata

All miners mine against this same block state.

### 3.2 Miners fetch the snapshot

Miners do **read-only RPC fetches** to learn:

- which block is currently open
- the live challenge
- the current target
- the reward
- the current era

This read path is off-chain and does not notify the contract that a miner is active.

### 3.3 Miners brute-force nonces off-chain

The proof formula is:

```text
hash = SHA256(challenge || miner_pubkey || nonce)
```

A solution is valid if:

```text
hash < target
```

This search happens entirely on miner hardware.

### 3.4 A winning solution is submitted

When a miner finds a valid nonce, the client builds a transaction and calls either:

- `submit_solution`
- `submit_solution_with_session`

The program recomputes the hash on-chain and checks:

- the block is still open
- the block has not expired
- the hash is still valid against the current target
- the reward vault has enough BLOC

### 3.5 The winner is settled

If valid:

- the miner receives the block reward minus the treasury BLOC fee
- the treasury receives its BLOC share
- the treasury also receives the flat SOL submit fee
- a canonical solved-block event is emitted
- the current block is marked closed

### 3.6 The next block is opened

After settlement, the program:

- increments the block number
- computes the next challenge
- recomputes the reward from the reward curve
- retargets difficulty
- opens the next block

### 3.7 If nobody solves the block, stale recovery preserves liveness

If the block remains open beyond the configured TTL, anyone can call:

- `rotate_stale_block`

This opens a fresh block and keeps the machine alive.

That means BlockMine is not dependent on a centralized operator to recover liveness when a block becomes too hard or the network temporarily weakens.

---

## 4. Why Only One Miner Can Be Accepted Per Block

This is one of the most important properties in the entire protocol.

Many miners can independently discover valid candidate solutions for the same block. But **only one solution can be accepted on-chain**.

That happens for three reasons:

### 4.1 There is one canonical mutable live block

`CurrentBlock` is a single mutable account. The winning transaction updates it from:

- `OPEN`
- to `CLOSED`

So once the winning path executes, later transactions are no longer solving the same open block.

### 4.2 Settlement is single-writer

The protocol does not need a second per-block account to enforce winner uniqueness.

That happens because:

- block `N` has one canonical mutable `CurrentBlock`
- the first accepted winner closes it
- later racing transactions hit closed-or-stale state and fail cleanly

### 4.3 Solana account locking serializes the settlement path

Competing winning transactions all need the same writable accounts:

- `config`
- `current_block`
- vault and stats state

So Solana serializes the state transition. Many miners can race to submit, but only one accepted state transition can settle the block.

This is why the correct mental model is:

- many miners may find valid solutions
- many transactions may race
- only one transaction can actually become the accepted block settlement

---

## 5. MEV, Winning Transactions, and What Is Actually Mitigated

This is the section that matters most if someone asks:

**“Can MEV steal the winning block?”**

The short answer is:

- **simple winner theft is already heavily mitigated**
- **ordering and censorship MEV are not mathematically eliminated**

That distinction is crucial.

### 5.1 The direct theft vector BlockMine already mitigates

In many naive mining designs, the attacker can:

1. see a winning nonce in a transaction
2. copy that nonce
3. resubmit it under their own wallet
4. steal the reward

BlockMine blocks that attack because the proof is **bound to the miner identity**.

The program verifies:

```text
hash = SHA256(challenge || miner_pubkey || nonce)
```

That means:

- Miner A finds a winning nonce for Miner A
- Miner B copies the nonce
- the contract recomputes the hash using Miner B's pubkey
- the resulting hash is different
- the copied proof is invalid

So the nonce alone is not the proof.

The proof is:

- challenge
- miner identity
- nonce

Taken together.

This is one of the cleanest V1 anti-theft properties in the protocol.

### 5.2 Why copying the exact signed transaction does not steal the reward

There is a second MEV-style misunderstanding:

“What if someone copies the exact signed winning transaction?”

In that case:

- the transaction is still signed by the original winner
- the miner identity inside the proof is still the original winner
- the reward destination is still constrained to the original winner's token account

So a copied broadcast may create noise or race conditions, but it does **not** rewrite the reward destination to the attacker.

This matters because the protocol is not just protected by social convention. It is protected structurally by the proof and account constraints.

### 5.3 Delegated sessions preserve the same anti-theft property

The session path does **not** weaken the proof binding.

`submit_solution_with_session` still binds:

- the canonical miner
- the authorized delegate
- the active session state

So even when a desktop or delegated miner submits on behalf of a wallet, the reward is still bound to the right miner identity, not to an arbitrary watcher.

### 5.4 What BlockMine does **not** fully eliminate

There is a different class of MEV that no public-chain protocol can honestly claim to erase completely:

- transaction censorship
- orderflow favoritism
- delayed inclusion
- private relay advantage
- validator-side ordering preference

These attacks do **not** let the attacker rewrite your winning nonce into their own reward.

Instead, they attempt to do one of these things:

- suppress your winning transaction
- include it late
- prioritize another transaction path
- cause you to lose the inclusion race

That is a real residual risk on any public chain with shared ordering infrastructure.

So the honest statement is:

- **winner theft by copied nonce is strongly mitigated**
- **inclusion MEV and censorship are reduced, but not reduced to zero**

### 5.5 How BlockMine narrows the MEV surface today

BlockMine already reduces the attack surface in several layers:

1. **Proof binding**  
   The nonce is useless without the right miner identity.

2. **Canonical current block**  
   The solution is checked against the live block state, not against a vague client-side guess.

3. **Unique settlement path**  
   There is one canonical mutable `CurrentBlock` for the live block number, and only one accepted settlement can close it.

4. **Session constraints**  
   Delegated mining is tightly bound to the authorized miner/delegate pair.

5. **Fast local mining**  
   The desktop miner is optimized so more of the time is spent hashing and less of the time is lost in unnecessary RPC churn, which improves real-world submit timing.

### 5.6 What can harden it even further

Before or around mainnet, the following hardening steps can reduce residual MEV risk further:

- include `expected_block_number` and `expected_challenge` in submit arguments, so a transaction is explicitly valid only for the snapshot it was mined against
- use aggressive priority fees for winning submissions
- optionally support private relay or direct validator routing for high-end miners
- keep browser mining positioned as an onboarding path, while the desktop miner remains the serious low-latency path

### 5.7 Why commit-reveal is not the first thing BlockMine needs

Commit-reveal is the classic answer people give to any MEV question.

But BlockMine is not in the naive state where copied nonces steal rewards directly. That part is already substantially solved by miner-bound proofs.

Commit-reveal would add:

- more transactions
- more latency
- more client complexity
- a more complicated UX

So the technically honest position is:

- commit-reveal is a valid future hardening path
- but it is **not** the first fix BlockMine needs, because the primary theft vector is already structurally mitigated

This is an important distinction for technical readers: BlockMine has already addressed the simpler and more dangerous theft form. What remains is mostly **ordering risk**, not **proof ownership risk**.

---

## 6. Difficulty, Targeting, and Liveness

BlockMine does not use a timer to “pretend” that a block happened.

It uses a live difficulty system so blocks are **earned** by work, while the protocol still converges toward a target rhythm.

### 6.1 Target rhythm

The current protocol target is:

- **about 15 seconds average per block**

This is a target average, not a promise that every single block lands at exactly 15 seconds. Proof-of-work is probabilistic, so the goal is convergence in the mean, not deterministic cadence.

### 6.2 Full 256-bit retarget

Difficulty is not adjusted only as coarse integer “bits”.

The protocol stores and updates:

- a full **256-bit target**

This makes retargeting smoother and more expressive than a whole-bit jump model.

### 6.3 Per-block adjustment

After each solved block, the program compares:

- observed solve time
- expected solve time

and computes the next target.

The retarget logic includes:

- smoothing
- outlier clamping
- emergency fast-ramp for very fast blocks
- min and max difficulty bounds

This makes the system far more reactive to sudden hashrate shifts than a slow epoch-based adjustment scheme.

### 6.4 Stale recovery

If a block is not solved within the configured TTL:

- stale rotation can be called permissionlessly

That preserves liveness even if:

- hashrate falls suddenly
- difficulty overshoots
- miners go offline

This matters because immutable systems need permissionless recovery paths, not operator intervention.

### 6.5 Operational resilience under real network variance

The difficulty system is designed to stay stable under real-world conditions:

- probabilistic solve timing
- sudden hashrate shocks
- miner entry and exit
- temporary network latency

The key point is that BlockMine does not rely on synthetic timing. It relies on:

- measured block outcomes
- bounded target adjustment
- permissionless stale recovery

That makes the mining loop resilient while preserving the core proof-of-work model.

---

## 7. Treasury, Sustainability, and Why the `0.01 SOL` Fee Exists

BlockMine has two treasury flows:

1. **1% of each BLOC block reward**
2. **a flat `0.01 SOL` fee on each accepted block submission**

Both routes are deliberate.

### 7.1 Why the BLOC treasury fee exists

The BLOC treasury share creates protocol-owned alignment inside the native asset itself.

That treasury inventory can support:

- buybacks
- liquidity support
- ecosystem incentives
- security work
- long-term growth initiatives

### 7.2 Why the `0.01 SOL` fee exists

The SOL fee is not a fee on every hash attempt.

It is only charged when a block is actually accepted on-chain.

That matters, because it means:

- off-chain bruteforce remains effectively free from the protocol's perspective
- only successful settlement pays the fixed SOL fee

The SOL treasury stream gives the protocol a non-inflationary operating budget that can support:

- infrastructure
- audits
- listings
- security work
- buybacks
- marketing
- operational continuity

In other words, the fee is not just “friction.” It is what makes the protocol capable of funding its own growth and defense without constantly depending on external capital.

### 7.3 Treasury transparency

The intention is not to hide treasury activity behind vague promises.

**Every treasury expense is meant to be disclosed on the Transparency page**, with on-chain references wherever possible.

That includes treasury usage for:

- buybacks
- marketing
- infrastructure
- security and audits
- liquidity operations
- ecosystem development

So the treasury model is designed to be both:

- economically useful
- publicly inspectable

---

## 8. Token Supply and Mining Curve

BlockMine launches with a fixed total minted supply of:

- **21,000,000 BLOC**

That supply is split into:

- **20,000,000 BLOC** reserved for protocol mining emissions
- **550,000 BLOC** reserved for launch LP
- **450,000 BLOC** reserved as treasury inventory

The smart contract mining schedule covers only the **20,000,000 BLOC mining allocation**.

The LP allocation and treasury reserve are outside the mining schedule.

Era progression advances on successfully settled blocks, not on raw block openings. If a block expires and rotates stale, the schedule is preserved instead of silently burning emissions.

### 8.1 Era schedule

| Era | Name | Block range | Reward per block (BLOC) | Era emissions (BLOC) | Cumulative emissions (BLOC) |
| --- | ---- | ----------- | ----------------------- | -------------------- | --------------------------- |
| 0 | Genesis | `0 - 9,999` | `21.0` | `210,000` | `210,000` |
| 1 | Aurum | `10,000 - 99,999` | `12.0` | `1,080,000` | `1,290,000` |
| 2 | Phoenix | `100,000 - 299,999` | `7.0` | `1,400,000` | `2,690,000` |
| 3 | Horizon | `300,000 - 599,999` | `5.0` | `1,500,000` | `4,190,000` |
| 4 | Quasar | `600,000 - 999,999` | `3.8` | `1,520,000` | `5,710,000` |
| 5 | Pulsar | `1,000,000 - 1,499,999` | `3.0` | `1,500,000` | `7,210,000` |
| 6 | Voidfall | `1,500,000 - 2,099,999` | `2.3` | `1,380,000` | `8,590,000` |
| 7 | Eclipse | `2,100,000 - 2,999,999` | `1.8` | `1,620,000` | `10,210,000` |
| 8 | Mythos | `3,000,000 - 4,199,999` | `1.4` | `1,680,000` | `11,890,000` |
| 9 | Paragon | `4,200,000 - 5,799,999` | `1.1` | `1,760,000` | `13,650,000` |
| 10 | Hyperion | `5,800,000 - 7,499,999` | `0.9` | `1,530,000` | `15,180,000` |
| 11 | Singularity | `7,500,000 - 9,499,999` | `0.7` | `1,400,000` | `16,580,000` |
| 12 | Eternal I | `9,500,000 - 11,999,999` | `0.5` | `1,250,000` | `17,830,000` |
| 13 | Eternal II | `12,000,000 - 15,999,999` | `0.3` | `1,200,000` | `19,030,000` |
| 14 | Scarcity | starts at `16,000,000` | nominally `0.15` | remaining `970,000` | `20,000,000` |

### 8.2 Exact Scarcity tail

The Scarcity era is intentionally capped so the protocol stops exactly at `20,000,000 BLOC` mined.

The exact tail is:

- `6,466,666` Scarcity blocks at `0.15 BLOC`
- `1` final Scarcity block at `0.10 BLOC`
- then `0` reward forever after that

So the last mining blocks are:

- Scarcity full-reward blocks: `16,000,000 - 22,466,665`
- Scarcity final partial block: `22,466,666`
- reward after block `22,466,666`: `0`

This is not an infinite-tail model.

It is a capped mining schedule with:

- `20M` mined through SMART MINING
- `550k` reserved for launch LP and `450k` held as treasury reserve outside the mining schedule

---

## 9. The Miner Stack

BlockMine is not just a contract. It is a protocol plus a miner stack.

### 9.1 Browser miner

The browser miner is the lightweight entry path.

It:

- mines directly in the tab
- reads the live challenge over RPC
- hashes off-chain in-browser
- opens the wallet only when a real on-chain submit or stale rotation requires approval

This makes the browser miner ideal for:

- onboarding
- demos
- try-before-you-download usage

### 9.2 Desktop miner

The desktop miner is the serious execution path.

It supports:

- CPU mining
- GPU mining
- multi-GPU selection
- GPU autotune
- real off-chain throughput measurement
- persistent local wallet balance
- SOL funding for submit treasury fees
- live BLOC balance
- SOL and BLOC withdrawals

The desktop miner is optimized so hardware spends more time hashing and less time waiting on unnecessary RPC round trips.

### 9.3 Why both paths matter

The browser miner lowers friction.

The desktop miner unlocks real performance.

That combination matters commercially:

- anyone can try the protocol immediately
- serious miners can scale into dedicated desktop mining

---

## 10. Core On-Chain State and Instruction Set

The protocol is driven by a small, auditable set of accounts.

### `ProtocolConfig`

Global control plane for:

- mint and vault addresses
- treasury routing
- reward metadata
- difficulty state
- timing parameters
- treasury fee configuration
- canonical current target

### `CurrentBlock`

The live block all miners are hashing against:

- block number
- challenge
- reward
- target
- status
- timestamps
- winner metadata

### `MinerStats`

Per-miner protocol state:

- submissions
- valid blocks found
- lifetime BLOC earned
- nickname metadata

### `MiningSession`

Delegated mining authorization:

- canonical miner
- delegate
- expiry
- submission cap
- active flag

### Solved-block event trail

Accepted blocks are published through canonical `BlockSolved` events instead of forcing miners to fund a rent-bearing history account on every win.

That preserves:

- block number
- winner
- reward
- nonce
- hash
- challenge snapshot
- difficulty snapshot

while removing the old per-win rent overhead from the settlement path.

### Instruction set

The current program exposes the following instruction families:

- `initialize_protocol`
- `register_miner`
- `update_nickname`
- `submit_solution`
- `authorize_mining_session`
- `submit_solution_with_session`
- `rotate_stale_block`
- admin/runtime configuration instructions used during the tuning phase

In practical terms, that means the contract handles four jobs:

1. protocol initialization
2. miner identity and session authorization
3. canonical block settlement
4. liveness preservation through stale rotation

The most important instruction is the submit path.

That path:

- verifies the live block snapshot
- recomputes the proof on-chain
- enforces the target check
- transfers miner and treasury rewards
- updates global counters
- retargets difficulty
- opens the next block

So the contract is not a passive reward faucet. It is the deterministic state machine that controls the entire mining cycle.

---

## 11. Mainnet Posture

The deployed mainnet program and the public core repo are aligned on the following principles:

- fixed supply
- pre-funded reward vault
- fixed `0.01 SOL` accepted-block fee
- fixed `1%` treasury cut in `BLOC`
- deterministic per-block settlement
- transparent treasury routing

The intended end state remains:

- immutable deployment
- admin controls removed
- upgrade authority removed
- fixed treasury routing
- fixed mining curve
- fixed proof rules

That matters because SMART MINING becomes much more credible when the market knows the engine cannot be silently reprogrammed later.

---

## 12. Security Architecture and Settlement Guarantees

BlockMine is engineered so that the most important safety properties come from protocol structure, not from UI promises.

### 12.1 Winner uniqueness

Only one block settlement can be accepted for a given block number because:

- there is one canonical mutable `CurrentBlock`
- Solana account locking serializes the winning settlement path

### 12.2 Proof ownership binding

The proof includes the miner identity directly in the hash input. That is why copied nonces do not let another wallet steal a reward.

### 12.3 Canonical vault routing

The submit path does not trust arbitrary accounts passed by clients. Mint, reward vault, and treasury accounts are checked against protocol configuration and mint constraints.

That gives BlockMine strong protection against:

- fake mint routing
- fake treasury vault substitution
- fake reward sink substitution

### 12.4 Difficulty integrity

Difficulty is never set from miner-reported hashrate.

The protocol infers work conditions from observed block timing and updates the canonical target on-chain. This avoids one of the most common design mistakes in hybrid mining systems: trusting client-side throughput claims.

### 12.5 Liveness protection

If a block remains open too long, stale rotation keeps the block machine moving permissionlessly. That means liveness does not depend on a centralized operator staying awake.

### 12.6 Emission integrity

The mining curve is deterministic, named by era, and computed from the canonical block number. That makes the reward path auditable and legible instead of opaque.

### 12.7 Why this matters

BlockMine is not trying to become "another chain."

It is doing something more specific:

- using Solana as deterministic settlement
- using off-chain hardware for proof-of-work
- binding proof ownership to the miner
- keeping liveness with permissionless stale recovery
- funding protocol growth with transparent treasury flows
- exposing both browser and desktop mining paths

The result is a system that is:

- technically elegant
- economically legible
- operationally scalable
- compatible with consumer onboarding and serious miner hardware

That combination is what makes BlockMine more than a toy miner UI or a fake browser gimmick.

It is a real settlement-aware mining protocol.

---

## 13. Security Notes

The public security summary for the current codebase lives in:

- [`docs/security-notes.md`](docs/security-notes.md)

That file is the right place for public-facing security assumptions, launch posture, and remaining hardening notes.
| Solana chain-level outage / congestion | Network is slow or unavailable | Clients retry, stale rotation exists, but chain dependency remains | External platform risk |
| Cluster rollback / fork effects | Confirmed state is later rolled back | Normal Solana finality assumptions apply | External platform risk |

---

### Already mitigated vs. still hardening

#### Mitigated well enough for V1

| Attack / vulnerability | Current mitigation in V1 | Why it is considered mitigated for V1 |
|---|---|---|
| Simple nonce theft | The proof is bound to the miner wallet inside the hash input | Copying another miner's nonce does not let an attacker steal the reward under a different wallet |
| Replay across blocks | The challenge rotates every time a new block opens | A solve for block `N` does not remain valid for block `N+1` |
| Duplicate payout race on the same block | `current_block` is a single mutable account and Solana account locking serializes updates | Only one solve can close the block; later competing transactions lose the race |
| Reward vault theft by fake signer path | Reward transfers require the PDA signer path through `vault_authority` | External wallets cannot arbitrarily drain the reward vault |
| Fake treasury token account substitution | Treasury BLOC account is checked against config and mint constraints | Users cannot redirect treasury BLOC by supplying an arbitrary token account |
| Fake mint substitution | The mint account must equal `config.bloc_mint` | Rewards cannot be redirected into another token mint |
| Fake miner reward sink | The miner token account must be owned by the miner and match the BLOC mint | Reward destination validation is strict enough for V1 |
| Miner-reported hashrate manipulation | The contract never reads self-reported miner hashrate | Difficulty is inferred from observed block timing, not from untrusted client claims |
| Arithmetic overflow in counters / fees | Checked math and explicit overflow errors | Counters and fee math fail closed instead of wrapping silently |

#### Partially mitigated, but not fully solved

| Attack / vulnerability | Current mitigation in V1 | Why it is only partially mitigated |
|---|---|---|
| Difficulty shock attack | Per-block retarget, full-target scaling, smoothing, clamp, emergency fast-ramp, stale fallback | Difficulty still only truly changes on solved blocks or stale rotation; a large enough farm shock can still create awkward transitions |
| Permanent stall from an over-hard block | Permissionless `rotate_stale_block` restores liveness after TTL expiry | Liveness is restored, but the stale path currently hard-resets difficulty to the minimum |
| Front-running / MEV / ordering abuse | Wallet binding blocks simple nonce theft | Ordering MEV and censorship still exist; V1 does not yet eliminate public-orderflow timing risk |
| Session abuse after approval | Sessions store `active`, `expires_at`, `max_submissions`, and delegate binding | If the delegate key is stolen, it remains dangerous until expiry or exhaustion |
| Historical traceability dependency | Canonical solved-block events | Rich solved-block history is now event/indexer-driven rather than stored in a dedicated account per accepted block |

---

### Implementation map

For technical readers who want the exact code anchors, these are the substantive Rust files that define protocol behavior today. The various `mod.rs` files are only module wiring and are omitted on purpose.

#### On-chain program

- Program entrypoints: [`onchain/programs/blockmine/src/lib.rs`](onchain/programs/blockmine/src/lib.rs)
- Constants and seeds: [`onchain/programs/blockmine/src/constants.rs`](onchain/programs/blockmine/src/constants.rs)
- Error surface: [`onchain/programs/blockmine/src/errors.rs`](onchain/programs/blockmine/src/errors.rs)
- Events: [`onchain/programs/blockmine/src/events.rs`](onchain/programs/blockmine/src/events.rs)
- Protocol init invariants: [`onchain/programs/blockmine/src/instructions/initialize_protocol.rs`](onchain/programs/blockmine/src/instructions/initialize_protocol.rs)
- Miner registration: [`onchain/programs/blockmine/src/instructions/register_miner.rs`](onchain/programs/blockmine/src/instructions/register_miner.rs)
- Nickname updates: [`onchain/programs/blockmine/src/instructions/update_nickname.rs`](onchain/programs/blockmine/src/instructions/update_nickname.rs)
- Winning submit path: [`onchain/programs/blockmine/src/instructions/submit_solution.rs`](onchain/programs/blockmine/src/instructions/submit_solution.rs)
- Session authorization and delegated submit path: [`onchain/programs/blockmine/src/instructions/session_mining.rs`](onchain/programs/blockmine/src/instructions/session_mining.rs)
- Stale rotation and liveness recovery: [`onchain/programs/blockmine/src/instructions/rotate_stale_block.rs`](onchain/programs/blockmine/src/instructions/rotate_stale_block.rs)
- Devnet-only admin/tuning surface: [`onchain/programs/blockmine/src/instructions/admin.rs`](onchain/programs/blockmine/src/instructions/admin.rs)
- Difficulty logic: [`onchain/programs/blockmine/src/math/difficulty.rs`](onchain/programs/blockmine/src/math/difficulty.rs)
- Reward curve and scarcity tail: [`onchain/programs/blockmine/src/math/rewards.rs`](onchain/programs/blockmine/src/math/rewards.rs)
- Global config state: [`onchain/programs/blockmine/src/state/protocol_config.rs`](onchain/programs/blockmine/src/state/protocol_config.rs)
- Live block state: [`onchain/programs/blockmine/src/state/current_block.rs`](onchain/programs/blockmine/src/state/current_block.rs)
- Miner lifetime stats: [`onchain/programs/blockmine/src/state/miner_stats.rs`](onchain/programs/blockmine/src/state/miner_stats.rs)
- Delegated session state: [`onchain/programs/blockmine/src/state/mining_session.rs`](onchain/programs/blockmine/src/state/mining_session.rs)

#### Desktop miner

- Desktop miner UI shell: [`miner-client/src/bin/blockmine-studio.rs`](miner-client/src/bin/blockmine-studio.rs)
- Mining runtime orchestration: [`miner-client/src/mining_service.rs`](miner-client/src/mining_service.rs)
- Mining loop: [`miner-client/src/miner_loop.rs`](miner-client/src/miner_loop.rs)
- RPC integration: [`miner-client/src/rpc.rs`](miner-client/src/rpc.rs)
- Submit pipeline: [`miner-client/src/submitter.rs`](miner-client/src/submitter.rs)
- Desktop wallet lifecycle: [`miner-client/src/wallet.rs`](miner-client/src/wallet.rs)
- Session wallet handling: [`miner-client/src/session_wallet.rs`](miner-client/src/session_wallet.rs)
- CPU backend: [`miner-client/src/engine/cpu.rs`](miner-client/src/engine/cpu.rs)
- GPU backend: [`miner-client/src/engine/gpu.rs`](miner-client/src/engine/gpu.rs)

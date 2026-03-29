# Mining Curve

This file describes the exact mining curve intended for the protocol and the operational rules around it.

## Supply split

- Total BLOC minted at launch: `21,000,000`
- Reserved for protocol mining emissions: `20,000,000`
- Reserved for initial liquidity: `550,000`
- Reserved for treasury reserve: `450,000`

Important:

- The smart contract emission schedule covers only the `20,000,000` allocated to mining.
- The LP allocation and treasury reserve are outside the mining schedule.

## Era schedule

| Era | Name        | Block range               | Reward per block (BLOC) | Era emissions (BLOC) | Cumulative emissions (BLOC) |
| --- | ----------- | ------------------------- | ----------------------- | -------------------- | --------------------------- |
| 0   | Genesis     | `0 - 9,999`               | `21.0`                  | `210,000`            | `210,000`                   |
| 1   | Aurum       | `10,000 - 99,999`         | `12.0`                  | `1,080,000`          | `1,290,000`                 |
| 2   | Phoenix     | `100,000 - 299,999`       | `7.0`                   | `1,400,000`          | `2,690,000`                 |
| 3   | Horizon     | `300,000 - 599,999`       | `5.0`                   | `1,500,000`          | `4,190,000`                 |
| 4   | Quasar      | `600,000 - 999,999`       | `3.8`                   | `1,520,000`          | `5,710,000`                 |
| 5   | Pulsar      | `1,000,000 - 1,499,999`   | `3.0`                   | `1,500,000`          | `7,210,000`                 |
| 6   | Voidfall    | `1,500,000 - 2,099,999`   | `2.3`                   | `1,380,000`          | `8,590,000`                 |
| 7   | Eclipse     | `2,100,000 - 2,999,999`   | `1.8`                   | `1,620,000`          | `10,210,000`                |
| 8   | Mythos      | `3,000,000 - 4,199,999`   | `1.4`                   | `1,680,000`          | `11,890,000`                |
| 9   | Paragon     | `4,200,000 - 5,799,999`   | `1.1`                   | `1,760,000`          | `13,650,000`                |
| 10  | Hyperion    | `5,800,000 - 7,499,999`   | `0.9`                   | `1,530,000`          | `15,180,000`                |
| 11  | Singularity | `7,500,000 - 9,499,999`   | `0.7`                   | `1,400,000`          | `16,580,000`                |
| 12  | Eternal I   | `9,500,000 - 11,999,999`  | `0.5`                   | `1,250,000`          | `17,830,000`                |
| 13  | Eternal II  | `12,000,000 - 15,999,999` | `0.3`                   | `1,200,000`          | `19,030,000`                |
| 14  | Scarcity    | starts at `16,000,000`    | nominally `0.15`        | remaining `970,000`  | `20,000,000`                |

## Exact Scarcity tail

The last era needs special handling.

If we keep paying `0.15 BLOC` forever after block `16,000,000`, the protocol would emit more than `20,000,000`.

So the exact capped tail is:

- `6,466,666` Scarcity blocks at `0.15 BLOC`
- `1` final Scarcity block at `0.10 BLOC`
- then `0` reward forever after that

That means:

- Scarcity full-reward blocks: `16,000,000 - 22,466,665`
- Scarcity final partial block: `22,466,666`
- reward after block `22,466,666`: `0`

This is what makes the mining schedule stop exactly at:

- `20,000,000 BLOC` emitted by the protocol

## Emission indexing

- Reward selection advances on `total_blocks_mined`, not on raw open-block numbers.
- Stale rotations preserve the scheduled reward path instead of silently skipping emissions.
- Era transitions therefore happen on accepted settlements, not on expired timers.

## Devnet reset behavior

For devnet-only admin resets:

- the protocol reopens at a fresh block number instead of rewinding to block `0`
- this avoids replaying historical block numbers during rehearsal resets

For mainnet posture:

- the protocol should launch once from Genesis
- the admin surface should then be removed

## Treasury behavior

The treasury receives:

- `1%` of each BLOC block reward
- the flat `0.01 SOL` submit fee on accepted block submissions

## Target mining rhythm

The intended live protocol rhythm remains:

- target block time: `15 seconds average`
- difficulty updates on solved blocks
- stale rotation available after `60 seconds`

## What this means in practice

This is not a classic infinite-tail emission model.

It is a capped mining emission model:

- `20M` are emitted through mining
- `550k` is reserved for launch LP
- `450k` is held as treasury reserve
- after the capped Scarcity tail is exhausted, mining rewards are `0`

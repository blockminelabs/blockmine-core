<p align="center">
  <a href="README.md#overview"><img alt="Home" src="https://img.shields.io/badge/Home-151515?style=for-the-badge"></a>
  <a href="docs/protocol.md"><img alt="Protocol" src="https://img.shields.io/badge/Protocol-f7931a?style=for-the-badge"></a>
  <a href="docs/architecture.md"><img alt="Architecture" src="https://img.shields.io/badge/Architecture-1f2937?style=for-the-badge"></a>
  <a href="docs/miner-client.md"><img alt="Miner" src="https://img.shields.io/badge/Miner-374151?style=for-the-badge"></a>
  <a href="docs/security-notes.md"><img alt="Security" src="https://img.shields.io/badge/Security-7c2d12?style=for-the-badge"></a>
  <a href="docs/tokenomics.md"><img alt="Tokenomics" src="https://img.shields.io/badge/Tokenomics-92400e?style=for-the-badge"></a>
  <a href="MINING_CURVE.md"><img alt="Mining Curve" src="https://img.shields.io/badge/Mining_Curve-b45309?style=for-the-badge"></a>
  <a href="LIVE_CONFIG_NOTES.md"><img alt="Live Config" src="https://img.shields.io/badge/Live_Config-4b5563?style=for-the-badge"></a>
</p>

# Mining Curve

This file records the exact Blockmine emission schedule implemented by `onchain/programs/blockmine/src/math/rewards.rs`.

## Supply split

- Total minted supply: `21,000,000 BLOC`
- Reward vault emissions: `20,000,000 BLOC`
- Initial liquidity inventory: `550,000 BLOC`
- Treasury reserve inventory: `450,000 BLOC`

The era schedule applies only to the `20,000,000 BLOC` reward-vault inventory.

## Era schedule

| Era | Name | Settled block range | Gross reward per block | Era emissions | Cumulative emissions |
| --- | --- | --- | ---: | ---: | ---: |
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

## Exact Scarcity tail

The capped tail is:

- `6,466,666` blocks at `0.15 BLOC`
- `1` final block at `0.10 BLOC`
- then `0`

Therefore:

- last full `0.15 BLOC` block: `22,466,665`
- last non-zero reward block: `22,466,666`

The reward function returns zero after that point.

## Gross reward and net reward

The era table defines the gross block reward.

For every accepted block:

- treasury BLOC share = `gross_reward * 1%`
- miner BLOC share = `gross_reward * 99%`

Example at Genesis:

- gross reward = `21.0 BLOC`
- treasury BLOC share = `0.21 BLOC`
- miner BLOC share = `20.79 BLOC`

In addition, the accepted winner pays `0.01 SOL` to the treasury wallet.

## What advances the schedule

The schedule is keyed to `total_blocks_mined`.

That means:

- a solved block advances the reward index
- a stale rotation does not advance the reward index
- `current_block_number` may be greater than `total_blocks_mined`

This is deliberate. A stale block consumes time, not emissions.

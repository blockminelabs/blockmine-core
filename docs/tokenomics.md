# Tokenomics

## Supply

- Name: `Blockmine`
- Symbol: `BLOC`
- Decimals: `9`
- Fixed supply: `21,000,000 BLOC`

Raw integer supply:

```text
21,000,000 * 10^9 = 21,000,000,000,000,000
```

## Initial allocation

| Allocation | Tokens | Share |
| --- | ---: | ---: |
| Mining emissions | `20,000,000` | `95.238095%` |
| Initial liquidity inventory | `550,000` | `2.619048%` |
| Treasury reserve inventory | `450,000` | `2.142857%` |

Only the `20,000,000 BLOC` mining allocation participates in the protocol reward schedule.

## Reward accounting

Every block has a gross reward determined by the era table.

For an accepted block:

```text
treasury_reward = gross_reward * 100 / 10000
miner_reward    = gross_reward - treasury_reward
```

With `treasury_fee_bps = 100`, this is a `1%` BLOC treasury share and a `99%` miner share.

In addition to the BLOC split, each accepted block transfers:

```text
submit_fee_lamports = 10,000,000 = 0.01 SOL
```

to the treasury wallet.

## Emission indexing

The reward schedule advances on `total_blocks_mined`.

That is the count of successfully settled blocks.

It does not advance on:

- raw block openings
- stale rotations
- elapsed wall clock time

This distinction preserves the full scheduled mining curve even if some logical blocks expire unsolved.

## Scarcity cap

The final era is a capped tail, not an infinite stream.

Scarcity emits:

- `6,466,666` blocks at `0.15 BLOC`
- `1` final block at `0.10 BLOC`
- then `0` thereafter

This is what makes the mining allocation stop exactly at `20,000,000 BLOC`.

## Supply after initialization

The intended public posture is:

1. mint the full fixed supply
2. allocate the three inventories
3. fund the reward vault with the mining allocation
4. revoke mint authority
5. revoke freeze authority if one was used

Once mint authority is removed, the program does not need future minting to settle rewards. The reward vault is already the inventory source.

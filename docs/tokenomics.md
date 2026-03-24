# Tokenomics

## Supply

- token name: `Blockmine`
- ticker: `BLOC`
- max supply: `21,000,000 BLOC`
- decimals: `9`

Raw max supply used by the program:

- `21,000,000 * 10^9 = 21,000,000,000,000,000`

## Launch allocation

| Category | Tokens | % | Description |
| --- | ---: | ---: | --- |
| Mining rewards | `20,000,000` | `95.24%` | Distributed through Smart Mining emissions |
| Initial liquidity | `550,000` | `2.62%` | Initial LP reserved for launch market depth |
| Treasury reserve | `450,000` | `2.14%` | Protocol-owned reserve inventory |

## Minting model

The intended setup is:

1. create the SPL mint
2. complete supply allocation
3. fund the reward vault with the mining allocation
4. hold the LP allocation separately for launch
5. hold the treasury reserve separately under treasury control
6. revoke mint authority
7. revoke freeze authority if one was used during setup

After authority revocation, no new BLOC can be minted or frozen by policy.

## Reward release

- rewards come from the reward vault, not from future minting
- the protocol mining schedule covers only the `20,000,000 BLOC` mining allocation
- era progression is keyed to settled blocks mined, so stale rotation does not burn scheduled emissions
- the protocol applies a fixed `1%` BLOC treasury fee on accepted rewards
- accepted block submissions also route a flat `0.01 SOL` treasury fee
- miners receive the remaining net block reward

## Treasury mandate

The treasury reserve is balance-sheet inventory, not a fake spreadsheet of pre-claimed spend categories.

The intended capital policy is:

- buyback-first
- selective deployment into listings, infrastructure, liquidity, and growth only when justified
- public disclosure of balances and treasury movements on the Transparency page

The important design point is not a marketing promise about spending buckets.

The important design point is:

- fixed disclosed reserve inventory
- live treasury fee inflows
- public transparency around balances and treasury actions

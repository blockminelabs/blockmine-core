# Tokenomics

## Supply

- token name: `BlockMine`
- ticker: `BLOC`
- max supply: `21,000,000 BLOC`
- decimals: `9`

Raw max supply used by the program:

- `21,000,000 * 10^9 = 21,000,000,000,000,000`

## Minting model

The recommended V1 setup is:

1. create the SPL mint
2. initialize the BlockMine protocol
3. let the program create the reward vault ATA
4. mint the full supply into the reward vault
5. revoke mint authority

After mint-authority revocation, no new BLOC can be minted.

## Reward release

- initial reward is configurable, default scaffold uses `10 BLOC`
- rewards come from the reward vault, not from new minting after launch
- V1 applies a fixed `1%` treasury fee to each successful block reward
- miners receive the remaining `99%`
- halving happens every configured interval
- if the vault is empty, no more rewards can be paid

## Why all supply starts in the vault

This keeps the monetary model simple:

- fixed supply from day one
- emission is controlled by the program logic, not future mint calls
- the reward vault becomes the visible emission source

## Treasury

V1 leaves room for a future treasury address in config, but keeps it inactive.

That is deliberate. Emission and miner fairness come first. Treasury mechanics can be added later if governance requires them.

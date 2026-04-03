<p align="center">
  <a href="README.md#overview"><img alt="Home" src="https://img.shields.io/badge/Home-151515?style=for-the-badge"></a>
  <a href="docs/protocol.md"><img alt="Protocol" src="https://img.shields.io/badge/Protocol-f7931a?style=for-the-badge"></a>
  <a href="docs/architecture.md"><img alt="Architecture" src="https://img.shields.io/badge/Architecture-1f2937?style=for-the-badge"></a>
  <a href="docs/miner-client.md"><img alt="Miner" src="https://img.shields.io/badge/Miner-374151?style=for-the-badge"></a>
  <a href="docs/vast-ai.md"><img alt="Vast.ai" src="https://img.shields.io/badge/Vast.ai-0f766e?style=for-the-badge"></a>
  <a href="docs/security-notes.md"><img alt="Security" src="https://img.shields.io/badge/Security-7c2d12?style=for-the-badge"></a>
  <a href="docs/tokenomics.md"><img alt="Tokenomics" src="https://img.shields.io/badge/Tokenomics-92400e?style=for-the-badge"></a>
  <a href="MINING_CURVE.md"><img alt="Mining Curve" src="https://img.shields.io/badge/Mining_Curve-b45309?style=for-the-badge"></a>
  <a href="LIVE_CONFIG_NOTES.md"><img alt="Live Config" src="https://img.shields.io/badge/Live_Config-4b5563?style=for-the-badge"></a>
</p>

# Live Configuration Notes

## Public mainnet references

- Program ID: `FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv`
- Vault authority PDA: `6yfyKscrWeqsB4bsjtRL9vw3hfFXsYcrhit4WyG91GUF`
- Reward vault: `ApA17DcAYh7pVCcbUemQaDaqW1YxXaU62b73cUBHmdcS`
- Mint: `9AJa38FiS8kD2n2Ztubrk6bCSYt55Lz2fBye3Comu1mg`
- Treasury wallet: `8DVGdWLzDu8mXV8UuTPtqMpdST6PY2eoEAypK1fARCMb`
- Treasury ATA: `Db4mNDjDJocoGC3Vi7RNaiApxjhyRDzmmgbfbdDWXUJi`

## Public miner defaults

- Miner state relay: `https://blockmine.dev/api/miner/state`
- Default raw RPC: `https://solana-rpc.publicnode.com`
- Default program ID: `FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv`

## Monetary constants

- Total supply: `21,000,000 BLOC`
- Mining inventory: `20,000,000 BLOC`
- Treasury reserve: `450,000 BLOC`
- Initial liquidity inventory: `550,000 BLOC`
- Accepted-block SOL fee: `0.01 SOL`
- Accepted-block BLOC treasury fee: `1%`

## Public runtime shape

- one canonical live block
- one pre-funded reward vault
- one treasury wallet for accepted-block SOL fees
- one treasury ATA for BLOC treasury rewards
- emissions indexed by settled blocks mined

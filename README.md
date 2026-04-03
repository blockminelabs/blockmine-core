<p align="center">
  <img src="miner-client/img/logocircle.png" alt="Blockmine mark" width="56" valign="middle">
  <img src="miner-client/img/blockmine-logo-final.png" alt="Blockmine" width="360" valign="middle">
</p>

<p align="center">
  <strong>Proof-of-work settlement on Solana.</strong><br>
  CPU and GPU hardware search nonces off-chain. Solana verifies, settles, and advances the chain state.
</p>

<p align="center">
  <a href="https://blockmine.dev"><img alt="Website" src="https://img.shields.io/badge/Website-blockmine.dev-f7931a?style=for-the-badge"></a>
  <a href="https://t.me/blockmine"><img alt="Telegram" src="https://img.shields.io/badge/Telegram-@blockmine-229ED9?style=for-the-badge"></a>
  <a href="https://x.com/blockminelabs"><img alt="X" src="https://img.shields.io/badge/X-@blockminelabs-111111?style=for-the-badge"></a>
</p>

<p align="center">
  <a href="#overview"><img alt="Overview" src="https://img.shields.io/badge/Overview-151515?style=for-the-badge"></a>
  <a href="docs/protocol.md"><img alt="Protocol" src="https://img.shields.io/badge/Protocol-f7931a?style=for-the-badge"></a>
  <a href="docs/architecture.md"><img alt="Architecture" src="https://img.shields.io/badge/Architecture-1f2937?style=for-the-badge"></a>
  <a href="docs/miner-client.md"><img alt="Miner" src="https://img.shields.io/badge/Miner-374151?style=for-the-badge"></a>
  <a href="docs/security-notes.md"><img alt="Security" src="https://img.shields.io/badge/Security-7c2d12?style=for-the-badge"></a>
  <a href="docs/tokenomics.md"><img alt="Tokenomics" src="https://img.shields.io/badge/Tokenomics-92400e?style=for-the-badge"></a>
  <a href="MINING_CURVE.md"><img alt="Mining Curve" src="https://img.shields.io/badge/Mining_Curve-b45309?style=for-the-badge"></a>
</p>

<p align="center">
  <img alt="Rust" src="https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust">
  <img alt="Solana" src="https://img.shields.io/badge/Solana-0f172a?style=flat-square&logo=solana">
  <img alt="Anchor" src="https://img.shields.io/badge/Anchor-7c3aed?style=flat-square">
  <img alt="SPL Token" src="https://img.shields.io/badge/SPL_Token-111827?style=flat-square">
  <img alt="SHA-256" src="https://img.shields.io/badge/SHA--256-f59e0b?style=flat-square">
  <img alt="OpenCL" src="https://img.shields.io/badge/OpenCL-2563eb?style=flat-square">
  <img alt="Windows" src="https://img.shields.io/badge/Windows-1d4ed8?style=flat-square&logo=windows">
  <img alt="macOS" src="https://img.shields.io/badge/macOS-374151?style=flat-square&logo=apple">
</p>

## Overview

Blockmine is a proof-of-work settlement protocol on Solana.

The chain does not brute-force hashes. The chain publishes one canonical challenge, one canonical target, one canonical reward, and one canonical settlement path. CPU and GPU hardware search nonce space off-chain. The program verifies valid proofs, routes fees, pays BLOC rewards from a pre-funded vault, and opens the next logical block.

The accepted proof is:

```text
H = SHA256(challenge || miner_pubkey || nonce_le_u64)
```

with acceptance rule:

```text
H < target
```

The target is stored on-chain as a full 256-bit threshold. Reward issuance advances on successfully settled blocks, not on stale timers.

## Mainnet References

| Item | Value |
| --- | --- |
| Program ID | `FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv` |
| Reward vault | `ApA17DcAYh7pVCcbUemQaDaqW1YxXaU62b73cUBHmdcS` |
| Mint | `9AJa38FiS8kD2n2Ztubrk6bCSYt55Lz2fBye3Comu1mg` |
| Treasury wallet | `8DVGdWLzDu8mXV8UuTPtqMpdST6PY2eoEAypK1fARCMb` |
| Default RPC | `https://api.mainnet-beta.solana.com` |
| Fixed accepted-block fee | `0.01 SOL` |
| Treasury BLOC share | `1%` |

## Technical Index

| Document | Scope |
| --- | --- |
| [docs/protocol.md](docs/protocol.md) | State machine, proof rule, submit path, stale rotation, challenge derivation |
| [docs/architecture.md](docs/architecture.md) | Off-chain search vs on-chain settlement, account model, event trail |
| [docs/miner-client.md](docs/miner-client.md) | CLI miner, desktop miner, wallet manager, CPU/GPU flow |
| [docs/security-notes.md](docs/security-notes.md) | Core invariants, fee routing, vault routing, non-goals |
| [docs/tokenomics.md](docs/tokenomics.md) | Supply, allocation, reward accounting, cap structure |
| [MINING_CURVE.md](MINING_CURVE.md) | Exact era schedule and Scarcity tail |
| [LIVE_CONFIG_NOTES.md](LIVE_CONFIG_NOTES.md) | Public live constants and runtime references |

## System Shape

### Off-chain

- fetch the current block snapshot
- iterate nonces on CPU or GPU
- validate candidates against the current 256-bit target
- submit only a winning nonce

### On-chain

- store the canonical live block
- verify `SHA256(challenge || miner_pubkey || nonce_le_u64)`
- transfer the fixed `0.01 SOL` accepted-block fee to the treasury wallet
- split the BLOC reward `99% / 1%`
- emit the solved-block event trail
- retarget difficulty and open the next block

## Repository Layout

- `onchain/` - Anchor workspace and Solana program
- `miner-client/` - Rust CLI miner and desktop client
- `docs/` - technical notes for the public core
- `packaging/` - Windows and macOS packaging helpers
- `scripts/` - local build wrappers

## Build

### Windows

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\build-miner-exe.ps1
```

### macOS

```bash
chmod +x packaging/macos/*.command
chmod +x packaging/macos/scripts/*.sh
./packaging/macos/build-macos.command
```

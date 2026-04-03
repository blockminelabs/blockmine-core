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

# Security Notes

## Core invariants

The Blockmine runtime enforces the following invariants on-chain.

### Miner-bound proof

The accepted hash includes the miner public key in the preimage:

```text
SHA256(challenge || miner_pubkey || nonce_le_u64)
```

Copying a nonce is therefore not sufficient to steal a reward under another wallet.

### One winner per block

There is one writable `CurrentBlock`. Settlement transitions that account from open to solved. Solana account locking serializes competing winners on the same mutable state.

### Fee-before-reward ordering

Accepted settlement transfers the fixed `0.01 SOL` submit fee before BLOC rewards are paid.

If the fee transfer fails, the instruction aborts and no reward transfer is committed.

### Canonical vault routing

The program reads the mint, reward vault, treasury wallet, and treasury ATA from `ProtocolConfig`, then enforces those addresses during settlement.

### Pre-funded reward inventory

The mining allocation is not minted per block. Rewards are paid out of the reward vault that was funded at initialization.

### Emissions keyed to settled work

Reward era progression is indexed by `total_blocks_mined`, not by raw block openings. Stale rotations therefore preserve scheduled emissions.

## Accepted-block fee

The accepted-block fee is fixed at `10_000_000` lamports, which is `0.01 SOL`.

The initialization path enforces this fixed value. The runtime settlement path transfers it directly to the treasury wallet. A public miner cannot settle a winning block without executing that transfer.

## Treasury split

For each accepted reward:

- `1%` of the BLOC reward is transferred to the treasury ATA
- `99%` is transferred to the miner ATA
- `0.01 SOL` is transferred to the treasury wallet

The BLOC split is enforced by the program. The SOL fee is enforced by the program. Neither flow depends on a client-declared percentage.

## Event audit trail

The public history of settlement exists in the event stream:

- `BlockOpened`
- `BlockSolved`
- `DifficultyAdjusted`
- `BlockStaleRotated`

`BlockSolved` includes enough information to reconstruct the solved block:

- block number
- winner
- nonce
- hash
- challenge
- difficulty bits
- full difficulty target
- gross reward
- miner reward
- treasury reward
- submit fee

## Explicit non-goals

The protocol does not claim to solve all ordering and routing problems in a public mempool.

Binding the miner pubkey to the proof prevents simple nonce theft. It does not mathematically eliminate:

- validator ordering
- censorship
- generalized mempool games

## Operational assumptions

Three important properties depend on deployment posture rather than on the runtime alone:

- treasury wallet custody
- mint authority removal
- upgrade authority removal

The reward vault itself is program-controlled. The treasury wallet remains an external wallet. That is a custody model, not a flaw in the reward vault logic.

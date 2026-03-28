# Security Notes

## Core protections in V1

- the proof binds `challenge + miner_pubkey + nonce`
- the reward vault is owned by the program PDA
- accepted-block `SOL` fees are routed by the contract
- the treasury `BLOC` cut is routed by the contract
- one mutable current-block account serializes winner settlement
- stale rotation preserves liveness

## What is strongly enforced on-chain

### Proof ownership

Copying a nonce is not enough to steal a reward because the miner pubkey is part of the hashed preimage.

### Canonical reward routing

The program reads:

- canonical mint
- canonical reward vault
- canonical treasury authority
- canonical treasury vault

from protocol config, then enforces those accounts during settlement.

### Reward vault safety

The mining allocation sits in a PDA-controlled reward vault. Transfers from that vault require the program signer path.

## Important operational assumptions

The launch still depends on correct operations around:

- funding the reward vault with the full mining allocation
- funding the treasury and LP allocations correctly
- revoking mint authority after allocation
- revoking freeze authority after allocation
- deciding when to remove admin and upgrade authority

These are launch and governance concerns, not automatic properties of the runtime alone.

## Current V1 limitations

### Ordering and mempool visibility

Binding the miner pubkey blocks simple nonce theft, but it does not fully solve:

- validator ordering
- censorship
- more advanced MEV-style routing issues

### Admin surface

The codebase still contains admin instructions and an upgrade path. If a fully immutable posture is required, those controls must be removed or permanently disabled at the deployment layer.

### Treasury model

The `BLOC` reward vault is program-controlled. The treasury authority remains an external wallet authority, which means treasury custody is operational rather than fully contract-locked.

## Recommended end-state posture

- mint authority revoked
- freeze authority revoked
- treasury routing fixed
- admin controls removed or locked down
- upgrade authority removed
- reproducible and publicly verifiable build for the deployed program

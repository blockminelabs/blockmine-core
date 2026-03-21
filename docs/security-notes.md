# Security Notes

## Main V1 protections

- solution hash binds `challenge + miner pubkey + nonce`
- reward vault is PDA-controlled
- mint authority should be revoked after funding the vault
- current block is a single mutable account, so winners are serialized by account locking
- admin pause exists for emergencies

## Replay protection

Replay across blocks is prevented by challenge rotation. A valid nonce for block `N` should not remain valid for block `N+1`.

## Front-running

V1 mitigation:

- bind the miner wallet into the hash

V1 limitation:

- direct submit remains visible in the mempool

V2 recommendation:

- commit-reveal flow for anti-front-running hardening

## Duplicate rewards

The reward path and block rotation happen in one instruction over the same mutable state. If two submissions race:

- one transaction wins the account lock and closes the block
- the later transaction should fail because the state has already advanced

## Vault safety

The reward vault is an ATA owned by the vault-authority PDA, so rewards require a valid PDA signer path from the program.

## Trust assumptions

V1 assumes:

- admin performs setup correctly
- full supply is minted to the reward vault
- mint authority is revoked

Those steps are social-operational risks, not purely programmatic ones, so launch checklists matter.


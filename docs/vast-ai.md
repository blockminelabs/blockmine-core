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

# Vast.ai Mining

Blockmine can run on Vast.ai as a headless Linux worker.

The public Vast template is designed so that every instance mines to its own wallet. No shared key is embedded in the image.

## Launch model

Use the Blockmine Vast template in:

- `Jupyter + SSH` mode

This gives the instance an interactive terminal. The worker uses that terminal once on first boot to reveal the recovery material only after explicit confirmation.

## First boot

On first boot the container creates a dedicated worker wallet if one does not already exist in the instance storage directory.

To reveal the wallet recovery material, run:

```bash
blockmine-wallet reveal
```

The command will:

1. warn that the recovery material controls the mined funds
2. require `YES` before showing the recovery material
3. show the public address, recovery phrase, and private key
4. require `Y` to confirm that the recovery material has been stored
5. print the funding target for the current era and current block

## Funding

Once the recovery material has been confirmed, the worker prints:

- the wallet address
- the fixed accepted-block fee in SOL
- the current gross block reward in BLOC
- the current era
- the current block number

Send SOL to the worker wallet. The worker waits until the wallet balance is high enough to start mining.

## Start behavior

The Vast worker starts automatically after:

- the wallet backup has been confirmed
- the wallet has enough SOL to pay the accepted-block fee and transaction fees

No manual `mine` command is required after that point.

## Leaderboard

The headless worker uses the same signed leaderboard heartbeat path as the desktop miner.

Once the worker starts mining, it appears on the public leaderboard as:

- platform: `Linux`
- backend: `CPU`, `GPU`, or `BOTH`, depending on the worker configuration

The worker does not need to find a block first. It appears as soon as the mining loop is live and the heartbeat starts flowing.

## Runtime controls

The worker binary is:

```bash
blockmine-vast-worker
```

Useful environment variables:

- `BLOCKMINE_RPC_URL`
- `BLOCKMINE_PROGRAM_ID`
- `BLOCKMINE_SITE_URL`
- `BLOCKMINE_BACKEND`
- `BLOCKMINE_BATCH_SIZE`
- `BLOCKMINE_GPU_BATCH_SIZE`
- `BLOCKMINE_CPU_THREADS`
- `BLOCKMINE_GPU_PLATFORM`
- `BLOCKMINE_GPU_DEVICE`
- `BLOCKMINE_GPU_DEVICES`
- `BLOCKMINE_GPU_LOCAL_WORK_SIZE`
- `BLOCKMINE_MIN_START_SOL`
- `BLOCKMINE_FUNDING_POLL_SECONDS`
- `BLOCKMINE_STORAGE_DIR`

## Wallet commands

The image also includes:

```bash
blockmine-wallet ensure
blockmine-wallet address
blockmine-wallet keypair-path
blockmine-wallet backup-status
blockmine-wallet funding-hint
blockmine-wallet reveal
```

## Persistence

The worker stores its wallet and backup marker under:

- `BLOCKMINE_STORAGE_DIR`

Default:

- `/workspace/blockmine-data`

If that directory is preserved, the worker keeps the same wallet across restarts.

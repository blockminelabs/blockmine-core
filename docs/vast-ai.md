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

Blockmine can run on Vast.ai as an interactive Linux mining console.

The public Vast template is designed so that every instance mines to its own wallet. No shared key is embedded in the image.

## Launch model

Use the Blockmine Vast template in:

- `Jupyter + SSH` mode

This gives the instance an interactive terminal. The Blockmine console uses that terminal once on first boot to reveal the recovery material only after explicit confirmation, then it stays open as a live mining dashboard.

## Template source

There are two valid template sources:

1. a prebuilt Blockmine image
2. a public bootstrap flow that starts from a public CUDA base image and pulls the public Blockmine core repository at boot

The public bootstrap path exists so the template can be used immediately even before a dedicated container registry release is published.

Recommended public bootstrap image:

- `nvidia/cuda:12.8.0-devel-ubuntu22.04`

That CUDA floor matters for modern NVIDIA fleets. On Vast, RTX 5000-series / Blackwell inventory is matched against CUDA 12.8-compatible templates.

Recommended public on-start command:

```bash
bash -lc "$(curl -fsSL https://raw.githubusercontent.com/blockminelabs/blockmine-core/main/packaging/vastai/scripts/bootstrap-vast.sh)"
```

## First boot

On first boot the container creates a dedicated worker wallet if one does not already exist in the instance storage directory.

When the user opens a Jupyter terminal or SSH session, the Blockmine console launches automatically.

If it does not, run:

```bash
blockmine-vast-console
```

The console will:

1. warn that the recovery material controls the mined funds
2. require `YES` before showing the recovery material
3. show the public address, recovery phrase, and private key
4. require `YES` again to confirm that the recovery material has been stored
5. clear the screen and open the live mining console

## Funding

Once the recovery material has been confirmed, the console prints:

- the wallet address
- the fixed accepted-block fee in SOL
- the current gross block reward in BLOC
- the current era
- the current block number

Send SOL to the worker wallet. The console polls the wallet balance live and starts mining automatically as soon as the balance is high enough.

## Start behavior

The Vast mining loop starts automatically after:

- the wallet backup has been confirmed
- the wallet has enough SOL to pay the accepted-block fee and transaction fees

No manual `mine` command is required after that point.

## Console controls

Inside the live console:

- `S` starts or stops mining
- `W` opens the withdrawal flow for SOL and BLOC
- `R` refreshes the GPU probe
- `Q` exits the console

The withdrawal flow accepts fixed amounts or `MAX`.

## GPU detection

The template probes both:

- the NVIDIA runtime via `nvidia-smi`
- the OpenCL device layer used by the Blockmine GPU miner

Today the Linux GPU miner still uses OpenCL for the hashing backend. That means a Vast instance must expose:

- the NVIDIA runtime
- a usable OpenCL platform inside the container

If the instance shows `nvidia-smi` but no OpenCL devices, the console stays alive and reports the mismatch instead of crashing.

If the instance shows NVIDIA hardware but no OpenCL devices, the template keeps the console live and waits instead of crashing the miner process.

## Leaderboard

The interactive console uses the same signed leaderboard heartbeat path as the desktop miner.

Once the worker starts mining, it appears on the public leaderboard as:

- platform: `Mining Rig - Vast.ai`
- backend: `CPU`, `GPU`, or `BOTH`, depending on the worker configuration
- hardware summary: the detected GPU fleet, for example `4x NVIDIA GeForce RTX 5090`

The worker does not need to find a block first. It appears as soon as the mining loop is live and the heartbeat starts flowing.

## Runtime controls

The interactive console binary is:

```bash
blockmine-vast-console
```

The headless worker binary remains available for non-interactive use:

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
- `BLOCKMINE_REPO_URL`
- `BLOCKMINE_REPO_DIR`

## Wallet commands

The image also includes:

```bash
blockmine-wallet ensure
blockmine-wallet address
blockmine-wallet keypair-path
blockmine-wallet backup-status
blockmine-wallet funding-hint
blockmine-wallet reveal
blockmine-vast-console
```

## Persistence

The worker stores its wallet and backup marker under:

- `BLOCKMINE_STORAGE_DIR`

Default:

- `/workspace/blockmine-data`

If that directory is preserved, the worker keeps the same wallet across restarts.

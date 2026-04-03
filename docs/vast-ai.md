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

Blockmine can run on Vast.ai as an interactive Linux mining rig.

The working path is:

1. launch the Blockmine template in `Jupyter + SSH`
2. bootstrap the worker
3. fix OpenCL manually if the instance does not expose it immediately
4. open the Blockmine Vast console
5. reveal the wallet, back up the recovery material, fund it, and start mining

## Recommended template

Image:

- `nvidia/cuda:12.8.0-devel-ubuntu22.04`

Launch mode:

- `Jupyter + SSH`

On-start script:

```bash
bash -lc "$(curl -fsSL https://raw.githubusercontent.com/blockminelabs/blockmine-core/main/packaging/vastai/scripts/bootstrap-vast.sh)"
```

Environment variables:

```text
BLOCKMINE_RPC_URL=https://solana-rpc.publicnode.com
BLOCKMINE_PROGRAM_ID=FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv
BLOCKMINE_SITE_URL=https://blockmine.dev
BLOCKMINE_BACKEND=gpu
BLOCKMINE_BATCH_SIZE=250000
BLOCKMINE_GPU_BATCH_SIZE=1048576
BLOCKMINE_CPU_THREADS=0
BLOCKMINE_MIN_START_SOL=0.05
BLOCKMINE_FUNDING_POLL_SECONDS=5
BLOCKMINE_STORAGE_DIR=/workspace/blockmine-data
BLOCKMINE_PLATFORM_DETAIL=Mining Rig - Vast.ai
NVIDIA_VISIBLE_DEVICES=all
NVIDIA_DRIVER_CAPABILITIES=all
```

## Full operator flow

### 1. Open the instance terminal

Open the Vast instance created from the Blockmine template, then enter the Jupyter terminal or SSH session.

### 2. Bootstrap the worker

```bash
bash -lc "$(curl -fsSL https://raw.githubusercontent.com/blockminelabs/blockmine-core/main/packaging/vastai/scripts/bootstrap-vast.sh)"
```

### 3. If OpenCL is not visible immediately, initialize it manually

```bash
mkdir -p /etc/OpenCL/vendors
printf 'libnvidia-opencl.so.1\n' >/etc/OpenCL/vendors/nvidia.icd

cat >/etc/profile.d/blockmine-opencl.sh <<'EOF'
export OCL_ICD_VENDORS=/etc/OpenCL/vendors
export OPENCL_VENDOR_PATH=/etc/OpenCL/vendors
export LD_LIBRARY_PATH=/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}
EOF

export OCL_ICD_VENDORS=/etc/OpenCL/vendors
export OPENCL_VENDOR_PATH=/etc/OpenCL/vendors
export LD_LIBRARY_PATH=/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH}
```

### 4. Verify that the OpenCL GPUs are visible

```bash
clinfo -l
```

Expected result:

- one NVIDIA platform
- one device entry for each GPU in the rig

### 5. Start the Blockmine Vast console

```bash
/workspace/blockmine-core/miner-client/target/release/blockmine-vast-console
```

### 6. First boot flow

On first boot the console will:

1. ask for `YES` before it reveals the recovery material
2. show the wallet address, recovery phrase, and private key
3. ask for `YES` again to confirm the backup
4. show the deposit address, fee per accepted block, current era, and current reward

Fund the wallet shown in the console with SOL.

### 7. Console controls

Inside the console:

- `S` starts or stops mining
- `W` withdraws `SOL` or `BLOC`
- `R` refreshes the GPU probe
- `Q` exits the console

### 8. Reopen the console later

```bash
/workspace/blockmine-core/miner-client/target/release/blockmine-vast-console
```

### 9. Print only the wallet address

```bash
/workspace/blockmine-core/miner-client/target/release/blockmine-wallet address
```

### 10. Reveal the recovery material again

```bash
/workspace/blockmine-core/miner-client/target/release/blockmine-wallet reveal
```

### 11. Inspect the GPUs outside the console

```bash
nvidia-smi
watch -n 1 nvidia-smi
```

## Bootstrap status

If you want to watch the pull, dependency install, and Rust build:

```bash
tail -f /workspace/blockmine-logs/bootstrap.log
```

## What the console is supposed to show

Once funded and running, the console reports:

- live rate
- attempts
- blocks mined
- mined BLOC
- current era
- current block
- current reward
- difficulty
- wallet balances
- rig summary
- OpenCL status

## Leaderboard behavior

Vast workers send the same signed heartbeat used by the desktop miner.

When the worker is live, the public leaderboard shows:

- platform detail: `Mining Rig - Vast.ai`
- hardware summary: for example `2x NVIDIA GeForce RTX 5090` or `8x NVIDIA GeForce RTX 5090`

The worker can appear on the leaderboard before it finds the next block, as soon as the mining loop and heartbeat are both alive.

## Notes

- The Linux GPU miner currently uses OpenCL.
- `nvidia-smi` alone is not enough. A usable OpenCL platform must exist inside the container.
- If the template boots with CUDA visible but OpenCL missing, the manual ICD fix above is the correct recovery path.

# Vast.ai Packaging

Build the public Vast.ai image from the repository root:

```bash
docker build -f packaging/vastai/Dockerfile -t ghcr.io/blockminelabs/blockmine-vast:latest .
```

The image includes:

- `blockmine-miner`
- `blockmine-wallet`
- `blockmine-vast-console`
- `blockmine-vast-worker`
- `/opt/blockmine/scripts/on-start.sh`
- `/opt/blockmine/scripts/start-miner.sh`

Recommended Vast launch mode:

- `Jupyter + SSH`

Recommended on-start command:

```bash
/opt/blockmine/scripts/on-start.sh
```

Runtime storage defaults to:

- `/workspace/blockmine-data`

The interactive console uses the same signed leaderboard heartbeat path as the desktop miner. When mining starts, the worker appears on the public leaderboard as a Linux miner.

The on-start flow:

- ensures the worker wallet exists
- configures the NVIDIA OpenCL ICD if the driver is mounted into the container
- installs an auto-launch hook so `blockmine-vast-console` opens when the user enters Jupyter or SSH
- leaves headless background mining disabled unless `BLOCKMINE_HEADLESS_AUTOSTART=1`

## Public bootstrap path

If a prebuilt image is not yet published, the public Vast template can use an Ubuntu CUDA base image and bootstrap Blockmine directly from this repository.

Recommended public base image:

- `nvidia/cuda:12.8.0-devel-ubuntu22.04`

Use CUDA 12.8 for public Vast templates. That is the compatibility floor for RTX 5000-series / Blackwell inventory on Vast.

Recommended on-start command:

```bash
bash -lc "$(curl -fsSL https://raw.githubusercontent.com/blockminelabs/blockmine-core/main/packaging/vastai/scripts/bootstrap-vast.sh)"
```

That path:

- installs the required packages
- installs Rust if needed
- clones or updates the public Blockmine core repo
- builds `blockmine-wallet`, `blockmine-vast-console`, and `blockmine-vast-worker`
- installs the interactive auto-console flow
- keeps the same wallet under `/workspace/blockmine-data`

Important:

- the current Linux GPU miner still uses OpenCL
- the template therefore needs both the NVIDIA runtime and a usable OpenCL platform in the container
- the console will stay live and report the mismatch if `nvidia-smi` works but no OpenCL device is exposed

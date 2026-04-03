# Vast.ai Packaging

Build the public Vast.ai image from the repository root:

```bash
docker build -f packaging/vastai/Dockerfile -t ghcr.io/blockminelabs/blockmine-vast:latest .
```

The image includes:

- `blockmine-miner`
- `blockmine-wallet`
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

The headless worker uses the same signed leaderboard heartbeat path as the desktop miner. When mining starts, the worker appears on the public leaderboard as a Linux miner.

## Public bootstrap path

If a prebuilt image is not yet published, the public Vast template can use an Ubuntu CUDA base image and bootstrap Blockmine directly from this repository.

Recommended public base image:

- `nvidia/cuda:12.4.1-devel-ubuntu22.04`

Recommended on-start command:

```bash
bash -lc "$(curl -fsSL https://raw.githubusercontent.com/blockminelabs/blockmine-core/main/packaging/vastai/scripts/bootstrap-vast.sh)"
```

That path:

- installs the required packages
- installs Rust if needed
- clones or updates the public Blockmine core repo
- builds `blockmine-wallet` and `blockmine-vast-worker`
- starts the worker
- keeps the same wallet under `/workspace/blockmine-data`

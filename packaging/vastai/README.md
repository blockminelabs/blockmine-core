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

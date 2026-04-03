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

# Miner Client

## Binaries

The repository ships two public miner interfaces built from the same Rust codebase.

- `blockmine-miner` - command-line interface
- `blockmine-studio` - desktop client for Windows and macOS
- `blockmine-wallet` - worker-wallet utility for headless Vast.ai instances
- `blockmine-vast-worker` - headless mining worker with signed leaderboard heartbeats

Both use the same:

- program ID
- RPC read path
- proof rule
- submission flow

The Vast worker also uses the same signed heartbeat route as the desktop miner, so Linux workers appear on the public leaderboard as soon as the mining loop starts.

## Public defaults

- Program ID: `FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv`
- Default RPC: `auto` (`https://api.mainnet-beta.solana.com`, fallback `https://solana-rpc.publicnode.com`)

These defaults can be overridden with CLI flags.

## Command-line interface

The CLI supports:

- `protocol-state`
- `register`
- `mine`
- `desktop`
- `benchmark`
- `list-devices`
- `wallet-stats`
- `submit-test`
- `init-protocol`

Example usage:

```bash
cargo run --manifest-path miner-client/Cargo.toml --bin blockmine-miner -- protocol-state
cargo run --manifest-path miner-client/Cargo.toml --bin blockmine-miner -- register --nickname rig01
cargo run --manifest-path miner-client/Cargo.toml --bin blockmine-miner -- mine --backend cpu --batch-size 250000
cargo run --manifest-path miner-client/Cargo.toml --bin blockmine-miner --features opencl -- list-devices
cargo run --manifest-path miner-client/Cargo.toml --bin blockmine-miner --features opencl -- mine --backend gpu --gpu-platform 0 --gpu-device 0 --gpu-batch-size 1048576
```

## Mining loop

The miner loop is simple.

1. Fetch the current block snapshot.
2. Read `challenge`, `difficulty_target`, `block_reward`, and block status.
3. Construct `challenge || miner_pubkey || nonce_le_u64`.
4. Search nonce space on CPU, GPU, or both.
5. Check candidate hashes against the current target.
6. Build a settlement transaction only when a valid nonce is found.

The miner does not stream every attempt to the chain. It only submits a candidate that already satisfies the target.

## Desktop client

The desktop client adds:

- local wallet manager
- create new wallet
- import by mnemonic
- import by private key
- remembered selected wallet
- QR-assisted manual funding
- live hashrate chart
- CPU, GPU, and hybrid execution

The selected wallet is the active miner identity whose balances and mining state are displayed.

## Wallet storage

Managed wallets live in the local Blockmine application data directory on the user's machine.

The client stores:

- the wallet label
- the key material
- the selected-wallet preference

Wallets are not derived from a shared global seed. Each created wallet is generated locally from fresh entropy on the host machine.

## Funding and payout

The miner wallet is responsible for:

- the accepted-block `0.01 SOL` fee
- standard Solana transaction fees
- miner ATA creation if needed

Accepted BLOC rewards are sent to the selected miner wallet ATA.

## GPU execution

GPU mode requires:

- an OpenCL-enabled build
- a working OpenCL runtime
- correct platform and device selection

The GPU path and the CPU path use the same proof rule and the same submit path. Only the nonce-search engine changes.

## Vast.ai worker

The repository also includes a headless worker flow for Vast.ai:

- `blockmine-wallet ensure`
- `blockmine-wallet reveal`
- `blockmine-wallet funding-hint`
- `blockmine-vast-worker`

The intended public template flow is:

1. launch the worker in `Jupyter + SSH`
2. generate or load the worker wallet
3. reveal and confirm the recovery material
4. fund the wallet with SOL
5. start mining automatically
6. broadcast signed heartbeats to the public leaderboard

See [vast-ai.md](vast-ai.md) for the public runtime guide.

## Windows build

From the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\packaging\windows\build-miner-exe.ps1
```

Artifacts:

- `dist/Blockmine Miner.exe`
- `dist/start-blockmine-studio.bat`
- `dist/README-blockmine-studio.txt`

## macOS build

From the repository root on a Mac:

```bash
chmod +x packaging/macos/*.command
chmod +x packaging/macos/scripts/*.sh
./packaging/macos/build-macos.command
```

Artifacts:

- `dist/Blockmine Miner.app`
- `dist/Blockmine Miner.dmg`

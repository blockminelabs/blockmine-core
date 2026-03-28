# Miner Client

## Purpose

The miner client is the public execution layer for Blockmine.

It supports:

- CPU mining
- GPU mining with OpenCL builds
- hybrid CPU + GPU mining
- protocol-state inspection
- miner registration
- test submission and debugging flows
- desktop wallet management

## Mainnet defaults

Current public defaults in the core repo:

- program ID: `FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv`
- RPC default: `https://api.mainnet-beta.solana.com`

These defaults can always be overridden with CLI flags.

## CLI commands

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

## Desktop client

The desktop app is the main end-user miner for Windows and macOS.

Current features:

- wallet manager
- create new wallet
- import from seed phrase
- import from private key
- remember the last selected wallet
- manual funding with QR code
- CPU, GPU, and hybrid execution
- live mining stats and hashrate chart

The selected wallet is the wallet whose balances and mining state the app should display.

## Wallet model

The miner stores local wallets under the Blockmine app storage directory on the user machine.

The wallet manager is intentionally local-first:

- wallet keys stay on the machine
- the app can create dedicated mining wallets
- the app can import existing recovery phrases or private keys
- the desktop app remembers the last selected wallet between launches

Legacy desktop-session wallet files are migrated into the newer managed-wallet format automatically.

## Windows build

The current packaged Windows binary is:

- `dist/Blockmine Miner.exe`

Packaging command:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-miner-exe.ps1
```

The build script handles the branded icon and emits the final executable into `dist/`.

## macOS build

Use the packaging helper:

```bash
chmod +x packaging/macos/*.command
chmod +x packaging/macos/scripts/*.sh
./packaging/macos/build-macos.command
```

Expected output:

- `dist/Blockmine Miner.app`
- optional `.dmg` output in `dist/`

## GPU notes

GPU mining requires:

- an OpenCL-enabled build
- a working OpenCL runtime on the host
- correct platform and device selection

The miner will fail clearly if GPU mode is selected without the required runtime support.

## Dev and rehearsal tooling

The repo still contains devnet and rehearsal helpers under `onchain/scripts/` and `miner-client/src/bin/devnet-admin.rs`.

Those tools are for local testing and controlled rehearsals. They are not part of the normal public mining flow.

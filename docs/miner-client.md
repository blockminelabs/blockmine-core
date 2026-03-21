# Miner Client

## Purpose

The miner is a Rust CLI for:

- mining BLOC off-chain on CPU
- mining BLOC off-chain on GPU when built with OpenCL support
- running CPU+GPU hybrid batches
- submitting winning solutions
- reading protocol state
- reading miner stats
- running benchmarks
- bootstrapping the protocol on Devnet
- inspecting treasury config and fee settings

## Structure

- `cli.rs`: command parsing
- `config.rs`: runtime config
- `rpc.rs`: on-chain reads and PDA derivation
- `hashing.rs`: SHA-256 logic
- `engine/`: pluggable mining backend
- `submitter.rs`: transaction building and submission
- `commands/`: user-facing commands

## Commands

- `init-protocol`
- `mine`
- `desktop`
- `benchmark`
- `list-devices`
- `protocol-state`
- `wallet-stats`
- `submit-test`
- `register`

## Init protocol notes

`init-protocol` now supports:

- optional `--treasury-authority`
- fixed `--treasury-fee-bps`, which must be `100` in V1 for a `1%` treasury cut

If `--treasury-authority` is omitted, the initializer wallet is used as treasury authority.

## Example usage

```bash
cargo run --manifest-path miner-client/Cargo.toml -- --rpc https://api.devnet.solana.com protocol-state
cargo run --manifest-path miner-client/Cargo.toml -- --rpc https://api.devnet.solana.com register --nickname rig01
cargo run --manifest-path miner-client/Cargo.toml --features opencl -- list-devices
cargo run --manifest-path miner-client/Cargo.toml -- --rpc https://api.devnet.solana.com mine --backend cpu --batch-size 250000
cargo run --manifest-path miner-client/Cargo.toml --features opencl -- --rpc https://api.devnet.solana.com mine --backend gpu --gpu-platform 0 --gpu-device 0 --batch-size 262144
cargo run --manifest-path miner-client/Cargo.toml --features opencl -- --rpc https://api.devnet.solana.com mine --backend both --batch-size 250000 --gpu-platform 0 --gpu-device 0 --gpu-batch-size 131072
```

## Desktop EXE

The repo now supports a Windows desktop build with a dynamic terminal dashboard.

What the live miner terminal shows:

- current block
- reward
- difficulty bits
- session blocks mined
- session BLOC mined
- total wallet blocks mined
- total wallet BLOC mined
- treasury fees distributed
- last nonce, hash, and accepted transaction signature

Artifacts:

- `dist/miner.exe`
- `dist/start-blockmine-studio.bat`
- `dist/README-blockmine-studio.txt`

Packaging script:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build-miner-exe.ps1
```

Notes:

- the current Windows GUI miner is `dist/miner.exe`
- the GUI must be built with OpenCL support or GPU selection disappears
- the most reliable desktop build path is:

```bash
bash /mnt/c/Users/drums/Desktop/BLOC/miner-client/scripts/wsl-build-windows.sh --features opencl --bin blockmine-studio
```

- that build emits:

```text
miner-client/target/x86_64-pc-windows-gnu/release/blockmine-studio.exe
```

- after building, copy it to:

```text
dist/miner.exe
```

- if the GUI shows `Listing GPU devices requires building the miner with --features opencl and an installed OpenCL runtime`, the running EXE is the wrong build
- the desktop wallet approval bridge should open `http://127.0.0.1:3000/desktop-bridge`
- in this environment `127.0.0.1` has proven more reliable than `localhost`

## Backend selection

- `--backend cpu`: CPU only
- `--backend gpu`: GPU only
- `--backend both`: one CPU batch plus one GPU batch in the same round
- `--gpu-platform`: OpenCL platform index to use
- `--gpu-device`: device index inside the selected platform

Recommended first step for any machine:

- run `list-devices`
- identify the right OpenCL platform and device
- then launch `mine` or `benchmark` with those indices

Important:

- GPU mode requires building with `--features opencl`
- the host machine also needs a working OpenCL runtime and drivers
- without that feature/runtime, GPU mode fails fast with a clear error instead of silently falling back

## Desktop GUI operational notes

- the GUI miner is no longer positioned around dedicated local wallets; the main flow is wallet bridge plus delegated mining
- the GUI should show:
  - current era
  - reward per block
  - info popup for the era schedule from `MINING_CURVE.md`
- the browser approval page and the desktop callback depend on the local site being up at `127.0.0.1:3000`
- if the bridge page loads without CSS/JS, restart the local Next server before debugging wallet logic
- if GPU mining is selected and no checkboxes appear, treat it as a build artifact problem first, not a runtime mining bug

## V1 engine notes

The miner runs in batches, refreshes chain state between rounds, and then submits the first valid nonce found in the round. That keeps the control flow simple and reduces wasted work when another miner wins the block first.

For `--backend both`, V1 is still batch-oriented. In practice that means:

- smaller GPU batch sizes reduce stale-solution latency
- larger GPU batch sizes improve throughput but can delay submission if CPU finds first

That tradeoff is acceptable for V1 and can be improved later with persistent workers and better cancellation.

## Why OpenCL

OpenCL is the most practical cross-vendor starting point for a V1 GPU path because it can target more than one GPU ecosystem. CUDA-specific optimization can still be added later if NVIDIA-only tuning becomes important.

This also makes the miner more general for:

- NVIDIA GPUs
- AMD GPUs
- Intel integrated and discrete GPUs
- mixed systems with more than one OpenCL platform

## Future GPU work

- better kernel tuning
- persistent GPU workers
- multi-GPU scheduling
- CUDA-specific backend
- benchmark and auto-tuning per device

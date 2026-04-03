#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export BLOCKMINE_STORAGE_DIR="${BLOCKMINE_STORAGE_DIR:-/workspace/blockmine-data}"
export BLOCKMINE_SITE_URL="${BLOCKMINE_SITE_URL:-https://blockmine.dev}"
export BLOCKMINE_RPC_URL="${BLOCKMINE_RPC_URL:-auto}"
export BLOCKMINE_PROGRAM_ID="${BLOCKMINE_PROGRAM_ID:-FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv}"
export BLOCKMINE_LEADERBOARD_INGEST_URL="${BLOCKMINE_LEADERBOARD_INGEST_URL:-}"
export BLOCKMINE_REPO_URL="${BLOCKMINE_REPO_URL:-https://github.com/blockminelabs/blockmine-core.git}"
export BLOCKMINE_REPO_DIR="${BLOCKMINE_REPO_DIR:-/workspace/blockmine-core}"
export BLOCKMINE_HEADLESS_AUTOSTART="${BLOCKMINE_HEADLESS_AUTOSTART:-0}"
export PATH="${HOME}/.cargo/bin:${PATH}"

LOG_DIR="${BLOCKMINE_LOG_DIR:-/workspace/blockmine-logs}"
LOG_FILE="${LOG_DIR}/blockmine-vast-worker.log"
BOOTSTRAP_LOG_FILE="${LOG_DIR}/bootstrap.log"
PROFILE_FILE="/etc/profile.d/blockmine-vast.sh"

mkdir -p "${BLOCKMINE_STORAGE_DIR}" "${LOG_DIR}" /workspace
touch "${BOOTSTRAP_LOG_FILE}"
if [ -z "${BLOCKMINE_BOOTSTRAP_LOGGING:-}" ]; then
  export BLOCKMINE_BOOTSTRAP_LOGGING=1
  exec > >(tee -a "${BOOTSTRAP_LOG_FILE}") 2>&1
fi
env | grep _ >> /etc/environment || true

cat >"${PROFILE_FILE}" <<EOF
export BLOCKMINE_STORAGE_DIR="${BLOCKMINE_STORAGE_DIR}"
export BLOCKMINE_SITE_URL="${BLOCKMINE_SITE_URL}"
export BLOCKMINE_RPC_URL="${BLOCKMINE_RPC_URL}"
export BLOCKMINE_PROGRAM_ID="${BLOCKMINE_PROGRAM_ID}"
export BLOCKMINE_LEADERBOARD_INGEST_URL="${BLOCKMINE_LEADERBOARD_INGEST_URL}"
export PATH="${HOME}/.cargo/bin:\${PATH}"
EOF
chmod 644 "${PROFILE_FILE}"

if ! command -v apt-get >/dev/null 2>&1; then
  echo "[blockmine] apt-get not found; use the Ubuntu CUDA base image for this template." >&2
  exit 1
fi

echo "[blockmine] installing system dependencies"
apt-get update
apt-get install -y --no-install-recommends \
  bash \
  build-essential \
  ca-certificates \
  clang \
  clinfo \
  cmake \
  curl \
  git \
  libssl-dev \
  ocl-icd-libopencl1 \
  ocl-icd-opencl-dev \
  opencl-headers \
  pkg-config \
  procps \
  tmux
rm -rf /var/lib/apt/lists/*

if ! command -v cargo >/dev/null 2>&1; then
  echo "[blockmine] installing rust toolchain"
  curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal
fi

if [ ! -d "${BLOCKMINE_REPO_DIR}/.git" ]; then
  echo "[blockmine] cloning core repo"
  git clone "${BLOCKMINE_REPO_URL}" "${BLOCKMINE_REPO_DIR}"
else
  echo "[blockmine] updating core repo"
  git -C "${BLOCKMINE_REPO_DIR}" fetch origin
  git -C "${BLOCKMINE_REPO_DIR}" pull --ff-only origin main
fi

echo "[blockmine] building worker binaries"
cargo build --release \
  --manifest-path "${BLOCKMINE_REPO_DIR}/miner-client/Cargo.toml" \
  --features opencl \
  --bin blockmine-wallet \
  --bin blockmine-vast-worker \
  --bin blockmine-vast-console

echo "[blockmine] preparing worker wallet"
"${BLOCKMINE_REPO_DIR}/miner-client/target/release/blockmine-wallet" ensure
"${BLOCKMINE_REPO_DIR}/packaging/vastai/scripts/ensure-opencl-nvidia.sh"
"${BLOCKMINE_REPO_DIR}/packaging/vastai/scripts/install-auto-console.sh" \
  "${BLOCKMINE_REPO_DIR}/miner-client/target/release/blockmine-vast-console"

echo
echo "[blockmine] open a Jupyter or SSH terminal."
echo "[blockmine] the Blockmine console will launch automatically."
echo "[blockmine] if it does not, run:"
echo "[blockmine]   ${BLOCKMINE_REPO_DIR}/miner-client/target/release/blockmine-vast-console"
echo "[blockmine] bootstrap log: ${BOOTSTRAP_LOG_FILE}"
echo "[blockmine] logs (headless mode only): ${LOG_FILE}"

if [ "${BLOCKMINE_HEADLESS_AUTOSTART}" = "1" ]; then
  echo "[blockmine] headless autostart enabled"
  nohup "${BLOCKMINE_REPO_DIR}/miner-client/target/release/blockmine-vast-worker" >>"${LOG_FILE}" 2>&1 &
fi

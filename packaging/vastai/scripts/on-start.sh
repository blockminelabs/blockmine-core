#!/usr/bin/env bash
set -euo pipefail

export BLOCKMINE_STORAGE_DIR="${BLOCKMINE_STORAGE_DIR:-/workspace/blockmine-data}"
export BLOCKMINE_SITE_URL="${BLOCKMINE_SITE_URL:-https://blockmine.dev}"
export BLOCKMINE_RPC_URL="${BLOCKMINE_RPC_URL:-https://api.mainnet-beta.solana.com}"
export BLOCKMINE_PROGRAM_ID="${BLOCKMINE_PROGRAM_ID:-FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv}"
export BLOCKMINE_HEADLESS_AUTOSTART="${BLOCKMINE_HEADLESS_AUTOSTART:-0}"

LOG_DIR="${BLOCKMINE_LOG_DIR:-/workspace/blockmine-logs}"
LOG_FILE="${LOG_DIR}/blockmine-vast-worker.log"

mkdir -p "${BLOCKMINE_STORAGE_DIR}" "${LOG_DIR}"
env | grep _ >> /etc/environment || true

echo "[blockmine] preparing worker wallet"
blockmine-wallet ensure
"$(dirname "$0")/ensure-opencl-nvidia.sh"
"$(dirname "$0")/install-auto-console.sh" blockmine-vast-console
echo
echo "[blockmine] open a Jupyter or SSH terminal."
echo "[blockmine] the Blockmine console will launch automatically."
echo "[blockmine] if it does not, run: blockmine-vast-console"
echo "[blockmine] logs (headless mode only): ${LOG_FILE}"

if [ "${BLOCKMINE_HEADLESS_AUTOSTART}" = "1" ]; then
  nohup /opt/blockmine/scripts/start-miner.sh >>"${LOG_FILE}" 2>&1 &
fi

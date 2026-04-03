#!/usr/bin/env bash
set -euo pipefail

export BLOCKMINE_STORAGE_DIR="${BLOCKMINE_STORAGE_DIR:-/workspace/blockmine-data}"
export BLOCKMINE_SITE_URL="${BLOCKMINE_SITE_URL:-https://blockmine.dev}"
export BLOCKMINE_RPC_URL="${BLOCKMINE_RPC_URL:-https://api.mainnet-beta.solana.com}"
export BLOCKMINE_PROGRAM_ID="${BLOCKMINE_PROGRAM_ID:-FgRe73gAkZPhxpiCFHMYMfLR4dabDaB1FDVFazVTcCtv}"

LOG_DIR="${BLOCKMINE_LOG_DIR:-/workspace/blockmine-logs}"
LOG_FILE="${LOG_DIR}/blockmine-vast-worker.log"

mkdir -p "${BLOCKMINE_STORAGE_DIR}" "${LOG_DIR}"

echo "[blockmine] preparing worker wallet"
blockmine-wallet ensure
echo
echo "[blockmine] if this is the first boot, open a terminal and run:"
echo "[blockmine]   blockmine-wallet reveal"
echo "[blockmine] the worker will wait for backup confirmation and funding before mining."
echo "[blockmine] logs: ${LOG_FILE}"

nohup /opt/blockmine/scripts/start-miner.sh >>"${LOG_FILE}" 2>&1 &

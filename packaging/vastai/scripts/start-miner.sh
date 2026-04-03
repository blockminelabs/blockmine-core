#!/usr/bin/env bash
set -euo pipefail

export BLOCKMINE_STORAGE_DIR="${BLOCKMINE_STORAGE_DIR:-/workspace/blockmine-data}"
mkdir -p "${BLOCKMINE_STORAGE_DIR}"

exec /usr/local/bin/blockmine-vast-worker "$@"

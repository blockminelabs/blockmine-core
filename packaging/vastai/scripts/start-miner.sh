#!/usr/bin/env bash
set -euo pipefail

export BLOCKMINE_STORAGE_DIR="${BLOCKMINE_STORAGE_DIR:-/workspace/blockmine-data}"
mkdir -p "${BLOCKMINE_STORAGE_DIR}"

if [ -f /etc/profile.d/blockmine-vast.sh ]; then
  . /etc/profile.d/blockmine-vast.sh
fi
if [ -f /etc/profile.d/blockmine-opencl.sh ]; then
  . /etc/profile.d/blockmine-opencl.sh
fi

exec /usr/local/bin/blockmine-vast-worker "$@"

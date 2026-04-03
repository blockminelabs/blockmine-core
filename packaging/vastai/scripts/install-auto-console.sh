#!/usr/bin/env bash
set -euo pipefail

CONSOLE_COMMAND="${1:-blockmine-vast-console}"
BASHRC_TARGET="${HOME}/.bashrc"
MARKER_BEGIN="# >>> blockmine-vast-console >>>"
MARKER_END="# <<< blockmine-vast-console <<<"

mkdir -p "$(dirname "${BASHRC_TARGET}")"
touch "${BASHRC_TARGET}"

if grep -Fq "${MARKER_BEGIN}" "${BASHRC_TARGET}"; then
  exit 0
fi

cat >>"${BASHRC_TARGET}" <<EOF
${MARKER_BEGIN}
if [[ \$- == *i* ]] && [ -t 1 ] && [ -z "\${BLOCKMINE_VAST_CONSOLE_RUNNING:-}" ]; then
  export BLOCKMINE_VAST_CONSOLE_RUNNING=1
  if command -v ${CONSOLE_COMMAND} >/dev/null 2>&1; then
    ${CONSOLE_COMMAND} || true
    echo
    echo "[blockmine] Console closed. Run \`${CONSOLE_COMMAND}\` to reopen it."
  fi
fi
${MARKER_END}
EOF

#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALLER="$SCRIPT_DIR/../install_minotari_ledger.py"

if command -v python3 >/dev/null 2>&1; then
  exec python3 "$INSTALLER" --model nanox "$@"
fi

exec python "$INSTALLER" --model nanox "$@"

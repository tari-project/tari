#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# Minotari Ledger Wallet — Unified Installer (entry point)
#
# Usage:
#   chmod +x install_minotari_ledger.sh
#   ./install_minotari_ledger.sh [--tag TAG] [--device DEVICE]
#
# This wrapper bootstraps a Python venv and delegates to the main installer:
#   applications/minotari_ledger_wallet/wallet/install_minotari_ledger.py
# ──────────────────────────────────────────────────────────────────────────────

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$SCRIPT_DIR"
VENV_DIR="$PROJECT_DIR/.venv"

# ── Bootstrap venv ───────────────────────────────────────────────────────────

if [[ ! -d "$VENV_DIR" ]]; then
    echo "🐍 Creating Python virtual environment..."
    python3 -m venv "$VENV_DIR"
fi

# shellcheck disable=SC1091
source "$VENV_DIR/bin/activate"

# ── Run installer ────────────────────────────────────────────────────────────

exec python3 "$SCRIPT_DIR/install_minotari_ledger.py" "$@"

#!/usr/bin/env bash
# Unified Minotari Ledger Wallet Installer for macOS/Linux
# This script runs the cross-platform Python installer

set -euo pipefail

echo "Minotari Ledger Wallet Installer for macOS/Linux"

if ! command -v python3 >/dev/null 2>&1; then
    echo "Python 3 is not installed or not on PATH."
    echo "Install Python from https://www.python.org/downloads/ or use:"
    echo "  macOS: brew install python3"
    echo "  Linux: apt-get install python3 (Debian/Ubuntu) or equivalent"
    exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PYTHON_INSTALLER="$SCRIPT_DIR/install_minotari_ledger.py"

if [[ ! -f "$PYTHON_INSTALLER" ]]; then
    echo "install_minotari_ledger.py not found at $PYTHON_INSTALLER"
    exit 1
fi

echo "Using Python installer: $PYTHON_INSTALLER"

python3 "$PYTHON_INSTALLER"

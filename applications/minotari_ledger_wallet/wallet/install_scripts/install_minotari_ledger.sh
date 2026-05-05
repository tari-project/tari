#!/usr/bin/env bash
# install_minotari_ledger.sh — macOS/Linux launcher for the unified Minotari Ledger installer
# Usage: chmod +x install_minotari_ledger.sh && ./install_minotari_ledger.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "\U0001f527 Checking Python..."

if ! command -v python3 &>/dev/null; then
  echo "\u274c Python 3 is required. Install it from https://python.org or via your package manager."
  exit 1
fi

PYTHON_VER=$(python3 -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')")
echo "   Found Python $PYTHON_VER"

# macOS: install brew dependencies if needed
if [[ "$(uname)" == "Darwin" ]]; then
  if command -v brew &>/dev/null; then
    echo "\U0001f37a Installing system dependencies via Homebrew..."
    brew install libusb 2>/dev/null || true
  fi
fi

# Linux: install libusb if missing
if [[ "$(uname)" == "Linux" ]]; then
  if command -v apt-get &>/dev/null; then
    sudo apt-get install -y libhidapi-dev libusb-1.0-0-dev 2>/dev/null || true
  elif command -v dnf &>/dev/null; then
    sudo dnf install -y hidapi-devel libusb-devel 2>/dev/null || true
  fi
fi

# Create venv if it doesn't exist
VENV_DIR="$HOME/.minotari-ledger-installer"
if [[ ! -d "$VENV_DIR" ]]; then
  echo "\U0001f40d Creating Python virtual environment at $VENV_DIR..."
  python3 -m venv "$VENV_DIR"
fi

# Activate and run
# shellcheck disable=SC1091
source "$VENV_DIR/bin/activate"
python3 "$SCRIPT_DIR/install_minotari_ledger.py" "$@"

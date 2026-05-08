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

# Detect missing system libraries and *advise* the user instead of running
# `sudo apt-get install` blindly. Calling `sudo` from a launcher script is
# disruptive (prompts for a password mid-flow, fails outright in
# non-interactive contexts like CI) and `|| true` would mask real failures
# that surface later as cryptic Python import errors.

advise_install() {
  echo
  echo "  System library '$1' was not found."
  echo "  Please install it before continuing, e.g.:"
  echo "    $2"
  echo
}

if [[ "$(uname)" == "Darwin" ]]; then
  if ! pkg-config --exists libusb-1.0 2>/dev/null && ! [[ -f /opt/homebrew/lib/libusb-1.0.dylib ]] && ! [[ -f /usr/local/lib/libusb-1.0.dylib ]]; then
    advise_install "libusb" "brew install libusb"
  fi
fi

if [[ "$(uname)" == "Linux" ]]; then
  if ! pkg-config --exists libusb-1.0 2>/dev/null; then
    if command -v apt-get &>/dev/null; then
      advise_install "libusb / hidapi" "sudo apt-get install -y libhidapi-dev libusb-1.0-0-dev"
    elif command -v dnf &>/dev/null; then
      advise_install "libusb / hidapi" "sudo dnf install -y hidapi-devel libusb-devel"
    else
      advise_install "libusb / hidapi" "(use your distribution's package manager)"
    fi
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

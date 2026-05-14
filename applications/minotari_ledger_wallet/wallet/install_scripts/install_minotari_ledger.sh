#!/usr/bin/env bash
# install_minotari_ledger.sh — macOS/Linux launcher for the unified Minotari Ledger installer
# Usage: chmod +x install_minotari_ledger.sh && ./install_minotari_ledger.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "🔧 Checking Python..."

if ! command -v python3 &>/dev/null; then
  echo "❌ Python 3 is required. Install it from https://python.org or via your package manager."
  exit 1
fi

PYTHON_VER=$(python3 -c "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')")
echo "   Found Python $PYTHON_VER"

# Advise about missing system libraries instead of blindly running sudo.
# On Debian/Ubuntu, libusb and udev rules are needed for USB HID access.
if [[ "$(uname)" == "Linux" ]]; then
  if ! ldconfig -p 2>/dev/null | grep -q "libusb"; then
    echo ""
    echo "⚠️  libusb not found. You may need:"
    echo "   sudo apt-get install libusb-1.0-0"
    echo ""
  fi
  if [[ ! -f /etc/udev/rules.d/20-ledger.rules ]]; then
    echo "⚠️  Ledger udev rules not detected. See:"
    echo "   https://support.ledger.com/hc/en-us/articles/360018300334"
    echo ""
  fi
fi

# Run the Python installer
exec python3 "$SCRIPT_DIR/install_minotari_ledger.py" "$@"

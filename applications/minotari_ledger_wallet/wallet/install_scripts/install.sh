#!/bin/sh
set -e

# Tari Ledger Installer Wrapper
# This script ensures Astral's `uv` is installed, then runs the Python script
# in a completely isolated environment without needing system Python/pip configuration.

if ! command -v uv >/dev/null 2>&1; then
    echo "==> 'uv' not found. Installing Astral's uv for isolated execution..."
    curl -LsSf https://astral.sh/uv/install.sh | sh
    
    # Source the environment just in case it's not in PATH for this session
    if [ -f "$HOME/.cargo/env" ]; then
        . "$HOME/.cargo/env"
    fi
fi

# Run the unified python installer seamlessly
uv run "$(dirname "$0")/install_minotari_ledger.py"

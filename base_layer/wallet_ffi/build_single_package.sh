#!/bin/bash

# Build a single Python package that supports all networks via runtime configuration

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Set up target directory
TARGET_DIR="../../target/wheels"
mkdir -p "$TARGET_DIR"

echo "Building single tari_wallet package with all network support..."

# Build with mainnet as default, but include all network code
export TARI_TARGET_NETWORK=mainnet

# Clean previous builds
rm -rf target/wheels/*.whl || true

# Build the wheel
maturin build --release --out "$TARGET_DIR"

echo "✅ Built tari_wallet package successfully"
echo "📦 Wheel available in: $TARGET_DIR"

# List the generated wheel
ls -la "$TARGET_DIR"/tari_wallet-*.whl

echo ""
echo "To publish to PyPI:"
echo "1. Install twine: pip install twine"
echo "2. Test on TestPyPI: twine upload --repository testpypi $TARGET_DIR/tari_wallet-*.whl"
echo "3. Publish to PyPI: twine upload $TARGET_DIR/tari_wallet-*.whl"

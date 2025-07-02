#!/bin/bash

# Tari Wallet Python Bindings Build Script
# Builds Python wheels for mainnet, testnet, and nextnet

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WHEEL_DIR="../../target/wheels"

echo "Building Tari Wallet Python bindings for all networks..."

# Clean previous builds
echo "Cleaning previous builds..."
cargo clean
rm -rf "$WHEEL_DIR"
mkdir -p "$WHEEL_DIR"

# Function to build wheel for a specific network
build_network() {
    local network=$1
    local package_suffix=$2
    
    echo ""
    echo "========================================="
    echo "Building for $network network..."
    echo "========================================="
    
    # Set network environment variable
    export TARI_TARGET_NETWORK=$network
    
    # Build the wheel
    maturin build --features python-bindings --release
    
    # Find the generated wheel (maturin creates it in workspace target/wheels)
    local wheel_file=$(find ../../target/wheels -name "tari_wallet-*.whl" -type f 2>/dev/null | head -n 1)
    if [ -z "$wheel_file" ]; then
        # Try current directory target/wheels
        wheel_file=$(find target/wheels -name "tari_wallet-*.whl" -type f 2>/dev/null | head -n 1)
    fi
    if [ -z "$wheel_file" ]; then
        # Try searching recursively
        wheel_file=$(find ../.. -name "tari_wallet-*.whl" -type f 2>/dev/null | head -n 1)
    fi
    
    if [ -n "$wheel_file" ]; then
        local wheel_basename=$(basename "$wheel_file")
        local new_name="${wheel_basename/tari_wallet/tari_wallet_${package_suffix}}"
        local new_path="$WHEEL_DIR/$new_name"
        
        # Create wheel directory if it doesn't exist
        mkdir -p "$WHEEL_DIR"
        
        # Move and rename the wheel
        mv "$wheel_file" "$new_path"
        echo "✅ Built: $new_name"
    else
        echo "❌ Failed to find wheel file for $network"
        echo "Searched in: target/wheels and current directory"
        ls -la target/wheels/ 2>/dev/null || echo "target/wheels directory doesn't exist"
        exit 1
    fi
}

# Build for each network
build_network "mainnet" "mainnet"
build_network "testnet" "testnet" 
build_network "nextnet" "nextnet"

echo ""
echo "========================================="
echo "✅ All builds completed successfully!"
echo "========================================="
echo ""
echo "Generated wheels:"
if [ -d "$WHEEL_DIR" ]; then
    ls -la "$WHEEL_DIR"/tari_wallet_*.whl 2>/dev/null || echo "No network-specific wheels found in $WHEEL_DIR"
    ls -la "$WHEEL_DIR"/ 2>/dev/null
else
    echo "Wheel directory $WHEEL_DIR does not exist"
fi

echo ""
echo "Installation commands:"
echo "For mainnet:  pip install $WHEEL_DIR/tari_wallet_mainnet-*.whl"
echo "For testnet:  pip install $WHEEL_DIR/tari_wallet_testnet-*.whl" 
echo "For nextnet:  pip install $WHEEL_DIR/tari_wallet_nextnet-*.whl"
echo ""
echo "Note: Only install one network version at a time to avoid conflicts."

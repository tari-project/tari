#!/bin/bash

# Tari Wallet Python Bindings Build Script
# Builds Python wheels for mainnet, testnet, and nextnet

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WHEEL_DIR="../../target/wheels"

echo "Building Tari Wallet Python bindings for all networks..."

# Clean previous wheels
echo "Cleaning previous wheels..."
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
    
    # Backup original pyproject.toml
    cp pyproject.toml pyproject.toml.backup
    
    # Modify pyproject.toml to change package name (but keep module name as tari_wallet)
    sed -i.bak "s/name = \"tari-wallet\"/name = \"tari-wallet-${package_suffix}\"/" pyproject.toml
    
    # Build the wheel
    maturin build --features python-bindings --release
    
    # Restore original pyproject.toml
    mv pyproject.toml.backup pyproject.toml
    rm -f pyproject.toml.bak
    
    # Find the generated wheel (maturin creates it in workspace target/wheels)
    local wheel_file=$(find ../../target/wheels -name "tari_wallet_${package_suffix}-*.whl" -type f 2>/dev/null | head -n 1)
    if [ -z "$wheel_file" ]; then
        # The wheel will have the package name tari-wallet-${package_suffix} but filename uses underscores
        wheel_file=$(find ../../target/wheels -name "tari-wallet-${package_suffix}-*.whl" -type f 2>/dev/null | head -n 1)
    fi
    if [ -z "$wheel_file" ]; then
        # Try current directory target/wheels  
        wheel_file=$(find target/wheels -name "*${package_suffix}*.whl" -type f 2>/dev/null | head -n 1)
    fi
    if [ -z "$wheel_file" ]; then
        # Try searching recursively
        wheel_file=$(find ../.. -name "*${package_suffix}*.whl" -type f 2>/dev/null | head -n 1)
    fi
    
    if [ -n "$wheel_file" ]; then
        # Create wheel directory if it doesn't exist
        mkdir -p "$WHEEL_DIR"
        
        # Move wheel to final location (no renaming needed since pyproject.toml was modified)
        local wheel_basename=$(basename "$wheel_file")
        local final_path="$WHEEL_DIR/$wheel_basename"
        
        mv "$wheel_file" "$final_path"
        echo "✅ Built: $wheel_basename"
    else
        echo "❌ Failed to find wheel file for $network"
        echo "Searched in: ../../target/wheels, target/wheels and current directory"
        find ../.. -name "*${package_suffix}*.whl" -type f 2>/dev/null || echo "No ${package_suffix} wheels found anywhere"
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

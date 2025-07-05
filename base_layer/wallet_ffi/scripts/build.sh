#!/bin/bash
# Build script for Unix-like systems (Linux, macOS)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo_status() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

echo_warning() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

echo_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check prerequisites
echo_status "Checking prerequisites..."

# Check Rust
if ! command -v rustc &> /dev/null; then
    echo_error "Rust is not installed. Please install Rust 1.70+ from https://rustup.rs/"
    exit 1
fi

# Check Python
if ! command -v python3 &> /dev/null; then
    echo_error "Python 3 is not installed. Please install Python 3.8+ from https://python.org/"
    exit 1
fi

# Check maturin
if ! command -v maturin &> /dev/null; then
    echo_warning "maturin is not installed. Installing..."
    pip3 install maturin
fi

# Set environment variables
export TARI_TARGET_NETWORK="${TARI_TARGET_NETWORK:-nextnet}"
export RUSTFLAGS="${RUSTFLAGS:-} -D warnings"

# Build modes
BUILD_MODE="${1:-release}"
FEATURES="${2:-python-bindings}"

echo_status "Building with mode: $BUILD_MODE, features: $FEATURES"

# Clean previous builds if requested
if [ "$BUILD_MODE" = "clean" ]; then
    echo_status "Cleaning previous builds..."
    cargo clean
    rm -rf target/wheels
    exit 0
fi

# Build Rust library
echo_status "Building Rust library..."
if [ "$BUILD_MODE" = "debug" ]; then
    cargo build --features "$FEATURES"
else
    cargo build --release --features "$FEATURES"
fi

# Build Python wheel
echo_status "Building Python wheel..."
if [ "$BUILD_MODE" = "debug" ]; then
    maturin develop --features "$FEATURES"
else
    maturin develop --release --features "$FEATURES"
fi

echo_status "Build completed successfully!"

# Optional: Install development dependencies
if [ "$3" = "install-dev" ]; then
    echo_status "Installing development dependencies..."
    pip3 install -r requirements-dev.txt
fi

echo_status "You can now import tari_wallet in Python!"

# Building Tari Wallet Python Bindings

This guide covers building and installing the Tari Python wallet bindings from source.

## Prerequisites

### System Requirements

- **Python 3.8+** (3.9+ recommended)
- **Rust toolchain** (latest stable)
- **maturin** Python package for building Rust-Python extensions
- Compatible with Linux, macOS, and Windows

### Install Build Dependencies

```bash
# Install maturin
pip install maturin

# Verify Rust installation
rustc --version
cargo --version
```

## Quick Build (All Networks)

The simplest way to build Python wheels for all Tari networks:

```bash
# Navigate to the wallet FFI directory
cd base_layer/wallet_ffi

# Build all network variants
./python_build.sh
```

This script builds three separate Python wheels:
- `tari_wallet_mainnet-*.whl` - for Mainnet
- `tari_wallet_testnet-*.whl` - for Testnet (default)
- `tari_wallet_nextnet-*.whl` - for Nextnet

Wheels are created in `target/wheels/`

## Manual Build (Specific Networks)

To build for a specific network manually:

### Testnet (Default)
```bash
cd base_layer/wallet_ffi
TARI_TARGET_NETWORK=testnet maturin build --features python-bindings --release
```

### Mainnet
```bash
cd base_layer/wallet_ffi
TARI_TARGET_NETWORK=mainnet maturin build --features python-bindings --release
```

### Nextnet
```bash
cd base_layer/wallet_ffi
TARI_TARGET_NETWORK=nextnet maturin build --features python-bindings --release
```

### Development Build
```bash
cd base_layer/wallet_ffi
# Debug build (faster compilation, slower runtime)
maturin build --features python-bindings

# Development build with immediate installation
maturin develop --features python-bindings
```

## Installation

### Install Built Wheels

```bash
# Install the appropriate wheel for your target network
pip install target/wheels/tari_wallet_testnet-*.whl   # For testnet
pip install target/wheels/tari_wallet_mainnet-*.whl  # For mainnet  
pip install target/wheels/tari_wallet_nextnet-*.whl  # For nextnet
```

**Important:** Only install one network version at a time to avoid conflicts.

### Verify Installation

```python
import tari_wallet
print("Tari wallet bindings imported successfully!")

# Check available classes
print(dir(tari_wallet))
```

## Build Configuration

### Network Differences

Each network build is compiled with different network parameters at compile time:

- **Mainnet**: Production Tari network with real economic value
- **Testnet**: Development and testing network (default for development)
- **Nextnet**: Tari's testing network for upcoming features

The network configuration is baked into the compiled binary, which is why separate wheels are needed for each network.

### Build Features

The Python bindings are controlled by the `python-bindings` feature flag in [`Cargo.toml`](../../base_layer/wallet_ffi/Cargo.toml):

```toml
[features]
python-bindings = ["pyo3/extension-module"]
```

### Python Configuration

The Python package configuration is defined in [`pyproject.toml`](../../base_layer/wallet_ffi/pyproject.toml):

```toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "tari_wallet"
requires-python = ">=3.8"
```

## Development Workflow

### Development Build and Test

```bash
cd base_layer/wallet_ffi

# Install in development mode (changes to Python code reflect immediately)
maturin develop --features python-bindings

# Run tests
python -m pytest tests/ -v

# Run specific test
python -m pytest tests/test_wallet.py::TestPyTariWallet::test_wallet_creation -v
```

### Debugging Build Issues

#### Common Build Problems

1. **Missing Rust toolchain:**
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   source ~/.cargo/env
   ```

2. **maturin not found:**
   ```bash
   pip install --upgrade maturin
   ```

3. **Python version too old:**
   ```bash
   python --version  # Must be 3.8+
   ```

4. **Missing system dependencies (Linux):**
   ```bash
   # Ubuntu/Debian
   sudo apt-get update
   sudo apt-get install build-essential pkg-config libssl-dev

   # CentOS/RHEL
   sudo yum groupinstall "Development Tools"
   sudo yum install openssl-devel
   ```

#### Verbose Build Output

```bash
# Enable verbose build output for debugging
RUST_LOG=debug maturin build --features python-bindings --release -v
```

#### Clean Build

```bash
# Clean Rust build artifacts
cargo clean

# Clean Python build artifacts
rm -rf target/wheels/
rm -rf build/
```

## Cross-Compilation

### Linux to Windows

```bash
# Install Windows target
rustup target add x86_64-pc-windows-gnu

# Build for Windows
TARI_TARGET_NETWORK=testnet maturin build --features python-bindings --target x86_64-pc-windows-gnu --release
```

### macOS Universal Wheels

```bash
# Install targets for Apple Silicon and Intel
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin

# Build universal wheel
TARI_TARGET_NETWORK=testnet maturin build --features python-bindings --target universal2-apple-darwin --release
```

## Testing Builds

### Basic Functionality Test

```python
#!/usr/bin/env python3
import tempfile
import os
import tari_wallet

# Create temp directory
temp_dir = tempfile.mkdtemp()

try:
    # Test config creation
    config = tari_wallet.PyTariCommsConfig(
        public_address="/ip4/127.0.0.1/tcp/18188",
        database_name="test_wallet",
        datastore_path=temp_dir,
        discovery_timeout=30,
        exclude_dial_test_addresses=True
    )
    
    # Test wallet creation
    wallet = tari_wallet.PyTariWallet(
        config=config,
        log_path=os.path.join(temp_dir, "logs"),
        log_verbosity=0,  # Error level only
        num_rolling_log_files=1,
        size_per_log_file_bytes=64*1024,
        network_str="localnet"
    )
    
    print("✓ Build test passed - basic functionality works")
    
except Exception as e:
    print(f"✗ Build test failed: {e}")
    
finally:
    import shutil
    shutil.rmtree(temp_dir, ignore_errors=True)
```

### Run Test Suite

```bash
cd base_layer/wallet_ffi

# Run all tests
python -m pytest tests/ -v

# Run tests with coverage
pip install pytest-cov
python -m pytest tests/ --cov=tari_wallet --cov-report=html
```

## Performance Optimization

### Release Builds

Always use `--release` for production builds:

```bash
maturin build --features python-bindings --release
```

### Build Profiles

The build uses optimized profiles defined in `Cargo.toml`:

```toml
[profile.release]
opt-level = 3
lto = true
codegen-units = 1
```

### Binary Size Optimization

```bash
# Strip debug symbols for smaller binaries
strip target/wheels/tari_wallet-*.whl
```

## Troubleshooting

### Build Errors

1. **Linker errors:** Ensure system development tools are installed
2. **Python version mismatch:** Use the same Python version for build and install
3. **Feature conflicts:** Ensure only `python-bindings` feature is enabled

### Runtime Errors

1. **Import errors:** Check Python version compatibility
2. **Network errors:** Verify network configuration matches build
3. **Permission errors:** Ensure write access to log and data directories

### Getting Help

- Check the [main README](README.md) for general usage
- Review [test files](../../base_layer/wallet_ffi/tests/) for working examples
- See [API documentation](api.md) for detailed interface information

## Contributing

### Build Standards

- Always test on multiple platforms before submitting
- Include tests for new functionality
- Update documentation for API changes
- Follow Rust and Python style guidelines

### Testing Checklist

- [ ] Build succeeds on Linux, macOS, Windows
- [ ] All existing tests pass
- [ ] New functionality has tests
- [ ] Documentation is updated
- [ ] Example code works with changes

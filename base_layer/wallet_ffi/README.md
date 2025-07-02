# Tari Wallet FFI

Foreign Function Interface (FFI) for the Tari cryptocurrency wallet, providing C-compatible bindings and Python bindings.

## Python Bindings

The Python bindings have been moved to the main documentation directory for better organization:

**📍 [Python Bindings Documentation](../../docs/python_bindings/)**

- [Getting Started](../../docs/python_bindings/README.md) - Installation and basic usage
- [API Reference](../../docs/python_bindings/api.md) - Complete API documentation  
- [Build Instructions](../../docs/python_bindings/build.md) - Building from source
- [Examples](../../docs/python_bindings/examples.md) - Real-world usage examples
- [Migration Guide](../../docs/python_bindings/migration.md) - Upgrading from older versions

## Quick Start (Python)

```bash
# Build Python bindings
cd base_layer/wallet_ffi
./python_build.sh

# Install for testnet (default)
pip install target/wheels/tari_wallet_testnet-*.whl

# See docs/python_bindings/ for complete documentation
```

## C FFI

The C FFI provides low-level access to wallet functionality for integration with other languages and systems.

### Header File

The C interface is defined in [`wallet.h`](wallet.h).

### Building C FFI

```bash
cd base_layer/wallet_ffi
cargo build --release --features ffi
```

The compiled library will be available in `target/release/`.

## Directory Structure

```
base_layer/wallet_ffi/
├── src/
│   ├── lib.rs              # Main FFI implementation
│   └── python_bindings.rs  # Python-specific bindings
├── tests/                  # Python test suite
├── examples/               # Python example scripts
├── wallet.h               # C header file
├── pyproject.toml         # Python package configuration
├── python_build.sh        # Python build script
└── README.md              # This file
```

## Features

- **python-bindings**: Enables Python bindings via PyO3
- **ffi**: Enables C FFI interface

## Network Support

Both C and Python bindings support multiple Tari networks:

- **localnet**: Local development network
- **nextnet**: Tari test network for upcoming features  
- **mainnet**: Tari production network

Network configuration is determined at compile time using the `TARI_TARGET_NETWORK` environment variable.

## Documentation

- **Python**: See [docs/python_bindings/](../../docs/python_bindings/)
- **C FFI**: See [`wallet.h`](wallet.h) for interface definition
- **Examples**: See [`examples/`](examples/) directory

## Testing

```bash
# Python tests
python -m pytest tests/ -v

# C FFI tests  
cargo test --features ffi
```

## License

Licensed under the same license as the Tari project.

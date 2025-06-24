# Tari Lightweight Wallet Libraries

[![Crates.io](https://img.shields.io/crates/v/lightweight_wallet_libs.svg)](https://crates.io/crates/lightweight_wallet_libs)
[![Documentation](https://docs.rs/lightweight_wallet_libs/badge.svg)](https://docs.rs/lightweight_wallet_libs)
[![License](https://img.shields.io/badge/license-BSD--3--Clause-blue.svg)](LICENSE)

A standalone, dependency-free implementation of core Tari wallet functionality designed for lightweight applications, mobile wallets, web applications, and embedded systems.

## 🚀 **What is this?**

The Tari Lightweight Wallet Libraries provide essential wallet functionality extracted from the main Tari codebase, designed to be:

- **🪶 Lightweight**: Minimal dependencies, optimized for resource-constrained environments
- **🌐 Cross-platform**: Native Rust, WASM, mobile, and web compatibility
- **🔒 Secure**: Industry-standard cryptography with secure memory handling
- **🔧 Modular**: Use only the components you need
- **✅ Compatible**: 100% compatible with main Tari wallet key derivation and address generation

## 🎯 **Key Features**

### 💼 **Wallet Operations**
- ✅ Create wallets from seed phrases (24-word Tari CipherSeed format)
- ✅ Generate new wallets with cryptographically secure entropy
- ✅ Master key derivation following Tari specification
- ✅ Wallet metadata management and secure storage

### 🔑 **Key Management**
- ✅ BIP39-like mnemonic generation and validation (Tari format)
- ✅ Hierarchical deterministic key derivation
- ✅ View and spend key generation
- ✅ Stealth address support
- ✅ Secure key zeroization and memory protection

### 🏠 **Address Generation**
- ✅ Dual addresses (view + spend keys) for advanced features
- ✅ Single addresses (spend key only) for simplified use
- ✅ Multiple formats: Emoji 🦀, Base58, and Hex
- ✅ Payment ID embedding and extraction
- ✅ Network support (MainNet, StageNet, TestNet)

### 🔍 **Blockchain Scanning**
- ✅ GRPC-based blockchain scanning
- ✅ UTXO discovery and validation
- ✅ Progress tracking and resumable scans
- ✅ Batch processing for performance
- ✅ Wallet output reconstruction from blockchain data

### ✅ **Cryptographic Validation**
- ✅ Range proof validation (BulletProof+, RevealedValue)
- ✅ Signature verification (metadata, script signatures)
- ✅ Commitment validation and integrity checks
- ✅ Encrypted data decryption and validation

## 📦 **Installation**

Add to your `Cargo.toml`:

```toml
[dependencies]
lightweight_wallet_libs = "0.1"

# Optional features
lightweight_wallet_libs = { version = "0.1", features = ["wasm", "grpc"] }
```

### Feature Flags

- `default`: Core wallet functionality
- `wasm`: WASM compatibility and JavaScript bindings
- `grpc`: GRPC blockchain scanning support
- `parallel`: Parallel processing optimizations

## 🏗️ **Quick Start**

### Create a New Wallet

```rust
use lightweight_wallet_libs::wallet::Wallet;
use lightweight_wallet_libs::data_structures::address::TariAddressFeatures;

// Generate a new wallet with a 24-word seed phrase
let wallet = Wallet::generate_new_with_seed_phrase(None)?;

// Export the seed phrase for backup
let seed_phrase = wallet.export_seed_phrase()?;
println!("Backup this seed phrase: {}", seed_phrase);

// Generate a Tari address
let features = TariAddressFeatures::create_interactive_and_one_sided();
let address = wallet.get_dual_address(features, None)?;

println!("Your Tari address: {}", address.to_emoji_string());
```

### Restore Wallet from Seed Phrase

```rust
use lightweight_wallet_libs::wallet::Wallet;

// Restore wallet from existing seed phrase
let seed_phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art";
let wallet = Wallet::new_from_seed_phrase(seed_phrase, None)?;

// Generate the same address as before
let address = wallet.get_dual_address(
    TariAddressFeatures::create_interactive_and_one_sided(),
    None
)?;
```

### Key Management

```rust
use lightweight_wallet_libs::key_management::{
    generate_seed_phrase,
    validate_seed_phrase,
    derive_view_and_spend_keys_from_entropy,
};

// Generate a new 24-word seed phrase
let seed_phrase = generate_seed_phrase()?;

// Validate an existing seed phrase
validate_seed_phrase(&seed_phrase)?;

// Derive keys from entropy
let entropy = [42u8; 16]; // Your entropy source
let (view_key, spend_key) = derive_view_and_spend_keys_from_entropy(&entropy)?;
```

### Blockchain Scanning

```rust
use lightweight_wallet_libs::scanning::{GrpcBlockchainScanner, WalletScanConfig};

// Connect to a Tari base node
let mut scanner = GrpcBlockchainScanner::new("http://127.0.0.1:18142".to_string()).await?;

// Configure wallet scanning
let wallet_birthday = 950; // Block height when wallet was created
let scan_config = WalletScanConfig::new(wallet_birthday)
    .with_stealth_address_scanning(true)
    .with_max_addresses_per_account(100);

// Scan for wallet outputs
let results = scanner.scan_wallet(scan_config).await?;
println!("Found {} wallet outputs", results.total_wallet_outputs);
```

## 🏛️ **Architecture**

```
lightweight_wallet_libs/
├── wallet/           # Core wallet operations
├── key_management/   # Key derivation and mnemonics  
├── data_structures/  # Wallet data types
├── validation/       # Cryptographic validation
├── extraction/       # UTXO processing
├── scanning/         # Blockchain scanning
├── crypto/           # Independent crypto primitives
└── errors/           # Comprehensive error handling
```

### Core Components

- **`Wallet`**: Main wallet struct for key management and address generation
- **`CipherSeed`**: Tari's encrypted seed format with birthday tracking
- **`TariAddress`**: Dual and single address types with multiple encoding formats
- **`BlockchainScanner`**: GRPC-based scanning for wallet output discovery
- **`ValidationEngine`**: Cryptographic proof and signature validation

## 🌐 **Cross-Platform Support**

### Native Rust
```rust
// Standard Rust usage
let wallet = Wallet::generate_new_with_seed_phrase(None)?;
```

### WASM (Web Assembly)
```rust
// WASM-compatible with feature flag
#[cfg(feature = "wasm")]
use lightweight_wallet_libs::wasm::*;
```

### Mobile Development
- Android: Use via JNI bindings
- iOS: Use via C FFI or Swift Package Manager
- React Native: Use via WASM bindings

## 🧪 **Examples**

Check out the [`examples/`](examples/) directory for complete working examples:

- [`wallet_example.rs`](examples/wallet_example.rs) - Comprehensive wallet operations
- [`grpc_scanner_example.rs`](examples/grpc_scanner_example.rs) - Blockchain scanning demo

Run examples:
```bash
# Basic wallet operations
cargo run --example wallet_example

# GRPC scanning (requires running Tari base node)
cargo run --example grpc_scanner_example --features grpc
```

## 🔒 **Security Features**

- **Secure Memory**: Automatic zeroization of sensitive data
- **Constant-time Operations**: Timing attack resistant comparisons
- **Domain Separation**: Cryptographic domain separation for security
- **Memory Safety**: Rust's memory safety guarantees
- **Secure Randomness**: Cryptographically secure random number generation

## ⚡ **Performance**

- **Batch Operations**: Optimized for processing multiple UTXOs
- **Parallel Processing**: Optional parallel validation (with `parallel` feature)
- **Memory Efficient**: Minimal memory footprint for mobile/embedded use
- **Fast Scanning**: Efficient blockchain scanning with progress tracking

## 🧰 **Use Cases**

### ✅ **Perfect For**
- 📱 Mobile wallet applications
- 🌐 Web wallets and browser extensions
- 🔧 Hardware wallet firmware
- 📡 Lightweight desktop applications
- 🚀 DeFi integrations requiring Tari addresses
- 🔍 Blockchain analysis tools

### ❌ **Not Suitable For**
- ⛏️ Running Tari base nodes
- 🏭 Mining operations
- 🌐 Peer-to-peer networking
- 💾 Full blockchain storage
- 🏛️ Consensus mechanisms

## 🆚 **vs. Main Tari Project**

| Feature | Main Tari | Lightweight Libs |
|---------|-----------|------------------|
| **Purpose** | Full blockchain protocol | Wallet functionality only |
| **Dependencies** | Heavy (tari-* crates) | Minimal (crypto only) |
| **Size** | ~100MB+ | ~5MB |
| **Platforms** | Desktop/Server | All platforms + WASM |
| **Use Case** | Run nodes/miners | Build wallet apps |

## 🤝 **Contributing**

We welcome contributions! Please see our [Contributing Guidelines](CONTRIBUTING.md) for details.

### Development Setup

```bash
# Clone the repository
git clone https://github.com/tari-project/lightweight_wallet_libs.git
cd lightweight_wallet_libs

# Run tests
cargo test

# Run with all features
cargo test --all-features

# Check WASM compatibility
cargo check --target wasm32-unknown-unknown --features wasm
```

### Testing

```bash
# Unit tests
cargo test

# Integration tests with GRPC (requires base node)
cargo test --features grpc

# WASM tests
wasm-pack test --node --features wasm
```

## 📋 **Compatibility**

- **Rust**: 1.70.0 or later
- **WASM**: All major browsers
- **Mobile**: iOS 12+, Android API 21+
- **Tari**: Compatible with main Tari wallet key derivation

## 📄 **License**

This project is licensed under the [BSD 3-Clause License](LICENSE).

## 🆘 **Support**

- 📖 [Documentation](https://docs.rs/lightweight_wallet_libs)
- 💬 [Tari Discord](https://discord.gg/tari)
- 🐛 [GitHub Issues](https://github.com/tari-project/lightweight_wallet_libs/issues)
- 📧 [Tari Community](https://tari.com/community)

## 🎯 **Roadmap**

- [ ] Hardware wallet integration (Ledger, Trezor)
- [ ] Additional language bindings (Python, JavaScript)
- [ ] Advanced stealth address features
- [ ] Performance optimizations for mobile
- [ ] Enhanced error recovery mechanisms

---

**Made with ❤️ by the Tari Community**

*Building the future of digital assets, one lightweight library at a time.*

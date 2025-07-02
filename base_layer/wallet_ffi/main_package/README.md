# Tari Wallet Python Bindings

Python bindings for the Tari cryptocurrency wallet.

## Installation

### Default (Mainnet)
```bash
pip install tari-wallet
```

### Specific Networks
```bash
# Testnet
pip install tari-wallet-testnet

# Nextnet  
pip install tari-wallet-nextnet
```

## Usage

All network packages provide the same `tari_wallet` module:

```python
import tari_wallet

# Use the wallet functions
# (Network was determined at install time)
```

## Network Selection

- **Mainnet**: `pip install tari-wallet` (default)
- **Testnet**: `pip install tari-wallet-testnet` 
- **Nextnet**: `pip install tari-wallet-nextnet`

Only one network package can be installed at a time, as they all provide the same `tari_wallet` module.

## Development

This package is a convenience wrapper that automatically installs the mainnet version. The actual wallet implementations are in the network-specific packages.

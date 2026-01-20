# Minotari Offline Signer

A standalone binary for signing one-sided Tari transactions offline. This tool allows you to sign transactions without requiring a full wallet connection, using private spend and view keys that are securely stored in the OS keystore.

## Overview

The offline signing workflow consists of four steps:

1. **Initialize**: Set up the offline signer with your spend and view keys, protected by a passphrase. Keys are encrypted and stored in the OS keystore.

2. **Prepare**: Use the `minotari_console_wallet` to prepare a one-sided transaction for signing with the `PrepareOneSidedTransactionForSigning` command. This generates a JSON file containing the unsigned transaction data.

3. **Sign**: Use this `minotari_offline_signer` tool to sign the transaction on an air-gapped or secure machine.

4. **Broadcast**: Use the `minotari_console_wallet` to broadcast the signed transaction with the `BroadcastSignedOneSidedTransaction` command.

## Usage

### Initialize the Signer

First, initialize the signer with your keys. The passphrase will be prompted interactively if not provided:

```bash
minotari_offline_signer init \
    --spend-key <PRIVATE_SPEND_KEY_HEX> \
    --view-key <PRIVATE_VIEW_KEY_HEX>
```

Or with a passphrase via environment variable:

```bash
export TARI_PASSPHRASE=<your_passphrase>
minotari_offline_signer init \
    --spend-key <PRIVATE_SPEND_KEY_HEX> \
    --view-key <PRIVATE_VIEW_KEY_HEX>
```

### Check Status

Check if the signer has been initialized:

```bash
minotari_offline_signer status
```

### Sign a Transaction

Sign a prepared transaction (passphrase will be prompted if not provided):

```bash
minotari_offline_signer sign \
    --input-file unsigned_transaction.json \
    --output-file signed_transaction.json \
    --network mainnet
```

### Clear Stored Keys

Remove all keys from the keystore:

```bash
minotari_offline_signer clear
```

## Commands

| Command  | Description |
|----------|-------------|
| `init`   | Initialize the signer with spend and view keys |
| `sign`   | Sign a one-sided transaction using stored keys |
| `status` | Check if the signer has been initialized |
| `clear`  | Clear all stored keys from the keystore |

## Arguments

### `init` Command

- `--spend-key`: Private spend key in hexadecimal format (can also be set via `TARI_SPEND_KEY` env var)
- `--view-key`: Private view key in hexadecimal format (can also be set via `TARI_VIEW_KEY` env var)
- `--passphrase`: Passphrase to encrypt the keys (will prompt if not provided, can also be set via `TARI_PASSPHRASE` env var)

### `sign` Command

- `--input-file, -i`: Path to the JSON file containing the prepared unsigned transaction
- `--output-file, -o`: Path where the signed transaction will be written
- `--passphrase`: Passphrase to decrypt the keys (will prompt if not provided, can also be set via `TARI_PASSPHRASE` env var)
- `--network, -n`: Network to use (mainnet, nextnet, esmeralda, localnet). Default: mainnet

## Security

### Key Storage

Keys are stored securely using:
- **macOS**: Keychain
- **Windows**: Credential Manager
- **Linux**: Secret Service (via D-Bus)

### Encryption

Before storing in the keystore, keys are encrypted using:
- **Key Derivation**: Argon2id (from passphrase)
- **Encryption**: ChaCha20-Poly1305

This provides defense-in-depth: even if the OS keystore is compromised, the keys remain encrypted with your passphrase.

### Security Recommendations

- **Air-gapped signing**: This tool is designed to be used on an air-gapped machine that never connects to the internet. Transfer the unsigned transaction file via USB or QR code, sign it, then transfer the signed transaction back.

- **Passphrase protection**: Use a strong, unique passphrase. The passphrase is never stored - only a key derived from it is used for encryption.

- **Key protection**: When initializing, avoid passing keys via command line arguments as they may be visible in shell history. Use environment variables instead.

- **Verify before signing**: Always verify the transaction details before signing, especially the recipient addresses and amounts.

## Building

```bash
cargo build --release -p minotari_offline_signer
```

The binary will be available at `target/release/minotari_offline_signer`.

## License

BSD-3-Clause


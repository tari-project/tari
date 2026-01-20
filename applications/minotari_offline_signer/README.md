# Minotari Offline Signer

A standalone binary for signing one-sided Tari transactions offline. This tool allows you to sign transactions without requiring a full wallet connection, using only private spend and view keys.

## Overview

The offline signing workflow consists of three steps:

1. **Prepare**: Use the `minotari_console_wallet` to prepare a one-sided transaction for signing with the `PrepareOneSidedTransactionForSigning` command. This generates a JSON file containing the unsigned transaction data.

2. **Sign**: Use this `minotari_offline_signer` tool to sign the transaction on an air-gapped or secure machine using your private keys.

3. **Broadcast**: Use the `minotari_console_wallet` to broadcast the signed transaction with the `BroadcastSignedOneSidedTransaction` command.

## Usage

### Sign a Transaction

```bash
minotari_offline_signer sign \
    --input-file unsigned_transaction.json \
    --output-file signed_transaction.json \
    --spend-key <PRIVATE_SPEND_KEY_HEX> \
    --view-key <PRIVATE_VIEW_KEY_HEX> \
    --network mainnet
```

### Arguments

- `--input-file, -i`: Path to the JSON file containing the prepared unsigned transaction
- `--output-file, -o`: Path where the signed transaction will be written
- `--spend-key`: Private spend key in hexadecimal format (can also be set via `TARI_SPEND_KEY` environment variable)
- `--view-key`: Private view key in hexadecimal format (can also be set via `TARI_VIEW_KEY` environment variable)  
- `--network, -n`: Network to use (mainnet, nextnet, esmeralda, localnet). Default: mainnet

### Using Environment Variables

For added security, you can provide the keys via environment variables instead of command line arguments:

```bash
export TARI_SPEND_KEY=<your_spend_key_hex>
export TARI_VIEW_KEY=<your_view_key_hex>

minotari_offline_signer sign \
    --input-file unsigned_transaction.json \
    --output-file signed_transaction.json \
    --network mainnet
```

## Security Considerations

- **Air-gapped signing**: This tool is designed to be used on an air-gapped machine that never connects to the internet. Transfer the unsigned transaction file via USB or QR code, sign it, then transfer the signed transaction back.

- **Key protection**: Never expose your private keys in shell history. Use environment variables or secure key management practices.

- **Verify before signing**: Always verify the transaction details before signing, especially the recipient addresses and amounts.

## Building

```bash
cargo build --release -p minotari_offline_signer
```

The binary will be available at `target/release/minotari_offline_signer`.

## License

BSD-3-Clause


# Minotari Offline Signer

A standalone binary for signing one-sided Tari transactions offline. This tool allows you to sign transactions without requiring a full wallet connection, using private spend and view keys that are securely stored in the OS keystore.

## Overview

The offline signing workflow consists of four steps:

1. **Initialize**: Set up the offline signer with your keys (either directly or from seed words), protected by a passphrase. Keys are encrypted and stored in the OS keystore.

2. **Prepare**: Use the `minotari_console_wallet` to prepare a one-sided transaction for signing with the `PrepareOneSidedTransactionForSigning` command. This generates a JSON file containing the unsigned transaction data.

3. **Sign**: Use this `minotari_offline_signer` tool to sign the transaction on an air-gapped or secure machine.

4. **Broadcast**: Use the `minotari_console_wallet` to broadcast the signed transaction with the `BroadcastSignedOneSidedTransaction` command.

## Usage

### Initialize the Signer

You can initialize the signer using either direct keys or seed words (mnemonic phrase).

#### Option 1: Initialize with Keys

```bash
minotari_offline_signer init keys \
    --spend-key <PRIVATE_SPEND_KEY_HEX> \
    --view-key <PRIVATE_VIEW_KEY_HEX>
```

#### Option 2: Initialize with Seed Words

```bash
# With seed words provided as argument
minotari_offline_signer init seed-words \
    --seed-words "word1 word2 word3 ... word24"

# Or interactively (seed words will be prompted securely)
minotari_offline_signer init seed-words
```

If you have a BIP39 passphrase for your seed words:

```bash
minotari_offline_signer init seed-words \
    --seed-words "word1 word2 word3 ... word24" \
    --seed-passphrase "your_bip39_passphrase"
```

The encryption passphrase (for storing keys in the keystore) will be prompted interactively if not provided via `--passphrase` or `TARI_PASSPHRASE` environment variable.

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

Before any key is used, the signer prints a summary of the transaction: each recipient's amount, address and output
features, any other outputs, the fee, total spend and change, then any memo text (with control characters shown
escaped), and finally a recap. When run from a terminal it then asks `Sign this transaction? [y/N]` and signs only on
`y` or `yes`. When stdin is not a terminal it prints the summary and signs without a prompt.

The signer only signs transactions that leave a change output. A payload where what is left after the outputs and
the fee is no more than a change output would cost, such as a send-all or sweep, is refused, because with no change
output the script offset of the signed transaction, together
with what the online (view key) wallet already knows, would let that wallet work out the spend key. Prepare it again
with a slightly smaller amount. This closes that one known way of recovering the spend key; it does not make it safe
to treat the online wallet as untrusted in general, because a view key holder can produce a payload that passes
verification. Check every recipient, amount and "Other outputs" entry, and the fee, total spend and change, in the
summary before approving.

### Payload Keys and Strict Mode

The online wallet signs each payload it prepares so the signer can tell whether it was changed on the way. By default
it signs with the wallet's view key, which is also held by view-only wallets, auditors and explorers, so anyone with
the view key could produce a payload the signer accepts. A payload key closes that gap: it is derived from the spend
key on the signer, so only the signer can create it, and only the wallet it is given to can sign with it.

Nothing changes unless you set it up; payloads signed with the view key, and older version 5 payloads, keep working.
Upgrade the signer before the online wallet: an older signer cannot read the version 6 payloads a new online wallet
prepares, while a new signer signs version 5 payloads and returns the result as version 5, which an older online
wallet can broadcast.
To set it up:

1. On the signer, export payload key 1 (any index works; a key is retired by no longer accepting its index):

   ```bash
   minotari_offline_signer export-payload-key --index 1 --output payload_key.json
   ```

   The file holds the private payload key and is created readable by the owner only. Without `--output` the key is
   printed. Move it to the online wallet the same way you move payloads, and delete it once imported.

2. On the online (view) wallet, import it:

   ```bash
   minotari_console_wallet import-payload-key --index 1 --file payload_key.json
   ```

   or `import-payload-key --index 1 --key <hex>` (the key is then visible in shell history and `ps`, so prefer
   `--file`). The key is stored in the wallet's database, encrypted with the wallet passphrase. Importing again
   replaces it; `clear-payload-key` removes it. From then on every prepared payload is signed with it.

   The signer holds the spend key, so it derives the payload key itself and needs no import. A wallet that prepares
   payloads needs the import to know which index to sign with: a view wallet signs with the imported key, while a
   wallet holding the spend key ignores the imported key and signs with its own derived key for that index.

3. On the signer, require it:

   ```bash
   minotari_offline_signer sign --require-payload-key --payload-key-index 1 \
       --input-file unsigned_transaction.json --output-file signed_transaction.json
   ```

   With `--require-payload-key` the signer refuses version 5 payloads, payloads signed with the view key, and payloads
   signed with a payload key whose index is not given with `--payload-key-index` (repeat it to accept more than one).
   With no index given it refuses everything. A console wallet that signs offline payloads does the same with
   `require_payload_key = true` and `accepted_payload_key_indices = [1]` in its `[wallet.transactions]` config.

Before showing the summary the signer checks the payload's expiry, checks its signing key against these settings,
and verifies its signature, so a payload it would refuse is never shown for approval. The summary then says which key
signed the payload: `payload key #<index>` or `view key (shared key)`.

### Payload Expiry

A payload prepared by this version expires 24 hours after it was prepared, or after `--payload-expiry-hours <n>` on
the console wallet's `prepare-*` commands (`payload_expiry_hours` over gRPC). The signer refuses an expired payload
before showing its summary, and the summary shows the expiry in UTC and the time left. The signer's own clock is used,
so keep it roughly right. Version 5 payloads have no expiry.

### Clear Stored Keys

Remove all keys from the keystore:

```bash
minotari_offline_signer clear
```

## Commands

| Command              | Description |
|----------------------|-------------|
| `init keys`          | Initialize the signer with spend and view keys directly |
| `init seed-words`    | Initialize the signer with seed words (mnemonic phrase) |
| `sign`               | Sign a one-sided transaction using stored keys |
| `export-payload-key` | Export a payload key for the online wallet |
| `status`             | Check if the signer has been initialized |
| `clear`              | Clear all stored keys from the keystore |

## Arguments

### `init keys` Command

- `--spend-key`: Private spend key in hexadecimal format (can also be set via `TARI_SPEND_KEY` env var)
- `--view-key`: Private view key in hexadecimal format (can also be set via `TARI_VIEW_KEY` env var)
- `--passphrase`: Passphrase to encrypt the keys (will prompt if not provided, can also be set via `TARI_PASSPHRASE` env var)

### `init seed-words` Command

- `--seed-words`: Seed words (mnemonic phrase) separated by spaces (will prompt if not provided, can also be set via `TARI_SEED_WORDS` env var)
- `--seed-passphrase`: Optional BIP39 passphrase for the seed words (can also be set via `TARI_SEED_PASSPHRASE` env var)
- `--passphrase`: Passphrase to encrypt the keys in the keystore (will prompt if not provided, can also be set via `TARI_PASSPHRASE` env var)

### `sign` Command

- `--input-file, -i`: Path to the JSON file containing the prepared unsigned transaction
- `--output-file, -o`: Path where the signed transaction will be written
- `--passphrase`: Passphrase to decrypt the keys (will prompt if not provided, can also be set via `TARI_PASSPHRASE` env var)
- `--network, -n`: Network to use (mainnet, nextnet, esmeralda, localnet). Default: mainnet
- `--require-payload-key`: Refuse any payload not signed with an accepted payload key
- `--payload-key-index`: A payload key index to accept with `--require-payload-key`; repeat for more

### `export-payload-key` Command

- `--index`: The payload key index
- `--output, -o`: Write the key to this new file (owner-only) instead of stdout
- `--passphrase`: Passphrase to decrypt the keys (can also be set via `TARI_PASSPHRASE` env var)

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

- **Key protection**: When initializing, avoid passing keys or seed words via command line arguments as they may be visible in shell history. Use the interactive prompt or environment variables instead.

- **Verify before signing**: Always verify the transaction details before signing: every recipient address, every amount, every "Other outputs" entry, and the fee, total spend and change (a payload can keep a dust change while sending the rest to the miner as fee). Unless the signer requires a payload key, anyone holding the view key can produce a payload that passes the integrity check, and the change-output rule above only closes one known way of recovering the spend key, so the summary is the check that matters.

## Building

```bash
cargo build --release -p minotari_offline_signer
```

The binary will be available at `target/release/minotari_offline_signer`.

## License

BSD-3-Clause


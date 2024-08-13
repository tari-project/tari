// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use tari_crypto::hash_domain;

// These are the hash domains that are also used in tari-dan.

hash_domain!(ConfidentialOutputHashDomain, "com.tari.dan.confidential_output", 1);
hash_domain!(TariEngineHashDomain, "com.tari.dan.engine", 0);

// Hash domain used to derive the final AEAD encryption key for encrypted data in UTXOs
hash_domain!(
    TransactionSecureNonceKdfDomain,
    "com.tari.base_layer.core.transactions.secure_nonce_kdf",
    0
);
hash_domain!(
    ValidatorNodeBmtHashDomain,
    "com.tari.base_layer.core.validator_node_mmr",
    1
);
hash_domain!(
    WalletOutputEncryptionKeysDomain,
    "com.tari.base_layer.wallet.output_encryption_keys",
    1
);

// Hash domain for all transaction-related hashes, including the script signature challenge, transaction hash and kernel
// signature challenge
hash_domain!(TransactionHashDomain, "com.tari.base_layer.core.transactions", 0);

hash_domain!(LedgerHashDomain, "com.tari.minotari_ledger_wallet", 0);

hash_domain!(
    KeyManagerTransactionsHashDomain,
    "com.tari.base_layer.core.transactions.key_manager",
    1
);

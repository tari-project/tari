// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod aggregated_body;

mod crypto_factories;

pub use crypto_factories::CryptoFactories;
use tari_crypto::hash_domain;

mod coinbase_builder;
pub use coinbase_builder::{
    generate_coinbase,
    generate_coinbase_with_wallet_output,
    CoinbaseBuildError,
    CoinbaseBuilder,
};

pub mod fee;
pub mod tari_amount;
pub mod transaction_components;

mod format_currency;
pub use format_currency::format_currency;

pub mod transaction_protocol;
pub use transaction_protocol::{recipient::ReceiverTransactionProtocol, sender::SenderTransactionProtocol};

pub mod weight;

pub mod key_manager;

#[macro_use]
#[cfg(feature = "base_node")]
pub mod test_helpers;

// Hash domain for all transaction-related hashes, including the script signature challenge, transaction hash and kernel
// signature challenge
hash_domain!(TransactionHashDomain, "com.tari.base_layer.core.transactions", 0);

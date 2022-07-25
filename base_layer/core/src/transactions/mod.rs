// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod aggregated_body;

mod crypto_factories;
pub use crypto_factories::CryptoFactories;

mod coinbase_builder;
pub use coinbase_builder::{CoinbaseBuildError, CoinbaseBuilder};

pub mod fee;
pub mod tari_amount;
pub mod transaction_components;

mod format_currency;
pub use format_currency::format_currency;
use tari_common::hashing_domain::HashingDomain;

pub mod transaction_protocol;
pub use transaction_protocol::{recipient::ReceiverTransactionProtocol, sender::SenderTransactionProtocol};

pub mod types;
pub mod weight;

#[macro_use]
pub mod test_helpers;

/// The base layer core transactions domain separated hashing domain
/// Usage:
///   let hash = core_transactions_hash_domain().digest::<Blake256>(b"my secret");
///   etc.
pub fn core_transactions_hash_domain() -> HashingDomain {
    HashingDomain::new("base_layer.core.transactions")
}

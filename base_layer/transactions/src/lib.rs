// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod aggregated_body;

mod crypto_factories;

pub use crypto_factories::CryptoFactories;
use tari_crypto::hash_domain;

mod coinbase_builder;
pub use coinbase_builder::{CoinbaseBuildError, CoinbaseBuilder};

pub mod fee;
pub mod tari_amount;
pub mod transaction_components;
pub mod transaction_protocol;

mod format_currency;
pub use format_currency::format_currency;

pub mod types;
pub mod weight;

#[macro_use]
pub mod test_helpers;

#[macro_use]
pub mod covenants;

#[macro_use]
extern crate bitflags;

hash_domain!(TransactionHashDomain, "com.tari.base_layer.core.transactions", 0);
hash_domain!(TransactionKdfDomain, "com.tari.base_layer.core.transactions.kdf", 0);

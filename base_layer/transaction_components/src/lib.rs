// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

pub mod aggregated_body;

pub mod crypto_factories;

mod coinbase_builder;
pub use coinbase_builder::{
    generate_coinbase,
    generate_coinbase_with_wallet_output,
    CoinbaseBuildError,
    CoinbaseBuilder,
};
pub mod consensus;
pub mod fee;
pub mod key_manager;
// pub mod legacy_key_manager;
pub mod tari_amount;
pub use tari_amount::MicroMinotari;
pub mod tari_proof_of_work;
pub mod test_helpers;
pub mod transaction_builder;
pub mod transaction_components;
pub mod validation;
pub use transaction_builder::{TransactionBuilder, TransactionBuilderError};
pub mod multisig;
// pub mod offline_signing;

pub mod rpc;

#[cfg(feature = "wasm")]
pub mod wasm;

mod format_currency;
pub use format_currency::format_currency;

pub mod weight;

pub mod helpers;

/// The reason for a peer being banned
#[derive(Clone, Debug)]
pub struct BanReason {
    /// The reason for the ban
    pub reason: String,
    /// The duration of the ban
    pub ban_duration: BanPeriod,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BanPeriod {
    Short,
    Long,
}

impl BanReason {
    /// Create a new ban reason
    pub fn reason(&self) -> &str {
        &self.reason
    }

    /// The duration of the ban
    pub fn ban_duration(&self) -> BanPeriod {
        self.ban_duration
    }
}

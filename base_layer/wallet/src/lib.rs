// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

// #![recursion_limit = "2048"]
// Some functions have a large amount of dependencies (e.g. services) and historically this warning
// has lead to bundling of dependencies into a resources struct, which is then overused and is the
// wrong abstraction
#![allow(clippy::too_many_arguments)]
#[macro_use]
mod macros;
pub mod base_node_service;
pub mod client;
pub mod connectivity_service;
pub mod error;
pub mod legacy_transaction_protocol;
mod operation_id;
pub mod output_manager_service;
pub mod storage;
pub mod test_utils;
pub mod transaction_service;

use tari_transaction_components::key_manager::TransactionKeyManagerWrapper;
pub mod util;
pub mod wallet;

pub use operation_id::OperationId;

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;

mod config;
pub mod schema;
pub mod utxo_scanner_service;
pub use config::{TransactionStage, WalletConfig};
use tari_common_types::transaction::TxId;
use tari_transaction_components::transaction_components::TransactionOutput;
use tari_transaction_key_manager::storage::sqlite_db::TransactionKeyManagerSqliteDatabase;
pub use wallet::Wallet;

use crate::{
    client::http_client_factory::DefaultHttpClientFactory,
    output_manager_service::storage::sqlite_db::OutputManagerSqliteDatabase,
    storage::{sqlite_db::wallet::WalletSqliteDatabase, sqlite_utilities::WalletDbConnection},
    transaction_service::storage::sqlite_db::TransactionServiceSqliteDatabase,
};

mod consts {
    // Import the auto-generated const values from the Manifest and Git
    include!(concat!(env!("OUT_DIR"), "/consts.rs"));
}

pub type WalletSqlite = Wallet<
    WalletSqliteDatabase,
    TransactionServiceSqliteDatabase,
    OutputManagerSqliteDatabase,
    WalletKeyManager,
    DefaultHttpClientFactory,
>;

pub type WalletKeyManager = TransactionKeyManagerWrapper<TransactionKeyManagerSqliteDatabase<WalletDbConnection>>;

// Helper function to derive a TxId from the first ransaction output, or a random TxId if there are no outputs
pub(crate) fn tx_outputs_to_tx_id(outputs: &[TransactionOutput]) -> TxId {
    if let Some(first_output) = outputs.first() {
        TxId::new_deterministic(&first_output.hash())
    } else {
        TxId::new_random()
    }
}

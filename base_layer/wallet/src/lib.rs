// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

#![recursion_limit = "2048"]
// Some functions have a large amount of dependencies (e.g. services) and historically this warning
// has lead to bundling of dependencies into a resources struct, which is then overused and is the
// wrong abstraction
#![allow(clippy::too_many_arguments)]

#[macro_use]
mod macros;
pub mod base_node_service;
pub mod connectivity_service;
pub mod contacts_service;
pub mod error;
mod operation_id;
pub mod output_manager_service;
pub mod storage;
pub mod test_utils;
pub mod transaction_service;
pub mod types;

pub use types::WalletHasher; // For use externally to the code base
pub mod util;
pub mod wallet;

pub use operation_id::OperationId;
use tari_crypto::{
    hash::blake2::Blake256,
    hash_domain,
    hashing::{DomainSeparatedHash, DomainSeparatedHasher},
    keys::PublicKey as PKtrait,
};

#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;

mod config;
pub mod key_manager_service;
pub mod schema;
pub mod utxo_scanner_service;

pub use config::{TransactionStage, WalletConfig};
use tari_common_types::types::{PrivateKey, PublicKey};
use tari_comms::types::CommsDHKE;
use tari_utilities::{ByteArray, ByteArrayError};
pub use wallet::Wallet;

use crate::{
    contacts_service::storage::sqlite_db::ContactsServiceSqliteDatabase,
    key_manager_service::storage::sqlite_db::KeyManagerSqliteDatabase,
    output_manager_service::storage::sqlite_db::OutputManagerSqliteDatabase,
    storage::sqlite_db::wallet::WalletSqliteDatabase,
    transaction_service::storage::sqlite_db::TransactionServiceSqliteDatabase,
};

pub type WalletSqlite = Wallet<
    WalletSqliteDatabase,
    TransactionServiceSqliteDatabase,
    OutputManagerSqliteDatabase,
    ContactsServiceSqliteDatabase,
    KeyManagerSqliteDatabase,
>;

hash_domain!(
    WalletOutputRewindKeysDomain,
    "com.tari.tari_project.base_layer.wallet.output_rewind_keys",
    1
);
type WalletOutputRewindKeysDomainHasher = DomainSeparatedHasher<Blake256, WalletOutputRewindKeysDomain>;

hash_domain!(
    WalletOutputEncryptionKeysDomain,
    "com.tari.tari_project.base_layer.wallet.output_encryption_keys",
    1
);
type WalletOutputEncryptionKeysDomainHasher = DomainSeparatedHasher<Blake256, WalletOutputEncryptionKeysDomain>;

hash_domain!(
    WalletOutputSpendingKeysDomain,
    "com.tari.tari_project.base_layer.wallet.output_spending_keys",
    1
);
type WalletOutputSpendingKeysDomainHasher = DomainSeparatedHasher<Blake256, WalletOutputSpendingKeysDomain>;

/// Generate an output rewind key from a Diffie-Hellman shared secret
pub fn shared_secret_to_output_rewind_key(shared_secret: &CommsDHKE) -> Result<PrivateKey, ByteArrayError> {
    PrivateKey::from_bytes(
        WalletOutputRewindKeysDomainHasher::new()
            .chain(shared_secret.as_bytes())
            .finalize()
            .as_ref(),
    )
}

/// Generate an output encryption key from a Diffie-Hellman shared secret
pub fn shared_secret_to_output_encryption_key(shared_secret: &CommsDHKE) -> Result<PrivateKey, ByteArrayError> {
    PrivateKey::from_bytes(
        WalletOutputEncryptionKeysDomainHasher::new()
            .chain(shared_secret.as_bytes())
            .finalize()
            .as_ref(),
    )
}

/// Generate an output spending key from a Diffie-Hellman shared secret
pub fn shared_secret_to_output_spending_key(shared_secret: &CommsDHKE) -> Result<PrivateKey, ByteArrayError> {
    PrivateKey::from_bytes(
        WalletOutputSpendingKeysDomainHasher::new()
            .chain(shared_secret.as_bytes())
            .finalize()
            .as_ref(),
    )
}

/// Stealth address domain separated hasher using Diffie-Hellman shared secret
pub fn diffie_hellman_stealth_address_wallet_domain_hasher(
    private_key: &PrivateKey,
    public_key: &PublicKey,
) -> DomainSeparatedHash<Blake256> {
    WalletHasher::new_with_label("stealth_address")
        .chain(CommsDHKE::new(private_key, public_key).as_bytes())
        .finalize()
}

/// Stealth payment script spending key
pub fn stealth_address_script_spending_key(
    dh_domain_hasher: &DomainSeparatedHash<Blake256>,
    destination_public_key: &PublicKey,
) -> PublicKey {
    PublicKey::from_secret_key(
        &PrivateKey::from_bytes(dh_domain_hasher.as_ref()).expect("'DomainSeparatedHash<Blake256>' has correct size"),
    ) + destination_public_key
}

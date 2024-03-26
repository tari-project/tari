// Copyright 2023 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{iter, mem::size_of};

use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng, RngCore};
use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};
use tari_common_types::wallet_types::WalletType;
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager_service::storage::{database::KeyManagerDatabase, sqlite_db::KeyManagerSqliteDatabase},
};

use crate::transactions::{key_manager::TransactionKeyManagerWrapper, CryptoFactories};

pub type MemoryDbKeyManager = TransactionKeyManagerWrapper<KeyManagerSqliteDatabase<DbConnection>>;

fn random_string(len: usize) -> String {
    iter::repeat(())
        .map(|_| OsRng.sample(Alphanumeric) as char)
        .take(len)
        .collect()
}

pub fn create_memory_db_key_manager_with_range_proof_size(size: usize) -> MemoryDbKeyManager {
    let connection = DbConnection::connect_url(&DbConnectionUrl::MemoryShared(random_string(8))).unwrap();
    let cipher = CipherSeed::new();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);
    let factory = CryptoFactories::new(size);
    let wallet_type = WalletType::Software;

    TransactionKeyManagerWrapper::<KeyManagerSqliteDatabase<DbConnection>>::new(
        cipher,
        KeyManagerDatabase::new(KeyManagerSqliteDatabase::init(connection, db_cipher)),
        factory,
        wallet_type,
    )
    .unwrap()
}

pub fn create_memory_db_key_manager() -> MemoryDbKeyManager {
    create_memory_db_key_manager_with_range_proof_size(64)
}

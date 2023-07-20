// Copyright 2022. The Tari Project
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
use std::mem::size_of;

use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use minotari_wallet::storage::sqlite_utilities::WalletDbConnection;
use rand::{rngs::OsRng, RngCore};
use tari_common_types::types::PublicKey;
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager_service::{
        storage::{database::KeyManagerDatabase, sqlite_db::KeyManagerSqliteDatabase},
        AddResult,
        KeyId,
        KeyManagerHandle,
        KeyManagerInterface,
    },
};

use crate::support::data::get_temp_sqlite_database_connection;

#[tokio::test]
async fn get_key_at_test_with_encryption() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let cipher = CipherSeed::new();
    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);
    let key_manager = KeyManagerHandle::<KeyManagerSqliteDatabase<WalletDbConnection>, PublicKey>::new(
        cipher,
        KeyManagerDatabase::new(KeyManagerSqliteDatabase::init(connection, db_cipher)),
    );
    key_manager.add_new_branch("branch1").await.unwrap();
    let key_1 = key_manager.get_next_key("branch1").await.unwrap();
    let key_2 = key_manager.get_next_key("branch1").await.unwrap();
    let key_3 = key_manager.get_next_key("branch1").await.unwrap();

    assert_ne!(key_1, key_2);
    assert_ne!(key_1, key_3);
    assert_ne!(key_2, key_3);

    let key_1_2 = key_manager
        .get_public_key_at_key_id(&KeyId::Managed {
            branch: "branch1".to_string(),
            index: 1,
        })
        .await
        .unwrap();
    assert_eq!(key_1.1, key_1_2);
}

#[tokio::test]
async fn key_manager_multiple_branches() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let cipher = CipherSeed::new();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);

    let key_manager = KeyManagerHandle::<KeyManagerSqliteDatabase<WalletDbConnection>, PublicKey>::new(
        cipher,
        KeyManagerDatabase::new(KeyManagerSqliteDatabase::init(connection, db_cipher)),
    );
    assert_eq!(
        key_manager.add_new_branch("branch1").await.unwrap(),
        AddResult::NewEntry
    );
    assert_eq!(
        key_manager.add_new_branch("branch1").await.unwrap(),
        AddResult::AlreadyExists
    );
    key_manager.add_new_branch("branch2").await.unwrap();
    key_manager.add_new_branch("branch3").await.unwrap();
    let key_1 = key_manager.get_next_key("branch1").await.unwrap();
    let key_2 = key_manager.get_next_key("branch2").await.unwrap();
    let key_3 = key_manager.get_next_key("branch3").await.unwrap();
    assert!(key_manager.get_next_key("branch4").await.is_err());

    assert_ne!(key_1, key_2);
    assert_ne!(key_1, key_3);
    assert_ne!(key_2, key_3);

    let key_1 = key_manager
        .get_public_key_at_key_id(&KeyId::Managed {
            branch: "branch1".to_string(),
            index: 1,
        })
        .await
        .unwrap();
    let key_2 = key_manager
        .get_public_key_at_key_id(&KeyId::Managed {
            branch: "branch2".to_string(),
            index: 1,
        })
        .await
        .unwrap();
    let key_3 = key_manager
        .get_public_key_at_key_id(&KeyId::Managed {
            branch: "branch3".to_string(),
            index: 1,
        })
        .await
        .unwrap();

    assert_ne!(key_1, key_2);
    assert_ne!(key_1, key_3);
    assert_ne!(key_2, key_3);
}

#[tokio::test]
async fn key_manager_find_index() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let cipher = CipherSeed::new();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);

    let key_manager = KeyManagerHandle::<KeyManagerSqliteDatabase<WalletDbConnection>, PublicKey>::new(
        cipher,
        KeyManagerDatabase::new(KeyManagerSqliteDatabase::init(connection, db_cipher)),
    );
    key_manager.add_new_branch("branch1").await.unwrap();
    let _next_key = key_manager.get_next_key("branch1").await.unwrap();
    let _next_key = key_manager.get_next_key("branch1").await.unwrap();
    let key_1 = key_manager.get_next_key("branch1").await.unwrap();
    let index = key_manager.find_key_index("branch1", &key_1.1).await.unwrap();

    assert_eq!(index, 3);
}

#[tokio::test]
async fn key_manager_update_current_key_index_if_higher() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let cipher = CipherSeed::new();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);

    let key_manager = KeyManagerHandle::<KeyManagerSqliteDatabase<WalletDbConnection>, PublicKey>::new(
        cipher,
        KeyManagerDatabase::new(KeyManagerSqliteDatabase::init(connection, db_cipher)),
    );
    key_manager.add_new_branch("branch1").await.unwrap();
    let _next_key_result = key_manager.get_next_key("branch1").await.unwrap();
    let _next_key_result = key_manager.get_next_key("branch1").await.unwrap();
    let key_1 = key_manager.get_next_key("branch1").await.unwrap();
    let index = key_manager.find_key_index("branch1", &key_1.1).await.unwrap();

    assert_eq!(index, 3);

    key_manager
        .update_current_key_index_if_higher("branch1", 6)
        .await
        .unwrap();
    let key_1 = key_manager.get_next_key("branch1").await.unwrap();
    let key_1_2 = key_manager
        .get_public_key_at_key_id(&KeyId::Managed {
            branch: "branch1".to_string(),
            index: 7,
        })
        .await
        .unwrap();
    let index = key_manager.find_key_index("branch1", &key_1_2).await.unwrap();
    assert_eq!(index, 7);
    assert_eq!(key_1_2, key_1.1);
}

#[tokio::test]
async fn key_manager_test_index() {
    let (connection, _tempdir) = get_temp_sqlite_database_connection();
    let cipher = CipherSeed::new();

    let mut key = [0u8; size_of::<Key>()];
    OsRng.fill_bytes(&mut key);
    let key_ga = Key::from_slice(&key);
    let db_cipher = XChaCha20Poly1305::new(key_ga);

    let key_manager = KeyManagerHandle::<KeyManagerSqliteDatabase<WalletDbConnection>, PublicKey>::new(
        cipher,
        KeyManagerDatabase::new(KeyManagerSqliteDatabase::init(connection, db_cipher)),
    );
    key_manager.add_new_branch("branch1").await.unwrap();
    key_manager.add_new_branch("branch2").await.unwrap();
    let _next_key_result = key_manager.get_next_key("branch1").await.unwrap();
    let _next_key_result = key_manager.get_next_key("branch1").await.unwrap();
    let result = key_manager.get_next_key("branch1").await.unwrap();
    let key_2 = key_manager
        .get_public_key_at_key_id(&KeyId::Managed {
            branch: "branch2".to_string(),
            index: result.0.managed_index().unwrap(),
        })
        .await
        .unwrap();

    assert_eq!(
        result.0.managed_index().unwrap(),
        key_manager.find_key_index("branch1", &result.1).await.unwrap()
    );
    assert_eq!(
        result.0.managed_index().unwrap(),
        key_manager.find_key_index("branch2", &key_2).await.unwrap()
    );
}

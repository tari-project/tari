//  Copyright 2021. The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::storage::{
  models::key_index_row::KeyIndexRow,
  sqlite::{SqliteDbFactory, SqliteTransaction},
  CollectiblesStorage, KeyIndicesTableGateway, StorageError, StorageTransaction,
  WalletsTableGateway,
};
use std::fmt::Display;
use tari_common_types::types::{PrivateKey, PublicKey};
use tari_crypto::{common::Blake256, keys::PublicKey as PublicKeyTrait};
use tari_key_manager::key_manager::KeyManager;
use tari_utilities::{hex::Hex, ByteArrayError};
use uuid::Uuid;

pub trait KeyManagerProvider<T: StorageTransaction> {
  type Error: Display;
  fn generate_asset_public_key(
    &self,
    wallet_id: Uuid,
    passphrase: Option<String>,
    transaction: &T,
  ) -> Result<(String, PrivateKey, PublicKey), Self::Error>;

  // TODO: Maybe a better name here so that it doesn't get confused with generate_asset_public_key. This is used by ERC20-like receiving addresses
  fn generate_asset_address(
    &self,
    wallet_id: Uuid,
    asset_public_key: &PublicKey,
    passphrase: Option<String>,
    transaction: &T,
  ) -> Result<(String, PrivateKey, PublicKey), Self::Error>;
}

pub struct ConcreteKeyManagerProvider {
  db_factory: SqliteDbFactory,
}

impl ConcreteKeyManagerProvider {
  pub fn new(db_factory: SqliteDbFactory) -> Self {
    Self { db_factory }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum KeyManagerProviderError {
  #[error("Could not derive key:{0}")]
  CouldNotDeriveKey(#[from] ByteArrayError),
  #[error("Storage error:{0}")]
  StorageError(#[from] StorageError),
}

impl KeyManagerProvider<SqliteTransaction> for ConcreteKeyManagerProvider {
  type Error = KeyManagerProviderError;

  fn generate_asset_public_key(
    &self,
    wallet_id: Uuid,
    passphrase: Option<String>,
    transaction: &SqliteTransaction,
  ) -> Result<(String, PrivateKey, PublicKey), Self::Error> {
    let db = self.db_factory.create_db()?;
    let cipher_seed = db
      .wallets()
      .get_cipher_seed(wallet_id, passphrase, Some(transaction))?;
    let row = match db.key_indices().find("assets".to_string(), transaction)? {
      Some(row) => row,
      None => {
        let row = KeyIndexRow {
          id: Uuid::new_v4(),
          branch_seed: "assets".to_string(),
          last_index: 0,
        };

        db.key_indices().insert(&row, transaction)?;
        row
      }
    };

    let mut key_manager = KeyManager::<PrivateKey, Blake256>::from(
      cipher_seed,
      row.branch_seed.clone(),
      row.last_index,
    );
    let new_key = key_manager
      .next_key()
      .map_err(KeyManagerProviderError::CouldNotDeriveKey)?;

    db.key_indices()
      .update_last_index(&row, new_key.key_index, transaction)?;

    let pub_key = PublicKey::from_secret_key(&new_key.k);
    Ok((format!("assets:{}", new_key.key_index), new_key.k, pub_key))
  }

  fn generate_asset_address(
    &self,
    wallet_id: Uuid,
    asset_public_key: &PublicKey,
    passphrase: Option<String>,
    transaction: &SqliteTransaction,
  ) -> Result<(String, PrivateKey, PublicKey), Self::Error> {
    let db = self.db_factory.create_db()?;
    let cipher_seed = db
      .wallets()
      .get_cipher_seed(wallet_id, passphrase, Some(transaction))?;
    let row = match db
      .key_indices()
      .find(format!("assets/{}", asset_public_key.to_hex()), transaction)?
    {
      Some(row) => row,
      None => {
        let row = KeyIndexRow {
          id: Uuid::new_v4(),
          branch_seed: format!("assets/{}", asset_public_key.to_hex()),
          last_index: 0,
        };

        db.key_indices().insert(&row, transaction)?;
        row
      }
    };

    let mut key_manager = KeyManager::<PrivateKey, Blake256>::from(
      cipher_seed,
      row.branch_seed.clone(),
      row.last_index,
    );
    let new_key = key_manager
      .next_key()
      .map_err(KeyManagerProviderError::CouldNotDeriveKey)?;

    db.key_indices()
      .update_last_index(&row, new_key.key_index, transaction)?;

    let pub_key = PublicKey::from_secret_key(&new_key.k);
    Ok((
      format!("assets/{}:{}", asset_public_key.to_hex(), new_key.key_index),
      new_key.k,
      pub_key,
    ))
  }
}

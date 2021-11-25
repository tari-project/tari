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
use crate::{
  models::{NewWallet, Wallet, WalletInfo},
  schema::{self, *},
  storage::{
    models::{asset_row::AssetRow, wallet_row::WalletRow},
    sqlite::{models, sqlite_transaction::SqliteTransaction},
    AssetsTableGateway, CollectiblesStorage, StorageError, WalletsTableGateway,
  },
};
use diesel::{prelude::*, Connection, SqliteConnection};
use std::{fs, path::Path};
use tari_common_types::types::PublicKey;
use tari_key_manager::{
  cipher_seed::{CipherSeed, DEFAULT_CIPHER_SEED_PASSPHRASE},
  error::KeyManagerError,
};
use tari_utilities::ByteArray;
use uuid::Uuid;

pub struct SqliteWalletsTableGateway {
  pub database_url: String,
}

impl SqliteWalletsTableGateway {
  fn convert_wallet(w: &models::Wallet) -> Result<WalletRow, StorageError> {
    Ok(WalletRow {
      id: Uuid::from_slice(&w.id)?,
      name: w.name.clone(),
    })
  }
}

impl WalletsTableGateway<SqliteTransaction> for SqliteWalletsTableGateway {
  type Passphrase = String;

  fn list(&self, tx: Option<&SqliteTransaction>) -> Result<Vec<WalletRow>, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let results: Vec<models::Wallet> = schema::wallets::table.load(&conn)?;
    Ok(
      results
        .iter()
        .map(|w| WalletRow {
          id: Uuid::from_slice(&w.id).unwrap(),
          name: w.name.clone(),
          // cipher_seed: CipherSeed::w.cipher_seed.clone(),
        })
        .collect(),
    )
  }

  fn insert(
    &self,
    wallet: &WalletRow,
    passphrase: Option<Self::Passphrase>,
    tx: &SqliteTransaction,
  ) -> Result<(), StorageError> {
    let cipher_seed = CipherSeed::new();
    // todo: error
    let sql_model = models::Wallet {
      id: Vec::from(wallet.id.as_bytes().as_slice()),
      name: wallet.name.clone(),
      cipher_seed: cipher_seed.encipher(passphrase).unwrap(),
    };

    // use crate::schema::wallets;
    diesel::insert_into(wallets::table)
      .values(sql_model)
      .execute(tx.connection())?;

    Ok(())
  }

  fn find(&self, id: Uuid, tx: Option<&SqliteTransaction>) -> Result<WalletRow, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let db_wallet = schema::wallets::table
      .find(Vec::from(id.as_bytes().as_slice()))
      .get_result(&conn)?;

    SqliteWalletsTableGateway::convert_wallet(&db_wallet)
  }

  fn get_cipher_seed(
    &self,
    id: Uuid,
    pass: Option<Self::Passphrase>,
    tx: Option<&SqliteTransaction>,
  ) -> Result<CipherSeed, StorageError> {
    let mut other_conn = None;
    if tx.is_none() {
      other_conn = Some(SqliteConnection::establish(self.database_url.as_str())?);
    }
    let w: models::Wallet = schema::wallets::table
      .find(Vec::from(id.as_bytes().as_slice()))
      .get_result(
        tx.map(|t| t.connection())
          .unwrap_or_else(|| other_conn.as_ref().unwrap()),
      )?;
    let cipher_seed = match CipherSeed::from_enciphered_bytes(&w.cipher_seed, pass) {
      Ok(seed) => seed,
      Err(e) if matches!(e, KeyManagerError::DecryptionFailed) => {
        return Err(StorageError::WrongPassword)
      }
      Err(e) => return Err(e.into()),
    };

    Ok(cipher_seed)
  }
}

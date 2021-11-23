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
  models::{Account, KeyIndex, NewAccount, NewKeyIndex, NewWallet, Wallet, WalletInfo},
  schema::{self, *},
  storage::{AccountsTableGateway, CollectiblesStorage, StorageError, WalletsTableGateway},
};
use diesel::{prelude::*, Connection, SqliteConnection};
use std::{fs, path::Path};
use tari_common_types::types::PublicKey;
use tari_key_manager::{cipher_seed::CipherSeed, error::KeyManagerError};
use tari_utilities::ByteArray;
use uuid::Uuid;

use super::KeyIndicesTableGateway;

pub mod models;

pub struct SqliteDbFactory {
  database_url: String,
}

impl SqliteDbFactory {
  pub fn new(data_dir: &Path) -> Self {
    fs::create_dir_all(data_dir)
      .unwrap_or_else(|_| panic!("Could not create data directory: {:?}", data_dir));
    let database_url = data_dir
      .join("collectibles.sqlite")
      .into_os_string()
      .into_string()
      .unwrap();

    Self { database_url }
  }

  pub fn create_db(&self) -> Result<SqliteCollectiblesStorage, StorageError> {
    let connection = SqliteConnection::establish(self.database_url.as_str())?;
    connection.execute("PRAGMA foreign_keys = ON;")?;
    // Create the db
    embed_migrations!("./migrations");
    embedded_migrations::run(&connection)?;
    Ok(SqliteCollectiblesStorage {
      database_url: self.database_url.clone(),
    })
  }
}

pub struct SqliteCollectiblesStorage {
  database_url: String,
}

impl CollectiblesStorage for SqliteCollectiblesStorage {
  type Accounts = SqliteAccountsTableGateway;
  type KeyIndices = SqliteKeyIndicesTableGateway;
  type Wallets = SqliteWalletsTableGateway;

  fn accounts(&self) -> Self::Accounts {
    SqliteAccountsTableGateway {
      database_url: self.database_url.clone(),
    }
  }

  fn key_indices(&self) -> Self::KeyIndices {
    SqliteKeyIndicesTableGateway {
      database_url: self.database_url.clone(),
    }
  }

  fn wallets(&self) -> Self::Wallets {
    SqliteWalletsTableGateway {
      database_url: self.database_url.clone(),
    }
  }
}

pub struct SqliteAccountsTableGateway {
  database_url: String,
}

impl AccountsTableGateway for SqliteAccountsTableGateway {
  fn list(&self) -> Result<Vec<Account>, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let results: Vec<models::Account> = schema::accounts::table
      .order_by(schema::accounts::name.asc())
      .load(&conn)?;
    results
      .iter()
      .map(SqliteAccountsTableGateway::convert_account)
      .collect::<Result<_, _>>()
  }

  fn insert(&self, account: NewAccount) -> Result<Account, StorageError> {
    let id = Uuid::new_v4();
    let mut committee_pub_keys = vec![];
    if let Some(pub_keys) = account.committee.as_ref() {
      for key in pub_keys {
        committee_pub_keys.extend_from_slice(key.as_bytes());
      }
    }
    // let committee_pub_keys = if committee_pub_keys.is_empty() { None} else {Some(committee_pub_keys)};

    let sql_model = models::Account {
      id: Vec::from(id.as_bytes().as_slice()),
      asset_public_key: Vec::from(account.asset_public_key.as_bytes()),
      name: account.name.clone(),
      description: account.description.clone(),
      image: account.image.clone(),
      committee_length: account
        .committee
        .as_ref()
        .map(|s| s.len() as i32)
        .unwrap_or(0i32),
      committee_pub_keys,
    };
    let conn = SqliteConnection::establish(self.database_url.as_str())?;

    diesel::insert_into(accounts::table)
      .values(sql_model)
      .execute(&conn)?;
    let result = Account {
      id,
      asset_public_key: account.asset_public_key,
      name: account.name,
      description: account.description,
      image: account.image,
      committee: account.committee,
    };
    Ok(result)
  }

  fn find(&self, account_id: Uuid) -> Result<Account, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let db_account = schema::accounts::table
      .find(Vec::from(account_id.as_bytes().as_slice()))
      .get_result(&conn)?;

    SqliteAccountsTableGateway::convert_account(&db_account)
  }
}

impl SqliteAccountsTableGateway {
  fn convert_account(r: &models::Account) -> Result<Account, StorageError> {
    let mut committee = Vec::with_capacity(r.committee_length as usize);
    for i in 0..r.committee_length as usize {
      committee.push(PublicKey::from_bytes(&r.committee_pub_keys[i * 32..(i + 1) * 32]).unwrap());
    }
    Ok(Account {
      id: Uuid::from_slice(&r.id).unwrap(),
      asset_public_key: PublicKey::from_bytes(&r.asset_public_key).unwrap(),
      name: r.name.clone(),
      description: r.description.clone(),
      image: r.image.clone(),
      committee: if committee.is_empty() {
        None
      } else {
        Some(committee)
      },
    })
  }
}

pub struct SqliteWalletsTableGateway {
  database_url: String,
}

impl SqliteWalletsTableGateway {
  fn convert_wallet(w: &models::Wallet, pass: Option<String>) -> Result<Wallet, StorageError> {
    let cipher_seed = match CipherSeed::from_enciphered_bytes(&w.cipher_seed, pass) {
      Ok(seed) => seed,
      Err(e) if matches!(e, KeyManagerError::DecryptionFailed) => {
        return Err(StorageError::WrongPassword)
      }
      Err(e) => return Err(e.into()),
    };

    Ok(Wallet {
      id: Uuid::from_slice(&w.id)?,
      name: w.name.clone(),
      cipher_seed,
    })
  }
}

impl WalletsTableGateway for SqliteWalletsTableGateway {
  type Passphrase = Option<String>;

  fn list(&self) -> Result<Vec<WalletInfo>, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let results: Vec<models::Wallet> = schema::wallets::table.load(&conn)?;
    Ok(
      results
        .iter()
        .map(|w| WalletInfo {
          id: Uuid::from_slice(&w.id).unwrap(),
          name: w.name.clone(),
        })
        .collect(),
    )
  }

  fn insert(
    &self,
    wallet: NewWallet,
    passphrase: Self::Passphrase,
  ) -> Result<Wallet, StorageError> {
    let id = Uuid::new_v4();

    // todo: error
    let sql_model = models::Wallet {
      id: Vec::from(id.as_bytes().as_slice()),
      name: wallet.name.clone(),
      cipher_seed: wallet.cipher_seed.encipher(passphrase).unwrap(),
    };
    let conn = SqliteConnection::establish(self.database_url.as_str())?;

    // use crate::schema::wallets;
    diesel::insert_into(wallets::table)
      .values(sql_model)
      .execute(&conn)?;

    let result = Wallet {
      id,
      name: wallet.name,
      cipher_seed: wallet.cipher_seed,
    };
    Ok(result)
  }

  fn find(&self, id: Uuid, passphrase: Self::Passphrase) -> Result<Wallet, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let db_wallet = schema::wallets::table
      .find(Vec::from(id.as_bytes().as_slice()))
      .get_result(&conn)?;

    SqliteWalletsTableGateway::convert_wallet(&db_wallet, passphrase)
  }
}

pub struct SqliteKeyIndicesTableGateway {
  database_url: String,
}

impl KeyIndicesTableGateway for SqliteKeyIndicesTableGateway {
  fn list(&self) -> Result<Vec<KeyIndex>, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let results: Vec<models::KeyIndex> = schema::key_indices::table.load(&conn)?;
    Ok(
      results
        .iter()
        .map(|k| KeyIndex {
          id: Uuid::from_slice(&k.id).unwrap(),
          branch_seed: k.branch_seed.clone(),
          last_index: k.last_index as u64,
        })
        .collect(),
    )
  }

  fn insert(&self, key_index: NewKeyIndex) -> Result<KeyIndex, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let find_result: Option<models::KeyIndex> = schema::key_indices::table
      .filter(key_indices::branch_seed.eq(key_index.branch_seed.clone()))
      .first(&conn)
      .optional()?;

    let result = match find_result {
      Some(k) => {
        // update existing
        let id = k.id.clone();
        diesel::update(key_indices::table.filter(key_indices::id.eq(id)))
          .set(key_indices::last_index.eq(key_index.index as i64))
          .execute(&conn)?;
        KeyIndex {
          id: Uuid::from_slice(&k.id).unwrap(),
          branch_seed: k.branch_seed,
          last_index: key_index.index,
        }
      }
      None => {
        // insert new
        let id = Uuid::new_v4();
        let sql_model = models::KeyIndex {
          id: Vec::from(id.as_bytes().as_slice()),
          branch_seed: key_index.branch_seed.clone(),
          last_index: key_index.index as i64,
        };
        diesel::insert_into(key_indices::table)
          .values(sql_model)
          .execute(&conn)?;
        KeyIndex {
          id,
          branch_seed: key_index.branch_seed,
          last_index: key_index.index,
        }
      }
    };

    Ok(result)
  }

  fn find(&self, branch_seed: String) -> Result<Option<KeyIndex>, StorageError> {
    let conn = SqliteConnection::establish(self.database_url.as_str())?;
    let result: Option<models::KeyIndex> = schema::key_indices::table
      .filter(key_indices::branch_seed.eq(branch_seed))
      .first(&conn)
      .optional()?;

    Ok(result.map(|k| KeyIndex {
      id: Uuid::from_slice(&k.id).unwrap(),
      branch_seed: k.branch_seed.clone(),
      last_index: k.last_index as u64,
    }))
  }
}

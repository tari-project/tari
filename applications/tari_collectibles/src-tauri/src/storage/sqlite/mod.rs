use crate::{
  models::{Account, NewAccount},
  storage::{AccountsTableGateway, CollectiblesStorage, StorageError},
};
use diesel::{Connection, SqliteConnection};
use std::path::Path;
use tari_utilities::ByteArray;
use uuid::Uuid;

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
pub mod models;
use crate::schema::{self, accounts::dsl::accounts, *};
use diesel::prelude::*;
use std::fs;
use tari_common_types::types::PublicKey;

pub struct SqliteDbFactory {
  database_url: String,
}
impl SqliteDbFactory {
  pub fn new(data_dir: &Path) -> Self {
    fs::create_dir_all(data_dir)
      .expect(&format!("Could not create data directory: {:?}", data_dir));
    let database_url = data_dir
      .join("collectibles.sqlite")
      .into_os_string()
      .into_string()
      .unwrap();

    Self { database_url }
  }

  pub fn create_db(&self) -> Result<SqliteCollectiblesStorage, StorageError> {
    let connection = SqliteConnection::establish(self.database_url.as_str())?;
    connection.execute("PRAGMA foreign_keys = ON;");
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

  fn accounts(&self) -> Self::Accounts {
    SqliteAccountsTableGateway {
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
    Ok(
      results
        .iter()
        .map(|r| {
          let mut committee = Vec::with_capacity(r.committee_length as usize);
          for i in 0..r.committee_length as usize {
            committee
              .push(PublicKey::from_bytes(&r.committee_pub_keys[i * 32..(i + 1) * 32]).unwrap());
          }
          Account {
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
          }
        })
        .collect(),
    )
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

    use crate::schema::accounts;
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
}

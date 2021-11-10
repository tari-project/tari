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
    todo!()
  }

  fn insert(&self, account: NewAccount) -> Result<Account, StorageError> {
    let id = Uuid::new_v4();
    let sql_model = models::Account {
      id: Vec::from(id.as_bytes().as_slice()),
      asset_public_key: Vec::from(account.asset_public_key.as_bytes()),
      name: account.name.clone(),
      description: account.description.clone(),
      image: account.image.clone(),
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
    };
    Ok(result)
  }
}

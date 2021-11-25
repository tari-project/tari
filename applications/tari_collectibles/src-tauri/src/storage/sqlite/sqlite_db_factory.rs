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
use crate::storage::{sqlite::SqliteCollectiblesStorage, StorageError};
use diesel::{Connection, SqliteConnection};
use std::{fs, path::Path};

#[derive(Clone)]
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

  pub fn migrate(&self) -> Result<(), StorageError> {
    let connection = SqliteConnection::establish(self.database_url.as_str())?;
    connection.execute("PRAGMA foreign_keys = ON;")?;
    // Create the db
    embed_migrations!("./migrations");
    embedded_migrations::run(&connection)?;

    Ok(())
  }
  pub fn create_db(&self) -> Result<SqliteCollectiblesStorage, StorageError> {
    Ok(SqliteCollectiblesStorage {
      database_url: self.database_url.clone(),
    })
  }
}

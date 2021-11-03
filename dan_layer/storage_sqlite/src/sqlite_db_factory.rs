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

use crate::{error::SqliteStorageError, SqliteBackendAdapter};
use diesel::{prelude::*, Connection, SqliteConnection};
use diesel_migrations::embed_migrations;
use tari_common::GlobalConfig;
use tari_dan_core::storage::{ChainDb, DbFactory, StorageError};

#[derive(Clone)]
pub struct SqliteDbFactory {
    database_url: String,
}

impl SqliteDbFactory {
    pub fn new(config: &GlobalConfig) -> Self {
        let database_url = config
            .data_dir
            .join("dan_storage.sqlite")
            .into_os_string()
            .into_string()
            .unwrap();

        Self { database_url }
    }

    fn create_adapter(&self) -> SqliteBackendAdapter {
        SqliteBackendAdapter::new(self.database_url.clone())
    }
}

impl DbFactory<SqliteBackendAdapter> for SqliteDbFactory {
    fn create(&self) -> Result<ChainDb<SqliteBackendAdapter>, StorageError> {
        let connection = SqliteConnection::establish(self.database_url.as_str()).map_err(SqliteStorageError::from)?;
        connection.execute("PRAGMA foreign_keys = ON;");
        // Create the db
        embed_migrations!("./migrations");
        embedded_migrations::run(&connection).map_err(SqliteStorageError::from)?;
        Ok(ChainDb::new(self.create_adapter()))
    }
}

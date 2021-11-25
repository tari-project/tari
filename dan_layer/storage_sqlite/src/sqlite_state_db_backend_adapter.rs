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

use crate::{error::SqliteStorageError, models::state_key::StateKey, schema::*, SqliteTransaction};
use diesel::{prelude::*, Connection, SqliteConnection};
use log::*;
use tari_dan_core::storage::state::StateDbBackendAdapter;

const LOG_TARGET: &str = "tari::dan_layer::storage_sqlite::sqlite_state_db_backend_adapter";

#[derive(Clone)]
pub struct SqliteStateDbBackendAdapter {
    database_url: String,
}

impl SqliteStateDbBackendAdapter {
    pub fn new(database_url: String) -> Self {
        SqliteStateDbBackendAdapter { database_url }
    }
}

impl StateDbBackendAdapter for SqliteStateDbBackendAdapter {
    type BackendTransaction = SqliteTransaction;
    type Error = SqliteStorageError;

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error> {
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        connection
            .execute("PRAGMA foreign_keys = ON;")
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "set pragma".to_string(),
            })?;
        connection
            .execute("BEGIN EXCLUSIVE TRANSACTION;")
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "begin transaction".to_string(),
            })?;

        Ok(SqliteTransaction::new(connection))
    }

    fn update_key_value(
        &self,
        schema: &str,
        key: &[u8],
        value: &[u8],
        tx: &Self::BackendTransaction,
    ) -> Result<(), Self::Error> {
        use crate::schema::state_keys::dsl;
        let upsert_data = (
            state_keys::schema_name.eq(schema),
            state_keys::key_name.eq(key),
            state_keys::value.eq(value),
        );
        let row: Option<StateKey> = dsl::state_keys
            .find((schema, key))
            .first(tx.connection())
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "find::state_key".to_string(),
            })?;

        match row {
            Some(r) => diesel::update(&r)
                .set(state_keys::value.eq(value))
                .execute(tx.connection())
                .map_err(|source| SqliteStorageError::DieselError {
                    source,
                    operation: "update::state_key".to_string(),
                })?,
            None => diesel::insert_into(state_keys::table)
                .values(upsert_data)
                .execute(tx.connection())
                .map_err(|source| SqliteStorageError::DieselError {
                    source,
                    operation: "insert::state_key".to_string(),
                })?,
        };
        Ok(())
    }

    fn commit(&self, tx: &Self::BackendTransaction) -> Result<(), Self::Error> {
        debug!(target: LOG_TARGET, "Committing transaction");
        tx.connection()
            .execute("COMMIT TRANSACTION;")
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "commit::state".to_string(),
            })?;
        Ok(())
    }
}

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

use bytecodec::{
    bincode_codec::{BincodeDecoder, BincodeEncoder},
    DecodeExt,
    EncodeExt,
};
use diesel::{prelude::*, Connection, SqliteConnection};
use log::*;
use patricia_tree::{
    node::{Node, NodeDecoder, NodeEncoder},
    PatriciaMap,
};
use tari_dan_core::storage::state::StateDbBackendAdapter;

use crate::{
    error::SqliteStorageError,
    models::{
        state_key::StateKey,
        state_tree::{NewStateTree, StateTree},
    },
    schema::*,
    SqliteTransaction,
};

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

    fn get(&self, schema: &str, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        use crate::schema::state_keys::dsl;
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        let row: Option<StateKey> = dsl::state_keys
            .find((schema, key))
            .first(&connection)
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "get::state_key".to_string(),
            })?;
        Ok(row.map(|r| r.value))
    }

    fn find_keys_by_value(&self, schema: &str, value: &[u8]) -> Result<Vec<Vec<u8>>, Self::Error> {
        use crate::schema::state_keys::dsl;
        let connection = SqliteConnection::establish(self.database_url.as_str())?;
        let row: Vec<StateKey> = dsl::state_keys
            .filter(state_keys::schema_name.eq(schema))
            .filter(state_keys::value.eq(value))
            .get_results(&connection)
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "find_keys_by_value".to_string(),
            })?;
        Ok(row.into_iter().map(|r| r.key_name).collect())
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

    fn get_current_state_tree(&self, tx: &Self::BackendTransaction) -> Result<PatriciaMap<Vec<u8>>, Self::Error> {
        use crate::schema::state_tree::dsl;
        let row: StateTree = dsl::state_tree
            .filter(state_tree::is_current.eq(true))
            .order_by(state_tree::version.desc())
            .first(tx.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "get_current_state_tree".to_string(),
            })?;
        let mut decoder = NodeDecoder::new(BincodeDecoder::new());
        let nodes: Node<Vec<u8>> = decoder.decode_from_bytes(&row.data)?;
        Ok(nodes.into())
    }

    fn set_current_state_tree(
        &self,
        tree: patricia_tree::map::PatriciaMap<Vec<u8>>,
        tx: &Self::BackendTransaction,
    ) -> Result<(), Self::Error> {
        let mut encoder = NodeEncoder::new(BincodeEncoder::new());
        let encoded = encoder.encode_into_bytes(tree.into())?;

        use crate::schema::state_tree::dsl;
        let existing_row: Option<StateTree> = dsl::state_tree
            .filter(state_tree::is_current.eq(true))
            .order_by(state_tree::version.desc())
            .first(tx.connection())
            .optional()
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "set_current_state_tree::fetch".to_string(),
            })?;

        diesel::update(dsl::state_tree.filter(state_tree::is_current.eq(true)))
            .set(state_tree::is_current.eq(false))
            .execute(tx.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "set_current_state_tree:update".to_string(),
            })?;

        let row = NewStateTree {
            version: existing_row.map(|r| r.version).unwrap_or_default() + 1,
            is_current: true,
            data: encoded,
        };

        diesel::insert_into(dsl::state_tree)
            .values(row)
            .execute(tx.connection())
            .map_err(|source| SqliteStorageError::DieselError {
                source,
                operation: "set_current_state_tree::insert".to_string(),
            })?;

        Ok(())
    }
}

// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::state::{db_key_value::DbKeyValue, error::StateStorageError, DbStateOpLogEntry};

pub trait StateDbBackendAdapter: Send + Sync + Clone {
    type BackendTransaction;
    type Error: Into<StateStorageError>;

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error>;
    fn update_key_value(
        &self,
        schema: &str,
        key: &[u8],
        value: &[u8],
        tx: &Self::BackendTransaction,
    ) -> Result<(), Self::Error>;
    fn get(&self, schema: &str, key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error>;
    fn find_keys_by_value(&self, schema: &str, value: &[u8]) -> Result<Vec<Vec<u8>>, Self::Error>;
    fn commit(&self, tx: &Self::BackendTransaction) -> Result<(), Self::Error>;
    fn get_all_schemas(&self, tx: &Self::BackendTransaction) -> Result<Vec<String>, Self::Error>;
    fn get_all_values_for_schema(
        &self,
        schema: &str,
        tx: &Self::BackendTransaction,
    ) -> Result<Vec<DbKeyValue>, Self::Error>;
    fn get_state_op_logs_by_height(
        &self,
        height: u64,
        tx: &Self::BackendTransaction,
    ) -> Result<Vec<DbStateOpLogEntry>, Self::Error>;
    fn add_state_oplog_entry(&self, entry: DbStateOpLogEntry, tx: &Self::BackendTransaction)
        -> Result<(), Self::Error>;
    fn clear_all_state(&self, tx: &Self::BackendTransaction) -> Result<(), Self::Error>;
}

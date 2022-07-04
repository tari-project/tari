// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use crate::state::{error::StateStorageError, DbKeyValue, DbStateOpLogEntry, StateDbBackendAdapter};

#[derive(Debug, Clone, Default)]
pub struct MockStateDbBackupAdapter;

impl StateDbBackendAdapter for MockStateDbBackupAdapter {
    type BackendTransaction = ();
    type Error = StateStorageError;

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error> {
        todo!()
    }

    fn update_key_value(
        &self,
        _schema: &str,
        _key: &[u8],
        _value: &[u8],
        _tx: &Self::BackendTransaction,
    ) -> Result<(), Self::Error> {
        todo!()
    }

    fn get(&self, _schema: &str, _key: &[u8]) -> Result<Option<Vec<u8>>, Self::Error> {
        todo!()
    }

    fn find_keys_by_value(&self, _schema: &str, _value: &[u8]) -> Result<Vec<Vec<u8>>, Self::Error> {
        todo!()
    }

    fn commit(&self, _tx: &Self::BackendTransaction) -> Result<(), Self::Error> {
        todo!()
    }

    fn get_all_schemas(&self, _tx: &Self::BackendTransaction) -> Result<Vec<String>, Self::Error> {
        todo!()
    }

    fn get_all_values_for_schema(
        &self,
        _schema: &str,
        _tx: &Self::BackendTransaction,
    ) -> Result<Vec<DbKeyValue>, Self::Error> {
        todo!()
    }

    fn get_state_op_logs_by_height(
        &self,
        _height: u64,
        _tx: &Self::BackendTransaction,
    ) -> Result<Vec<DbStateOpLogEntry>, Self::Error> {
        todo!()
    }

    fn add_state_oplog_entry(
        &self,
        _entry: DbStateOpLogEntry,
        _tx: &Self::BackendTransaction,
    ) -> Result<(), Self::Error> {
        todo!()
    }

    fn clear_all_state(&self, _tx: &Self::BackendTransaction) -> Result<(), Self::Error> {
        todo!()
    }
}

//  Copyright 2022, The Tari Project
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

use patricia_tree::PatriciaMap;

use crate::storage::{
    state::{DbKeyValue, DbStateOpLogEntry, StateDbBackendAdapter},
    StorageError,
};

#[derive(Debug, Clone, Default)]
pub struct MockStateDbBackupAdapter;

impl StateDbBackendAdapter for MockStateDbBackupAdapter {
    type BackendTransaction = ();
    type Error = StorageError;

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

    fn get_current_state_tree(&self, _tx: &Self::BackendTransaction) -> Result<PatriciaMap<Vec<u8>>, Self::Error> {
        todo!()
    }

    fn set_current_state_tree(
        &self,
        _tree: PatriciaMap<Vec<u8>>,
        _tx: &Self::BackendTransaction,
    ) -> Result<(), Self::Error> {
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

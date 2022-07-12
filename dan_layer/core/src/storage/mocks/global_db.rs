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

use tari_common_types::types::FixedHash;

use crate::storage::{
    global::{ContractState, GlobalDbBackendAdapter, GlobalDbMetadataKey},
    StorageError,
};

#[derive(Debug, Clone, Default)]
pub struct MockGlobalDbBackupAdapter;

impl GlobalDbBackendAdapter for MockGlobalDbBackupAdapter {
    type BackendTransaction = ();
    type Error = StorageError;
    type Model = ();
    type NewModel = ();

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error> {
        todo!()
    }

    fn get_data(&self, _key: GlobalDbMetadataKey) -> Result<Option<Vec<u8>>, Self::Error> {
        todo!()
    }

    fn set_data(&self, _key: GlobalDbMetadataKey, _value: &[u8]) -> Result<(), Self::Error> {
        todo!()
    }

    fn commit(&self, _tx: &Self::BackendTransaction) -> Result<(), Self::Error> {
        todo!()
    }

    fn get_data_with_connection(
        &self,
        _key: &GlobalDbMetadataKey,
        _tx: &Self::BackendTransaction,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        todo!()
    }

    fn save_contract(&self, _contract: Self::Model, _status: ContractState) -> Result<(), Self::Error> {
        todo!()
    }

    fn update_contract_state(&self, _contract_id: FixedHash, _state: ContractState) -> Result<(), Self::Error> {
        todo!()
    }

    fn get_contracts_with_state(&self, _state: ContractState) -> Result<Vec<Self::Model>, Self::Error> {
        todo!()
    }
}

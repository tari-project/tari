//  Copyright 2022. The Tari Project
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

use std::sync::Arc;

use tari_common_types::types::FixedHash;

use crate::storage::{
    global::{ContractState, GlobalDbBackendAdapter, GlobalDbMetadataKey},
    StorageError,
};

#[derive(Debug, Clone)]
pub struct GlobalDb<TGlobalDbBackendAdapter> {
    adapter: Arc<TGlobalDbBackendAdapter>,
}

impl<TGlobalDbBackendAdapter: GlobalDbBackendAdapter> GlobalDb<TGlobalDbBackendAdapter> {
    pub fn new(adapter: TGlobalDbBackendAdapter) -> Self {
        Self {
            adapter: Arc::new(adapter),
        }
    }

    pub fn set_data(&self, key: GlobalDbMetadataKey, value: &[u8]) -> Result<(), StorageError> {
        self.adapter
            .set_data(key, value)
            .map_err(TGlobalDbBackendAdapter::Error::into)
    }

    pub fn get_data(&self, key: GlobalDbMetadataKey) -> Result<Option<Vec<u8>>, StorageError> {
        self.adapter.get_data(key).map_err(TGlobalDbBackendAdapter::Error::into)
    }

    pub fn save_contract(
        &self,
        contract: TGlobalDbBackendAdapter::NewModel,
        state: ContractState,
    ) -> Result<(), StorageError> {
        self.adapter
            .save_contract(contract, state)
            .map_err(TGlobalDbBackendAdapter::Error::into)
    }

    pub fn update_contract_state(&self, contract_id: FixedHash, state: ContractState) -> Result<(), StorageError> {
        self.adapter
            .update_contract_state(contract_id, state)
            .map_err(TGlobalDbBackendAdapter::Error::into)
    }

    pub fn get_contracts_with_state(
        &self,
        state: ContractState,
    ) -> Result<Vec<TGlobalDbBackendAdapter::Model>, StorageError> {
        self.adapter
            .get_contracts_with_state(state)
            .map_err(TGlobalDbBackendAdapter::Error::into)
    }
}

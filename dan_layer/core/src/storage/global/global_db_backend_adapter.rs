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

use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use tari_common_types::types::FixedHash;

use crate::storage::StorageError;

pub trait GlobalDbBackendAdapter: Send + Sync + Clone {
    type BackendTransaction;
    type Error: Into<StorageError>;
    type Model;
    type NewModel;

    fn create_transaction(&self) -> Result<Self::BackendTransaction, Self::Error>;
    fn commit(&self, tx: &Self::BackendTransaction) -> Result<(), Self::Error>;
    fn get_data(&self, key: GlobalDbMetadataKey) -> Result<Option<Vec<u8>>, Self::Error>;
    fn set_data(&self, key: GlobalDbMetadataKey, value: &[u8]) -> Result<(), Self::Error>;
    fn get_data_with_connection(
        &self,
        key: &GlobalDbMetadataKey,
        connection: &Self::BackendTransaction,
    ) -> Result<Option<Vec<u8>>, Self::Error>;
    fn save_contract(&self, contract: Self::NewModel, state: ContractState) -> Result<(), Self::Error>;
    fn update_contract_state(&self, contract_id: FixedHash, state: ContractState) -> Result<(), Self::Error>;
    fn get_contracts_with_state(&self, state: ContractState) -> Result<Vec<Self::Model>, Self::Error>;
}

#[derive(Debug, Clone, Copy)]
pub enum GlobalDbMetadataKey {
    LastScannedConstitutionHash,
    LastScannedConstitutionHeight,
}

impl GlobalDbMetadataKey {
    pub fn as_key_bytes(self) -> &'static [u8] {
        match self {
            GlobalDbMetadataKey::LastScannedConstitutionHash => b"last_scanned_constitution_hash",
            GlobalDbMetadataKey::LastScannedConstitutionHeight => b"last_scanned_constitution_height",
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, FromPrimitive)]
#[repr(u8)]
pub enum ContractState {
    Pending = 0,
    Accepted = 1,
    Expired = 2,
    QuorumMet = 3,
    Active = 4,
    Abandoned = 5,
    Quarantined = 6,
    Shutdown = 7,
}

impl ContractState {
    pub fn as_byte(self) -> u8 {
        self as u8
    }

    /// Returns the Status that corresponds to the byte. None is returned if the byte does not correspond
    pub fn from_byte(value: u8) -> Option<Self> {
        FromPrimitive::from_u8(value)
    }
}

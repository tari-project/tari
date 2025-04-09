// Copyright 2022. The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

mod backend;

use std::sync::Arc;

pub use backend::TransactionKeyManagerBackend;
use tari_common_types::types::{CompressedPublicKey, PrivateKey};

use crate::transactions::transaction_key_manager::error::KeyManagerStorageError;

/// Holds the state of the KeyManager for the branch
#[derive(Clone, Debug, PartialEq)]
pub struct KeyManagerState {
    pub branch_seed: String,
    pub primary_key_index: u64,
}

/// Holds the state of the KeyManager for the branch
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedKey {
    pub private_key: PrivateKey,
    pub public_key: CompressedPublicKey,
}

/// This structure holds an inner type that implements the `KeyManagerBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
#[derive(Clone)]
pub struct TransactionKeyManagerDatabase<T> {
    db: Arc<T>,
}

impl<T> TransactionKeyManagerDatabase<T>
where T: TransactionKeyManagerBackend + 'static
{
    /// Creates a new [KeyManagerDatabase] linked to the provided KeyManagerBackend
    pub fn new(db: T) -> Self {
        Self { db: Arc::new(db) }
    }

    /// Retrieves the key manager state of the provided branch
    /// Returns None if the request branch does not exist.
    pub fn get_key_manager_state(&self, branch: &str) -> Result<Option<KeyManagerState>, KeyManagerStorageError> {
        self.db.get_key_manager(branch)
    }

    /// Saves the specified key manager state to the backend database.
    pub fn set_key_manager_state(&self, state: KeyManagerState) -> Result<(), KeyManagerStorageError> {
        self.db.add_key_manager(state)
    }

    /// Increment the key index of the provided branch of the key manager.
    /// Will error if the branch does not exist.
    pub fn increment_key_index(&self, branch: &str) -> Result<(), KeyManagerStorageError> {
        self.db.increment_key_index(branch)
    }

    /// Sets the key index of the provided branch of the key manager.
    /// Will error if the branch does not exist.
    pub fn set_key_index(&self, branch: &str, index: u64) -> Result<(), KeyManagerStorageError> {
        self.db.set_key_index(branch, index)
    }

    /// This will import and save a private public key combo
    pub fn insert_imported_key(
        &self,
        public_key: CompressedPublicKey,
        private_key: PrivateKey,
    ) -> Result<(), KeyManagerStorageError> {
        self.db.insert_imported_key(public_key, private_key)
    }

    /// This will get the private key associated with the public key
    pub fn get_imported_key(&self, public_key: &CompressedPublicKey) -> Result<PrivateKey, KeyManagerStorageError> {
        self.db.get_imported_key(public_key)
    }
}

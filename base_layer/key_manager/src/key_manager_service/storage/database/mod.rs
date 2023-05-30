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

use std::{marker::PhantomData, sync::Arc};

pub use backend::KeyManagerBackend;
use tari_crypto::keys::PublicKey;

use crate::key_manager_service::error::KeyManagerStorageError;

/// Holds the state of the KeyManager for the branch
#[derive(Clone, Debug, PartialEq)]
pub struct KeyManagerState {
    pub branch_seed: String,
    pub primary_key_index: u64,
}

/// Holds the state of the KeyManager for the branch
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedKey<PK: PublicKey> {
    pub private_key: PK::K,
    pub public_key: PK,
}

/// This structure holds an inner type that implements the `KeyManagerBackend` trait and contains the more complex
/// data access logic required by the module built onto the functionality defined by the trait
#[derive(Clone)]
pub struct KeyManagerDatabase<T, PK> {
    db: Arc<T>,
    public_key: PhantomData<PK>,
}

impl<T, PK> KeyManagerDatabase<T, PK>
where
    T: KeyManagerBackend<PK> + 'static,
    PK: PublicKey,
{
    /// Creates a new [KeyManagerDatabase] linked to the provided KeyManagerBackend
    pub fn new(db: T) -> Self {
        Self {
            db: Arc::new(db),
            public_key: PhantomData,
        }
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
    pub fn insert_imported_key(&self, public_key: PK, private_key: PK::K) -> Result<(), KeyManagerStorageError> {
        self.db.insert_imported_key(public_key, private_key)
    }

    /// This will import and save a private public key combo
    pub fn get_imported_key(&self, public_key: &PK) -> Result<PK::K, KeyManagerStorageError> {
        self.db.get_imported_key(public_key)
    }
}

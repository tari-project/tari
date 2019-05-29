// Copyright 2019 The Tari Project
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

use derive_error::Error;
use serde::{de::DeserializeOwned, Serialize};
use tari_storage::{keyvalue_store::DataStore, lmdb::*};
use tari_utilities::message_format::MessageFormat;

#[derive(Debug, Error)]
pub enum MerkleStorageError {
    /// An error occurred with the underlying data store implementation
    #[error(embedded_msg, no_from, non_std)]
    InternalError(String),
    /// An error occurred during serialization
    #[error(no_from, non_std)]
    SerializationErr(String),
    /// An error occurred during deserialization
    #[error(no_from, non_std)]
    DeserializationErr(String),
    /// An error occurred during a put query
    #[error(embedded_msg, no_from, non_std)]
    PutError(String),
    /// An error occurred during a get query
    #[error(embedded_msg, no_from, non_std)]
    GetError(String),
    /// Sync error, expected some value where it found none
    SyncError,
    /// The persistant storage was not enabled
    StoreNotEnabledError,
}

/// This trait proves an interface for the MMR to store and retrieve data from some storage medium.
pub trait MerkleStorage {
    /// This function will store some object via an id/key id
    fn store<T: Serialize + DeserializeOwned>(
        &mut self,
        id: &str,
        database: &str,
        object: &T,
    ) -> Result<(), MerkleStorageError>;
    /// This function will load some object via an id/key id
    fn load<T: Serialize + DeserializeOwned>(&mut self, id: &str, database: &str) -> Result<T, MerkleStorageError>;
    /// This function will load some object via an id/key id
    fn delete(&mut self, id: &str, database: &str) -> Result<(), MerkleStorageError>;
    // This function is called after all the DB changes where made. It makes it possible to implement batch writes
    // implementing this.
    fn commit(&mut self) -> Result<(), MerkleStorageError>;
}

// todo add feature flags so this does not get added if you dont want our lmdb
impl MerkleStorage for LMDBStore {
    fn store<T: Serialize + DeserializeOwned>(
        &mut self,
        id: &str,
        database: &str,
        object: &T,
    ) -> Result<(), MerkleStorageError>
    {
        self.connect(database)
            .map_err(|e| MerkleStorageError::InternalError(e.to_string()))?;
        self.put_raw(
            id.as_bytes(),
            object
                .to_binary()
                .map_err(|e| MerkleStorageError::SerializationErr(e.to_string()))?,
        )
        .map_err(|e| MerkleStorageError::InternalError(e.to_string()))?;
        Ok(())
    }

    fn load<T: Serialize + DeserializeOwned>(&mut self, id: &str, database: &str) -> Result<T, MerkleStorageError> {
        self.connect(database)
            .map_err(|e| MerkleStorageError::InternalError(e.to_string()))?;
        let value = self
            .get_raw(id.as_bytes())
            .map_err(|e| MerkleStorageError::InternalError(e.to_string()))?;
        if value.is_none() {
            return Err(MerkleStorageError::GetError(("No value").to_string()));
        }
        let buff = value.unwrap();
        let object = T::from_binary(&buff).map_err(|e| MerkleStorageError::DeserializationErr(e.to_string()))?;
        Ok(object)
    }

    fn delete(&mut self, id: &str, database: &str) -> Result<(), MerkleStorageError> {
        self.connect(database)
            .map_err(|e| MerkleStorageError::InternalError(e.to_string()))?;
        self.put_raw(id.as_bytes(), b"".to_vec())
            .map_err(|e| MerkleStorageError::InternalError(e.to_string()))?;
        Ok(())
    }

    fn commit(&mut self) -> Result<(), MerkleStorageError> {
        Ok(())
    }
}

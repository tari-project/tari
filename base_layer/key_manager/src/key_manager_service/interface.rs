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

// use tari_common_types::types::{PrivateKey, PublicKey};

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use tari_common_types::types;
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_utilities::hex::Hex;

use crate::key_manager_service::error::KeyManagerServiceError;

/// The value returned from [add_new_branch]. `AlreadyExists` is returned if the branch was previously created,
/// otherwise `NewEntry` is returned.
#[derive(Debug, PartialEq)]
pub enum AddResult {
    NewEntry,
    AlreadyExists,
}

pub struct NextKeyResult<PK: PublicKey> {
    pub key: PK::K,
    pub index: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum KeyId {
    Managed { branch: String, index: u64 },
    Imported { key: types::PublicKey },
}

impl KeyId {
    pub fn managed_index(&self) -> Option<u64> {
        match self {
            KeyId::Managed { index, .. } => Some(*index),
            KeyId::Imported { .. } => None,
        }
    }

    pub fn managed_branch(&self) -> Option<String> {
        match self {
            KeyId::Managed { branch, .. } => Some(branch.clone()),
            KeyId::Imported { .. } => None,
        }
    }

    pub fn imported(&self) -> Option<types::PublicKey> {
        match self {
            KeyId::Managed { .. } => None,
            KeyId::Imported { key } => Some(key.clone()),
        }
    }
}

impl fmt::Display for KeyId {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KeyId::Managed { branch: b, index: i } => write!(f, "managed.'{}'.'{}'", b, i),
            KeyId::Imported { key: public_key } => write!(f, "imported.'{}'", public_key.to_hex()),
        }
    }
}

impl Default for KeyId {
    fn default() -> Self {
        KeyId::Managed {
            branch: "".to_string(),
            index: 0,
        }
    }
}

impl FromStr for KeyId {
    type Err = String;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = id.split('.').collect();
        match parts.first() {
            None => Err("Out of bounds".to_string()),
            Some(val) => match *val {
                "default" => {
                    if parts.len() != 3 {
                        return Err("Wrong format".to_string());
                    }
                    let index = parts[2]
                        .parse()
                        .map_err(|_| "Index for default, invalid u64".to_string())?;
                    Ok(KeyId::Managed {
                        branch: parts[1].into(),
                        index,
                    })
                },
                "imported" => {
                    if parts.len() != 2 {
                        return Err("Wrong format".to_string());
                    }
                    let key = types::PublicKey::from_hex(parts[1]).map_err(|_| "Invalid public key".to_string())?;
                    Ok(KeyId::Imported { key })
                },
                _ => Err("Wrong format".to_string()),
            },
        }
    }
}

/// Behaviour required for the Key manager service
#[async_trait::async_trait]
pub trait KeyManagerInterface<PK>: Clone + Send + Sync + 'static
where
    PK: PublicKey + Send + Sync + 'static,
    PK::K: SecretKey + Send + Sync + 'static,
{
    /// Creates a new branch for the key manager service to track
    /// If this is an existing branch, that is not yet tracked in memory, the key manager service will load the key
    /// manager from the backend to track in memory, will return `Ok(AddResult::NewEntry)`. If the branch is already
    /// tracked in memory the result will be `Ok(AddResult::AlreadyExists)`. If the branch does not exist in memory
    /// or in the backend, a new branch will be created and tracked the backend, `Ok(AddResult::NewEntry)`.
    async fn add_new_branch<T: Into<String> + Send>(&self, branch: T) -> Result<AddResult, KeyManagerServiceError>;

    /// Gets the next key from the branch. This will auto-increment the branch key index by 1
    async fn get_next_key<T: Into<String> + Send>(
        &self,
        branch: T,
    ) -> Result<NextKeyResult<PK>, KeyManagerServiceError>;

    /// Gets the next key id from the branch. This will auto-increment the branch key index by 1
    async fn get_next_key_id<T: Into<String> + Send>(&self, branch: T) -> Result<KeyId, KeyManagerServiceError>;

    /// Gets the fixed key id from the branch. This will use the branch key with index 0
    async fn get_static_key_id<T: Into<String> + Send>(&self, branch: T) -> Result<KeyId, KeyManagerServiceError>;

    /// Gets the key at the specified index
    async fn get_key_at_index<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<PK::K, KeyManagerServiceError>;

    /// Gets the key id at the specified index
    async fn get_public_key_at_key_id(&self, key_id: &KeyId) -> Result<PK, KeyManagerServiceError>;

    /// Searches the branch to find the index used to generated the key, O(N) where N = index used.
    async fn find_key_index<T: Into<String> + Send>(&self, branch: T, key: &PK) -> Result<u64, KeyManagerServiceError>;

    /// Will update the index of the branch if the index given is higher than the current saved index
    async fn update_current_key_index_if_higher<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<(), KeyManagerServiceError>;

    /// Add a new key to be tracked
    async fn import_key(&self, private_key: PK::K) -> Result<(), KeyManagerServiceError>;
}

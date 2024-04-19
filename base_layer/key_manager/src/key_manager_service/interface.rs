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
use tari_crypto::keys::{PublicKey, SecretKey};
use tari_utilities::{hex::Hex, ByteArray};

use crate::key_manager_service::error::KeyManagerServiceError;

/// The value returned from [add_new_branch]. `AlreadyExists` is returned if the branch was previously created,
/// otherwise `NewEntry` is returned.
#[derive(Debug, PartialEq)]
pub enum AddResult {
    NewEntry,
    AlreadyExists,
}

#[derive(Default, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum KeyId<PK> {
    Managed {
        branch: String,
        index: u64,
    },
    Derived {
        branch: String,
        index: u64,
    },
    Imported {
        key: PK,
    },
    #[default]
    Zero,
}

impl<PK> KeyId<PK>
where PK: Clone
{
    pub fn managed_index(&self) -> Option<u64> {
        match self {
            KeyId::Managed { index, .. } => Some(*index),
            KeyId::Derived { index, .. } => Some(*index),
            KeyId::Imported { .. } => None,
            KeyId::Zero => None,
        }
    }

    pub fn managed_branch(&self) -> Option<String> {
        match self {
            KeyId::Managed { branch, .. } => Some(branch.clone()),
            KeyId::Derived { branch, .. } => Some(branch.clone()),
            KeyId::Imported { .. } => None,
            KeyId::Zero => None,
        }
    }

    pub fn imported(&self) -> Option<PK> {
        match self {
            KeyId::Managed { .. } => None,
            KeyId::Derived { .. } => None,
            KeyId::Imported { key } => Some(key.clone()),
            KeyId::Zero => None,
        }
    }
}

pub const MANAGED_KEY_BRANCH: &str = "managed";
pub const IMPORTED_KEY_BRANCH: &str = "imported";
pub const ZERO_KEY_BRANCH: &str = "zero";

impl<PK> fmt::Display for KeyId<PK>
where PK: ByteArray
{
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KeyId::Managed { branch: b, index: i } => write!(f, "{}.{}.{}", MANAGED_KEY_BRANCH, b, i),
            KeyId::Derived { branch: b, index: i } => write!(f, "{}.{}.{}", MANAGED_KEY_BRANCH, b, i),
            KeyId::Imported { key: public_key } => write!(f, "{}.{}", IMPORTED_KEY_BRANCH, public_key.to_hex()),
            KeyId::Zero => write!(f, "{}", ZERO_KEY_BRANCH),
        }
    }
}

impl<PK> FromStr for KeyId<PK>
where PK: ByteArray
{
    type Err = String;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = id.split('.').collect();
        match parts.first() {
            None => Err("Out of bounds".to_string()),
            Some(val) => match *val {
                MANAGED_KEY_BRANCH => {
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
                IMPORTED_KEY_BRANCH => {
                    if parts.len() != 2 {
                        return Err("Wrong format".to_string());
                    }
                    let key = PK::from_hex(parts[1]).map_err(|_| "Invalid public key".to_string())?;
                    Ok(KeyId::Imported { key })
                },
                ZERO_KEY_BRANCH => Ok(KeyId::Zero),
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

    /// Gets the next key id from the branch. This will auto-increment the branch key index by 1
    async fn get_next_key<T: Into<String> + Send>(&self, branch: T) -> Result<(KeyId<PK>, PK), KeyManagerServiceError>;

    /// Gets the fixed key id from the branch. This will use the branch key with index 0
    async fn get_static_key<T: Into<String> + Send>(&self, branch: T) -> Result<KeyId<PK>, KeyManagerServiceError>;

    /// Gets the key id at the specified index
    async fn get_public_key_at_key_id(&self, key_id: &KeyId<PK>) -> Result<PK, KeyManagerServiceError>;

    /// Searches the branch to find the index used to generated the key, O(N) where N = index used.
    async fn find_key_index<T: Into<String> + Send>(&self, branch: T, key: &PK) -> Result<u64, KeyManagerServiceError>;

    /// Will update the index of the branch if the index given is higher than the current saved index
    async fn update_current_key_index_if_higher<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<(), KeyManagerServiceError>;

    /// Add a new key to be tracked
    async fn import_key(&self, private_key: PK::K) -> Result<KeyId<PK>, KeyManagerServiceError>;
}

#[cfg(test)]
mod test {
    use core::iter;
    use std::str::FromStr;

    use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
    use tari_common_types::types::{PrivateKey, PublicKey};
    use tari_crypto::keys::{PublicKey as PK, SecretKey as SK};

    use crate::key_manager_service::KeyId;

    fn random_string(len: usize) -> String {
        iter::repeat(())
            .map(|_| OsRng.sample(Alphanumeric) as char)
            .take(len)
            .collect()
    }

    #[test]
    fn key_id_converts_correctly() {
        let managed_key_id: KeyId<PublicKey> = KeyId::Managed {
            branch: random_string(8),
            index: {
                let mut rng = rand::thread_rng();
                let random_value: u64 = rng.gen();
                random_value
            },
        };
        let imported_key_id: KeyId<PublicKey> = KeyId::Imported {
            key: PublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        };
        let zero_key_id: KeyId<PublicKey> = KeyId::Zero;

        let managed_key_id_str = managed_key_id.to_string();
        let imported_key_id_str = imported_key_id.to_string();
        let zero_key_id_str = zero_key_id.to_string();

        assert_eq!(managed_key_id, KeyId::from_str(&managed_key_id_str).unwrap());
        assert_eq!(imported_key_id, KeyId::from_str(&imported_key_id_str).unwrap());
        assert_eq!(zero_key_id, KeyId::from_str(&zero_key_id_str).unwrap());
    }
}

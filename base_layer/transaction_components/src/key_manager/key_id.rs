//  Copyright 2023, The Tari Project
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

// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::fmt;
use std::str::FromStr;
use serde::{Deserialize, Serialize};
use tari_utilities::hex::{from_hex, Hex};
use tari_common_types::types::CompressedPublicKey;
use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
pub const VIEW_KEY_BRANCH: &str = "view_key";
pub const SPEND_KEY_BRANCH: &str = "spend_key";
pub const DERIVED_KEY_BRANCH: &str = "derived";
pub const ZERO_KEY_BRANCH: &str = "zero";
pub const DH_COMMITMENT_MASK_BRANCH: &str = "dh_commitment_mask";
pub const DH_ENCRYPTED_DATA_BRANCH: &str = "dh_encrypted_data";
pub const ENCRYPTED_BRANCH: &str = "encrypted";
pub const LEDGER_KEY_BRANCH: &str = "ledger_key";

#[derive(Default, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum TariKeyId {
    ViewKey,
    SpendKey,
    Derived {
        key: SerializedKeyString,
    },
    #[default]
    Zero,
    DHCommitmentMask {
        public_key: CompressedPublicKey,
        private_key: SerializedKeyString,
    },
    DHEncryptedData {
        public_key: CompressedPublicKey,
        private_key: SerializedKeyString,
    },
    Encrypted {
        encrypted: Vec<u8>,
        key: SerializedKeyString,
    },
    LedgerKey {
        branch: LedgerKeyBranch,
        index: u64,
    },
}


impl FromStr for TariKeyId {
    type Err = String;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = id.split('.').collect();
        match parts.first() {
            None => Err("Out of bounds".to_string()),
            Some(val) => match *val {
                ZERO_KEY_BRANCH => Ok(TariKeyId::Zero),
                DERIVED_KEY_BRANCH => {
                    if parts.len() < 3 {
                        return Err("Wrong derived format".to_string());
                    };

                    let key = parts.get(1..).expect("Already checked").join(".");
                    Ok(TariKeyId::Derived {
                        key: SerializedKeyString::from(key),
                    })
                },
                DH_COMMITMENT_MASK_BRANCH => {
                    if parts.len() < 3 {
                        return Err("Wrong dh_commitment_mask format".to_string());
                    }
                    let public_key = CompressedPublicKey::from_hex(parts.get(1).expect("Already checked"))
                        .map_err(|_| "Invalid public key".to_string())?;
                    let private_key = parts.get(2..).expect("Already checked").join(".");
                    Ok(TariKeyId::DHCommitmentMask {
                        public_key,
                        private_key: SerializedKeyString::from(private_key),
                    })
                },
                DH_ENCRYPTED_DATA_BRANCH => {
                    if parts.len() < 3 {
                        return Err("Wrong encryted data format".to_string());
                    }
                    let public_key = CompressedPublicKey::from_hex(parts.get(1).expect("Already checked"))
                        .map_err(|_| "Invalid public key".to_string())?;
                    let private_key = parts.get(2..).expect("Already checked").join(".");
                    Ok(TariKeyId::DHEncryptedData {
                        public_key,
                        private_key: SerializedKeyString::from(private_key),
                    })
                },
                ENCRYPTED_BRANCH => {
                    if parts.len() < 3 {
                        return Err("Wrong encrypted format".to_string());
                    }
                    let encrypted: Vec<u8> = from_hex(parts.get(1).expect("Already checked"))
                        .map_err(|_| "Invalid encrypted bytes".to_string())?;
                    let key = parts.get(2..).expect("Already checked").join(".");
                    Ok(TariKeyId::Encrypted {
                        encrypted,
                        key: SerializedKeyString::from(key),
                    })
                },
                SPEND_KEY_BRANCH => {
                    if parts.len() != 1 {
                        return Err("Wrong spend key format".to_string());
                    }
                    Ok(TariKeyId::SpendKey)
                }
                VIEW_KEY_BRANCH => {
                    if parts.len() != 1 {
                        return Err("Wrong view key format".to_string());
                    }
                    Ok(TariKeyId::ViewKey)
                }
                LEDGER_KEY_BRANCH=> {
                    if parts.len() != 3 {
                        return Err("Wrong ledger key format".to_string());
                    }
                    let branch_str = parts.get(1).expect("Already checked");
                    let branch = LedgerKeys::from_str(branch_str)?;
                    let index: u64 = parts
                        .get(2)
                        .expect("Already checked")
                        .parse()
                        .map_err(|_| "Invalid ledger key index".to_string())?;
                    Ok(TariKeyId::LedgerKey { branch, index })
                }
                _ => Err("Wrong generic format".to_string()),
            },
        }
    }
}

impl fmt::Display for TariKeyId {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TariKeyId::Derived { key } => write!(f, "{DERIVED_KEY_BRANCH}.{key}"),
            TariKeyId::Zero => write!(f, "{ZERO_KEY_BRANCH}"),
            TariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => {
                write!(f, "{DH_COMMITMENT_MASK_BRANCH}.{public_key}.{private_key}")
            },
            TariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => {
                write!(f, "{DH_ENCRYPTED_DATA_BRANCH}.{public_key}.{private_key}")
            },
            TariKeyId::Encrypted { encrypted, key } => {
                write!(f, "{ENCRYPTED_BRANCH}.{}.{}", encrypted.to_hex(), key)
            },
            TariKeyId::SpendKey => write!(f, "{SPEND_KEY_BRANCH}"),
            TariKeyId::ViewKey => write!(f, "{VIEW_KEY_BRANCH}"),
            TariKeyId::LedgerKey { branch, index } => {
                write!(f, "{LEDGER_KEY_BRANCH}.{}.{}", branch, index)
            }
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SerializedKeyString {
    inner: String,
}

impl From<String> for SerializedKeyString {
    fn from(inner: String) -> Self {
        Self { inner }
    }
}

impl From<&str> for SerializedKeyString {
    fn from(inner: &str) -> Self {
        Self { inner: inner.into() }
    }
}

impl fmt::Display for SerializedKeyString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

impl From<TariKeyId> for SerializedKeyString {
    fn from(key_id: TariKeyId) -> Self {
        Self::from(key_id.to_string())
    }
}

impl From<&TariKeyId> for SerializedKeyString {
    fn from(key_id: &TariKeyId) -> Self {
        Self::from(key_id.to_string())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TariKeyAndId {
    pub pub_key: CompressedPublicKey,
    pub key_id: TariKeyId,
}


#[cfg(test)]
mod test {
    use core::iter;
    use std::str::FromStr;

    use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
    use tari_common_types::types::{CompressedPublicKey, PrivateKey};
    use tari_crypto::keys::SecretKey as SK;

    use crate::legacy_key_manager::TariKeyId;

    fn random_string(len: usize) -> String {
        iter::repeat(())
            .map(|_| OsRng.sample(Alphanumeric) as char)
            .take(len)
            .collect()
    }

    #[test]
    fn key_id_converts_correctly() {
        let managed_key_id: TariKeyId = TariKeyId::Managed {
            branch: random_string(8) + " " + &random_string(5),
            index: {
                let mut rng = rand::thread_rng();
                let random_value: u64 = rng.gen();
                random_value
            },
        };
        let imported_key_id = TariKeyId::Imported {
            key: CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        };
        let zero_key_id = TariKeyId::Zero;
        let derived_key_id = TariKeyId::Derived {
            key: managed_key_id.clone().into(),
        };

        let dh_commitment_mask_key_id = TariKeyId::DHCommitmentMask {
            public_key: CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            private_key: managed_key_id.clone().into(),
        };

        let derived_key_id2 = TariKeyId::Derived {
            key: dh_commitment_mask_key_id.clone().into(),
        };
        let dh_encrypted_data_key_id = TariKeyId::DHEncryptedData {
            public_key: CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            private_key: managed_key_id.clone().into(),
        };

        let managed_key_id_str = managed_key_id.to_string();
        let imported_key_id_str = imported_key_id.to_string();
        let zero_key_id_str = zero_key_id.to_string();
        let derived_key_id_str = derived_key_id.to_string();
        let derived_key_id_str2 = derived_key_id2.to_string();
        let dh_commitment_mask_key_id_str = dh_commitment_mask_key_id.to_string();
        let dh_encrypted_data_key_id_str = dh_encrypted_data_key_id.to_string();

        assert_eq!(managed_key_id, TariKeyId::from_str(&managed_key_id_str).unwrap());
        assert_eq!(imported_key_id, TariKeyId::from_str(&imported_key_id_str).unwrap());
        assert_eq!(zero_key_id, TariKeyId::from_str(&zero_key_id_str).unwrap());
        assert_eq!(derived_key_id, TariKeyId::from_str(&derived_key_id_str).unwrap());
        assert_eq!(derived_key_id2, TariKeyId::from_str(&derived_key_id_str2).unwrap());
        assert_eq!(
            dh_commitment_mask_key_id,
            TariKeyId::from_str(&dh_commitment_mask_key_id_str).unwrap()
        );
        assert_eq!(
            dh_encrypted_data_key_id,
            TariKeyId::from_str(&dh_encrypted_data_key_id_str).unwrap()
        );
    }
}

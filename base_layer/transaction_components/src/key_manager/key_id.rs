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

use std::{fmt, str::FromStr};

use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
use serde::{Deserialize, Serialize};
use tari_common_types::types::CompressedPublicKey;
use tari_utilities::hex::{from_hex, Hex};
pub const VIEW_KEY_BRANCH: &str = "view_key";
pub const SPEND_KEY_BRANCH: &str = "spend_key";
pub const DERIVED_KEY_BRANCH: &str = "derived";
pub const ZERO_KEY_BRANCH: &str = "zero";
pub const DH_COMMITMENT_MASK_BRANCH: &str = "dh_commitment_mask";
pub const DH_ENCRYPTED_DATA_BRANCH: &str = "dh_encrypted_data";
pub const ENCRYPTED_BRANCH: &str = "encrypted";
pub const LEDGER_KEY_BRANCH: &str = "ledger_key";
pub const CODE_TEMPLATE_AUTHOR: &str = "code-template-author";

#[derive(Default, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum TariKeyId {
    ViewKey,
    SpendKey,
    Derived {
        key: SerializedKeyString,
    },
    CodeTemplateAuthor,
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
                },
                VIEW_KEY_BRANCH => {
                    if parts.len() != 1 {
                        return Err("Wrong view key format".to_string());
                    }
                    Ok(TariKeyId::ViewKey)
                },
                CODE_TEMPLATE_AUTHOR => {
                    if parts.len() != 1 {
                        return Err("Wrong code template format".to_string());
                    }
                    Ok(TariKeyId::CodeTemplateAuthor)
                },
                LEDGER_KEY_BRANCH => {
                    if parts.len() != 3 {
                        return Err("Wrong ledger key format".to_string());
                    }
                    let branch_str = parts.get(1).expect("Already checked");
                    let branch = LedgerKeyBranch::from_str(branch_str)?;
                    let index: u64 = parts
                        .get(2)
                        .expect("Already checked")
                        .parse()
                        .map_err(|_| "Invalid ledger key index".to_string())?;
                    Ok(TariKeyId::LedgerKey { branch, index })
                },
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
            TariKeyId::CodeTemplateAuthor => write!(f, "{CODE_TEMPLATE_AUTHOR}"),
            TariKeyId::LedgerKey { branch, index } => {
                write!(f, "{LEDGER_KEY_BRANCH}.{}.{}", branch, index)
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SerializedKeyString {
    inner: String,
}

impl SerializedKeyString {
    pub fn as_str(&self) -> &str {
        &self.inner
    }
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

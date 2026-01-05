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

/// String prefix used when serializing and parsing a `TariKeyId::ViewKey`.
///
/// Display/parse form: `"view_key"`
///
/// See the `TariKeyId` enum docs for full string encoding rules.
pub const VIEW_KEY_BRANCH: &str = "view_key";
/// String prefix used when serializing and parsing a `TariKeyId::SpendKey`.
///
/// Display/parse form: `"spend_key"`
pub const SPEND_KEY_BRANCH: &str = "spend_key";
/// String prefix for `TariKeyId::Derived` entries.
///
/// Display/parse form: `"derived.<path>"` where `<path>` is an opaque string that
/// may contain dots. For example: `derived.wallet.receive`
pub const DERIVED_KEY_BRANCH: &str = "derived";
/// String prefix for the default zero key `TariKeyId::Zero`.
///
/// Display/parse form: `"zero"`
pub const ZERO_KEY_BRANCH: &str = "zero";
/// String prefix for `TariKeyId::DHCommitmentMask` entries.
///
/// Display/parse form: `"dh_commitment_mask.<pubkey_hex>.<private_key_path>"` where
/// `<pubkey_hex>` is a 32-byte compressed public key encoded as lowercase hex and
/// `<private_key_path>` is an opaque serialized key string (may contain dots).
pub const DH_COMMITMENT_MASK_BRANCH: &str = "dh_commitment_mask";
/// String prefix for `TariKeyId::DHEncryptedData` entries.
///
/// Display/parse form: `"dh_encrypted_data.<pubkey_hex>.<private_key_path>"` where
/// `<pubkey_hex>` is a 32-byte compressed public key encoded as lowercase hex and
/// `<private_key_path>` is an opaque serialized key string (may contain dots).
pub const DH_ENCRYPTED_DATA_BRANCH: &str = "dh_encrypted_data";
/// String prefix for `TariKeyId::Encrypted` entries.
///
/// Display/parse form: `"encrypted.<ciphertext_hex>.<key_path>"` where
/// `<ciphertext_hex>` is arbitrary bytes encoded as hex and `<key_path>` is the
/// underlying key path (may contain dots).
pub const ENCRYPTED_BRANCH: &str = "encrypted";
/// String prefix for `TariKeyId::LedgerKey` entries.
///
/// Display/parse form: `"ledger_key.<branch>.<index>"` where `<branch>` is a
/// value supported by `minotari_ledger_wallet_common::common_types::LedgerKeyBranch`
/// and `<index>` is a non-negative integer.
pub const LEDGER_KEY_BRANCH: &str = "ledger_key";
/// String prefix for the code template author identity.
///
/// Display/parse form: `"code-template-author"`
pub const CODE_TEMPLATE_AUTHOR: &str = "code-template-author";

#[derive(Default, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
/// Identifiers for different logical key types used by Tari components.
///
/// A `TariKeyId` is an enum that captures the purpose and derivation context of a
/// key in the wallet or node. Each variant has a canonical string representation that
/// is used for persistence, logging and inter-component communication. The `Display`
/// and `FromStr` implementations perform a lossless round-trip between the enum and
/// its string form.
///
/// General encoding rules:
/// - Simple variants with no data render as a single token like `"zero"`, `"view_key"`, `"spend_key"`.
/// - Variants with associated data render as `"<branch>.<arg1>[.<arg2>...]"`.
/// - When a variant contains an opaque serialized key path, that path may itself contain dots, and is therefore
///   captured by taking the remainder of the string after the fixed-length prefix.
/// - Hex-encoded fields use lowercase hex in `Display` and accept case-insensitive hex in `FromStr`.
///
/// See individual variants for details and examples.
pub enum TariKeyId {
    /// The deterministic view key used to scan for outputs and decrypt view-related data.
    ///
    /// String form: `view_key`
    ViewKey,
    /// The primary spend key used to authorize spending of funds.
    ///
    /// String form: `spend_key`
    SpendKey,
    /// A key derived from the hash of the private key of the TariKeyId listed.
    ///
    /// This allows grouping or namespacing of keys beyond the predefined variants.
    /// The path can include dots and is serialized as-is after the `derived` branch.
    ///
    /// String form: `derived.<path>` (e.g. `derived.wallet.receive`)
    Derived {
        /// The opaque derived key path/label. May contain dots.
        key: SerializedKeyString,
    },
    /// Identity used to sign or identify code template authors within Tari DAN.
    ///
    /// String form: `code-template-author`
    CodeTemplateAuthor,
    #[default]
    /// The default or zero key identifier. Often used as a placeholder or for
    /// deterministic base derivations.
    ///
    /// String form: `zero`
    Zero,
    /// A key identifier used for constructing Diffie-Hellman commitment masks using the public key listed the private
    /// key of the TariKeyId listed. The corresponding public shared secret is then hashed using a unique hashing
    /// domain to produce the commitment mask.
    ///
    /// String form: `dh_commitment_mask.<pubkey_hex>.<private_key_path>`
    /// - `<pubkey_hex>`: 32-byte compressed public key in hex
    /// - `<private_key_path>`: opaque serialized key string (may contain dots)
    DHCommitmentMask {
        /// The other party's compressed public key (hex in the string form).
        public_key: CompressedPublicKey,
        /// The local private key path used in the DH operation (may contain dots).
        private_key: SerializedKeyString,
    },
    /// A key identifier used for deriving encrypted data keys using Diffie-Hellman using the public key listed the
    /// private key of the TariKeyId listed. The corresponding public shared secret is then hashed using a unique
    /// hashing domain to produce the private key.
    ///
    /// String form: `dh_encrypted_data.<pubkey_hex>.<private_key_path>`
    /// - `<pubkey_hex>`: 32-byte compressed public key in hex
    /// - `<private_key_path>`: opaque serialized key string (may contain dots)
    DHEncryptedData {
        /// The other party's compressed public key (hex in the string form).
        public_key: CompressedPublicKey,
        /// The local private key path used in the DH operation (may contain dots).
        private_key: SerializedKeyString,
    },
    /// A key identifier representing data that is first encrypted, and then linked
    /// to a base key path. This allows the key id to carry encrypted context.
    ///
    /// String form: `encrypted.<ciphertext_hex>.<key_path>`
    Encrypted {
        /// Arbitrary encrypted bytes, hex-encoded in the string form.
        encrypted: Vec<u8>,
        /// The underlying key path/label (may contain dots).
        key: SerializedKeyString,
    },
    /// A key derived from a Ledger hardware wallet branch and index.
    ///
    /// String form: `ledger_key.<branch>.<index>`
    /// where `<branch>` is any `LedgerKeyBranch` variant supported by the Ledger
    /// app and `<index>` is a non-negative integer.
    LedgerKey {
        /// The Ledger key branch (account/path family) to derive from.
        branch: LedgerKeyBranch,
        /// Concrete index within the branch.
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
                    let _check_valid_key = TariKeyId::from_str(&key)?;
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
                    let _check_valid_key = TariKeyId::from_str(&private_key)?;
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
                    let _check_valid_key = TariKeyId::from_str(&private_key)?;
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
                    let _check_valid_key = TariKeyId::from_str(&key)?;
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn display_simple_variants() {
        assert_eq!(TariKeyId::Zero.to_string(), ZERO_KEY_BRANCH);
        assert_eq!(TariKeyId::SpendKey.to_string(), SPEND_KEY_BRANCH);
        assert_eq!(TariKeyId::ViewKey.to_string(), VIEW_KEY_BRANCH);
        assert_eq!(TariKeyId::CodeTemplateAuthor.to_string(), CODE_TEMPLATE_AUTHOR);
    }

    #[test]
    fn parse_simple_variants() {
        assert_eq!(TariKeyId::from_str("zero").unwrap(), TariKeyId::Zero);
        assert_eq!(TariKeyId::from_str("spend_key").unwrap(), TariKeyId::SpendKey);
        assert_eq!(TariKeyId::from_str("view_key").unwrap(), TariKeyId::ViewKey);
        assert_eq!(
            TariKeyId::from_str("code-template-author").unwrap(),
            TariKeyId::CodeTemplateAuthor
        );
    }

    #[test]
    fn roundtrip_derived_with_dots() {
        let s = "derived.ledger_key.MetadataEphemeralNonce.0";
        let parsed = TariKeyId::from_str(s).unwrap();
        assert!(matches!(parsed, TariKeyId::Derived { .. }));
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn roundtrip_dh_commitment_mask() {
        // Use a known-good 32-byte compressed public key hex from repo examples
        let pk = "28e8efe4e5576aac931d358d0f6ace43c55fa9d4186d1d259d1436caa876d5c9";
        let inner = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::MetadataEphemeralNonce,
            index: 0,
        };
        let key = TariKeyId::DHCommitmentMask {
            public_key: CompressedPublicKey::from_hex(pk).unwrap(),
            private_key: SerializedKeyString::from(inner.to_string()),
        };
        let s = key.to_string();
        let parsed = TariKeyId::from_str(&s).unwrap();
        assert!(matches!(parsed, TariKeyId::DHCommitmentMask { .. }));
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn roundtrip_dh_encrypted_data() {
        // Use a known-good 32-byte compressed public key hex from repo examples
        let pk = "5c6bfaceaa1c83fa4482a816b5f82ca3975cb9b61b6e8be4ee8f01c5f1bee5a2";
        let inner = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::MetadataEphemeralNonce,
            index: 0,
        };
        let key = TariKeyId::DHEncryptedData {
            public_key: CompressedPublicKey::from_hex(pk).unwrap(),
            private_key: SerializedKeyString::from(inner.to_string()),
        };
        let s = key.to_string();
        let parsed = TariKeyId::from_str(&s).unwrap();
        assert!(matches!(parsed, TariKeyId::DHEncryptedData { .. }));
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn roundtrip_encrypted() {
        let enc_hex = "deadbeef00cafebabe";
        let inner = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::MetadataEphemeralNonce,
            index: 0,
        };
        let key = TariKeyId::Encrypted {
            encrypted: enc_hex.as_bytes().to_vec(),
            key: SerializedKeyString::from(inner.to_string()),
        };
        let s = key.to_string();
        let parsed = TariKeyId::from_str(&s).unwrap();
        assert!(matches!(parsed, TariKeyId::Encrypted { .. }));
        // Display will use lowercase hex; our enc_hex is already lowercase
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn roundtrip_ledger_key() {
        // Use a valid branch string as per LedgerKeyBranch::from_str implementation
        let s = format!("{branch}.{b}.{i}", branch = LEDGER_KEY_BRANCH, b = "Random", i = 42u64);
        let parsed = TariKeyId::from_str(&s).unwrap();
        assert_eq!(parsed.to_string(), s);
        match parsed {
            TariKeyId::LedgerKey { branch, index } => {
                assert_eq!(branch.to_string(), "Random");
                assert_eq!(index, 42);
            },
            _ => panic!("Expected LedgerKey"),
        }
    }

    #[test]
    fn serialized_key_string_helpers() {
        let k = SerializedKeyString::from("abc.def");
        assert_eq!(k.as_str(), "abc.def");
        assert_eq!(k.to_string(), "abc.def");

        let kid = TariKeyId::Derived {
            key: SerializedKeyString::from("x.y"),
        };
        let sks1 = SerializedKeyString::from(kid.clone());
        let sks2 = SerializedKeyString::from(&kid);
        assert_eq!(sks1, sks2);
        assert_eq!(sks1.to_string(), kid.to_string());
    }

    #[test]
    fn parse_error_cases() {
        // Empty
        assert_eq!(TariKeyId::from_str("").unwrap_err(), "Wrong generic format");
        // Derived must have at least 3 parts
        assert_eq!(
            TariKeyId::from_str("derived.onlytwo").unwrap_err(),
            "Wrong derived format"
        );
        // DHCommitmentMask invalid public key
        assert_eq!(
            TariKeyId::from_str("dh_commitment_mask.nothex.priv").unwrap_err(),
            "Invalid public key"
        );
        // DHEncryptedData invalid public key
        assert_eq!(
            TariKeyId::from_str("dh_encrypted_data.nothex.priv").unwrap_err(),
            "Invalid public key"
        );
        // Encrypted invalid bytes
        assert_eq!(
            TariKeyId::from_str("encrypted.zzz.key").unwrap_err(),
            "Invalid encrypted bytes"
        );
        // Spend/View/CodeTemplate wrong formats
        assert_eq!(
            TariKeyId::from_str("spend_key.extra").unwrap_err(),
            "Wrong spend key format"
        );
        assert_eq!(
            TariKeyId::from_str("view_key.extra").unwrap_err(),
            "Wrong view key format"
        );
        assert_eq!(
            TariKeyId::from_str("code-template-author.extra").unwrap_err(),
            "Wrong code template format"
        );
        // Ledger wrong formats
        assert_eq!(
            TariKeyId::from_str("ledger_key.Random").unwrap_err(),
            "Wrong ledger key format"
        );
        assert_eq!(
            TariKeyId::from_str("ledger_key.Random.notnumber").unwrap_err(),
            "Invalid ledger key index"
        );
        // Unknown branch
        assert_eq!(
            TariKeyId::from_str("unknown.branch").unwrap_err(),
            "Wrong generic format"
        );
    }
}

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
use tari_max_size::{ValidatedDecode, impl_validated_decode};
use tari_utilities::hex::{Hex, from_hex};

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
/// Display/parse form: `"derived.<key_id>"` where `<key_id>` is the string form of another
/// `TariKeyId`, with or without dots. For example: `derived.ledger_key.Random.0`, `derived.spend_key`
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
/// String prefix for `TariKeyId::LedgerEphemeralNonce` entries.
///
/// Display/parse form: `"ledger_ephemeral_nonce.<handle>"` where `<handle>` is a
/// non-negative integer issued by the Ledger device.
pub const LEDGER_EPHEMERAL_NONCE_BRANCH: &str = "ledger_ephemeral_nonce";
/// String prefix for the code template author identity.
///
/// Display/parse form: `"code-template-author"`
pub const CODE_TEMPLATE_AUTHOR: &str = "code-template-author";

#[derive(Default, Clone, Debug, Serialize, Eq, PartialEq, borsh::BorshSerialize)]
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
///
/// The serde and borsh decoders are generated from the [`ValidatedDecode`] implementation below and must not be
/// derived: they accept exactly the values whose string form `FromStr` accepts, so a key id decoded from JSON (for
/// example an offline signing file) can be stored as text and read back.
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
    /// The nested key id is serialized as-is after the `derived` branch; it must itself be a valid key id string,
    /// either a single token or one with dots of its own.
    ///
    /// String form: `derived.<key_id>` (e.g. `derived.ledger_key.Random.0`, `derived.spend_key`)
    Derived {
        /// The string form of the key id this key is derived from. May contain dots.
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
    /// A one-shot nonce that a wallet reserved through `reserve_ephemeral_nonce`, named by the handle the issuer
    /// gave it. On a Ledger wallet the private nonce lives on the device and never crosses the wire; on a software
    /// wallet it lives in the key manager's in-memory store.
    ///
    /// The handle is issued, never chosen: a caller can echo back a handle it was given, but cannot name a nonce it
    /// was never told about. Signing with the nonce consumes it, so a handle is good for exactly one signature -
    /// two signatures over different challenges under one nonce give up the private key that signed them.
    ///
    /// String form: `ledger_ephemeral_nonce.<handle>`
    ///
    /// Note: this variant is appended last on purpose. The enum is `borsh` encoded by variant declaration order, so
    /// inserting it anywhere else would silently renumber every variant after it.
    LedgerEphemeralNonce {
        /// The issuer supplied handle naming the reserved nonce.
        handle: u64,
    },
}

impl TariKeyId {
    pub fn is_ledger_key(&self) -> bool {
        matches!(self, TariKeyId::LedgerKey { .. } | TariKeyId::Derived { .. })
    }
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
                    // The nested key id may be a single token (`derived.spend_key`, which the legacy key
                    // conversion produces) or have parts of its own (`derived.ledger_key.Random.0`)
                    let key = parts.get(1..).unwrap_or_default().join(".");
                    if key.is_empty() {
                        return Err("Wrong derived format".to_string());
                    };
                    check_key_string(&key)?;
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
                    check_key_string(&private_key)?;
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
                    check_key_string(&private_key)?;
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
                    check_key_string(&key)?;
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
                LEDGER_EPHEMERAL_NONCE_BRANCH => {
                    if parts.len() != 2 {
                        return Err("Wrong ledger ephemeral nonce format".to_string());
                    }
                    let handle: u64 = parts
                        .get(1)
                        .expect("Already checked")
                        .parse()
                        .map_err(|_| "Invalid ledger ephemeral nonce handle".to_string())?;
                    Ok(TariKeyId::LedgerEphemeralNonce { handle })
                },
                _ => Err("Wrong generic format".to_string()),
            },
        }
    }
}

/// Checks that `id` is a key id string `TariKeyId::from_str` accepts, including every nested key id, with the same
/// error messages. A nested key id is always the tail of the string, so this walks it in a loop rather than
/// recursing, which keeps a long chain of nested key ids from overflowing the stack.
fn check_key_string(id: &str) -> Result<(), String> {
    let mut rest = id;
    loop {
        // `tail` is everything after the first dot: `FromStr` splits on every dot, so it has `parts.len() >= 3`
        // exactly when `tail` contains a dot, and its nested key (`parts[1..]` joined) is `tail`
        let (branch, tail) = match rest.split_once('.') {
            Some((branch, tail)) => (branch, Some(tail)),
            None => (rest, None),
        };
        rest = match branch {
            ZERO_KEY_BRANCH => return Ok(()),
            DERIVED_KEY_BRANCH => match tail {
                Some(key) if !key.is_empty() => key,
                _ => return Err("Wrong derived format".to_string()),
            },
            DH_COMMITMENT_MASK_BRANCH | DH_ENCRYPTED_DATA_BRANCH => {
                let Some((public_key, private_key)) = tail.and_then(|t| t.split_once('.')) else {
                    return Err(if branch == DH_COMMITMENT_MASK_BRANCH {
                        "Wrong dh_commitment_mask format".to_string()
                    } else {
                        "Wrong encryted data format".to_string()
                    });
                };
                CompressedPublicKey::from_hex(public_key).map_err(|_| "Invalid public key".to_string())?;
                private_key
            },
            ENCRYPTED_BRANCH => {
                let Some((encrypted, key)) = tail.and_then(|t| t.split_once('.')) else {
                    return Err("Wrong encrypted format".to_string());
                };
                from_hex(encrypted).map_err(|_| "Invalid encrypted bytes".to_string())?;
                key
            },
            SPEND_KEY_BRANCH | VIEW_KEY_BRANCH | CODE_TEMPLATE_AUTHOR => {
                if tail.is_some() {
                    return Err(match branch {
                        SPEND_KEY_BRANCH => "Wrong spend key format".to_string(),
                        VIEW_KEY_BRANCH => "Wrong view key format".to_string(),
                        _ => "Wrong code template format".to_string(),
                    });
                }
                return Ok(());
            },
            LEDGER_KEY_BRANCH => {
                let Some((ledger_branch, index)) = tail
                    .and_then(|t| t.split_once('.'))
                    .filter(|(_, index)| !index.contains('.'))
                else {
                    return Err("Wrong ledger key format".to_string());
                };
                LedgerKeyBranch::from_str(ledger_branch)?;
                index
                    .parse::<u64>()
                    .map_err(|_| "Invalid ledger key index".to_string())?;
                return Ok(());
            },
            LEDGER_EPHEMERAL_NONCE_BRANCH => {
                let Some(handle) = tail.filter(|t| !t.contains('.')) else {
                    return Err("Wrong ledger ephemeral nonce format".to_string());
                };
                handle
                    .parse::<u64>()
                    .map_err(|_| "Invalid ledger ephemeral nonce handle".to_string())?;
                return Ok(());
            },
            _ => return Err("Wrong generic format".to_string()),
        };
    }
}

/// The raw form of [`TariKeyId`]: the same variants, fields and order as the derived serde and borsh shapes, decoded
/// before the string form is checked.
#[derive(Deserialize, borsh::BorshDeserialize)]
#[serde(rename = "TariKeyId")]
pub enum TariKeyIdRaw {
    ViewKey,
    SpendKey,
    Derived {
        key: SerializedKeyString,
    },
    CodeTemplateAuthor,
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
    LedgerEphemeralNonce {
        handle: u64,
    },
}

impl ValidatedDecode for TariKeyId {
    type Error = String;
    type Raw = TariKeyIdRaw;

    /// Accepts the key id only if `FromStr` accepts its string form, which is how the wallet stores it.
    fn validate(raw: Self::Raw) -> Result<Self, Self::Error> {
        let key_id = match raw {
            TariKeyIdRaw::ViewKey => TariKeyId::ViewKey,
            TariKeyIdRaw::SpendKey => TariKeyId::SpendKey,
            TariKeyIdRaw::Derived { key } => TariKeyId::Derived { key },
            TariKeyIdRaw::CodeTemplateAuthor => TariKeyId::CodeTemplateAuthor,
            TariKeyIdRaw::Zero => TariKeyId::Zero,
            TariKeyIdRaw::DHCommitmentMask {
                public_key,
                private_key,
            } => TariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            },
            TariKeyIdRaw::DHEncryptedData {
                public_key,
                private_key,
            } => TariKeyId::DHEncryptedData {
                public_key,
                private_key,
            },
            TariKeyIdRaw::Encrypted { encrypted, key } => TariKeyId::Encrypted { encrypted, key },
            TariKeyIdRaw::LedgerKey { branch, index } => TariKeyId::LedgerKey { branch, index },
            TariKeyIdRaw::LedgerEphemeralNonce { handle } => TariKeyId::LedgerEphemeralNonce { handle },
        };
        check_key_string(&key_id.to_string())?;
        Ok(key_id)
    }
}

impl_validated_decode!(TariKeyId);

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
            TariKeyId::LedgerEphemeralNonce { handle } => {
                write!(f, "{LEDGER_EPHEMERAL_NONCE_BRANCH}.{}", handle)
            },
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, borsh::BorshSerialize, borsh::BorshDeserialize)]
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct TariKeyAndId {
    pub pub_key: CompressedPublicKey,
    pub key_id: TariKeyId,
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use borsh::BorshDeserialize;

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
        let s = "derived.ledger_key.Random.0";
        let parsed = TariKeyId::from_str(s).unwrap();
        assert!(matches!(parsed, TariKeyId::Derived { .. }));
        assert_eq!(parsed.to_string(), s);
    }

    #[test]
    fn roundtrip_dh_commitment_mask() {
        // Use a known-good 32-byte compressed public key hex from repo examples
        let pk = "28e8efe4e5576aac931d358d0f6ace43c55fa9d4186d1d259d1436caa876d5c9";
        let inner = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::Random,
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
            branch: LedgerKeyBranch::Random,
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
            branch: LedgerKeyBranch::Random,
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
    fn roundtrip_ledger_ephemeral_nonce() {
        let s = format!("{LEDGER_EPHEMERAL_NONCE_BRANCH}.{}", 7u64);
        let parsed = TariKeyId::from_str(&s).unwrap();
        assert_eq!(parsed, TariKeyId::LedgerEphemeralNonce { handle: 7 });
        assert_eq!(parsed.to_string(), s);

        // A handle may legitimately be any `u64`, including the extremes.
        for handle in [0u64, 1, u64::MAX] {
            let key_id = TariKeyId::LedgerEphemeralNonce { handle };
            assert_eq!(TariKeyId::from_str(&key_id.to_string()).unwrap(), key_id);
        }
    }

    /// The enum is `borsh` encoded by variant declaration order, so appending `LedgerEphemeralNonce` must not have
    /// moved any existing variant. A stored key id that no longer decodes to what it was written as is silent
    /// wallet corruption, so pin the encoding rather than only the round trip.
    #[test]
    fn borsh_roundtrip_and_variant_ordering() {
        let key_ids = [
            TariKeyId::ViewKey,
            TariKeyId::SpendKey,
            TariKeyId::Zero,
            TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::Random,
                index: 42,
            },
            TariKeyId::LedgerEphemeralNonce { handle: 9 },
        ];
        for key_id in &key_ids {
            let bytes = borsh::to_vec(key_id).unwrap();
            let decoded = TariKeyId::try_from_slice(&bytes).unwrap();
            assert_eq!(&decoded, key_id);
        }

        // ViewKey is variant 0 and LedgerKey is variant 8; both must keep their tags.
        assert_eq!(borsh::to_vec(&TariKeyId::ViewKey).unwrap().first().copied(), Some(0u8));
        assert_eq!(
            borsh::to_vec(&TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::Random,
                index: 42
            })
            .unwrap()
            .first()
            .copied(),
            Some(8u8)
        );
        assert_eq!(
            borsh::to_vec(&TariKeyId::LedgerEphemeralNonce { handle: 9 })
                .unwrap()
                .first()
                .copied(),
            Some(9u8)
        );
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
        // Derived must wrap a non-empty, valid key id
        assert_eq!(TariKeyId::from_str("derived").unwrap_err(), "Wrong derived format");
        assert_eq!(TariKeyId::from_str("derived.").unwrap_err(), "Wrong derived format");
        assert_eq!(
            TariKeyId::from_str("derived.onlytwo").unwrap_err(),
            "Wrong generic format"
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
        // Ledger ephemeral nonce wrong formats
        assert_eq!(
            TariKeyId::from_str("ledger_ephemeral_nonce").unwrap_err(),
            "Wrong ledger ephemeral nonce format"
        );
        assert_eq!(
            TariKeyId::from_str("ledger_ephemeral_nonce.1.2").unwrap_err(),
            "Wrong ledger ephemeral nonce format"
        );
        assert_eq!(
            TariKeyId::from_str("ledger_ephemeral_nonce.notnumber").unwrap_err(),
            "Invalid ledger ephemeral nonce handle"
        );
        // Unknown branch
        assert_eq!(
            TariKeyId::from_str("unknown.branch").unwrap_err(),
            "Wrong generic format"
        );
    }

    const PK: &str = "28e8efe4e5576aac931d358d0f6ace43c55fa9d4186d1d259d1436caa876d5c9";

    /// One key id of every variant, all valid
    fn valid_key_ids() -> Vec<TariKeyId> {
        let ledger = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::Random,
            index: 42,
        };
        vec![
            TariKeyId::ViewKey,
            TariKeyId::SpendKey,
            TariKeyId::Derived { key: (&ledger).into() },
            TariKeyId::CodeTemplateAuthor,
            TariKeyId::Zero,
            TariKeyId::DHCommitmentMask {
                public_key: CompressedPublicKey::from_hex(PK).unwrap(),
                private_key: (&ledger).into(),
            },
            TariKeyId::DHEncryptedData {
                public_key: CompressedPublicKey::from_hex(PK).unwrap(),
                private_key: "view_key".into(),
            },
            TariKeyId::Encrypted {
                encrypted: vec![1, 2, 3],
                key: "view_key".into(),
            },
            ledger.clone(),
            TariKeyId::LedgerEphemeralNonce { handle: 9 },
            // `Derived` wrapping a single-token key id, which the legacy key conversion produces (a legacy
            // `derived.managed.comms.0` becomes `derived.spend_key`), also nested in the other wrapping variants
            TariKeyId::Derived {
                key: "spend_key".into(),
            },
            TariKeyId::Derived { key: "view_key".into() },
            TariKeyId::Derived { key: "zero".into() },
            TariKeyId::Derived {
                key: "code-template-author".into(),
            },
            TariKeyId::Derived {
                key: "derived.spend_key".into(),
            },
            TariKeyId::DHCommitmentMask {
                public_key: CompressedPublicKey::from_hex(PK).unwrap(),
                private_key: "derived.spend_key".into(),
            },
            TariKeyId::DHEncryptedData {
                public_key: CompressedPublicKey::from_hex(PK).unwrap(),
                private_key: "derived.view_key".into(),
            },
            TariKeyId::Encrypted {
                encrypted: vec![4, 5],
                key: "derived.zero".into(),
            },
        ]
    }

    fn assert_all_decoders_reject(key_id: &TariKeyId) {
        let json = serde_json::to_string(key_id).unwrap();
        assert!(
            serde_json::from_str::<TariKeyId>(&json).is_err(),
            "serde_json accepted {json}"
        );
        let bincode_bytes = bincode::serialize(key_id).unwrap();
        assert!(
            bincode::deserialize::<TariKeyId>(&bincode_bytes).is_err(),
            "bincode accepted {key_id}"
        );
        let borsh_bytes = borsh::to_vec(key_id).unwrap();
        assert!(
            TariKeyId::try_from_slice(&borsh_bytes).is_err(),
            "borsh accepted {key_id}"
        );
    }

    /// The decoders are generated from `TariKeyIdRaw` rather than derived; the encodings must stay exactly what the
    /// derived decoders read, so stored and exchanged key ids keep decoding.
    #[test]
    fn decoders_keep_the_derived_encodings() {
        let key_id = TariKeyId::DHCommitmentMask {
            public_key: CompressedPublicKey::from_hex(PK).unwrap(),
            private_key: "zero".into(),
        };
        let json = serde_json::to_string(&key_id).unwrap();
        assert_eq!(
            json,
            format!(r#"{{"DHCommitmentMask":{{"public_key":"{PK}","private_key":{{"inner":"zero"}}}}}}"#)
        );
        // Variant 5, the public key behind a u32 length prefix, then the string behind a u32 length prefix
        let mut borsh_bytes = vec![5u8, 32, 0, 0, 0];
        borsh_bytes.extend(hex::decode(PK).unwrap());
        borsh_bytes.extend([4, 0, 0, 0]);
        borsh_bytes.extend(b"zero");
        assert_eq!(borsh::to_vec(&key_id).unwrap(), borsh_bytes);
        let mut bincode_bytes = vec![5u8, 0, 0, 0];
        bincode_bytes.extend(bincode::serialize(&CompressedPublicKey::from_hex(PK).unwrap()).unwrap());
        bincode_bytes.extend([4, 0, 0, 0, 0, 0, 0, 0]);
        bincode_bytes.extend(b"zero");
        assert_eq!(bincode::serialize(&key_id).unwrap(), bincode_bytes);
        assert_eq!(serde_json::to_string(&TariKeyId::Zero).unwrap(), r#""Zero""#);

        for key_id in valid_key_ids() {
            assert_eq!(TariKeyId::from_str(&key_id.to_string()).unwrap(), key_id);
            let json = serde_json::to_string(&key_id).unwrap();
            assert_eq!(serde_json::from_str::<TariKeyId>(&json).unwrap(), key_id);
            let bytes = bincode::serialize(&key_id).unwrap();
            assert_eq!(bincode::deserialize::<TariKeyId>(&bytes).unwrap(), key_id);
            let bytes = borsh::to_vec(&key_id).unwrap();
            assert_eq!(TariKeyId::try_from_slice(&bytes).unwrap(), key_id);
        }
    }

    /// A key id whose string form `FromStr` rejects can not be stored as text and read back, so no decoder may
    /// accept it either.
    #[test]
    fn decoders_reject_what_from_str_rejects() {
        let invalid = [
            TariKeyId::Derived { key: "".into() },
            TariKeyId::Derived { key: "bogus".into() },
            TariKeyId::Derived {
                key: "bogus.key".into(),
            },
            TariKeyId::DHCommitmentMask {
                public_key: CompressedPublicKey::from_hex(PK).unwrap(),
                private_key: "ledger_key.Random.notnumber".into(),
            },
            TariKeyId::DHEncryptedData {
                public_key: CompressedPublicKey::from_hex(PK).unwrap(),
                private_key: "".into(),
            },
            TariKeyId::Encrypted {
                encrypted: vec![1],
                key: "derived.".into(),
            },
        ];
        for key_id in &invalid {
            assert!(TariKeyId::from_str(&key_id.to_string()).is_err(), "{key_id}");
            assert_all_decoders_reject(key_id);
        }
    }

    #[test]
    fn check_key_string_matches_from_str() {
        let mut samples: Vec<String> = valid_key_ids().iter().map(|k| k.to_string()).collect();
        samples.extend(
            [
                "",
                ".",
                "zero.anything",
                "derived",
                "derived.",
                "derived.zero",
                "derived.zero.x",
                "derived.view_key.x",
                "derived.ledger_key.Random.1",
                "derived.ledger_key.Random.1.2",
                "derived.ledger_key.Bogus.1",
                "dh_commitment_mask",
                "dh_commitment_mask.nothex.zero",
                "dh_encrypted_data.ab",
                "encrypted..zero",
                "encrypted.zz.zero",
                "encrypted.0102",
                "spend_key.",
                "view_key",
                "ledger_key.Random",
                "ledger_ephemeral_nonce.",
                "ledger_ephemeral_nonce.-1",
                "ledger_ephemeral_nonce.18446744073709551616",
            ]
            .iter()
            .map(|s| s.to_string()),
        );
        samples.push(format!("dh_commitment_mask.{PK}.derived.zero"));
        samples.push(format!("dh_commitment_mask.{PK}.derived.encrypted.00.zero"));
        samples.push(format!("dh_encrypted_data.{PK}.spend_key.x"));
        for sample in &samples {
            assert_eq!(
                check_key_string(sample),
                TariKeyId::from_str(sample).map(|_| ()),
                "{sample:?}"
            );
        }
    }

    /// Key ids nest through their string form; neither `FromStr` nor the decoders recurse, so a long chain can not
    /// overflow the stack.
    #[test]
    fn deeply_nested_key_ids_do_not_overflow_the_stack() {
        let depth = 200_000;
        let valid = format!("{}ledger_key.Random.0", "derived.".repeat(depth));
        assert!(TariKeyId::from_str(&valid).is_ok());
        let key_id = TariKeyId::Derived {
            key: valid.strip_prefix("derived.").unwrap().into(),
        };
        let json = serde_json::to_string(&key_id).unwrap();
        assert_eq!(serde_json::from_str::<TariKeyId>(&json).unwrap(), key_id);

        let invalid = format!("{}bogus", "derived.".repeat(depth));
        assert!(TariKeyId::from_str(&invalid).is_err());
        assert_all_decoders_reject(&TariKeyId::Derived {
            key: invalid.strip_prefix("derived.").unwrap().into(),
        });
    }
}

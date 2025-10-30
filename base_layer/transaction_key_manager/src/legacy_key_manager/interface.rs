// Copyright 2023 The Tari Project
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

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use tari_common_types::{
    tari_address::TariAddress,
    types::{
        ComAndPubSignature,
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        PrivateKey,
        RangeProof,
        WalletMessageSchnorrSignature,
    },
    WALLET_COMMS_AND_SPEND_KEY_BRANCH,
};
use tari_script::{CompressedCheckSigSchnorrSignature, TariScript};
use tari_utilities::hex::{from_hex, Hex};

pub const MANAGED_KEY_BRANCH: &str = "managed";
pub const DERIVED_KEY_BRANCH: &str = "derived";
pub const IMPORTED_KEY_BRANCH: &str = "imported";
pub const ZERO_KEY_BRANCH: &str = "zero";
pub const DH_COMMITMENT_MASK_BRANCH: &str = "dh_commitment_mask";
pub const DH_ENCRYPTED_DATA_BRANCH: &str = "dh_encrypted_data";
pub const ENCRYPTED_BRANCH: &str = "encrypted";
use tari_transaction_components::{
    key_manager::{error::KeyManagerError, TxoStage},
    transaction_components::{
        EncryptedData,
        KernelFeatures,
        MemoField,
        RangeProofType,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutputVersion,
    },
    MicroMinotari,
};

use crate::legacy_key_manager::error::KeyManagerStorageError;

#[repr(u8)]
#[derive(Clone, Copy, EnumIter)]
pub enum KeyManagerBranch {
    Comms,
}

impl KeyManagerBranch {
    /// Warning: Changing these strings will affect the backwards compatibility of the wallet with older databases or
    /// recovery.
    pub fn get_branch_key(self) -> String {
        match self {
            KeyManagerBranch::Comms => WALLET_COMMS_AND_SPEND_KEY_BRANCH.to_string(),
        }
    }
}

/// TariKeyId Variants and Private Key Calculation
// 1. Managed { branch, index }
// Description: Represents a key derived from a deterministic key manager using a specific branch and index.
// Private Key Calculation:
// The private key is deterministically derived using the key manager's master seed, the branch string, and the index.
// Formula: private_key = derive(master_seed, branch, index)
// The derivation uses a cryptographic key derivation function (KDF) such as HKDF or similar, ensuring that the same
// inputs always produce the same private key.
// 2. Derived { key }
// Description: Represents a key derived from a serialized key string.
// Private Key Calculation:
// The serialized key string encodes the derivation path or method. The key manager parses this string and applies the
// appropriate derivation logic to reconstruct the private key.
// 3. Imported { key }
// Description: Represents a key that was imported directly.
// Private Key Calculation:
// The private key is stored in the key manager's backend, associated with the given public key.
// Retrieval: The key manager looks up the private key using the public key.
// 4. Zero
// Description: Represents a special zero key.
// Private Key Calculation:
// The private key is a constant value, typically all zeros, and is not used for real cryptographic operations.
// 5. DHCommitmentMask { public_key, private_key } and DHEncryptedData { public_key, private_key }
// Description: Used for Diffie-Hellman operations, storing both a public and a serialized private key.
// Private Key Calculation:
// The private key is reconstructed from the serialized string, which may represent a derived or imported key.
// 6. Encrypted { encrypted, key }
// Description: Represents a key that is encrypted.
// Private Key Calculation:
// The encrypted bytes are decrypted using the provided key string, which is used as a decryption key or derivation
// path.
#[derive(Default, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum LegacyTariKeyId {
    Managed {
        branch: String,
        index: u64,
    },
    Derived {
        key: LegacySerializedKeyString,
    },
    Imported {
        key: CompressedPublicKey,
    },
    #[default]
    Zero,
    DHCommitmentMask {
        public_key: CompressedPublicKey,
        private_key: LegacySerializedKeyString,
    },
    DHEncryptedData {
        public_key: CompressedPublicKey,
        private_key: LegacySerializedKeyString,
    },
    Encrypted {
        encrypted: Vec<u8>,
        key: LegacySerializedKeyString,
    },
}

impl LegacyTariKeyId {
    pub fn managed_index(&self) -> Option<u64> {
        match self {
            LegacyTariKeyId::Managed { index, .. } => Some(*index),
            LegacyTariKeyId::Derived { .. } => None,
            LegacyTariKeyId::Imported { .. } => None,
            LegacyTariKeyId::Zero => None,
            LegacyTariKeyId::DHCommitmentMask { .. } => None,
            LegacyTariKeyId::DHEncryptedData { .. } => None,
            LegacyTariKeyId::Encrypted { .. } => None,
        }
    }

    pub fn managed_branch(&self) -> Option<String> {
        match self {
            LegacyTariKeyId::Managed { branch, .. } => Some(branch.clone()),
            LegacyTariKeyId::Derived { .. } => None,
            LegacyTariKeyId::Imported { .. } => None,
            LegacyTariKeyId::Zero => None,
            LegacyTariKeyId::DHCommitmentMask { .. } => None,
            LegacyTariKeyId::DHEncryptedData { .. } => None,
            LegacyTariKeyId::Encrypted { .. } => None,
        }
    }

    pub fn imported(&self) -> Option<CompressedPublicKey> {
        match self {
            LegacyTariKeyId::Managed { .. } => None,
            LegacyTariKeyId::Derived { .. } => None,
            LegacyTariKeyId::Imported { key } => Some(key.clone()),
            LegacyTariKeyId::Zero => None,
            LegacyTariKeyId::DHCommitmentMask { .. } => None,
            LegacyTariKeyId::DHEncryptedData { .. } => None,
            LegacyTariKeyId::Encrypted { .. } => None,
        }
    }
}

impl FromStr for LegacyTariKeyId {
    type Err = String;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = id.split('.').collect();
        match parts.first() {
            None => Err("Out of bounds".to_string()),
            Some(val) => match *val {
                MANAGED_KEY_BRANCH => {
                    if parts.len() != 3 {
                        return Err("Wrong managed format".to_string());
                    }
                    let index = parts
                        .get(2)
                        .expect("Already checked")
                        .parse()
                        .map_err(|_| "Index for default, invalid u64".to_string())?;
                    Ok(LegacyTariKeyId::Managed {
                        branch: (*parts.get(1).expect("Already checked")).into(),
                        index,
                    })
                },
                IMPORTED_KEY_BRANCH => {
                    if parts.len() != 2 {
                        return Err("Wrong imported format".to_string());
                    }
                    let key = CompressedPublicKey::from_hex(parts.get(1).expect("Already checked"))
                        .map_err(|_| "Invalid public key".to_string())?;
                    Ok(LegacyTariKeyId::Imported { key })
                },
                ZERO_KEY_BRANCH => Ok(LegacyTariKeyId::Zero),
                DERIVED_KEY_BRANCH => {
                    if parts.len() < 3 {
                        return Err("Wrong derived format".to_string());
                    };

                    let key = parts.get(1..).expect("Already checked").join(".");
                    Ok(LegacyTariKeyId::Derived {
                        key: LegacySerializedKeyString::from(key),
                    })
                },
                DH_COMMITMENT_MASK_BRANCH => {
                    if parts.len() < 3 {
                        return Err("Wrong dh_commitment_mask format".to_string());
                    }
                    let public_key = CompressedPublicKey::from_hex(parts.get(1).expect("Already checked"))
                        .map_err(|_| "Invalid public key".to_string())?;
                    let private_key = parts.get(2..).expect("Already checked").join(".");
                    Ok(LegacyTariKeyId::DHCommitmentMask {
                        public_key,
                        private_key: LegacySerializedKeyString::from(private_key),
                    })
                },
                DH_ENCRYPTED_DATA_BRANCH => {
                    if parts.len() < 3 {
                        return Err("Wrong encryted data format".to_string());
                    }
                    let public_key = CompressedPublicKey::from_hex(parts.get(1).expect("Already checked"))
                        .map_err(|_| "Invalid public key".to_string())?;
                    let private_key = parts.get(2..).expect("Already checked").join(".");
                    Ok(LegacyTariKeyId::DHEncryptedData {
                        public_key,
                        private_key: LegacySerializedKeyString::from(private_key),
                    })
                },
                ENCRYPTED_BRANCH => {
                    if parts.len() < 3 {
                        return Err("Wrong encrypted format".to_string());
                    }
                    let encrypted: Vec<u8> = from_hex(parts.get(1).expect("Already checked"))
                        .map_err(|_| "Invalid encrypted bytes".to_string())?;
                    let key = parts.get(2..).expect("Already checked").join(".");
                    Ok(LegacyTariKeyId::Encrypted {
                        encrypted,
                        key: LegacySerializedKeyString::from(key),
                    })
                },
                _ => Err("Wrong generic format".to_string()),
            },
        }
    }
}

impl fmt::Display for LegacyTariKeyId {
    // This trait requires `fmt` with this exact signature.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LegacyTariKeyId::Managed { branch, index } => write!(f, "{MANAGED_KEY_BRANCH}.{branch}.{index}"),
            LegacyTariKeyId::Derived { key } => write!(f, "{DERIVED_KEY_BRANCH}.{key}"),
            LegacyTariKeyId::Imported { key: public_key } => write!(f, "{IMPORTED_KEY_BRANCH}.{public_key}"),
            LegacyTariKeyId::Zero => write!(f, "{ZERO_KEY_BRANCH}"),
            LegacyTariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => {
                write!(f, "{DH_COMMITMENT_MASK_BRANCH}.{public_key}.{private_key}")
            },
            LegacyTariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => {
                write!(f, "{DH_ENCRYPTED_DATA_BRANCH}.{public_key}.{private_key}")
            },
            LegacyTariKeyId::Encrypted { encrypted, key } => {
                write!(f, "{ENCRYPTED_BRANCH}.{}.{}", encrypted.to_hex(), key)
            },
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct LegacyTariKeyAndId {
    pub pub_key: CompressedPublicKey,
    pub key_id: LegacyTariKeyId,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LegacySerializedKeyString {
    inner: String,
}

impl LegacySerializedKeyString {
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

impl From<String> for LegacySerializedKeyString {
    fn from(inner: String) -> Self {
        Self { inner }
    }
}

impl From<&str> for LegacySerializedKeyString {
    fn from(inner: &str) -> Self {
        Self { inner: inner.into() }
    }
}

impl fmt::Display for LegacySerializedKeyString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

// impl From<LegacyTariKeyId> for LegacySerializedKeyString {
//     fn from(key_id: LegacyTariKeyId) -> Self {
//         Self::from(key_id.to_string())
//     }
// }
//
// impl From<&LegacyTariKeyId> for LegacySerializedKeyString {
//     fn from(key_id: &LegacyTariKeyId) -> Self {
//         Self::from(key_id.to_string())
//     }
// }

pub trait LegacyTransactionKeyManagerInterface: Clone + Send + Sync + 'static {
    /// Gets a randomly generated key, which the key manager will manage
    fn get_random_key(&self) -> Result<LegacyTariKeyAndId, KeyManagerError>;

    /// Gets the key id at the specified index
    fn get_public_key_at_key_id(&self, key_id: &LegacyTariKeyId) -> Result<CompressedPublicKey, KeyManagerError>;

    /// Add a new key to be tracked
    fn import_key(
        &self,
        private_key: PrivateKey,
        encryption_key: Option<LegacyTariKeyId>,
    ) -> Result<LegacyTariKeyId, KeyManagerError>;

    /// Gets the pedersen commitment for the specified index
    fn get_commitment(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: &PrivateKey,
    ) -> Result<CompressedCommitment, KeyManagerError>;

    fn verify_mask(
        &self,
        commitment: &CompressedCommitment,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerError>;

    fn get_view_key(&self) -> Result<LegacyTariKeyAndId, KeyManagerError>;

    fn get_private_view_key(&self) -> PrivateKey;

    fn get_spend_key(&self) -> Result<LegacyTariKeyAndId, KeyManagerError>;

    fn get_next_commitment_mask_and_script_key(
        &mut self,
    ) -> Result<(LegacyTariKeyAndId, LegacyTariKeyAndId), KeyManagerError>;

    fn find_script_key_id_from_commitment_mask_key_id(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        public_script_key: Option<&CompressedPublicKey>,
    ) -> Result<Option<LegacyTariKeyId>, KeyManagerError>;

    fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &LegacyTariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, KeyManagerError>;

    fn construct_range_proof(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, KeyManagerError>;

    fn get_script_signature(
        &self,
        script_key_id: &LegacyTariKeyId,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError>;

    fn get_partial_script_signature(
        &self,
        commitment_mask_id: &LegacyTariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        ephemeral_pubkey: &CompressedPublicKey,
        script_public_key: &CompressedPublicKey,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError>;

    fn get_partial_txo_kernel_signature(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        nonce_id: &LegacyTariKeyId,
        total_nonce: &CompressedPublicKey,
        total_excess: &CompressedPublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
        kernel_features: &KernelFeatures,
        txo_type: TxoStage,
    ) -> Result<CompressedSignature, KeyManagerError>;

    fn get_txo_kernel_signature_excess_with_offset(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        nonce: &LegacyTariKeyId,
    ) -> Result<CompressedPublicKey, KeyManagerError>;

    fn get_txo_private_kernel_offset(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        nonce_id: &LegacyTariKeyId,
    ) -> Result<PrivateKey, KeyManagerError>;

    fn encrypt_data_for_recovery(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        custom_recovery_key_id: Option<&LegacyTariKeyId>,
        value: u64,
        payment_id: MemoField,
    ) -> Result<EncryptedData, KeyManagerError>;

    fn extract_payment_id_from_encrypted_data(
        &self,
        encrypted_data: &EncryptedData,
        commitment: &CompressedCommitment,
        custom_recovery_key_id: Option<&LegacyTariKeyId>,
    ) -> Result<MemoField, KeyManagerError>;

    fn try_output_key_recovery(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        sender_offset_public_key: &CompressedPublicKey,
    ) -> Result<Option<(LegacyTariKeyId, MicroMinotari, MemoField)>, KeyManagerError>;

    fn is_this_output_ours(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        custom_recovery_key_id: Option<PrivateKey>,
    ) -> Result<bool, KeyManagerError>;

    fn get_script_offset(
        &self,
        script_key_ids: &[LegacyTariKeyId],
        sender_offset_key_ids: &[LegacyTariKeyId],
    ) -> Result<PrivateKey, KeyManagerError>;

    // Look into perhaps removing all nonce here, if the signer and receiver are the same it should not be required to
    // share or pre calc the nonces
    fn get_metadata_signature(
        &mut self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &LegacyTariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, KeyManagerError>;

    fn get_one_sided_metadata_signature(
        &mut self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: MicroMinotari,
        sender_offset_key_id: &LegacyTariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message_common: &[u8; 32],
        range_proof_type: RangeProofType,
        script: &TariScript,
        receiver_address: &TariAddress,
    ) -> Result<ComAndPubSignature, KeyManagerError>;

    fn sign_message_with_spend_key(
        &self,
        message: &[u8],
        sender_offset_key: Option<&CompressedPublicKey>,
    ) -> Result<WalletMessageSchnorrSignature, KeyManagerError>;

    fn sign_script_message(
        &self,
        private_key_id: &LegacyTariKeyId,
        challenge: &[u8],
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerError>;

    fn sign_script_message_with_spend_key(
        &self,
        message: &[u8],
        sender_offset_pub_key: Option<&CompressedPublicKey>,
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerError>;

    fn sign_with_nonce_and_challenge(
        &self,
        private_key_id: &LegacyTariKeyId,
        nonce: &LegacyTariKeyId,
        challenge: &[u8; 64],
    ) -> Result<CompressedSignature, KeyManagerError>;

    fn get_receiver_partial_metadata_signature(
        &mut self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &CompressedPublicKey,
        ephemeral_pubkey: &CompressedPublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, KeyManagerError>;

    // In the case where the sender is an aggregated signer, we need to parse in the other public key shares, this is
    // done in: aggregated_sender_offset_public_keys and aggregated_ephemeral_public_keys. If there is no aggregated
    // signers, this can be left as none
    fn get_sender_partial_metadata_signature(
        &self,
        ephemeral_private_nonce_id: &LegacyTariKeyId,
        sender_offset_key_id: &LegacyTariKeyId,
        commitment: &CompressedCommitment,
        ephemeral_commitment: &CompressedCommitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError>;

    fn generate_burn_claim_signature(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        amount: u64,
        claim_public_key: &CompressedPublicKey,
    ) -> Result<CompressedSignature, KeyManagerError>;

    fn stealth_address_script_spending_key(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        spend_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, KeyManagerError>;
}

pub trait LegacySecretTransactionKeyManagerInterface: LegacyTransactionKeyManagerInterface {
    /// Gets the pedersen commitment for the specified index
    fn get_private_key(&self, key_id: &LegacyTariKeyId) -> Result<PrivateKey, KeyManagerError>;
}

/// This trait defines the required behaviour that a storage backend must provide for the Key Manager service.
#[async_trait::async_trait]
pub trait TransactionKeyManagerBackend: Clone + Send + Sync {
    /// This will retrieve the key manager specified by the branch string, None is returned if the key manager is not
    /// found for the branch.
    async fn get_key_manager(&self, branch: &str) -> Result<Option<KeyManagerState>, KeyManagerStorageError>;
    /// This will add an additional branch for the key manager to track.
    async fn add_key_manager(&self, key_manager: KeyManagerState) -> Result<(), KeyManagerStorageError>;
    /// This will increase the key index of the specified branch, and returns an error if the branch does not exist.
    async fn increment_key_index(&self, branch: &str) -> Result<(), KeyManagerStorageError>;
    /// This method will set the currently stored key index for the key manager.
    async fn set_key_index(&self, branch: &str, index: u64) -> Result<(), KeyManagerStorageError>;
    /// This method will import a new public private key pair into the database
    async fn insert_imported_key(
        &self,
        public_key: CompressedPublicKey,
        private_key: PrivateKey,
    ) -> Result<(), KeyManagerStorageError>;
    /// This method will retrieve  public private key pair from the database
    async fn get_imported_key(&self, public_key: &CompressedPublicKey) -> Result<PrivateKey, KeyManagerStorageError>;
}

/// Holds the state of the KeyManager for the branch
#[derive(Clone, Debug, PartialEq)]
pub struct KeyManagerState {
    pub branch_seed: String,
    pub primary_key_index: u64,
}

#[cfg(test)]
mod test {
    use core::iter;
    use std::str::FromStr;

    use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
    use tari_common_types::types::{CompressedPublicKey, PrivateKey};
    use tari_crypto::keys::SecretKey as SK;

    use crate::legacy_key_manager::LegacyTariKeyId;

    fn random_string(len: usize) -> String {
        iter::repeat(())
            .map(|_| OsRng.sample(Alphanumeric) as char)
            .take(len)
            .collect()
    }

    #[test]
    fn key_id_converts_correctly() {
        let managed_key_id: LegacyTariKeyId = LegacyTariKeyId::Managed {
            branch: random_string(8) + " " + &random_string(5),
            index: {
                let mut rng = rand::thread_rng();
                let random_value: u64 = rng.gen();
                random_value
            },
        };
        let imported_key_id = LegacyTariKeyId::Imported {
            key: CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
        };
        let zero_key_id = LegacyTariKeyId::Zero;
        let derived_key_id = LegacyTariKeyId::Derived {
            key: managed_key_id.clone().into(),
        };

        let dh_commitment_mask_key_id = LegacyTariKeyId::DHCommitmentMask {
            public_key: CompressedPublicKey::from_secret_key(&PrivateKey::random(&mut OsRng)),
            private_key: managed_key_id.clone().into(),
        };

        let derived_key_id2 = LegacyTariKeyId::Derived {
            key: dh_commitment_mask_key_id.clone().into(),
        };
        let dh_encrypted_data_key_id = LegacyTariKeyId::DHEncryptedData {
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

        assert_eq!(managed_key_id, LegacyTariKeyId::from_str(&managed_key_id_str).unwrap());
        assert_eq!(
            imported_key_id,
            LegacyTariKeyId::from_str(&imported_key_id_str).unwrap()
        );
        assert_eq!(zero_key_id, LegacyTariKeyId::from_str(&zero_key_id_str).unwrap());
        assert_eq!(derived_key_id, LegacyTariKeyId::from_str(&derived_key_id_str).unwrap());
        assert_eq!(
            derived_key_id2,
            LegacyTariKeyId::from_str(&derived_key_id_str2).unwrap()
        );
        assert_eq!(
            dh_commitment_mask_key_id,
            LegacyTariKeyId::from_str(&dh_commitment_mask_key_id_str).unwrap()
        );
        assert_eq!(
            dh_encrypted_data_key_id,
            LegacyTariKeyId::from_str(&dh_encrypted_data_key_id_str).unwrap()
        );
    }
}

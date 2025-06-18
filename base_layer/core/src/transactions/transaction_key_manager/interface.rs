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

use blake2::Blake2b;
use digest::consts::U64;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;
use tari_common_types::{
    tari_address::TariAddress,
    types::{ComAndPubSignature, CompressedCommitment, CompressedPublicKey, PrivateKey, RangeProof, Signature},
};
use tari_comms::types::CommsDHKE;
use tari_crypto::{hashing::DomainSeparatedHash, ristretto::RistrettoComSig};
use tari_key_manager::key_manager_service::AddResult;
use tari_script::{CompressedCheckSigSchnorrSignature, TariScript};
use tari_utilities::hex::Hex;

pub const MANAGED_KEY_BRANCH: &str = "managed";
pub const DERIVED_KEY_BRANCH: &str = "derived";
pub const IMPORTED_KEY_BRANCH: &str = "imported";
pub const ZERO_KEY_BRANCH: &str = "zero";

use crate::transactions::{
    tari_amount::MicroMinotari,
    transaction_components::{
        payment_id::PaymentId,
        EncryptedData,
        KernelFeatures,
        RangeProofType,
        TransactionError,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
    },
    transaction_key_manager::error::KeyManagerServiceError,
};

#[derive(Default, Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum TariKeyId {
    Managed {
        branch: String,
        index: u64,
    },
    Derived {
        key: SerializedKeyString,
    },
    Imported {
        key: CompressedPublicKey,
    },
    #[default]
    Zero,
}

impl TariKeyId {
    pub fn managed_index(&self) -> Option<u64> {
        match self {
            TariKeyId::Managed { index, .. } => Some(*index),
            TariKeyId::Derived { .. } => None,
            TariKeyId::Imported { .. } => None,
            TariKeyId::Zero => None,
        }
    }

    pub fn managed_branch(&self) -> Option<String> {
        match self {
            TariKeyId::Managed { branch, .. } => Some(branch.clone()),
            TariKeyId::Derived { .. } => None,
            TariKeyId::Imported { .. } => None,
            TariKeyId::Zero => None,
        }
    }

    pub fn imported(&self) -> Option<CompressedPublicKey> {
        match self {
            TariKeyId::Managed { .. } => None,
            TariKeyId::Derived { .. } => None,
            TariKeyId::Imported { key } => Some(key.clone()),
            TariKeyId::Zero => None,
        }
    }
}

impl FromStr for TariKeyId {
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
                    let index = parts[2]
                        .parse()
                        .map_err(|_| "Index for default, invalid u64".to_string())?;
                    Ok(TariKeyId::Managed {
                        branch: parts[1].into(),
                        index,
                    })
                },
                IMPORTED_KEY_BRANCH => {
                    if parts.len() != 2 {
                        return Err("Wrong imported format".to_string());
                    }
                    let key = CompressedPublicKey::from_hex(parts[1]).map_err(|_| "Invalid public key".to_string())?;
                    Ok(TariKeyId::Imported { key })
                },
                ZERO_KEY_BRANCH => Ok(TariKeyId::Zero),
                DERIVED_KEY_BRANCH => {
                    match parts.len() {
                        4 | 3 => (),
                        _ => return Err("Wrong derived format".to_string()),
                    }

                    let key = parts[1..].join(".");
                    Ok(TariKeyId::Derived {
                        key: SerializedKeyString::from(key),
                    })
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
            TariKeyId::Managed { branch: b, index: i } => write!(f, "{}.{}.{}", MANAGED_KEY_BRANCH, b, i),
            TariKeyId::Derived { key: k } => write!(f, "{}.{}", DERIVED_KEY_BRANCH, k),
            TariKeyId::Imported { key: public_key } => write!(f, "{}.{}", IMPORTED_KEY_BRANCH, public_key.to_hex()),
            TariKeyId::Zero => write!(f, "{}", ZERO_KEY_BRANCH),
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct TariKeyAndId {
    pub pub_key: CompressedPublicKey,
    pub key_id: TariKeyId,
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

#[derive(Clone, Copy, PartialEq)]
pub enum TxoStage {
    Input,
    Output,
}

#[derive(Clone, Copy, EnumIter)]
pub enum TransactionKeyManagerLabel {
    ScriptKey,
}

impl TransactionKeyManagerLabel {
    /// Warning: Changing these strings will affect the backwards compatibility of the wallet with older databases or
    /// recovery.
    pub fn get_branch_key(self) -> String {
        match self {
            TransactionKeyManagerLabel::ScriptKey => "script key".to_string(),
        }
    }
}

impl FromStr for TransactionKeyManagerLabel {
    type Err = String;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        match id {
            "script key" => Ok(TransactionKeyManagerLabel::ScriptKey),
            _ => Err("Unknown label".to_string()),
        }
    }
}

#[async_trait::async_trait]
pub trait TransactionKeyManagerInterface: Clone + Send + Sync + 'static {
    /// Creates a new branch for the key manager service to track
    /// If this is an existing branch, that is not yet tracked in memory, the key manager service will load the key
    /// manager from the backend to track in memory, will return `Ok(AddResult::NewEntry)`. If the branch is already
    /// tracked in memory the result will be `Ok(AddResult::AlreadyExists)`. If the branch does not exist in memory
    /// or in the backend, a new branch will be created and tracked the backend, `Ok(AddResult::NewEntry)`.
    async fn add_new_branch<T: Into<String> + Send>(&self, branch: T) -> Result<AddResult, KeyManagerServiceError>;

    /// Gets the next key id from the branch. This will auto-increment the branch key index by 1
    async fn get_next_key<T: Into<String> + Send>(&self, branch: T) -> Result<TariKeyAndId, KeyManagerServiceError>;

    /// Gets a randomly generated key, which the key manager will manage
    async fn get_random_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError>;

    /// Gets the fixed key id from the branch. This will use the branch key with index 0
    async fn get_static_key<T: Into<String> + Send>(&self, branch: T) -> Result<TariKeyId, KeyManagerServiceError>;

    /// Gets the key id at the specified index
    async fn get_public_key_at_key_id(&self, key_id: &TariKeyId)
        -> Result<CompressedPublicKey, KeyManagerServiceError>;

    /// Searches the branch to find the index used to generated the key, O(N) where N = index used.
    async fn find_key_index<T: Into<String> + Send>(
        &self,
        branch: T,
        key: &CompressedPublicKey,
    ) -> Result<u64, KeyManagerServiceError>;

    /// Will update the index of the branch if the index given is higher than the current saved index
    async fn update_current_key_index_if_higher<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<(), KeyManagerServiceError>;

    /// Add a new key to be tracked
    async fn import_key(&self, private_key: PrivateKey) -> Result<TariKeyId, KeyManagerServiceError>;

    /// Gets the pedersen commitment for the specified index
    async fn get_commitment(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
    ) -> Result<CompressedCommitment, KeyManagerServiceError>;

    async fn verify_mask(
        &self,
        commitment: &CompressedCommitment,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError>;

    async fn get_view_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError>;

    async fn get_private_view_key(&self) -> Result<PrivateKey, KeyManagerServiceError>;

    async fn get_spend_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError>;

    async fn get_comms_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError>;

    async fn get_next_commitment_mask_and_script_key(
        &self,
    ) -> Result<(TariKeyAndId, TariKeyAndId), KeyManagerServiceError>;

    async fn find_script_key_id_from_commitment_mask_key_id(
        &self,
        commitment_mask_key_id: &TariKeyId,
        public_script_key: Option<&CompressedPublicKey>,
    ) -> Result<Option<TariKeyId>, KeyManagerServiceError>;

    async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<CommsDHKE, TransactionError>;

    async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<DomainSeparatedHash<Blake2b<U64>>, TransactionError>;

    async fn get_spending_key_id(
        &self,
        public_spending_key: &CompressedPublicKey,
    ) -> Result<TariKeyId, TransactionError>;

    async fn construct_range_proof(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError>;

    async fn get_script_signature(
        &self,
        script_key_id: &TariKeyId,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_partial_script_signature(
        &self,
        commitment_mask_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        ephemeral_pubkey: &CompressedPublicKey,
        script_public_key: &CompressedPublicKey,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_partial_txo_kernel_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
        total_nonce: &CompressedPublicKey,
        total_excess: &CompressedPublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
        kernel_features: &KernelFeatures,
        txo_type: TxoStage,
    ) -> Result<Signature, TransactionError>;

    async fn get_txo_kernel_signature_excess_with_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce: &TariKeyId,
    ) -> Result<CompressedPublicKey, TransactionError>;

    async fn get_txo_private_kernel_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, TransactionError>;

    async fn encrypt_data_for_recovery(
        &self,
        commitment_mask_key_id: &TariKeyId,
        custom_recovery_key_id: Option<&TariKeyId>,
        value: u64,
        payment_id: PaymentId,
    ) -> Result<EncryptedData, TransactionError>;

    async fn extract_payment_id_from_encrypted_data(
        &self,
        encrypted_data: &EncryptedData,
        commitment: &CompressedCommitment,
        custom_recovery_key_id: Option<&TariKeyId>,
    ) -> Result<PaymentId, TransactionError>;

    async fn try_output_key_recovery(
        &self,
        output: &TransactionOutput,
        custom_recovery_key_id: Option<&TariKeyId>,
    ) -> Result<(TariKeyId, MicroMinotari, PaymentId), TransactionError>;

    async fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
    ) -> Result<PrivateKey, TransactionError>;

    async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<CompressedCommitment, TransactionError>;

    // Look into perhaps removing all nonce here, if the signer and receiver are the same it should not be required to
    // share or pre calc the nonces
    async fn get_metadata_signature(
        &self,
        spending_key_id: &TariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &TariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_one_sided_metadata_signature(
        &self,
        spending_key_id: &TariKeyId,
        value: MicroMinotari,
        sender_offset_key_id: &TariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message_common: &[u8; 32],
        range_proof_type: RangeProofType,
        script: &TariScript,
        receiver_address: &TariAddress,
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn sign_script_message(
        &self,
        private_key_id: &TariKeyId,
        challenge: &[u8],
    ) -> Result<CompressedCheckSigSchnorrSignature, TransactionError>;

    async fn sign_with_nonce_and_challenge(
        &self,
        private_key_id: &TariKeyId,
        nonce: &TariKeyId,
        challenge: &[u8; 64],
    ) -> Result<Signature, TransactionError>;

    async fn get_receiver_partial_metadata_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &CompressedPublicKey,
        ephemeral_pubkey: &CompressedPublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError>;

    // In the case where the sender is an aggregated signer, we need to parse in the other public key shares, this is
    // done in: aggregated_sender_offset_public_keys and aggregated_ephemeral_public_keys. If there is no aggregated
    // signers, this can be left as none
    async fn get_sender_partial_metadata_signature(
        &self,
        ephemeral_private_nonce_id: &TariKeyId,
        sender_offset_key_id: &TariKeyId,
        commitment: &CompressedCommitment,
        ephemeral_commitment: &CompressedCommitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn generate_burn_proof(
        &self,
        spending_key: &TariKeyId,
        amount: &PrivateKey,
        claim_public_key: &CompressedPublicKey,
    ) -> Result<RistrettoComSig, TransactionError>;

    async fn stealth_address_script_spending_key(
        &self,
        commitment_mask_key_id: &TariKeyId,
        spend_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, TransactionError>;

    async fn encrypted_key(
        &self,
        key_id: &TariKeyId,
        encryption_key_id: Option<&TariKeyId>,
    ) -> Result<Vec<u8>, KeyManagerServiceError>;

    async fn import_encrypted_key(
        &self,
        encrypted: Vec<u8>,
        encryption_key_id: Option<&TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerServiceError>;
}

#[async_trait::async_trait]
pub trait SecretTransactionKeyManagerInterface: TransactionKeyManagerInterface {
    /// Gets the pedersen commitment for the specified index
    async fn get_private_key(&self, key_id: &TariKeyId) -> Result<PrivateKey, KeyManagerServiceError>;
}

#[cfg(test)]
mod test {
    use core::iter;
    use std::str::FromStr;

    use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
    use tari_common_types::types::{CompressedPublicKey, PrivateKey};
    use tari_crypto::keys::SecretKey as SK;

    use crate::transactions::transaction_key_manager::TariKeyId;

    fn random_string(len: usize) -> String {
        iter::repeat(())
            .map(|_| OsRng.sample(Alphanumeric) as char)
            .take(len)
            .collect()
    }

    #[test]
    fn key_id_converts_correctly() {
        let managed_key_id: TariKeyId = TariKeyId::Managed {
            branch: random_string(8),
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

        let managed_key_id_str = managed_key_id.to_string();
        let imported_key_id_str = imported_key_id.to_string();
        let zero_key_id_str = zero_key_id.to_string();
        let derived_key_id_str = derived_key_id.to_string();

        assert_eq!(managed_key_id, TariKeyId::from_str(&managed_key_id_str).unwrap());
        assert_eq!(imported_key_id, TariKeyId::from_str(&imported_key_id_str).unwrap());
        assert_eq!(zero_key_id, TariKeyId::from_str(&zero_key_id_str).unwrap());
        assert_eq!(derived_key_id, TariKeyId::from_str(&derived_key_id_str).unwrap());
    }
}

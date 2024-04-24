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

use std::str::FromStr;
use blake2::Blake2b;
use digest::consts::U64;
use strum_macros::EnumIter;
use tari_common_types::types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature};
use tari_comms::types::CommsDHKE;
use tari_crypto::{hashing::DomainSeparatedHash, ristretto::RistrettoComSig};
use tari_key_manager::key_manager_service::{KeyId, KeyManagerInterface, KeyManagerServiceError};

use crate::transactions::{
    tari_amount::MicroMinotari,
    transaction_components::{
        EncryptedData,
        KernelFeatures,
        RangeProofType,
        TransactionError,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
    },
};

pub type TariKeyId = KeyId<PublicKey>;

#[derive(Clone, Copy, PartialEq)]
pub enum TxoStage {
    Input,
    Output,
}

#[derive(Clone, Copy, EnumIter)]
pub enum TransactionKeyManagerBranch {
    DataEncryption,
    Coinbase,
    CommitmentMask,
    Nonce,
    KernelNonce,
    SenderOffset,
}

impl TransactionKeyManagerBranch {
    /// Warning: Changing these strings will affect the backwards compatibility of the wallet with older databases or
    /// recovery.
    pub fn get_branch_key(self) -> String {
        match self {
            TransactionKeyManagerBranch::DataEncryption => "data encryption".to_string(),
            TransactionKeyManagerBranch::Coinbase => "coinbase".to_string(),
            TransactionKeyManagerBranch::CommitmentMask => "commitment mask".to_string(),
            TransactionKeyManagerBranch::Nonce => "nonce".to_string(),
            TransactionKeyManagerBranch::KernelNonce => "kernel nonce".to_string(),
            TransactionKeyManagerBranch::SenderOffset => "sender offset".to_string(),
        }
    }
}

#[derive(Clone, Copy, EnumIter)]
pub enum TransactionKeyManagerLabel {
    ScriptKey
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

impl FromStr for TransactionKeyManagerLabel
{
    type Err = String;

    fn from_str(id: &str) -> Result<Self, Self::Err> {
        match id{
            "script key" => Ok(TransactionKeyManagerLabel::ScriptKey),
            _ => Err("Unknown label".to_string()),
        }
    }
}

#[async_trait::async_trait]
pub trait TransactionKeyManagerInterface: KeyManagerInterface<PublicKey> {
    /// Gets the pedersen commitment for the specified index
    async fn get_commitment(
        &self,
        spend_key_id: &TariKeyId,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError>;

    async fn verify_mask(
        &self,
        commitment: &Commitment,
        spend_key_id: &TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError>;

    async fn get_recovery_key_id(&self) -> Result<TariKeyId, KeyManagerServiceError>;

    async fn get_next_spend_and_script_key_ids(
        &self,
    ) -> Result<(TariKeyId, PublicKey, TariKeyId, PublicKey), KeyManagerServiceError>;

    async fn find_script_key_id_from_spend_key_id(
        &self,
        spend_key_id: &TariKeyId,
        public_script_key: Option<&PublicKey>,
    ) -> Result<Option<TariKeyId>, KeyManagerServiceError>;

    async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<CommsDHKE, TransactionError>;

    async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<DomainSeparatedHash<Blake2b<U64>>, TransactionError>;

    async fn import_add_offset_to_private_key(
        &self,
        secret_key_id: &TariKeyId,
        offset: PrivateKey,
    ) -> Result<TariKeyId, KeyManagerServiceError>;

    async fn get_spending_key_id(&self, public_spending_key: &PublicKey) -> Result<TariKeyId, TransactionError>;

    async fn construct_range_proof(
        &self,
        spend_key_id: &TariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError>;

    async fn get_script_signature(
        &self,
        script_key_id: &TariKeyId,
        spend_key_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_partial_txo_kernel_signature(
        &self,
        spend_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
        total_nonce: &PublicKey,
        total_excess: &PublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
        kernel_features: &KernelFeatures,
        txo_type: TxoStage,
    ) -> Result<Signature, TransactionError>;

    async fn get_txo_kernel_signature_excess_with_offset(
        &self,
        spend_key_id: &TariKeyId,
        nonce: &TariKeyId,
    ) -> Result<PublicKey, TransactionError>;

    async fn get_txo_private_kernel_offset(
        &self,
        spend_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, TransactionError>;

    async fn encrypt_data_for_recovery(
        &self,
        spend_key_id: &TariKeyId,
        custom_recovery_key_id: Option<&TariKeyId>,
        value: u64,
    ) -> Result<EncryptedData, TransactionError>;

    async fn try_output_key_recovery(
        &self,
        output: &TransactionOutput,
        custom_recovery_key_id: Option<&TariKeyId>,
    ) -> Result<(TariKeyId, MicroMinotari), TransactionError>;

    async fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
    ) -> Result<PrivateKey, TransactionError>;

    async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<Commitment, TransactionError>;

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

    async fn get_receiver_partial_metadata_signature(
        &self,
        spend_key_id: &TariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &PublicKey,
        ephemeral_pubkey: &PublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_sender_partial_metadata_signature(
        &self,
        ephemeral_private_nonce_id: &TariKeyId,
        sender_offset_key_id: &TariKeyId,
        commitment: &Commitment,
        ephemeral_commitment: &Commitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn generate_burn_proof(
        &self,
        spending_key: &TariKeyId,
        amount: &PrivateKey,
        claim_public_key: &PublicKey,
    ) -> Result<RistrettoComSig, TransactionError>;
}

#[async_trait::async_trait]
pub trait SecretTransactionKeyManagerInterface: TransactionKeyManagerInterface {
    /// Gets the pedersen commitment for the specified index
    async fn get_private_key(&self, key_id: &TariKeyId) -> Result<PrivateKey, KeyManagerServiceError>;
}

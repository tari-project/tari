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

use strum_macros::EnumIter;
use tari_common_types::types::{
    ComAndPubSignature,
    Commitment,
    PrivateKey,
    PublicKey,
    RangeProof,
    RangeProofService,
    Signature,
};
use tari_comms::types::CommsDHKE;
use tari_key_manager::key_manager_service::{KeyId, KeyManagerInterface, KeyManagerServiceError};

use crate::transactions::{
    tari_amount::MicroTari,
    transaction_components::{
        EncryptedData,
        TransactionError,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutputVersion,
    },
};

#[derive(Clone, Copy, EnumIter)]
pub enum CoreKeyManagerBranch {
    DataEncryption,
    Coinbase,
    CommitmentMask,
    Nonce,
    ScriptKey,
}

impl CoreKeyManagerBranch {
    /// Warning: Changing these strings will affect the backwards compatibility of the wallet with older databases or
    /// recovery.
    pub fn get_branch_key(self) -> String {
        match self {
            CoreKeyManagerBranch::DataEncryption => "core: data encryption".to_string(),
            CoreKeyManagerBranch::Coinbase => "core: coinbase".to_string(),
            CoreKeyManagerBranch::CommitmentMask => "core: commitment mask".to_string(),
            CoreKeyManagerBranch::Nonce => "core: nonce".to_string(),
            CoreKeyManagerBranch::ScriptKey => "core: script key".to_string(),
        }
    }
}

#[async_trait::async_trait]
pub trait BaseLayerKeyManagerInterface: KeyManagerInterface<PublicKey> {
    /// Gets the pedersen commitment for the specified index
    async fn get_commitment(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError>;

    async fn verify_mask(
        &self,
        prover: &RangeProofService,
        commitment: &Commitment,
        spend_key_id: &KeyId<PublicKey>,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError>;

    async fn get_recovery_key_id(&self) -> Result<KeyId<PublicKey>, KeyManagerServiceError>;

    async fn get_next_spend_and_script_key_ids(
        &self,
    ) -> Result<(KeyId<PublicKey>, KeyId<PublicKey>), KeyManagerServiceError>;

    async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &KeyId<PublicKey>,
        public_key: &PublicKey,
    ) -> Result<CommsDHKE, TransactionError>;

    async fn get_spending_key_id(&self, public_spending_key: &PublicKey) -> Result<KeyId<PublicKey>, TransactionError>;

    async fn construct_range_proof(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError>;

    async fn get_script_signature(
        &self,
        script_key_id: &KeyId<PublicKey>,
        spend_key_id: &KeyId<PublicKey>,
        value: &PrivateKey,
        tx_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_partial_kernel_signature(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        nonce_id: &KeyId<PublicKey>,
        total_nonce: &PublicKey,
        total_excess: &PublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
    ) -> Result<Signature, TransactionError>;

    async fn get_partial_kernel_signature_excess(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        nonce: &KeyId<PublicKey>,
    ) -> Result<PublicKey, TransactionError>;

    async fn get_partial_private_kernel_offset(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        nonce_id: &KeyId<PublicKey>,
    ) -> Result<PrivateKey, TransactionError>;

    async fn encrypt_data_for_recovery(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        custom_recovery_key_id: &Option<KeyId<PublicKey>>,
        value: u64,
    ) -> Result<EncryptedData, TransactionError>;

    async fn try_commitment_key_recovery(
        &self,
        commitment: &Commitment,
        data: &EncryptedData,
        custom_recovery_key_id: &Option<KeyId<PublicKey>>,
    ) -> Result<(KeyId<PublicKey>, MicroTari), TransactionError>;

    async fn get_script_offset(
        &self,
        script_key_ids: &[KeyId<PublicKey>],
        sender_offset_key_ids: &[KeyId<PublicKey>],
    ) -> Result<PrivateKey, TransactionError>;

    async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &KeyId<PublicKey>,
    ) -> Result<Commitment, TransactionError>;

    async fn get_metadata_signature(
        &self,
        value_as_private_key: &PrivateKey,
        spending_key_id: &KeyId<PublicKey>,
        sender_offset_private_key: &PrivateKey,
        nonce_a: &PrivateKey,
        nonce_b: &PrivateKey,
        nonce_x: &PrivateKey,
        challenge_bytes: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_receiver_partial_metadata_signature(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        value: &PrivateKey,
        nonce_id: &KeyId<PublicKey>,
        sender_offset_public_key: &PublicKey,
        ephemeral_pubkey: &PublicKey,
        tx_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    async fn get_sender_partial_metadata_signature(
        &self,
        nonce_id: &KeyId<PublicKey>,
        sender_offset_key_id: &KeyId<PublicKey>,
        commitment: &Commitment,
        ephemeral_commitment: &Commitment,
        tx_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;
}

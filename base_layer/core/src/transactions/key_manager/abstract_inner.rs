//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that
// the  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED
// WARRANTIES,  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A
// PARTICULAR PURPOSE ARE  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY
// DIRECT, INDIRECT, INCIDENTAL,  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO,
// PROCUREMENT OF SUBSTITUTE GOODS OR  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY,  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR
// OTHERWISE) ARISING IN ANY WAY OUT OF THE  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH
// DAMAGE.

use tari_common_types::types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature};
use tari_comms::types::CommsDHKE;
use tari_crypto::{hash::blake2::Blake256, hash_domain, hashing::DomainSeparatedHash, ristretto::RistrettoComSig};
use tari_key_manager::key_manager_service::{AddResult, KeyManagerServiceError};

use crate::transactions::{
    key_manager::{interface::TxoStage, TariKeyId},
    tari_amount::MicroTari,
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

hash_domain!(
    KeyManagerHashingDomain,
    "com.tari.base_layer.core.transactions.key_manager"
);

#[async_trait::async_trait]
pub trait TransactionKeyManagerAbstractInner {
    fn add_standard_core_branches(&mut self) -> Result<(), KeyManagerServiceError>;
    fn add_key_manager_branch(&mut self, branch: &str) -> Result<AddResult, KeyManagerServiceError>;
    async fn get_next_key(&self, branch: &str) -> Result<(TariKeyId, PublicKey), KeyManagerServiceError>;
    async fn get_static_key(&self, branch: &str) -> Result<TariKeyId, KeyManagerServiceError>;
    async fn get_public_key_at_key_id(&self, key_id: &TariKeyId) -> Result<PublicKey, KeyManagerServiceError>;
    async fn get_next_spend_and_script_key_ids(
        &self,
    ) -> Result<(TariKeyId, PublicKey, TariKeyId, PublicKey), KeyManagerServiceError>;

    /// Search the specified branch key manager key chain to find the index of the specified key.
    async fn find_key_index(&self, branch: &str, key: &PublicKey) -> Result<u64, KeyManagerServiceError>;

    /// Search the specified branch key manager key chain to find the index of the specified private key.
    async fn find_private_key_index(&self, branch: &str, key: &PrivateKey) -> Result<u64, KeyManagerServiceError>;

    /// If the supplied index is higher than the current UTXO key chain indices then they will be updated.
    async fn update_current_key_index_if_higher(&self, branch: &str, index: u64) -> Result<(), KeyManagerServiceError>;

    async fn import_key(&self, private_key: PrivateKey) -> Result<TariKeyId, KeyManagerServiceError>;
    async fn get_private_key(&self, key_id: &TariKeyId) -> Result<PrivateKey, KeyManagerServiceError>;

    // -----------------------------------------------------------------------------------------------------------------
    // General crypto section
    // -----------------------------------------------------------------------------------------------------------------

    async fn get_commitment(
        &self,
        private_key: &TariKeyId,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError>;

    /// Verify that the commitment matches the value and the spending key/mask
    async fn verify_mask(
        &self,
        commitment: &Commitment,
        spending_key_id: &TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError>;
    async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<CommsDHKE, TransactionError>;
    async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<DomainSeparatedHash<Blake256>, TransactionError>;
    async fn import_add_offset_to_private_key(
        &self,
        secret_key_id: &TariKeyId,
        offset: PrivateKey,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        let secret_key = self.get_private_key(secret_key_id).await?;
        self.import_key(secret_key + offset).await
    }
    async fn generate_burn_proof(
        &self,
        spending_key: &TariKeyId,
        amount: &PrivateKey,
        claim_public_key: &PublicKey,
    ) -> Result<RistrettoComSig, TransactionError>;

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction input section (transactions > transaction_components > transaction_input)
    // -----------------------------------------------------------------------------------------------------------------

    async fn get_script_signature(
        &self,
        script_key_id: &TariKeyId,
        spend_key_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError>;

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction output section (transactions > transaction_components > transaction_output)
    // -----------------------------------------------------------------------------------------------------------------

    async fn get_spending_key_id(&self, public_spending_key: &PublicKey) -> Result<TariKeyId, TransactionError>;
    async fn construct_range_proof(
        &self,
        private_key: &TariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError>;
    async fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
    ) -> Result<PrivateKey, TransactionError>;
    async fn get_metadata_signature_ephemeral_private_key_pair(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<(PrivateKey, PrivateKey), TransactionError>;
    async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<Commitment, TransactionError>;
    async fn get_metadata_signature_raw(
        &self,
        spending_key_id: &TariKeyId,
        value_as_private_key: &PrivateKey,
        ephemeral_private_nonce_id: &TariKeyId,
        sender_offset_key_id: &TariKeyId,
        ephemeral_pubkey: &PublicKey,
        ephemeral_commitment: &Commitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError>;
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

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction kernel section (transactions > transaction_components > transaction_kernel)
    // -----------------------------------------------------------------------------------------------------------------

    async fn get_txo_private_kernel_offset(
        &self,
        spend_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, TransactionError>;

    async fn get_partial_txo_kernel_signature(
        &self,
        spending_key_id: &TariKeyId,
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
        nonce_id: &TariKeyId,
    ) -> Result<PublicKey, TransactionError>;

    // -----------------------------------------------------------------------------------------------------------------
    // Encrypted data section (transactions > transaction_components > encrypted_data)
    // -----------------------------------------------------------------------------------------------------------------

    async fn get_recovery_key(&self) -> Result<PrivateKey, KeyManagerServiceError>;

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
    ) -> Result<(TariKeyId, MicroTari), TransactionError>;
}

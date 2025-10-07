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

use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use blake2::Blake2b;
use digest::consts::U64;
use tari_common_types::{
    seeds::cipher_seed::CipherSeed,
    tari_address::TariAddress,
    types::{
        ComAndPubSignature,
        CommsDHKE,
        CompressedCommitment,
        CompressedPublicKey,
        CompressedSignature,
        PrivateKey,
        RangeProof,
        WalletMessageSchnorrSignature,
    },
    wallet_types::WalletType,
};
use tari_crypto::hashing::DomainSeparatedHash;
use tari_script::{CompressedCheckSigSchnorrSignature, TariScript};

use crate::{
    crypto_factories::CryptoFactories,
    key_manager::{
        error::KeyManagerServiceError,
        interface::{SecretTransactionKeyManagerInterface, TariKeyAndId, TransactionKeyManagerBackend, TxoStage},
        AddResult,
        TariKeyId,
        TransactionKeyManagerInner,
        TransactionKeyManagerInterface,
    },
    transaction_components::{
        EncryptedData,
        KernelFeatures,
        MemoField,
        RangeProofType,
        TransactionError,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutputVersion,
    },
    MicroMinotari,
};

/// The key manager provides a hierarchical key derivation function (KDF) that derives uniformly random secret keys from
/// a single seed key for arbitrary branches, using an implementation of `KeyManagerBackend` to store the current index
/// for each branch.
///
/// This handle can be cloned cheaply and safely shared across multiple threads.
#[derive(Clone)]
pub struct TransactionKeyManagerWrapper<TBackend> {
    transaction_key_manager_inner: TransactionKeyManagerInner<TBackend>,
}

impl<TBackend> TransactionKeyManagerWrapper<TBackend>
where TBackend: TransactionKeyManagerBackend + 'static
{
    /// Creates a new key manager.
    /// * `master_seed` is the primary seed that will be used to derive all unique branch keys with their indexes
    /// * `db` implements `KeyManagerBackend` and is used for persistent storage of branches and indices.
    pub async fn new_with_legacy_storage(
        master_seed: Option<CipherSeed>,
        db: TBackend,
        crypto_factories: CryptoFactories,
        wallet_type: Arc<WalletType>,
    ) -> Result<Self, KeyManagerServiceError> {
        Ok(TransactionKeyManagerWrapper {
            transaction_key_manager_inner: TransactionKeyManagerInner::new(
                master_seed,
                Some(db),
                crypto_factories,
                wallet_type,
            )
            .await?,
        })
    }

    pub async fn new(
        master_seed: Option<CipherSeed>,
        crypto_factories: CryptoFactories,
        wallet_type: Arc<WalletType>,
    ) -> Result<Self, KeyManagerServiceError> {
        Ok(TransactionKeyManagerWrapper {
            transaction_key_manager_inner: TransactionKeyManagerInner::new(
                master_seed,
                None,
                crypto_factories,
                wallet_type,
            )
            .await?,
        })
    }

    /// Get the wallet type
    pub async fn get_wallet_type(&self) -> Arc<WalletType> {
        self.transaction_key_manager_inner.get_wallet_type()
    }

    /// Get the birthday of the wallet seed
    pub async fn get_birthday(&self) -> Option<u16> {
        self.transaction_key_manager_inner
            .master_seed()
            .map(|s| s.birthday())
            .or_else(|| match self.transaction_key_manager_inner.get_wallet_type().as_ref() {
                WalletType::ProvidedKeys(keys) => keys.birthday,
                _ => None,
            })
    }
}

#[async_trait::async_trait]
impl<TBackend> TransactionKeyManagerInterface for TransactionKeyManagerWrapper<TBackend>
where TBackend: TransactionKeyManagerBackend + 'static
{
    async fn add_new_branch<T: Into<String> + Send>(&mut self, branch: T) -> Result<AddResult, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .add_key_manager_branch(&branch.into())
            .await
    }

    async fn get_next_key<T: Into<String> + Send>(
        &mut self,
        branch: T,
    ) -> Result<TariKeyAndId, KeyManagerServiceError> {
        self.transaction_key_manager_inner.get_next_key(&branch.into()).await
    }

    async fn get_random_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        self.transaction_key_manager_inner.get_random_key().await
    }

    async fn get_static_key<T: Into<String> + Send>(&self, branch: T) -> Result<TariKeyId, KeyManagerServiceError> {
        self.transaction_key_manager_inner.get_static_key(&branch.into()).await
    }

    async fn get_public_key_at_key_id(
        &self,
        key_id: &TariKeyId,
    ) -> Result<CompressedPublicKey, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .get_public_key_at_key_id(key_id)
            .await
    }

    async fn import_key(
        &self,
        private_key: PrivateKey,
        encryption_key: Option<TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .import_key(private_key, encryption_key)
            .await
    }

    async fn create_encrypted_key_from_existing_key(
        &self,
        key_id: &TariKeyId,
        encryption_key: Option<TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .create_encrypted_key_from_existing_key(key_id, encryption_key)
            .await
    }

    async fn get_commitment(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
    ) -> Result<CompressedCommitment, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .get_commitment(commitment_mask_key_id, value)
            .await
    }

    async fn verify_mask(
        &self,
        commitment: &CompressedCommitment,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .verify_mask(commitment, commitment_mask_key_id, value)
            .await
    }

    async fn get_view_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        self.transaction_key_manager_inner.get_view_key().await
    }

    async fn get_private_view_key(&self) -> Result<PrivateKey, KeyManagerServiceError> {
        self.transaction_key_manager_inner.get_private_view_key().await
    }

    async fn get_spend_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        self.transaction_key_manager_inner.get_spend_key().await
    }

    async fn sign_message_with_spend_key(
        &self,
        message: &[u8],
        sender_offset_pub_key: Option<&CompressedPublicKey>,
    ) -> Result<WalletMessageSchnorrSignature, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .sign_message(message, sender_offset_pub_key)
            .await
    }

    async fn get_comms_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        self.transaction_key_manager_inner.get_comms_key().await
    }

    async fn get_next_commitment_mask_and_script_key(
        &mut self,
    ) -> Result<(TariKeyAndId, TariKeyAndId), KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .get_next_commitment_mask_and_script_key()
            .await
    }

    async fn find_script_key_id_from_commitment_mask_key_id(
        &self,
        commitment_mask_key_id: &TariKeyId,
        public_script_key: Option<&CompressedPublicKey>,
    ) -> Result<Option<TariKeyId>, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .find_script_key_id_from_commitment_mask_key_id(commitment_mask_key_id, public_script_key)
            .await
    }

    async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<CommsDHKE, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .get_diffie_hellman_shared_secret(secret_key_id, public_key)
            .await
    }

    async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<DomainSeparatedHash<Blake2b<U64>>, TransactionError> {
        self.transaction_key_manager_inner
            .get_diffie_hellman_stealth_domain_hasher(secret_key_id, public_key)
            .await
    }

    async fn construct_range_proof(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError> {
        self.transaction_key_manager_inner
            .construct_range_proof(commitment_mask_key_id, value, min_value)
            .await
    }

    async fn get_script_signature(
        &self,
        script_key_id: &TariKeyId,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .get_script_signature(
                script_key_id,
                commitment_mask_key_id,
                value,
                txi_version,
                script_message,
            )
            .await
    }

    async fn get_partial_script_signature(
        &self,
        commitment_mask_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        ephemeral_pubkey: &CompressedPublicKey,
        script_public_key: &CompressedPublicKey,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .get_partial_script_signature(
                commitment_mask_id,
                value,
                txi_version,
                ephemeral_pubkey,
                script_public_key,
                script_message,
            )
            .await
    }

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
    ) -> Result<CompressedSignature, TransactionError> {
        self.transaction_key_manager_inner
            .get_partial_txo_kernel_signature(
                commitment_mask_key_id,
                nonce_id,
                total_nonce,
                total_excess,
                kernel_version,
                kernel_message,
                kernel_features,
                txo_type,
            )
            .await
    }

    async fn get_txo_kernel_signature_excess_with_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<CompressedPublicKey, TransactionError> {
        self.transaction_key_manager_inner
            .get_txo_kernel_signature_excess_with_offset(commitment_mask_key_id, nonce_id)
            .await
    }

    async fn get_txo_private_kernel_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, TransactionError> {
        self.transaction_key_manager_inner
            .get_txo_private_kernel_offset(commitment_mask_key_id, nonce_id)
            .await
    }

    async fn encrypt_data_for_recovery(
        &self,
        commitment_mask_key_id: &TariKeyId,
        custom_recovery_key_id: Option<&TariKeyId>,
        value: u64,
        payment_id: MemoField,
    ) -> Result<EncryptedData, TransactionError> {
        self.transaction_key_manager_inner
            .encrypt_data_for_recovery(commitment_mask_key_id, custom_recovery_key_id, value, payment_id)
            .await
    }

    async fn extract_payment_id_from_encrypted_data(
        &self,
        encrypted_data: &EncryptedData,
        commitment: &CompressedCommitment,
        custom_recovery_key_id: Option<&TariKeyId>,
    ) -> Result<MemoField, TransactionError> {
        self.transaction_key_manager_inner
            .extract_payment_id_from_encrypted_data(encrypted_data, commitment, custom_recovery_key_id)
            .await
    }

    async fn try_output_key_recovery(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        sender_offset_public_key: &CompressedPublicKey,
    ) -> Result<Option<(TariKeyId, MicroMinotari, MemoField)>, TransactionError> {
        self.transaction_key_manager_inner
            .try_output_key_recovery(commitment, encrypted_data, sender_offset_public_key)
            .await
    }

    async fn is_this_output_ours(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        custom_recovery_key_id: Option<PrivateKey>,
    ) -> Result<bool, TransactionError> {
        self.transaction_key_manager_inner
            .is_this_output_ours(commitment, encrypted_data, custom_recovery_key_id)
            .await
    }

    async fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
    ) -> Result<PrivateKey, TransactionError> {
        self.transaction_key_manager_inner
            .get_script_offset(script_key_ids, sender_offset_key_ids)
            .await
    }

    async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<CompressedCommitment, TransactionError> {
        self.transaction_key_manager_inner
            .get_metadata_signature_ephemeral_commitment(nonce_id, range_proof_type)
            .await
    }

    async fn get_metadata_signature(
        &mut self,
        commitment_mask_key_id: &TariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &TariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .get_metadata_signature(
                commitment_mask_key_id,
                value_as_private_key,
                sender_offset_key_id,
                txo_version,
                metadata_signature_message,
                range_proof_type,
            )
            .await
    }

    async fn get_one_sided_metadata_signature(
        &mut self,
        commitment_mask_key_id: &TariKeyId,
        value: MicroMinotari,
        sender_offset_key_id: &TariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message_common: &[u8; 32],
        range_proof_type: RangeProofType,
        script: &TariScript,
        receiver_address: &TariAddress,
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .get_one_sided_metadata_signature(
                commitment_mask_key_id,
                value,
                sender_offset_key_id,
                txo_version,
                metadata_signature_message_common,
                range_proof_type,
                script,
                receiver_address,
            )
            .await
    }

    async fn sign_script_message(
        &self,
        private_key_id: &TariKeyId,
        challenge: &[u8],
    ) -> Result<CompressedCheckSigSchnorrSignature, TransactionError> {
        self.transaction_key_manager_inner
            .sign_script_message(private_key_id, challenge)
            .await
    }

    async fn sign_script_message_with_spend_key(
        &self,
        message: &[u8],
        sender_offset_pub_key: Option<&CompressedPublicKey>,
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .sign_script_message_with_spend_key(message, sender_offset_pub_key)
            .await
    }

    async fn sign_with_nonce_and_challenge(
        &self,
        private_key_id: &TariKeyId,
        nonce: &TariKeyId,
        challenge: &[u8; 64],
    ) -> Result<CompressedSignature, TransactionError> {
        self.transaction_key_manager_inner
            .sign_with_nonce_and_challenge(private_key_id, nonce, challenge)
            .await
    }

    async fn get_receiver_partial_metadata_signature(
        &mut self,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &CompressedPublicKey,
        ephemeral_pubkey: &CompressedPublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .get_receiver_partial_metadata_signature(
                commitment_mask_key_id,
                value,
                sender_offset_public_key,
                ephemeral_pubkey,
                txo_version,
                metadata_signature_message,
                range_proof_type,
            )
            .await
    }

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
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .get_sender_partial_metadata_signature(
                ephemeral_private_nonce_id,
                sender_offset_key_id,
                commitment,
                ephemeral_commitment,
                txo_version,
                metadata_signature_message,
            )
            .await
    }

    async fn generate_burn_claim_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
        claim_public_key: &CompressedPublicKey,
    ) -> Result<CompressedSignature, TransactionError> {
        self.transaction_key_manager_inner
            .generate_burn_claim_proof_signature(commitment_mask_key_id, value, claim_public_key)
            .await
    }

    async fn stealth_address_script_spending_key(
        &self,
        commitment_mask_key_id: &TariKeyId,
        spend_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, TransactionError> {
        self.transaction_key_manager_inner
            .stealth_address_script_spending_key(commitment_mask_key_id, spend_key)
            .await
    }

    async fn add_offset_to_spend_key(
        &self,
        spend_key_id: &TariKeyId,
        sender_offset_pub_key: &CompressedPublicKey,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .add_offset_to_spend_key(spend_key_id, sender_offset_pub_key)
            .await
    }

    async fn encrypted_key(
        &self,
        key_id: &TariKeyId,
        encryption_key_id: Option<&TariKeyId>,
    ) -> Result<Vec<u8>, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .encrypted_key(key_id, encryption_key_id)
            .await
    }

    async fn import_encrypted_key(
        &self,
        encrypted: Vec<u8>,
        encryption_key_id: Option<&TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .import_encrypted_key(encrypted, encryption_key_id)
            .await
    }
}

#[async_trait::async_trait]
impl<TBackend> SecretTransactionKeyManagerInterface for TransactionKeyManagerWrapper<TBackend>
where TBackend: TransactionKeyManagerBackend + 'static
{
    async fn get_private_key(&self, key_id: &TariKeyId) -> Result<PrivateKey, KeyManagerServiceError> {
        self.transaction_key_manager_inner.get_private_key(key_id).await
    }
}

impl<KM> Debug for TransactionKeyManagerWrapper<KM> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Key Manager").finish()
    }
}

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

use std::sync::Arc;

use blake2::Blake2b;
use digest::consts::U64;
use tari_common_types::{
    tari_address::TariAddress,
    types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature},
    wallet_types::WalletType,
};
use tari_crypto::{hashing::DomainSeparatedHash, ristretto::RistrettoComSig};
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager_service::{
        storage::database::{KeyManagerBackend, KeyManagerDatabase},
        AddResult,
        KeyAndId,
        KeyManagerInterface,
        KeyManagerServiceError,
    },
};
use tari_script::{CheckSigSchnorrSignature, TariScript};
use tokio::sync::RwLock;

use crate::transactions::{
    key_manager::{
        interface::{SecretTransactionKeyManagerInterface, TxoStage},
        RistrettoDiffieHellmanSharedSecret,
        TariKeyId,
        TransactionKeyManagerInner,
        TransactionKeyManagerInterface,
    },
    tari_amount::MicroMinotari,
    transaction_components::{
        encrypted_data::PaymentId,
        EncryptedData,
        KernelFeatures,
        RangeProofType,
        TransactionError,
        TransactionInputVersion,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
    },
    CryptoFactories,
};

/// The key manager provides a hierarchical key derivation function (KDF) that derives uniformly random secret keys from
/// a single seed key for arbitrary branches, using an implementation of `KeyManagerBackend` to store the current index
/// for each branch.
///
/// This handle can be cloned cheaply and safely shared across multiple threads.
#[derive(Clone)]
pub struct TransactionKeyManagerWrapper<TBackend> {
    transaction_key_manager_inner: Arc<RwLock<TransactionKeyManagerInner<TBackend>>>,
}

impl<TBackend> TransactionKeyManagerWrapper<TBackend>
where TBackend: KeyManagerBackend<PublicKey> + 'static
{
    /// Creates a new key manager.
    /// * `master_seed` is the primary seed that will be used to derive all unique branch keys with their indexes
    /// * `db` implements `KeyManagerBackend` and is used for persistent storage of branches and indices.
    pub fn new(
        master_seed: CipherSeed,
        db: KeyManagerDatabase<TBackend, PublicKey>,
        crypto_factories: CryptoFactories,
        wallet_type: Arc<WalletType>,
    ) -> Result<Self, KeyManagerServiceError> {
        Ok(TransactionKeyManagerWrapper {
            transaction_key_manager_inner: Arc::new(RwLock::new(TransactionKeyManagerInner::new(
                master_seed,
                db,
                crypto_factories,
                wallet_type,
            )?)),
        })
    }

    /// Get the wallet type
    pub async fn get_wallet_type(&self) -> Arc<WalletType> {
        self.transaction_key_manager_inner.read().await.get_wallet_type()
    }
}

#[async_trait::async_trait]
impl<TBackend> KeyManagerInterface<PublicKey> for TransactionKeyManagerWrapper<TBackend>
where TBackend: KeyManagerBackend<PublicKey> + 'static
{
    async fn add_new_branch<T: Into<String> + Send>(&self, branch: T) -> Result<AddResult, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .write()
            .await
            .add_key_manager_branch(&branch.into())
    }

    async fn get_next_key<T: Into<String> + Send>(
        &self,
        branch: T,
    ) -> Result<KeyAndId<PublicKey>, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_next_key(&branch.into())
            .await
    }

    async fn get_random_key(&self) -> Result<KeyAndId<PublicKey>, KeyManagerServiceError> {
        self.transaction_key_manager_inner.read().await.get_random_key().await
    }

    async fn get_static_key<T: Into<String> + Send>(&self, branch: T) -> Result<TariKeyId, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_static_key(&branch.into())
            .await
    }

    async fn get_public_key_at_key_id(&self, key_id: &TariKeyId) -> Result<PublicKey, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_public_key_at_key_id(key_id)
            .await
    }

    async fn find_key_index<T: Into<String> + Send>(
        &self,
        branch: T,
        key: &PublicKey,
    ) -> Result<u64, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .find_key_index(&branch.into(), key)
            .await
    }

    async fn update_current_key_index_if_higher<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<(), KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .update_current_key_index_if_higher(&branch.into(), index)
            .await
    }

    async fn import_key(&self, private_key: PrivateKey) -> Result<TariKeyId, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .import_key(private_key)
            .await
    }
}

#[async_trait::async_trait]
impl<TBackend> TransactionKeyManagerInterface for TransactionKeyManagerWrapper<TBackend>
where TBackend: KeyManagerBackend<PublicKey> + 'static
{
    async fn get_commitment(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_commitment(commitment_mask_key_id, value)
            .await
    }

    async fn verify_mask(
        &self,
        commitment: &Commitment,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .verify_mask(commitment, commitment_mask_key_id, value)
            .await
    }

    async fn get_view_key(&self) -> Result<KeyAndId<PublicKey>, KeyManagerServiceError> {
        self.transaction_key_manager_inner.read().await.get_view_key().await
    }

    async fn get_private_view_key(&self) -> Result<PrivateKey, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_private_view_key()
            .await
    }

    async fn get_spend_key(&self) -> Result<KeyAndId<PublicKey>, KeyManagerServiceError> {
        self.transaction_key_manager_inner.read().await.get_spend_key().await
    }

    async fn get_comms_key(&self) -> Result<KeyAndId<PublicKey>, KeyManagerServiceError> {
        self.transaction_key_manager_inner.read().await.get_comms_key().await
    }

    async fn get_next_commitment_mask_and_script_key(
        &self,
    ) -> Result<(KeyAndId<PublicKey>, KeyAndId<PublicKey>), KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_next_commitment_mask_and_script_key()
            .await
    }

    async fn find_script_key_id_from_commitment_mask_key_id(
        &self,
        commitment_mask_key_id: &TariKeyId,
        public_script_key: Option<&PublicKey>,
    ) -> Result<Option<TariKeyId>, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .find_script_key_id_from_commitment_mask_key_id(commitment_mask_key_id, public_script_key)
            .await
    }

    async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<RistrettoDiffieHellmanSharedSecret, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_diffie_hellman_shared_secret(secret_key_id, public_key)
            .await
    }

    async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<DomainSeparatedHash<Blake2b<U64>>, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_diffie_hellman_stealth_domain_hasher(secret_key_id, public_key)
            .await
    }

    async fn get_spending_key_id(&self, public_spending_key: &PublicKey) -> Result<TariKeyId, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_spending_key_id(public_spending_key)
            .await
    }

    async fn construct_range_proof(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
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
            .read()
            .await
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
        ephemeral_pubkey: &PublicKey,
        script_public_key: &PublicKey,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
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
        total_nonce: &PublicKey,
        total_excess: &PublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
        kernel_features: &KernelFeatures,
        txo_type: TxoStage,
    ) -> Result<Signature, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
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
    ) -> Result<PublicKey, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_txo_kernel_signature_excess_with_offset(commitment_mask_key_id, nonce_id)
            .await
    }

    async fn get_txo_private_kernel_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_txo_private_kernel_offset(commitment_mask_key_id, nonce_id)
            .await
    }

    async fn encrypt_data_for_recovery(
        &self,
        commitment_mask_key_id: &TariKeyId,
        custom_recovery_key_id: Option<&TariKeyId>,
        value: u64,
        payment_id: PaymentId,
    ) -> Result<EncryptedData, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .encrypt_data_for_recovery(commitment_mask_key_id, custom_recovery_key_id, value, payment_id)
            .await
    }

    async fn try_output_key_recovery(
        &self,
        output: &TransactionOutput,
        custom_recovery_key_id: Option<&TariKeyId>,
    ) -> Result<(TariKeyId, MicroMinotari, PaymentId), TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .try_output_key_recovery(output, custom_recovery_key_id)
            .await
    }

    async fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
    ) -> Result<PrivateKey, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_script_offset(script_key_ids, sender_offset_key_ids)
            .await
    }

    async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<Commitment, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_metadata_signature_ephemeral_commitment(nonce_id, range_proof_type)
            .await
    }

    async fn get_metadata_signature(
        &self,
        spending_key_id: &TariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &TariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_metadata_signature(
                spending_key_id,
                value_as_private_key,
                sender_offset_key_id,
                txo_version,
                metadata_signature_message,
                range_proof_type,
            )
            .await
    }

    async fn get_one_sided_metadata_signature(
        &self,
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
            .read()
            .await
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
    ) -> Result<CheckSigSchnorrSignature, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .sign_script_message(private_key_id, challenge)
            .await
    }

    async fn sign_with_nonce_and_challenge(
        &self,
        private_key_id: &TariKeyId,
        nonce: &TariKeyId,
        challenge: &[u8; 64],
    ) -> Result<Signature, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .sign_with_nonce_and_challenge(private_key_id, nonce, challenge)
            .await
    }

    async fn get_receiver_partial_metadata_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &PublicKey,
        ephemeral_pubkey: &PublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
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
        commitment: &Commitment,
        ephemeral_commitment: &Commitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
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

    async fn generate_burn_proof(
        &self,
        spending_key: &TariKeyId,
        amount: &PrivateKey,
        claim_public_key: &PublicKey,
    ) -> Result<RistrettoComSig, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .generate_burn_proof(spending_key, amount, claim_public_key)
            .await
    }

    async fn stealth_address_script_spending_key(
        &self,
        commitment_mask_key_id: &TariKeyId,
        spend_key: &PublicKey,
    ) -> Result<PublicKey, TransactionError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .stealth_address_script_spending_key(commitment_mask_key_id, spend_key)
            .await
    }
}

#[async_trait::async_trait]
impl<TBackend> SecretTransactionKeyManagerInterface for TransactionKeyManagerWrapper<TBackend>
where TBackend: KeyManagerBackend<PublicKey> + 'static
{
    async fn get_private_key(&self, key_id: &TariKeyId) -> Result<PrivateKey, KeyManagerServiceError> {
        self.transaction_key_manager_inner
            .read()
            .await
            .get_private_key(key_id)
            .await
    }
}

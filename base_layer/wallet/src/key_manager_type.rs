// Copyright 2023. The Tari Project
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

use async_trait::async_trait;
use blake2::Blake2b;
use digest::consts::U32;
use tari_common_types::types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature};
use tari_comms::types::CommsDHKE;
use tari_core::transactions::{
    key_manager::{
        SecretTransactionKeyManagerInterface,
        TransactionKeyManagerInterface,
        TransactionKeyManagerWrapper,
        TxoStage,
    },
    ledger_key_manager::TransactionKeyManagerLedgerWrapper,
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
use tari_crypto::{
    hashing::DomainSeparatedHash,
    keys::{PublicKey as TPublicKey, SecretKey},
    ristretto::RistrettoComSig,
};
use tari_key_manager::key_manager_service::{
    storage::sqlite_db::KeyManagerSqliteDatabase,
    AddResult,
    KeyId,
    KeyManagerInterface,
    KeyManagerServiceError,
};

use crate::WalletDbConnection;

#[derive(Clone)]
pub enum KeyManagerType {
    Ledger(TransactionKeyManagerLedgerWrapper<KeyManagerSqliteDatabase<WalletDbConnection>>),
    Console(TransactionKeyManagerWrapper<KeyManagerSqliteDatabase<WalletDbConnection>>),
}

#[async_trait]
impl<PK> KeyManagerInterface<PK> for KeyManagerType
where
    PK: TPublicKey + Send + Sync + 'static,
    PK::K: SecretKey + Send + Sync + 'static,
{
    async fn add_new_branch<T: Into<String> + Send>(&self, branch: T) -> Result<AddResult, KeyManagerServiceError> {
        match self {
            KeyManagerType::Ledger(km) => km.add_new_branch(branch).await,
            KeyManagerType::Console(km) => km.add_new_branch(branch).await,
        }
    }

    async fn get_next_key<T: Into<String> + Send>(&self, branch: T) -> Result<(KeyId<PK>, PK), KeyManagerServiceError> {
        todo!()
    }

    async fn get_static_key<T: Into<String> + Send>(&self, branch: T) -> Result<KeyId<PK>, KeyManagerServiceError> {
        todo!()
    }

    async fn get_public_key_at_key_id(&self, key_id: &KeyId<PK>) -> Result<PK, KeyManagerServiceError> {
        todo!()
    }

    async fn find_key_index<T: Into<String> + Send>(&self, branch: T, key: &PK) -> Result<u64, KeyManagerServiceError> {
        todo!()
    }

    async fn update_current_key_index_if_higher<T: Into<String> + Send>(
        &self,
        branch: T,
        index: u64,
    ) -> Result<(), KeyManagerServiceError> {
        todo!()
    }

    async fn import_key(&self, private_key: PK::K) -> Result<KeyId<PK>, KeyManagerServiceError> {
        todo!()
    }
}

#[async_trait]
impl TransactionKeyManagerInterface for KeyManagerType {
    async fn get_commitment(
        &self,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError> {
        todo!()
    }

    async fn verify_mask(
        &self,
        commitment: &Commitment,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError> {
        todo!()
    }

    async fn get_recovery_key_id(
        &self,
    ) -> Result<tari_core::transactions::key_manager::TariKeyId, KeyManagerServiceError> {
        todo!()
    }

    async fn get_next_spend_and_script_key_ids(
        &self,
    ) -> Result<
        (
            tari_core::transactions::key_manager::TariKeyId,
            PublicKey,
            tari_core::transactions::key_manager::TariKeyId,
            PublicKey,
        ),
        KeyManagerServiceError,
    > {
        todo!()
    }

    async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &tari_core::transactions::key_manager::TariKeyId,
        public_key: &PublicKey,
    ) -> Result<CommsDHKE, TransactionError> {
        todo!()
    }

    async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &tari_core::transactions::key_manager::TariKeyId,
        public_key: &PublicKey,
    ) -> Result<DomainSeparatedHash<Blake2b<U32>>, TransactionError> {
        todo!()
    }

    async fn import_add_offset_to_private_key(
        &self,
        secret_key_id: &tari_core::transactions::key_manager::TariKeyId,
        offset: PrivateKey,
    ) -> Result<tari_core::transactions::key_manager::TariKeyId, KeyManagerServiceError> {
        todo!()
    }

    async fn get_spending_key_id(
        &self,
        public_spending_key: &PublicKey,
    ) -> Result<tari_core::transactions::key_manager::TariKeyId, TransactionError> {
        todo!()
    }

    async fn construct_range_proof(
        &self,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError> {
        todo!()
    }

    async fn get_script_signature(
        &self,
        script_key_id: &tari_core::transactions::key_manager::TariKeyId,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        todo!()
    }

    async fn get_partial_txo_kernel_signature(
        &self,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        nonce_id: &tari_core::transactions::key_manager::TariKeyId,
        total_nonce: &PublicKey,
        total_excess: &PublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
        kernel_features: &KernelFeatures,
        txo_type: TxoStage,
    ) -> Result<Signature, TransactionError> {
        todo!()
    }

    async fn get_txo_kernel_signature_excess_with_offset(
        &self,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        nonce: &tari_core::transactions::key_manager::TariKeyId,
    ) -> Result<PublicKey, TransactionError> {
        todo!()
    }

    async fn get_txo_private_kernel_offset(
        &self,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        nonce_id: &tari_core::transactions::key_manager::TariKeyId,
    ) -> Result<PrivateKey, TransactionError> {
        todo!()
    }

    async fn encrypt_data_for_recovery(
        &self,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        custom_recovery_key_id: Option<&tari_core::transactions::key_manager::TariKeyId>,
        value: u64,
    ) -> Result<EncryptedData, TransactionError> {
        todo!()
    }

    async fn try_output_key_recovery(
        &self,
        output: &TransactionOutput,
        custom_recovery_key_id: Option<&tari_core::transactions::key_manager::TariKeyId>,
    ) -> Result<(tari_core::transactions::key_manager::TariKeyId, MicroTari), TransactionError> {
        todo!()
    }

    async fn get_script_offset(
        &self,
        script_key_ids: &[tari_core::transactions::key_manager::TariKeyId],
        sender_offset_key_ids: &[tari_core::transactions::key_manager::TariKeyId],
    ) -> Result<PrivateKey, TransactionError> {
        todo!()
    }

    async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &tari_core::transactions::key_manager::TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<Commitment, TransactionError> {
        todo!()
    }

    async fn get_metadata_signature(
        &self,
        spending_key_id: &tari_core::transactions::key_manager::TariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &tari_core::transactions::key_manager::TariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        todo!()
    }

    async fn get_receiver_partial_metadata_signature(
        &self,
        spend_key_id: &tari_core::transactions::key_manager::TariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &PublicKey,
        ephemeral_pubkey: &PublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        todo!()
    }

    async fn get_sender_partial_metadata_signature(
        &self,
        ephemeral_private_nonce_id: &tari_core::transactions::key_manager::TariKeyId,
        sender_offset_key_id: &tari_core::transactions::key_manager::TariKeyId,
        commitment: &Commitment,
        ephemeral_commitment: &Commitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        todo!()
    }

    async fn generate_burn_proof(
        &self,
        spending_key: &tari_core::transactions::key_manager::TariKeyId,
        amount: &PrivateKey,
        claim_public_key: &PublicKey,
    ) -> Result<RistrettoComSig, TransactionError> {
        todo!()
    }
}

#[async_trait]
impl SecretTransactionKeyManagerInterface for KeyManagerType {
    async fn get_private_key(
        &self,
        key_id: &tari_core::transactions::key_manager::TariKeyId,
    ) -> Result<PrivateKey, KeyManagerServiceError> {
        todo!()
    }
}

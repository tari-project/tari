//  Copyright 2022, The Tari Project
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
use std::{collections::HashMap, ops::Shl, str::FromStr, sync::Arc};

use blake2::Blake2b;
use chacha20poly1305::{Key, KeyInit, XChaCha20Poly1305};
use digest::consts::{U32, U64};
use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
#[cfg(feature = "ledger")]
use minotari_ledger_wallet_comms::accessor_methods::{
    ledger_get_dh_shared_secret_dhke,
    ledger_get_one_sided_metadata_signature,
    ledger_get_public_key,
    ledger_get_raw_schnorr_signature,
    ledger_get_script_offset,
    ledger_get_script_schnorr_signature,
    ledger_get_script_signature,
    ScriptSignatureKey,
};
use rand::{rngs::OsRng, RngCore};
use strum::IntoEnumIterator;
use tari_common_types::{
    encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce},
    key_branches::TransactionKeyManagerBranch,
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
        SignatureWithDomain,
        UncompressedComAndPubSignature,
        UncompressedSignature,
        WalletMessageSchnorrSignature,
    },
};
use tari_crypto::{
    commitment::{ExtensionDegree, HomomorphicCommitmentFactory},
    extended_range_proof::ExtendedRangeProofService,
    hashing::{DomainSeparatedHash, DomainSeparatedHasher},
    keys::SecretKey,
    range_proof::RangeProofService as RPService,
    ristretto::bulletproofs_plus::{RistrettoExtendedMask, RistrettoExtendedWitness},
};
use tari_hashing::{KeyManagerTransactionsHashDomain, WalletMessageSigningDomain};
use tari_script::{CheckSigSchnorrSignature, CompressedCheckSigSchnorrSignature, TariScript};
use tari_utilities::{ByteArray, ByteArrayError, Hidden};
use tokio::sync::RwLock;
use zeroize::Zeroize;

use crate::legacy_key_manager::{
    interface::LegacyTariKeyAndId,
    tari_key_manager::TariKeyManager,
    wallet_types::WalletType,
    AddResult,
    KeyDigest,
    KeyManagerBranch,
};

const HASHER_LABEL_STEALTH_KEY: &str = "script key";
use crate::legacy_key_manager::interface::KeyManagerState;
pub const LEDGER_NOT_SUPPORTED: &str = "Ledger is not supported in this build, please enable the \"ledger\" feature.";
use tari_transaction_components::{
    crypto_factories::CryptoFactories,
    key_manager::{
        error::KeyManagerError,
        key,
        wallet_types::{HASHER_LABEL_DERIVE_KEY, SPEND_KEY_BRANCH, VIEW_KEY_BRANCH},
        KeyManager,
        TariKeyId,
        TransactionKeyManagerInterface,
        TxoStage,
    },
    transaction_components::{
        one_sided::{
            diffie_hellman_stealth_domain_hasher_dhke,
            shared_secret_to_output_encryption_key,
            shared_secret_to_output_spending_key,
        },
        EncryptedData,
        KernelFeatures,
        MemoField,
        RangeProofType,
        TransactionError,
        TransactionInput,
        TransactionInputVersion,
        TransactionKernel,
        TransactionKernelVersion,
        TransactionOutput,
        TransactionOutputVersion,
    },
    MicroMinotari,
};
use tari_transaction_components::key_manager::SecretTransactionKeyManagerInterface;
use crate::legacy_key_manager::{
    interface::TransactionKeyManagerBackend,
    wallet_types::LegacyWalletType,
    ConfidentialOutputHasher,
    LegacyTariKeyId,
};

const PRE_MINE: &str = "pre-mine";

#[derive(Clone)]
pub struct TransactionKeyManagerInner<TBackend> {
    master_seed: CipherSeed,
    db: Option<Arc<RwLock<TBackend>>>,
    key_manager: KeyManager,
}

impl<TBackend> TransactionKeyManagerInner<TBackend>
where TBackend: TransactionKeyManagerBackend + 'static
{
    pub async fn new(
        master_seed: CipherSeed,
        db: Option<TBackend>,
        crypto_factories: CryptoFactories,
        wallet_type: Arc<LegacyWalletType>,
    ) -> Result<Self, KeyManagerError> {
        let wallet_type = wallet_type
            .to_new_wallet_type(master_seed.clone())
            .map_err(|e| KeyManagerError::UnexpectedError(format!("Failed to convert legacy wallet type: {}", e)))?;
        let key_manager = KeyManager::new_with_crypto_factories(crypto_factories, wallet_type)?;
        Ok(TransactionKeyManagerInner {
            db: db.map(|db| Arc::new(RwLock::new(db))),
            master_seed,
            key_manager,
        })
    }

    fn derive_private_key(seed: &CipherSeed, branch_seed: String, account: u64) -> Result<PrivateKey, ByteArrayError> {
        // apply domain separation to generate derive key. Under the hood, the hashing api prepends the length of each
        // piece of data for concatenation, reducing the risk of collisions due to redundancy of variable length
        // input
        let derive_key = DomainSeparatedHasher::<KeyDigest, KeyManagerDomain>::new_with_label(HASHER_LABEL_DERIVE_KEY)
            .chain(seed.entropy())
            .chain(branch_seed.as_bytes())
            .chain(account.to_le_bytes())
            .finalize();

        let derive_key = derive_key.as_ref();
        let s = PrivateKey::from_uniform_bytes(derive_key)?;
        Ok(s)
    }

    pub fn convert_legacy_tari_key_id(&self, key_id: &LegacyTariKeyId) -> Result<TariKeyId, KeyManagerError> {
        match key_id {
            LegacyTariKeyId::Managed { branch, index } => match branch.as_str() {
                SPEND_KEY_BRANCH => Ok(TariKeyId::SpendKey),
                VIEW_KEY_BRANCH => Ok(TariKeyId::ViewKey),
                _ => {
                    let private_key = Self::derive_private_key(&self.master_seed, branch.clone(), *index)?;
                    self.key_manager.import_key(private_key, None)
                },
            },
            LegacyTariKeyId::Derived { key } => Ok(TariKeyId::Derived {
                key: key.as_str().into(),
            }),
            LegacyTariKeyId::Imported { .. } => {
                let private_key = self.get_private_key(&key_id)?;
                self.key_manager.import_key(private_key, None)
            },
            LegacyTariKeyId::Zero => Ok(TariKeyId::Zero),
            LegacyTariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => Ok(TariKeyId::DHCommitmentMask {
                public_key: public_key.clone(),
                private_key: private_key.as_str().into(),
            }),
            LegacyTariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => Ok(TariKeyId::DHEncryptedData {
                public_key: public_key.clone(),
                private_key: private_key.as_str().into(),
            }),
            LegacyTariKeyId::Encrypted { encrypted, key } => Ok(TariKeyId::Encrypted {
                encrypted: encrypted.clone(),
                key: key.as_str().into(),
            }),
        }
    }

    pub fn convert_key_id_to_legacy(&self, key_id: &TariKeyId) -> LegacyTariKeyId {
        match key_id {
            TariKeyId::SpendKey => LegacyTariKeyId::Managed {
                branch: SPEND_KEY_BRANCH.to_string(),
                index: 0,
            },
            TariKeyId::ViewKey => LegacyTariKeyId::Managed {
                branch: VIEW_KEY_BRANCH.to_string(),
                index: 0,
            },
            TariKeyId::Derived { key } => LegacyTariKeyId::Derived {
                key: key.as_str().into(),
            },
            TariKeyId::Zero => LegacyTariKeyId::Zero,
            TariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => LegacyTariKeyId::DHCommitmentMask {
                public_key: public_key.clone(),
                private_key: private_key.as_str().into(),
            },
            TariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => LegacyTariKeyId::DHEncryptedData {
                public_key: public_key.clone(),
                private_key: private_key.as_str().into(),
            },
            TariKeyId::Encrypted { encrypted, key } => LegacyTariKeyId::Encrypted {
                encrypted: (*encrypted).clone(),
                key: key.as_str().into(),
            },
            TariKeyId::LedgerKey { branch, index } => LegacyTariKeyId::Managed {
                branch: branch.as_str().to_string(),
                index: *index,
            },
        }
    }

    pub fn get_random_key(&self) -> Result<LegacyTariKeyAndId, KeyManagerError> {
        let key = self.key_manager.get_random_key(None, true)?;
        let legacy_key_id = self.convert_key_id_to_legacy(&key.key_id);
        Ok(LegacyTariKeyAndId {
            key_id: legacy_key_id,
            pub_key: key.pub_key.clone(),
        })
    }

    pub fn get_public_key_at_key_id(
        &self,
        key_id: &LegacyTariKeyId,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(key_id)?;
        self.key_manager.get_public_key_at_key_id(&key)
    }

    pub(crate) fn get_private_key(&self, key_id: &LegacyTariKeyId) -> Result<PrivateKey, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(key_id)?;
        self.key_manager.get_private_key(&key)
    }

    pub fn get_wallet_type(&self) -> &WalletType {
        self.key_manager.get_wallet_type()
    }

    pub fn get_view_key(&self) -> Result<LegacyTariKeyAndId, KeyManagerError> {
        let key = self.key_manager.get_view_key()?;
        let key_id = self.convert_key_id_to_legacy(&key.key_id);
        Ok(LegacyTariKeyAndId {
            key_id,
            pub_key: key.pub_key.clone(),
        })
    }

    pub fn get_spend_key(&self) -> Result<LegacyTariKeyAndId, KeyManagerError> {
        let key = self.key_manager.get_spend_key()?;
        let key_id = self.convert_key_id_to_legacy(&key.key_id);
        Ok(LegacyTariKeyAndId {
            key_id,
            pub_key: key.pub_key.clone(),
        })
    }

    pub fn get_comms_key(&self) -> Result<LegacyTariKeyAndId, KeyManagerError> {
        let key = self.key_manager.get_spend_key()?;
        let key_id = self.convert_key_id_to_legacy(&key.key_id);
        Ok(LegacyTariKeyAndId {
            key_id,
            pub_key: key.pub_key.clone(),
        })
    }

    pub fn get_next_commitment_mask_and_script_key(
        &mut self,
    ) -> Result<(LegacyTariKeyAndId, LegacyTariKeyAndId), KeyManagerError> {
        let (key_1, key_2) = self.key_manager.get_next_commitment_mask_and_script_key()?;
        let commitment_mask_key_id = self.convert_key_id_to_legacy(&key_1.key_id);
        let script_key_id = self.convert_key_id_to_legacy(&key_2.key_id);
        let commitment_mask = LegacyTariKeyAndId {
            key_id: commitment_mask_key_id,
            pub_key: key_1.pub_key.clone(),
        };
        Ok((commitment_mask, LegacyTariKeyAndId {
            key_id: script_key_id,
            pub_key: key_2.pub_key,
        }))
    }

    pub fn import_key(
        &self,
        private_key: PrivateKey,
        encryption_key: Option<LegacyTariKeyId>,
    ) -> Result<LegacyTariKeyId, KeyManagerError> {
        let encryption_key = encryption_key.map(|key| self.convert_legacy_tari_key_id(&key)?);
        let key = self.key_manager.import_key(private_key, encryption_key)?;
        Ok(self.convert_key_id_to_legacy(&key))
    }


    pub fn get_private_view_key(&self) -> PrivateKey {
        self.key_manager.get_private_view_key()
    }

    /// Calculates a script key id from the spend key id, if a public key is provided, it will only return a result of
    /// the public keys match
    pub fn find_script_key_id_from_commitment_mask_key_id(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        public_script_key: Option<&CompressedPublicKey>,
    ) -> Result<Option<LegacyTariKeyId>, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        let result = self.key_manager.find_script_key_id_from_commitment_mask_key_id(&key, public_script_key)?;
        Ok(result.map(|key_id| self.convert_key_id_to_legacy(&key_id)))
    }

    // -----------------------------------------------------------------------------------------------------------------
    // General crypto section
    // -----------------------------------------------------------------------------------------------------------------

    pub fn get_commitment(
        &self,
        private_key: &LegacyTariKeyId,
        value: &PrivateKey,
    ) -> Result<CompressedCommitment, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(private_key)?;
        self.key_manager.get_commitment(&key, value)
    }

    /// Verify that the commitment matches the value and the spending key/mask
    pub fn verify_mask(
        &self,
        commitment: &CompressedCommitment,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        self.key_manager.verify_mask(commitment, &key, value)
    }

    pub fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &LegacyTariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(secret_key_id)?;
        self.key_manager.get_diffie_hellman_shared_secret(&key, public_key)
    }

    pub fn generate_burn_claim_signature(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: u64,
        claim_public_key: &CompressedPublicKey,
    ) -> Result<CompressedSignature, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        self.key_manager.generate_burn_claim_signature(&key, value, claim_public_key)
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction input section (transactions > transaction_components > transaction_input)
    // -----------------------------------------------------------------------------------------------------------------

    pub fn get_script_signature(
        &self,
        script_key_id: &LegacyTariKeyId,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let script_key = self.convert_legacy_tari_key_id(script_key_id)?;
        let commitment_key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        self.key_manager.get_script_signature(
            &script_key,
            &commitment_key,
            value,
            txi_version,
            script_message,
        )
    }

    pub fn get_partial_script_signature(
        &self,
        commitment_mask_id: &LegacyTariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        ephemeral_pubkey: &CompressedPublicKey,
        script_public_key: &CompressedPublicKey,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(commitment_mask_id)?;
        self.key_manager.get_partial_script_signature(
            &key,
            value,
            txi_version,
            ephemeral_pubkey,
            script_public_key,
            script_message,
        )
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction output section (transactions > transaction_components > transaction_output)
    // -----------------------------------------------------------------------------------------------------------------

    pub fn construct_range_proof(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        self.key_manager.construct_range_proof(&key, value, min_value)
    }

    #[allow(clippy::too_many_lines)]
    pub fn get_script_offset(
        &self,
        script_key_ids: &[LegacyTariKeyId],
        sender_offset_key_ids: &[LegacyTariKeyId],
    ) -> Result<PrivateKey, KeyManagerError> {
        let mut script_keys = Vec::new();
        for key in script_key_ids {
            script_keys.push(self.convert_legacy_tari_key_id(key)?);
        }
        let mut sender_offset_keys = Vec::new();
        for key in sender_offset_key_ids {
            sender_offset_keys.push(self.convert_legacy_tari_key_id(key)?);
        }
        self.key_manager.get_script_offset(&script_keys, &sender_offset_keys)
    }

    pub fn sign_script_message(
        &self,
        private_key_id: &LegacyTariKeyId,
        challenge: &[u8],
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(private_key_id)?;
        self.key_manager.sign_script_message(&key, challenge)
    }

    pub fn sign_script_message_with_spend_key(
        &self,
        message: &[u8],
        sender_offset_pub_key: Option<&CompressedPublicKey>,
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerError> {
        self.key_manager.sign_script_message_with_spend_key(message, sender_offset_pub_key)
    }

    pub fn sign_with_nonce_and_challenge(
        &self,
        private_key_id: &LegacyTariKeyId,
        nonce_key_id: &LegacyTariKeyId,
        challenge: &[u8; 64],
    ) -> Result<CompressedSignature, KeyManagerError> {
        let private_key = self.convert_legacy_tari_key_id(private_key_id)?;
        let nonce_key = self.convert_legacy_tari_key_id(nonce_key_id)?;
        self.key_manager.sign_with_nonce_and_challenge(&private_key, &nonce_key, challenge)
    }

    pub fn get_metadata_signature(
        &mut self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &LegacyTariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let commitment_key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        let sender_key = self.convert_legacy_tari_key_id(sender_offset_key_id)?;
        self.key_manager.get_metadata_signature(
            &commitment_key,
            value_as_private_key,
            &sender_key,
            txo_version,
            metadata_signature_message,
            range_proof_type,
        )
    }

    pub fn get_one_sided_metadata_signature(
        &mut self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: MicroMinotari,
        sender_offset_key_id: &LegacyTariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message_common: &[u8; 32],
        range_proof_type: RangeProofType,
        script: &TariScript,
        receiver_address: &TariAddress,
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let commitment_key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        let sender_key = self.convert_legacy_tari_key_id(sender_offset_key_id)?;
        self.key_manager.get_one_sided_metadata_signature(
            &commitment_key,
            value,
            &sender_key,
            txo_version,
            metadata_signature_message_common,
            range_proof_type,
            script,
            receiver_address,
        )
    }

    pub fn get_receiver_partial_metadata_signature(
        &mut self,
        commitment_mask_key_id: &LegacyTariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &CompressedPublicKey,
        ephemeral_pubkey: &CompressedPublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        self.key_manager.get_receiver_partial_metadata_signature(
            &key,
            value,
            sender_offset_public_key,
            ephemeral_pubkey,
            txo_version,
            metadata_signature_message,
            range_proof_type,
        )
    }

    // In the case where the sender is an aggregated signer, we need to parse in the total public key shares, this is
    // done in: aggregated_sender_offset_public_keys and aggregated_ephemeral_public_keys. If there is no aggregated
    // signers, this can be left as none
    pub fn get_sender_partial_metadata_signature(
        &self,
        ephemeral_private_nonce_id: &LegacyTariKeyId,
        sender_offset_key_id: &LegacyTariKeyId,
        commitment: &CompressedCommitment,
        ephemeral_commitment: &CompressedCommitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let nonce = self.convert_legacy_tari_key_id(ephemeral_private_nonce_id)?;
        let sender_key = self.convert_legacy_tari_key_id(sender_offset_key_id)?;
        self.key_manager.get_sender_partial_metadata_signature(
            &nonce,
            &sender_key,
            commitment,
            ephemeral_commitment,
            txo_version,
            metadata_signature_message,
        )
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction kernel section (transactions > transaction_components > transaction_kernel)
    // -----------------------------------------------------------------------------------------------------------------

    pub fn get_txo_private_kernel_offset(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        nonce_id: &LegacyTariKeyId,
    ) -> Result<PrivateKey, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        let nonce = self.convert_legacy_tari_key_id(nonce_id)?;
        self.key_manager.get_txo_private_kernel_offset(&key, &nonce)
    }

    pub fn get_partial_txo_kernel_signature(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        nonce_id: &LegacyTariKeyId,
        total_nonce: &CompressedPublicKey,
        total_excess: &CompressedPublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
        kernel_features: &KernelFeatures,
        txo_type: TxoStage,
    ) -> Result<CompressedSignature, KeyManagerError> {
        let commitment_key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        let nonce_key = self.convert_legacy_tari_key_id(nonce_id)?;
        self.key_manager.get_partial_txo_kernel_signature(
            &commitment_key,
            &nonce_key,
            total_nonce,
            total_excess,
            kernel_version,
            kernel_message,
            kernel_features,
            txo_type,
        )
    }

    pub fn get_txo_kernel_signature_excess_with_offset(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        nonce: &LegacyTariKeyId,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        let commitment_key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        let nonce_key = self.convert_legacy_tari_key_id(nonce)?;
        self.key_manager.get_txo_kernel_signature_excess_with_offset(&commitment_key, &nonce_key)
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Encrypted data section (transactions > transaction_components > encrypted_data)
    // -----------------------------------------------------------------------------------------------------------------

    pub fn encrypt_data_for_recovery(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        custom_recovery_key_id: Option<&LegacyTariKeyId>,
        value: u64,
        payment_id: MemoField,
    ) -> Result<EncryptedData, KeyManagerError> {
        let commitment_key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        let custom_recovery_key = if let Some(key_id) = custom_recovery_key_id {
            Some(self.convert_legacy_tari_key_id(key_id)?)
        } else {
            None
        };
        self.key_manager.encrypt_data_for_recovery(
            &commitment_key,
            custom_recovery_key.as_ref(),
            value,
            payment_id,
        )
    }

    pub fn extract_payment_id_from_encrypted_data(
        &self,
        encrypted_data: &EncryptedData,
        commitment: &CompressedCommitment,
        custom_recovery_key_id: Option<&LegacyTariKeyId>,
    ) -> Result<MemoField, KeyManagerError> {
        let custom_recovery_key = if let Some(key_id) = custom_recovery_key_id {
            Some(self.convert_legacy_tari_key_id(key_id)?)
        } else {
            None
        };
        let recovery_key = if let Some(key_id) = custom_recovery_key {
            self.key_manager.get_private_key(&key_id)?
        } else {
            self.get_private_view_key()?
        };
        let (_, _, payment_id) = EncryptedData::decrypt_data(&recovery_key, commitment, encrypted_data)?;
        Ok(payment_id)
    }

    pub fn try_output_key_recovery(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        sender_offset_public_key: &CompressedPublicKey,
    ) -> Result<Option<(LegacyTariKeyId, MicroMinotari, MemoField)>, KeyManagerError> {
        let res = self.key_manager.try_output_key_recovery(commitment, encrypted_data, sender_offset_public_key)?;
        Ok(res.map(|(key_id, value, payment_id)| {
            (
                self.convert_key_id_to_legacy(&key_id),
                value,
                payment_id,
            )
        }))
    }

    pub fn is_this_output_ours(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        custom_recovery_key_id: Option<PrivateKey>,
    ) -> Result<bool, KeyManagerError> {
       self.key_manager.is_this_output_ours(commitment, encrypted_data, custom_recovery_key_id)
    }

    pub fn stealth_address_script_spending_key(
        &self,
        commitment_mask_key_id: &LegacyTariKeyId,
        spend_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        let key = self.convert_legacy_tari_key_id(commitment_mask_key_id)?;
        self.key_manager.stealth_address_script_spending_key(&key, spend_key)
    }

}

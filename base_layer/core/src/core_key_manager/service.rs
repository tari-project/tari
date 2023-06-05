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
use std::{collections::HashMap, ops::Shl};

use futures::lock::Mutex;
use log::*;
use rand::rngs::OsRng;
use strum::IntoEnumIterator;
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
use tari_crypto::{
    commitment::{ExtensionDegree, HomomorphicCommitmentFactory},
    errors::RangeProofError,
    extended_range_proof::ExtendedRangeProofService,
    hash::blake2::Blake256,
    hash_domain,
    hashing::DomainSeparatedHasher,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::RangeProofService as RPService,
    ristretto::bulletproofs_plus::{RistrettoExtendedMask, RistrettoExtendedWitness},
};
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager::KeyManager,
    key_manager_service::{
        storage::database::{KeyManagerBackend, KeyManagerDatabase, KeyManagerState},
        AddResult,
        KeyDigest,
        KeyId,
        KeyManagerServiceError,
        NextKeyResult,
    },
};
use tari_utilities::{hex::Hex, ByteArray};

use crate::transactions::{
    transaction_components::{TransactionError, TransactionInput, TransactionInputVersion},
    CryptoFactories,
};

const LOG_TARGET: &str = "key_manager::key_manager_service";
const KEY_MANAGER_MAX_SEARCH_DEPTH: u64 = 1_000_000;

use crate::{
    core_key_manager::interface::CoreKeyManagerBranch,
    transactions::{
        tari_amount::MicroTari,
        transaction_components::{
            EncryptedData,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
    },
};

hash_domain!(KeyManagerHashingDomain, "base_layer.core.key_manager");

pub struct CoreKeyManagerInner<TBackend> {
    key_managers: HashMap<String, Mutex<KeyManager<PublicKey, KeyDigest>>>,
    db: KeyManagerDatabase<TBackend, PublicKey>,
    master_seed: CipherSeed,
    crypto_factories: CryptoFactories,
}

impl<TBackend> CoreKeyManagerInner<TBackend>
where TBackend: KeyManagerBackend<PublicKey> + 'static
{
    // -----------------------------------------------------------------------------------------------------------------
    // Key manager section
    // -----------------------------------------------------------------------------------------------------------------

    pub fn new(
        master_seed: CipherSeed,
        db: KeyManagerDatabase<TBackend, PublicKey>,
        crypto_factories: CryptoFactories,
    ) -> Result<Self, KeyManagerServiceError> {
        let mut km = CoreKeyManagerInner {
            key_managers: HashMap::new(),
            db,
            master_seed,
            crypto_factories,
        };
        km.add_standard_core_branches()?;
        Ok(km)
    }

    fn add_standard_core_branches(&mut self) -> Result<(), KeyManagerServiceError> {
        for branch in CoreKeyManagerBranch::iter() {
            self.add_key_manager_branch(&branch.get_branch_key())?;
        }
        Ok(())
    }

    pub fn add_key_manager_branch(&mut self, branch: &str) -> Result<AddResult, KeyManagerServiceError> {
        let result = if self.key_managers.contains_key(branch) {
            AddResult::AlreadyExists
        } else {
            AddResult::NewEntry
        };
        let state = match self.db.get_key_manager_state(branch)? {
            None => {
                let starting_state = KeyManagerState {
                    branch_seed: branch.to_string(),
                    primary_key_index: 0,
                };
                self.db.set_key_manager_state(starting_state.clone())?;
                starting_state
            },
            Some(km) => km,
        };
        self.key_managers.insert(
            branch.to_string(),
            Mutex::new(KeyManager::<PublicKey, KeyDigest>::from(
                self.master_seed.clone(),
                state.branch_seed,
                state.primary_key_index,
            )),
        );
        Ok(result)
    }

    pub async fn get_next_key(&self, branch: &str) -> Result<NextKeyResult<PublicKey>, KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;
        let derived_key = km.next_key()?;
        self.db.increment_key_index(branch)?;
        Ok(NextKeyResult {
            key: derived_key.key,
            index: km.key_index(),
        })
    }

    pub async fn get_next_key_id(&self, branch: &str) -> Result<KeyId<PublicKey>, KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;
        self.db.increment_key_index(branch)?;
        Ok(KeyId::Managed {
            branch: branch.to_string(),
            index: km.increment_key_index(1),
        })
    }

    pub async fn get_static_key_id(&self, branch: &str) -> Result<KeyId<PublicKey>, KeyManagerServiceError> {
        match self.key_managers.get(branch) {
            None => Err(KeyManagerServiceError::UnknownKeyBranch),
            Some(_) => Ok(KeyId::Managed {
                branch: branch.to_string(),
                index: 0,
            }),
        }
    }

    pub async fn get_key_at_index(&self, branch: &str, index: u64) -> Result<PrivateKey, KeyManagerServiceError> {
        let km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;
        let derived_key = km.derive_key(index)?;
        Ok(derived_key.key)
    }

    pub async fn get_public_key_at_key_id(
        &self,
        key_id: &KeyId<PublicKey>,
    ) -> Result<PublicKey, KeyManagerServiceError> {
        match key_id {
            KeyId::Managed { branch, index } => {
                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                    .lock()
                    .await;
                Ok(km.derive_public_key(*index)?.key)
            },
            KeyId::Imported { key } => Ok(key.clone()),
        }
    }

    pub async fn get_next_spend_and_script_key_ids(
        &self,
    ) -> Result<(KeyId<PublicKey>, KeyId<PublicKey>), KeyManagerServiceError> {
        let spend_key_id = self
            .get_next_key_id(&CoreKeyManagerBranch::CommitmentMask.get_branch_key())
            .await?;
        let index = spend_key_id
            .managed_index()
            .ok_or(KeyManagerServiceError::KyeIdWithoutIndex)?;
        self.db
            .set_key_index(&CoreKeyManagerBranch::ScriptKey.get_branch_key(), index)?;
        let script_key_id = KeyId::Managed {
            branch: CoreKeyManagerBranch::ScriptKey.get_branch_key(),
            index,
        };
        Ok((spend_key_id, script_key_id))
    }

    /// Search the specified branch key manager key chain to find the index of the specified key.
    pub async fn find_key_index(&self, branch: &str, key: &PublicKey) -> Result<u64, KeyManagerServiceError> {
        let km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;

        let current_index = km.key_index();

        for i in 0u64..current_index + KEY_MANAGER_MAX_SEARCH_DEPTH {
            let public_key = PublicKey::from_secret_key(&km.derive_key(i)?.key);
            if public_key == *key {
                trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, i);
                return Ok(i);
            }
        }

        Err(KeyManagerServiceError::KeyNotFoundInKeyChain)
    }

    /// If the supplied index is higher than the current UTXO key chain indices then they will be updated.
    pub async fn update_current_key_index_if_higher(
        &self,
        branch: &str,
        index: u64,
    ) -> Result<(), KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;
        let current_index = km.key_index();
        if index > current_index {
            km.update_key_index(index);
            self.db.set_key_index(branch, index)?;
            trace!(target: LOG_TARGET, "Updated UTXO Key Index to {}", index);
        }
        Ok(())
    }

    pub async fn import_key(&self, private_key: PrivateKey) -> Result<KeyId<PublicKey>, KeyManagerServiceError> {
        let public_key = PublicKey::from_secret_key(&private_key);
        let hex_key = public_key.to_hex();
        self.db.insert_imported_key(public_key.clone(), private_key)?;
        trace!(target: LOG_TARGET, "Imported key {}", hex_key);
        let key_id = KeyId::Imported { key: public_key };
        Ok(key_id)
    }

    // Note!: This method may not be made public
    async fn get_private_key(&self, key_id: &KeyId<PublicKey>) -> Result<PrivateKey, KeyManagerServiceError> {
        match key_id {
            KeyId::Managed { branch, index } => {
                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                    .lock()
                    .await;
                let key = km.get_private_key(*index)?;
                Ok(key)
            },
            KeyId::Imported { key } => {
                let pvt_key = self.db.get_imported_key(key)?;
                Ok(pvt_key)
            },
        }
    }

    // -----------------------------------------------------------------------------------------------------------------
    // General crypto section
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_commitment(
        &self,
        private_key: &KeyId<PublicKey>,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError> {
        let key = self.get_private_key(private_key).await?;
        Ok(self.crypto_factories.commitment.commit(&key, value))
    }

    /// Verify that the commitment matches the value and the spending key/mask
    pub async fn verify_mask(
        &self,
        prover: &RangeProofService,
        commitment: &Commitment,
        spending_key_id: &KeyId<PublicKey>,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError> {
        let spending_key = self.get_private_key(spending_key_id).await?;
        Ok(prover.verify_mask(commitment, &spending_key, value)?)
    }

    pub async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &KeyId<PublicKey>,
        public_key: &PublicKey,
    ) -> Result<CommsDHKE, TransactionError> {
        let secret_key = self.get_private_key(secret_key_id).await?;
        let shared_secret = CommsDHKE::new(&secret_key, public_key);
        Ok(shared_secret)
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction input section (transactions > transaction_components > transaction_input)
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_script_signature(
        &self,
        script_key_id: &KeyId<PublicKey>,
        spend_key_id: &KeyId<PublicKey>,
        value: &PrivateKey,
        tx_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let r_a = PrivateKey::random(&mut OsRng);
        let r_x = PrivateKey::random(&mut OsRng);
        let r_y = PrivateKey::random(&mut OsRng);
        let ephemeral_commitment = self.crypto_factories.commitment.commit(&r_x, &r_a);
        let ephemeral_pubkey = PublicKey::from_secret_key(&r_y);
        let commitment = self.get_commitment(spend_key_id, value).await?;
        let script_private_key = self.get_private_key(script_key_id).await?;
        let spend_private_key = self.get_private_key(spend_key_id).await?;

        let challenge = TransactionInput::finalize_script_signature_challenge(
            tx_version,
            &ephemeral_commitment,
            &ephemeral_pubkey,
            &PublicKey::from_secret_key(&script_private_key),
            &commitment,
            script_message,
        );

        let script_signature = ComAndPubSignature::sign(
            value,
            &spend_private_key,
            &script_private_key,
            &r_a,
            &r_x,
            &r_y,
            &challenge,
            &*self.crypto_factories.commitment,
        )?;
        Ok(script_signature)
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction output section (transactions > transaction_components > transaction_output)
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_spending_key_id(
        &self,
        public_spending_key: &PublicKey,
    ) -> Result<KeyId<PublicKey>, TransactionError> {
        let index = self
            .find_key_index(
                &CoreKeyManagerBranch::CommitmentMask.get_branch_key(),
                public_spending_key,
            )
            .await?;
        let spending_key_id = KeyId::Managed {
            branch: CoreKeyManagerBranch::CommitmentMask.get_branch_key(),
            index,
        };
        Ok(spending_key_id)
    }

    pub async fn construct_range_proof(
        &self,
        private_key: &KeyId<PublicKey>,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError> {
        if self.crypto_factories.range_proof.range() < 64 &&
            value >= 1u64.shl(&self.crypto_factories.range_proof.range())
        {
            return Err(TransactionError::ValidationError(
                "Value provided is outside the range allowed by the range proof".into(),
            ));
        }

        let spend_private_key = self.get_private_key(private_key).await?;
        let proof_bytes_result = if min_value == 0 {
            self.crypto_factories
                .range_proof
                .construct_proof(&spend_private_key, value)
        } else {
            let extended_mask =
                RistrettoExtendedMask::assign(ExtensionDegree::DefaultPedersen, vec![spend_private_key])?;

            let extended_witness = RistrettoExtendedWitness {
                mask: extended_mask,
                value,
                minimum_value_promise: min_value,
            };

            self.crypto_factories
                .range_proof
                .construct_extended_proof(vec![extended_witness], None)
        };

        let proof_bytes = proof_bytes_result.map_err(|err| {
            TransactionError::RangeProofError(RangeProofError::ProofConstructionError(format!(
                "Failed to construct range proof: {}",
                err
            )))
        })?;

        RangeProof::from_bytes(&proof_bytes).map_err(|_| {
            TransactionError::RangeProofError(RangeProofError::ProofConstructionError(
                "Rangeproof factory returned invalid range proof bytes".to_string(),
            ))
        })
    }

    pub async fn get_script_offset(
        &self,
        script_key_ids: &[KeyId<PublicKey>],
        sender_offset_key_ids: &[KeyId<PublicKey>],
    ) -> Result<PrivateKey, TransactionError> {
        let mut total_sender_offset_private_key = PrivateKey::default();
        for sender_offset_key_id in sender_offset_key_ids {
            total_sender_offset_private_key =
                total_sender_offset_private_key + self.get_private_key(sender_offset_key_id).await?;
        }
        let mut total_script_private_key = PrivateKey::default();
        for script_key_id in script_key_ids {
            total_script_private_key = total_script_private_key + self.get_private_key(script_key_id).await?;
        }
        let script_offset = total_script_private_key - total_sender_offset_private_key;
        Ok(script_offset)
    }

    // Note!: This method may not be made public
    async fn get_metadata_signature_ephemeral_private_key_pair(
        &self,
        nonce_id: &KeyId<PublicKey>,
    ) -> Result<(PrivateKey, PrivateKey), TransactionError> {
        let nonce_private_key = self.get_private_key(nonce_id).await?;
        let hasher_a = DomainSeparatedHasher::<Blake256, KeyManagerHashingDomain>::new_with_label(
            "metadata_signature_ephemeral_nonce_a",
        );
        let a_hash = hasher_a.chain(nonce_private_key.as_bytes()).finalize();
        let nonce_a = PrivateKey::from_bytes(a_hash.as_ref()).map_err(|_| {
            TransactionError::ConversionError("Invalid private key for sender offset private key".to_string())
        })?;
        let hasher_b = DomainSeparatedHasher::<Blake256, KeyManagerHashingDomain>::new_with_label(
            "metadata_signature_ephemeral_nonce_b",
        );
        let b_hash = hasher_b.chain(nonce_private_key.as_bytes()).finalize();
        let nonce_b = PrivateKey::from_bytes(b_hash.as_ref()).map_err(|_| {
            TransactionError::ConversionError("Invalid private key for sender offset private key".to_string())
        })?;
        Ok((nonce_a, nonce_b))
    }

    pub async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &KeyId<PublicKey>,
    ) -> Result<Commitment, TransactionError> {
        let (nonce_a, nonce_b) = self.get_metadata_signature_ephemeral_private_key_pair(nonce_id).await?;
        Ok(self.crypto_factories.commitment.commit(&nonce_a, &nonce_b))
    }

    pub async fn get_metadata_signature(
        &self,
        value_as_private_key: &PrivateKey,
        spending_key_id: &KeyId<PublicKey>,
        sender_offset_private_key: &PrivateKey,
        nonce_a: &PrivateKey,
        nonce_b: &PrivateKey,
        nonce_x: &PrivateKey,
        challenge_bytes: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let spending_key = self.get_private_key(spending_key_id).await?;
        Ok(ComAndPubSignature::sign(
            value_as_private_key,
            &spending_key,
            sender_offset_private_key,
            nonce_a,
            nonce_b,
            nonce_x,
            challenge_bytes,
            &*self.crypto_factories.commitment,
        )?)
    }

    pub async fn get_receiver_partial_metadata_signature(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        value: &PrivateKey,
        nonce_id: &KeyId<PublicKey>,
        sender_offset_public_key: &PublicKey,
        ephemeral_pubkey: &PublicKey,
        tx_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let (nonce_a, nonce_b) = self.get_metadata_signature_ephemeral_private_key_pair(nonce_id).await?;
        let ephemeral_commitment = self.crypto_factories.commitment.commit(&nonce_a, &nonce_b);
        let spend_private_key = self.get_private_key(spend_key_id).await?;
        let commitment = self.crypto_factories.commitment.commit(&spend_private_key, value);
        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            tx_version,
            sender_offset_public_key,
            &ephemeral_commitment,
            ephemeral_pubkey,
            &commitment,
            metadata_signature_message,
        );

        let metadata_signature = ComAndPubSignature::sign(
            value,
            &spend_private_key,
            &PrivateKey::default(),
            &nonce_a,
            &nonce_b,
            &PrivateKey::default(),
            &challenge,
            &*self.crypto_factories.commitment,
        )?;
        Ok(metadata_signature)
    }

    pub async fn get_sender_partial_metadata_signature(
        &self,
        nonce_id: &KeyId<PublicKey>,
        sender_offset_key_id: &KeyId<PublicKey>,
        commitment: &Commitment,
        ephemeral_commitment: &Commitment,
        tx_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let ephemeral_private_key = self.get_private_key(nonce_id).await?;
        let ephemeral_pubkey = PublicKey::from_secret_key(&ephemeral_private_key);
        let sender_offset_private_key = self.get_private_key(sender_offset_key_id).await?;
        let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);

        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            tx_version,
            &sender_offset_public_key,
            ephemeral_commitment,
            &ephemeral_pubkey,
            commitment,
            metadata_signature_message,
        );

        let metadata_signature = ComAndPubSignature::sign(
            &PrivateKey::default(),
            &PrivateKey::default(),
            &sender_offset_private_key,
            &PrivateKey::default(),
            &PrivateKey::default(),
            &ephemeral_private_key,
            &challenge,
            &*self.crypto_factories.commitment,
        )?;
        Ok(metadata_signature)
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction kernel section (transactions > transaction_components > transaction_kernel)
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_partial_private_kernel_offset(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        nonce_id: &KeyId<PublicKey>,
    ) -> Result<PrivateKey, TransactionError> {
        let hasher = DomainSeparatedHasher::<Blake256, KeyManagerHashingDomain>::new_with_label("kernel_excess_offset");
        let spending_private_key = self.get_private_key(spend_key_id).await?;
        let nonce_private_key = self.get_private_key(nonce_id).await?;
        let key_hash = hasher
            .chain(spending_private_key.as_bytes())
            .chain(nonce_private_key.as_bytes())
            .finalize();
        PrivateKey::from_bytes(key_hash.as_ref()).map_err(|_| {
            TransactionError::ConversionError("Invalid private key for kernel signature nonce".to_string())
        })
    }

    pub async fn get_partial_kernel_signature(
        &self,
        spending_key: &KeyId<PublicKey>,
        nonce_id: &KeyId<PublicKey>,
        total_nonce: &PublicKey,
        total_excess: &PublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
    ) -> Result<Signature, TransactionError> {
        let spending_private_key = self.get_private_key(spending_key).await?;
        let private_nonce = self.get_private_key(nonce_id).await?;
        let signing_key =
            spending_private_key - &self.get_partial_private_kernel_offset(spending_key, nonce_id).await?;
        let challenge = TransactionKernel::finalize_kernel_signature_challenge(
            kernel_version,
            total_nonce,
            total_excess,
            kernel_message,
        );

        let signature = Signature::sign_raw(&signing_key, private_nonce, &challenge)?;
        Ok(signature)
    }

    pub async fn get_partial_kernel_signature_excess(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        nonce_id: &KeyId<PublicKey>,
    ) -> Result<PublicKey, TransactionError> {
        let offset = self.get_partial_private_kernel_offset(spend_key_id, nonce_id).await?;
        let excess = self.get_private_key(spend_key_id).await?;
        let combined_excess = excess - &offset;
        Ok(PublicKey::from_secret_key(&combined_excess))
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Encrypted data section (transactions > transaction_components > encrypted_data)
    // -----------------------------------------------------------------------------------------------------------------

    // Note!: This method may not be made public
    async fn get_recovery_key(&self) -> Result<PrivateKey, KeyManagerServiceError> {
        let recovery_id = KeyId::Managed {
            branch: CoreKeyManagerBranch::DataEncryption.get_branch_key(),
            index: 0,
        };
        self.get_private_key(&recovery_id).await
    }

    pub async fn encrypt_data_for_recovery(
        &self,
        spend_key_id: &KeyId<PublicKey>,
        custom_recovery_key_id: &Option<KeyId<PublicKey>>,
        value: u64,
    ) -> Result<EncryptedData, TransactionError> {
        let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            self.get_private_key(key_id).await?
        } else {
            self.get_recovery_key().await?
        };
        let value_key = value.into();
        let commitment = self.get_commitment(spend_key_id, &value_key).await?;
        let spend_key = self.get_private_key(spend_key_id).await?;
        let data = EncryptedData::encrypt_data(&recovery_key, &commitment, value.into(), &spend_key)?;
        Ok(data)
    }

    pub async fn try_commitment_key_recovery(
        &self,
        commitment: &Commitment,
        encrypted_data: &EncryptedData,
        custom_recovery_key_id: &Option<KeyId<PublicKey>>,
    ) -> Result<(KeyId<PublicKey>, MicroTari), TransactionError> {
        let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            self.get_private_key(key_id).await?
        } else {
            self.get_recovery_key().await?
        };
        let (value, private_key) = EncryptedData::decrypt_data(&recovery_key, commitment, encrypted_data)?;
        self.crypto_factories
            .range_proof
            .verify_mask(commitment, &private_key, value.into())?;
        let public_key = PublicKey::from_secret_key(&private_key);
        let key = match self
            .find_key_index(&CoreKeyManagerBranch::CommitmentMask.get_branch_key(), &public_key)
            .await
        {
            Ok(index) => {
                self.update_current_key_index_if_higher(&CoreKeyManagerBranch::CommitmentMask.get_branch_key(), index)
                    .await?;
                KeyId::Managed {
                    branch: CoreKeyManagerBranch::CommitmentMask.get_branch_key(),
                    index,
                }
            },
            Err(_) => {
                self.import_key(private_key).await?;
                KeyId::Imported { key: public_key }
            },
        };
        Ok((key, value))
    }
}

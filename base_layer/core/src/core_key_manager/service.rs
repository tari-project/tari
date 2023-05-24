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
use tari_common_types::types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature};
use tari_crypto::{
    commitment::{ExtensionDegree, HomomorphicCommitmentFactory},
    errors::RangeProofError,
    extended_range_proof::ExtendedRangeProofService,
    hash::blake2::Blake256,
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::RangeProofService,
    ristretto::bulletproofs_plus::{RistrettoExtendedMask, RistrettoExtendedWitness},
};
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager::KeyManager,
    key_manager_service::{
        storage::database::{KeyManagerBackend, KeyManagerDatabase, KeyManagerState},
        AddResult,
        KeyDigest,
        KeyManagerServiceError,
        NextKeyResult,
    },
};
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    core_key_manager::interface::KeyId,
    transactions::{
        transaction_components::{TransactionError, TransactionInput, TransactionInputVersion},
        CryptoFactories,
    },
};

const LOG_TARGET: &str = "key_manager::key_manager_service";
const KEY_MANAGER_MAX_SEARCH_DEPTH: u64 = 1_000_000;
use tari_crypto::{hash_domain, hashing::DomainSeparatedHasher};

use crate::transactions::transaction_components::{TransactionKernel, TransactionKernelVersion};

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
    pub fn new(
        master_seed: CipherSeed,
        db: KeyManagerDatabase<TBackend, PublicKey>,
        crypto_factories: CryptoFactories,
    ) -> Self {
        CoreKeyManagerInner {
            key_managers: HashMap::new(),
            db,
            master_seed,
            crypto_factories,
        }
    }

    pub fn add_key_manager_branch(&mut self, branch: String) -> Result<AddResult, KeyManagerServiceError> {
        let result = if self.key_managers.contains_key(&branch) {
            AddResult::AlreadyExists
        } else {
            AddResult::NewEntry
        };
        let state = match self.db.get_key_manager_state(branch.clone())? {
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
            branch,
            Mutex::new(KeyManager::<PublicKey, KeyDigest>::from(
                self.master_seed.clone(),
                state.branch_seed,
                state.primary_key_index,
            )),
        );
        Ok(result)
    }

    pub async fn get_next_key(&self, branch: String) -> Result<NextKeyResult<PublicKey>, KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(&branch)
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

    pub async fn get_key_at_index(&self, branch: String, index: u64) -> Result<PrivateKey, KeyManagerServiceError> {
        let km = self
            .key_managers
            .get(&branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .lock()
            .await;
        let derived_key = km.derive_key(index)?;
        Ok(derived_key.key)
    }

    /// Search the specified branch key manager key chain to find the index of the specified key.
    pub async fn find_key_index(&self, branch: String, key: &PublicKey) -> Result<u64, KeyManagerServiceError> {
        let km = self
            .key_managers
            .get(&branch)
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
        branch: String,
        index: u64,
    ) -> Result<(), KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(&branch)
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

    pub async fn import_key(&self, private_key: PrivateKey) -> Result<(), KeyManagerServiceError> {
        let public_key = PublicKey::from_secret_key(&private_key);
        let hex_key = public_key.to_hex();
        self.db.insert_imported_key(public_key, private_key)?;
        trace!(target: LOG_TARGET, "Imported key {}", hex_key);
        Ok(())
    }

    async fn get_private_key(&self, key_id: &KeyId) -> Result<PrivateKey, KeyManagerServiceError> {
        match key_id {
            KeyId::Default { branch, index } => {
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

    pub async fn get_commitment(
        &self,
        private_key: &KeyId,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError> {
        let key = self.get_private_key(private_key).await?;
        Ok(self.crypto_factories.commitment.commit(&key, value))
    }

    pub async fn get_script_signature(
        &self,
        script_key: &KeyId,
        spending_key: &KeyId,
        value: &PrivateKey,
        tx_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let r_a = PrivateKey::random(&mut OsRng);
        let r_x = PrivateKey::random(&mut OsRng);
        let r_y = PrivateKey::random(&mut OsRng);
        let ephemeral_commitment = self.crypto_factories.commitment.commit(&r_x, &r_a);
        let ephemeral_pubkey = PublicKey::from_secret_key(&r_y);
        let commitment = self.get_commitment(spending_key, value).await?;
        let script_private_key = self.get_private_key(script_key).await?;
        let spend_private_key = self.get_private_key(spending_key).await?;

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

    pub async fn construct_range_proof(
        &self,
        private_key: &KeyId,
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

    async fn get_private_kernel_signature_nonce(&self, spending_key: &KeyId) -> Result<PrivateKey, TransactionError> {
        let hasher = DomainSeparatedHasher::<Blake256, KeyManagerHashingDomain>::new_with_label("kernel_private_nonce");
        let spending_private_key = self.get_private_key(spending_key).await?;
        let spending_key_hash = hasher.chain(spending_private_key.as_bytes()).finalize();
        PrivateKey::from_bytes(spending_key_hash.as_ref()).map_err(|_| {
            TransactionError::ConversionError("Invalid private key for kernel signature nonce".to_string())
        })
    }

    pub async fn get_partial_kernel_signature(
        &self,
        spending_key: &KeyId,
        total_nonce: &PublicKey,
        total_excess: &PublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
    ) -> Result<Signature, TransactionError> {
        let spending_private_key = self.get_private_key(spending_key).await?;
        let private_nonce = self.get_private_kernel_signature_nonce(spending_key).await?;
        let challenge = TransactionKernel::finalize_kernel_signature_challenge(
            kernel_version,
            total_nonce,
            total_excess,
            kernel_message,
        );

        let signature = Signature::sign_raw(&spending_private_key, private_nonce, &challenge)?;
        Ok(signature)
    }

    pub async fn get_kernel_signature_nonce(&self, spending_key: &KeyId) -> Result<PublicKey, TransactionError> {
        let private_key = self.get_private_kernel_signature_nonce(spending_key).await?;
        Ok(PublicKey::from_secret_key(&private_key))
    }
}

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
#[cfg(feature = "ledger")]
use std::sync::{Arc, Mutex};
use std::{collections::HashMap, ops::Shl};

use blake2::Blake2b;
use digest::consts::U64;
#[cfg(feature = "ledger")]
use ledger_transport::APDUCommand;
#[cfg(feature = "ledger")]
use ledger_transport_hid::TransportNativeHID;
use log::*;
#[cfg(feature = "ledger")]
use once_cell::sync::Lazy;
use rand::rngs::OsRng;
use strum::IntoEnumIterator;
use tari_common_types::{
    types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature},
    wallet_types::WalletType,
};
use tari_comms::types::CommsDHKE;
use tari_crypto::{
    commitment::{ExtensionDegree, HomomorphicCommitmentFactory},
    extended_range_proof::ExtendedRangeProofService,
    hash_domain,
    hashing::{DomainSeparatedHash, DomainSeparatedHasher},
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::RangeProofService as RPService,
    ristretto::{
        bulletproofs_plus::{RistrettoExtendedMask, RistrettoExtendedWitness},
        RistrettoComSig,
    },
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
    },
};
use tari_utilities::{hex::Hex, ByteArray};
use tokio::sync::RwLock;

const LOG_TARGET: &str = "key_manager::key_manager_service";
const KEY_MANAGER_MAX_SEARCH_DEPTH: u64 = 1_000_000;

use crate::{
    common::ConfidentialOutputHasher,
    one_sided::diffie_hellman_stealth_domain_hasher,
    transactions::{
        key_manager::{
            interface::{TransactionKeyManagerBranch, TxoStage},
            LedgerDeviceError,
            TariKeyId,
        },
        tari_amount::MicroMinotari,
        transaction_components::{
            EncryptedData,
            KernelFeatures,
            RangeProofType,
            TransactionError,
            TransactionInput,
            TransactionInputVersion,
            TransactionKernel,
            TransactionKernelVersion,
            TransactionOutput,
            TransactionOutputVersion,
        },
        CryptoFactories,
    },
};

hash_domain!(
    KeyManagerHashingDomain,
    "com.tari.base_layer.core.transactions.key_manager",
    1
);

pub struct TransactionKeyManagerInner<TBackend> {
    key_managers: HashMap<String, RwLock<KeyManager<PublicKey, KeyDigest>>>,
    db: KeyManagerDatabase<TBackend, PublicKey>,
    master_seed: CipherSeed,
    crypto_factories: CryptoFactories,
    wallet_type: WalletType,
}

#[cfg(feature = "ledger")]
pub static TRANSPORT: Lazy<Arc<Mutex<Option<TransportNativeHID>>>> = Lazy::new(|| Arc::new(Mutex::new(None)));

impl<TBackend> TransactionKeyManagerInner<TBackend>
where TBackend: KeyManagerBackend<PublicKey> + 'static
{
    // -----------------------------------------------------------------------------------------------------------------
    // Key manager section
    // -----------------------------------------------------------------------------------------------------------------

    pub fn new(
        master_seed: CipherSeed,
        db: KeyManagerDatabase<TBackend, PublicKey>,
        crypto_factories: CryptoFactories,
        wallet_type: WalletType,
    ) -> Result<Self, KeyManagerServiceError> {
        let mut km = TransactionKeyManagerInner {
            key_managers: HashMap::new(),
            db,
            master_seed,
            crypto_factories,
            wallet_type,
        };
        km.add_standard_core_branches()?;
        Ok(km)
    }

    fn add_standard_core_branches(&mut self) -> Result<(), KeyManagerServiceError> {
        for branch in TransactionKeyManagerBranch::iter() {
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
            RwLock::new(KeyManager::<PublicKey, KeyDigest>::from(
                self.master_seed.clone(),
                state.branch_seed,
                state.primary_key_index,
            )),
        );
        Ok(result)
    }

    pub async fn get_next_key(&self, branch: &str) -> Result<(TariKeyId, PublicKey), KeyManagerServiceError> {
        let mut km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .write()
            .await;
        self.db.increment_key_index(branch)?;
        let index = km.increment_key_index(1);
        let key = km.derive_public_key(index)?.key;
        Ok((
            KeyId::Managed {
                branch: branch.to_string(),
                index,
            },
            key,
        ))
    }

    pub async fn get_static_key(&self, branch: &str) -> Result<TariKeyId, KeyManagerServiceError> {
        match self.key_managers.get(branch) {
            None => Err(KeyManagerServiceError::UnknownKeyBranch),
            Some(_) => Ok(KeyId::Managed {
                branch: branch.to_string(),
                index: 0,
            }),
        }
    }

    pub async fn get_public_key_at_key_id(&self, key_id: &TariKeyId) -> Result<PublicKey, KeyManagerServiceError> {
        match key_id {
            KeyId::Managed { branch, index } => {
                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                    .read()
                    .await;
                Ok(km.derive_public_key(*index)?.key)
            },
            KeyId::Imported { key } => Ok(key.clone()),
            KeyId::Zero => Ok(PublicKey::default()),
        }
    }

    pub async fn get_next_spend_and_script_key_ids(
        &self,
    ) -> Result<(TariKeyId, PublicKey, TariKeyId, PublicKey), KeyManagerServiceError> {
        let (spend_key_id, spend_public_key) = self
            .get_next_key(&TransactionKeyManagerBranch::CommitmentMask.get_branch_key())
            .await?;
        let index = spend_key_id
            .managed_index()
            .ok_or(KeyManagerServiceError::KyeIdWithoutIndex)?;
        self.db
            .set_key_index(&TransactionKeyManagerBranch::ScriptKey.get_branch_key(), index)?;
        let script_key_id = KeyId::Managed {
            branch: TransactionKeyManagerBranch::ScriptKey.get_branch_key(),
            index,
        };
        let script_public_key = self.get_public_key_at_key_id(&script_key_id).await?;
        Ok((spend_key_id, spend_public_key, script_key_id, script_public_key))
    }

    /// Calculates a script key id from the spend key id, if a public key is provided, it will only return a result of
    /// the public keys match
    pub async fn find_script_key_id_from_spend_key_id(
        &self,
        spend_key_id: &TariKeyId,
        public_script_key: Option<&PublicKey>,
    ) -> Result<Option<TariKeyId>, KeyManagerServiceError> {
        let index = match spend_key_id {
            KeyId::Managed { index, .. } => *index,
            KeyId::Imported { .. } => return Ok(None),
            KeyId::Zero => return Ok(None),
        };
        let script_key_id = KeyId::Managed {
            branch: TransactionKeyManagerBranch::ScriptKey.get_branch_key(),
            index,
        };

        if let Some(key) = public_script_key {
            let script_public_key = self.get_public_key_at_key_id(&script_key_id).await?;
            if *key == script_public_key {
                return Ok(Some(script_key_id));
            }
            return Ok(None);
        }
        Ok(Some(script_key_id))
    }

    /// Search the specified branch key manager key chain to find the index of the specified key.
    pub async fn find_key_index(&self, branch: &str, key: &PublicKey) -> Result<u64, KeyManagerServiceError> {
        let km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .read()
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

    /// Search the specified branch key manager key chain to find the index of the specified private key.
    async fn find_private_key_index(&self, branch: &str, key: &PrivateKey) -> Result<u64, KeyManagerServiceError> {
        let km = self
            .key_managers
            .get(branch)
            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
            .read()
            .await;

        let current_index = km.key_index();

        for i in 0u64..current_index + KEY_MANAGER_MAX_SEARCH_DEPTH {
            let private_key = &km.derive_key(i)?.key;
            if private_key == key {
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
            .write()
            .await;
        let current_index = km.key_index();
        if index > current_index {
            km.update_key_index(index);
            self.db.set_key_index(branch, index)?;
            trace!(target: LOG_TARGET, "Updated UTXO Key Index to {}", index);
        }
        Ok(())
    }

    pub async fn import_key(&self, private_key: PrivateKey) -> Result<TariKeyId, KeyManagerServiceError> {
        let public_key = PublicKey::from_secret_key(&private_key);
        let hex_key = public_key.to_hex();
        self.db.insert_imported_key(public_key.clone(), private_key)?;
        trace!(target: LOG_TARGET, "Imported key {}", hex_key);
        let key_id = KeyId::Imported { key: public_key };
        Ok(key_id)
    }

    pub(crate) async fn get_private_key(&self, key_id: &TariKeyId) -> Result<PrivateKey, KeyManagerServiceError> {
        match key_id {
            KeyId::Managed { branch, index } => {
                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                    .read()
                    .await;
                let key = km.get_private_key(*index)?;
                Ok(key)
            },
            KeyId::Imported { key } => {
                let pvt_key = self.db.get_imported_key(key)?;
                Ok(pvt_key)
            },
            KeyId::Zero => Ok(PrivateKey::default()),
        }
    }

    // -----------------------------------------------------------------------------------------------------------------
    // General crypto section
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_commitment(
        &self,
        private_key: &TariKeyId,
        value: &PrivateKey,
    ) -> Result<Commitment, KeyManagerServiceError> {
        let key = self.get_private_key(private_key).await?;
        Ok(self.crypto_factories.commitment.commit(&key, value))
    }

    /// Verify that the commitment matches the value and the spending key/mask
    pub async fn verify_mask(
        &self,
        commitment: &Commitment,
        spending_key_id: &TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError> {
        let spending_key = self.get_private_key(spending_key_id).await?;
        self.crypto_factories
            .range_proof
            .verify_mask(commitment, &spending_key, value)
            .map_err(|e| e.into())
    }

    pub async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<CommsDHKE, TransactionError> {
        let secret_key = self.get_private_key(secret_key_id).await?;
        let shared_secret = CommsDHKE::new(&secret_key, public_key);
        Ok(shared_secret)
    }

    pub async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<DomainSeparatedHash<Blake2b<U64>>, TransactionError> {
        let secret_key = self.get_private_key(secret_key_id).await?;
        Ok(diffie_hellman_stealth_domain_hasher(&secret_key, public_key))
    }

    pub async fn import_add_offset_to_private_key(
        &self,
        secret_key_id: &TariKeyId,
        offset: PrivateKey,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        let secret_key = self.get_private_key(secret_key_id).await?;
        self.import_key(secret_key + offset).await
    }

    pub async fn generate_burn_proof(
        &self,
        spending_key: &TariKeyId,
        amount: &PrivateKey,
        claim_public_key: &PublicKey,
    ) -> Result<RistrettoComSig, TransactionError> {
        let nonce_a = PrivateKey::random(&mut OsRng);
        let nonce_x = PrivateKey::random(&mut OsRng);
        let pub_nonce = self.crypto_factories.commitment.commit(&nonce_x, &nonce_a);

        let commitment = self.get_commitment(spending_key, amount).await?;

        let challenge = ConfidentialOutputHasher::new("commitment_signature")
            .chain(&pub_nonce)
            .chain(&commitment)
            .chain(claim_public_key)
            .finalize();

        let spend_key = self.get_private_key(spending_key).await?;

        RistrettoComSig::sign(
            amount,
            &spend_key,
            &nonce_a,
            &nonce_x,
            &challenge,
            &*self.crypto_factories.commitment,
        )
        .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction input section (transactions > transaction_components > transaction_input)
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_script_private_key(&self, script_key_id: &TariKeyId) -> Result<PrivateKey, TransactionError> {
        match self.wallet_type {
            WalletType::Software => self.get_private_key(script_key_id).await.map_err(|e| e.into()),
            WalletType::Ledger(_account) => {
                #[cfg(not(feature = "ledger"))]
                return Err(TransactionError::LedgerDeviceError(LedgerDeviceError::NotSupported));

                #[cfg(feature = "ledger")]
                {
                    let data = script_key_id.managed_index().expect("and index").to_le_bytes().to_vec();
                    let command = APDUCommand {
                        cla: 0x80,
                        ins: 0x02, // GetPrivateKey - see `./applications/mp_ledger/src/main.rs/Instruction`
                        p1: 0x00,
                        p2: 0x00,
                        data,
                    };
                    let binding = TRANSPORT.lock().expect("lock exists");
                    let transport = binding.as_ref().expect("transport exists");
                    match transport.exchange(&command) {
                        Ok(result) => {
                            if result.data().len() < 33 {
                                return Err(LedgerDeviceError::Processing(format!(
                                    "'get_private_key' insufficient data - expected 33 got {} bytes ({:?})",
                                    result.data().len(),
                                    result
                                ))
                                .into());
                            }
                            PrivateKey::from_canonical_bytes(&result.data()[1..33])
                                .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))
                        },
                        Err(e) => Err(LedgerDeviceError::Instruction(format!("GetPrivateKey: {}", e)).into()),
                    }
                }
                // end script private key
            },
        }
    }

    pub async fn get_script_signature(
        &self,
        script_key_id: &TariKeyId,
        spend_key_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let r_a = PrivateKey::random(&mut OsRng);
        let r_x = PrivateKey::random(&mut OsRng);
        let r_y = PrivateKey::random(&mut OsRng);
        let ephemeral_commitment = self.crypto_factories.commitment.commit(&r_x, &r_a);
        let ephemeral_pubkey = PublicKey::from_secret_key(&r_y);
        let commitment = self.get_commitment(spend_key_id, value).await?;
        let spend_private_key = self.get_private_key(spend_key_id).await?;
        let script_private_key = self.get_script_private_key(script_key_id).await?;

        let challenge = TransactionInput::finalize_script_signature_challenge(
            txi_version,
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

    pub async fn get_spending_key_id(&self, public_spending_key: &PublicKey) -> Result<TariKeyId, TransactionError> {
        let index = self
            .find_key_index(
                &TransactionKeyManagerBranch::CommitmentMask.get_branch_key(),
                public_spending_key,
            )
            .await?;
        let spending_key_id = KeyId::Managed {
            branch: TransactionKeyManagerBranch::CommitmentMask.get_branch_key(),
            index,
        };
        Ok(spending_key_id)
    }

    pub async fn construct_range_proof(
        &self,
        private_key: &TariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, TransactionError> {
        if self.crypto_factories.range_proof.range() < 64 &&
            value >= 1u64.shl(&self.crypto_factories.range_proof.range())
        {
            return Err(TransactionError::BuilderError(
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

        let proof_bytes = proof_bytes_result
            .map_err(|err| TransactionError::RangeProofError(format!("Failed to construct range proof: {}", err)))?;

        RangeProof::from_canonical_bytes(&proof_bytes).map_err(|_| {
            TransactionError::RangeProofError("Rangeproof factory returned invalid range proof bytes".to_string())
        })
    }

    pub async fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
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

    async fn get_metadata_signature_ephemeral_private_key_pair(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<(PrivateKey, PrivateKey), TransactionError> {
        let nonce_private_key = self.get_private_key(nonce_id).await?;
        // With BulletProofPlus type range proofs, the nonce is a secure random value
        // With RevealedValue type range proofs, the nonce is always 0 and the minimum value promise equal to the value
        let nonce_a = match range_proof_type {
            RangeProofType::BulletProofPlus => {
                let hasher_a = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerHashingDomain>::new_with_label(
                    "metadata_signature_ephemeral_nonce_a",
                );
                let a_hash = hasher_a.chain(nonce_private_key.as_bytes()).finalize();
                PrivateKey::from_uniform_bytes(a_hash.as_ref()).map_err(|_| {
                    TransactionError::KeyManagerError("Invalid private key for sender offset private key".to_string())
                })
            },
            RangeProofType::RevealedValue => Ok(PrivateKey::default()),
        }?;

        let hasher_b = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerHashingDomain>::new_with_label(
            "metadata_signature_ephemeral_nonce_b",
        );
        let b_hash = hasher_b.chain(nonce_private_key.as_bytes()).finalize();
        let nonce_b = PrivateKey::from_uniform_bytes(b_hash.as_ref()).map_err(|_| {
            TransactionError::KeyManagerError("Invalid private key for sender offset private key".to_string())
        })?;
        Ok((nonce_a, nonce_b))
    }

    pub async fn get_metadata_signature_ephemeral_commitment(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<Commitment, TransactionError> {
        let (nonce_a, nonce_b) = self
            .get_metadata_signature_ephemeral_private_key_pair(nonce_id, range_proof_type)
            .await?;
        Ok(self.crypto_factories.commitment.commit(&nonce_b, &nonce_a))
    }

    pub async fn get_metadata_signature_raw(
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
    ) -> Result<ComAndPubSignature, TransactionError> {
        let sender_offset_public_key = self.get_public_key_at_key_id(sender_offset_key_id).await?;
        let receiver_partial_metadata_signature = self
            .get_receiver_partial_metadata_signature(
                spending_key_id,
                value_as_private_key,
                &sender_offset_public_key,
                ephemeral_pubkey,
                txo_version,
                metadata_signature_message,
                range_proof_type,
            )
            .await?;
        let commitment = self.get_commitment(spending_key_id, value_as_private_key).await?;
        let sender_partial_metadata_signature = self
            .get_sender_partial_metadata_signature(
                ephemeral_private_nonce_id,
                sender_offset_key_id,
                &commitment,
                ephemeral_commitment,
                txo_version,
                metadata_signature_message,
            )
            .await?;
        let metadata_signature = &receiver_partial_metadata_signature + &sender_partial_metadata_signature;
        Ok(metadata_signature)
    }

    pub async fn get_metadata_signature(
        &self,
        spending_key_id: &TariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &TariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        let sender_offset_public_key = self.get_public_key_at_key_id(sender_offset_key_id).await?;
        let (ephemeral_private_nonce_id, ephemeral_pubkey) = self
            .get_next_key(&TransactionKeyManagerBranch::Nonce.get_branch_key())
            .await?;
        let receiver_partial_metadata_signature = self
            .get_receiver_partial_metadata_signature(
                spending_key_id,
                value_as_private_key,
                &sender_offset_public_key,
                &ephemeral_pubkey,
                txo_version,
                metadata_signature_message,
                range_proof_type,
            )
            .await?;
        let commitment = self.get_commitment(spending_key_id, value_as_private_key).await?;
        let ephemeral_commitment = receiver_partial_metadata_signature.ephemeral_commitment();
        let sender_partial_metadata_signature = self
            .get_sender_partial_metadata_signature(
                &ephemeral_private_nonce_id,
                sender_offset_key_id,
                &commitment,
                ephemeral_commitment,
                txo_version,
                metadata_signature_message,
            )
            .await?;
        let metadata_signature = &receiver_partial_metadata_signature + &sender_partial_metadata_signature;
        Ok(metadata_signature)
    }

    pub async fn get_receiver_partial_metadata_signature(
        &self,
        spend_key_id: &TariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &PublicKey,
        ephemeral_pubkey: &PublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        let (ephemeral_commitment_nonce_id, _) = self
            .get_next_key(&TransactionKeyManagerBranch::Nonce.get_branch_key())
            .await?;
        let (nonce_a, nonce_b) = self
            .get_metadata_signature_ephemeral_private_key_pair(&ephemeral_commitment_nonce_id, range_proof_type)
            .await?;
        let ephemeral_commitment = self.crypto_factories.commitment.commit(&nonce_b, &nonce_a);
        let spend_private_key = self.get_private_key(spend_key_id).await?;
        let commitment = self.crypto_factories.commitment.commit(&spend_private_key, value);
        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            txo_version,
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
        ephemeral_private_nonce_id: &TariKeyId,
        sender_offset_key_id: &TariKeyId,
        commitment: &Commitment,
        ephemeral_commitment: &Commitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let ephemeral_private_key = self.get_private_key(ephemeral_private_nonce_id).await?;
        let ephemeral_pubkey = PublicKey::from_secret_key(&ephemeral_private_key);
        let sender_offset_private_key = self.get_private_key(sender_offset_key_id).await?;
        let sender_offset_public_key = PublicKey::from_secret_key(&sender_offset_private_key);

        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            txo_version,
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

    pub async fn get_txo_private_kernel_offset(
        &self,
        spend_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, TransactionError> {
        let hasher =
            DomainSeparatedHasher::<Blake2b<U64>, KeyManagerHashingDomain>::new_with_label("kernel_excess_offset");
        let spending_private_key = self.get_private_key(spend_key_id).await?;
        let nonce_private_key = self.get_private_key(nonce_id).await?;
        let key_hash = hasher
            .chain(spending_private_key.as_bytes())
            .chain(nonce_private_key.as_bytes())
            .finalize();
        PrivateKey::from_uniform_bytes(key_hash.as_ref()).map_err(|_| {
            TransactionError::KeyManagerError("Invalid private key for kernel signature nonce".to_string())
        })
    }

    pub async fn get_partial_txo_kernel_signature(
        &self,
        spending_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
        total_nonce: &PublicKey,
        total_excess: &PublicKey,
        kernel_version: &TransactionKernelVersion,
        kernel_message: &[u8; 32],
        kernel_features: &KernelFeatures,
        txo_type: TxoStage,
    ) -> Result<Signature, TransactionError> {
        let private_key = self.get_private_key(spending_key_id).await?;
        // We cannot use an offset with a coinbase tx as this will not allow us to check the coinbase commitment and
        // because the offset function does not know if its a coinbase or not, we need to know if we need to bypass it
        // or not
        let private_signing_key = if kernel_features.is_coinbase() {
            private_key
        } else {
            private_key - &self.get_txo_private_kernel_offset(spending_key_id, nonce_id).await?
        };

        // We need to check if its input or output for which we are singing. Signing with an input, we need to sign
        // with `-k` while outputs are `k`
        let final_signing_key = if txo_type == TxoStage::Output {
            private_signing_key
        } else {
            PrivateKey::default() - &private_signing_key
        };

        let private_nonce = self.get_private_key(nonce_id).await?;
        let challenge = TransactionKernel::finalize_kernel_signature_challenge(
            kernel_version,
            total_nonce,
            total_excess,
            kernel_message,
        );

        let signature = Signature::sign_raw_uniform(&final_signing_key, private_nonce, &challenge)?;
        Ok(signature)
    }

    pub async fn get_txo_kernel_signature_excess_with_offset(
        &self,
        spend_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PublicKey, TransactionError> {
        let private_key = self.get_private_key(spend_key_id).await?;
        let offset = self.get_txo_private_kernel_offset(spend_key_id, nonce_id).await?;
        let excess = private_key - &offset;
        Ok(PublicKey::from_secret_key(&excess))
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Encrypted data section (transactions > transaction_components > encrypted_data)
    // -----------------------------------------------------------------------------------------------------------------

    async fn get_recovery_key(&self) -> Result<PrivateKey, KeyManagerServiceError> {
        let recovery_id = KeyId::Managed {
            branch: TransactionKeyManagerBranch::DataEncryption.get_branch_key(),
            index: 0,
        };
        self.get_private_key(&recovery_id).await
    }

    pub async fn encrypt_data_for_recovery(
        &self,
        spend_key_id: &TariKeyId,
        custom_recovery_key_id: Option<&TariKeyId>,
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

    pub async fn try_output_key_recovery(
        &self,
        output: &TransactionOutput,
        custom_recovery_key_id: Option<&TariKeyId>,
    ) -> Result<(TariKeyId, MicroMinotari), TransactionError> {
        let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            self.get_private_key(key_id).await?
        } else {
            self.get_recovery_key().await?
        };
        let (value, private_key) =
            EncryptedData::decrypt_data(&recovery_key, output.commitment(), output.encrypted_data())?;
        self.crypto_factories
            .range_proof
            .verify_mask(output.commitment(), &private_key, value.into())?;
        // Detect the branch we need to scan on for the key.
        let branch = if output.is_coinbase() {
            TransactionKeyManagerBranch::Coinbase.get_branch_key()
        } else {
            TransactionKeyManagerBranch::CommitmentMask.get_branch_key()
        };
        let key = match self.find_private_key_index(&branch, &private_key).await {
            Ok(index) => {
                self.update_current_key_index_if_higher(&branch, index).await?;
                KeyId::Managed { branch, index }
            },
            Err(_) => {
                let public_key = PublicKey::from_secret_key(&private_key);
                self.import_key(private_key).await?;
                KeyId::Imported { key: public_key }
            },
        };
        Ok((key, value))
    }
}

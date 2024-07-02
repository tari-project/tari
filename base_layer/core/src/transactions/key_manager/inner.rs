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

use blake2::Blake2b;
use digest::consts::U64;
use log::*;
#[cfg(feature = "ledger")]
use minotari_ledger_wallet_comms::{
    error::LedgerDeviceError,
    ledger_wallet::{get_transport, Instruction},
};
use rand::rngs::OsRng;
use strum::IntoEnumIterator;
#[cfg(feature = "ledger")]
use tari_common_types::wallet_types::LedgerWallet;
use tari_common_types::{
    types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, Signature},
    wallet_types::WalletType,
};
use tari_comms::types::CommsDHKE;
use tari_crypto::{
    commitment::{ExtensionDegree, HomomorphicCommitmentFactory},
    extended_range_proof::ExtendedRangeProofService,
    hashing::{DomainSeparatedHash, DomainSeparatedHasher},
    keys::{PublicKey as PublicKeyTrait, SecretKey},
    range_proof::RangeProofService as RPService,
    ristretto::{
        bulletproofs_plus::{RistrettoExtendedMask, RistrettoExtendedWitness},
        RistrettoComSig,
    },
};
use tari_hashing::KeyManagerTransactionsHashDomain;
#[cfg(feature = "ledger")]
use tari_key_manager::error::KeyManagerError;
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

const LOG_TARGET: &str = "c::bn::key_manager::key_manager_service";
const TRANSACTION_KEY_MANAGER_MAX_SEARCH_DEPTH: u64 = 1_000_000;

use crate::{
    common::ConfidentialOutputHasher,
    one_sided::diffie_hellman_stealth_domain_hasher,
    transactions::{
        key_manager::{
            interface::{TransactionKeyManagerBranch, TransactionKeyManagerLabel, TxoStage},
            TariKeyId,
        },
        tari_amount::MicroMinotari,
        transaction_components::{
            encrypted_data::PaymentId,
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

pub struct TransactionKeyManagerInner<TBackend> {
    key_managers: HashMap<String, RwLock<KeyManager<PublicKey, KeyDigest>>>,
    db: KeyManagerDatabase<TBackend, PublicKey>,
    master_seed: CipherSeed,
    crypto_factories: CryptoFactories,
    wallet_type: WalletType,
}

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
        let index = {
            let mut km = self
                .key_managers
                .get(branch)
                .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                .write()
                .await;
            self.db.increment_key_index(branch)?;
            km.increment_key_index(1)
        };
        let key_id = KeyId::Managed {
            branch: branch.to_string(),
            index,
        };
        let key = self.get_public_key_at_key_id(&key_id).await?;
        Ok((key_id, key))
    }

    pub async fn create_key_pair(&mut self, branch: &str) -> Result<(TariKeyId, PublicKey), KeyManagerServiceError> {
        self.add_key_manager_branch(branch)?;
        let (key_id, public_key) = self.get_next_key(branch).await?;
        Ok((key_id, public_key))
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
                // If we have the unique case of being a ledger wallet, and the key is a Managed EphemeralNonce, or
                // SenderOffset than we fetch from the ledger, all other keys are fetched below.
                #[allow(unused_variables)]
                if let WalletType::Ledger(ledger) = &self.wallet_type {
                    if branch == &TransactionKeyManagerBranch::MetadataEphemeralNonce.get_branch_key() ||
                        branch == &TransactionKeyManagerBranch::SenderOffset.get_branch_key()
                    {
                        #[cfg(not(feature = "ledger"))]
                        {
                            return Err(KeyManagerServiceError::LedgerError(
                                "Ledger is not supported".to_string(),
                            ));
                        }

                        #[cfg(feature = "ledger")]
                        {
                            let transport =
                                get_transport().map_err(|e| KeyManagerServiceError::LedgerError(e.to_string()))?;
                            let mut data = index.to_le_bytes().to_vec();
                            let branch_u8 = TransactionKeyManagerBranch::from_key(branch).as_byte();
                            data.extend_from_slice(&u64::from(branch_u8).to_le_bytes());
                            let command = ledger.build_command(Instruction::GetPublicKey, data);

                            match command.execute_with_transport(&transport) {
                                Ok(result) => {
                                    debug!(target: LOG_TARGET, "result length: {}, data: {:?}", result.data().len(), result.data());
                                    if result.data().len() < 33 {
                                        debug!(target: LOG_TARGET, "result less than 33");
                                    }

                                    return PublicKey::from_canonical_bytes(&result.data()[1..33])
                                        .map_err(|e| KeyManagerServiceError::LedgerError(e.to_string()));
                                },
                                Err(e) => return Err(KeyManagerServiceError::LedgerError(e.to_string())),
                            }
                        }
                    }

                    if &TransactionKeyManagerBranch::DataEncryption.get_branch_key() == branch {
                        let view_key = ledger
                            .view_key
                            .clone()
                            .ok_or(KeyManagerServiceError::LedgerViewKeyInaccessible)?;
                        return Ok(PublicKey::from_secret_key(&view_key));
                    }
                }

                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                    .read()
                    .await;
                Ok(km.derive_public_key(*index)?.key)
            },
            KeyId::Derived { branch, label, index } => {
                let public_alpha = match &self.wallet_type {
                    WalletType::Software => {
                        let km = self
                            .key_managers
                            .get(&TransactionKeyManagerBranch::Alpha.get_branch_key())
                            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                            .read()
                            .await;

                        km.derive_public_key(0)?.key
                    },
                    WalletType::Ledger(ledger) => {
                        ledger.public_alpha.clone().ok_or(KeyManagerServiceError::LedgerError(
                            "Key manager set to use ledger, ledger alpha public key missing".to_string(),
                        ))?
                    },
                };
                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                    .read()
                    .await;
                let branch_key = km.get_private_key(*index)?;
                let hasher = Self::get_domain_hasher(label)?;
                let hasher = hasher.chain(branch_key.as_bytes()).finalize();
                let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                    KeyManagerServiceError::UnknownError(
                        "Invalid private key for sender offset private key".to_string(),
                    )
                })?;
                let public_key = PublicKey::from_secret_key(&private_key);
                let public_key = public_alpha + &public_key;
                Ok(public_key)
            },
            KeyId::Imported { key } => Ok(key.clone()),
            KeyId::Zero => Ok(PublicKey::default()),
        }
    }

    fn get_domain_hasher(
        label: &str,
    ) -> Result<DomainSeparatedHasher<Blake2b<U64>, KeyManagerTransactionsHashDomain>, KeyManagerServiceError> {
        let tx_label = label.parse::<TransactionKeyManagerLabel>().map_err(|e| {
            KeyManagerServiceError::UnknownError(format!("Could not retrieve label for derived key: {}", e))
        })?;
        match tx_label {
            TransactionKeyManagerLabel::ScriptKey => Ok(DomainSeparatedHasher::<
                Blake2b<U64>,
                KeyManagerTransactionsHashDomain,
            >::new_with_label("script key")),
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
        let script_key_id = KeyId::Derived {
            branch: TransactionKeyManagerBranch::CommitmentMask.get_branch_key(),
            label: TransactionKeyManagerLabel::ScriptKey.get_branch_key(),
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
            KeyId::Derived { .. } => return Ok(None),
            KeyId::Imported { .. } => return Ok(None),
            KeyId::Zero => return Ok(None),
        };
        let script_key_id = KeyId::Derived {
            branch: TransactionKeyManagerBranch::CommitmentMask.get_branch_key(),
            label: TransactionKeyManagerLabel::ScriptKey.get_branch_key(),
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

        for i in 0u64..TRANSACTION_KEY_MANAGER_MAX_SEARCH_DEPTH {
            let index = current_index + i;
            let public_key = PublicKey::from_secret_key(&km.derive_key(index)?.key);
            if public_key == *key {
                trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, i);
                return Ok(index);
            }
            if i <= current_index && i != 0u64 {
                let index = current_index - i;
                let public_key = PublicKey::from_secret_key(&km.derive_key(index)?.key);
                if public_key == *key {
                    trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, index);
                    return Ok(index);
                }
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

        // its most likely that the key is close to the current index, so we start searching from the current index
        for i in 0u64..TRANSACTION_KEY_MANAGER_MAX_SEARCH_DEPTH {
            let index = current_index + i;
            let private_key = &km.derive_key(index)?.key;
            if private_key == key {
                trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, index);
                return Ok(index);
            }
            if i <= current_index && i != 0u64 {
                let index = current_index - i;
                let private_key = &km.derive_key(index)?.key;
                if private_key == key {
                    trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, index);
                    return Ok(index);
                }
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
                if let WalletType::Ledger(wallet) = &self.wallet_type {
                    // In the event we're asking for the view key, and we use a ledger, reference the stored key
                    if &TransactionKeyManagerBranch::DataEncryption.get_branch_key() == branch {
                        return wallet
                            .view_key
                            .clone()
                            .ok_or(KeyManagerServiceError::LedgerViewKeyInaccessible);
                    }

                    // If we're trying to access any of the private keys, just say no bueno
                    if &TransactionKeyManagerBranch::Alpha.get_branch_key() == branch ||
                        &TransactionKeyManagerBranch::SenderOffset.get_branch_key() == branch ||
                        &TransactionKeyManagerBranch::MetadataEphemeralNonce.get_branch_key() == branch
                    {
                        return Err(KeyManagerServiceError::LedgerPrivateKeyInaccessible);
                    }
                };

                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                    .read()
                    .await;
                let key = km.get_private_key(*index)?;
                Ok(key)
            },
            KeyId::Derived { branch, label, index } => match &self.wallet_type {
                WalletType::Ledger(_) => Err(KeyManagerServiceError::LedgerPrivateKeyInaccessible),
                WalletType::Software => {
                    let km = self
                        .key_managers
                        .get(&TransactionKeyManagerBranch::Alpha.get_branch_key())
                        .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                        .read()
                        .await;

                    let private_alpha = km.get_private_key(0)?;

                    let km = self
                        .key_managers
                        .get(branch)
                        .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                        .read()
                        .await;
                    let branch_key = km.get_private_key(*index)?;
                    let hasher = Self::get_domain_hasher(label)?;
                    let hasher = hasher.chain(branch_key.as_bytes()).finalize();
                    let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                        KeyManagerServiceError::UnknownError(format!("Invalid private key for {}", label))
                    })?;
                    let private_key = private_key + private_alpha;
                    Ok(private_key)
                },
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
        #[allow(unused_variables)]
        if let WalletType::Ledger(ledger) = &self.wallet_type {
            if let KeyId::Managed { branch, index } = secret_key_id {
                if branch == &TransactionKeyManagerBranch::SenderOffset.get_branch_key() {
                    #[cfg(not(feature = "ledger"))]
                    {
                        return Err(TransactionError::LedgerNotSupported);
                    }

                    #[cfg(feature = "ledger")]
                    {
                        return self.device_diffie_hellman(ledger, branch, index, public_key).await;
                    }
                }
            }
        }

        let secret_key = self.get_private_key(secret_key_id).await?;
        let shared_secret = CommsDHKE::new(&secret_key, public_key);
        Ok(shared_secret)
    }

    pub async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &PublicKey,
    ) -> Result<DomainSeparatedHash<Blake2b<U64>>, TransactionError> {
        #[allow(unused_variables)]
        if let WalletType::Ledger(ledger) = &self.wallet_type {
            if let KeyId::Managed { branch, index } = secret_key_id {
                if branch == &TransactionKeyManagerBranch::SenderOffset.get_branch_key() {
                    #[cfg(not(feature = "ledger"))]
                    {
                        return Err(TransactionError::LedgerNotSupported);
                    }

                    #[cfg(feature = "ledger")]
                    {
                        return self
                            .device_diffie_hellman(ledger, branch, index, public_key)
                            .await
                            .map(diffie_hellman_stealth_domain_hasher);
                    }
                }
            }
        }

        let secret_key = self.get_private_key(secret_key_id).await?;
        let dh = CommsDHKE::new(&secret_key, public_key);
        Ok(diffie_hellman_stealth_domain_hasher(dh))
    }

    #[allow(unused_variables)] // conditionally compiled paths
    #[cfg(feature = "ledger")]
    async fn device_diffie_hellman(
        &self,
        ledger: &LedgerWallet,
        branch: &str,
        index: &u64,
        public_key: &PublicKey,
    ) -> Result<CommsDHKE, TransactionError> {
        let transport = get_transport().map_err(|e| KeyManagerServiceError::LedgerError(e.to_string()))?;
        let mut data = index.to_le_bytes().to_vec();
        let branch_u8 = TransactionKeyManagerBranch::from_key(branch).as_byte();
        data.extend_from_slice(&u64::from(branch_u8).to_le_bytes());
        data.extend_from_slice(public_key.as_bytes());
        let command = ledger.build_command(Instruction::GetDHSharedSecret, data);

        return match command.execute_with_transport(&transport) {
            Ok(result) => {
                debug!(target: LOG_TARGET, "result length: {}, data: {:?}", result.data().len(), result.data());
                if result.data().len() < 33 {
                    debug!(target: LOG_TARGET, "result less than 33");
                }

                return CommsDHKE::from_canonical_bytes(&result.data()[1..33])
                    .map_err(|e| LedgerDeviceError::ByteArrayError(e.to_string()).into());
            },
            Err(e) => Err(KeyManagerServiceError::LedgerError(e.to_string()).into()),
        };
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

    pub async fn get_script_signature(
        &self,
        script_key_id: &TariKeyId,
        spend_key_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let commitment = self.get_commitment(spend_key_id, value).await?;
        let spend_private_key = self.get_private_key(spend_key_id).await?;

        #[allow(unused_variables)] // When ledger isn't enabled
        match (&self.wallet_type, script_key_id) {
            (
                WalletType::Ledger(ledger),
                KeyId::Derived {
                    branch,
                    label: _,
                    index,
                },
            ) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported)
                }

                #[cfg(feature = "ledger")]
                {
                    let km = self
                        .key_managers
                        .get(branch)
                        .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                        .read()
                        .await;
                    let branch_key = km
                        .get_private_key(*index)
                        .map_err(|e| TransactionError::KeyManagerError(e.to_string()))?;

                    let mut data = u64::from(ledger.network.as_byte()).to_le_bytes().to_vec();
                    data.extend_from_slice(&u64::from(txi_version.as_u8()).to_le_bytes());
                    data.extend_from_slice(branch_key.as_bytes());
                    data.extend_from_slice(value.as_bytes());
                    data.extend_from_slice(spend_private_key.as_bytes());
                    data.extend_from_slice(commitment.as_bytes());
                    data.extend_from_slice(script_message);

                    let command = ledger.build_command(Instruction::GetScriptSignature, data);
                    let transport = get_transport()?;

                    match command.execute_with_transport(&transport) {
                        Ok(result) => {
                            if result.data().len() < 161 {
                                debug!(target: LOG_TARGET, "result less than 161");
                                return Err(LedgerDeviceError::Processing(format!(
                                    "'get_script_signature' insufficient data - expected 161 got {} bytes ({:?})",
                                    result.data().len(),
                                    result
                                ))
                                .into());
                            }
                            let data = result.data();
                            debug!(target: LOG_TARGET, "result length: {}, data: {:?}", result.data().len(), result.data());
                            Ok(ComAndPubSignature::new(
                                Commitment::from_canonical_bytes(&data[1..33])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PublicKey::from_canonical_bytes(&data[33..65])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[65..97])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[97..129])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[129..161])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                            ))
                        },
                        Err(e) => Err(LedgerDeviceError::Instruction(format!("GetScriptSignature: {}", e)).into()),
                    }
                }
            },
            (_, _) => {
                let r_a = PrivateKey::random(&mut OsRng);
                let r_x = PrivateKey::random(&mut OsRng);
                let r_y = PrivateKey::random(&mut OsRng);
                let ephemeral_commitment = self.crypto_factories.commitment.commit(&r_x, &r_a);
                let ephemeral_pubkey = PublicKey::from_secret_key(&r_y);
                let script_private_key = self.get_private_key(script_key_id).await?;

                let challenge = TransactionInput::finalize_script_signature_challenge(
                    txi_version,
                    &ephemeral_commitment,
                    &ephemeral_pubkey,
                    &self.get_public_key_at_key_id(script_key_id).await?,
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
            },
        }
    }

    pub async fn get_script_signature_from_challenge(
        &self,
        script_key_id: &TariKeyId,
        spend_key_id: &TariKeyId,
        value: &PrivateKey,
        challenge: &[u8; 64],
        r_a: &PrivateKey,
        r_x: &PrivateKey,
        r_y: &PrivateKey,
    ) -> Result<ComAndPubSignature, TransactionError> {
        let spend_private_key = self.get_private_key(spend_key_id).await?;

        #[allow(unused_variables)] // When ledger isn't enabled
        match (&self.wallet_type, script_key_id) {
            (
                WalletType::Ledger(ledger),
                KeyId::Derived {
                    branch,
                    label: _,
                    index,
                },
            ) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported)
                }

                #[cfg(feature = "ledger")]
                {
                    let km = self
                        .key_managers
                        .get(branch)
                        .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                        .read()
                        .await;
                    let branch_key = km
                        .get_private_key(*index)
                        .map_err(|e| TransactionError::KeyManagerError(e.to_string()))?;

                    let mut data = branch_key.as_bytes().to_vec();
                    data.extend_from_slice(value.as_bytes());
                    data.extend_from_slice(spend_private_key.as_bytes());
                    data.extend_from_slice(challenge);
                    data.extend_from_slice(r_a.as_bytes());
                    data.extend_from_slice(r_x.as_bytes());
                    data.extend_from_slice(r_y.as_bytes());

                    let command = ledger.build_command(Instruction::GetScriptSignatureFromChallenge, data);
                    let transport = get_transport()?;

                    match command.execute_with_transport(&transport) {
                        Ok(result) => {
                            if result.data().len() < 161 {
                                debug!(target: LOG_TARGET, "result less than 161");
                                return Err(LedgerDeviceError::Processing(format!(
                                    "'get_script_signature' insufficient data - expected 161 got {} bytes ({:?})",
                                    result.data().len(),
                                    result
                                ))
                                .into());
                            }
                            let data = result.data();
                            debug!(target: LOG_TARGET, "result length: {}, data: {:?}", result.data().len(), result.data());
                            Ok(ComAndPubSignature::new(
                                Commitment::from_canonical_bytes(&data[1..33])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PublicKey::from_canonical_bytes(&data[33..65])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[65..97])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[97..129])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[129..161])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                            ))
                        },
                        Err(e) => Err(LedgerDeviceError::Instruction(format!(
                            "GetScriptSignatureFromChallenge: {}",
                            e
                        ))
                        .into()),
                    }
                }
            },
            (_, _) => {
                let script_private_key = self.get_private_key(script_key_id).await?;

                let script_signature = ComAndPubSignature::sign(
                    value,
                    &spend_private_key,
                    &script_private_key,
                    r_a,
                    r_x,
                    r_y,
                    challenge.as_slice(),
                    &*self.crypto_factories.commitment,
                )?;
                Ok(script_signature)
            },
        }
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

    #[allow(clippy::too_many_lines)]
    pub async fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
    ) -> Result<PrivateKey, TransactionError> {
        let mut total_script_private_key = PrivateKey::default();
        let mut derived_key_commitments = vec![];
        for script_key_id in script_key_ids {
            match script_key_id {
                KeyId::Imported { .. } | KeyId::Managed { .. } | KeyId::Zero => {
                    total_script_private_key = total_script_private_key + self.get_private_key(script_key_id).await?
                },
                KeyId::Derived {
                    branch,
                    label: _,
                    index,
                } => match &self.wallet_type {
                    WalletType::Software => {
                        total_script_private_key =
                            total_script_private_key + self.get_private_key(script_key_id).await?;
                    },
                    WalletType::Ledger(_) => {
                        let km = self
                            .key_managers
                            .get(branch)
                            .ok_or(KeyManagerServiceError::UnknownKeyBranch)?
                            .read()
                            .await;
                        let branch_key = km
                            .get_private_key(*index)
                            .map_err(|e| TransactionError::KeyManagerError(e.to_string()))?;
                        derived_key_commitments.push(branch_key);
                    },
                },
            }
        }

        match &self.wallet_type {
            WalletType::Software => {
                let mut total_sender_offset_private_key = PrivateKey::default();
                for sender_offset_key_id in sender_offset_key_ids {
                    total_sender_offset_private_key =
                        total_sender_offset_private_key + self.get_private_key(sender_offset_key_id).await?;
                }
                let script_offset = total_script_private_key - total_sender_offset_private_key;
                Ok(script_offset)
            },
            #[allow(unused_variables)]
            WalletType::Ledger(ledger) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported)
                }

                #[cfg(feature = "ledger")]
                {
                    let mut sender_offset_indexes = vec![];
                    for sender_offset_key_id in sender_offset_key_ids {
                        match sender_offset_key_id {
                            TariKeyId::Managed { branch: _, index } |
                            TariKeyId::Derived {
                                branch: _,
                                label: _,
                                index,
                            } => {
                                sender_offset_indexes.push(index);
                            },
                            TariKeyId::Imported { .. } | TariKeyId::Zero => {},
                        }
                    }

                    let num_commitments = derived_key_commitments.len() as u64;
                    let num_offset_key = sender_offset_indexes.len() as u64;

                    let mut instructions = num_offset_key.to_le_bytes().to_vec();
                    instructions.extend_from_slice(&num_commitments.to_le_bytes());

                    let mut data: Vec<Vec<u8>> = vec![instructions.to_vec()];
                    data.push(total_script_private_key.to_vec());

                    for sender_offset_index in sender_offset_indexes {
                        data.push(sender_offset_index.to_le_bytes().to_vec());
                    }

                    for derived_key_commitment in derived_key_commitments {
                        data.push(derived_key_commitment.to_vec());
                    }

                    let commands = ledger.chunk_command(Instruction::GetScriptOffset, data);
                    let transport = get_transport()?;

                    let mut result = None;
                    for command in commands {
                        match command.execute_with_transport(&transport) {
                            Ok(r) => result = Some(r),
                            Err(e) => {
                                return Err(LedgerDeviceError::Instruction(format!("GetScriptOffset: {}", e)).into())
                            },
                        }
                    }

                    if let Some(result) = result {
                        if result.data().len() < 33 {
                            debug!(target: LOG_TARGET, "result less than 33");
                            return Err(LedgerDeviceError::Processing(format!(
                                "'get_script_offset' insufficient data - expected 33 got {} bytes ({:?})",
                                result.data().len(),
                                result
                            ))
                            .into());
                        }
                        let data = result.data();
                        debug!(target: LOG_TARGET, "result length: {}, data: {:?}", result.data().len(), result.data());
                        return PrivateKey::from_canonical_bytes(&data[1..33])
                            .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()));
                    }

                    Err(
                        LedgerDeviceError::Instruction("GetScriptOffset failed to process correctly".to_string())
                            .into(),
                    )
                }
            },
        }
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
                let hasher_a = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                    "metadata_signature_ephemeral_nonce_a",
                );
                let a_hash = hasher_a.chain(nonce_private_key.as_bytes()).finalize();
                PrivateKey::from_uniform_bytes(a_hash.as_ref()).map_err(|_| {
                    TransactionError::KeyManagerError("Invalid private key for sender offset private key".to_string())
                })
            },
            RangeProofType::RevealedValue => Ok(PrivateKey::default()),
        }?;

        let hasher_b = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
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

    pub async fn sign_message(
        &self,
        private_key_id: &TariKeyId,
        challenge: &[u8],
    ) -> Result<Signature, TransactionError> {
        let private_key = self.get_private_key(private_key_id).await?;
        let nonce = PrivateKey::random(&mut OsRng);
        let signature = Signature::sign_with_nonce_and_message(&private_key, nonce, challenge)?;

        Ok(signature)
    }

    pub async fn sign_with_nonce_and_message(
        &self,
        private_key_id: &TariKeyId,
        nonce: &TariKeyId,
        challenge: &[u8],
    ) -> Result<Signature, TransactionError> {
        let private_key = self.get_private_key(private_key_id).await?;
        let private_nonce = self.get_private_key(nonce).await?;
        let signature = Signature::sign_with_nonce_and_message(&private_key, private_nonce, challenge)?;

        Ok(signature)
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
        // Use the pubkey, but generate the nonce on ledger
        let (ephemeral_private_nonce_id, ephemeral_pubkey) = self
            .get_next_key(&TransactionKeyManagerBranch::MetadataEphemeralNonce.get_branch_key())
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
        match &self.wallet_type {
            WalletType::Software => {
                let ephemeral_private_key = self.get_private_key(ephemeral_private_nonce_id).await?;
                let ephemeral_pubkey = PublicKey::from_secret_key(&ephemeral_private_key);
                let sender_offset_private_key = self.get_private_key(sender_offset_key_id).await?; // Take the index and use it to find the key from ledger
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
            },
            #[allow(unused_variables)]
            WalletType::Ledger(ledger) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported)
                }

                #[cfg(feature = "ledger")]
                {
                    let ephemeral_private_nonce_index =
                        ephemeral_private_nonce_id
                            .managed_index()
                            .ok_or(TransactionError::KeyManagerError(
                                KeyManagerError::InvalidKeyID.to_string(),
                            ))?;
                    let sender_offset_key_index =
                        sender_offset_key_id
                            .managed_index()
                            .ok_or(TransactionError::KeyManagerError(
                                KeyManagerError::InvalidKeyID.to_string(),
                            ))?;

                    let mut data = u64::from(ledger.network.as_byte()).to_le_bytes().to_vec();
                    data.extend_from_slice(&u64::from(txo_version.as_u8()).to_le_bytes());
                    data.extend_from_slice(&ephemeral_private_nonce_index.to_le_bytes());
                    data.extend_from_slice(&sender_offset_key_index.to_le_bytes());
                    data.extend_from_slice(&commitment.to_vec());
                    data.extend_from_slice(&ephemeral_commitment.to_vec());
                    data.extend_from_slice(&metadata_signature_message.to_vec());

                    let command = ledger.build_command(Instruction::GetMetadataSignature, data);
                    let transport = get_transport()?;

                    match command.execute_with_transport(&transport) {
                        Ok(result) => {
                            if result.data().len() < 161 {
                                debug!(target: LOG_TARGET, "result less than 161");
                                return Err(LedgerDeviceError::Processing(format!(
                                    "'get_metadata_signature' insufficient data - expected 161 got {} bytes ({:?})",
                                    result.data().len(),
                                    result
                                ))
                                .into());
                            }
                            let data = result.data();
                            debug!(target: LOG_TARGET, "result length: {}, data: {:?}", result.data().len(), result.data());
                            Ok(ComAndPubSignature::new(
                                Commitment::from_canonical_bytes(&data[1..33])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PublicKey::from_canonical_bytes(&data[33..65])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[65..97])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[97..129])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                                PrivateKey::from_canonical_bytes(&data[129..161])
                                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?,
                            ))
                        },
                        Err(e) => Err(LedgerDeviceError::Instruction(format!("GetMetadataSignature: {}", e)).into()),
                    }
                }
            },
        }
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction kernel section (transactions > transaction_components > transaction_kernel)
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_txo_private_kernel_offset(
        &self,
        spend_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, TransactionError> {
        let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
            "kernel_excess_offset",
        );
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

    async fn get_view_key(&self) -> Result<PrivateKey, KeyManagerServiceError> {
        self.get_private_key(&TariKeyId::Managed {
            branch: TransactionKeyManagerBranch::DataEncryption.get_branch_key(),
            index: 0,
        })
        .await
    }

    pub async fn encrypt_data_for_recovery(
        &self,
        spend_key_id: &TariKeyId,
        custom_recovery_key_id: Option<&TariKeyId>,
        value: u64,
        payment_id: PaymentId,
    ) -> Result<EncryptedData, TransactionError> {
        let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            self.get_private_key(key_id).await?
        } else {
            self.get_view_key().await?
        };
        let value_key = value.into();
        let commitment = self.get_commitment(spend_key_id, &value_key).await?;
        let spend_key = self.get_private_key(spend_key_id).await?;
        let data = EncryptedData::encrypt_data(&recovery_key, &commitment, value.into(), &spend_key, payment_id)?;
        Ok(data)
    }

    pub async fn try_output_key_recovery(
        &self,
        output: &TransactionOutput,
        custom_recovery_key_id: Option<&TariKeyId>,
    ) -> Result<(TariKeyId, MicroMinotari, PaymentId), TransactionError> {
        let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            self.get_private_key(key_id).await?
        } else {
            self.get_view_key().await?
        };
        let (value, private_key, payment_id) =
            EncryptedData::decrypt_data(&recovery_key, output.commitment(), output.encrypted_data())?;
        self.crypto_factories
            .range_proof
            .verify_mask(output.commitment(), &private_key, value.into())?;
        let branch = TransactionKeyManagerBranch::CommitmentMask.get_branch_key();
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
        Ok((key, value, payment_id))
    }
}

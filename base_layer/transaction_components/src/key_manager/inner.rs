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
#[cfg(feature = "ledger")]
use minotari_ledger_wallet_comms::accessor_methods::{
    ledger_get_dh_shared_secret,
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
    key_branches::{TransactionKeyManagerBranch, PRE_MINE},
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
    wallet_types::WalletType,
};
use tari_crypto::{
    commitment::{ExtensionDegree, HomomorphicCommitmentFactory},
    extended_range_proof::ExtendedRangeProofService,
    hashing::{DomainSeparatedHash, DomainSeparatedHasher},
    keys::SecretKey,
    range_proof::RangeProofService as RPService,
    ristretto::{
        bulletproofs_plus::{RistrettoExtendedMask, RistrettoExtendedWitness},
        RistrettoComSig,
    },
};
use tari_hashing::{KeyManagerTransactionsHashDomain, WalletMessageSigningDomain};
use tari_script::{CheckSigSchnorrSignature, CompressedCheckSigSchnorrSignature, TariScript};
use tari_utilities::{ByteArray, Hidden};
use tokio::sync::RwLock;
use zeroize::Zeroize;

use crate::key_manager::{
    error::KeyManagerServiceError,
    interface::TariKeyAndId,
    tari_key_manager::TariKeyManager,
    AddResult,
    KeyDigest,
};

const HASHER_LABEL_STEALTH_KEY: &str = "script key";
use crate::key_manager::interface::KeyManagerState;
pub const LEDGER_NOT_SUPPORTED: &str = "Ledger is not supported in this build, please enable the \"ledger\" feature.";
use crate::{
    crypto_factories::CryptoFactories,
    key_manager::{
        interface::{TransactionKeyManagerBackend, TxoStage},
        ConfidentialOutputHasher,
        TariKeyId,
    },
    transaction_components::{
        one_sided::{
            diffie_hellman_stealth_domain_hasher,
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

pub struct TransactionKeyManagerInner<TBackend> {
    key_managers: HashMap<String, RwLock<TariKeyManager<KeyDigest>>>,
    db: Option<TBackend>,
    master_seed: CipherSeed,
    crypto_factories: CryptoFactories,
    wallet_type: Arc<WalletType>,
}

impl<TBackend> TransactionKeyManagerInner<TBackend>
where TBackend: TransactionKeyManagerBackend + 'static
{
    pub async fn new(
        master_seed: CipherSeed,
        db: Option<TBackend>,
        crypto_factories: CryptoFactories,
        wallet_type: Arc<WalletType>,
    ) -> Result<Self, KeyManagerServiceError> {
        let mut km = TransactionKeyManagerInner {
            key_managers: HashMap::new(),
            db,
            master_seed,
            crypto_factories,
            wallet_type,
        };
        km.add_standard_core_branches().await?;
        Ok(km)
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Key manager section
    // -----------------------------------------------------------------------------------------------------------------

    pub fn master_seed(&self) -> &CipherSeed {
        &self.master_seed
    }

    async fn add_standard_core_branches(&mut self) -> Result<(), KeyManagerServiceError> {
        for branch in TransactionKeyManagerBranch::iter() {
            self.add_key_manager_branch(&branch.get_branch_key()).await?;
        }
        Ok(())
    }

    pub async fn add_key_manager_branch(&mut self, branch: &str) -> Result<AddResult, KeyManagerServiceError> {
        let result = if self.key_managers.contains_key(branch) {
            AddResult::AlreadyExists
        } else {
            AddResult::NewEntry
        };
        let state = KeyManagerState {
            branch_seed: branch.to_string(),
            primary_key_index: 0,
        };
        self.key_managers.insert(
            branch.to_string(),
            RwLock::new(TariKeyManager::<KeyDigest>::from(
                self.master_seed.clone(),
                state.branch_seed,
                state.primary_key_index,
            )),
        );
        Ok(result)
    }

    pub async fn get_next_key(&self, branch: &str) -> Result<TariKeyAndId, KeyManagerServiceError> {
        let index = {
            match branch {
                PRE_MINE => {
                    let mut km = self
                        .key_managers
                        .get(branch)
                        .ok_or_else(|| self.unknown_key_branch_error("get_next_key", branch))?
                        .write()
                        .await;
                    km.increment_key_index(1)
                },
                _ => OsRng.next_u64(),
            }
        };
        let key_id = TariKeyId::Managed {
            branch: branch.to_string(),
            index,
        };
        let key = self.get_public_key_at_key_id(&key_id).await?;
        Ok(TariKeyAndId { key_id, pub_key: key })
    }

    pub async fn get_random_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        match &*self.wallet_type {
            WalletType::Ledger(ledger) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(KeyManagerServiceError::LedgerError(format!(
                        "{} 'get_random_key' was called. ({})",
                        LEDGER_NOT_SUPPORTED, ledger
                    )))
                }
                #[cfg(feature = "ledger")]
                {
                    let random_index = OsRng.next_u64();

                    let branch = TransactionKeyManagerBranch::RandomKey;
                    let public_key = CompressedPublicKey::new_from_pk(
                        ledger_get_public_key(ledger.account, random_index, branch)
                            .map_err(|e| KeyManagerServiceError::LedgerError(e.to_string()))?,
                    );
                    Ok(TariKeyAndId {
                        key_id: TariKeyId::Managed {
                            branch: TransactionKeyManagerBranch::RandomKey.get_branch_key(),
                            index: random_index,
                        },
                        pub_key: public_key,
                    })
                }
            },
            _ => {
                let random_private_key = PrivateKey::random(&mut OsRng);
                let key_id = self.import_key(random_private_key, None).await?;
                let public_key = self.get_public_key_at_key_id(&key_id).await?;
                Ok(TariKeyAndId {
                    key_id,
                    pub_key: public_key,
                })
            },
        }
    }

    pub async fn get_static_key(&self, branch: &str) -> Result<TariKeyId, KeyManagerServiceError> {
        match self.key_managers.get(branch) {
            None => Err(self.unknown_key_branch_error("get_static_key", branch)),
            Some(_) => Ok(TariKeyId::Managed {
                branch: branch.to_string(),
                index: 0,
            }),
        }
    }

    pub async fn get_public_key_at_key_id(
        &self,
        key_id: &TariKeyId,
    ) -> Result<CompressedPublicKey, KeyManagerServiceError> {
        match key_id {
            TariKeyId::Managed { .. } => self.get_managed_public_key_at_key_id(key_id).await,
            TariKeyId::Derived { key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                let public_alpha = self.get_spend_key().await?.pub_key;
                let branch_key = self.get_private_key(&key).await?;
                let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                    HASHER_LABEL_STEALTH_KEY,
                );
                let hasher = hasher.chain(branch_key.as_bytes()).finalize();
                let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                    KeyManagerServiceError::UnknownError(
                        "Invalid private key for sender offset private key".to_string(),
                    )
                })?;
                let public_key = CompressedPublicKey::from_secret_key(&private_key);
                let public_key = public_alpha.to_public_key()? + &public_key.to_public_key()?;
                Ok(CompressedPublicKey::new_from_pk(public_key))
            },
            TariKeyId::Imported { key } => Ok(key.clone()),
            TariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;

                let shared_secret = self.get_diffie_hellman_shared_secret(&key, public_key).await?;
                let commitment_mask_private_key = shared_secret_to_output_spending_key(&shared_secret)?;
                Ok(CompressedPublicKey::from_secret_key(&commitment_mask_private_key))
            },
            TariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;

                let shared_secret = self.get_diffie_hellman_shared_secret(&key, public_key).await?;
                let encryption_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
                Ok(CompressedPublicKey::from_secret_key(&encryption_private_key))
            },
            TariKeyId::Encrypted { encrypted, key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                let private_key = self.decrypt_encrypted_key(encrypted, key).await?;
                Ok(CompressedPublicKey::from_secret_key(&private_key))
            },
            TariKeyId::Zero => Ok(CompressedPublicKey::default()),
        }
    }

    async fn get_managed_public_key_at_key_id(
        &self,
        key_id: &TariKeyId,
    ) -> Result<CompressedPublicKey, KeyManagerServiceError> {
        if let TariKeyId::Managed { branch, index } = key_id {
            // handle special cases
            match &*self.wallet_type {
                WalletType::DerivedKeys => {},
                WalletType::Ledger(ledger) => {
                    match TransactionKeyManagerBranch::from_key(branch) {
                        TransactionKeyManagerBranch::OneSidedSenderOffset |
                        TransactionKeyManagerBranch::RandomKey |
                        TransactionKeyManagerBranch::PreMine => {
                            #[cfg(not(feature = "ledger"))]
                            {
                                return Err(KeyManagerServiceError::LedgerError(format!(
                                    "{} 'get_public_key_at_key_id' was called.",
                                    LEDGER_NOT_SUPPORTED
                                )));
                            }

                            #[cfg(feature = "ledger")]
                            {
                                let public_key = ledger_get_public_key(
                                    ledger.account,
                                    *index,
                                    TransactionKeyManagerBranch::from_key(branch),
                                )
                                .map_err(|e| KeyManagerServiceError::LedgerError(e.to_string()))?;
                                return Ok(CompressedPublicKey::new_from_pk(public_key));
                            }
                        },
                        TransactionKeyManagerBranch::Spend => {
                            return ledger.public_alpha.clone().ok_or(KeyManagerServiceError::LedgerError(
                                "Key manager set to use ledger, ledger alpha public key missing".to_string(),
                            ))
                        },
                        TransactionKeyManagerBranch::DataEncryption => {
                            let view_key = ledger
                                .view_key
                                .clone()
                                .ok_or(KeyManagerServiceError::LedgerViewKeyInaccessible(key_id.to_string()))?;
                            return Ok(CompressedPublicKey::from_secret_key(&view_key));
                        },
                        _ => {},
                    };
                },
                WalletType::ProvidedKeys(wallet) => match TransactionKeyManagerBranch::from_key(branch) {
                    TransactionKeyManagerBranch::Spend => {
                        return Ok(wallet.public_spend_key.clone());
                    },
                    TransactionKeyManagerBranch::DataEncryption => {
                        return Ok(CompressedPublicKey::from_secret_key(&wallet.view_key));
                    },
                    _ => {},
                },
            };
            let km = self
                .key_managers
                .get(branch)
                .ok_or_else(|| self.unknown_key_branch_error("get_public_key_at_key_id", branch))?
                .read()
                .await;
            Ok(km.derive_public_key(*index)?.key)
        } else {
            Err(KeyManagerServiceError::UnknownError(
                "should only handle managed key types".to_string(),
            ))
        }
    }

    fn unknown_key_branch_error(&self, caller: &str, branch: &str) -> KeyManagerServiceError {
        KeyManagerServiceError::UnknownKeyBranch(format!(
            "{}: branch: {}, wallet_type: {}",
            caller, branch, self.wallet_type
        ))
    }

    fn branch_not_supported_error(&self, caller: &str, branch: &str) -> KeyManagerServiceError {
        KeyManagerServiceError::BranchNotSupported(format!(
            "{}: branch: {}, wallet_type: {}",
            caller, branch, self.wallet_type
        ))
    }

    fn key_id_not_supported_error(&self, caller: &str, expected: &str, key_id: &TariKeyId) -> TransactionError {
        TransactionError::UnsupportedTariKeyId(format!(
            "{}: Expected '{}', got {}, wallet_type: {}",
            caller, expected, key_id, self.wallet_type
        ))
    }

    pub async fn add_offset_to_spend_key(
        &self,
        spend_key_id: &TariKeyId,
        sender_offset_pub_key: &CompressedPublicKey,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        let mut secret = self.get_private_key(spend_key_id).await?;
        // 1. Get the shared secret (Ad)
        let shared_secret = self
            .get_diffie_hellman_shared_secret(spend_key_id, sender_offset_pub_key)
            .await
            .map_err(|e| KeyManagerServiceError::UnknownError(e.to_string()))?;

        // 2. Hash the shared secret for stealth domain separation
        let stealth_hash = diffie_hellman_stealth_domain_hasher(shared_secret);

        // 3. Convert hash to a private key
        let shared_secret_private_key = PrivateKey::from_uniform_bytes(stealth_hash.as_ref())?;

        secret = secret + shared_secret_private_key;
        let shared_secret_key = self.import_key(secret, None).await?;

        Ok(shared_secret_key)
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) async fn get_private_key(&self, key_id: &TariKeyId) -> Result<PrivateKey, KeyManagerServiceError> {
        match key_id {
            TariKeyId::Zero => Ok(PrivateKey::default()),
            TariKeyId::Imported { key } => {
                if let Some(db) = &self.db {
                    let pvt_key = db.get_imported_key(key).await?;
                    return Ok(pvt_key);
                }
                Err(KeyManagerServiceError::NoStorage)
            },
            TariKeyId::Managed { branch, index } => {
                match &*self.wallet_type {
                    WalletType::DerivedKeys => {},
                    WalletType::Ledger(wallet) => {
                        if &TransactionKeyManagerBranch::DataEncryption.get_branch_key() == branch {
                            return wallet
                                .view_key
                                .clone()
                                .ok_or(KeyManagerServiceError::LedgerViewKeyInaccessible(key_id.to_string()));
                        }

                        // If we're trying to access any of the private keys, just say no bueno
                        if &TransactionKeyManagerBranch::Spend.get_branch_key() == branch ||
                            &TransactionKeyManagerBranch::OneSidedSenderOffset.get_branch_key() == branch ||
                            &TransactionKeyManagerBranch::PreMine.get_branch_key() == branch ||
                            &TransactionKeyManagerBranch::RandomKey.get_branch_key() == branch
                        {
                            return Err(KeyManagerServiceError::LedgerPrivateKeyInaccessible(key_id.to_string()));
                        }
                    },
                    WalletType::ProvidedKeys(wallet) => {
                        if &TransactionKeyManagerBranch::DataEncryption.get_branch_key() == branch {
                            return Ok(wallet.view_key.clone());
                        }

                        // If we're trying to access any of the private keys, just say no bueno
                        if &TransactionKeyManagerBranch::Spend.get_branch_key() == branch {
                            return wallet.private_spend_key.clone().ok_or(
                                KeyManagerServiceError::ImportedPrivateKeyInaccessible(key_id.to_string()),
                            );
                        }
                    },
                }

                let km = self
                    .key_managers
                    .get(branch)
                    .ok_or_else(|| self.unknown_key_branch_error("get_private_key", branch))?
                    .read()
                    .await;
                let key = km.get_private_key(*index)?;
                Ok(key)
            },
            TariKeyId::Derived { key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;

                let commitment_mask = Box::pin(self.get_private_key(&key)).await?;

                match &*self.wallet_type {
                    WalletType::Ledger(_) => {
                        Err(KeyManagerServiceError::LedgerPrivateKeyInaccessible(key_id.to_string()))
                    },
                    WalletType::DerivedKeys => {
                        let km = self
                            .key_managers
                            .get(&TransactionKeyManagerBranch::Spend.get_branch_key())
                            .ok_or_else(|| {
                                self.unknown_key_branch_error(
                                    "get_private_key",
                                    &TransactionKeyManagerBranch::Spend.get_branch_key(),
                                )
                            })?
                            .read()
                            .await;
                        let private_alpha = km.get_private_key(0)?;
                        let hasher =
                            DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                                HASHER_LABEL_STEALTH_KEY,
                            );
                        let hasher = hasher.chain(commitment_mask.as_bytes()).finalize();
                        let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                            KeyManagerServiceError::UnknownError("Invalid private key for Spend".to_string())
                        })?;
                        let private_key = private_key + private_alpha;
                        Ok(private_key)
                    },
                    WalletType::ProvidedKeys(wallet) => {
                        let private_alpha = wallet.private_spend_key.clone().ok_or(
                            KeyManagerServiceError::ImportedPrivateKeyInaccessible(key_id.to_string()),
                        )?;

                        let hasher =
                            DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                                HASHER_LABEL_STEALTH_KEY,
                            );
                        let hasher = hasher.chain(commitment_mask.as_bytes()).finalize();
                        let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                            KeyManagerServiceError::UnknownError("Invalid private key for Spend".to_string())
                        })?;
                        let private_key = private_key + private_alpha;
                        Ok(private_key)
                    },
                }
            },
            TariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                let shared_secret = Box::pin(self.get_diffie_hellman_shared_secret(&key, public_key)).await?;
                let commitment_mask_private_key = shared_secret_to_output_spending_key(&shared_secret)?;
                Ok(commitment_mask_private_key)
            },
            TariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                let shared_secret = Box::pin(self.get_diffie_hellman_shared_secret(&key, public_key)).await?;
                let commitment_mask_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
                Ok(commitment_mask_private_key)
            },
            TariKeyId::Encrypted { encrypted, key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                let private_key = Box::pin(self.decrypt_encrypted_key(encrypted, key)).await?;
                Ok(private_key)
            },
        }
    }

    async fn created_encrypted_key(
        &self,
        private_key: PrivateKey,
        encryption_key: TariKeyId,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        let pvt_bytes = private_key.to_vec();
        let private_encryption_key = self.get_private_key(&encryption_key).await?.to_vec();
        let domain = "KEY_MANAGER_private_key".as_bytes().to_vec();
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&private_encryption_key));
        let encrypted_vec = encrypt_bytes_integral_nonce(&cipher, domain, Hidden::hide(pvt_bytes))
            .map_err(|e| KeyManagerServiceError::EncryptionFailed(e.to_string()))?;
        let encrypted = TryFrom::try_from(encrypted_vec.as_slice())
            .map_err(|_| KeyManagerServiceError::EncryptionFailed("invalid encrypted bytes length".to_string()))?;
        Ok(TariKeyId::Encrypted {
            encrypted,
            key: encryption_key.into(),
        })
    }

    async fn decrypt_encrypted_key(
        &self,
        bytes: &[u8],
        decryption_key: TariKeyId,
    ) -> Result<PrivateKey, KeyManagerServiceError> {
        let private_decryption_key = self.get_private_key(&decryption_key).await?.to_vec();
        let domain = "KEY_MANAGER_private_key".as_bytes().to_vec();
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&private_decryption_key));
        let decrypted_bytes = decrypt_bytes_integral_nonce(&cipher, domain, bytes)
            .map_err(|e| KeyManagerServiceError::EncryptionFailed(e.to_string()))?;
        let pvt_key = PrivateKey::from_vec(&decrypted_bytes)?;
        Ok(pvt_key)
    }

    pub fn get_wallet_type(&self) -> Arc<WalletType> {
        self.wallet_type.clone()
    }

    pub async fn get_view_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        let key_id = TariKeyId::Managed {
            branch: TransactionKeyManagerBranch::DataEncryption.get_branch_key(),
            index: 0,
        };
        let key = CompressedPublicKey::from_secret_key(&self.get_private_view_key().await?);
        Ok(TariKeyAndId { key_id, pub_key: key })
    }

    pub async fn get_spend_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        let key_id = TariKeyId::Managed {
            branch: TransactionKeyManagerBranch::Spend.get_branch_key(),
            index: 0,
        };

        let key = match &*self.wallet_type {
            WalletType::DerivedKeys => {
                let private_key = self.get_private_key(&key_id).await?;
                CompressedPublicKey::from_secret_key(&private_key)
            },
            WalletType::Ledger(ledger) => ledger.public_alpha.clone().ok_or(KeyManagerServiceError::LedgerError(
                "Key manager set to use ledger, ledger alpha public key missing".to_string(),
            ))?,
            WalletType::ProvidedKeys(wallet) => wallet.public_spend_key.clone(),
        };
        Ok(TariKeyAndId { key_id, pub_key: key })
    }

    pub async fn get_comms_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        let key_id = TariKeyId::Managed {
            branch: TransactionKeyManagerBranch::Spend.get_branch_key(),
            index: 0,
        };
        let private_key = self.get_private_comms_key().await?;
        let key = CompressedPublicKey::from_secret_key(&private_key);
        Ok(TariKeyAndId { key_id, pub_key: key })
    }

    pub async fn get_next_commitment_mask_and_script_key(
        &self,
    ) -> Result<(TariKeyAndId, TariKeyAndId), KeyManagerServiceError> {
        let commitment_mask = self
            .get_next_key(&TransactionKeyManagerBranch::CommitmentMask.get_branch_key())
            .await?;
        let script_key_id = TariKeyId::Derived {
            key: (&commitment_mask.key_id).into(),
        };
        let script_public_key = self.get_public_key_at_key_id(&script_key_id).await?;
        Ok((commitment_mask, TariKeyAndId {
            key_id: script_key_id,
            pub_key: script_public_key,
        }))
    }

    pub async fn import_key(
        &self,
        private_key: PrivateKey,
        encryption_key: Option<TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        let encryption_key = match encryption_key {
            Some(key) => key,
            None => self.get_view_key().await?.key_id,
        };
        let key = self.created_encrypted_key(private_key, encryption_key).await?;
        Ok(key)
    }

    pub async fn create_encrypted_key_from_existing_key(
        &self,
        key_id: &TariKeyId,
        encryption_key: Option<TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        let private_key = self.get_private_key(key_id).await?;
        let encryption_key = match encryption_key {
            Some(key) => key,
            None => self.get_view_key().await?.key_id,
        };
        let key = self.created_encrypted_key(private_key, encryption_key).await?;
        Ok(key)
    }

    pub async fn get_private_view_key(&self) -> Result<PrivateKey, KeyManagerServiceError> {
        match &*self.wallet_type {
            WalletType::DerivedKeys => {
                self.get_private_key(&TariKeyId::Managed {
                    branch: TransactionKeyManagerBranch::DataEncryption.get_branch_key(),
                    index: 0,
                })
                .await
            },
            WalletType::Ledger(ledger) => ledger
                .view_key
                .clone()
                .ok_or(KeyManagerServiceError::LedgerViewKeyInaccessible(format!("{ledger}"))),
            WalletType::ProvidedKeys(wallet) => Ok(wallet.view_key.clone()),
        }
    }

    async fn get_private_comms_key(&self) -> Result<PrivateKey, KeyManagerServiceError> {
        let branch = TransactionKeyManagerBranch::Spend.get_branch_key();
        let index = 0;

        match &*self.wallet_type {
            WalletType::DerivedKeys => {
                self.get_private_key(&TariKeyId::Managed {
                    branch: branch.clone(),
                    index,
                })
                .await
            },
            WalletType::ProvidedKeys(wallet) => {
                if let Some(pvt_comms_key) = &wallet.private_comms_key {
                    Ok(pvt_comms_key.clone())
                } else {
                    let km = self
                        .key_managers
                        .get(&branch)
                        .ok_or_else(|| self.unknown_key_branch_error("get_private_comms_key", &branch))?
                        .read()
                        .await;
                    let key = km.get_private_key(index)?;
                    Ok(key)
                }
            },
            WalletType::Ledger(_) => {
                let km = self
                    .key_managers
                    .get(&branch)
                    .ok_or_else(|| self.unknown_key_branch_error("get_private_comms_key", &branch))?
                    .read()
                    .await;
                let key = km.get_private_key(index)?;
                Ok(key)
            },
        }
    }

    /// Calculates a script key id from the spend key id, if a public key is provided, it will only return a result of
    /// the public keys match
    pub async fn find_script_key_id_from_commitment_mask_key_id(
        &self,
        commitment_mask_key_id: &TariKeyId,
        public_script_key: Option<&CompressedPublicKey>,
    ) -> Result<Option<TariKeyId>, KeyManagerServiceError> {
        let script_key_id = TariKeyId::Derived {
            key: commitment_mask_key_id.into(),
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

    // -----------------------------------------------------------------------------------------------------------------
    // General crypto section
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_commitment(
        &self,
        private_key: &TariKeyId,
        value: &PrivateKey,
    ) -> Result<CompressedCommitment, KeyManagerServiceError> {
        let key = self.get_private_key(private_key).await?;
        Ok(CompressedCommitment::from_commitment(
            self.crypto_factories.commitment.commit(&key, value),
        ))
    }

    /// Verify that the commitment matches the value and the spending key/mask
    pub async fn verify_mask(
        &self,
        commitment: &CompressedCommitment,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerServiceError> {
        let commitment_mask_key = self.get_private_key(commitment_mask_key_id).await?;
        self.crypto_factories
            .range_proof
            .verify_mask(&commitment.to_commitment()?, &commitment_mask_key, value)
            .map_err(|e| e.into())
    }

    pub async fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<CommsDHKE, KeyManagerServiceError> {
        if let WalletType::Ledger(ledger) = &*self.wallet_type {
            if let TariKeyId::Managed { branch, index } = secret_key_id {
                match TransactionKeyManagerBranch::from_key(branch) {
                    TransactionKeyManagerBranch::OneSidedSenderOffset | TransactionKeyManagerBranch::RandomKey => {
                        #[cfg(not(feature = "ledger"))]
                        {
                            return Err(KeyManagerServiceError::LedgerError(format!(
                                "{} 'get_diffie_hellman_shared_secret' was called. ({} (has index {}))",
                                LEDGER_NOT_SUPPORTED, ledger, index
                            )));
                        }

                        #[cfg(feature = "ledger")]
                        {
                            return ledger_get_dh_shared_secret(
                                ledger.account,
                                *index,
                                TransactionKeyManagerBranch::from_key(branch),
                                public_key,
                            )
                            .map_err(|e| KeyManagerServiceError::LedgerError(e.to_string()));
                        }
                    },
                    _ => {},
                }
            }
        }

        let secret_key = self.get_private_key(secret_key_id).await?;
        let shared_secret = CommsDHKE::new(&secret_key, &public_key.to_public_key()?);
        Ok(shared_secret)
    }

    pub async fn get_diffie_hellman_stealth_domain_hasher(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<DomainSeparatedHash<Blake2b<U64>>, TransactionError> {
        match &*self.wallet_type {
            WalletType::Ledger(ledger) => match secret_key_id {
                TariKeyId::Managed { branch, index } => match TransactionKeyManagerBranch::from_key(branch) {
                    TransactionKeyManagerBranch::OneSidedSenderOffset => {
                        #[cfg(not(feature = "ledger"))]
                        {
                            Err(TransactionError::LedgerNotSupported(format!(
                                "{} 'get_diffie_hellman_stealth_domain_hasher' was called. ({} (has index {}))",
                                LEDGER_NOT_SUPPORTED, ledger, index
                            )))
                        }

                        #[cfg(feature = "ledger")]
                        {
                            ledger_get_dh_shared_secret(
                                ledger.account,
                                *index,
                                TransactionKeyManagerBranch::from_key(branch),
                                public_key,
                            )
                            .map_err(TransactionError::LedgerDeviceError)
                            .map(diffie_hellman_stealth_domain_hasher)
                        }
                    },
                    _ => Err(TransactionError::from(self.branch_not_supported_error(
                        "get_diffie_hellman_stealth_domain_hasher",
                        branch,
                    ))),
                },
                _ => Err(self.key_id_not_supported_error(
                    "get_diffie_hellman_stealth_domain_hasher",
                    "KeyId::Managed",
                    secret_key_id,
                )),
            },
            _ => {
                let secret_key = self.get_private_key(secret_key_id).await?;
                let dh = CommsDHKE::new(&secret_key, &public_key.to_public_key()?);
                Ok(diffie_hellman_stealth_domain_hasher(dh))
            },
        }
    }

    pub async fn generate_burn_proof(
        &self,
        commitment_mask_key_id: &TariKeyId,
        amount: &PrivateKey,
        claim_public_key: &CompressedPublicKey,
    ) -> Result<RistrettoComSig, TransactionError> {
        let nonce_a = PrivateKey::random(&mut OsRng);
        let nonce_x = PrivateKey::random(&mut OsRng);
        let pub_nonce = self.crypto_factories.commitment.commit(&nonce_x, &nonce_a);

        let commitment = self.get_commitment(commitment_mask_key_id, amount).await?;

        let challenge = ConfidentialOutputHasher::new("commitment_signature")
            .chain(&pub_nonce)
            .chain(&commitment)
            .chain(claim_public_key)
            .finalize();

        let commitment_mask = self.get_private_key(commitment_mask_key_id).await?;

        RistrettoComSig::sign(
            amount,
            &commitment_mask,
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
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let commitment = self.get_commitment(commitment_mask_key_id, value).await?;
        let commitment_private_key = self.get_private_key(commitment_mask_key_id).await?;

        match &*self.wallet_type {
            WalletType::Ledger(ledger) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported(format!(
                        "{} 'get_script_signature' was called. ({})",
                        LEDGER_NOT_SUPPORTED, ledger
                    )))
                }

                #[cfg(feature = "ledger")]
                {
                    let signature_key = match script_key_id {
                        TariKeyId::Managed { branch, index } => ScriptSignatureKey::Managed {
                            branch: TransactionKeyManagerBranch::from_key(branch),
                            index: *index,
                        },
                        TariKeyId::Derived { key: key_str } => {
                            let key = TariKeyId::from_str(key_str.to_string().as_str())
                                .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                            ScriptSignatureKey::Derived {
                                branch_key: self.get_private_key(&key).await?,
                            }
                        },
                        _ => {
                            return Err(self.key_id_not_supported_error(
                                "get_script_signature",
                                "KeyId::Managed or KeyId::Derived",
                                script_key_id,
                            ));
                        },
                    };

                    let signature = ledger_get_script_signature(
                        ledger.account,
                        ledger.network,
                        txi_version.as_u8(),
                        &signature_key,
                        value,
                        &commitment_private_key,
                        &commitment,
                        *script_message,
                    )
                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?;
                    Ok(signature)
                }
            },
            _ => {
                let r_a = PrivateKey::random(&mut OsRng);
                let r_x = PrivateKey::random(&mut OsRng);
                let r_y = PrivateKey::random(&mut OsRng);
                let ephemeral_commitment = self.crypto_factories.commitment.commit(&r_x, &r_a);
                let ephemeral_pubkey = CompressedPublicKey::from_secret_key(&r_y);
                let script_private_key = self.get_private_key(script_key_id).await?;

                let challenge = TransactionInput::finalize_script_signature_challenge(
                    txi_version,
                    &(CompressedCommitment::from_commitment(ephemeral_commitment)),
                    &ephemeral_pubkey,
                    &self.get_public_key_at_key_id(script_key_id).await?,
                    &commitment,
                    script_message,
                );

                let script_signature = UncompressedComAndPubSignature::sign(
                    value,
                    &commitment_private_key,
                    &script_private_key,
                    &r_a,
                    &r_x,
                    &r_y,
                    &challenge,
                    &*self.crypto_factories.commitment,
                )?;
                Ok(ComAndPubSignature::new_from_capk_signature(script_signature))
            },
        }
    }

    pub async fn get_partial_script_signature(
        &self,
        commitment_mask_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: &TransactionInputVersion,
        ephemeral_pubkey: &CompressedPublicKey,
        script_public_key: &CompressedPublicKey,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let private_commitment_mask = self.get_private_key(commitment_mask_id).await?;
        let commitment = self.get_commitment(commitment_mask_id, value).await?;
        let r_a = PrivateKey::random(&mut OsRng);
        let r_x = PrivateKey::random(&mut OsRng);
        let ephemeral_commitment = self.crypto_factories.commitment.commit(&r_x, &r_a);
        let challenge = TransactionInput::finalize_script_signature_challenge(
            txi_version,
            &CompressedCommitment::from_commitment(ephemeral_commitment),
            ephemeral_pubkey,
            script_public_key,
            &commitment,
            script_message,
        );

        let script_signature = UncompressedComAndPubSignature::sign(
            value,
            &private_commitment_mask,
            &PrivateKey::default(),
            &r_a,
            &r_x,
            &PrivateKey::default(),
            &challenge,
            &*self.crypto_factories.commitment,
        )?;
        Ok(ComAndPubSignature::new_from_capk_signature(script_signature))
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction output section (transactions > transaction_components > transaction_output)
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn construct_range_proof(
        &self,
        commitment_mask_key_id: &TariKeyId,
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

        let commitment_private_key = self.get_private_key(commitment_mask_key_id).await?;
        let proof_bytes_result = if min_value == 0 {
            self.crypto_factories
                .range_proof
                .construct_proof(&commitment_private_key, value)
        } else {
            let extended_mask =
                RistrettoExtendedMask::assign(ExtensionDegree::DefaultPedersen, vec![commitment_private_key])?;

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
            .map_err(|err| TransactionError::RangeProofError(format!("Failed to construct range proof: {err}")))?;

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
        match &*self.wallet_type {
            WalletType::DerivedKeys | WalletType::ProvidedKeys(_) => {
                let mut total_script_private_key = PrivateKey::default();
                for script_key_id in script_key_ids {
                    total_script_private_key = &total_script_private_key + self.get_private_key(script_key_id).await?
                }
                let mut total_sender_offset_private_key = PrivateKey::default();
                for sender_offset_key_id in sender_offset_key_ids {
                    total_sender_offset_private_key =
                        total_sender_offset_private_key + self.get_private_key(sender_offset_key_id).await?;
                }
                let script_offset = total_script_private_key - total_sender_offset_private_key;
                Ok(script_offset)
            },
            WalletType::Ledger(ledger) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported(format!(
                        "{} 'get_script_offset' was called. ({})",
                        LEDGER_NOT_SUPPORTED, ledger
                    )))
                }

                #[cfg(feature = "ledger")]
                {
                    let mut partial_script_offset = PrivateKey::default();
                    let mut derived_script_keys = vec![];
                    let mut script_key_indexes = vec![];
                    for script_key_id in script_key_ids {
                        match script_key_id {
                            TariKeyId::Managed { branch, index } => {
                                match TransactionKeyManagerBranch::from_key(branch) {
                                    TransactionKeyManagerBranch::Spend |
                                    TransactionKeyManagerBranch::PreMine |
                                    TransactionKeyManagerBranch::RandomKey |
                                    TransactionKeyManagerBranch::OneSidedSenderOffset => {
                                        script_key_indexes
                                            .push((TransactionKeyManagerBranch::from_key(branch), *index));
                                    },
                                    _ => {
                                        partial_script_offset =
                                            &partial_script_offset + self.get_private_key(script_key_id).await?
                                    },
                                }
                            },
                            TariKeyId::Derived { key } => {
                                let key_id = TariKeyId::from_str(key.to_string().as_str())
                                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                                // Note: If the derived key is a TariKeyId::Managed, but not allowed in
                                //       'self.get_private_key(...)' this will error.
                                let k = self.get_private_key(&key_id).await?;
                                derived_script_keys.push(k);
                            },
                            TariKeyId::Imported { .. } |
                            TariKeyId::Encrypted { .. } |
                            TariKeyId::DHCommitmentMask { .. } |
                            TariKeyId::DHEncryptedData { .. } => {
                                partial_script_offset =
                                    &partial_script_offset + self.get_private_key(script_key_id).await?
                            },
                            TariKeyId::Zero => {},
                        }
                    }

                    let mut derived_offset_keys = vec![];
                    let mut sender_offset_indexes = vec![];
                    for sender_offset_key_id in sender_offset_key_ids {
                        match sender_offset_key_id {
                            TariKeyId::Managed { branch, index } => {
                                match TransactionKeyManagerBranch::from_key(branch) {
                                    TransactionKeyManagerBranch::Spend |
                                    TransactionKeyManagerBranch::PreMine |
                                    TransactionKeyManagerBranch::RandomKey |
                                    TransactionKeyManagerBranch::OneSidedSenderOffset => {
                                        sender_offset_indexes
                                            .push((TransactionKeyManagerBranch::from_key(branch), *index));
                                    },
                                    _ => {
                                        partial_script_offset =
                                            partial_script_offset - self.get_private_key(sender_offset_key_id).await?;
                                    },
                                }
                            },
                            TariKeyId::Derived { key } => {
                                let key_id = TariKeyId::from_str(key.to_string().as_str())
                                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                                // Note: If the derived key is a TariKeyId::Managed, but not allowed in
                                //       'self.get_private_key(...)' this will error.
                                let k = self.get_private_key(&key_id).await?;
                                derived_offset_keys.push(k);
                            },
                            TariKeyId::Imported { .. } |
                            TariKeyId::Encrypted { .. } |
                            TariKeyId::DHCommitmentMask { .. } |
                            TariKeyId::DHEncryptedData { .. } => {
                                partial_script_offset =
                                    partial_script_offset - self.get_private_key(sender_offset_key_id).await?;
                            },
                            TariKeyId::Zero => {},
                        }
                    }

                    let script_offset = ledger_get_script_offset(
                        ledger.account,
                        &partial_script_offset,
                        &derived_script_keys,
                        &script_key_indexes,
                        &derived_offset_keys,
                        &sender_offset_indexes,
                    )
                    .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?;
                    Ok(script_offset)
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
    ) -> Result<CompressedCommitment, TransactionError> {
        let (nonce_a, nonce_b) = self
            .get_metadata_signature_ephemeral_private_key_pair(nonce_id, range_proof_type)
            .await?;
        Ok(CompressedCommitment::from_commitment(
            self.crypto_factories.commitment.commit(&nonce_b, &nonce_a),
        ))
    }

    pub async fn sign_script_message(
        &self,
        private_key_id: &TariKeyId,
        challenge: &[u8],
    ) -> Result<CompressedCheckSigSchnorrSignature, TransactionError> {
        match &*self.wallet_type {
            WalletType::Ledger(ledger) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported(format!(
                        "{} 'sign_script_message' was called. ({})",
                        LEDGER_NOT_SUPPORTED, ledger
                    )))
                }

                #[cfg(feature = "ledger")]
                {
                    match private_key_id {
                        TariKeyId::Managed { branch, index } => {
                            let signature = ledger_get_script_schnorr_signature(
                                ledger.account,
                                *index,
                                TransactionKeyManagerBranch::from_key(branch),
                                challenge,
                            )?;
                            Ok(signature)
                        },
                        _ => Err(self.key_id_not_supported_error(
                            "sign_script_message",
                            "KeyId::Managed",
                            private_key_id,
                        )),
                    }
                }
            },
            _ => {
                let private_key = self.get_private_key(private_key_id).await?;
                let signature = CheckSigSchnorrSignature::sign(&private_key, challenge, &mut OsRng)?;

                Ok(CompressedCheckSigSchnorrSignature::new_from_schnorr(signature))
            },
        }
    }

    pub async fn sign_script_message_with_spend_key(
        &self,
        message: &[u8],
        sender_offset_pub_key: Option<&CompressedPublicKey>,
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerServiceError> {
        let spend_key = self.get_spend_key().await?;

        if let Some(sender_offset_pub_key) = sender_offset_pub_key {
            return self
                .sign_script_message(
                    &self
                        .add_offset_to_spend_key(&spend_key.key_id, sender_offset_pub_key)
                        .await?,
                    message,
                )
                .await
                .map_err(|e| KeyManagerServiceError::UnknownError(e.to_string()));
        } else {
            return self
                .sign_script_message(&spend_key.key_id, message)
                .await
                .map_err(|e| KeyManagerServiceError::UnknownError(e.to_string()));
        }
    }

    pub async fn sign_message(
        &self,
        message: &[u8],
        sender_offset_pub_key: Option<&CompressedPublicKey>,
    ) -> Result<WalletMessageSchnorrSignature, KeyManagerServiceError> {
        if matches!(*self.wallet_type, WalletType::Ledger(_)) {
            return Err(KeyManagerServiceError::LedgerPrivateKeyInaccessible(
                "sign_message requires software keys".to_string(),
            ));
        }

        let spend_key = self.get_spend_key().await?;

        if let Some(sender_offset_pub_key) = sender_offset_pub_key {
            let spend_key_id = &self
                .add_offset_to_spend_key(&spend_key.key_id, sender_offset_pub_key)
                .await?;
            return SignatureWithDomain::<WalletMessageSigningDomain>::sign(
                &self.get_private_key(spend_key_id).await?,
                message,
                &mut OsRng,
            )
            .map_err(|e| KeyManagerServiceError::UnknownError(e.to_string()));
        } else {
            return SignatureWithDomain::<WalletMessageSigningDomain>::sign(
                &self.get_private_key(&spend_key.key_id).await?,
                message,
                &mut OsRng,
            )
            .map_err(|e| KeyManagerServiceError::UnknownError(e.to_string()));
        }
    }

    pub async fn sign_with_nonce_and_challenge(
        &self,
        private_key_id: &TariKeyId,
        nonce_key_id: &TariKeyId,
        challenge: &[u8; 64],
    ) -> Result<CompressedSignature, TransactionError> {
        match &*self.wallet_type {
            WalletType::Ledger(ledger) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported(format!(
                        "{} 'sign_with_nonce_and_challenge' was called. ({})",
                        LEDGER_NOT_SUPPORTED, ledger
                    )))
                }

                #[cfg(feature = "ledger")]
                {
                    match private_key_id {
                        TariKeyId::Managed {
                            branch: private_key_branch,
                            index: private_key_index,
                        } => match nonce_key_id {
                            TariKeyId::Managed {
                                branch: nonce_branch,
                                index: nonce_index,
                            } => {
                                if TransactionKeyManagerBranch::is_ledger_branch(private_key_branch) &&
                                    TransactionKeyManagerBranch::is_ledger_branch(nonce_branch)
                                {
                                    let signature = ledger_get_raw_schnorr_signature(
                                        ledger.account,
                                        *private_key_index,
                                        TransactionKeyManagerBranch::from_key(private_key_branch),
                                        *nonce_index,
                                        TransactionKeyManagerBranch::from_key(nonce_branch),
                                        challenge,
                                    )
                                    .map_err(|e| KeyManagerServiceError::LedgerError(e.to_string()))?;
                                    Ok(signature)
                                } else {
                                    let private_key = self.get_private_key(private_key_id).await?;
                                    let private_nonce = self.get_private_key(nonce_key_id).await?;
                                    let signature = UncompressedSignature::sign_raw_uniform(
                                        &private_key,
                                        private_nonce,
                                        challenge,
                                    )?;

                                    Ok(CompressedSignature::new_from_schnorr(signature))
                                }
                            },
                            _ => Err(self.key_id_not_supported_error(
                                "sign_with_nonce_and_challenge",
                                "KeyId::Managed",
                                nonce_key_id,
                            )),
                        },
                        _ => Err(self.key_id_not_supported_error(
                            "sign_with_nonce_and_challenge",
                            "KeyId::Managed",
                            private_key_id,
                        )),
                    }
                }
            },
            _ => {
                let private_key = self.get_private_key(private_key_id).await?;
                let private_nonce = self.get_private_key(nonce_key_id).await?;
                let signature = UncompressedSignature::sign_raw_uniform(&private_key, private_nonce, challenge)?;

                Ok(CompressedSignature::new_from_schnorr(signature))
            },
        }
    }

    pub async fn get_metadata_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &TariKeyId,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        let sender_offset_public_key = self.get_public_key_at_key_id(sender_offset_key_id).await?;
        // Use the pubkey, but generate the nonce on ledger
        let ephemeral_pubkey = self
            .get_next_key(&TransactionKeyManagerBranch::MetadataEphemeralNonce.get_branch_key())
            .await?;
        let receiver_partial_metadata_signature = self
            .get_receiver_partial_metadata_signature(
                commitment_mask_key_id,
                value_as_private_key,
                &sender_offset_public_key,
                &ephemeral_pubkey.pub_key,
                txo_version,
                metadata_signature_message,
                range_proof_type,
            )
            .await?;
        let commitment = self
            .get_commitment(commitment_mask_key_id, value_as_private_key)
            .await?;
        let ephemeral_commitment = receiver_partial_metadata_signature.ephemeral_commitment();
        let sender_partial_metadata_signature = self
            .get_sender_partial_metadata_signature(
                &ephemeral_pubkey.key_id,
                sender_offset_key_id,
                &commitment,
                ephemeral_commitment,
                txo_version,
                metadata_signature_message,
            )
            .await?;
        let metadata_signature = ComAndPubSignature::new_from_capk_signature(
            &receiver_partial_metadata_signature.to_capk_signature()? +
                &sender_partial_metadata_signature.to_capk_signature()?,
        );
        Ok(metadata_signature)
    }

    #[allow(unused_variables)]
    pub async fn get_one_sided_metadata_signature(
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
        match &*self.wallet_type {
            WalletType::DerivedKeys | WalletType::ProvidedKeys(_) => {
                let metadata_signature_message = TransactionOutput::metadata_signature_message_from_script_and_common(
                    script,
                    metadata_signature_message_common,
                );
                let value = value.into();
                self.get_metadata_signature(
                    commitment_mask_key_id,
                    &value,
                    sender_offset_key_id,
                    txo_version,
                    &metadata_signature_message,
                    range_proof_type,
                )
                .await
            },
            WalletType::Ledger(ledger) => {
                #[cfg(not(feature = "ledger"))]
                {
                    Err(TransactionError::LedgerNotSupported(format!(
                        "{} 'get_one_sided_metadata_signature' was called. ({})",
                        LEDGER_NOT_SUPPORTED, ledger
                    )))
                }

                #[cfg(feature = "ledger")]
                {
                    let sender_offset_key_index = sender_offset_key_id.managed_index().ok_or_else(|| {
                        TransactionError::KeyManagerError("Invalid index for sender offset".to_string())
                    })?;

                    let commitment_mask = self.get_private_key(commitment_mask_key_id).await?;

                    let comm_and_pub_sig = ledger_get_one_sided_metadata_signature(
                        ledger.account,
                        ledger.network,
                        txo_version.as_u8(),
                        value.into(),
                        sender_offset_key_index,
                        &commitment_mask,
                        receiver_address,
                        metadata_signature_message_common,
                    )
                    .map_err(TransactionError::LedgerDeviceError)?;

                    Ok(comm_and_pub_sig)
                }
            },
        }
    }

    pub async fn get_receiver_partial_metadata_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &CompressedPublicKey,
        ephemeral_pubkey: &CompressedPublicKey,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, TransactionError> {
        let ephemeral_commitment_nonce = self
            .get_next_key(&TransactionKeyManagerBranch::Nonce.get_branch_key())
            .await?;
        let (nonce_a, nonce_b) = self
            .get_metadata_signature_ephemeral_private_key_pair(&ephemeral_commitment_nonce.key_id, range_proof_type)
            .await?;
        let ephemeral_commitment = self.crypto_factories.commitment.commit(&nonce_b, &nonce_a);
        let commitment_private_key = self.get_private_key(commitment_mask_key_id).await?;
        let commitment = self.crypto_factories.commitment.commit(&commitment_private_key, value);
        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            txo_version,
            sender_offset_public_key,
            &(CompressedCommitment::from_commitment(ephemeral_commitment)),
            ephemeral_pubkey,
            &(CompressedCommitment::from_commitment(commitment)),
            metadata_signature_message,
        );

        let metadata_signature = UncompressedComAndPubSignature::sign(
            value,
            &commitment_private_key,
            &PrivateKey::default(),
            &nonce_a,
            &nonce_b,
            &PrivateKey::default(),
            &challenge,
            &*self.crypto_factories.commitment,
        )?;
        Ok(ComAndPubSignature::new_from_capk_signature(metadata_signature))
    }

    // In the case where the sender is an aggregated signer, we need to parse in the total public key shares, this is
    // done in: aggregated_sender_offset_public_keys and aggregated_ephemeral_public_keys. If there is no aggregated
    // signers, this can be left as none
    pub async fn get_sender_partial_metadata_signature(
        &self,
        ephemeral_private_nonce_id: &TariKeyId,
        sender_offset_key_id: &TariKeyId,
        commitment: &CompressedCommitment,
        ephemeral_commitment: &CompressedCommitment,
        txo_version: &TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, TransactionError> {
        let ephemeral_pubkey = self.get_public_key_at_key_id(ephemeral_private_nonce_id).await?;
        let sender_offset_public_key = self.get_public_key_at_key_id(sender_offset_key_id).await?;

        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            txo_version,
            &sender_offset_public_key,
            ephemeral_commitment,
            &ephemeral_pubkey,
            commitment,
            metadata_signature_message,
        );

        let sender_partial_metadata_signature_self = self
            .sign_with_nonce_and_challenge(sender_offset_key_id, ephemeral_private_nonce_id, &challenge)
            .await?;

        let metadata_signature = ComAndPubSignature::new(
            Default::default(),
            sender_partial_metadata_signature_self
                .get_compressed_public_nonce()
                .clone(),
            Default::default(),
            Default::default(),
            sender_partial_metadata_signature_self.get_signature().clone(),
        );

        Ok(metadata_signature)
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Transaction kernel section (transactions > transaction_components > transaction_kernel)
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn get_txo_private_kernel_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, TransactionError> {
        let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
            "kernel_excess_offset",
        );
        let commitment_private_key = self.get_private_key(commitment_mask_key_id).await?;
        let nonce_private_key = self.get_private_key(nonce_id).await?;
        let key_hash = hasher
            .chain(commitment_private_key.as_bytes())
            .chain(nonce_private_key.as_bytes())
            .finalize();
        PrivateKey::from_uniform_bytes(key_hash.as_ref()).map_err(|_| {
            TransactionError::KeyManagerError("Invalid private key for kernel signature nonce".to_string())
        })
    }

    pub async fn get_partial_txo_kernel_signature(
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
        let private_key = self.get_private_key(commitment_mask_key_id).await?;
        // We cannot use an offset with a coinbase tx as this will not allow us to check the coinbase commitment and
        // because the offset function does not know if its a coinbase or not, we need to know if we need to bypass it
        // or not
        let private_signing_key = if kernel_features.is_coinbase() {
            private_key
        } else {
            private_key -
                &self
                    .get_txo_private_kernel_offset(commitment_mask_key_id, nonce_id)
                    .await?
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

        let signature = UncompressedSignature::sign_raw_uniform(&final_signing_key, private_nonce, &challenge)?;
        Ok(CompressedSignature::new_from_schnorr(signature))
    }

    pub async fn get_txo_kernel_signature_excess_with_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<CompressedPublicKey, TransactionError> {
        let private_key = self.get_private_key(commitment_mask_key_id).await?;
        let offset = self
            .get_txo_private_kernel_offset(commitment_mask_key_id, nonce_id)
            .await?;
        let excess = private_key - &offset;
        Ok(CompressedPublicKey::from_secret_key(&excess))
    }

    // -----------------------------------------------------------------------------------------------------------------
    // Encrypted data section (transactions > transaction_components > encrypted_data)
    // -----------------------------------------------------------------------------------------------------------------

    pub async fn encrypt_data_for_recovery(
        &self,
        commitment_mask_key_id: &TariKeyId,
        custom_recovery_key_id: Option<&TariKeyId>,
        value: u64,
        payment_id: MemoField,
    ) -> Result<EncryptedData, TransactionError> {
        let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            self.get_private_key(key_id).await?
        } else {
            self.get_private_view_key().await?
        };
        let value_key = value.into();
        let commitment = self.get_commitment(commitment_mask_key_id, &value_key).await?;
        let commitment_private_key = self.get_private_key(commitment_mask_key_id).await?;
        let data = EncryptedData::encrypt_data(
            &recovery_key,
            &commitment,
            value.into(),
            &commitment_private_key,
            payment_id,
        )?;
        Ok(data)
    }

    pub async fn extract_payment_id_from_encrypted_data(
        &self,
        encrypted_data: &EncryptedData,
        commitment: &CompressedCommitment,
        custom_recovery_key_id: Option<&TariKeyId>,
    ) -> Result<MemoField, TransactionError> {
        let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            self.get_private_key(key_id).await?
        } else {
            self.get_private_view_key().await?
        };
        let (_, _, payment_id) = EncryptedData::decrypt_data(&recovery_key, commitment, encrypted_data)?;
        Ok(payment_id)
    }

    pub async fn try_output_key_recovery(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        sender_offset_public_key: &CompressedPublicKey,
    ) -> Result<Option<(TariKeyId, MicroMinotari, MemoField)>, TransactionError> {
        let (value, private_key, payment_id, key_id) =
            match EncryptedData::decrypt_data(&self.get_private_view_key().await?, commitment, encrypted_data) {
                Ok((value, private_key, payment_id)) => {
                    let key = self.import_key(private_key.clone(), None).await?;
                    (value, private_key, payment_id, key)
                },
                Err(_) => {
                    // so this is not change, lets try with the offset key
                    let view_key = self.get_view_key().await?.key_id;
                    let shared_secret = self
                        .get_diffie_hellman_shared_secret(&view_key, sender_offset_public_key)
                        .await?;

                    let encryption_key = shared_secret_to_output_encryption_key(&shared_secret)?;
                    match EncryptedData::decrypt_data(&encryption_key, commitment, encrypted_data) {
                        Ok((value, private_key, payment_id)) => {
                            let key = TariKeyId::DHCommitmentMask {
                                public_key: sender_offset_public_key.clone(),
                                private_key: view_key.into(),
                            };
                            if self.get_private_key(&key).await? == private_key {
                                (value, private_key, payment_id, key)
                            } else {
                                let key = self.import_key(private_key.clone(), None).await?;
                                (value, private_key, payment_id, key)
                            }
                        },
                        Err(_) => return Ok(None),
                    }
                },
            };
        self.crypto_factories
            .range_proof
            .verify_mask(&commitment.to_commitment()?, &private_key, value.into())?;

        Ok(Some((key_id, value, payment_id)))
    }

    pub async fn is_this_output_ours(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        custom_recovery_key_id: Option<PrivateKey>,
    ) -> Result<bool, TransactionError> {
        let recovery_key = if let Some(key) = custom_recovery_key_id {
            key
        } else {
            self.get_private_view_key().await?
        };
        let (value, private_key, _payment_id) =
            match EncryptedData::decrypt_data(&recovery_key, commitment, encrypted_data) {
                Ok(res) => res,
                Err(_) => return Ok(false),
            };
        self.crypto_factories
            .range_proof
            .verify_mask(&commitment.to_commitment()?, &private_key, value.into())?;
        Ok(true)
    }

    pub async fn stealth_address_script_spending_key(
        &self,
        commitment_mask_key_id: &TariKeyId,
        spend_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, TransactionError> {
        let private_key = self.get_private_key(commitment_mask_key_id).await?;
        let hasher =
            DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label("script key");
        let hasher = hasher.chain(private_key.as_bytes()).finalize();
        let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref())
            .map_err(|_| KeyManagerServiceError::UnknownError("Invalid commitment mask private key".to_string()))?;
        let public_key = CompressedPublicKey::from_secret_key(&private_key);
        let public_key = spend_key.to_public_key()? + &public_key.to_public_key()?;
        Ok(CompressedPublicKey::new_from_pk(public_key))
    }

    async fn prepare_cipher(
        &self,
        encryption_key_id: Option<&TariKeyId>,
    ) -> Result<XChaCha20Poly1305, KeyManagerServiceError> {
        let encryption_key = match encryption_key_id {
            Some(key_id) => self.get_private_key(key_id).await?,
            None => self.get_private_view_key().await?,
        };
        let hasher =
            DomainSeparatedHasher::<Blake2b<U32>, KeyManagerTransactionsHashDomain>::new_with_label("key_encryption");
        let hash = hasher.chain(encryption_key.as_bytes()).finalize();
        let key_ga = Key::from_slice(hash.as_ref());
        Ok(XChaCha20Poly1305::new(key_ga))
    }

    async fn encrypt_key(
        &self,
        key: PrivateKey,
        encryption_key_id: Option<&TariKeyId>,
    ) -> Result<Vec<u8>, KeyManagerServiceError> {
        let cipher = self.prepare_cipher(encryption_key_id).await?;
        let domain = b"key".to_vec();

        let encrypted = encrypt_bytes_integral_nonce(&cipher, domain.clone(), Hidden::hide(key.to_vec()))
            .map_err(KeyManagerServiceError::EncryptionFailed)?;

        Ok(encrypted)
    }

    pub async fn encrypted_key(
        &self,
        key_id: &TariKeyId,
        encryption_key_id: Option<&TariKeyId>,
    ) -> Result<Vec<u8>, KeyManagerServiceError> {
        let key = self.get_private_key(key_id).await?;
        self.encrypt_key(key, encryption_key_id).await
    }

    pub async fn import_encrypted_key(
        &self,
        encrypted: Vec<u8>,
        encryption_key_id: Option<&TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerServiceError> {
        let cipher = self.prepare_cipher(encryption_key_id).await?;
        let domain = b"key".to_vec();

        let mut decrypted_data = decrypt_bytes_integral_nonce(&cipher, domain.clone(), &encrypted)
            .map_err(KeyManagerServiceError::DecryptionFailed)?;

        let key = PrivateKey::from_vec(&decrypted_data)
            .map_err(|err| KeyManagerServiceError::DecryptionFailed(err.to_string()))?;

        decrypted_data.zeroize();
        let imported_key = self.import_key(key, None).await?;
        Ok(imported_key)
    }
}

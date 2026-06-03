//  Copyright 2025, The Tari Project
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

use std::{ops::Shl, str::FromStr};

use blake2::Blake2b;
use chacha20poly1305::{Key, XChaCha20Poly1305};
use digest::{KeyInit, consts::U64};
use log::trace;
use minotari_ledger_wallet_common::common_types::LedgerKeyBranch;
#[cfg(feature = "ledger")]
use minotari_ledger_wallet_comms::accessor_methods::{
    ScriptSignatureKey,
    ledger_get_dh_shared_secret,
    ledger_get_one_sided_metadata_signature,
    ledger_get_public_key,
    ledger_get_raw_schnorr_signature,
    ledger_get_script_offset,
    ledger_get_script_schnorr_signature,
    ledger_get_script_signature,
};
use rand::Rng;
use tari_common_types::{
    encryption::{decrypt_bytes_integral_nonce, encrypt_bytes_integral_nonce},
    tari_address::TariAddress,
    types::{
        ComAndPubSignature,
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
    hashing::DomainSeparatedHasher,
    keys::SecretKey,
    range_proof::RangeProofService,
    ristretto::bulletproofs_plus::{RistrettoExtendedMask, RistrettoExtendedWitness},
};
use tari_hashing::{KeyManagerTransactionsHashDomain, WalletMessageSigningDomain};
use tari_script::{CheckSigSchnorrSignature, CompressedCheckSigSchnorrSignature, TariScript};
use tari_utilities::{ByteArray, Hidden, hex::Hex};
use zeroize::Zeroize;

use crate::{
    MicroMinotari,
    crypto_factories::CryptoFactories,
    key_manager::{
        ConfidentialOutputHasher,
        SecretTransactionKeyManagerInterface,
        TxoStage,
        error::KeyManagerError,
        interface::TransactionKeyManagerInterface,
        key_id::{TariKeyAndId, TariKeyId},
        wallet_types::WalletType,
    },
    transaction_components::{
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
        one_sided::{
            diffie_hellman_stealth_domain_hasher,
            public_key_to_output_encryption_key,
            public_key_to_output_spending_key,
        },
    },
};
const HASHER_LABEL_STEALTH_KEY: &str = "script key";
const CODE_TEMPLATE_AUTHOR_LABEL: &str = "code-template-author";
const HASHER_LABEL_BURN_SENDER_OFFSET: &str = "burn-sender-offset";

#[derive(Clone)]
pub struct KeyManager {
    crypto_factories: CryptoFactories,
    wallet_type: WalletType,
}

impl KeyManager {
    pub fn new_with_crypto_factories(
        crypto_factories: CryptoFactories,
        wallet_type: WalletType,
    ) -> Result<Self, KeyManagerError> {
        #[cfg(not(feature = "ledger"))]
        if wallet_type.is_ledger() {
            return Err(KeyManagerError::InvalidWalletType(
                "Trying to use the key manager without ledger features compiled in".to_string(),
            ));
        }
        Ok(Self {
            crypto_factories,
            wallet_type,
        })
    }

    pub fn new(wallet_type: WalletType) -> Result<Self, KeyManagerError> {
        #[cfg(not(feature = "ledger"))]
        if wallet_type.is_ledger() {
            return Err(KeyManagerError::InvalidWalletType(
                "Trying to use the key manager without ledger features compiled in".to_string(),
            ));
        }
        Ok(Self {
            crypto_factories: CryptoFactories::default(),
            wallet_type,
        })
    }

    pub fn new_random() -> Result<Self, KeyManagerError> {
        Ok(Self {
            crypto_factories: CryptoFactories::default(),
            wallet_type: WalletType::new_random()?,
        })
    }

    fn created_encrypted_key(
        &self,
        private_key: PrivateKey,
        encryption_key: TariKeyId,
    ) -> Result<TariKeyId, KeyManagerError> {
        let private_encryption_key = self.get_private_key(&encryption_key)?.to_vec();
        let domain = "KEY_MANAGER_private_key".as_bytes().to_vec();
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&private_encryption_key));
        let encrypted_vec = encrypt_bytes_integral_nonce(&cipher, domain, Hidden::hide(private_key.to_vec()))
            .map_err(|e| KeyManagerError::EncryptionFailed(e.to_string()))?;
        let encrypted = encrypted_vec.as_slice().to_vec();
        Ok(TariKeyId::Encrypted {
            encrypted,
            key: encryption_key.into(),
        })
    }

    fn add_offset_to_key(
        &self,
        private_key_id: &TariKeyId,
        sender_offset_pub_key: &CompressedPublicKey,
    ) -> Result<TariKeyId, KeyManagerError> {
        let mut secret = self.get_private_key(private_key_id)?;
        // 1. Get the shared secret (Ad)
        let shared_secret = self.get_diffie_hellman_shared_secret(private_key_id, sender_offset_pub_key)?;

        // 2. Hash the shared secret for stealth domain separation
        let stealth_hash = diffie_hellman_stealth_domain_hasher(&shared_secret);

        // 3. Convert hash to a private key
        let shared_secret_private_key = PrivateKey::from_uniform_bytes(stealth_hash.as_ref())?;

        secret = secret + shared_secret_private_key;
        let shared_secret_key = self.create_encrypted_key(secret, None)?;

        Ok(shared_secret_key)
    }

    fn get_private_spend_key(&self) -> Result<PrivateKey, KeyManagerError> {
        self.wallet_type
            .get_private_spend_key()
            .ok_or(KeyManagerError::InvalidWalletType(format!(
                "Trying to access private spend key on wallet that does not have access to it {}",
                self.wallet_type
            )))
    }

    fn ledger_get_script_signature_wrapper(
        &self,
        txi_version: TransactionInputVersion,
        script_key_id: &TariKeyId,
        value: &PrivateKey,
        commitment_mask_key_id: &TariKeyId,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let commitment = self.get_commitment(commitment_mask_key_id, value)?;
            let commitment_private_key = self.get_private_key(commitment_mask_key_id)?;
            let signature_key = match script_key_id {
                TariKeyId::LedgerKey { branch, index } => ScriptSignatureKey::Managed {
                    branch: *branch,
                    index: *index,
                },
                TariKeyId::Derived { key: key_str } => {
                    let key = TariKeyId::from_str(key_str.to_string().as_str())
                        .map_err(|_| KeyManagerError::InvalidKeyId(script_key_id.to_string()))?;
                    ScriptSignatureKey::Derived {
                        branch_key: self.get_private_key(&key)?,
                    }
                },
                _ => {
                    return Err(KeyManagerError::LedgerError(format!(
                        "Ledger does not support the following key id {script_key_id}"
                    )));
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
            .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(signature);
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger signature with tx_version: {:?}, script key:{}, value: {:?}, commitment_mask: {}, script_message: {:?}",
            txi_version,
            script_key_id,
            value.to_vec(),
            commitment_mask_key_id,
            script_message);

        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    fn ledger_get_script_schnorr_signature_wrapper(
        &self,
        index: u64,
        branch: LedgerKeyBranch,
        challenge: &[u8],
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let signature = ledger_get_script_schnorr_signature(ledger.account, index, branch, challenge)
                .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(signature);
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger schnorr signature with index: {}, ledger branch key:{}, challenge: {:?}",
            index,
            branch,
            challenge);
        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    fn ledger_get_script_offset_wrapper(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
    ) -> Result<PrivateKey, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let mut partial_script_offset = PrivateKey::default();
            let mut derived_script_keys = vec![];
            let mut script_key_indexes = vec![];
            for script_key_id in script_key_ids {
                match script_key_id {
                    TariKeyId::LedgerKey { branch, index } => {
                        script_key_indexes.push((*branch, *index));
                    },
                    TariKeyId::Derived { key } => {
                        let key_id = TariKeyId::from_str(key.to_string().as_str())
                            .map_err(|_| KeyManagerError::InvalidKeyId(key.to_string()))?;
                        // Note: If the derived key is a TariKeyId::Managed, but not allowed in
                        //       'self.get_private_key(...)' this will error.
                        let k = self.get_private_key(&key_id)?;
                        derived_script_keys.push(k);
                    },
                    TariKeyId::Zero => {},
                    _ => partial_script_offset = &partial_script_offset + self.get_private_key(script_key_id)?,
                }
            }

            let mut derived_offset_keys = vec![];
            let mut sender_offset_indexes = vec![];
            for sender_offset_key_id in sender_offset_key_ids {
                match sender_offset_key_id {
                    TariKeyId::LedgerKey { branch, index } => {
                        sender_offset_indexes.push((*branch, *index));
                    },
                    TariKeyId::Derived { key } => {
                        let key_id = TariKeyId::from_str(key.to_string().as_str())
                            .map_err(|_| KeyManagerError::InvalidKeyId(key.to_string()))?;
                        // Note: If the derived key is a TariKeyId::Managed, but not allowed in
                        //       'self.get_private_key(...)' this will error.
                        let k = self.get_private_key(&key_id)?;
                        derived_offset_keys.push(k);
                    },
                    TariKeyId::Zero => {},
                    _ => {
                        partial_script_offset = partial_script_offset - self.get_private_key(sender_offset_key_id)?;
                    },
                }
            }
            let signature = ledger_get_script_offset(
                ledger.account,
                &partial_script_offset,
                &derived_script_keys,
                &script_key_indexes,
                &derived_offset_keys,
                &sender_offset_indexes,
            )
            .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(signature);
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger script offset with script_key_ids: {:?}, sender_offset_key_ids:{:?}",
            script_key_ids,
            sender_offset_key_ids);
        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    fn ledger_get_raw_schnorr_signature_wrapper(
        &self,
        private_key_index: u64,
        private_key: LedgerKeyBranch,
        nonce_index: u64,
        nonce: LedgerKeyBranch,
        challenge: &[u8; 64],
    ) -> Result<CompressedSignature, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let signature = ledger_get_raw_schnorr_signature(
                ledger.account,
                private_key_index,
                private_key,
                nonce_index,
                nonce,
                challenge,
            )
            .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(signature);
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger raw schnorr signature with private_key_index: {:?}, private_key:{}, nonce_index: {}, nonce: {}, challenge: {:?}",
            private_key_index,
            private_key,
            nonce_index,
            nonce,
            challenge);

        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    fn ledger_get_public_key_wrapper(
        &self,
        branch: LedgerKeyBranch,
        index: u64,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let key = ledger_get_public_key(ledger.account, index, branch)
                .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(CompressedPublicKey::new_from_pk(key));
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger public key with branch: {:?}, index :{}",
            branch,
            index);
        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    fn ledger_get_dh_shared_secret_wrapper(
        &self,
        branch: LedgerKeyBranch,
        index: u64,
        public_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let key = ledger_get_dh_shared_secret(ledger.account, index, branch, public_key)
                .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(key);
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger DH shared secret key with branch: {:?}, index :{}, public key: {}",
            branch, index, public_key.to_hex());
        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    fn ledger_get_one_sided_metadata_signature_wrapper(
        &self,
        txo_version: TransactionOutputVersion,
        value: MicroMinotari,
        sender_offset_key_id: &TariKeyId,
        commitment_mask_key_id: &TariKeyId,
        receiver_address: &TariAddress,
        metadata_signature_message_common: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let sender_offset_key_index = match sender_offset_key_id {
                TariKeyId::LedgerKey { branch: _, index } => index,
                _ => {
                    return Err(KeyManagerError::LedgerError(
                        "Non ledger key for sender offset in ledger wallet".to_string(),
                    ));
                },
            };
            let commitment_mask = self.get_private_key(commitment_mask_key_id)?;
            let sig = ledger_get_one_sided_metadata_signature(
                ledger.account,
                ledger.network,
                txo_version.as_u8(),
                value.into(),
                *sender_offset_key_index,
                &commitment_mask,
                receiver_address,
                metadata_signature_message_common,
            )
            .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(sig);
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger metadata signature with txo_version: {:?}, value:{}, sender_offset_key_id: {}, commitment_mask_key_id: {}, receiver_address: {}, message: {:?}",
            txo_version,
            value,
            sender_offset_key_id,
            commitment_mask_key_id,
            receiver_address,
            metadata_signature_message_common);

        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    fn decrypt_encrypted_key(&self, bytes: &[u8], decryption_key: TariKeyId) -> Result<PrivateKey, KeyManagerError> {
        let mut private_decryption_key = self.get_private_key(&decryption_key)?.to_vec();
        let domain = "KEY_MANAGER_private_key".as_bytes().to_vec();
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&private_decryption_key));
        private_decryption_key.zeroize();
        let decrypted_bytes = decrypt_bytes_integral_nonce(&cipher, domain, bytes)
            .map_err(|e| KeyManagerError::EncryptionFailed(e.to_string()))?;
        let pvt_key = PrivateKey::from_vec(&decrypted_bytes)?;
        Ok(pvt_key)
    }

    fn get_metadata_signature_ephemeral_private_key_pair(
        &self,
        nonce_id: &TariKeyId,
        range_proof_type: RangeProofType,
    ) -> Result<(PrivateKey, PrivateKey), KeyManagerError> {
        let nonce_private_key = self.get_private_key(nonce_id)?;
        // With BulletProofPlus type range proofs, the nonce is a secure random value
        // With RevealedValue type range proofs, the nonce is always 0 and the minimum value promise equal to the value
        let nonce_a = match range_proof_type {
            RangeProofType::BulletProofPlus => {
                let hasher_a = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                    "metadata_signature_ephemeral_nonce_a",
                );
                let a_hash = hasher_a.chain(nonce_private_key.as_bytes()).finalize();
                PrivateKey::from_uniform_bytes(a_hash.as_ref())
            },
            RangeProofType::RevealedValue => Ok(PrivateKey::default()),
        }?;

        let hasher_b = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
            "metadata_signature_ephemeral_nonce_b",
        );
        let b_hash = hasher_b.chain(nonce_private_key.as_bytes()).finalize();
        let nonce_b = PrivateKey::from_uniform_bytes(b_hash.as_ref())?;
        Ok((nonce_a, nonce_b))
    }

    pub fn get_wallet_type(&self) -> &WalletType {
        &self.wallet_type
    }
}

impl TransactionKeyManagerInterface for KeyManager {
    fn get_random_key(
        &self,
        encryption_key: Option<TariKeyId>,
        ledger_key: Option<LedgerKeyBranch>,
    ) -> Result<TariKeyAndId, KeyManagerError> {
        if let Some(branch) = ledger_key &&
            self.wallet_type.is_ledger()
        {
            let random_index = rand::rng().next_u64();
            let public_key = self.ledger_get_public_key_wrapper(branch, random_index)?;
            return Ok(TariKeyAndId {
                key_id: TariKeyId::LedgerKey {
                    branch,
                    index: random_index,
                },
                pub_key: public_key,
            });
        }

        let random_private_key = PrivateKey::random(&mut rand::rng());
        let key_id = self.create_encrypted_key(random_private_key, encryption_key)?;
        let public_key = self.get_public_key_at_key_id(&key_id)?;
        Ok(TariKeyAndId {
            key_id,
            pub_key: public_key,
        })
    }

    fn get_public_key_at_key_id(&self, key_id: &TariKeyId) -> Result<CompressedPublicKey, KeyManagerError> {
        match key_id {
            TariKeyId::Derived { key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerError::InvalidKeyId(key_id.to_string()))?;
                let public_alpha = self.get_spend_key().pub_key;
                let branch_key = self.get_private_key(&key)?;
                let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                    HASHER_LABEL_STEALTH_KEY,
                );
                let hasher = hasher.chain(branch_key.as_bytes()).finalize();
                let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                    KeyManagerError::UnexpectedError("Invalid private key for sender offset private key".to_string())
                })?;
                let public_key = CompressedPublicKey::from_secret_key(&private_key);
                let public_key = public_alpha.to_public_key()? + &public_key.to_public_key()?;
                Ok(CompressedPublicKey::new_from_pk(public_key))
            },
            TariKeyId::CodeTemplateAuthor => {
                let public_spend_key = self.get_spend_key().pub_key;
                let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                    CODE_TEMPLATE_AUTHOR_LABEL,
                );
                let hasher = hasher.chain(public_spend_key.as_bytes()).finalize();
                let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                    KeyManagerError::UnexpectedError("Invalid private key for sender offset private key".to_string())
                })?;
                let public_key = CompressedPublicKey::from_secret_key(&private_key);
                let public_key = public_key.to_public_key()? + &public_spend_key.to_public_key()?;
                Ok(CompressedPublicKey::new_from_pk(public_key))
            },
            TariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerError::InvalidKeyId(key_id.to_string()))?;

                let shared_secret = self.get_diffie_hellman_shared_secret(&key, public_key)?;
                let commitment_mask_private_key = public_key_to_output_spending_key(&shared_secret)?;
                Ok(CompressedPublicKey::from_secret_key(&commitment_mask_private_key))
            },
            TariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerError::InvalidKeyId(key_id.to_string()))?;

                let shared_secret = self.get_diffie_hellman_shared_secret(&key, public_key)?;
                let encryption_private_key = public_key_to_output_encryption_key(&shared_secret)?;
                Ok(CompressedPublicKey::from_secret_key(&encryption_private_key))
            },
            TariKeyId::Encrypted { encrypted, key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerError::InvalidKeyId(key_id.to_string()))?;
                let private_key = self.decrypt_encrypted_key(encrypted, key)?;
                Ok(CompressedPublicKey::from_secret_key(&private_key))
            },
            TariKeyId::Zero => Ok(CompressedPublicKey::default()),
            TariKeyId::LedgerKey { branch, index } => {
                if !self.wallet_type.is_ledger() {
                    return Err(KeyManagerError::InvalidWalletType(
                        "Trying to access Ledger key on non-Ledger wallet".to_string(),
                    ));
                }
                match branch {
                    LedgerKeyBranch::Spend => Ok(self.wallet_type.get_public_spend_key()),
                    _ => self.ledger_get_public_key_wrapper(*branch, *index),
                }
            },
            TariKeyId::SpendKey => Ok(self.wallet_type.get_public_spend_key()),
            TariKeyId::ViewKey => Ok(self.wallet_type.get_public_view_key()),
        }
    }

    fn create_encrypted_key(
        &self,
        private_key: PrivateKey,
        encryption_key: Option<TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerError> {
        let encryption_key = match encryption_key {
            Some(key) => key,
            None => self.get_view_key().key_id,
        };
        let key = self.created_encrypted_key(private_key, encryption_key)?;
        Ok(key)
    }

    fn get_commitment(
        &self,
        private_key: &TariKeyId,
        value: &PrivateKey,
    ) -> Result<CompressedCommitment, KeyManagerError> {
        let key = self.get_private_key(private_key)?;
        Ok(CompressedCommitment::from_commitment(
            self.crypto_factories.commitment.commit(&key, value),
        ))
    }

    fn verify_mask(
        &self,
        commitment: &CompressedCommitment,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
    ) -> Result<bool, KeyManagerError> {
        let commitment_mask_key = self.get_private_key(commitment_mask_key_id)?;
        self.crypto_factories
            .range_proof
            .verify_mask(&commitment.to_commitment()?, &commitment_mask_key, value)
            .map_err(|e| e.into())
    }

    fn get_view_key(&self) -> TariKeyAndId {
        let key_id = TariKeyId::ViewKey;
        let key = self.wallet_type.get_public_view_key();
        TariKeyAndId { key_id, pub_key: key }
    }

    fn get_private_view_key(&self) -> PrivateKey {
        self.wallet_type.get_view_key().clone()
    }

    fn get_spend_key(&self) -> TariKeyAndId {
        let public_key = self.wallet_type.get_public_spend_key();
        let key_id = TariKeyId::SpendKey;
        TariKeyAndId {
            key_id,
            pub_key: public_key,
        }
    }

    fn get_next_commitment_mask_and_script_key(&self) -> Result<(TariKeyAndId, TariKeyAndId), KeyManagerError> {
        let commitment_mask = self.get_random_key(None, None)?;
        let script_key_id = TariKeyId::Derived {
            key: (&commitment_mask.key_id).into(),
        };
        let script_public_key = self.get_public_key_at_key_id(&script_key_id)?;
        Ok((commitment_mask, TariKeyAndId {
            key_id: script_key_id,
            pub_key: script_public_key,
        }))
    }

    fn find_script_key_id_from_commitment_mask_key_id(
        &self,
        commitment_mask_key_id: &TariKeyId,
        public_script_key: Option<&CompressedPublicKey>,
    ) -> Result<Option<TariKeyId>, KeyManagerError> {
        let script_key_id = TariKeyId::Derived {
            key: commitment_mask_key_id.into(),
        };

        if let Some(key) = public_script_key {
            let script_public_key = self.get_public_key_at_key_id(&script_key_id)?;
            if *key == script_public_key {
                return Ok(Some(script_key_id));
            }
            return Ok(None);
        }
        Ok(Some(script_key_id))
    }

    fn get_diffie_hellman_shared_secret(
        &self,
        secret_key_id: &TariKeyId,
        public_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        if let TariKeyId::LedgerKey { branch, index } = secret_key_id {
            return self.ledger_get_dh_shared_secret_wrapper(*branch, *index, public_key);
        }

        let secret_key = self.get_private_key(secret_key_id)?;
        let pk = (&secret_key) * (&(public_key.to_public_key()?));
        let shared_secret = CompressedPublicKey::new_from_pk(pk);
        Ok(shared_secret)
    }

    fn construct_range_proof(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: u64,
        min_value: u64,
    ) -> Result<RangeProof, KeyManagerError> {
        if self.crypto_factories.range_proof.range() < 64 &&
            value >= 1u64.shl(&self.crypto_factories.range_proof.range())
        {
            return Err(KeyManagerError::TransactionError(TransactionError::BuilderError(
                "Value provided is outside the range allowed by the range proof".into(),
            )));
        }

        let commitment_private_key = self.get_private_key(commitment_mask_key_id)?;
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
            KeyManagerError::TransactionError(TransactionError::RangeProofError(
                "Rangeproof factory returned invalid range proof bytes".to_string(),
            ))
        })
    }

    fn get_script_signature(
        &self,
        script_key_id: &TariKeyId,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: TransactionInputVersion,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        if self.wallet_type.is_ledger() {
            let signature = self
                .ledger_get_script_signature_wrapper(
                    txi_version,
                    script_key_id,
                    value,
                    commitment_mask_key_id,
                    script_message,
                )
                .map_err(|e| TransactionError::InvalidSignatureError(e.to_string()))?;
            Ok(signature)
        } else {
            let commitment = self.get_commitment(commitment_mask_key_id, value)?;
            let commitment_private_key = self.get_private_key(commitment_mask_key_id)?;
            let r_a = PrivateKey::random(&mut rand::rng());
            let r_x = PrivateKey::random(&mut rand::rng());
            let r_y = PrivateKey::random(&mut rand::rng());
            let ephemeral_commitment = self.crypto_factories.commitment.commit(&r_x, &r_a);
            let ephemeral_pubkey = CompressedPublicKey::from_secret_key(&r_y);
            let script_private_key = self.get_private_key(script_key_id)?;

            let challenge = TransactionInput::finalize_script_signature_challenge(
                txi_version,
                &(CompressedCommitment::from_commitment(ephemeral_commitment)),
                &ephemeral_pubkey,
                &self.get_public_key_at_key_id(script_key_id)?,
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
        }
    }

    fn get_partial_script_signature(
        &self,
        commitment_mask_id: &TariKeyId,
        value: &PrivateKey,
        txi_version: TransactionInputVersion,
        ephemeral_pubkey: &CompressedPublicKey,
        script_public_key: &CompressedPublicKey,
        script_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let private_commitment_mask = self.get_private_key(commitment_mask_id)?;
        let commitment = self.get_commitment(commitment_mask_id, value)?;
        let r_a = PrivateKey::random(&mut rand::rng());
        let r_x = PrivateKey::random(&mut rand::rng());
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

    fn get_partial_txo_kernel_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
        total_nonce: &CompressedPublicKey,
        total_excess: &CompressedPublicKey,
        kernel_version: TransactionKernelVersion,
        kernel_message: &[u8; 32],
        kernel_features: &KernelFeatures,
        txo_type: TxoStage,
    ) -> Result<CompressedSignature, KeyManagerError> {
        let private_key = self.get_private_key(commitment_mask_key_id)?;
        // We cannot use an offset with a coinbase tx as this will not allow us to check the coinbase commitment and
        // because the offset function does not know if its a coinbase or not, we need to know if we need to bypass it
        // or not
        let private_signing_key = if kernel_features.is_coinbase() {
            private_key
        } else {
            private_key - &self.get_txo_private_kernel_offset(commitment_mask_key_id, nonce_id)?
        };

        // We need to check if its input or output for which we are singing. Signing with an input, we need to sign
        // with `-k` while outputs are `k`
        let final_signing_key = if txo_type == TxoStage::Output {
            private_signing_key
        } else {
            PrivateKey::default() - &private_signing_key
        };

        let private_nonce = self.get_private_key(nonce_id)?;
        let challenge = TransactionKernel::finalize_kernel_signature_challenge(
            kernel_version,
            total_nonce,
            total_excess,
            kernel_message,
        );

        let signature = UncompressedSignature::sign_raw_uniform(&final_signing_key, private_nonce, &challenge)?;
        Ok(CompressedSignature::new_from_schnorr(signature))
    }

    fn get_txo_kernel_signature_excess_with_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce: &TariKeyId,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        let private_key = self.get_private_key(commitment_mask_key_id)?;
        let offset = self.get_txo_private_kernel_offset(commitment_mask_key_id, nonce)?;
        let excess = private_key - &offset;
        Ok(CompressedPublicKey::from_secret_key(&excess))
    }

    fn get_txo_private_kernel_offset(
        &self,
        commitment_mask_key_id: &TariKeyId,
        nonce_id: &TariKeyId,
    ) -> Result<PrivateKey, KeyManagerError> {
        let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
            "kernel_excess_offset",
        );
        let commitment_private_key = self.get_private_key(commitment_mask_key_id)?;
        let nonce_private_key = self.get_private_key(nonce_id)?;
        let key_hash = hasher
            .chain(commitment_private_key.as_bytes())
            .chain(nonce_private_key.as_bytes())
            .finalize();
        PrivateKey::from_uniform_bytes(key_hash.as_ref()).map_err(|_| {
            KeyManagerError::TransactionError(TransactionError::KeyManagerError(
                "Invalid private key for kernel signature nonce".to_string(),
            ))
        })
    }

    fn encrypt_data_for_recovery(
        &self,
        commitment_mask_key_id: &TariKeyId,
        custom_recovery_key_id: Option<&TariKeyId>,
        value: u64,
        payment_id: MemoField,
    ) -> Result<EncryptedData, KeyManagerError> {
        let recovery_key = if let Some(key_id) = custom_recovery_key_id {
            self.get_private_key(key_id)?
        } else {
            self.get_private_view_key()
        };
        let value_key = value.into();
        let commitment = self.get_commitment(commitment_mask_key_id, &value_key)?;
        let commitment_private_key = self.get_private_key(commitment_mask_key_id)?;
        let data = EncryptedData::encrypt_data(
            &recovery_key,
            &commitment,
            value.into(),
            &commitment_private_key,
            payment_id,
        )?;
        Ok(data)
    }

    fn try_output_key_recovery(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        sender_offset_public_key: &CompressedPublicKey,
    ) -> Result<Option<(TariKeyId, MicroMinotari, MemoField)>, KeyManagerError> {
        let (value, private_key, payment_id, key_id) =
            match EncryptedData::decrypt_data(&self.get_private_view_key(), commitment, encrypted_data) {
                Ok((value, private_key, payment_id)) => {
                    let key = self.create_encrypted_key(private_key.clone(), None)?;
                    (value, private_key, payment_id, key)
                },
                Err(_) => {
                    // so this is not change, lets try with the offset key
                    let view_key = self.get_view_key().key_id;
                    let shared_secret = self.get_diffie_hellman_shared_secret(&view_key, sender_offset_public_key)?;

                    let encryption_key = public_key_to_output_encryption_key(&shared_secret)?;
                    match EncryptedData::decrypt_data(&encryption_key, commitment, encrypted_data) {
                        Ok((value, private_key, payment_id)) => {
                            let key = TariKeyId::DHCommitmentMask {
                                public_key: sender_offset_public_key.clone(),
                                private_key: view_key.into(),
                            };
                            if self.get_private_key(&key)? == private_key {
                                (value, private_key, payment_id, key)
                            } else {
                                let key = self.create_encrypted_key(private_key.clone(), None)?;
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

    fn is_this_output_ours(
        &self,
        commitment: &CompressedCommitment,
        encrypted_data: &EncryptedData,
        custom_recovery_key_id: Option<PrivateKey>,
    ) -> Result<bool, KeyManagerError> {
        let recovery_key = if let Some(key) = custom_recovery_key_id {
            key
        } else {
            self.get_private_view_key()
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

    fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_key_ids: &[TariKeyId],
    ) -> Result<PrivateKey, KeyManagerError> {
        if self.wallet_type.is_ledger() {
            self.ledger_get_script_offset_wrapper(script_key_ids, sender_offset_key_ids)
        } else {
            let mut total_script_private_key = PrivateKey::default();
            for script_key_id in script_key_ids {
                total_script_private_key = &total_script_private_key + self.get_private_key(script_key_id)?
            }
            let mut total_sender_offset_private_key = PrivateKey::default();
            for sender_offset_key_id in sender_offset_key_ids {
                total_sender_offset_private_key =
                    total_sender_offset_private_key + self.get_private_key(sender_offset_key_id)?;
            }
            let script_offset = total_script_private_key - total_sender_offset_private_key;
            Ok(script_offset)
        }
    }

    // Creates a metadata signature for the output without requiring manual user verification on a ledger device
    fn get_metadata_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &TariKeyId,
        txo_version: TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let sender_offset_public_key = self.get_public_key_at_key_id(sender_offset_key_id)?;
        let ephemeral_pubkey = self.get_random_key(None, None)?;
        let receiver_partial_metadata_signature = self.get_receiver_partial_metadata_signature(
            commitment_mask_key_id,
            value_as_private_key,
            &sender_offset_public_key,
            &ephemeral_pubkey.pub_key,
            txo_version,
            metadata_signature_message,
            range_proof_type,
        )?;
        let commitment = self.get_commitment(commitment_mask_key_id, value_as_private_key)?;
        let ephemeral_commitment = receiver_partial_metadata_signature.ephemeral_commitment();
        let sender_partial_metadata_signature = self.get_sender_partial_metadata_signature(
            &ephemeral_pubkey.key_id,
            sender_offset_key_id,
            &commitment,
            ephemeral_commitment,
            txo_version,
            metadata_signature_message,
        )?;
        let metadata_signature = ComAndPubSignature::new_from_capk_signature(
            &receiver_partial_metadata_signature.to_capk_signature()? +
                &sender_partial_metadata_signature.to_capk_signature()?,
        );
        Ok(metadata_signature)
    }

    // Creates a metadata signature for the output requiring manual user verification on a ledger device
    fn get_metadata_signature_user_verified(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: MicroMinotari,
        sender_offset_key_id: &TariKeyId,
        txo_version: TransactionOutputVersion,
        metadata_signature_message_common: &[u8; 32],
        range_proof_type: RangeProofType,
        script: &TariScript,
        receiver_address: &TariAddress,
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        if self.wallet_type.is_ledger() {
            let comm_and_pub_sig = self.ledger_get_one_sided_metadata_signature_wrapper(
                txo_version,
                value,
                sender_offset_key_id,
                commitment_mask_key_id,
                receiver_address,
                metadata_signature_message_common,
            )?;

            Ok(comm_and_pub_sig)
        } else {
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
        }
    }

    fn sign_message_with_spend_key(
        &self,
        message: &[u8],
        sender_offset_key: Option<&CompressedPublicKey>,
    ) -> Result<WalletMessageSchnorrSignature, KeyManagerError> {
        if self.wallet_type.is_ledger() {
            return Err(KeyManagerError::LedgerError(
                "sign_message requires software keys".to_string(),
            ));
        }

        let spend_key = self.get_spend_key();

        if let Some(sender_offset_pub_key) = sender_offset_key {
            let spend_key_id = &self.add_offset_to_key(&spend_key.key_id, sender_offset_pub_key)?;
            SignatureWithDomain::<WalletMessageSigningDomain>::sign(
                &self.get_private_key(spend_key_id)?,
                message,
                &mut rand::rng(),
            )
            .map_err(|e| KeyManagerError::UnexpectedError(e.to_string()))
        } else {
            SignatureWithDomain::<WalletMessageSigningDomain>::sign(
                &self.get_private_key(&spend_key.key_id)?,
                message,
                &mut rand::rng(),
            )
            .map_err(|e| KeyManagerError::UnexpectedError(e.to_string()))
        }
    }

    fn sign_script_message(
        &self,
        private_key_id: &TariKeyId,
        challenge: &[u8],
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerError> {
        if self.wallet_type.is_ledger() &&
            let TariKeyId::LedgerKey { branch, index } = private_key_id
        {
            return self.ledger_get_script_schnorr_signature_wrapper(*index, *branch, challenge);
        }

        let private_key = self.get_private_key(private_key_id)?;
        let signature = CheckSigSchnorrSignature::sign(&private_key, challenge, &mut rand::rng())?;

        Ok(CompressedCheckSigSchnorrSignature::new_from_schnorr(signature))
    }

    fn sign_script_message_with_spend_key(
        &self,
        message: &[u8],
        sender_offset_pub_key: Option<&CompressedPublicKey>,
    ) -> Result<CompressedCheckSigSchnorrSignature, KeyManagerError> {
        let spend_key = self.get_spend_key();

        if let Some(sender_offset_pub_key) = sender_offset_pub_key {
            self.sign_script_message(
                &self.add_offset_to_key(&spend_key.key_id, sender_offset_pub_key)?,
                message,
            )
        } else {
            self.sign_script_message(&spend_key.key_id, message)
        }
    }

    fn sign_with_nonce_and_challenge(
        &self,
        private_key_id: &TariKeyId,
        nonce: &TariKeyId,
        challenge: &[u8; 64],
    ) -> Result<CompressedSignature, KeyManagerError> {
        match (private_key_id, nonce) {
            (
                TariKeyId::LedgerKey {
                    branch: private_key_branch,
                    index: private_key_index,
                },
                TariKeyId::LedgerKey {
                    branch: nonce_branch,
                    index: nonce_index,
                },
            ) => self.ledger_get_raw_schnorr_signature_wrapper(
                *private_key_index,
                *private_key_branch,
                *nonce_index,
                *nonce_branch,
                challenge,
            ),
            (TariKeyId::LedgerKey { .. }, _) | (_, TariKeyId::LedgerKey { .. }) => Err(KeyManagerError::LedgerError(
                "Trying to access Ledger key paired to a non ledger key".to_string(),
            )),
            _ => {
                let private_key = self.get_private_key(private_key_id)?;
                let private_nonce = self.get_private_key(nonce)?;
                let signature = UncompressedSignature::sign_raw_uniform(&private_key, private_nonce, challenge)?;

                Ok(CompressedSignature::new_from_schnorr(signature))
            },
        }
    }

    fn get_receiver_partial_metadata_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value: &PrivateKey,
        sender_offset_public_key: &CompressedPublicKey,
        ephemeral_pubkey: &CompressedPublicKey,
        txo_version: TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let ephemeral_commitment_nonce = self.get_random_key(None, None)?;
        let (nonce_a, nonce_b) = self
            .get_metadata_signature_ephemeral_private_key_pair(&ephemeral_commitment_nonce.key_id, range_proof_type)?;
        let ephemeral_commitment = self.crypto_factories.commitment.commit(&nonce_b, &nonce_a);
        let commitment_private_key = self.get_private_key(commitment_mask_key_id)?;
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

    // In the case where the sender is an aggregated signer, we need to parse in the other public key shares, this is
    // done in: aggregated_sender_offset_public_keys and aggregated_ephemeral_public_keys. If there is no aggregated
    // signers, this can be left as none
    fn get_sender_partial_metadata_signature(
        &self,
        ephemeral_private_nonce_id: &TariKeyId,
        sender_offset_key_id: &TariKeyId,
        commitment: &CompressedCommitment,
        ephemeral_commitment: &CompressedCommitment,
        txo_version: TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let ephemeral_pubkey = self.get_public_key_at_key_id(ephemeral_private_nonce_id)?;
        let sender_offset_public_key = self.get_public_key_at_key_id(sender_offset_key_id)?;

        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            txo_version,
            &sender_offset_public_key,
            ephemeral_commitment,
            &ephemeral_pubkey,
            commitment,
            metadata_signature_message,
        );

        let sender_partial_metadata_signature_self =
            self.sign_with_nonce_and_challenge(sender_offset_key_id, ephemeral_private_nonce_id, &challenge)?;

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

    fn generate_burn_claim_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        amount: u64,
        claim_public_key: &CompressedPublicKey,
        sidechain_id: Option<&CompressedPublicKey>,
    ) -> Result<CompressedSignature, KeyManagerError> {
        let mask = self.get_private_key(commitment_mask_key_id)?;
        let commitment =
            CompressedCommitment::from_commitment(self.crypto_factories.commitment.commit(&mask, &amount.into()));

        // Bind the proof to the target sidechain so it cannot be replayed to claim the burn on a
        // different sidechain/application that shares this claim mechanism (tari-ootle#445). The
        // Tari network is already mixed into the hash domain by ConfidentialOutputHasher, and the
        // Option encoding distinguishes "no sidechain" from a specific one.
        let message = ConfidentialOutputHasher::new("commitment_signature")
            .chain(&commitment)
            .chain(claim_public_key)
            .chain(&sidechain_id)
            .finalize();

        let s = UncompressedSignature::sign(&mask, message, &mut rand::rng())
            .map_err(|e| TransactionError::InvalidSignatureError(format!("Failed to sign burn claim proof: {}", e)))?;
        Ok(CompressedSignature::new_from_schnorr(s))
    }

    fn derive_burn_sender_offset_key(
        &self,
        commitment_mask_key_id: &TariKeyId,
    ) -> Result<TariKeyAndId, KeyManagerError> {
        let mask = self.get_private_key(commitment_mask_key_id)?;
        let hash = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
            HASHER_LABEL_BURN_SENDER_OFFSET,
        )
        .chain(mask.as_bytes())
        .finalize();
        let secret = PrivateKey::from_uniform_bytes(hash.as_ref())?;
        let pub_key = CompressedPublicKey::from_secret_key(&secret);
        let key_id = self.create_encrypted_key(secret, None)?;
        Ok(TariKeyAndId { pub_key, key_id })
    }

    fn compute_stealth_claim_public_key(
        &self,
        sender_offset_key_id: &TariKeyId,
        account_public_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        let shared_secret = self.get_diffie_hellman_shared_secret(sender_offset_key_id, account_public_key)?;
        let stealth_hash = diffie_hellman_stealth_domain_hasher(&shared_secret);
        let scalar = PrivateKey::from_uniform_bytes(stealth_hash.as_ref())?;
        let scalar_point = CompressedPublicKey::from_secret_key(&scalar);
        let stealth_public = scalar_point.to_public_key()? + &account_public_key.to_public_key()?;
        Ok(CompressedPublicKey::new_from_pk(stealth_public))
    }

    fn stealth_address_script_spending_key(
        &self,
        commitment_mask_key_id: &TariKeyId,
        spend_key: &CompressedPublicKey,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        let private_key = self.get_private_key(commitment_mask_key_id)?;
        let hasher =
            DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label("script key");
        let hasher = hasher.chain(private_key.as_bytes()).finalize();
        let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref())?;
        let public_key = CompressedPublicKey::from_secret_key(&private_key);
        let public_key = spend_key.to_public_key()? + &public_key.to_public_key()?;
        Ok(CompressedPublicKey::new_from_pk(public_key))
    }
}

impl SecretTransactionKeyManagerInterface for KeyManager {
    fn get_private_key(&self, key_id: &TariKeyId) -> Result<PrivateKey, KeyManagerError> {
        match key_id {
            TariKeyId::Zero => Ok(PrivateKey::default()),
            TariKeyId::SpendKey => self.get_private_spend_key(),
            TariKeyId::ViewKey => Ok(self.get_private_view_key()),
            TariKeyId::Derived { key } => {
                let inner_key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerError::InvalidKeyId(key.to_string()))?;
                let commitment_mask = self.get_private_key(&inner_key)?;
                let spend_key = self.get_private_spend_key()?;
                let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                    HASHER_LABEL_STEALTH_KEY,
                );
                let hasher = hasher.chain(commitment_mask.as_bytes()).finalize();
                let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref())
                    .map_err(|_| KeyManagerError::UnexpectedError("Invalid private key for Spend".to_string()))?;
                let private_key = private_key + spend_key;
                Ok(private_key)
            },
            TariKeyId::CodeTemplateAuthor => {
                let public_spend_key = self.get_spend_key().pub_key;
                let hasher = DomainSeparatedHasher::<Blake2b<U64>, KeyManagerTransactionsHashDomain>::new_with_label(
                    CODE_TEMPLATE_AUTHOR_LABEL,
                );
                let hasher = hasher.chain(public_spend_key.as_bytes()).finalize();
                let private_key = PrivateKey::from_uniform_bytes(hasher.as_ref()).map_err(|_| {
                    KeyManagerError::UnexpectedError("Invalid private key for sender offset private key".to_string())
                })?;
                let spend_key = self.get_private_spend_key()?;
                let private_key = private_key + &spend_key;
                Ok(private_key)
            },
            TariKeyId::LedgerKey { .. } => Err(KeyManagerError::LedgerError(
                "Cannot access ledger private keys".to_string(),
            )),
            TariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerError::InvalidKeyId(private_key.to_string()))?;
                let shared_secret = self.get_diffie_hellman_shared_secret(&key, public_key)?;
                let commitment_mask_private_key = public_key_to_output_spending_key(&shared_secret)?;
                Ok(commitment_mask_private_key)
            },
            TariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerError::InvalidKeyId(private_key.to_string()))?;
                let shared_secret = self.get_diffie_hellman_shared_secret(&key, public_key)?;
                let commitment_mask_private_key = public_key_to_output_encryption_key(&shared_secret)?;
                Ok(commitment_mask_private_key)
            },
            TariKeyId::Encrypted { encrypted, key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerError::InvalidKeyId(key.to_string()))?;
                let private_key = self.decrypt_encrypted_key(encrypted, key)?;
                Ok(private_key)
            },
        }
    }
}

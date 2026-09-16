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

use std::{
    collections::HashMap,
    ops::Shl,
    str::FromStr,
    sync::{Arc, Mutex, MutexGuard},
};

use blake2::Blake2b;
use chacha20poly1305::{Key, XChaCha20Poly1305};
use digest::{KeyInit, consts::U64};
use log::trace;
use minotari_ledger_wallet_common::{common_types::LedgerKeyBranch, script_offset::MAX_SENDER_OFFSET_KEYS};
#[cfg(feature = "ledger")]
use minotari_ledger_wallet_comms::accessor_methods::{
    ScriptSignatureKey,
    ledger_generate_ephemeral_nonce,
    ledger_get_dh_shared_secret,
    ledger_get_one_sided_metadata_signature,
    ledger_get_public_key,
    ledger_get_raw_schnorr_signature,
    ledger_get_raw_schnorr_signature_legacy_nonce,
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
use zeroize::{Zeroize, Zeroizing};

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

/// How many ephemeral nonces a software wallet will hold at once.
///
/// A nonce only leaves this store by being signed with, and a caller that reserves and then fails before it signs
/// abandons its entry for the life of the process. The bound stops that from growing without limit; reaching it is
/// not an error, because the store evicts its oldest entry to make room, exactly as the device's does and for the
/// same reason. An evicted nonce was never signed with, so no signature exists over it and there is nothing to
/// reuse - see `minotari_ledger_wallet_common::ephemeral_nonce::EphemeralNonceStore::insert`.
///
/// It is deliberately far above any real multi-party flow, so eviction should only ever be reclaiming leaks.
const MAX_SOFTWARE_EPHEMERAL_NONCES: usize = 1024;

/// The software wallet's counterpart to the Ledger device's ephemeral nonce store.
///
/// It exists so that there is exactly one reserve-then-sign call pattern regardless of wallet type: without it the
/// multi-party flows would only ever exercise device-issued handles on hardware nobody runs in CI. It mirrors the
/// device's eviction policy for the same reason, so the two cannot fail differently under the same abuse.
#[derive(Default)]
struct SoftwareEphemeralNonceStore {
    nonces: HashMap<u64, PrivateKey>,
    /// The last handle issued. Handles are never reused, and zero is never issued, so a handle that has been signed
    /// with cannot be resurrected by a later reservation landing on the same value.
    last_handle: u64,
}

#[derive(Clone)]
pub struct KeyManager {
    crypto_factories: CryptoFactories,
    wallet_type: WalletType,
    /// Shared across clones on purpose: a nonce reserved through one handle to the key manager has to be signable
    /// through another, because the wrappers hand out clones freely.
    software_ephemeral_nonces: Arc<Mutex<SoftwareEphemeralNonceStore>>,
}

/// Whether the sender half of a metadata signature can be signed with a reserved ephemeral nonce.
///
/// `sign_with_nonce_and_challenge` only pairs a nonce with a key that is held by the same side. On a ledger wallet
/// `reserve_ephemeral_nonce` returns a *device* nonce, so it pairs with a device held sender offset key - which is
/// every key `get_script_offset` issues, and therefore every sender offset key a ledger wallet spends - and with
/// nothing else. A software sender offset key on a ledger wallet (the coinbase builder still mints its own) keeps
/// the host drawn nonce it has always had, because both a device nonce and a software store nonce are refused
/// against it.
///
/// On a software wallet everything is host held, so a reserved nonce always pairs, and it is strictly better than a
/// host drawn one: it can only be signed with once.
fn sender_offset_key_takes_a_reserved_nonce(wallet_is_ledger: bool, sender_offset_key_id: &TariKeyId) -> bool {
    !wallet_is_ledger || matches!(sender_offset_key_id, TariKeyId::LedgerKey { .. })
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
            software_ephemeral_nonces: Arc::default(),
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
            software_ephemeral_nonces: Arc::default(),
        })
    }

    pub fn new_random() -> Result<Self, KeyManagerError> {
        Ok(Self {
            crypto_factories: CryptoFactories::default(),
            wallet_type: WalletType::new_random()?,
            software_ephemeral_nonces: Arc::default(),
        })
    }

    fn created_encrypted_key(
        &self,
        private_key: PrivateKey,
        encryption_key: TariKeyId,
    ) -> Result<TariKeyId, KeyManagerError> {
        // `to_vec` copies the key out of the zeroizing `PrivateKey` into a plain heap allocation; wrap it so that copy
        // is wiped when it goes out of scope rather than left in freed memory.
        let private_encryption_key = Zeroizing::new(self.get_private_key(&encryption_key)?.to_vec());
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

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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

    /// Ask the ledger device for a script offset.
    ///
    /// Only the script side of the sum crosses the wire. The device generates `sender_offset_count` sender offset
    /// keys itself and returns the base index it derived them from, so the host can neither choose nor learn the
    /// keys that blind the result.
    ///
    /// The device also refuses to answer unless at least one script side term was derived on the device, so this
    /// mirrors [`TariKeyId::is_ledger_key`]: a request whose only script keys are host known would be answered
    /// with a sender offset private key the device just generated.
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    fn ledger_get_script_offset_wrapper(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_count: usize,
    ) -> Result<(PrivateKey, Vec<TariKeyAndId>), KeyManagerError> {
        // Checked before any key material is touched, and before the transport is opened, so a request the device
        // would refuse costs nothing and surfaces as a typed error rather than a status word.
        let max = usize::try_from(MAX_SENDER_OFFSET_KEYS).unwrap_or(usize::MAX);
        if sender_offset_count > max {
            return Err(KeyManagerError::TooManySenderOffsetKeys {
                requested: sender_offset_count,
                max,
            });
        }
        if !script_key_ids.iter().any(|k| k.is_ledger_key()) {
            return Err(KeyManagerError::NoDeviceScriptKeys {
                script_keys: script_key_ids.len(),
            });
        }

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
                    _ => {
                        partial_script_offset = &partial_script_offset + self.get_private_key(script_key_id)?;
                    },
                }
            }

            let (script_offset, sender_offset_indexes) = ledger_get_script_offset(
                ledger.account,
                &partial_script_offset,
                &derived_script_keys,
                &script_key_indexes,
                sender_offset_count,
            )
            .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;

            let mut sender_offset_keys = Vec::with_capacity(sender_offset_indexes.len());
            for index in sender_offset_indexes {
                let key_id = TariKeyId::LedgerKey {
                    branch: LedgerKeyBranch::OneSidedSenderOffset,
                    index,
                };
                let pub_key = self.get_public_key_at_key_id(&key_id)?;
                sender_offset_keys.push(TariKeyAndId { key_id, pub_key });
            }
            return Ok((script_offset, sender_offset_keys));
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger script offset with script_key_ids: {:?}, sender_offset_count: {}",
            script_key_ids,
            sender_offset_count);
        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    /// Reserve a nonce on the ledger device.
    ///
    /// The device draws the scalar and keeps it; only the handle naming it and its public form come back. The host
    /// can therefore neither choose the nonce nor use it twice, which is what stops two signatures over different
    /// challenges from giving up the key that signed them.
    fn ledger_generate_ephemeral_nonce_wrapper(&self) -> Result<TariKeyAndId, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let (handle, pub_key) = ledger_generate_ephemeral_nonce(ledger.account)
                .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(TariKeyAndId {
                key_id: TariKeyId::LedgerEphemeralNonce { handle },
                pub_key,
            });
        }

        trace!(target: "wallet::key_manager::ledger", "Trying to reserve a ledger ephemeral nonce");
        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    fn ledger_get_raw_schnorr_signature_wrapper(
        &self,
        private_key_index: u64,
        private_key: LedgerKeyBranch,
        nonce_handle: u64,
        challenge: &[u8; 64],
    ) -> Result<CompressedSignature, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let signature = ledger_get_raw_schnorr_signature(
                ledger.account,
                private_key_index,
                private_key,
                nonce_handle,
                challenge,
            )
            .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?;
            return Ok(signature);
        }

        trace!(target: "wallet::key_manager::ledger",
            "Trying to get ledger raw schnorr signature with private_key_index: {:?}, private_key: {}, nonce_handle: {}, challenge: {:?}",
            private_key_index,
            private_key,
            nonce_handle,
            challenge);

        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    /// DEPRECATED - DO NOT ADD CALLERS. Sign with a deterministic, host indexed nonce.
    ///
    /// The host picks the nonce index here, so a compromised host can ask for two signatures over the same key and
    /// nonce with different challenges and solve for the private key. Only the pre-mine spend flow still uses it,
    /// because its nonces are reserved in step 2 and spent in step 3 with a file, not a device session, in between.
    ///
    /// See [`minotari_ledger_wallet_common::legacy_nonce`] for the canonical account of what this costs -
    /// including why allowing the sender offset branch reaches pre-mine script keys as well - the scope of the
    /// exposure, and the TODO that deletes this wrapper along with the rest of the legacy path.
    fn ledger_get_raw_schnorr_signature_legacy_nonce_wrapper(
        &self,
        private_key_index: u64,
        private_key: LedgerKeyBranch,
        nonce_index: u64,
        nonce: LedgerKeyBranch,
        challenge: &[u8; 64],
    ) -> Result<CompressedSignature, KeyManagerError> {
        #[cfg(feature = "ledger")]
        if let Some(ledger) = self.wallet_type.get_ledger_details() {
            let signature = ledger_get_raw_schnorr_signature_legacy_nonce(
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
            "Trying to get ledger legacy raw schnorr signature with private_key_index: {:?}, private_key:{}, nonce_index: {}, nonce: {}, challenge: {:?}",
            private_key_index,
            private_key,
            nonce_index,
            nonce,
            challenge);

        Err(KeyManagerError::InvalidWalletType(
            "Trying to access Ledger key on non-Ledger wallet".to_string(),
        ))
    }

    /// Poisoning means a previous holder panicked mid-update, so the store's contents cannot be trusted to say
    /// which nonces are still unused. Surfacing that as an error is the only safe answer; recovering the guard
    /// would risk handing out a nonce that was already signed with.
    fn lock_software_ephemeral_nonces(&self) -> Result<MutexGuard<'_, SoftwareEphemeralNonceStore>, KeyManagerError> {
        self.software_ephemeral_nonces
            .lock()
            .map_err(|_| KeyManagerError::EphemeralNonceStorePoisoned)
    }

    fn reserve_software_ephemeral_nonce(&self) -> Result<TariKeyAndId, KeyManagerError> {
        let private_nonce = PrivateKey::random(&mut rand::rng());
        let pub_key = CompressedPublicKey::from_secret_key(&private_nonce);

        let mut store = self.lock_software_ephemeral_nonces()?;
        // Handles are issued from a strictly increasing counter and zero is never issued, so a consumed handle is
        // dead for good rather than something a later reservation can land on again. Exhausting the counter is the
        // only condition here that refuses, because re-issuing a handle is the only one that would be unsafe.
        if store.last_handle == u64::MAX {
            return Err(KeyManagerError::EphemeralNonceHandlesExhausted);
        }
        // A full store evicts its oldest entry instead of refusing, so that reservations abandoned by a failure
        // between reserving and signing are reclaimed rather than wedging the wallet for the life of the process.
        // The lowest handle is the oldest reservation. See
        // `minotari_ledger_wallet_common::ephemeral_nonce::EphemeralNonceStore::insert` for why this is safe.
        if store.nonces.len() >= MAX_SOFTWARE_EPHEMERAL_NONCES &&
            let Some(oldest) = store.nonces.keys().min().copied()
        {
            // Dropping the evicted nonce zeroizes it.
            drop(store.nonces.remove(&oldest));
        }
        let handle = store.last_handle.saturating_add(1);
        store.last_handle = handle;
        store.nonces.insert(handle, private_nonce);

        Ok(TariKeyAndId {
            key_id: TariKeyId::LedgerEphemeralNonce { handle },
            pub_key,
        })
    }

    /// Take a reserved nonce out of the software store.
    ///
    /// Reading a nonce and consuming it are the same operation, so there is no way to sign with one twice.
    fn take_software_ephemeral_nonce(&self, handle: u64) -> Result<PrivateKey, KeyManagerError> {
        self.lock_software_ephemeral_nonces()?
            .nonces
            .remove(&handle)
            .ok_or(KeyManagerError::UnknownEphemeralNonce { handle })
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
        // Sender offset keys must be generated by the device inside `get_script_offset`, otherwise the host picks
        // the index and can strip the blinding back out of the script offset it is handed. The spend branch is
        // never addressable by index at all.
        //
        // Note: signing nonces are no longer requested here at all. They are reserved through
        // `reserve_ephemeral_nonce`, which issues a handle to a nonce the issuer generated, because a nonce the
        // host indexed can be asked for twice and two signatures under one nonce give up the key that signed them.
        if let Some(branch) = ledger_key {
            match branch {
                LedgerKeyBranch::Random | LedgerKeyBranch::PreMine => {},
                LedgerKeyBranch::OneSidedSenderOffset | LedgerKeyBranch::Spend => {
                    return Err(KeyManagerError::InvalidKeyBranch(format!(
                        "'{branch}' keys cannot be requested through 'get_random_key'; sender offset keys are only \
                         issued by 'get_script_offset'"
                    )));
                },
            }
        }
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

    fn reserve_ephemeral_nonce(&self) -> Result<TariKeyAndId, KeyManagerError> {
        if self.wallet_type.is_ledger() {
            return self.ledger_generate_ephemeral_nonce_wrapper();
        }
        self.reserve_software_ephemeral_nonce()
    }

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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
            TariKeyId::LedgerEphemeralNonce { handle } => {
                // The public nonce is handed out once, by the `reserve_ephemeral_nonce` call that created the
                // handle, and is not recoverable from the handle afterwards - on a ledger wallet the device is the
                // only thing that could recompute it, and it will not. Callers must keep the `TariKeyAndId` they
                // were given.
                Err(KeyManagerError::InvalidKeyId(format!(
                    "The public nonce of ephemeral nonce handle '{handle}' is only returned by \
                     'reserve_ephemeral_nonce' and cannot be recovered from the key id"
                )))
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

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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

    /// Compute a partial script offset for `script_key_ids`, generating `sender_offset_count` fresh sender offset
    /// keys in the process.
    ///
    /// The returned offset is `sum(script keys) - sum(generated sender offset keys)`, and the caller must use each
    /// returned key on exactly one output.
    ///
    /// Neither sum may leave the key manager unblinded by a term the caller cannot compute, so both sides need at
    /// least one key that contributes:
    ///
    /// - With no sender offset key the result is the plain sum of the input script private keys. Those keys are
    ///   `H("script key", b) + alpha` for blinding factors `b` the caller chose, so the caller can subtract the hashes
    ///   it already knows and be left with `alpha`, the wallet's root spend key.
    /// - With no script key the result is `-k_sender` for a key the key manager just generated, which on a ledger
    ///   wallet hands the host a `OneSidedSenderOffset` private key: enough to recompute the one sided Diffie-Hellman
    ///   secrets for that output and re-sign its metadata signature without the device.
    ///
    /// `TariKeyId::Zero` is rejected rather than filtered out. It is dropped on the ledger path and yields the zero
    /// scalar in software, so filtering would let a caller satisfy the length check with a slice that contributes
    /// nothing.
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    fn get_script_offset(
        &self,
        script_key_ids: &[TariKeyId],
        sender_offset_count: usize,
    ) -> Result<(PrivateKey, Vec<TariKeyAndId>), KeyManagerError> {
        // Checked before any key material is touched.
        let contributing_script_keys = script_key_ids.iter().filter(|k| **k != TariKeyId::Zero).count();
        if sender_offset_count == 0 || contributing_script_keys == 0 || contributing_script_keys != script_key_ids.len()
        {
            return Err(KeyManagerError::UnblindedScriptOffset {
                script_keys: contributing_script_keys,
                sender_offset_keys: sender_offset_count,
            });
        }
        if self.wallet_type.is_ledger() {
            self.ledger_get_script_offset_wrapper(script_key_ids, sender_offset_count)
        } else {
            let mut sender_offsets = Vec::with_capacity(sender_offset_count);
            let mut total_script_private_key = PrivateKey::default();
            for script_key_id in script_key_ids {
                total_script_private_key = &total_script_private_key + self.get_private_key(script_key_id)?
            }
            let mut total_sender_offset_private_key = PrivateKey::default();
            for _ in 0..sender_offset_count {
                let random_key = self.get_random_key(None, None)?;
                total_sender_offset_private_key =
                    total_sender_offset_private_key + self.get_private_key(&random_key.key_id)?;
                sender_offsets.push(random_key);
            }
            let script_offset = total_script_private_key - total_sender_offset_private_key;
            Ok((script_offset, sender_offsets))
        }
    }

    // Creates a metadata signature for the output without requiring manual user verification on a ledger device
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
    fn get_metadata_signature(
        &self,
        commitment_mask_key_id: &TariKeyId,
        value_as_private_key: &PrivateKey,
        sender_offset_key_id: &TariKeyId,
        txo_version: TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
        range_proof_type: RangeProofType,
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        // Fetched once and carried: on a ledger wallet this is a device round trip, and the sender partial
        // signature below needs the same key.
        let sender_offset = TariKeyAndId {
            pub_key: self.get_public_key_at_key_id(sender_offset_key_id)?,
            key_id: sender_offset_key_id.clone(),
        };

        // Reserved, not chosen. Since sender offset keys moved into `get_script_offset` a ledger wallet's are
        // device held, and a device held key can only be signed against a device held nonce - a host drawn one is
        // refused by `sign_with_nonce_and_challenge`, which is what broke every send that produces a change
        // output. See `sender_offset_key_takes_a_reserved_nonce` for the pairs that are and are not allowed.
        //
        // The public nonce is not recoverable from a reservation handle, so the `TariKeyAndId` is carried all the
        // way to the challenge rather than looked up again. The reserve sits after the device round trip above so
        // that the window in which a later failure abandons the reservation is as narrow as it can be.
        let ephemeral_nonce =
            if sender_offset_key_takes_a_reserved_nonce(self.wallet_type.is_ledger(), &sender_offset.key_id) {
                self.reserve_ephemeral_nonce()?
            } else {
                self.get_random_key(None, None)?
            };

        let receiver_partial_metadata_signature = self.get_receiver_partial_metadata_signature(
            commitment_mask_key_id,
            value_as_private_key,
            &sender_offset.pub_key,
            &ephemeral_nonce.pub_key,
            txo_version,
            metadata_signature_message,
            range_proof_type,
        )?;
        let commitment = self.get_commitment(commitment_mask_key_id, value_as_private_key)?;
        let ephemeral_commitment = receiver_partial_metadata_signature.ephemeral_commitment();
        let sender_partial_metadata_signature = self.get_sender_partial_metadata_signature(
            &ephemeral_nonce,
            &sender_offset,
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
                TariKeyId::LedgerEphemeralNonce { handle },
            ) => self.ledger_get_raw_schnorr_signature_wrapper(
                *private_key_index,
                *private_key_branch,
                *handle,
                challenge,
            ),
            // DEPRECATED. A ledger key paired to a host indexed ledger nonce is the pre-mine spend flow, and only
            // the pre-mine spend flow. See `minotari_ledger_wallet_common::legacy_nonce` for why it is still
            // reachable, what it costs, and the TODO that deletes this arm along with the rest of that path.
            (
                TariKeyId::LedgerKey {
                    branch: private_key_branch,
                    index: private_key_index,
                },
                TariKeyId::LedgerKey {
                    branch: nonce_branch,
                    index: nonce_index,
                },
            ) => self.ledger_get_raw_schnorr_signature_legacy_nonce_wrapper(
                *private_key_index,
                *private_key_branch,
                *nonce_index,
                *nonce_branch,
                challenge,
            ),
            (TariKeyId::LedgerKey { .. }, _) | (_, TariKeyId::LedgerKey { .. }) => Err(KeyManagerError::LedgerError(
                "Trying to access Ledger key paired to a non ledger key".to_string(),
            )),
            (_, TariKeyId::LedgerEphemeralNonce { handle }) => {
                // A ledger wallet's ephemeral nonces live on the device, so there is nothing here to look up. Say
                // so rather than reporting the handle as unknown, which would read as a caller bug.
                if self.wallet_type.is_ledger() {
                    return Err(KeyManagerError::LedgerError(
                        "Trying to access Ledger key paired to a non ledger key".to_string(),
                    ));
                }
                let private_key = self.get_private_key(private_key_id)?;
                // Consume before signing, so that no path out of here - including one a later change adds - can
                // leave the nonce available for a second challenge.
                let private_nonce = self.take_software_ephemeral_nonce(*handle)?;
                let signature = UncompressedSignature::sign_raw_uniform(&private_key, private_nonce, challenge)?;

                Ok(CompressedSignature::new_from_schnorr(signature))
            },
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
        ephemeral_private_nonce: &TariKeyAndId,
        sender_offset: &TariKeyAndId,
        commitment: &CompressedCommitment,
        ephemeral_commitment: &CompressedCommitment,
        txo_version: TransactionOutputVersion,
        metadata_signature_message: &[u8; 32],
    ) -> Result<ComAndPubSignature, KeyManagerError> {
        let challenge = TransactionOutput::finalize_metadata_signature_challenge(
            txo_version,
            &sender_offset.pub_key,
            ephemeral_commitment,
            &ephemeral_private_nonce.pub_key,
            commitment,
            metadata_signature_message,
        );

        let sender_partial_metadata_signature_self =
            self.sign_with_nonce_and_challenge(&sender_offset.key_id, &ephemeral_private_nonce.key_id, &challenge)?;

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

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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

    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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
            // An ephemeral nonce is never extractable through the generic accessor, on either wallet type. The one
            // operation a reserved nonce supports is `sign_with_nonce_and_challenge`, which consumes it; handing
            // the scalar out here would let a caller sign twice with it and give up the key it signed for.
            TariKeyId::LedgerEphemeralNonce { handle } => Err(KeyManagerError::InvalidKeyId(format!(
                "Ephemeral nonce handle '{handle}' names a one-shot signing nonce; its private key cannot be read"
            ))),
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

#[cfg(test)]
mod tests {
    use minotari_ledger_wallet_common::{common_types::LedgerKeyBranch, script_offset::MAX_SENDER_OFFSET_KEYS};
    use tari_common_types::types::PrivateKey;

    use super::{MAX_SOFTWARE_EPHEMERAL_NONCES, sender_offset_key_takes_a_reserved_nonce};
    use crate::{
        MicroMinotari,
        key_manager::{
            KeyManager,
            SecretTransactionKeyManagerInterface,
            TransactionKeyManagerInterface,
            error::KeyManagerError,
            key_id::TariKeyId,
        },
        transaction_components::{RangeProofType, TransactionOutputVersion},
    };

    /// The plain sum of the input script private keys is `H("script key", b) + alpha` summed over blinding factors
    /// the caller chose, so a caller that gets it back can subtract the hashes it already knows and be left with
    /// `alpha`, the wallet's root spend key.
    #[test]
    fn get_script_offset_refuses_an_offset_with_no_sender_offset_key() {
        let key_manager = KeyManager::new_random().unwrap();
        let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;

        let err = key_manager
            .get_script_offset(std::slice::from_ref(&script_key.key_id), 0)
            .unwrap_err();
        assert_eq!(err, KeyManagerError::UnblindedScriptOffset {
            script_keys: 1,
            sender_offset_keys: 0,
        });
    }

    /// The mirror case: with no script key the reply is `-k_sender` for a key the key manager just generated, which
    /// on a ledger wallet is a sender offset private key the host was never meant to see.
    #[test]
    fn get_script_offset_refuses_an_offset_with_no_script_keys() {
        let key_manager = KeyManager::new_random().unwrap();

        let err = key_manager.get_script_offset(&[], 1).unwrap_err();
        assert_eq!(err, KeyManagerError::UnblindedScriptOffset {
            script_keys: 0,
            sender_offset_keys: 1,
        });
    }

    /// `TariKeyId::Zero` contributes nothing - it is dropped on the ledger path and yields the zero scalar in
    /// software - so a slice of nothing but zeros must not be able to satisfy the length check.
    #[test]
    fn get_script_offset_refuses_a_slice_of_only_zero_script_keys() {
        let key_manager = KeyManager::new_random().unwrap();

        let err = key_manager
            .get_script_offset(&[TariKeyId::Zero, TariKeyId::Zero], 1)
            .unwrap_err();
        assert_eq!(err, KeyManagerError::UnblindedScriptOffset {
            script_keys: 0,
            sender_offset_keys: 1,
        });
    }

    /// ...and a zero mixed in with a real key is rejected outright rather than quietly filtered, so a caller cannot
    /// pad a slice and be surprised by which keys were actually folded in.
    #[test]
    fn get_script_offset_refuses_a_zero_script_key_mixed_with_a_real_one() {
        let key_manager = KeyManager::new_random().unwrap();
        let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;

        let err = key_manager
            .get_script_offset(&[TariKeyId::Zero, script_key.key_id.clone()], 1)
            .unwrap_err();
        assert_eq!(err, KeyManagerError::UnblindedScriptOffset {
            script_keys: 1,
            sender_offset_keys: 1,
        });

        // The same slice without the zero is fine, so the rejection really is about the zero.
        assert!(key_manager.get_script_offset(&[script_key.key_id], 1).is_ok());
    }

    /// The device derives every sender offset key in a single exchange, so the count is bounded. The bound is
    /// checked before any key is generated and before the transport is opened, so an over-large request costs
    /// nothing and surfaces as a typed error rather than a status word.
    #[test]
    fn get_script_offset_refuses_more_sender_offset_keys_than_the_device_will_derive() {
        let key_manager = KeyManager::new_random().unwrap();
        let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;
        let max = usize::try_from(MAX_SENDER_OFFSET_KEYS).unwrap();

        let err = key_manager
            .ledger_get_script_offset_wrapper(std::slice::from_ref(&script_key.key_id), max.saturating_add(1))
            .unwrap_err();
        assert_eq!(err, KeyManagerError::TooManySenderOffsetKeys {
            requested: max + 1,
            max,
        });

        // A software key manager has no such bound - the cap is a guard on device work, not a protocol rule.
        assert_eq!(
            key_manager
                .get_script_offset(std::slice::from_ref(&script_key.key_id), max + 1)
                .unwrap()
                .1
                .len(),
            max + 1
        );
    }

    /// Every returned key must be distinct, and the offset must be exactly the sum the caller can verify, otherwise
    /// the transaction it is used in will not validate.
    #[test]
    fn get_script_offset_returns_distinct_keys_and_a_matching_offset() {
        let key_manager = KeyManager::new_random().unwrap();
        let script_key_a = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;
        let script_key_b = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;
        let script_keys = [script_key_a.key_id.clone(), script_key_b.key_id.clone()];

        let (offset, sender_offset_keys) = key_manager.get_script_offset(&script_keys, 3).unwrap();
        assert_eq!(sender_offset_keys.len(), 3);
        for (i, first) in sender_offset_keys.iter().enumerate() {
            for second in sender_offset_keys.iter().skip(i + 1) {
                assert_ne!(first.key_id, second.key_id, "the same key was handed out twice");
            }
        }

        // Ristretto scalar arithmetic, not integer arithmetic: these operators cannot overflow.
        #[allow(clippy::arithmetic_side_effects)]
        {
            let mut expected = PrivateKey::default();
            for key_id in &script_keys {
                expected = expected + key_manager.get_private_key(key_id).unwrap();
            }
            for key in &sender_offset_keys {
                expected = expected - key_manager.get_private_key(&key.key_id).unwrap();
            }
            assert_eq!(offset, expected);
        }
    }

    /// The device's second rule turns on this classification: only a ledger branch index and an alpha derived
    /// blinding factor are terms the *device* turns into key material. Everything else is summed into the one
    /// opaque `partial_script_key_sum` scalar the host computed itself, so it blinds nothing against the host.
    #[test]
    fn script_keys_are_classified_by_who_derives_them() {
        let key_manager = KeyManager::new_random().unwrap();
        let script_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().1;

        // An ordinary wallet script key is `Derived { key: commitment_mask_key_id }`, so the device rule costs
        // nothing in normal use.
        assert!(script_key.key_id.is_ledger_key());
        for device_derived in [
            TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::PreMine,
                index: 7,
            },
            TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::Random,
                index: 7,
            },
        ] {
            assert!(
                device_derived.is_ledger_key(),
                "expected a device key: {device_derived:?}"
            );
        }
        for host_known in [TariKeyId::Zero, TariKeyId::SpendKey, TariKeyId::ViewKey] {
            assert!(!host_known.is_ledger_key(), "expected a host known key: {host_known:?}");
        }
    }

    /// A call whose only script term is host known would be answered with a sender offset private key the device
    /// just generated, so the ledger path refuses it before the transport is opened. The error names the fix,
    /// because the reachable way to get here is an output recovered by an older build.
    #[test]
    fn a_script_offset_whose_only_script_key_is_host_known_is_refused() {
        let key_manager = KeyManager::new_random().unwrap();

        let err = key_manager
            .ledger_get_script_offset_wrapper(&[TariKeyId::SpendKey], 1)
            .unwrap_err();
        assert_eq!(err, KeyManagerError::NoDeviceScriptKeys { script_keys: 1 });
        assert!(
            err.to_string().contains("re-run wallet recovery"),
            "the error must tell the user what to do: {err}"
        );
    }

    #[test]
    fn get_random_key_refuses_the_sender_offset_and_spend_branches() {
        let key_manager = KeyManager::new_random().unwrap();

        for branch in [LedgerKeyBranch::OneSidedSenderOffset, LedgerKeyBranch::Spend] {
            let err = key_manager.get_random_key(None, Some(branch)).unwrap_err();
            match err {
                KeyManagerError::InvalidKeyBranch(message) => {
                    assert!(message.contains("get_script_offset"), "unexpected message: {message}");
                },
                other => panic!("expected InvalidKeyBranch for '{branch}', got {other:?}"),
            }
        }

        // The branches that are still host indexed are untouched by this guard.
        for branch in [LedgerKeyBranch::Random, LedgerKeyBranch::PreMine] {
            assert!(key_manager.get_random_key(None, Some(branch)).is_ok());
        }
    }

    fn challenge(byte: u8) -> [u8; 64] {
        [byte; 64]
    }

    fn ephemeral_nonce_handle(key_id: &TariKeyId) -> u64 {
        match key_id {
            TariKeyId::LedgerEphemeralNonce { handle } => *handle,
            other => panic!("expected an ephemeral nonce handle, got {other:?}"),
        }
    }

    /// The pair `get_metadata_signature` hands to `sign_with_nonce_and_challenge` on a ledger wallet: a device held
    /// sender offset key - every sender offset key is device held since they moved into `get_script_offset` - and a
    /// device reserved nonce. It has to reach the device call rather than being turned away by the dispatch. On a
    /// software wallet "reached the device call" shows up as `InvalidWalletType`, which is raised at the transport
    /// boundary, after every guard.
    #[test]
    fn a_ledger_sender_offset_key_and_a_reserved_nonce_reach_the_ledger_call() {
        let key_manager = KeyManager::new_random().unwrap();
        let sender_offset_key_id = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::OneSidedSenderOffset,
            index: 7,
        };

        let err = key_manager
            .sign_with_nonce_and_challenge(
                &sender_offset_key_id,
                &TariKeyId::LedgerEphemeralNonce { handle: 9 },
                &challenge(1),
            )
            .unwrap_err();
        match err {
            KeyManagerError::InvalidWalletType(message) => {
                assert!(message.contains("non-Ledger wallet"), "unexpected message: {message}");
            },
            other => panic!("the change output's signing pair was turned away before the device call: {other:?}"),
        }

        // The regression this guards: a host drawn nonce cannot be paired with a device held key at all, so a
        // signing path that reaches for one breaks every send that produces a change output.
        let host_drawn_nonce = key_manager.get_random_key(None, None).unwrap();
        let err = key_manager
            .sign_with_nonce_and_challenge(&sender_offset_key_id, &host_drawn_nonce.key_id, &challenge(1))
            .unwrap_err();
        assert_eq!(
            err,
            KeyManagerError::LedgerError("Trying to access Ledger key paired to a non ledger key".to_string())
        );
    }

    /// A reserved nonce is only usable against a key held by the same side. On a ledger wallet that means the
    /// device held sender offset keys `get_script_offset` issues - and only those: the coinbase builder still mints
    /// a software sender offset key, and pairing a device nonce with it would break coinbases the way a host nonce
    /// broke change outputs. A software wallet has no such split.
    #[test]
    fn only_a_key_held_by_the_nonces_issuer_takes_a_reserved_nonce() {
        let device_held = TariKeyId::LedgerKey {
            branch: LedgerKeyBranch::OneSidedSenderOffset,
            index: 7,
        };
        // What the coinbase builder mints: `get_random_key(None, None)` never returns a device key.
        let host_held = KeyManager::new_random()
            .unwrap()
            .get_random_key(None, None)
            .unwrap()
            .key_id;

        assert!(sender_offset_key_takes_a_reserved_nonce(true, &device_held));
        assert!(!sender_offset_key_takes_a_reserved_nonce(true, &host_held));
        assert!(sender_offset_key_takes_a_reserved_nonce(false, &host_held));
    }

    /// So `get_metadata_signature` - the signing path change outputs, self payments and HTLC claims take - must
    /// draw its nonce from `reserve_ephemeral_nonce` and never from `get_random_key`. Only a software wallet can be
    /// exercised here, so this asserts the shape rather than the device call: exactly one reservation is taken
    /// across the call, and it is spent rather than abandoned.
    #[test]
    fn get_metadata_signature_signs_with_a_reserved_ephemeral_nonce() {
        let key_manager = KeyManager::new_random().unwrap();
        let commitment_mask_key = key_manager.get_next_commitment_mask_and_script_key().unwrap().0;
        let sender_offset = key_manager.get_random_key(None, None).unwrap();

        // Handles are issued from a strictly increasing counter, so bracketing the call counts the reservations it
        // made.
        let before = ephemeral_nonce_handle(&key_manager.reserve_ephemeral_nonce().unwrap().key_id);
        key_manager
            .get_metadata_signature(
                &commitment_mask_key.key_id,
                &MicroMinotari(100).into(),
                &sender_offset.key_id,
                TransactionOutputVersion::get_current_version(),
                &[1u8; 32],
                RangeProofType::BulletProofPlus,
            )
            .unwrap();
        let after = ephemeral_nonce_handle(&key_manager.reserve_ephemeral_nonce().unwrap().key_id);

        let used = before.saturating_add(1);
        assert_eq!(
            after,
            used.saturating_add(1),
            "get_metadata_signature did not reserve exactly one ephemeral nonce"
        );

        // ...and it signed with that reservation rather than leaving it behind: signing is what consumes it.
        let err = key_manager
            .sign_with_nonce_and_challenge(
                &sender_offset.key_id,
                &TariKeyId::LedgerEphemeralNonce { handle: used },
                &challenge(1),
            )
            .unwrap_err();
        assert_eq!(err, KeyManagerError::UnknownEphemeralNonce { handle: used });
    }

    /// The pre-mine spend flow signs its script signature with a `PreMine` key and its metadata signature with the
    /// `OneSidedSenderOffset` key `get_script_offset` issued, both against a `Random` branch nonce reserved back in
    /// step 2. Both pairs have to reach the device call rather than being turned away by the legacy branch
    /// whitelist - on a software wallet "reached the device call" shows up as `InvalidWalletType`, which is raised
    /// at the transport boundary, after every guard.
    ///
    /// See `minotari_ledger_wallet_common::legacy_nonce` for why that whitelist is as wide as it is.
    #[test]
    fn the_pre_mine_signing_pairs_reach_the_ledger_call() {
        let key_manager = KeyManager::new_random().unwrap();

        for key_branch in [LedgerKeyBranch::PreMine, LedgerKeyBranch::OneSidedSenderOffset] {
            let private_key_id = TariKeyId::LedgerKey {
                branch: key_branch,
                index: 7,
            };
            let nonce = TariKeyId::LedgerKey {
                branch: LedgerKeyBranch::Random,
                index: 9,
            };

            let err = key_manager
                .sign_with_nonce_and_challenge(&private_key_id, &nonce, &challenge(1))
                .unwrap_err();
            match err {
                KeyManagerError::InvalidWalletType(message) => {
                    assert!(message.contains("non-Ledger wallet"), "unexpected message: {message}");
                },
                other => panic!("'{key_branch}' was turned away before the device call: {other:?}"),
            }
        }
    }

    /// The software wallet mirrors the device's reserve-then-sign shape, so this is the path CI actually exercises.
    /// It has to produce a signature that verifies against the public nonce the reservation handed back - if the
    /// two came apart, every multi-party signature share would silently fail to aggregate.
    #[test]
    fn a_reserved_software_nonce_signs_and_verifies() {
        let key_manager = KeyManager::new_random().unwrap();
        let signing_key = key_manager.get_random_key(None, None).unwrap();
        let reserved_nonce = key_manager.reserve_ephemeral_nonce().unwrap();

        let challenge = challenge(1);
        let signature = key_manager
            .sign_with_nonce_and_challenge(&signing_key.key_id, &reserved_nonce.key_id, &challenge)
            .unwrap();

        assert_eq!(signature.get_compressed_public_nonce(), &reserved_nonce.pub_key);
        assert!(
            signature
                .to_schnorr_signature()
                .unwrap()
                .verify_raw_uniform(&signing_key.pub_key.to_public_key().unwrap(), &challenge)
        );
    }

    /// The whole point of a handle: two signatures over different challenges under one nonce give up the private
    /// key as `k = (s1 - s2) / (e1 - e2)`, so the second attempt has to fail rather than sign.
    #[test]
    fn a_software_nonce_handle_cannot_be_signed_with_twice() {
        let key_manager = KeyManager::new_random().unwrap();
        let signing_key = key_manager.get_random_key(None, None).unwrap();
        let reserved_nonce = key_manager.reserve_ephemeral_nonce().unwrap();

        key_manager
            .sign_with_nonce_and_challenge(&signing_key.key_id, &reserved_nonce.key_id, &challenge(1))
            .unwrap();

        let err = key_manager
            .sign_with_nonce_and_challenge(&signing_key.key_id, &reserved_nonce.key_id, &challenge(2))
            .unwrap_err();
        match (err, &reserved_nonce.key_id) {
            (
                KeyManagerError::UnknownEphemeralNonce { handle },
                TariKeyId::LedgerEphemeralNonce { handle: expected },
            ) => {
                assert_eq!(handle, *expected);
            },
            (other, _) => panic!("expected UnknownEphemeralNonce, got {other:?}"),
        }
    }

    /// Handles are issued, never chosen, so a handle the key manager never handed out names nothing.
    #[test]
    fn a_never_issued_software_nonce_handle_is_refused() {
        let key_manager = KeyManager::new_random().unwrap();
        let signing_key = key_manager.get_random_key(None, None).unwrap();

        for handle in [0u64, 1, u64::MAX] {
            let err = key_manager
                .sign_with_nonce_and_challenge(
                    &signing_key.key_id,
                    &TariKeyId::LedgerEphemeralNonce { handle },
                    &challenge(1),
                )
                .unwrap_err();
            assert_eq!(err, KeyManagerError::UnknownEphemeralNonce { handle });
        }
    }

    /// A reservation and the signature that spends it can arrive through different clones of the key manager,
    /// because the wrappers hand out clones freely. If the store were per-clone, every real caller would break.
    #[test]
    fn a_reserved_nonce_is_visible_through_a_clone() {
        let key_manager = KeyManager::new_random().unwrap();
        let signing_key = key_manager.get_random_key(None, None).unwrap();
        let reserved_nonce = key_manager.reserve_ephemeral_nonce().unwrap();

        let clone = key_manager.clone();
        assert!(
            clone
                .sign_with_nonce_and_challenge(&signing_key.key_id, &reserved_nonce.key_id, &challenge(1))
                .is_ok()
        );
        // ... and consuming it through the clone consumes it for the original too.
        assert!(
            key_manager
                .sign_with_nonce_and_challenge(&signing_key.key_id, &reserved_nonce.key_id, &challenge(2))
                .is_err()
        );
    }

    /// An ephemeral nonce private key must never be readable through the generic accessor: a caller that could
    /// read it could sign with it again outside the key manager, which is the reuse the handle exists to prevent.
    #[test]
    fn the_private_key_of_an_ephemeral_nonce_is_not_readable() {
        let key_manager = KeyManager::new_random().unwrap();
        let reserved_nonce = key_manager.reserve_ephemeral_nonce().unwrap();

        for key_id in [reserved_nonce.key_id.clone(), TariKeyId::LedgerEphemeralNonce {
            handle: 7,
        }] {
            match key_manager.get_private_key(&key_id).unwrap_err() {
                KeyManagerError::InvalidKeyId(message) => {
                    assert!(message.contains("cannot be read"), "unexpected message: {message}");
                },
                other => panic!("expected InvalidKeyId, got {other:?}"),
            }
        }

        // The nonce is still there to be signed with; refusing to read it must not have consumed it.
        let signing_key = key_manager.get_random_key(None, None).unwrap();
        assert!(
            key_manager
                .sign_with_nonce_and_challenge(&signing_key.key_id, &reserved_nonce.key_id, &challenge(1))
                .is_ok()
        );
    }

    /// A nonce is only released by being signed with, so a caller that reserves and then fails before it signs
    /// leaks its entry for the life of the process. The store therefore evicts rather than refusing, so those
    /// leaks are reclaimed instead of eventually wedging the wallet.
    #[test]
    fn a_full_software_nonce_store_evicts_the_oldest_entry_rather_than_refusing() {
        let key_manager = KeyManager::new_random().unwrap();
        let signing_key = key_manager.get_random_key(None, None).unwrap();

        let mut reserved = Vec::with_capacity(MAX_SOFTWARE_EPHEMERAL_NONCES);
        for _ in 0..MAX_SOFTWARE_EPHEMERAL_NONCES {
            reserved.push(key_manager.reserve_ephemeral_nonce().unwrap());
        }

        // The store is full, and reserving again still succeeds.
        let newest = key_manager.reserve_ephemeral_nonce().unwrap();

        // The oldest reservation is the one that went, and it is refused exactly like a consumed one.
        let evicted = reserved.first().expect("just filled the store");
        let err = key_manager
            .sign_with_nonce_and_challenge(&signing_key.key_id, &evicted.key_id, &challenge(1))
            .unwrap_err();
        assert!(
            matches!(err, KeyManagerError::UnknownEphemeralNonce { .. }),
            "expected the evicted handle to be unknown, got {err:?}"
        );

        // The next oldest survived, as did the reservation that displaced the evicted one.
        let survivor = reserved.get(1).expect("just filled the store");
        assert!(
            key_manager
                .sign_with_nonce_and_challenge(&signing_key.key_id, &survivor.key_id, &challenge(2))
                .is_ok()
        );
        assert!(
            key_manager
                .sign_with_nonce_and_challenge(&signing_key.key_id, &newest.key_id, &challenge(3))
                .is_ok()
        );
    }

    /// The leak this eviction exists for: on a ledger wallet the calls between reserving a nonce and signing with
    /// it are device round trips, and a user rejecting one of them abandons the reservation with no way to release
    /// it. Enough of those and a refusing store would never issue another nonce.
    #[test]
    fn abandoned_reservations_do_not_wedge_the_software_nonce_store() {
        let key_manager = KeyManager::new_random().unwrap();
        let signing_key = key_manager.get_random_key(None, None).unwrap();

        // Reserve and walk away, many times over the bound.
        for _ in 0..MAX_SOFTWARE_EPHEMERAL_NONCES.saturating_mul(2) {
            key_manager.reserve_ephemeral_nonce().unwrap();
        }

        // A caller that does pair its reserve with a sign is still served.
        let reserved = key_manager.reserve_ephemeral_nonce().unwrap();
        assert!(
            key_manager
                .sign_with_nonce_and_challenge(&signing_key.key_id, &reserved.key_id, &challenge(1))
                .is_ok()
        );
    }
}

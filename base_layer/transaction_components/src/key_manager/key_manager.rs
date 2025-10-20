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

use std::str::FromStr;
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
use tari_hashing::{KeyManagerTransactionsHashDomain, WalletMessageSigningDomain};
use blake2::Blake2b;
use chacha20poly1305::{Key, XChaCha20Poly1305};
use rand::{rngs::OsRng, RngCore};
use tari_crypto::commitment::HomomorphicCommitmentFactory;
use tari_crypto::extended_range_proof::ExtendedRangeProofService;
use tari_crypto::hashing::DomainSeparatedHasher;
use tari_crypto::keys::SecretKey;
use tari_utilities::ByteArray;
use minotari_ledger_wallet_comms::accessor_methods::ledger_get_public_key;
use tari_common_types::encryption::encrypt_bytes_integral_nonce;
use tari_common_types::key_branches::TransactionKeyManagerBranch;
use tari_common_types::types::{CompressedCommitment, CompressedPublicKey, PrivateKey};
use crate::crypto_factories::CryptoFactories;
use crate::key_manager::error::KeyManagerError;
use crate::key_manager::wallet_types::WalletType;
use crate::key_manager::interface::TransactionKeyManagerInterface;
use crate::key_manager::key_id::{LedgerKeys, TariKeyAndId, TariKeyId};
use crate::legacy_key_manager::error::KeyManagerServiceError;
use crate::transaction_components::one_sided::{shared_secret_to_output_encryption_key, shared_secret_to_output_spending_key};

pub struct KeyManager {
    crypto_factories: CryptoFactories,
    wallet_type: WalletType,
}

impl KeyManager {
    pub fn new_with_crypto_factories(crypto_factories: CryptoFactories, wallet_type: WalletType) -> Result<Self,KeyManagerError> {
        #[cfg(not(feature = "ledger"))]
        if wallet_type.is_ledger() {
            return Err(KeyManagerError::InvalidWalletType("Trying to use the key manager without ledger features compiled in".to_string()));
        }
        Ok(Self {
            crypto_factories,
            wallet_type,
        })
    }

    pub fn new(wallet_type: WalletType) ->  Result<Self,KeyManagerError>  {
        #[cfg(not(feature = "ledger"))]
        if wallet_type.is_ledger() {
            return Err(KeyManagerError::InvalidWalletType("Trying to use the key manager without ledger features compiled in".to_string()));
        }
        Ok(Self {
            crypto_factories: CryptoFactories::default(),
            wallet_type,
        })
    }

    fn created_encrypted_key(
        &self,
        private_key: PrivateKey,
        encryption_key: TariKeyId,
    ) -> Result<TariKeyId, KeyManagerError> {
        let pvt_bytes = private_key.to_vec();
        let private_encryption_key = self.get_private_key(&encryption_key)?.to_vec();
        let domain = "KEY_MANAGER_private_key".as_bytes().to_vec();
        let cipher = XChaCha20Poly1305::new(Key::from_slice(&private_encryption_key));
        let encrypted_vec = encrypt_bytes_integral_nonce(&cipher, domain, Hidden::hide(pvt_bytes))
            .map_err(|e| KeyManagerError::EncryptionFailed(e.to_string()))?;
        let encrypted = encrypted_vec.as_slice().to_vec();
        Ok(TariKeyId::Encrypted {
            encrypted,
            key: encryption_key.into(),
        })
    }
}
const HASHER_LABEL_STEALTH_KEY: &str = "script key";

impl TransactionKeyManagerInterface for KeyManager {
    fn get_random_key(&self) -> Result<TariKeyAndId, KeyManagerError> {
        match &*self.wallet_type {
            WalletType::Ledger(ledger) => {
                    let random_index = OsRng.next_u64();

                    let branch = TransactionKeyManagerBranch::RandomKey;
                    let public_key = CompressedPublicKey::new_from_pk(
                        ledger_get_public_key(ledger.account, random_index, branch)
                            .map_err(|e| KeyManagerError::LedgerError(e.to_string()))?,
                    );
                    Ok(TariKeyAndId {
                        key_id: TariKeyId::LedgerKey {
                            branch: LedgerKeys::Random,
                            index: random_index,
                        },
                        pub_key: public_key,
                    })
            },
            _ => {
                let random_private_key = PrivateKey::random(&mut OsRng);
                let key_id = self.import_key(random_private_key, None)?;
                let public_key = self.get_public_key_at_key_id(&key_id)?;
                Ok(TariKeyAndId {
                    key_id,
                    pub_key: public_key,
                })
            },
        }
    }

    fn get_public_key_at_key_id(
        &self,
        key_id: &TariKeyId,
    ) -> Result<CompressedPublicKey, KeyManagerError> {
        match key_id {
            TariKeyId::Derived { key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                let public_alpha = self.get_spend_key()?.pub_key;
                let branch_key = self.get_private_key(&key)?;
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
            TariKeyId::DHCommitmentMask {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;

                let shared_secret = self.get_diffie_hellman_shared_secret(&key, public_key)?;
                let commitment_mask_private_key = shared_secret_to_output_spending_key(&shared_secret)?;
                Ok(CompressedPublicKey::from_secret_key(&commitment_mask_private_key))
            },
            TariKeyId::DHEncryptedData {
                public_key,
                private_key,
            } => {
                let key = TariKeyId::from_str(private_key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;

                let shared_secret = self.get_diffie_hellman_shared_secret(&key, public_key)?;
                let encryption_private_key = shared_secret_to_output_encryption_key(&shared_secret)?;
                Ok(CompressedPublicKey::from_secret_key(&encryption_private_key))
            },
            TariKeyId::Encrypted { encrypted, key } => {
                let key = TariKeyId::from_str(key.to_string().as_str())
                    .map_err(|_| KeyManagerServiceError::KeySerializationError)?;
                let private_key = self.decrypt_encrypted_key(encrypted, key)?;
                Ok(CompressedPublicKey::from_secret_key(&private_key))
            },
            TariKeyId::Zero => Ok(CompressedPublicKey::default()),
            TariKeyId::LedgerKey{branch, index}=> {
                if !self.wallet_type.is_ledger(){
                    return Err(KeyManagerError::InvalidWalletType("Trying to access Ledger key on non-Ledger wallet".to_string()));
                }
                let ledger = self.wallet_type.get_ledger_details().expect("already checked is_ledger");
                let public_key = ledger_get_public_key(
                    ledger.account,
                    *index,
                    TransactionKeyManagerBranch::from_key(branch),
                )
                    .map_err(|e| KeyManagerServiceError::LedgerError(e.to_string()))?;
                Ok(CompressedPublicKey::new_from_pk(public_key))
            }
            TariKeyId::SpendKey => {
                Ok(self.wallet_type.get_public_spend_key())
            }
            TariKeyId::ViewKey => {
                Ok(self.wallet_type.get_public_view_key())
            }
        }
    }

    fn import_key(
        &self,
        private_key: PrivateKey,
        encryption_key: Option<TariKeyId>,
    ) -> Result<TariKeyId, KeyManagerError> {
        let encryption_key = match encryption_key {
            Some(key) => key,
            None => self.get_view_key()?.key_id,
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

    fn get_view_key(&self) -> Result<TariKeyAndId, KeyManagerServiceError> {
        let key_id = TariKeyId::ViewKey;
        let key = self.wallet_type.get_public_view_key();
        Ok(TariKeyAndId { key_id, pub_key: key })
    }

}
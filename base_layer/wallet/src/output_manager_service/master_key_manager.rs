// Copyright 2021. The Tari Project
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

use std::fmt::{Display, Error, Formatter};

use futures::lock::Mutex;
use log::*;
use tari_common_types::types::{PrivateKey, PublicKey};
use tari_core::transactions::transaction_protocol::RewindData;
use tari_crypto::{keys::PublicKey as PublicKeyTrait, range_proof::REWIND_USER_MESSAGE_LENGTH};
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager::KeyManager,
    mnemonic::{Mnemonic, MnemonicLanguage},
};

use crate::{
    output_manager_service::{
        error::OutputManagerError,
        handle::PublicRewindKeys,
        storage::database::{KeyManagerState, OutputManagerBackend, OutputManagerDatabase},
    },
    types::KeyDigest,
};

const LOG_TARGET: &str = "wallet::output_manager_service::master_key_manager";
const KEY_MANAGER_MAX_SEARCH_DEPTH: u64 = 1_000_000;

#[derive(Clone, Copy)]
pub enum KeyManagerBranch {
    Spend,
    SpendScript,
    Coinbase,
    CoinbaseScript,
    RecoveryViewOnly,
    RecoveryBlinding,
}

impl Display for KeyManagerBranch {
    fn fmt(&self, fmt: &mut Formatter<'_>) -> Result<(), Error> {
        let response = match self {
            KeyManagerBranch::Spend => "",
            KeyManagerBranch::SpendScript => "script",
            KeyManagerBranch::Coinbase => "coinbase",
            KeyManagerBranch::CoinbaseScript => "coinbase_script",
            KeyManagerBranch::RecoveryViewOnly => "recovery_viewonly",
            KeyManagerBranch::RecoveryBlinding => "recovery_blinding",
        };
        fmt.write_str(response)
    }
}

pub(crate) struct MasterKeyManager<TBackend> {
    utxo_key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
    utxo_script_key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
    coinbase_key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
    coinbase_script_key_manager: Mutex<KeyManager<PrivateKey, KeyDigest>>,
    rewind_data: RewindData,
    db: OutputManagerDatabase<TBackend>,
}

impl<TBackend> MasterKeyManager<TBackend>
where TBackend: OutputManagerBackend + 'static
{
    pub async fn new(master_seed: CipherSeed, db: OutputManagerDatabase<TBackend>) -> Result<Self, OutputManagerError> {
        // Check to see if there is any persisted state. If there is confirm that the provided master secret key matches
        let key_manager_state = match db.get_key_manager_state().await? {
            None => {
                let starting_state = KeyManagerState {
                    seed: master_seed,
                    branch_seed: KeyManagerBranch::Spend.to_string(),
                    primary_key_index: 0,
                };
                db.set_key_manager_state(starting_state.clone()).await?;
                starting_state
            },
            Some(km) => {
                if km.seed != master_seed {
                    return Err(OutputManagerError::MasterSeedMismatch);
                }
                km
            },
        };

        let utxo_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.seed.clone(),
            key_manager_state.branch_seed,
            key_manager_state.primary_key_index,
        );

        let utxo_script_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.seed.clone(),
            KeyManagerBranch::SpendScript.to_string(),
            key_manager_state.primary_key_index,
        );

        let coinbase_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.seed.clone(),
            KeyManagerBranch::Coinbase.to_string(),
            0,
        );

        let coinbase_script_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.seed.clone(),
            KeyManagerBranch::CoinbaseScript.to_string(),
            0,
        );

        let rewind_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.seed.clone(),
            KeyManagerBranch::RecoveryViewOnly.to_string(),
            0,
        );
        let rewind_key = rewind_key_manager.derive_key(0)?.k;

        let rewind_blinding_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
            key_manager_state.seed,
            KeyManagerBranch::RecoveryBlinding.to_string(),
            0,
        );
        let rewind_blinding_key = rewind_blinding_key_manager.derive_key(0)?.k;

        let rewind_data = RewindData {
            rewind_key,
            rewind_blinding_key,
            proof_message: [0u8; REWIND_USER_MESSAGE_LENGTH],
        };

        Ok(Self {
            utxo_key_manager: Mutex::new(utxo_key_manager),
            utxo_script_key_manager: Mutex::new(utxo_script_key_manager),
            coinbase_key_manager: Mutex::new(coinbase_key_manager),
            coinbase_script_key_manager: Mutex::new(coinbase_script_key_manager),
            rewind_data,
            db,
        })
    }

    pub fn rewind_data(&self) -> &RewindData {
        &self.rewind_data
    }

    /// Return the next pair of (spending_key, script_private_key) from the key managers. These will always be generated
    /// in tandem and at corresponding increments
    pub async fn get_next_spend_and_script_key(&self) -> Result<(PrivateKey, PrivateKey), OutputManagerError> {
        let mut km = self.utxo_key_manager.lock().await;
        let key = km.next_key()?;

        let mut skm = self.utxo_script_key_manager.lock().await;
        let script_key = skm.next_key()?;

        self.db.increment_key_index().await?;
        Ok((key.k, script_key.k))
    }

    pub async fn get_script_key_at_index(&self, index: u64) -> Result<PrivateKey, OutputManagerError> {
        let skm = self.utxo_script_key_manager.lock().await;
        let script_key = skm.derive_key(index)?;
        Ok(script_key.k)
    }

    pub async fn get_coinbase_script_key_at_index(&self, index: u64) -> Result<PrivateKey, OutputManagerError> {
        let skm = self.coinbase_script_key_manager.lock().await;
        let script_key = skm.derive_key(index)?;
        Ok(script_key.k)
    }

    pub async fn get_coinbase_spend_and_script_key_for_height(
        &self,
        height: u64,
    ) -> Result<(PrivateKey, PrivateKey), OutputManagerError> {
        let km = self.coinbase_key_manager.lock().await;
        let spending_key = km.derive_key(height)?;

        let mut skm = self.coinbase_script_key_manager.lock().await;
        let script_key = skm.next_key()?;
        Ok((spending_key.k, script_key.k))
    }

    /// Return the Seed words for the current Master Key set in the Key Manager
    pub async fn get_seed_words(&self, language: &MnemonicLanguage) -> Result<Vec<String>, OutputManagerError> {
        let km = self.utxo_key_manager.lock().await;
        let seed_words = (*km).cipher_seed().to_mnemonic(language, None)?;
        Ok(seed_words)
    }

    /// Return the public rewind keys
    pub fn get_rewind_public_keys(&self) -> PublicRewindKeys {
        PublicRewindKeys {
            rewind_public_key: PublicKey::from_secret_key(&self.rewind_data.rewind_key),
            rewind_blinding_public_key: PublicKey::from_secret_key(&self.rewind_data.rewind_blinding_key),
        }
    }

    /// Search the specified branch key manager key chain to find the index of the specified key.
    pub async fn find_key_index(&self, key: PrivateKey, branch: KeyManagerBranch) -> Result<u64, OutputManagerError> {
        let key_manager = match branch {
            KeyManagerBranch::Spend => self.utxo_key_manager.lock().await,
            KeyManagerBranch::Coinbase => self.coinbase_key_manager.lock().await,
            _ => return Err(OutputManagerError::KeyManagerBranchNotSupported),
        };

        let current_index = (*key_manager).key_index();

        for i in 0u64..current_index + KEY_MANAGER_MAX_SEARCH_DEPTH {
            if (*key_manager).derive_key(i)?.k == key {
                trace!(target: LOG_TARGET, "Key found in {} Key Chain at index {}", branch, i);
                return Ok(i);
            }
        }

        Err(OutputManagerError::KeyNotFoundInKeyChain)
    }

    /// If the supplied index is higher than the current UTXO key chain indices then they will be updated.
    pub async fn update_current_spend_key_index_if_higher(&self, index: u64) -> Result<(), OutputManagerError> {
        let mut utxo_key_manager = self.utxo_key_manager.lock().await;
        let mut utxo_script_key_manager = self.utxo_script_key_manager.lock().await;
        let current_index = (*utxo_key_manager).key_index();
        if index > current_index {
            (*utxo_key_manager).update_key_index(index);
            (*utxo_script_key_manager).update_key_index(index);
            self.db.set_key_index(index).await?;
            trace!(target: LOG_TARGET, "Updated UTXO Key Index to {}", index);
        }
        Ok(())
    }
}

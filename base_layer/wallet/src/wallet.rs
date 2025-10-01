// Copyright 2019. The Tari Project
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

use std::{marker::PhantomData, sync::Arc};

use blake2::Blake2b;
use digest::consts::U32;
use log::*;
use rand::rngs::OsRng;
use tari_common::configuration::bootstrap::ApplicationType;
use tari_common_types::{
    seeds::{
        cipher_seed::CipherSeed,
        mnemonic::{Mnemonic, MnemonicLanguage},
        seed_words::SeedWords,
    },
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::{LegacyImportStatus, TxId},
    types::{
        ComAndPubSignature,
        CompressedCommitment,
        CompressedPublicKey,
        PrivateKey,
        RangeProof,
        SignatureWithDomain,
    },
    wallet_types::WalletType,
};
use tari_comms::{types::CommsSecretKey, NodeIdentity};
use tari_crypto::signatures::SchnorrSignatureError;
use tari_hashing::WalletMessageSigningDomain;
use tari_p2p::auto_update::{AutoUpdateConfig, SoftwareUpdaterHandle, SoftwareUpdaterService};
use tari_script::{push_pubkey_script, ExecutionStack, TariScript};
use tari_service_framework::StackBuilder;
use tari_shutdown::ShutdownSignal;
use tari_transaction_components::{
    consensus::{ConsensusManager, NetworkConsensus},
    crypto_factories::CryptoFactories,
    key_manager::{
        error::KeyManagerServiceError,
        tari_key_manager::TariKeyManager,
        KeyDigest,
        KeyManagerBranch,
        SecretTransactionKeyManagerInterface,
        TariKeyId,
        TransactionKeyManagerBackend,
        TransactionKeyManagerInitializer,
        TransactionKeyManagerInterface,
    },
    transaction_components::{
        covenants::Covenant,
        memo_field::{MemoField, TxType},
        EncryptedData,
        OutputFeatures,
        UnblindedOutput,
    },
    MicroMinotari,
};
use tari_utilities::{hex::Hex, ByteArray};
use url::Url;

use crate::{
    base_node_service::{handle::BaseNodeServiceHandle, BaseNodeServiceInitializer},
    client::http_client_factory::{DefaultHttpClientFactory, HttpClientFactory},
    config::WalletConfig,
    connectivity_service::{WalletConnectivityHandle, WalletConnectivityInitializer},
    consts,
    error::{WalletError, WalletStorageError},
    output_manager_service::{
        error::OutputManagerError,
        handle::OutputManagerHandle,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::KnownOneSidedPaymentScript,
        },
        OutputManagerServiceInitializer,
    },
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        handle::TransactionServiceHandle,
        storage::database::TransactionBackend,
        TransactionServiceInitializer,
    },
    util::wallet_identity::WalletIdentity,
    utxo_scanner_service::{handle::UtxoScannerHandle, initializer::UtxoScannerServiceInitializer, RECOVERY_KEY},
};

const LOG_TARGET: &str = "wallet";

/// A structure containing the config and services that a Wallet application will require. This struct will start up all
/// the services and provide the APIs that applications will use to interact with the services
#[derive(Clone)]
pub struct Wallet<T, U, V, TKeyManagerInterface, THttpClientFactory>
where THttpClientFactory: HttpClientFactory
{
    pub network: NetworkConsensus,
    pub output_manager_service: OutputManagerHandle<TKeyManagerInterface>,
    pub key_manager_service: TKeyManagerInterface,
    pub transaction_service: TransactionServiceHandle,
    pub wallet_connectivity: WalletConnectivityHandle<THttpClientFactory>,
    pub base_node_service: BaseNodeServiceHandle,
    pub utxo_scanner_service: UtxoScannerHandle,
    pub updater_service: Option<SoftwareUpdaterHandle>,
    pub db: WalletDatabase<T>,
    pub output_db: OutputManagerDatabase<V>,
    pub factories: CryptoFactories,
    wallet_type: Arc<WalletType>,
    pub config: WalletConfig,
    pub shutdown_signal: ShutdownSignal,
    pub node_identity: Arc<NodeIdentity>,
    _u: PhantomData<U>,
    _v: PhantomData<V>,
}

impl<T, U, V, TKeyManagerInterface, THttpClientFactory> Wallet<T, U, V, TKeyManagerInterface, THttpClientFactory>
where
    T: WalletBackend + 'static,
    U: TransactionBackend + 'static,
    V: OutputManagerBackend + 'static,
    TKeyManagerInterface: SecretTransactionKeyManagerInterface,
    THttpClientFactory: HttpClientFactory,
{
    #[allow(clippy::too_many_lines)]
    pub async fn start<TKeyManagerBackend: TransactionKeyManagerBackend + 'static>(
        config: WalletConfig,
        auto_update: AutoUpdateConfig,
        node_identity: Arc<NodeIdentity>,
        consensus_manager: ConsensusManager,
        factories: CryptoFactories,
        wallet_database: WalletDatabase<T>,
        output_manager_database: OutputManagerDatabase<V>,
        transaction_backend: U,
        output_manager_backend: V,
        key_manager_backend: TKeyManagerBackend,
        shutdown_signal: ShutdownSignal,
        master_seed: Option<CipherSeed>,
        wallet_type: Option<WalletType>,
    ) -> Result<Self, WalletError> {
        let wallet_type = Arc::new(read_or_create_wallet_type(wallet_type, &wallet_database)?);

        debug!(target: LOG_TARGET, "Wallet Initializing");
        info!(
            target: LOG_TARGET,
            "Transaction sending mechanism is {}", config.transaction_service_config.transaction_routing_mechanism
        );
        trace!(target: LOG_TARGET, "Wallet config: {config:?}");
        let stack = StackBuilder::new(shutdown_signal.clone())
            .add_initializer(OutputManagerServiceInitializer::<
                V,
                TKeyManagerInterface,
                THttpClientFactory,
            >::new(
                config.output_manager_service_config.clone(),
                output_manager_backend.clone(),
                factories.clone(),
                config.network.into(),
            ))
            .add_initializer(TransactionKeyManagerInitializer::new_with_legacy_storage(
                key_manager_backend,
                master_seed,
                factories.clone(),
                wallet_type.clone(),
            ))
            .add_initializer(TransactionServiceInitializer::<
                U,
                T,
                TKeyManagerInterface,
                THttpClientFactory,
            >::new(
                config.transaction_service_config.clone(),
                transaction_backend,
                node_identity.clone(),
                config.network,
                consensus_manager,
                factories.clone(),
                wallet_database.clone(),
                wallet_type.clone(),
            ))
            .add_initializer(BaseNodeServiceInitializer::<THttpClientFactory>::new())
            .add_initializer(WalletConnectivityInitializer::<DefaultHttpClientFactory>::new(
                config
                    .http_server_url
                    .parse()
                    .map_err(|e| WalletError::InvalidHttpNodeUrl(format!("base node URL is invalid:{e}")))?,
                config
                    .fallback_http_server_url
                    .parse()
                    .map_err(|e| WalletError::InvalidHttpNodeUrl(format!("fallback seed URL is invalid:{e}")))?,
            ))
            .add_initializer(UtxoScannerServiceInitializer::<T, TKeyManagerInterface>::new(
                wallet_database.clone(),
                config.network,
                config.birthday_offset,
                Url::parse(&config.http_server_url)
                    .map_err(|e| WalletError::InvalidHttpNodeUrl(format!("base node URL is invalid:{e}")))?,
                Url::parse(&config.fallback_http_server_url)
                    .map_err(|e| WalletError::InvalidHttpNodeUrl(format!("fallback seed URL is invalid:{e}")))?,
                config.scanning_interval,
            ));

        // Check if we have update config. FFI wallets don't do this, the update on mobile is done differently.
        let stack = if auto_update.is_update_enabled() {
            stack.add_initializer(SoftwareUpdaterService::new(
                ApplicationType::ConsoleWallet,
                env!("CARGO_PKG_VERSION")
                    .to_string()
                    .parse()
                    .expect("Unable to parse console wallet version."),
                auto_update.clone(),
            ))
        } else {
            stack
        };

        let handles = stack.build().await?;

        let transaction_service_handle = handles.expect_handle::<TransactionServiceHandle>();

        let mut output_manager_handle = handles.expect_handle::<OutputManagerHandle<TKeyManagerInterface>>();
        let key_manager_handle = handles.expect_handle::<TKeyManagerInterface>();

        let base_node_service_handle = handles.expect_handle::<BaseNodeServiceHandle>();
        let utxo_scanner_service_handle = handles.expect_handle::<UtxoScannerHandle>();
        let wallet_connectivity = handles.expect_handle::<WalletConnectivityHandle<THttpClientFactory>>();
        let updater_handle = if auto_update.is_update_enabled() {
            Some(handles.expect_handle::<SoftwareUpdaterHandle>())
        } else {
            None
        };
        let spend_key = key_manager_handle.get_spend_key().await?;

        persist_one_sided_payment_script_for_node_identity(
            &mut output_manager_handle,
            &spend_key.pub_key,
            spend_key.key_id,
        )
        .await
        .inspect_err(|e| {
            error!(target: LOG_TARGET, "{e:?}");
        })?;

        // storing current network and version
        if let Err(e) = wallet_database
            .set_last_network_and_version(config.network.to_string(), consts::APP_VERSION_NUMBER.to_string())
        {
            warn!("failed to store network and version: {e:#?}");
        }

        Ok(Self {
            network: config.network.into(),
            output_manager_service: output_manager_handle,
            key_manager_service: key_manager_handle,
            transaction_service: transaction_service_handle,
            base_node_service: base_node_service_handle,
            utxo_scanner_service: utxo_scanner_service_handle,
            updater_service: updater_handle,
            wallet_connectivity,
            db: wallet_database,
            output_db: output_manager_database,
            factories,
            wallet_type,
            config,
            shutdown_signal: shutdown_signal.clone(),
            node_identity: node_identity.clone(),
            _u: PhantomData,
            _v: PhantomData,
        })
    }

    /// This method consumes the wallet so that the handles are dropped which will result in the services async loops
    /// exiting.
    pub async fn wait_until_shutdown(self) {
        self.shutdown_signal.await;
    }

    pub async fn check_for_update(&self) -> Option<String> {
        let mut updater = self.updater_service.clone().unwrap();
        debug!(
            target: LOG_TARGET,
            "Checking for updates (current version: {})...",
            env!("CARGO_PKG_VERSION")
        );
        match updater.check_for_updates().await {
            Some(update) => {
                debug!(
                    target: LOG_TARGET,
                    "Version {} of the {} is available: {} (sha: {})",
                    update.version(),
                    update.app(),
                    update.download_url(),
                    update.to_hash_hex()
                );
                Some(format!(
                    "Version {} of the {} is available: {} (sha: {})",
                    update.version(),
                    update.app(),
                    update.download_url(),
                    update.to_hash_hex()
                ))
            },
            None => {
                debug!(target: LOG_TARGET, "No updates found.",);
                None
            },
        }
    }

    pub async fn get_wallet_interactive_address(&self) -> Result<TariAddress, KeyManagerServiceError> {
        let view_key = self.key_manager_service.get_view_key().await?;
        let comms_key = self.key_manager_service.get_comms_key().await?;
        let features = match *self.wallet_type {
            WalletType::DerivedKeys => TariAddressFeatures::default(),
            WalletType::Ledger(_) | WalletType::ProvidedKeys(_) => TariAddressFeatures::create_interactive_only(),
        };
        Ok(TariAddress::new_dual_address(
            view_key.pub_key,
            comms_key.pub_key,
            self.network.as_network(),
            features,
            None,
        )?)
    }

    pub async fn get_wallet_one_sided_address(&self) -> Result<TariAddress, KeyManagerServiceError> {
        let view_key = self.key_manager_service.get_view_key().await?;
        let spend_key = self.key_manager_service.get_spend_key().await?;
        Ok(TariAddress::new_dual_address(
            view_key.pub_key,
            spend_key.pub_key,
            self.network.as_network(),
            TariAddressFeatures::create_one_sided_only(),
            None,
        )?)
    }

    pub async fn get_wallet_id(&self) -> Result<WalletIdentity, WalletError> {
        let address_interactive = self.get_wallet_interactive_address().await?;
        let address_one_sided = self.get_wallet_one_sided_address().await?;
        Ok(WalletIdentity::new(
            self.node_identity.clone(),
            address_interactive,
            address_one_sided,
        ))
    }

    pub fn get_software_updater(&self) -> Option<SoftwareUpdaterHandle> {
        self.updater_service.as_ref().cloned()
    }

    /// Import an external spendable UTXO into the wallet as a non-rewindable/non-recoverable UTXO. The output will be
    /// added to the Output Manager and made EncumberedToBeReceived. A faux incoming transaction will be created to
    /// provide a record of the event. The TxId of the generated transaction is returned.
    pub async fn import_external_utxo_as_non_rewindable(
        &mut self,
        amount: MicroMinotari,
        spending_key: &PrivateKey,
        script: TariScript,
        input_data: ExecutionStack,
        source_address: TariAddress,
        features: OutputFeatures,
        metadata_signature: ComAndPubSignature,
        script_private_key: &PrivateKey,
        sender_offset_public_key: &CompressedPublicKey,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<RangeProof>,
        payment_id: MemoField,
    ) -> Result<TxId, WalletError> {
        let unblinded_output = UnblindedOutput::new_current_version(
            amount,
            spending_key.clone(),
            features.clone(),
            script,
            input_data,
            script_private_key.clone(),
            sender_offset_public_key.clone(),
            metadata_signature,
            script_lock_height,
            covenant,
            encrypted_data,
            minimum_value_promise,
            range_proof,
        );
        self.import_unblinded_output_as_non_rewindable(unblinded_output, source_address, payment_id)
            .await
    }

    /// Import an external spendable UTXO into the wallet as a non-rewindable/non-recoverable UTXO. The output will be
    /// added to the Output Manager and made spendable. A faux incoming transaction will be created to provide a record
    /// of the event. The TxId of the generated transaction is returned.
    pub async fn import_unblinded_output_as_non_rewindable(
        &mut self,
        unblinded_output: UnblindedOutput,
        source_address: TariAddress,
        payment_id: MemoField,
    ) -> Result<TxId, WalletError> {
        let value = unblinded_output.value;
        let wallet_output = unblinded_output
            .to_wallet_output(&self.key_manager_service, MemoField::new_empty())
            .await?;

        let tx_id = self
            .transaction_service
            .import_utxo_with_status(
                value,
                source_address,
                LegacyImportStatus::Imported,
                None,
                None,
                None,
                wallet_output.to_transaction_output()?,
                payment_id,
            )
            .await?;
        // As non-rewindable
        self.output_manager_service
            .add_unvalidated_output(tx_id, wallet_output.clone(), None)
            .await?;
        info!(
            target: LOG_TARGET,
            "UTXO (Commitment: {}, value: {}, txID: {}) imported into wallet as 'ImportStatus::Imported' and is non-rewindable",
            wallet_output.commitment().to_hex(),
            wallet_output.value(),
            tx_id,
        );

        Ok(tx_id)
    }

    pub fn sign_message(
        &mut self,
        secret: &PrivateKey,
        message: &str,
    ) -> Result<SignatureWithDomain<WalletMessageSigningDomain>, SchnorrSignatureError> {
        SignatureWithDomain::<WalletMessageSigningDomain>::sign(secret, message.as_bytes(), &mut OsRng)
    }

    pub fn verify_message_signature(
        &mut self,
        public_key: &CompressedPublicKey,
        signature: &SignatureWithDomain<WalletMessageSigningDomain>,
        message: &str,
    ) -> bool {
        if let Ok(key) = public_key.clone().to_public_key() {
            signature.verify(&key, message.as_bytes())
        } else {
            false
        }
    }

    /// Appraise the expected outputs and a fee
    pub async fn preview_coin_split_with_commitments_no_amount(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        split_count: usize,
        fee_per_gram: MicroMinotari,
    ) -> Result<(Vec<MicroMinotari>, MicroMinotari), WalletError> {
        self.output_manager_service
            .preview_coin_split_with_commitments_no_amount(commitments, split_count, fee_per_gram)
            .await
            .map_err(WalletError::OutputManagerError)
    }

    /// Appraise the expected outputs and a fee
    pub async fn preview_coin_join_with_commitments(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        fee_per_gram: MicroMinotari,
    ) -> Result<(Vec<MicroMinotari>, MicroMinotari), WalletError> {
        self.output_manager_service
            .preview_coin_join_with_commitments(commitments, fee_per_gram)
            .await
            .map_err(WalletError::OutputManagerError)
    }

    /// Do a coin split
    pub async fn coin_split(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        amount_per_split: MicroMinotari,
        split_count: usize,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
    ) -> Result<TxId, WalletError> {
        let coin_split_tx = self
            .output_manager_service
            .create_coin_split(commitments, amount_per_split, split_count, fee_per_gram)
            .await;

        match coin_split_tx {
            Ok((tx_id, split_tx, amount)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, split_tx, amount, payment_id)
                    .await;
                match coin_tx {
                    Ok(_) => Ok(tx_id),
                    Err(e) => Err(WalletError::TransactionServiceError(e)),
                }
            },
            Err(e) => Err(WalletError::OutputManagerError(e)),
        }
    }

    /// Do a coin split
    pub async fn coin_split_even(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        split_count: usize,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
    ) -> Result<TxId, WalletError> {
        let coin_split_tx = self
            .output_manager_service
            .create_coin_split_even(commitments, split_count, fee_per_gram)
            .await;

        match coin_split_tx {
            Ok((tx_id, split_tx, amount)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, split_tx, amount, payment_id)
                    .await;
                match coin_tx {
                    Ok(_) => Ok(tx_id),
                    Err(e) => Err(WalletError::TransactionServiceError(e)),
                }
            },
            Err(e) => Err(WalletError::OutputManagerError(e)),
        }
    }

    /// Do a coin split
    pub async fn coin_split_even_with_commitments(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        split_count: usize,
        fee_per_gram: MicroMinotari,
        payment_id: MemoField,
    ) -> Result<TxId, WalletError> {
        let coin_split_tx = self
            .output_manager_service
            .create_coin_split_even(commitments, split_count, fee_per_gram)
            .await;

        match coin_split_tx {
            Ok((tx_id, split_tx, amount)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, split_tx, amount, payment_id)
                    .await;
                match coin_tx {
                    Ok(_) => Ok(tx_id),
                    Err(e) => Err(WalletError::TransactionServiceError(e)),
                }
            },
            Err(e) => Err(WalletError::OutputManagerError(e)),
        }
    }

    pub async fn coin_join(
        &mut self,
        commitments: Vec<CompressedCommitment>,
        fee_per_gram: MicroMinotari,
        payment_id: Option<MemoField>,
    ) -> Result<TxId, WalletError> {
        let payment_id = payment_id.unwrap_or(MemoField::open_from_string(
            &format!("Coin join {} outputs", commitments.len()),
            TxType::CoinJoin,
        ));
        let coin_join_tx = self
            .output_manager_service
            .create_coin_join(commitments, fee_per_gram, payment_id.clone())
            .await;

        match coin_join_tx {
            Ok((tx_id, tx, output_value)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, tx, output_value, payment_id)
                    .await;

                match coin_tx {
                    Ok(_) => Ok(tx_id),
                    Err(e) => Err(WalletError::TransactionServiceError(e)),
                }
            },
            Err(e) => Err(WalletError::OutputManagerError(e)),
        }
    }

    /// Utility function to find out if there is data in the database indicating that there is an incomplete recovery
    /// process in progress
    pub fn is_recovery_in_progress(&self) -> Result<bool, WalletError> {
        Ok(self.db.get_client_key_value(RECOVERY_KEY.to_string())?.is_some())
    }

    pub fn get_seed_words(&self, language: &MnemonicLanguage) -> Result<SeedWords, WalletError> {
        let master_seed = self.db.get_master_seed()?.ok_or_else(|| {
            WalletError::WalletStorageError(WalletStorageError::RecoverySeedError(
                "Cipher Seed not found".to_string(),
            ))
        })?;

        let seed_words = master_seed.to_mnemonic(*language, None)?;
        Ok(seed_words)
    }
}

pub fn read_or_create_master_seed<T: WalletBackend + 'static>(
    recovery_seed: Option<CipherSeed>,
    db: &WalletDatabase<T>,
) -> Result<CipherSeed, WalletError> {
    let db_master_seed = db.get_master_seed()?;

    let master_seed = match recovery_seed {
        None => match db_master_seed {
            None => {
                let seed = CipherSeed::random();
                db.set_master_seed(seed.clone())?;
                seed
            },
            Some(seed) => seed,
        },
        Some(recovery_seed) => {
            if db_master_seed.is_none() {
                db.set_master_seed(recovery_seed.clone())?;
                recovery_seed
            } else {
                error!(
                    target: LOG_TARGET,
                    "Attempted recovery would overwrite the existing wallet database master seed"
                );
                let msg = "Wallet already exists! Move the existing wallet database file.".to_string();
                return Err(WalletError::WalletRecoveryError(msg));
            }
        },
    };

    Ok(master_seed)
}

pub fn read_or_create_wallet_type<T: WalletBackend + 'static>(
    wallet_type: Option<WalletType>,
    db: &WalletDatabase<T>,
) -> Result<WalletType, WalletError> {
    let db_wallet_type = db.get_wallet_type()?;

    match (db_wallet_type, wallet_type) {
        (None, None) => {
            // this is most likely an older wallet pre ledger support, lets put it in software
            let wallet_type = WalletType::default();
            db.set_wallet_type(wallet_type.clone())?;
            Ok(wallet_type)
        },
        (None, Some(t)) => {
            db.set_wallet_type(t.clone())?;
            Ok(t.clone())
        },
        (Some(t), _) => Ok(t),
    }
}

pub fn derive_comms_secret_key(master_seed: &CipherSeed) -> Result<CommsSecretKey, WalletError> {
    let comms_key_manager =
        TariKeyManager::<KeyDigest>::from(master_seed.clone(), KeyManagerBranch::Comms.get_branch_key(), 0);
    Ok(comms_key_manager.derive_key(0)?.key)
}

/// Persist the one-sided payment script for the current wallet NodeIdentity for use during scanning for One-sided
/// payment outputs. This is peristed so that if the Node Identity changes the wallet will still scan for outputs
/// using old node identities.
async fn persist_one_sided_payment_script_for_node_identity<KM: TransactionKeyManagerInterface>(
    output_manager_service: &mut OutputManagerHandle<KM>,
    spend_key: &CompressedPublicKey,
    spend_key_id: TariKeyId,
) -> Result<(), WalletError> {
    let script = push_pubkey_script(spend_key);
    let known_script = KnownOneSidedPaymentScript {
        script_hash: script
            .as_hash::<Blake2b<U32>>()
            .map_err(|e| WalletError::OutputManagerError(OutputManagerError::ScriptError(e)))?
            .to_vec(),
        script_key_id: spend_key_id,
        script,
        input: ExecutionStack::default(),
        script_lock_height: 0,
    };

    output_manager_service.add_known_script(known_script).await?;
    Ok(())
}

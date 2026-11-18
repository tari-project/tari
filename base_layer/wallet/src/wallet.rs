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

use std::{marker::PhantomData, sync::Arc, time::Duration};

use blake2::Blake2b;
use digest::consts::U32;
use log::*;
use tari_common::configuration::bootstrap::ApplicationType;
use tari_common_types::{
    seeds::{
        cipher_seed::CipherSeed,
        mnemonic::{Mnemonic, MnemonicLanguage},
        seed_words::SeedWords,
    },
    tari_address::{TariAddress, TariAddressError, TariAddressFeatures},
    transaction::{LegacyImportStatus, TxId},
    types::{
        ComAndPubSignature,
        CompressedCommitment,
        CompressedPublicKey,
        PrivateKey,
        RangeProof,
        SignatureWithDomain,
    },
};
use tari_comms::{NodeIdentity, types::CommsSecretKey};
use tari_crypto::signatures::SchnorrSignatureError;
use tari_hashing::WalletMessageSigningDomain;
use tari_p2p::auto_update::{AutoUpdateConfig, SoftwareUpdaterHandle, SoftwareUpdaterService};
use tari_script::{ExecutionStack, TariScript, push_pubkey_script};
use tari_service_framework::StackBuilder;
use tari_shutdown::ShutdownSignal;
use tari_transaction_components::{
    MicroMinotari,
    consensus::{ConsensusManager, NetworkConsensus},
    crypto_factories::CryptoFactories,
    key_manager::{
        KeyManager,
        RecoveryProvenance,
        SecretTransactionKeyManagerInterface,
        TariKeyId,
        wallet_types::KeyDigest,
    },
    transaction_components::{
        EncryptedData,
        OutputFeatures,
        UnblindedOutput,
        covenants::Covenant,
        memo_field::{MemoField, TxType},
    },
};
use tari_transaction_key_manager::legacy_key_manager::{
    KeyManagerBranch,
    LegacyTransactionKeyManagerInitializer,
    LegacyTransactionKeyManagerInterface,
    TransactionKeyManagerBackend,
    wallet_types::LegacyWalletType,
};
use tari_utilities::{ByteArray, hex::Hex};
use url::Url;
use zeroize::Zeroizing;

use crate::{
    base_node_service::{BaseNodeServiceInitializer, handle::BaseNodeServiceHandle},
    client::http_client_factory::{DefaultHttpClientFactory, HttpClientFactory},
    config::WalletConfig,
    connectivity_service::{WalletConnectivityHandle, WalletConnectivityInitializer},
    consts,
    error::{WalletError, WalletStorageError},
    output_manager_service::{
        OutputManagerServiceInitializer,
        error::OutputManagerError,
        handle::OutputManagerHandle,
        storage::{
            database::{OutputManagerBackend, OutputManagerDatabase},
            models::KnownOneSidedPaymentScript,
        },
    },
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        TransactionServiceInitializer,
        handle::TransactionServiceHandle,
        storage::database::{TransactionBackend, TransactionDatabase},
    },
    util::wallet_identity::WalletIdentity,
    utxo_scanner_service::{RECOVERY_KEY, handle::UtxoScannerHandle, initializer::UtxoScannerServiceInitializer},
};

const LOG_TARGET: &str = "wallet";
/// Client key under which the imported offline-signing payload key is stored, as `<index>:<private key hex>`. Client
/// values are encrypted at rest with the wallet database cipher.
const PAYLOAD_AUTH_KEY: &str = "payload_auth_key";

/// Stores offline-signing payload key `index` in the wallet database, replacing any stored before, and gives it to
/// `key_manager`.
fn store_payload_auth_key<T: WalletBackend + 'static>(
    db: &WalletDatabase<T>,
    key_manager: &KeyManager,
    index: u64,
    private_key: PrivateKey,
) -> Result<(), WalletError> {
    let hex = Zeroizing::new(private_key.to_hex());
    // shortcut: the value is handed to the database by value and is not zeroized after it is encrypted there; wiping
    // it needs a client-value API that takes a zeroizing buffer.
    db.set_client_key_value(PAYLOAD_AUTH_KEY.to_string(), format!("{index}:{}", *hex))?;
    info!(
        target: LOG_TARGET,
        "Imported offline-signing payload key #{index} (public key {})",
        CompressedPublicKey::from_secret_key(&private_key).to_hex()
    );
    key_manager.set_payload_auth_key(Some((index, private_key)));
    Ok(())
}

/// Removes the stored payload key from the database and `key_manager`. Returns whether one was stored.
fn clear_payload_auth_key<T: WalletBackend + 'static>(
    db: &WalletDatabase<T>,
    key_manager: &KeyManager,
) -> Result<bool, WalletError> {
    let removed = read_stored_payload_auth_key(db)?;
    let cleared = db.clear_client_value(PAYLOAD_AUTH_KEY.to_string())?;
    key_manager.set_payload_auth_key(None);
    match (cleared, removed) {
        (true, Some(Some((index, key)))) => info!(
            target: LOG_TARGET,
            "Cleared offline-signing payload key #{index} (public key {})",
            CompressedPublicKey::from_secret_key(&key).to_hex()
        ),
        (true, _) => info!(target: LOG_TARGET, "Cleared a malformed stored offline-signing payload key"),
        (false, _) => info!(target: LOG_TARGET, "No offline-signing payload key was stored to clear"),
    }
    Ok(cleared)
}

/// The stored payload key: `None` if none is stored, `Some(None)` if the stored value is malformed.
fn read_stored_payload_auth_key<T: WalletBackend + 'static>(
    db: &WalletDatabase<T>,
) -> Result<Option<Option<(u64, PrivateKey)>>, WalletError> {
    let Some(stored) = db
        .get_client_key_value(PAYLOAD_AUTH_KEY.to_string())?
        .map(Zeroizing::new)
    else {
        return Ok(None);
    };
    Ok(Some(stored.split_once(':').and_then(|(index, key)| {
        Some((index.parse().ok()?, PrivateKey::from_hex(key).ok()?))
    })))
}

/// Gives the stored payload key, if any, to `key_manager`. A malformed value is logged and ignored rather than
/// stopping the wallet, which could then not start to clear or replace it.
fn load_payload_auth_key<T: WalletBackend + 'static>(
    db: &WalletDatabase<T>,
    key_manager: &KeyManager,
) -> Result<(), WalletError> {
    match read_stored_payload_auth_key(db)? {
        None => {},
        Some(Some(payload_key)) => key_manager.set_payload_auth_key(Some(payload_key)),
        Some(None) => warn!(
            target: LOG_TARGET,
            "The stored offline-signing payload key is malformed and is ignored; payloads will be signed with the \
             view key. Import the payload key again."
        ),
    }
    Ok(())
}
/// How long [`Wallet::wait_until_shutdown`] waits for wallet tasks to exit after the shutdown signal fires.
pub const WALLET_SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

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
    wallet_type: Arc<LegacyWalletType>,
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
    TKeyManagerInterface: SecretTransactionKeyManagerInterface + LegacyTransactionKeyManagerInterface,
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
        master_seed: CipherSeed,
        wallet_type: Option<LegacyWalletType>,
    ) -> Result<Self, WalletError> {
        let wallet_type = Arc::new(read_or_create_wallet_type(wallet_type, &wallet_database)?);

        debug!(target: LOG_TARGET, "Wallet Initializing");
        info!(
            target: LOG_TARGET,
            "Transaction sending mechanism is {}", config.transaction_service_config.transaction_routing_mechanism
        );
        trace!(target: LOG_TARGET, "Wallet config: {config:?}");
        let stack = StackBuilder::new(shutdown_signal.clone())
            .add_initializer(
                OutputManagerServiceInitializer::<V, TKeyManagerInterface, THttpClientFactory>::new(
                    config.output_manager_service_config.clone(),
                    output_manager_backend.clone(),
                    factories.clone(),
                    config.network.into(),
                )
                .with_migration_flag_store(Arc::new(wallet_database.clone()))
                .with_invalid_output_tx_sink(Arc::new(TransactionDatabase::new(transaction_backend.clone()))),
            )
            .add_initializer(LegacyTransactionKeyManagerInitializer::new_with_legacy_storage(
                key_manager_backend,
                master_seed,
                factories.clone(),
                wallet_type.clone(),
            ))
            .add_initializer(TransactionServiceInitializer::<
                U,
                TKeyManagerInterface,
                THttpClientFactory,
            >::new(
                config.transaction_service_config.clone(),
                transaction_backend,
                node_identity.clone(),
                config.network,
                consensus_manager,
                factories.clone(),
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
        load_payload_auth_key(&wallet_database, key_manager_handle.key_manager())?;

        let spend_key = key_manager_handle.get_spend_key();

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

    /// Wait for the wallet's shutdown signal, then wait (bounded by [`WALLET_SHUTDOWN_DRAIN_TIMEOUT`]) for every task
    /// holding a signal for the same shutdown to exit.
    ///
    /// This method consumes the wallet so that its handles are dropped, which lets the services' async loops exit.
    pub async fn wait_until_shutdown(self) {
        let mut shutdown_signal = self.shutdown_signal.clone();
        (&mut shutdown_signal).await;
        debug!(
            target: LOG_TARGET,
            "Wallet shutdown signal received ({:?}); waiting for wallet tasks to exit",
            shutdown_signal.reason()
        );
        // Drop our own handles (and the signals they hold) before draining, or the drain would wait on itself
        drop(self);
        if tokio::time::timeout(WALLET_SHUTDOWN_DRAIN_TIMEOUT, shutdown_signal.drained())
            .await
            .is_err()
        {
            warn!(
                target: LOG_TARGET,
                "Timed out after {:.0?} waiting for wallet tasks to exit", WALLET_SHUTDOWN_DRAIN_TIMEOUT
            );
        }
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

    pub fn get_wallet_interactive_address(&self) -> Result<TariAddress, TariAddressError> {
        let view_key = self.key_manager_service.get_view_key();
        let comms_key = self.key_manager_service.get_spend_key();
        let features = match *self.wallet_type {
            LegacyWalletType::DerivedKeys => TariAddressFeatures::default(),
            LegacyWalletType::Ledger(_) | LegacyWalletType::ProvidedKeys(_) => {
                TariAddressFeatures::create_interactive_only()
            },
        };
        TariAddress::new_dual_address(
            view_key.pub_key,
            comms_key.pub_key,
            self.network.as_network(),
            features,
            None,
        )
    }

    pub fn get_wallet_one_sided_address(&self) -> Result<TariAddress, TariAddressError> {
        let view_key = self.key_manager_service.get_view_key();
        let spend_key = self.key_manager_service.get_spend_key();
        TariAddress::new_dual_address(
            view_key.pub_key,
            spend_key.pub_key,
            self.network.as_network(),
            TariAddressFeatures::create_one_sided_only(),
            None,
        )
    }

    pub fn get_wallet_id(&self) -> Result<WalletIdentity, WalletError> {
        let address_interactive = self.get_wallet_interactive_address()?;
        let address_one_sided = self.get_wallet_one_sided_address()?;
        Ok(WalletIdentity::new(
            self.node_identity.clone(),
            address_interactive,
            address_one_sided,
        ))
    }

    /// Imports offline-signing payload key `index` (exported by the offline signer), replacing any imported before.
    /// It is stored in the wallet database, and the `prepare_*` offline-signing calls sign with it from then on.
    pub fn import_payload_auth_key(&self, index: u64, private_key: PrivateKey) -> Result<(), WalletError> {
        store_payload_auth_key(&self.db, self.key_manager_service.key_manager(), index, private_key)
    }

    /// Removes the imported payload key, so payloads are signed with the view key again. Returns whether one was set.
    pub fn clear_payload_auth_key(&self) -> Result<bool, WalletError> {
        clear_payload_auth_key(&self.db, self.key_manager_service.key_manager())
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
        let wallet_output = unblinded_output.to_wallet_output(&self.key_manager_service, MemoField::new_empty())?;

        let tx_id = self
            .transaction_service
            .import_utxo_with_status(
                value,
                source_address,
                LegacyImportStatus::Imported,
                None,
                None,
                wallet_output.to_transaction_output()?,
                payment_id,
                None,
                wallet_output.max_lock_height(),
                // The memo and output come from outside this wallet, so they are treated like a received output.
                RecoveryProvenance::DiffieHellman,
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
        SignatureWithDomain::<WalletMessageSigningDomain>::sign(secret, message.as_bytes(), &mut rand::rng())
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
        let payment_id = payment_id.unwrap_or(
            MemoField::new_open_from_string(&format!("Coin join {} outputs", commitments.len()), TxType::CoinJoin)
                .map_err(OutputManagerError::InvalidPaymentIdFormat)?,
        );
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
    wallet_type: Option<LegacyWalletType>,
    db: &WalletDatabase<T>,
) -> Result<LegacyWalletType, WalletError> {
    let db_wallet_type = db.get_wallet_type()?;
    // An existing wallet always has its wallet type stored; a missing one means the row was removed
    if db_wallet_type.is_none() && !db.wallet_type_init_allowed() {
        return Err(WalletError::WalletStorageError(WalletStorageError::UnexpectedResult(
            "The wallet type is missing from the wallet database. If this wallet's setup was interrupted before it \
             completed, the wallet was never fully created: delete the wallet database file and create the wallet \
             again, or restore it from its seed words. If setup did complete, the database file has been modified."
                .to_string(),
        )));
    }

    match (db_wallet_type, wallet_type) {
        (None, None) => {
            // this is most likely an older wallet pre ledger support, lets put it in software
            let wallet_type = LegacyWalletType::default();
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
    use tari_transaction_key_manager::legacy_key_manager::tari_key_manager::TariKeyManager;
    let comms_key_manager =
        TariKeyManager::<KeyDigest>::from(master_seed.clone(), KeyManagerBranch::Comms.get_branch_key(), 0);
    Ok(comms_key_manager.derive_key(0)?.key)
}

/// Persist the one-sided payment script for the current wallet NodeIdentity for use during scanning for One-sided
/// payment outputs. This is peristed so that if the Node Identity changes the wallet will still scan for outputs
/// using old node identities.
async fn persist_one_sided_payment_script_for_node_identity<KM: LegacyTransactionKeyManagerInterface>(
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

#[cfg(test)]
mod test {
    use tari_test_utils::random::string;
    use tari_transaction_components::key_manager::{
        TransactionKeyManagerInterface,
        wallet_types::{ViewWallet, WalletType},
    };
    use tempfile::tempdir;

    use super::*;
    use crate::storage::{
        sqlite_db::wallet::WalletSqliteDatabase,
        sqlite_utilities::run_migration_and_create_sqlite_connection,
    };

    #[test]
    fn payload_auth_key_is_stored_loaded_and_cleared() {
        let temp_dir = tempdir().unwrap();
        let path = temp_dir.path().join(format!("{}.sqlite3", string(8)));
        let connection = run_migration_and_create_sqlite_connection(path.to_str().unwrap(), 16).unwrap();
        let db = WalletDatabase::new(WalletSqliteDatabase::new(connection, "passphrase".to_string().into()).unwrap());
        let spend = KeyManager::new_random().unwrap();
        // A view-only wallet, which cannot derive the key and has to import it
        let view_only = || {
            KeyManager::new(WalletType::ViewWallet(ViewWallet::new(
                spend.get_spend_key().pub_key,
                spend.get_private_view_key(),
                None,
            )))
            .unwrap()
        };
        let payload_key = TariKeyId::PayloadAuth { index: 1 };
        let key = spend.get_private_key(&payload_key).unwrap();

        store_payload_auth_key(&db, &view_only(), 1, key.clone()).unwrap();
        // As on the next start
        let reloaded = view_only();
        load_payload_auth_key(&db, &reloaded).unwrap();
        assert_eq!(reloaded.get_payload_auth_key(), Some(1));
        assert_eq!(reloaded.get_private_key(&payload_key).unwrap(), key);

        assert!(clear_payload_auth_key(&db, &reloaded).unwrap());
        assert_eq!(reloaded.get_payload_auth_key(), None);
        let reloaded = view_only();
        load_payload_auth_key(&db, &reloaded).unwrap();
        assert_eq!(reloaded.get_payload_auth_key(), None);

        for malformed in ["1", "x:00", "1:not hex"] {
            db.set_client_key_value(PAYLOAD_AUTH_KEY.to_string(), malformed.to_string())
                .unwrap();
            let reloaded = view_only();
            load_payload_auth_key(&db, &reloaded).unwrap();
            assert_eq!(reloaded.get_payload_auth_key(), None);
        }
    }
}

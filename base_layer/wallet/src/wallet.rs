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
    tari_address::{TariAddress, TariAddressFeatures},
    transaction::{ImportStatus, TxId},
    types::{ComAndPubSignature, Commitment, PrivateKey, PublicKey, RangeProof, SignatureWithDomain},
    wallet_types::WalletType,
};
use tari_contacts::contacts_service::{
    handle::ContactsServiceHandle,
    storage::database::ContactsBackend,
    ContactsServiceInitializer,
};
use tari_core::{
    consensus::{ConsensusManager, NetworkConsensus},
    covenants::Covenant,
    transactions::{
        key_manager::{SecretTransactionKeyManagerInterface, TariKeyId, TransactionKeyManagerInitializer},
        tari_amount::MicroMinotari,
        transaction_components::{encrypted_data::PaymentId, EncryptedData, OutputFeatures, UnblindedOutput},
        CryptoFactories,
    },
};
use tari_crypto::{
    hash_domain,
    ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    signatures::SchnorrSignatureError,
};
use tari_key_manager::{
    cipher_seed::CipherSeed,
    key_manager::KeyManager,
    key_manager_service::{storage::database::KeyManagerBackend, KeyDigest, KeyManagerBranch, KeyManagerServiceError},
    mnemonic::{Mnemonic, MnemonicLanguage},
    SeedWords,
};
use tari_network::{identity, multiaddr::Multiaddr, NetworkHandle, Peer, ToPeerId};
use tari_p2p::{
    auto_update::{AutoUpdateConfig, SoftwareUpdaterHandle, SoftwareUpdaterService},
    connector::InboundMessaging,
    initialization::P2pInitializer,
    message::TariNodeMessageSpec,
    services::liveness::{config::LivenessConfig, LivenessInitializer},
    Dispatcher,
    PeerSeedsConfig,
};
use tari_script::{push_pubkey_script, ExecutionStack, TariScript};
use tari_service_framework::StackBuilder;
use tari_shutdown::ShutdownSignal;
use tari_utilities::{hex::Hex, ByteArray};

use crate::{
    base_node_service::{handle::BaseNodeServiceHandle, BaseNodeServiceInitializer},
    config::WalletConfig,
    connectivity_service::{
        BaseNodePeerManager,
        WalletConnectivityHandle,
        WalletConnectivityInitializer,
        WalletConnectivityInterface,
    },
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

// Domain separator for signing arbitrary messages with a wallet secret key
hash_domain!(
    WalletMessageSigningDomain,
    "com.tari.base_layer.wallet.message_signing",
    1
);

/// A structure containing the config and services that a Wallet application will require. This struct will start up all
/// the services and provide the APIs that applications will use to interact with the services
#[derive(Clone)]
pub struct Wallet<T, U, V, W, TKeyManagerInterface> {
    pub network_consensus: NetworkConsensus,
    pub network: NetworkHandle,
    pub network_public_key: RistrettoPublicKey,
    pub network_keypair: Arc<identity::Keypair>,
    pub output_manager_service: OutputManagerHandle,
    pub key_manager_service: TKeyManagerInterface,
    pub transaction_service: TransactionServiceHandle,
    pub wallet_connectivity: WalletConnectivityHandle,
    pub contacts_service: ContactsServiceHandle,
    pub base_node_service: BaseNodeServiceHandle,
    pub utxo_scanner_service: UtxoScannerHandle,
    pub updater_service: Option<SoftwareUpdaterHandle>,
    pub db: WalletDatabase<T>,
    pub output_db: OutputManagerDatabase<V>,
    pub factories: CryptoFactories,
    pub shutdown_signal: ShutdownSignal,
    wallet_type: Arc<WalletType>,
    _u: PhantomData<U>,
    _v: PhantomData<V>,
    _w: PhantomData<W>,
}

impl<T, U, V, W, TKeyManagerInterface> Wallet<T, U, V, W, TKeyManagerInterface>
where
    T: WalletBackend + 'static,
    U: TransactionBackend + 'static,
    V: OutputManagerBackend + 'static,
    W: ContactsBackend + 'static,
    TKeyManagerInterface: SecretTransactionKeyManagerInterface,
{
    #[allow(clippy::too_many_lines)]
    pub async fn start<TKeyManagerBackend: KeyManagerBackend<PublicKey> + 'static>(
        config: WalletConfig,
        peer_seeds: PeerSeedsConfig,
        auto_update: AutoUpdateConfig,
        network_keypair: Arc<identity::Keypair>,
        consensus_manager: ConsensusManager,
        factories: CryptoFactories,
        wallet_database: WalletDatabase<T>,
        output_manager_database: OutputManagerDatabase<V>,
        transaction_backend: U,
        output_manager_backend: V,
        contacts_backend: W,
        key_manager_backend: TKeyManagerBackend,
        shutdown_signal: ShutdownSignal,
        master_seed: CipherSeed,
        wallet_type: Option<WalletType>,
        user_agent: String,
    ) -> Result<Self, WalletError> {
        let wallet_type = Arc::new(read_or_create_wallet_type(wallet_type, &wallet_database)?);

        let dispatcher = Dispatcher::new();

        debug!(target: LOG_TARGET, "Wallet Initializing");
        info!(
            target: LOG_TARGET,
            "Transaction sending mechanism is {}", config.transaction_service_config.transaction_routing_mechanism
        );
        trace!(target: LOG_TARGET, "Wallet config: {:?}", config);
        let stack = StackBuilder::new(shutdown_signal.clone())
            .add_initializer(P2pInitializer::new(
                config.p2p.clone(),
                user_agent,
                peer_seeds,
                config.network,
                network_keypair.clone(),
            ))
            .add_initializer(OutputManagerServiceInitializer::<V, TKeyManagerInterface>::new(
                config.output_manager_service_config,
                output_manager_backend.clone(),
                factories.clone(),
                config.network.into(),
            ))
            .add_initializer(TransactionKeyManagerInitializer::new(
                key_manager_backend,
                master_seed,
                factories.clone(),
                wallet_type.clone(),
            ))
            .add_initializer(TransactionServiceInitializer::<U, T, TKeyManagerInterface>::new(
                config.transaction_service_config,
                dispatcher.clone(),
                transaction_backend,
                network_keypair.clone(),
                config.network,
                consensus_manager,
                factories.clone(),
                wallet_database.clone(),
                wallet_type.clone(),
            ))
            .add_initializer(LivenessInitializer::new(
                LivenessConfig {
                    auto_ping_interval: Some(config.contacts_auto_ping_interval),
                    num_peers_per_round: 0,       // No random peers
                    max_allowed_ping_failures: 0, // Peer with failed ping-pong will never be removed
                    ..Default::default()
                },
                dispatcher.clone(),
            ))
            .add_initializer(ContactsServiceInitializer::new(
                contacts_backend,
                dispatcher.clone(),
                config.contacts_auto_ping_interval,
                config.contacts_online_ping_window,
            ))
            .add_initializer(BaseNodeServiceInitializer::new(
                config.base_node_service_config.clone(),
                wallet_database.clone(),
            ))
            .add_initializer(WalletConnectivityInitializer::new(config.base_node_service_config))
            .add_initializer(UtxoScannerServiceInitializer::<T, TKeyManagerInterface>::new(
                wallet_database.clone(),
                factories.clone(),
                config.network,
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

        let mut handles = stack.build().await?;

        let inbound = handles
            .take_handle::<InboundMessaging<TariNodeMessageSpec>>()
            .expect("InboundMessaging not setup");
        dispatcher.spawn(inbound);

        let network = handles.expect_handle::<NetworkHandle>();
        let transaction_service_handle = handles.expect_handle::<TransactionServiceHandle>();

        let mut output_manager_handle = handles.expect_handle::<OutputManagerHandle>();
        let key_manager_handle = handles.expect_handle::<TKeyManagerInterface>();
        let contacts_handle = handles.expect_handle::<ContactsServiceHandle>();

        let base_node_service_handle = handles.expect_handle::<BaseNodeServiceHandle>();
        let utxo_scanner_service_handle = handles.expect_handle::<UtxoScannerHandle>();
        let wallet_connectivity = handles.expect_handle::<WalletConnectivityHandle>();
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
        .map_err(|e| {
            error!(target: LOG_TARGET, "{:?}", e);
            e
        })?;

        // 0 == COMMUNICATION_CLIENT TODO: can be removed?
        wallet_database.set_node_features(0)?;

        // storing current network and version
        if let Err(e) = wallet_database
            .set_last_network_and_version(config.network.to_string(), consts::APP_VERSION_NUMBER.to_string())
        {
            warn!("failed to store network and version: {:#?}", e);
        }

        let network_public_key = network_keypair
            .public()
            .try_into_sr25519()
            .map_err(|e| WalletError::UnsupportedKeyType { details: e.to_string() })?
            .inner_key()
            .clone();

        Ok(Self {
            network_consensus: config.network.into(),
            network,
            network_public_key,
            network_keypair,
            output_manager_service: output_manager_handle,
            key_manager_service: key_manager_handle,
            transaction_service: transaction_service_handle,
            contacts_service: contacts_handle,
            base_node_service: base_node_service_handle,
            utxo_scanner_service: utxo_scanner_service_handle,
            updater_service: updater_handle,
            wallet_connectivity,
            db: wallet_database,
            output_db: output_manager_database,
            factories,
            wallet_type,
            shutdown_signal,
            _u: PhantomData,
            _v: PhantomData,
            _w: PhantomData,
        })
    }

    /// This method consumes the wallet so that the handles are dropped which will result in the services async loops
    /// exiting.
    pub async fn wait_until_shutdown(self) {
        self.network.wait_until_shutdown().await;
    }

    /// This function will set the base node that the wallet uses to broadcast transactions, monitor outputs, and
    /// monitor the base node state.
    pub async fn set_base_node_peer(
        &mut self,
        public_key: PublicKey,
        address: Option<Multiaddr>,
        backup_peers: Option<Vec<Peer>>,
    ) -> Result<(), WalletError> {
        info!(
            "Wallet setting base node peer, public key: {}, net address: {:?}.",
            public_key, address
        );

        if let Some(current_node) = self.wallet_connectivity.get_current_base_node_peer_node_id() {
            self.network.remove_peer_from_allow_list(current_node).await?;
        }

        let mut backup_peers = backup_peers.unwrap_or_default();
        let peer_id = public_key.to_peer_id();
        let known_addresses = self.network.get_known_peer_addresses(peer_id).await?;
        let mut peer = Peer::new(
            identity::PublicKey::from(identity::sr25519::PublicKey::from(public_key.clone())),
            known_addresses.unwrap_or_default(),
        );
        if let Some(address) = address {
            peer.add_address(address);
            self.network.add_peer(peer.clone()).await?;
        }
        self.network.add_peer_to_allow_list(peer_id).await?;
        let mut peer_list = vec![peer];
        if let Some(pos) = backup_peers.iter().position(|p| p.peer_id() == peer_id) {
            backup_peers.swap_remove(pos);
        }
        peer_list.extend(backup_peers);
        self.wallet_connectivity
            .set_base_node(BaseNodePeerManager::new(0, peer_list)?);

        Ok(())
    }

    pub fn get_base_node_peer(&mut self) -> Option<Peer> {
        self.wallet_connectivity.get_current_base_node_peer()
    }

    pub async fn check_for_update(&self) -> Option<String> {
        let mut updater = self.updater_service.clone().unwrap();
        debug!(
            target: LOG_TARGET,
            "Checking for updates (current version: {})...",
            env!("CARGO_PKG_VERSION").to_string()
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
            self.network_consensus.as_network(),
            features,
        ))
    }

    pub async fn get_wallet_one_sided_address(&self) -> Result<TariAddress, KeyManagerServiceError> {
        let view_key = self.key_manager_service.get_view_key().await?;
        let spend_key = self.key_manager_service.get_spend_key().await?;
        Ok(TariAddress::new_dual_address(
            view_key.pub_key,
            spend_key.pub_key,
            self.network_consensus.as_network(),
            TariAddressFeatures::create_one_sided_only(),
        ))
    }

    pub async fn get_wallet_id(&self) -> Result<WalletIdentity, WalletError> {
        let address_interactive = self.get_wallet_interactive_address().await?;
        let address_one_sided = self.get_wallet_one_sided_address().await?;
        Ok(WalletIdentity::new(
            self.network_public_key.clone(),
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
        message: String,
        metadata_signature: ComAndPubSignature,
        script_private_key: &PrivateKey,
        sender_offset_public_key: &PublicKey,
        script_lock_height: u64,
        covenant: Covenant,
        encrypted_data: EncryptedData,
        minimum_value_promise: MicroMinotari,
        range_proof: Option<RangeProof>,
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
        self.import_unblinded_output_as_non_rewindable(unblinded_output, source_address, message)
            .await
    }

    /// Import an external spendable UTXO into the wallet as a non-rewindable/non-recoverable UTXO. The output will be
    /// added to the Output Manager and made spendable. A faux incoming transaction will be created to provide a record
    /// of the event. The TxId of the generated transaction is returned.
    pub async fn import_unblinded_output_as_non_rewindable(
        &mut self,
        unblinded_output: UnblindedOutput,
        source_address: TariAddress,
        message: String,
    ) -> Result<TxId, WalletError> {
        let value = unblinded_output.value;
        let wallet_output = unblinded_output
            .to_wallet_output(&self.key_manager_service, PaymentId::Empty)
            .await?;
        let tx_id = self
            .transaction_service
            .import_utxo_with_status(
                value,
                source_address,
                message,
                ImportStatus::Imported,
                None,
                None,
                None,
                wallet_output.to_transaction_output(&self.key_manager_service).await?,
                PaymentId::Empty,
            )
            .await?;
        // As non-rewindable
        self.output_manager_service
            .add_unvalidated_output(tx_id, wallet_output.clone(), None)
            .await?;
        info!(
            target: LOG_TARGET,
            "UTXO (Commitment: {}, value: {}, txID: {}) imported into wallet as 'ImportStatus::Imported' and is non-rewindable",
            wallet_output.commitment(&self.key_manager_service).await?.to_hex(),
            wallet_output.value,
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
        public_key: &PublicKey,
        signature: &SignatureWithDomain<WalletMessageSigningDomain>,
        message: &str,
    ) -> bool {
        signature.verify(public_key, message)
    }

    /// Appraise the expected outputs and a fee
    pub async fn preview_coin_split_with_commitments_no_amount(
        &mut self,
        commitments: Vec<Commitment>,
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
        commitments: Vec<Commitment>,
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
        commitments: Vec<Commitment>,
        amount_per_split: MicroMinotari,
        split_count: usize,
        fee_per_gram: MicroMinotari,
        message: String,
    ) -> Result<TxId, WalletError> {
        let coin_split_tx = self
            .output_manager_service
            .create_coin_split(commitments, amount_per_split, split_count, fee_per_gram)
            .await;

        match coin_split_tx {
            Ok((tx_id, split_tx, amount)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, split_tx, amount, message)
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
        commitments: Vec<Commitment>,
        split_count: usize,
        fee_per_gram: MicroMinotari,
        message: String,
    ) -> Result<TxId, WalletError> {
        let coin_split_tx = self
            .output_manager_service
            .create_coin_split_even(commitments, split_count, fee_per_gram)
            .await;

        match coin_split_tx {
            Ok((tx_id, split_tx, amount)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, split_tx, amount, message)
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
        commitments: Vec<Commitment>,
        split_count: usize,
        fee_per_gram: MicroMinotari,
        message: String,
    ) -> Result<TxId, WalletError> {
        let coin_split_tx = self
            .output_manager_service
            .create_coin_split_even(commitments, split_count, fee_per_gram)
            .await;

        match coin_split_tx {
            Ok((tx_id, split_tx, amount)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, split_tx, amount, message)
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
        commitments: Vec<Commitment>,
        fee_per_gram: MicroMinotari,
        msg: Option<String>,
    ) -> Result<TxId, WalletError> {
        let coin_join_tx = self
            .output_manager_service
            .create_coin_join(commitments, fee_per_gram)
            .await;

        match coin_join_tx {
            Ok((tx_id, tx, output_value)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, tx, output_value, msg.unwrap_or_default())
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
                let seed = CipherSeed::new();
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

pub fn derive_comms_secret_key(master_seed: &CipherSeed) -> Result<RistrettoSecretKey, WalletError> {
    let comms_key_manager =
        KeyManager::<PublicKey, KeyDigest>::from(master_seed.clone(), KeyManagerBranch::Comms.get_branch_key(), 0);
    Ok(comms_key_manager.derive_key(0)?.key)
}

/// Persist the one-sided payment script for the current wallet NodeIdentity for use during scanning for One-sided
/// payment outputs. This is peristed so that if the Node Identity changes the wallet will still scan for outputs
/// using old node identities.
async fn persist_one_sided_payment_script_for_node_identity(
    output_manager_service: &mut OutputManagerHandle,
    spend_key: &PublicKey,
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

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

use aes_gcm::{
    aead::{generic_array::GenericArray, NewAead},
    Aes256Gcm,
};
use digest::Digest;
use log::*;
use rand::rngs::OsRng;
use tari_common::configuration::bootstrap::ApplicationType;
use tari_crypto::{
    common::Blake256,
    keys::SecretKey,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    script,
    script::{ExecutionStack, TariScript},
    signatures::{SchnorrSignature, SchnorrSignatureError},
    tari_utilities::hex::Hex,
};

use tari_common_types::types::{ComSignature, PrivateKey, PublicKey};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::{CommsPublicKey, CommsSecretKey},
    CommsNode,
    NodeIdentity,
    UnspawnedCommsNode,
};
use tari_comms_dht::{store_forward::StoreAndForwardRequester, Dht};
use tari_core::{
    consensus::NetworkConsensus,
    transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, UnblindedOutput},
        CryptoFactories,
    },
};
use tari_key_manager::key_manager::KeyManager;
use tari_p2p::{
    auto_update::{SoftwareUpdaterHandle, SoftwareUpdaterService},
    comms_connector::pubsub_connector,
    initialization,
    initialization::P2pInitializer,
};
use tari_service_framework::StackBuilder;
use tari_shutdown::ShutdownSignal;
use tracing::instrument;

use crate::{
    base_node_service::{handle::BaseNodeServiceHandle, BaseNodeServiceInitializer},
    config::{WalletConfig, KEY_MANAGER_COMMS_SECRET_KEY_BRANCH_KEY},
    connectivity_service::{WalletConnectivityHandle, WalletConnectivityInitializer, WalletConnectivityInterface},
    contacts_service::{handle::ContactsServiceHandle, storage::database::ContactsBackend, ContactsServiceInitializer},
    error::WalletError,
    output_manager_service::{
        error::OutputManagerError,
        handle::OutputManagerHandle,
        storage::{database::OutputManagerBackend, models::KnownOneSidedPaymentScript},
        OutputManagerServiceInitializer,
    },
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        handle::TransactionServiceHandle,
        storage::database::TransactionBackend,
        TransactionServiceInitializer,
    },
    types::KeyDigest,
    utxo_scanner_service::{handle::UtxoScannerHandle, UtxoScannerServiceInitializer},
};
use tari_common_types::transaction::TxId;

const LOG_TARGET: &str = "wallet";

/// A structure containing the config and services that a Wallet application will require. This struct will start up all
/// the services and provide the APIs that applications will use to interact with the services
#[derive(Clone)]
pub struct Wallet<T, U, V, W> {
    pub network: NetworkConsensus,
    pub comms: CommsNode,
    pub dht_service: Dht,
    pub store_and_forward_requester: StoreAndForwardRequester,
    pub output_manager_service: OutputManagerHandle,
    pub transaction_service: TransactionServiceHandle,
    pub wallet_connectivity: WalletConnectivityHandle,
    pub contacts_service: ContactsServiceHandle,
    pub base_node_service: BaseNodeServiceHandle,
    pub utxo_scanner_service: UtxoScannerHandle,
    pub updater_service: Option<SoftwareUpdaterHandle>,
    pub db: WalletDatabase<T>,
    pub factories: CryptoFactories,
    _u: PhantomData<U>,
    _v: PhantomData<V>,
    _w: PhantomData<W>,
}

impl<T, U, V, W> Wallet<T, U, V, W>
where
    T: WalletBackend + 'static,
    U: TransactionBackend + 'static,
    V: OutputManagerBackend + 'static,
    W: ContactsBackend + 'static,
{
    #[instrument(
        name = "wallet::start",
        skip(
            config,
            wallet_database,
            transaction_backend,
            output_manager_backend,
            contacts_backend,
            shutdown_signal,
            recovery_master_key
        )
    )]
    pub async fn start(
        config: WalletConfig,
        wallet_database: WalletDatabase<T>,
        transaction_backend: U,
        output_manager_backend: V,
        contacts_backend: W,
        shutdown_signal: ShutdownSignal,
        recovery_master_key: Option<CommsSecretKey>,
    ) -> Result<Self, WalletError> {
        let master_secret_key =
            read_or_create_master_secret_key(recovery_master_key, &mut wallet_database.clone()).await?;
        let comms_secret_key = derive_comms_secret_key(&master_secret_key)?;

        let node_identity = Arc::new(NodeIdentity::new(
            comms_secret_key,
            config.comms_config.node_identity.public_address(),
            config.comms_config.node_identity.features(),
        ));

        let mut comms_config = config.comms_config.clone();
        comms_config.node_identity = node_identity.clone();

        let bn_service_db = wallet_database.clone();

        let factories = config.factories.clone();
        let (publisher, subscription_factory) = pubsub_connector(config.buffer_size, config.rate_limit);
        let peer_message_subscription_factory = Arc::new(subscription_factory);
        let transport_type = config.comms_config.transport_type.clone();

        debug!(target: LOG_TARGET, "Wallet Initializing");
        info!(
            target: LOG_TARGET,
            "Transaction sending mechanism is {}",
            config
                .clone()
                .transaction_service_config
                .unwrap_or_default()
                .transaction_routing_mechanism
        );
        trace!(
            target: LOG_TARGET,
            "Wallet config: {:?}, {:?}, {:?}, buffer_size: {}, rate_limit: {}",
            config.base_node_service_config,
            config.output_manager_service_config,
            config.transaction_service_config,
            config.buffer_size,
            config.rate_limit
        );
        let stack = StackBuilder::new(shutdown_signal)
            .add_initializer(P2pInitializer::new(comms_config, publisher))
            .add_initializer(OutputManagerServiceInitializer::new(
                config.output_manager_service_config.unwrap_or_default(),
                output_manager_backend,
                factories.clone(),
                config.network,
                master_secret_key,
            ))
            .add_initializer(TransactionServiceInitializer::new(
                config.transaction_service_config.unwrap_or_default(),
                peer_message_subscription_factory,
                transaction_backend,
                node_identity.clone(),
                factories.clone(),
                wallet_database.clone(),
            ))
            .add_initializer(ContactsServiceInitializer::new(contacts_backend))
            .add_initializer(BaseNodeServiceInitializer::new(
                config.base_node_service_config.clone(),
                bn_service_db,
            ))
            .add_initializer(WalletConnectivityInitializer::new(config.base_node_service_config))
            .add_initializer(UtxoScannerServiceInitializer::new(
                config.scan_for_utxo_interval,
                wallet_database.clone(),
                factories.clone(),
                node_identity.clone(),
            ));

        // Check if we have update config. FFI wallets don't do this, the update on mobile is done differently.
        let stack = match config.updater_config {
            Some(ref updater_config) => stack.add_initializer(SoftwareUpdaterService::new(
                ApplicationType::ConsoleWallet,
                env!("CARGO_PKG_VERSION")
                    .to_string()
                    .parse()
                    .expect("Unable to parse console wallet version."),
                updater_config.clone(),
                config.autoupdate_check_interval,
            )),
            _ => stack,
        };

        let mut handles = stack.build().await?;

        let comms = handles
            .take_handle::<UnspawnedCommsNode>()
            .expect("P2pInitializer was not added to the stack");
        let comms = initialization::spawn_comms_using_transport(comms, transport_type).await?;

        let mut output_manager_handle = handles.expect_handle::<OutputManagerHandle>();
        let transaction_service_handle = handles.expect_handle::<TransactionServiceHandle>();
        let contacts_handle = handles.expect_handle::<ContactsServiceHandle>();
        let dht = handles.expect_handle::<Dht>();
        let store_and_forward_requester = dht.store_and_forward_requester();

        let base_node_service_handle = handles.expect_handle::<BaseNodeServiceHandle>();
        let utxo_scanner_service_handle = handles.expect_handle::<UtxoScannerHandle>();
        let wallet_connectivity = handles.expect_handle::<WalletConnectivityHandle>();
        let updater_handle = config
            .updater_config
            .map(|_updater_config| handles.expect_handle::<SoftwareUpdaterHandle>());

        persist_one_sided_payment_script_for_node_identity(&mut output_manager_handle, comms.node_identity())
            .await
            .map_err(|e| {
                error!(target: LOG_TARGET, "{:?}", e);
                e
            })?;

        // Persist the comms node address and features after it has been spawned to capture any modifications made
        // during comms startup. In the case of a Tor Transport the public address could have been generated
        wallet_database
            .set_node_address(comms.node_identity().public_address())
            .await?;
        wallet_database
            .set_node_features(comms.node_identity().features())
            .await?;

        Ok(Self {
            network: config.network,
            comms,
            dht_service: dht,
            store_and_forward_requester,
            output_manager_service: output_manager_handle,
            transaction_service: transaction_service_handle,
            contacts_service: contacts_handle,
            base_node_service: base_node_service_handle,
            utxo_scanner_service: utxo_scanner_service_handle,
            updater_service: updater_handle,
            wallet_connectivity,
            db: wallet_database,
            factories,
            _u: PhantomData,
            _v: PhantomData,
            _w: PhantomData,
        })
    }

    /// This method consumes the wallet so that the handles are dropped which will result in the services async loops
    /// exiting.
    pub async fn wait_until_shutdown(self) {
        self.comms.to_owned().wait_until_shutdown().await;
    }

    /// This function will set the base node that the wallet uses to broadcast transactions, monitor outputs, and
    /// monitor the base node state.
    pub async fn set_base_node_peer(
        &mut self,
        public_key: CommsPublicKey,
        net_address: String,
    ) -> Result<(), WalletError> {
        info!(
            "Wallet setting base node peer, public key: {}, net address: {}.",
            public_key, net_address
        );

        let address = net_address.parse::<Multiaddr>()?;
        let peer = Peer::new(
            public_key.clone(),
            NodeId::from_key(&public_key),
            vec![address].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            String::new(),
        );

        self.comms.peer_manager().add_peer(peer.clone()).await?;
        self.wallet_connectivity.set_base_node(peer);

        Ok(())
    }

    pub async fn get_base_node_peer(&mut self) -> Option<Peer> {
        self.wallet_connectivity.get_current_base_node_peer()
    }

    #[instrument(name = "wallet::check_for_update", skip(self))]
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

    pub fn get_software_updater(&self) -> SoftwareUpdaterHandle {
        self.updater_service.clone().unwrap()
    }

    /// Import an external spendable UTXO into the wallet. The output will be added to the Output Manager and made
    /// spendable. A faux incoming transaction will be created to provide a record of the event. The TxId of the
    /// generated transaction is returned.
    #[allow(clippy::too_many_arguments)]
    #[instrument(
        name = "wallet::import_utxo",
        skip(
            self,
            amount,
            spending_key,
            script,
            input_data,
            source_public_key,
            features,
            message,
            metadata_signature,
            script_private_key,
            sender_offset_public_key
        )
    )]
    pub async fn import_utxo(
        &mut self,
        amount: MicroTari,
        spending_key: &PrivateKey,
        script: TariScript,
        input_data: ExecutionStack,
        source_public_key: &CommsPublicKey,
        features: OutputFeatures,
        message: String,
        metadata_signature: ComSignature,
        script_private_key: &PrivateKey,
        sender_offset_public_key: &PublicKey,
    ) -> Result<TxId, WalletError> {
        let unblinded_output = UnblindedOutput::new(
            amount,
            spending_key.clone(),
            features.clone(),
            script,
            input_data,
            script_private_key.clone(),
            sender_offset_public_key.clone(),
            metadata_signature,
        );

        let tx_id = self
            .transaction_service
            .import_utxo(amount, source_public_key.clone(), message, Some(features.maturity))
            .await?;

        self.output_manager_service
            .add_output_with_tx_id(tx_id, unblinded_output.clone())
            .await?;

        info!(
            target: LOG_TARGET,
            "UTXO (Commitment: {}) imported into wallet",
            unblinded_output
                .as_transaction_input(&self.factories.commitment)?
                .commitment
                .to_hex()
        );

        Ok(tx_id)
    }

    /// Import an external spendable UTXO into the wallet. The output will be added to the Output Manager and made
    /// spendable. A faux incoming transaction will be created to provide a record of the event. The TxId of the
    /// generated transaction is returned.
    #[instrument(
        name = "wallet::import_blinded_utxo",
        skip(self, unblinded_output, source_public_key, message)
    )]
    pub async fn import_unblinded_utxo(
        &mut self,
        unblinded_output: UnblindedOutput,
        source_public_key: &CommsPublicKey,
        message: String,
    ) -> Result<TxId, WalletError> {
        let tx_id = self
            .transaction_service
            .import_utxo(
                unblinded_output.value,
                source_public_key.clone(),
                message,
                Some(unblinded_output.features.maturity),
            )
            .await?;

        self.output_manager_service
            .add_output_with_tx_id(tx_id, unblinded_output.clone())
            .await?;

        info!(
            target: LOG_TARGET,
            "UTXO (Commitment: {}) imported into wallet",
            unblinded_output
                .as_transaction_input(&self.factories.commitment)?
                .commitment
                .to_hex()
        );

        Ok(tx_id)
    }

    pub fn sign_message(
        &mut self,
        secret: RistrettoSecretKey,
        nonce: RistrettoSecretKey,
        message: &str,
    ) -> Result<SchnorrSignature<RistrettoPublicKey, RistrettoSecretKey>, SchnorrSignatureError> {
        let challenge = Blake256::digest(message.as_bytes());
        RistrettoSchnorr::sign(secret, nonce, &challenge)
    }

    pub fn verify_message_signature(
        &mut self,
        public_key: RistrettoPublicKey,
        public_nonce: RistrettoPublicKey,
        signature: RistrettoSecretKey,
        message: String,
    ) -> bool {
        let signature = RistrettoSchnorr::new(public_nonce, signature);
        let challenge = Blake256::digest(message.as_bytes());
        signature.verify_challenge(&public_key, challenge.clone().as_slice())
    }

    /// Do a coin split
    #[instrument(
        name = "wallet::coin_split",
        skip(self, amount_per_split, split_count, fee_per_gram, message, lock_height)
    )]
    pub async fn coin_split(
        &mut self,
        amount_per_split: MicroTari,
        split_count: usize,
        fee_per_gram: MicroTari,
        message: String,
        lock_height: Option<u64>,
    ) -> Result<TxId, WalletError> {
        let coin_split_tx = self
            .output_manager_service
            .create_coin_split(amount_per_split, split_count, fee_per_gram, lock_height)
            .await;

        match coin_split_tx {
            Ok((tx_id, split_tx, amount, fee)) => {
                let coin_tx = self
                    .transaction_service
                    .submit_transaction(tx_id, split_tx, fee, amount, message)
                    .await;
                match coin_tx {
                    Ok(_) => Ok(tx_id),
                    Err(e) => Err(WalletError::TransactionServiceError(e)),
                }
            },
            Err(e) => Err(WalletError::OutputManagerError(e)),
        }
    }

    /// Apply encryption to all the Wallet db backends. The Wallet backend will test if the db's are already encrypted
    /// in which case this will fail.
    #[instrument(name = "wallet::apply_encryption", skip(self, passphrase))]
    pub async fn apply_encryption(&mut self, passphrase: String) -> Result<(), WalletError> {
        debug!(target: LOG_TARGET, "Applying wallet encryption.");
        let passphrase_hash = Blake256::new().chain(passphrase.as_bytes()).finalize();
        let key = GenericArray::from_slice(passphrase_hash.as_slice());
        let cipher = Aes256Gcm::new(key);

        self.db.apply_encryption(cipher.clone()).await?;
        self.output_manager_service.apply_encryption(cipher.clone()).await?;
        self.transaction_service.apply_encryption(cipher).await?;
        Ok(())
    }

    /// Remove encryption from all the Wallet db backends. If any backends do not have encryption applied then this will
    /// fail
    #[instrument(name = "wallet::remove_encryption", skip(self))]
    pub async fn remove_encryption(&mut self) -> Result<(), WalletError> {
        self.db.remove_encryption().await?;
        self.output_manager_service.remove_encryption().await?;
        self.transaction_service.remove_encryption().await?;
        Ok(())
    }

    /// Utility function to find out if there is data in the database indicating that there is an incomplete recovery
    /// process in progress
    pub async fn is_recovery_in_progress(&self) -> Result<bool, WalletError> {
        use crate::utxo_scanner_service::utxo_scanning::RECOVERY_KEY;
        Ok(self.db.get_client_key_value(RECOVERY_KEY.to_string()).await?.is_some())
    }
}

async fn read_or_create_master_secret_key<T: WalletBackend + 'static>(
    recovery_master_key: Option<CommsSecretKey>,
    db: &mut WalletDatabase<T>,
) -> Result<CommsSecretKey, WalletError> {
    let db_master_secret_key = db.get_master_secret_key().await?;

    let master_secret_key = match recovery_master_key {
        None => match db_master_secret_key {
            None => {
                let secret_key = CommsSecretKey::random(&mut OsRng);
                db.set_master_secret_key(secret_key.clone()).await?;
                secret_key
            },
            Some(secret_key) => secret_key,
        },
        Some(recovery_key) => {
            if db_master_secret_key.is_none() {
                db.set_master_secret_key(recovery_key.clone()).await?;
                recovery_key
            } else {
                error!(
                    target: LOG_TARGET,
                    "Attempted recovery would overwrite the existing wallet database master secret key, causing a \
                     `MasterSecretKeyMismatch` error."
                );
                let msg = "Wallet already exists! Move the existing wallet database file.".to_string();
                return Err(WalletError::WalletRecoveryError(msg));
            }
        },
    };

    Ok(master_secret_key)
}

fn derive_comms_secret_key(master_secret_key: &CommsSecretKey) -> Result<CommsSecretKey, WalletError> {
    let comms_key_manager = KeyManager::<PrivateKey, KeyDigest>::from(
        master_secret_key.clone(),
        KEY_MANAGER_COMMS_SECRET_KEY_BRANCH_KEY.to_string(),
        0,
    );
    Ok(comms_key_manager.derive_key(0)?.k)
}

/// Persist the one-sided payment script for the current wallet NodeIdentity for use during scanning for One-sided
/// payment outputs. This is peristed so that if the Node Identity changes the wallet will still scan for outputs
/// using old node identities.
#[instrument(
    name = "wallet::persist_one_sided_payment_script_for_node_identity",
    skip(output_manager_service, node_identity)
)]
pub async fn persist_one_sided_payment_script_for_node_identity(
    output_manager_service: &mut OutputManagerHandle,
    node_identity: Arc<NodeIdentity>,
) -> Result<(), WalletError> {
    let script = script!(PushPubKey(Box::new(node_identity.public_key().clone())));
    let known_script = KnownOneSidedPaymentScript {
        script_hash: script
            .as_hash::<Blake256>()
            .map_err(|e| WalletError::OutputManagerError(OutputManagerError::ScriptError(e)))?
            .to_vec(),
        private_key: node_identity.secret_key().clone(),
        script,
        input: ExecutionStack::default(),
    };

    output_manager_service.add_known_script(known_script).await?;
    Ok(())
}

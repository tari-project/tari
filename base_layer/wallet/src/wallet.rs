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

use crate::{
    base_node_service::{config::BaseNodeServiceConfig, handle::BaseNodeServiceHandle, BaseNodeServiceInitializer},
    contacts_service::{handle::ContactsServiceHandle, storage::database::ContactsBackend, ContactsServiceInitializer},
    error::WalletError,
    output_manager_service::{
        config::OutputManagerServiceConfig,
        handle::OutputManagerHandle,
        storage::database::OutputManagerBackend,
        OutputManagerServiceInitializer,
        TxId,
    },
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        config::TransactionServiceConfig,
        handle::TransactionServiceHandle,
        storage::database::TransactionBackend,
        TransactionServiceInitializer,
    },
};
use aes_gcm::{
    aead::{generic_array::GenericArray, NewAead},
    Aes256Gcm,
};
use digest::Digest;
use log::*;
use std::{marker::PhantomData, sync::Arc};
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
    CommsNode,
    UnspawnedCommsNode,
};
use tari_comms_dht::{store_forward::StoreAndForwardRequester, Dht};
use tari_core::{
    consensus::Network,
    transactions::{
        tari_amount::MicroTari,
        transaction::{OutputFeatures, UnblindedOutput},
        types::{CryptoFactories, PrivateKey},
    },
};
use tari_crypto::{
    common::Blake256,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    signatures::{SchnorrSignature, SchnorrSignatureError},
    tari_utilities::hex::Hex,
};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization,
    initialization::{CommsConfig, P2pInitializer},
};
use tari_service_framework::StackBuilder;
use tari_shutdown::ShutdownSignal;
use tokio::runtime;

const LOG_TARGET: &str = "wallet";

#[derive(Clone)]
pub struct WalletConfig {
    pub comms_config: CommsConfig,
    pub factories: CryptoFactories,
    pub transaction_service_config: Option<TransactionServiceConfig>,
    pub output_manager_service_config: Option<OutputManagerServiceConfig>,
    pub buffer_size: usize,
    pub rate_limit: usize,
    pub network: Network,
    pub base_node_service_config: BaseNodeServiceConfig,
}

impl WalletConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        comms_config: CommsConfig,
        factories: CryptoFactories,
        transaction_service_config: Option<TransactionServiceConfig>,
        output_manager_service_config: Option<OutputManagerServiceConfig>,
        network: Network,
        base_node_service_config: Option<BaseNodeServiceConfig>,
        buffer_size: Option<usize>,
        rate_limit: Option<usize>,
    ) -> Self
    {
        Self {
            comms_config,
            factories,
            transaction_service_config,
            output_manager_service_config,
            buffer_size: buffer_size.unwrap_or_else(|| 1500),
            rate_limit: rate_limit.unwrap_or_else(|| 50),
            network,
            base_node_service_config: base_node_service_config.unwrap_or_default(),
        }
    }
}

/// A structure containing the config and services that a Wallet application will require. This struct will start up all
/// the services and provide the APIs that applications will use to interact with the services
#[derive(Clone)]
pub struct Wallet<T, U, V, W>
where
    T: WalletBackend + 'static,
    U: TransactionBackend + 'static,
    V: OutputManagerBackend + 'static,
    W: ContactsBackend + 'static,
{
    pub comms: CommsNode,
    pub dht_service: Dht,
    pub store_and_forward_requester: StoreAndForwardRequester,
    pub output_manager_service: OutputManagerHandle,
    pub transaction_service: TransactionServiceHandle,
    pub contacts_service: ContactsServiceHandle,
    pub base_node_service: BaseNodeServiceHandle,
    pub db: WalletDatabase<T>,
    pub factories: CryptoFactories,
    #[cfg(feature = "test_harness")]
    pub transaction_backend: U,
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
    pub async fn new(
        config: WalletConfig,
        wallet_backend: T,
        transaction_backend: U,
        output_manager_backend: V,
        contacts_backend: W,
        shutdown_signal: ShutdownSignal,
    ) -> Result<Wallet<T, U, V, W>, WalletError>
    {
        let db = WalletDatabase::new(wallet_backend);
        // Persist the Comms Private Key provided to this function
        db.set_comms_secret_key(config.comms_config.node_identity.secret_key().clone())
            .await?;
        let bn_service_db = db.clone();
        #[cfg(feature = "test_harness")]
        let transaction_backend_handle = transaction_backend.clone();

        let factories = config.clone().factories;
        let (publisher, subscription_factory) =
            pubsub_connector(runtime::Handle::current(), config.buffer_size, config.rate_limit);
        let peer_message_subscription_factory = Arc::new(subscription_factory);
        let transport_type = config.comms_config.transport_type.clone();
        let node_identity = config.comms_config.node_identity.clone();

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
            .add_initializer(P2pInitializer::new(config.comms_config, publisher))
            .add_initializer(OutputManagerServiceInitializer::new(
                config.output_manager_service_config.unwrap_or_default(),
                output_manager_backend,
                factories.clone(),
                config.network,
            ))
            .add_initializer(TransactionServiceInitializer::new(
                config.transaction_service_config.unwrap_or_default(),
                peer_message_subscription_factory,
                transaction_backend,
                node_identity.clone(),
                factories.clone(),
            ))
            .add_initializer(ContactsServiceInitializer::new(contacts_backend))
            .add_initializer(BaseNodeServiceInitializer::new(
                config.base_node_service_config,
                bn_service_db,
            ));

        let mut handles = stack.build().await?;

        let comms = handles
            .take_handle::<UnspawnedCommsNode>()
            .expect("P2pInitializer was not added to the stack");
        let comms = initialization::spawn_comms_using_transport(comms, transport_type).await?;

        let output_manager_handle = handles.expect_handle::<OutputManagerHandle>();
        let transaction_service_handle = handles.expect_handle::<TransactionServiceHandle>();
        let contacts_handle = handles.expect_handle::<ContactsServiceHandle>();
        let dht = handles.expect_handle::<Dht>();
        let store_and_forward_requester = dht.store_and_forward_requester();

        let base_node_service_handle = handles.expect_handle::<BaseNodeServiceHandle>();

        Ok(Wallet {
            comms,
            dht_service: dht,
            store_and_forward_requester,
            output_manager_service: output_manager_handle,
            transaction_service: transaction_service_handle,
            contacts_service: contacts_handle,
            base_node_service: base_node_service_handle,
            db,
            factories,
            #[cfg(feature = "test_harness")]
            transaction_backend: transaction_backend_handle,
            _u: PhantomData,
            _v: PhantomData,
            _w: PhantomData,
        })
    }

    /// This method consumes the wallet so that the handles are dropped which will result in the services async loops
    /// exiting.
    pub async fn wait_until_shutdown(self) {
        self.comms.clone().wait_until_shutdown().await;
    }

    /// This function will set the base_node that the wallet uses to broadcast transactions, monitor outputs, and
    /// monitor the base node state.
    pub async fn set_base_node_peer(
        &mut self,
        public_key: CommsPublicKey,
        net_address: String,
    ) -> Result<(), WalletError>
    {
        info!(
            "Wallet setting base node peer, public key: {}, net address: {}.",
            public_key, net_address
        );

        let address = net_address.parse::<Multiaddr>()?;
        let peer = Peer::new(
            public_key.clone(),
            NodeId::from_key(&public_key).unwrap(),
            vec![address].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            Default::default(),
            String::new(),
        );

        self.comms.peer_manager().add_peer(peer.clone()).await?;
        self.comms
            .connectivity()
            .add_managed_peers(vec![peer.node_id.clone()])
            .await?;

        self.transaction_service
            .set_base_node_public_key(peer.public_key.clone())
            .await?;

        self.output_manager_service
            .set_base_node_public_key(peer.public_key.clone())
            .await?;

        self.base_node_service.clone().set_base_node_peer(peer).await?;

        Ok(())
    }

    /// Import an external spendable UTXO into the wallet. The output will be added to the Output Manager and made
    /// spendable. A faux incoming transaction will be created to provide a record of the event. The TxId of the
    /// generated transaction is returned.
    pub async fn import_utxo(
        &mut self,
        amount: MicroTari,
        spending_key: &PrivateKey,
        source_public_key: &CommsPublicKey,
        message: String,
    ) -> Result<TxId, WalletError>
    {
        let unblinded_output = UnblindedOutput::new(amount, spending_key.clone(), None);

        self.output_manager_service.add_output(unblinded_output.clone()).await?;

        let tx_id = self
            .transaction_service
            .import_utxo(amount, source_public_key.clone(), message)
            .await?;

        info!(
            target: LOG_TARGET,
            "UTXO (Commitment: {}) imported into wallet",
            unblinded_output
                .as_transaction_input(&self.factories.commitment, OutputFeatures::default())
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
    ) -> Result<SchnorrSignature<RistrettoPublicKey, RistrettoSecretKey>, SchnorrSignatureError>
    {
        let challenge = Blake256::digest(message.as_bytes());
        RistrettoSchnorr::sign(secret, nonce, challenge.clone().as_slice())
    }

    pub fn verify_message_signature(
        &mut self,
        public_key: RistrettoPublicKey,
        public_nonce: RistrettoPublicKey,
        signature: RistrettoSecretKey,
        message: String,
    ) -> bool
    {
        let signature = RistrettoSchnorr::new(public_nonce, signature);
        let challenge = Blake256::digest(message.as_bytes());
        signature.verify_challenge(&public_key, challenge.clone().as_slice())
    }

    /// Do a coin split
    pub async fn coin_split(
        &mut self,
        amount_per_split: MicroTari,
        split_count: usize,
        fee_per_gram: MicroTari,
        message: String,
        lock_height: Option<u64>,
    ) -> Result<TxId, WalletError>
    {
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
    pub async fn apply_encryption(&mut self, passphrase: String) -> Result<(), WalletError> {
        debug!(target: LOG_TARGET, "Applying wallet encryption.");
        let passphrase_hash = Blake256::new().chain(passphrase.as_bytes()).result().to_vec();
        let key = GenericArray::from_slice(passphrase_hash.as_slice());
        let cipher = Aes256Gcm::new(key);

        self.db.apply_encryption(cipher.clone()).await?;
        self.output_manager_service.apply_encryption(cipher.clone()).await?;
        self.transaction_service.apply_encryption(cipher).await?;
        Ok(())
    }

    /// Remove encryption from all the Wallet db backends. If any backends do not have encryption applied then this will
    /// fail
    pub async fn remove_encryption(&mut self) -> Result<(), WalletError> {
        self.db.remove_encryption().await?;
        self.output_manager_service.remove_encryption().await?;
        self.transaction_service.remove_encryption().await?;
        Ok(())
    }
}

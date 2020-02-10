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
    contacts_service::{handle::ContactsServiceHandle, storage::database::ContactsBackend, ContactsServiceInitializer},
    error::WalletError,
    output_manager_service::{
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
use blake2::Digest;
use log::{LevelFilter, *};
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
    Handle as LogHandle,
};
use std::{marker::PhantomData, sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsNode,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::CommsPublicKey,
};
use tari_comms_dht::Dht;
use tari_core::transactions::{
    tari_amount::MicroTari,
    transaction::UnblindedOutput,
    types::{CryptoFactories, PrivateKey},
};
use tari_crypto::{
    common::Blake256,
    ristretto::{RistrettoPublicKey, RistrettoSchnorr, RistrettoSecretKey},
    signatures::{SchnorrSignature, SchnorrSignatureError},
};
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{initialize_comms, CommsConfig},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessConfig, LivenessHandle, LivenessInitializer},
    },
};
use tari_service_framework::StackBuilder;
use tokio::runtime::Runtime;

const LOG_TARGET: &'static str = "base_layer::wallet";

#[derive(Clone)]
pub struct WalletConfig {
    pub comms_config: CommsConfig,
    pub logging_path: Option<String>,
    pub factories: CryptoFactories,
}

/// A structure containing the config and services that a Wallet application will require. This struct will start up all
/// the services and provide the APIs that applications will use to interact with the services
pub struct Wallet<T, U, V, W>
where
    T: WalletBackend + 'static,
    U: TransactionBackend + Clone + 'static,
    V: OutputManagerBackend + 'static,
    W: ContactsBackend + 'static,
{
    pub comms: CommsNode,
    pub dht_service: Dht,
    pub liveness_service: LivenessHandle,
    pub output_manager_service: OutputManagerHandle,
    pub transaction_service: TransactionServiceHandle,
    pub contacts_service: ContactsServiceHandle,
    pub db: WalletDatabase<T>,
    pub runtime: Runtime,
    pub log_handle: Option<LogHandle>,
    #[cfg(feature = "test_harness")]
    pub transaction_backend: U,
    _u: PhantomData<U>,
    _v: PhantomData<V>,
    _w: PhantomData<W>,
}

impl<T, U, V, W> Wallet<T, U, V, W>
where
    T: WalletBackend + 'static,
    U: TransactionBackend + Clone + 'static,
    V: OutputManagerBackend + 'static,
    W: ContactsBackend + 'static,
{
    pub fn new(
        config: WalletConfig,
        mut runtime: Runtime,
        wallet_backend: T,
        transaction_backend: U,
        output_manager_backend: V,
        contacts_backend: W,
    ) -> Result<Wallet<T, U, V, W>, WalletError>
    {
        let mut log_handle = None;
        if let Some(path) = config.logging_path {
            let logfile = FileAppender::builder()
                .encoder(Box::new(PatternEncoder::new(
                    "{d(%Y-%m-%d %H:%M:%S.%f)} [{M}#{L}] [{t}] {l:5} {m} (({T}:{I})){n}",
                )))
                .append(false)
                .build(path.as_str())
                .unwrap();

            let config = Config::builder()
                .appender(Appender::builder().build("logfile", Box::new(logfile)))
                .build(Root::builder().appender("logfile").build(LevelFilter::Info))
                .unwrap();

            log_handle = Some(log4rs::init_config(config)?);
        }

        let db = WalletDatabase::new(wallet_backend);
        let base_node_peers = runtime.block_on(db.get_peers())?;

        #[cfg(feature = "test_harness")]
        let transaction_backend_handle = transaction_backend.clone();

        let factories = config.factories;
        let (publisher, subscription_factory) =
            pubsub_connector(runtime.handle().clone(), config.comms_config.inbound_buffer_size);
        let subscription_factory = Arc::new(subscription_factory);

        let (comms, dht) = initialize_comms(runtime.handle().clone(), config.comms_config.clone(), publisher)?;

        let fut = StackBuilder::new(runtime.handle().clone(), comms.shutdown_signal())
            .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
            .add_initializer(LivenessInitializer::new(
                LivenessConfig {
                    auto_ping_interval: Some(Duration::from_secs(30)),
                    enable_auto_join: false,
                    enable_auto_stored_message_request: false,
                    refresh_neighbours_interval: Default::default(),
                },
                Arc::clone(&subscription_factory),
                dht.dht_requester(),
            ))
            .add_initializer(OutputManagerServiceInitializer::new(
                output_manager_backend,
                factories.clone(),
            ))
            .add_initializer(TransactionServiceInitializer::new(
                TransactionServiceConfig::default(),
                subscription_factory.clone(),
                transaction_backend,
                comms.node_identity().clone(),
                factories.clone(),
            ))
            .add_initializer(ContactsServiceInitializer::new(contacts_backend))
            .finish();

        let handles = runtime.block_on(fut).expect("Service initialization failed");

        let output_manager_handle = handles
            .get_handle::<OutputManagerHandle>()
            .expect("Could not get Output Manager Service Handle");
        let mut transaction_service_handle = handles
            .get_handle::<TransactionServiceHandle>()
            .expect("Could not get Transaction Service Handle");
        let liveness_handle = handles
            .get_handle::<LivenessHandle>()
            .expect("Could not get Liveness Service Handle");
        let contacts_handle = handles
            .get_handle::<ContactsServiceHandle>()
            .expect("Could not get Contacts Service Handle");

        for p in base_node_peers {
            runtime.block_on(transaction_service_handle.set_base_node_public_key(p.public_key.clone()))?;
        }

        Ok(Wallet {
            comms,
            dht_service: dht,
            liveness_service: liveness_handle,
            output_manager_service: output_manager_handle,
            transaction_service: transaction_service_handle,
            contacts_service: contacts_handle,
            db,
            runtime,
            log_handle,
            #[cfg(feature = "test_harness")]
            transaction_backend: transaction_backend_handle,
            _u: PhantomData,
            _v: PhantomData,
            _w: PhantomData,
        })
    }

    /// This method consumes the wallet so that the handles are dropped which will result in the services async loops
    /// exiting.
    pub fn shutdown(self) -> Result<(), WalletError> {
        self.comms.shutdown()?;
        drop(self.runtime);
        Ok(())
    }

    /// This function will set the base_node that the wallet uses to broadcast transactions and monitor the blockchain
    /// state
    pub fn set_base_node_peer(&mut self, public_key: CommsPublicKey, net_address: String) -> Result<(), WalletError> {
        let address = net_address.parse::<Multiaddr>()?;
        let peer = Peer::new(
            public_key.clone(),
            NodeId::from_key(&public_key).unwrap(),
            vec![address].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
        );

        let existing_peers = self.runtime.block_on(self.db.get_peers())?;
        self.runtime.block_on(self.db.save_peer(peer.clone()))?;
        // Remove any peers in db to only persist a single peer at a time.
        for p in existing_peers {
            let _ = self.runtime.block_on(self.db.remove_peer(p.public_key.clone()))?;
        }

        self.comms.peer_manager().add_peer(peer.clone())?;
        self.runtime.block_on(
            self.transaction_service
                .set_base_node_public_key(peer.public_key.clone()),
        )?;

        Ok(())
    }

    /// Import an external spendable UTXO into the wallet. The output will be added to the Output Manager and made
    /// spendable. A faux incoming transaction will be created to provide a record of the event. The TxId of the
    /// generated transaction is returned.
    pub fn import_utxo(
        &mut self,
        amount: &MicroTari,
        spending_key: &PrivateKey,
        source_public_key: &CommsPublicKey,
        message: String,
    ) -> Result<TxId, WalletError>
    {
        let unblinded_output = UnblindedOutput::new(amount.clone(), spending_key.clone(), None);

        self.runtime
            .block_on(self.output_manager_service.add_output(unblinded_output))?;

        let tx_id = self.runtime.block_on(self.transaction_service.import_utxo(
            amount.clone(),
            source_public_key.clone(),
            message,
        ))?;

        info!(target: LOG_TARGET, "UTXO imported into wallet");

        Ok(tx_id)
    }

    pub fn sign_message(
        &mut self,
        secret: RistrettoSecretKey,
        nonce: RistrettoSecretKey,
        message: &str,
    ) -> Result<SchnorrSignature<RistrettoPublicKey, RistrettoSecretKey>, SchnorrSignatureError>
    {
        let challenge = Blake256::digest(message.clone().as_bytes());
        let signature = RistrettoSchnorr::sign(secret, nonce, challenge.clone().as_slice());
        signature
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
        let challenge = Blake256::digest(message.clone().as_bytes());
        signature.verify_challenge(&public_key, challenge.clone().as_slice())
    }
}

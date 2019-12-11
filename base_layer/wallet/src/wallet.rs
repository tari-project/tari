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
        OutputManagerConfig,
        OutputManagerServiceInitializer,
    },
    storage::database::{WalletBackend, WalletDatabase},
    transaction_service::{
        handle::TransactionServiceHandle,
        storage::database::TransactionBackend,
        TransactionServiceInitializer,
    },
};
use log::LevelFilter;
use log4rs::{
    append::file::FileAppender,
    config::{Appender, Config, Root},
    encode::pattern::PatternEncoder,
    Handle as LogHandle,
};
use std::{marker::PhantomData, sync::Arc};
use tari_comms::{
    builder::CommsNode,
    multiaddr::Multiaddr,
    peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
    types::{CommsPublicKey, CommsSecretKey},
};
use tari_comms_dht::Dht;
use tari_crypto::keys::PublicKey;
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization::{initialize_comms, CommsConfig},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessHandle, LivenessInitializer},
    },
};
use tari_service_framework::StackBuilder;
use tari_transactions::types::CryptoFactories;
use tokio::runtime::Runtime;

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
    T: WalletBackend,
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
    _u: PhantomData<U>,
    _v: PhantomData<V>,
    _w: PhantomData<W>,
}

impl<T, U, V, W> Wallet<T, U, V, W>
where
    T: WalletBackend,
    U: TransactionBackend + Clone + 'static,
    V: OutputManagerBackend + 'static,
    W: ContactsBackend + 'static,
{
    pub fn new(
        config: WalletConfig,
        runtime: Runtime,
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

        // TODO: Determine if there is KeyManager data stored in persistence and if so then construct the
        // OutputManagerConfig from that data At this stage a new random master key will be generated every
        // time the wallet starts up.
        let mut rng = rand::OsRng::new().unwrap();
        let (secret_key, _public_key): (CommsSecretKey, CommsPublicKey) = PublicKey::random_keypair(&mut rng);

        let oms_config = OutputManagerConfig {
            master_seed: secret_key,
            branch_seed: "".to_string(),
            primary_key_index: 0,
        };
        let factories = config.factories;
        let (publisher, subscription_factory) =
            pubsub_connector(runtime.executor(), config.comms_config.inbound_buffer_size);
        let subscription_factory = Arc::new(subscription_factory);

        let (comms, dht) = initialize_comms(runtime.executor(), config.comms_config.clone(), publisher)?;

        let fut = StackBuilder::new(runtime.executor(), comms.shutdown_signal())
            .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
            .add_initializer(LivenessInitializer::new(
                Default::default(),
                Arc::clone(&subscription_factory),
                dht.dht_requester(),
            ))
            .add_initializer(OutputManagerServiceInitializer::new(
                oms_config,
                output_manager_backend,
                factories.clone(),
            ))
            .add_initializer(TransactionServiceInitializer::new(
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
        let transaction_service_handle = handles
            .get_handle::<TransactionServiceHandle>()
            .expect("Could not get Transaction Service Handle");
        let liveness_handle = handles
            .get_handle::<LivenessHandle>()
            .expect("Could not get Liveness Service Handle");
        let contacts_handle = handles
            .get_handle::<ContactsServiceHandle>()
            .expect("Could not get Contacts Service Handle");

        Ok(Wallet {
            comms,
            dht_service: dht,
            liveness_service: liveness_handle,
            output_manager_service: output_manager_handle,
            transaction_service: transaction_service_handle,
            contacts_service: contacts_handle,
            db: WalletDatabase::new(wallet_backend),
            runtime,
            log_handle,
            _u: PhantomData,
            _v: PhantomData,
            _w: PhantomData,
        })
    }

    /// This method consumes the wallet so that the handles are dropped which will result in the services async loops
    /// exiting.
    pub fn shutdown(self) -> Result<(), WalletError> {
        self.comms.shutdown()?;
        self.runtime.shutdown_on_idle();
        Ok(())
    }

    /// This function will add a base_node
    pub fn add_base_node_peer(&mut self, public_key: CommsPublicKey, net_address: String) -> Result<(), WalletError> {
        let address = net_address.parse::<Multiaddr>()?;
        let peer = Peer::new(
            public_key.clone(),
            NodeId::from_key(&public_key).unwrap(),
            vec![address].into(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
        );

        self.comms.peer_manager().add_peer(peer.clone())?;

        self.db.save_peer(peer)?;

        Ok(())
    }
}

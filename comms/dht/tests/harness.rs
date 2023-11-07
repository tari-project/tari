// // Copyright 2023. The Tari Project
// //
// // Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// // following conditions are met:
// //
// // 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the
// following // disclaimer.
// //
// // 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// // following disclaimer in the documentation and/or other materials provided with the distribution.
// //
// // 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// // products derived from this software without specific prior written permission.
// //
// // THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// // INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// // DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// // SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// // SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// // WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// // USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{sync::Arc, time::Duration};

use rand::rngs::OsRng;
use tari_comms::{
    backoff::ConstantBackoff,
    peer_manager::{NodeIdentity, Peer, PeerFeatures},
    pipeline,
    pipeline::SinkService,
    protocol::{
        messaging::{MessagingEvent, MessagingEventSender, MessagingProtocolExtension},
        ProtocolId,
    },
    transports::MemoryTransport,
    types::CommsDatabase,
    CommsBuilder,
    CommsNode,
};
use tari_comms_dht::{inbound::DecryptedDhtMessage, DbConnectionUrl, Dht, DhtConfig};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBConfig},
    LMDBWrapper,
};
use tari_test_utils::{paths::create_temporary_data_path, random};
use tokio::{
    sync::{broadcast, mpsc},
    time,
};
use tower::ServiceBuilder;

pub struct TestNode {
    pub name: String,
    pub comms: CommsNode,
    pub dht: Dht,
    pub inbound_messages: mpsc::Receiver<DecryptedDhtMessage>,
    pub messaging_events: broadcast::Sender<MessagingEvent>,
    pub shutdown: Shutdown,
}

impl TestNode {
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        self.comms.node_identity()
    }

    pub fn to_peer(&self) -> Peer {
        self.comms.node_identity().to_peer()
    }

    #[allow(dead_code)]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[allow(dead_code)]
    pub async fn next_inbound_message(&mut self, timeout: Duration) -> Option<DecryptedDhtMessage> {
        time::timeout(timeout, self.inbound_messages.recv()).await.ok()?
    }

    pub async fn shutdown(mut self) {
        self.shutdown.trigger();
        self.comms.wait_until_shutdown().await;
    }
}

pub fn make_node_identity(features: PeerFeatures) -> Arc<NodeIdentity> {
    let port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(NodeIdentity::random(
        &mut OsRng,
        format!("/memory/{}", port).parse().unwrap(),
        features,
    ))
}

pub fn create_peer_storage() -> CommsDatabase {
    let database_name = random::string(8);
    let datastore = LMDBBuilder::new()
        .set_path(create_temporary_data_path())
        .set_env_config(LMDBConfig::default())
        .set_max_number_of_databases(1)
        .add_database(&database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(&database_name).unwrap();
    LMDBWrapper::new(Arc::new(peer_database))
}

pub async fn make_node<I: IntoIterator<Item = Peer>>(
    name: &str,
    features: PeerFeatures,
    dht_config: DhtConfig,
    known_peers: I,
) -> TestNode {
    let node_identity = make_node_identity(features);
    make_node_with_node_identity(name, node_identity, dht_config, known_peers).await
}

pub async fn make_node_with_node_identity<I: IntoIterator<Item = Peer>>(
    name: &str,
    node_identity: Arc<NodeIdentity>,
    dht_config: DhtConfig,
    known_peers: I,
) -> TestNode {
    let (tx, inbound_messages) = mpsc::channel(10);
    let shutdown = Shutdown::new();
    let (comms, dht, messaging_events) = setup_comms_dht(
        node_identity,
        create_peer_storage(),
        tx,
        known_peers.into_iter().collect(),
        dht_config,
        shutdown.to_signal(),
    )
    .await;

    TestNode {
        name: name.to_string(),
        comms,
        dht,
        inbound_messages,
        messaging_events,
        shutdown,
    }
}

pub async fn setup_comms_dht(
    node_identity: Arc<NodeIdentity>,
    storage: CommsDatabase,
    inbound_tx: mpsc::Sender<DecryptedDhtMessage>,
    peers: Vec<Peer>,
    dht_config: DhtConfig,
    shutdown_signal: ShutdownSignal,
) -> (CommsNode, Dht, MessagingEventSender) {
    // Create inbound and outbound channels
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let comms = CommsBuilder::new()
        .allow_test_addresses()
        // In this case the listener address and the public address are the same (/memory/...)
        .with_listener_address(node_identity.first_public_address().unwrap())
        .with_shutdown_signal(shutdown_signal)
        .with_node_identity(node_identity)
        .with_peer_storage(storage,None)
        .with_min_connectivity(1)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(100)))
        .build()
        .unwrap();

    let dht = Dht::builder()
        .with_config(dht_config)
        .with_database_url(DbConnectionUrl::MemoryShared(random::string(8)))
        .with_outbound_sender(outbound_tx)
        .build(
            comms.node_identity(),
            comms.peer_manager(),
            comms.connectivity(),
            comms.shutdown_signal(),
        )
        .await
        .unwrap();

    for peer in peers {
        comms.peer_manager().add_peer(peer).await.unwrap();
    }

    let dht_outbound_layer = dht.outbound_middleware_layer();
    let pipeline = pipeline::Builder::new()
        .with_outbound_pipeline(outbound_rx, |sink| {
            ServiceBuilder::new().layer(dht_outbound_layer).service(sink)
        })
        .max_concurrent_inbound_tasks(10)
        .with_inbound_pipeline(
            ServiceBuilder::new()
                .layer(dht.inbound_middleware_layer())
                .service(SinkService::new(inbound_tx)),
        )
        .build();

    let (event_tx, _) = broadcast::channel(100);
    let comms = comms
        .add_protocol_extension(
            MessagingProtocolExtension::new(ProtocolId::from_static(b"test"), event_tx.clone(), pipeline)
                .enable_message_received_event(),
        )
        .spawn_with_transport(MemoryTransport)
        .await
        .unwrap();

    (comms, dht, event_tx)
}

pub fn dht_config() -> DhtConfig {
    let mut config = DhtConfig::default_local_test();
    config.peer_validator_config.allow_test_addresses = true;
    config.saf.auto_request = false;
    config.discovery_request_timeout = Duration::from_secs(60);
    config.num_neighbouring_nodes = 8;
    config
}

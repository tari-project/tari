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
use tari_common_sqlite::connection::{DbConnection, DbConnectionUrl};
use tari_comms::{
    backoff::ConstantBackoff,
    multiaddr::Multiaddr,
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{
        database::{PeerDatabaseSql, MIGRATIONS},
        NodeId,
        NodeIdentity,
        Peer,
        PeerFeatures,
        PeerFlags,
    },
    pipeline,
    pipeline::SinkService,
    protocol::{
        messaging::{MessagingEvent, MessagingEventSender, MessagingProtocolExtension},
        ProtocolId,
    },
    transports::MemoryTransport,
    types::{CommsDatabase, CommsPublicKey},
    CommsBuilder,
    CommsNode,
};
use tari_comms_dht::{inbound::DecryptedDhtMessage, Dht, DhtConfig};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_test_utils::random;
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
    #[allow(dead_code)]
    pub messaging_events: broadcast::Sender<MessagingEvent>,
    pub shutdown: Shutdown,
}

impl TestNode {
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        self.comms.node_identity()
    }

    pub fn to_peer(&self) -> Peer {
        let mut peer = self.comms.node_identity().to_peer();
        let addresses: Vec<_> = peer.addresses.address_iter().cloned().collect();
        for addr in &addresses {
            peer.addresses.mark_last_seen_now(addr);
        }
        peer
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

pub fn create_test_peer() -> Peer {
    let mut rng = rand::rngs::OsRng;
    let (_sk, pk) = CommsPublicKey::random_keypair(&mut rng);
    let node_id = NodeId::from_key(&pk);
    let addresses = MultiaddressesWithStats::from_addresses_with_source(
        vec!["/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap()],
        &PeerAddressSource::Config,
    );
    Peer::new(
        pk,
        node_id,
        addresses,
        PeerFlags::default(),
        PeerFeatures::empty(),
        Default::default(),
        Default::default(),
    )
}

fn create_peer_storage() -> PeerDatabaseSql {
    let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
    PeerDatabaseSql::new(db_connection, &create_test_peer()).unwrap()
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
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();

    let comms = CommsBuilder::new()
        .allow_test_addresses()
        // In this case the listener address and the public address are the same (/memory/...)
        .with_listener_address(node_identity.first_public_address().unwrap())
        .with_shutdown_signal(shutdown_signal)
        .with_node_identity(node_identity)
        .with_peer_storage(storage)
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
        comms.peer_manager().add_or_update_peer(peer).await.unwrap();
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
    config.discovery_request_timeout = Duration::from_secs(60);
    config.num_neighbouring_nodes = 8;
    config
}

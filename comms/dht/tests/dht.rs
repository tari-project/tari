// Copyright 2019, The Tari Project
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

use std::{sync::Arc, time::Duration};

use rand::rngs::OsRng;
use tari_comms::{
    backoff::ConstantBackoff,
    connectivity::ConnectivityEvent,
    message::MessageExt,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures},
    pipeline,
    pipeline::SinkService,
    protocol::messaging::{MessagingEvent, MessagingEventSender, MessagingProtocolExtension},
    transports::MemoryTransport,
    types::CommsDatabase,
    wrap_in_envelope_body,
    CommsBuilder,
    CommsNode,
};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::{DhtMessageType, NodeDestination},
    event::DhtEvent,
    inbound::DecryptedDhtMessage,
    outbound::{OutboundEncryption, SendMessageParams},
    DbConnectionUrl,
    Dht,
    DhtConfig,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_storage::{
    lmdb_store::{LMDBBuilder, LMDBConfig},
    LMDBWrapper,
};
use tari_test_utils::{
    async_assert_eventually,
    collect_try_recv,
    paths::create_temporary_data_path,
    random,
    streams,
    unpack_enum,
};
use tokio::{
    sync::{broadcast, mpsc},
    time,
};
use tower::ServiceBuilder;

struct TestNode {
    name: String,
    comms: CommsNode,
    dht: Dht,
    inbound_messages: mpsc::Receiver<DecryptedDhtMessage>,
    messaging_events: broadcast::Sender<Arc<MessagingEvent>>,
    shutdown: Shutdown,
}

impl TestNode {
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        self.comms.node_identity()
    }

    pub fn to_peer(&self) -> Peer {
        self.comms.node_identity().to_peer()
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub async fn next_inbound_message(&mut self, timeout: Duration) -> Option<DecryptedDhtMessage> {
        time::timeout(timeout, self.inbound_messages.recv()).await.ok()?
    }

    pub async fn shutdown(mut self) {
        self.shutdown.trigger();
        self.comms.wait_until_shutdown().await;
    }
}

fn make_node_identity(features: PeerFeatures) -> Arc<NodeIdentity> {
    let port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(NodeIdentity::random(
        &mut OsRng,
        format!("/memory/{}", port).parse().unwrap(),
        features,
    ))
}

fn create_peer_storage() -> CommsDatabase {
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

async fn make_node<I: IntoIterator<Item = Peer>>(
    name: &str,
    features: PeerFeatures,
    dht_config: DhtConfig,
    known_peers: I,
) -> TestNode {
    let node_identity = make_node_identity(features);
    make_node_with_node_identity(name, node_identity, dht_config, known_peers).await
}

async fn make_node_with_node_identity<I: IntoIterator<Item = Peer>>(
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

async fn setup_comms_dht(
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
        .with_listener_address(node_identity.public_address())
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
        .outbound_buffer_size(10)
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
        .add_protocol_extension(MessagingProtocolExtension::new(event_tx.clone(), pipeline))
        .spawn_with_transport(MemoryTransport)
        .await
        .unwrap();

    (comms, dht, event_tx)
}

fn dht_config() -> DhtConfig {
    let mut config = DhtConfig::default_local_test();
    config.allow_test_addresses = true;
    config.saf_config.auto_request = false;
    config.discovery_request_timeout = Duration::from_secs(60);
    config.num_neighbouring_nodes = 8;
    config
}

#[tokio::test]
#[allow(non_snake_case)]
async fn dht_join_propagation() {
    // Create 3 nodes where only Node B knows A and C, but A and C want to talk to each other

    // Node C knows no one
    let node_C = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node B knows about Node C
    let node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;

    node_A
        .comms
        .connectivity()
        .wait_for_connectivity(Duration::from_secs(10))
        .await
        .unwrap();
    node_B
        .comms
        .connectivity()
        .wait_for_connectivity(Duration::from_secs(10))
        .await
        .unwrap();
    // Send a join request from Node A, through B to C. As all Nodes are in the same network region, once
    // Node C receives the join request from Node A, it will send a direct join request back
    // to A.
    node_A.dht.dht_requester().send_join().await.unwrap();

    let node_B_peer_manager = node_B.comms.peer_manager();
    let node_C_peer_manager = node_C.comms.peer_manager();

    // Check that Node B and C know node A
    async_assert_eventually!(
        node_B_peer_manager.exists(node_A.node_identity().public_key()).await,
        expect = true,
        max_attempts = 10,
        interval = Duration::from_millis(1000)
    );
    async_assert_eventually!(
        node_C_peer_manager.exists(node_A.node_identity().public_key()).await,
        expect = true,
        max_attempts = 10,
        interval = Duration::from_millis(500)
    );

    let node_A_peer = node_C_peer_manager
        .find_by_public_key(node_A.node_identity().public_key())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(node_A_peer.features, node_A.comms.node_identity().features());

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

#[tokio::test]
#[allow(non_snake_case)]
async fn dht_discover_propagation() {
    // Create 4 nodes where A knows B, B knows A and C, C knows B and D, and D knows C

    // Node D knows no one
    let node_D = make_node("node_D", PeerFeatures::COMMUNICATION_CLIENT, dht_config(), None).await;
    // Node C knows about Node D
    let node_C = make_node(
        "node_C",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_D.to_peer()),
    )
    .await;
    // Node B knows about Node C
    let node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;
    log::info!(
        "NodeA = {}, NodeB = {}, Node C = {}, Node D = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
        node_D.node_identity().node_id().short_str(),
    );

    // To receive messages, clients have to connect
    node_D.comms.peer_manager().add_peer(node_C.to_peer()).await.unwrap();
    node_D
        .comms
        .connectivity()
        .dial_peer(node_C.comms.node_identity().node_id().clone())
        .await
        .unwrap();

    // Send a discover request from Node A, through B and C, to D. Once Node D
    // receives the discover request from Node A, it should send a  discovery response
    // request back to A at which time this call will resolve (or timeout).
    node_A
        .dht
        .discovery_service_requester()
        .discover_peer(
            node_D.node_identity().public_key().clone(),
            node_D.node_identity().node_id().clone().into(),
        )
        .await
        .unwrap();

    let node_A_peer_manager = node_A.comms.peer_manager();
    let node_B_peer_manager = node_B.comms.peer_manager();
    let node_C_peer_manager = node_C.comms.peer_manager();
    let node_D_peer_manager = node_D.comms.peer_manager();

    // Check that all the nodes know about each other in the chain and the discovery worked
    assert!(node_A_peer_manager.exists(node_D.node_identity().public_key()).await);
    assert!(node_B_peer_manager.exists(node_A.node_identity().public_key()).await);
    assert!(node_C_peer_manager.exists(node_B.node_identity().public_key()).await);
    assert!(node_D_peer_manager.exists(node_C.node_identity().public_key()).await);
    assert!(node_D_peer_manager.exists(node_A.node_identity().public_key()).await);
}

#[tokio::test]
#[allow(non_snake_case)]
async fn dht_store_forward() {
    let node_C_node_identity = make_node_identity(PeerFeatures::COMMUNICATION_NODE);
    // Node B knows about Node C
    let node_B = make_node("node_B", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;
    log::info!(
        "NodeA = {}, NodeB = {}, Node C = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C_node_identity.node_id().short_str(),
    );

    node_A
        .comms
        .connectivity()
        .wait_for_connectivity(Duration::from_secs(10))
        .await
        .unwrap();

    let params = SendMessageParams::new()
        .broadcast(vec![])
        .with_encryption(OutboundEncryption::encrypt_for(
            node_C_node_identity.public_key().clone(),
        ))
        .with_destination(node_C_node_identity.node_id().clone().into())
        .finish();

    let secret_msg1 = b"NCZW VUSX PNYM INHZ XMQX SFWX WLKJ AHSH";
    let secret_msg2 = b"NMCO CCAK UQPM KCSM HKSE INJU SBLK";

    let mut node_B_msg_events = node_B.messaging_events.subscribe();

    node_A
        .dht
        .outbound_requester()
        .send_raw(
            params.clone(),
            wrap_in_envelope_body!(secret_msg1.to_vec()).to_encoded_bytes(),
        )
        .await
        .unwrap();
    node_A
        .dht
        .outbound_requester()
        .send_raw(params, wrap_in_envelope_body!(secret_msg2.to_vec()).to_encoded_bytes())
        .await
        .unwrap();

    // Wait for node B to receive 2 propagation messages
    collect_try_recv!(node_B_msg_events, take = 2, timeout = Duration::from_secs(20));

    let mut node_C =
        make_node_with_node_identity("node_C", node_C_node_identity, dht_config(), Some(node_B.to_peer())).await;
    let mut node_C_dht_events = node_C.dht.subscribe_dht_events();
    let mut node_C_msg_events = node_C.messaging_events.subscribe();
    // Ask node B for messages
    node_C
        .dht
        .store_and_forward_requester()
        .request_saf_messages_from_peer(node_B.node_identity().node_id().clone())
        .await
        .unwrap();
    node_C
        .dht
        .store_and_forward_requester()
        .request_saf_messages_from_peer(node_A.node_identity().node_id().clone())
        .await
        .unwrap();
    // Wait for node C to and receive a response from the SAF request
    let event = collect_try_recv!(node_C_msg_events, take = 1, timeout = Duration::from_secs(20));
    unpack_enum!(MessagingEvent::MessageReceived(_node_id, _msg) = &*event.get(0).unwrap().as_ref());

    let msg = node_C.next_inbound_message(Duration::from_secs(5)).await.unwrap();
    assert_eq!(
        msg.authenticated_origin.as_ref().unwrap(),
        node_A.comms.node_identity().public_key()
    );
    let mut msgs = vec![secret_msg1.to_vec(), secret_msg2.to_vec()];
    let secret = msg.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap();
    {
        let pos = msgs.iter().position(|m| m == &secret).unwrap();
        msgs.remove(pos);
    }

    let msg = node_C.next_inbound_message(Duration::from_secs(5)).await.unwrap();
    assert_eq!(
        msg.authenticated_origin.as_ref().unwrap(),
        node_A.comms.node_identity().public_key()
    );
    let secret = msg.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap();
    {
        let pos = msgs.iter().position(|m| m == &secret).unwrap();
        msgs.remove(pos);
    }

    assert!(msgs.is_empty());

    // Check that Node C emitted the StoreAndForwardMessagesReceived event when it went Online
    let event = collect_try_recv!(node_C_dht_events, take = 1, timeout = Duration::from_secs(20));
    unpack_enum!(DhtEvent::StoreAndForwardMessagesReceived = &*event.get(0).unwrap().as_ref());

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

#[tokio::test]
#[allow(non_snake_case)]
async fn dht_propagate_dedup() {
    let mut config = dht_config();
    // For this test we want to exactly measure the path of a message, so we disable repropagation of messages (i.e
    // allow 1 occurrence)
    config.dedup_allowed_message_occurrences = 1;
    // Node D knows no one
    let mut node_D = make_node("node_D", PeerFeatures::COMMUNICATION_NODE, config.clone(), None).await;
    // Node C knows about Node D
    let mut node_C = make_node(
        "node_C",
        PeerFeatures::COMMUNICATION_NODE,
        config.clone(),
        Some(node_D.to_peer()),
    )
    .await;
    // Node B knows about Node C
    let mut node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        config.clone(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B and C
    let mut node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        config.clone(),
        Some(node_B.to_peer()),
    )
    .await;
    node_A.comms.peer_manager().add_peer(node_C.to_peer()).await.unwrap();
    log::info!(
        "NodeA = {}, NodeB = {}, Node C = {}, Node D = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
        node_D.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    async fn connect_nodes(node1: &mut TestNode, node2: &mut TestNode) {
        node1
            .comms
            .connectivity()
            .dial_peer(node2.node_identity().node_id().clone())
            .await
            .unwrap();
    }
    // Pre-connect nodes, this helps message passing be more deterministic
    connect_nodes(&mut node_A, &mut node_B).await;
    connect_nodes(&mut node_A, &mut node_C).await;
    connect_nodes(&mut node_B, &mut node_C).await;
    connect_nodes(&mut node_C, &mut node_D).await;

    let mut node_A_messaging = node_A.messaging_events.subscribe();
    let mut node_B_messaging = node_B.messaging_events.subscribe();
    let mut node_B_messaging2 = node_B.messaging_events.subscribe();
    let mut node_C_messaging = node_C.messaging_events.subscribe();
    let mut node_C_messaging2 = node_C.messaging_events.subscribe();
    let mut node_D_messaging = node_D.messaging_events.subscribe();
    let mut node_D_messaging2 = node_D.messaging_events.subscribe();

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    let out_msg = OutboundDomainMessage::new(123, Person {
        name: "John Conway".into(),
        age: 82,
    });
    node_A
        .dht
        .outbound_requester()
        .propagate(
            node_D.node_identity().node_id().clone().into(),
            OutboundEncryption::encrypt_for(node_D.node_identity().public_key().clone()),
            vec![],
            out_msg,
        )
        .await
        .unwrap();

    let msg = node_D
        .next_inbound_message(Duration::from_secs(10))
        .await
        .expect("Node D expected an inbound message but it never arrived");
    assert!(msg.decryption_succeeded());
    log::info!("Received message {}", msg.tag);
    let person = msg
        .decryption_result
        .unwrap()
        .decode_part::<Person>(1)
        .unwrap()
        .unwrap();
    assert_eq!(person.name, "John Conway");

    let node_A_id = node_A.node_identity().node_id().clone();
    let node_B_id = node_B.node_identity().node_id().clone();
    let node_C_id = node_C.node_identity().node_id().clone();
    let node_D_id = node_D.node_identity().node_id().clone();

    // Ensure that the message has propagated before disconnecting everyone
    let _ = node_B_messaging2.recv().await.unwrap();
    let _ = node_C_messaging2.recv().await.unwrap();
    let _ = node_D_messaging2.recv().await.unwrap();

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
    node_D.shutdown().await;

    // Check the message flow BEFORE deduping
    let received = filter_received(collect_try_recv!(node_A_messaging, timeout = Duration::from_secs(20)));
    // Expected race condition: If A->(B|C)->(C|B) before A->(C|B) then (C|B)->A
    if !received.is_empty() {
        assert_eq!(count_messages_received(&received, &[&node_B_id, &node_C_id]), 1);
    }

    let received = filter_received(collect_try_recv!(node_B_messaging, timeout = Duration::from_secs(20)));
    let recv_count = count_messages_received(&received, &[&node_A_id, &node_C_id]);
    // Expected race condition: If A->B->C before A->C then C->B does not happen
    assert!(
        (1..=2).contains(&recv_count),
        "expected recv_count to be in [1-2] but was {}",
        recv_count
    );

    let received = filter_received(collect_try_recv!(node_C_messaging, timeout = Duration::from_secs(20)));
    let recv_count = count_messages_received(&received, &[&node_A_id, &node_B_id]);
    assert_eq!(recv_count, 2);
    assert_eq!(count_messages_received(&received, &[&node_D_id]), 0);

    let received = filter_received(collect_try_recv!(node_D_messaging, timeout = Duration::from_secs(20)));
    assert_eq!(received.len(), 1);
    assert_eq!(count_messages_received(&received, &[&node_C_id]), 1);
}

#[tokio::test]
#[allow(non_snake_case)]
async fn dht_do_not_store_invalid_message_in_dedup() {
    let mut config = dht_config();
    config.dedup_allowed_message_occurrences = 1;

    // Node C receives messages from A and B
    let mut node_C = make_node("node_B", PeerFeatures::COMMUNICATION_NODE, config.clone(), None).await;

    // Node B forwards a message from A but modifies it
    let mut node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        config.clone(),
        Some(node_C.to_peer()),
    )
    .await;

    // Node A creates a message sends it to B, B modifies it, sends it to C; Node A sends message to C
    let node_A = make_node("node_A", PeerFeatures::COMMUNICATION_NODE, config.clone(), [
        node_B.to_peer(),
        node_C.to_peer(),
    ])
    .await;

    log::info!(
        "NodeA = {}, NodeB = {}, NodeC = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    node_A
        .comms
        .connectivity()
        .dial_peer(node_B.node_identity().node_id().clone())
        .await
        .unwrap();

    node_A
        .comms
        .connectivity()
        .dial_peer(node_C.node_identity().node_id().clone())
        .await
        .unwrap();

    node_B
        .comms
        .connectivity()
        .dial_peer(node_C.node_identity().node_id().clone())
        .await
        .unwrap();

    let mut node_C_messaging = node_C.messaging_events.subscribe();

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    // Just a message to test connectivity between Node A -> Node C, and to get the header from
    let out_msg = OutboundDomainMessage::new(123, Person {
        name: "John Conway".into(),
        age: 82,
    });

    node_A
        .dht
        .outbound_requester()
        .send_message(
            SendMessageParams::new()
                .direct_node_id(node_B.node_identity().node_id().clone())
                .with_destination(node_C.node_identity().public_key().clone().into())
                .force_origin()
                .finish(),
            out_msg,
        )
        .await
        .unwrap();

    // Get the message that was received by Node B
    let mut msg = node_B.next_inbound_message(Duration::from_secs(10)).await.unwrap();
    let bytes = msg.decryption_result.unwrap().to_encoded_bytes();

    // Clone header without modification
    let header_unmodified = msg.dht_header.clone();

    // Modify the header
    msg.dht_header.message_type = DhtMessageType::from_i32(3i32).unwrap();

    // Forward modified message to Node C - Should get us banned
    node_B
        .dht
        .outbound_requester()
        .send_raw(
            SendMessageParams::new()
                .direct_node_id(node_C.node_identity().node_id().clone())
                .with_dht_header(msg.dht_header)
                .finish(),
            bytes.clone(),
        )
        .await
        .unwrap();

    async_assert_eventually!(
        {
            let n = node_C
                .comms
                .peer_manager()
                .find_by_node_id(node_B.node_identity().node_id())
                .await
                .unwrap()
                .unwrap();
            n.is_banned()
        },
        expect = true,
        max_attempts = 10,
        interval = Duration::from_secs(3)
    );

    node_A
        .dht
        .outbound_requester()
        .send_raw(
            SendMessageParams::new()
                .direct_node_id(node_C.node_identity().node_id().clone())
                .with_dht_header(header_unmodified)
                .finish(),
            bytes,
        )
        .await
        .unwrap();

    // Node C receives the correct message from Node A
    let msg = node_C
        .next_inbound_message(Duration::from_secs(10))
        .await
        .expect("Node C expected an inbound message but it never arrived");
    assert!(msg.decryption_succeeded());
    log::info!("Received message {}", msg.tag);
    let person = msg
        .decryption_result
        .unwrap()
        .decode_part::<Person>(1)
        .unwrap()
        .unwrap();
    assert_eq!(person.name, "John Conway");

    let node_A_id = node_A.node_identity().node_id().clone();
    let node_B_id = node_B.node_identity().node_id().clone();

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;

    // Check the message flow BEFORE deduping
    let received = filter_received(collect_try_recv!(node_C_messaging, timeout = Duration::from_secs(20)));

    let received_from_a = count_messages_received(&received, &[&node_A_id]);
    let received_from_b = count_messages_received(&received, &[&node_B_id]);

    assert_eq!(received_from_a, 1);
    assert_eq!(received_from_b, 1);
}

#[tokio::test]
#[allow(non_snake_case)]
async fn dht_repropagate() {
    let mut config = dht_config();
    config.dedup_allowed_message_occurrences = 3;
    let mut node_C = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, config.clone(), []).await;
    let mut node_B = make_node("node_B", PeerFeatures::COMMUNICATION_NODE, config.clone(), [
        node_C.to_peer()
    ])
    .await;
    let mut node_A = make_node("node_A", PeerFeatures::COMMUNICATION_NODE, config, [
        node_B.to_peer(),
        node_C.to_peer(),
    ])
    .await;
    node_A.comms.peer_manager().add_peer(node_C.to_peer()).await.unwrap();
    node_B.comms.peer_manager().add_peer(node_C.to_peer()).await.unwrap();
    node_C.comms.peer_manager().add_peer(node_A.to_peer()).await.unwrap();
    node_C.comms.peer_manager().add_peer(node_B.to_peer()).await.unwrap();
    log::info!(
        "NodeA = {}, NodeB = {}, Node C = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
        node_C.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    async fn connect_nodes(node1: &mut TestNode, node2: &mut TestNode) {
        node1
            .comms
            .connectivity()
            .dial_peer(node2.node_identity().node_id().clone())
            .await
            .unwrap();
    }
    // Pre-connect nodes, this helps message passing be more deterministic
    connect_nodes(&mut node_A, &mut node_B).await;
    connect_nodes(&mut node_A, &mut node_C).await;
    connect_nodes(&mut node_B, &mut node_C).await;

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    let out_msg = OutboundDomainMessage::new(123, Person {
        name: "Alan Turing".into(),
        age: 41,
    });
    node_A
        .dht
        .outbound_requester()
        .propagate(
            NodeDestination::Unknown,
            OutboundEncryption::ClearText,
            vec![],
            out_msg.clone(),
        )
        .await
        .unwrap();

    async fn receive_and_repropagate(node: &mut TestNode, out_msg: &OutboundDomainMessage<Person>) {
        let msg = node
            .next_inbound_message(Duration::from_secs(10))
            .await
            .unwrap_or_else(|| panic!("{} expected an inbound message but it never arrived", node.name()));
        log::info!("Received message {}", msg.tag);

        node.dht
            .outbound_requester()
            .send_message(
                SendMessageParams::new()
                    .propagate(NodeDestination::Unknown, vec![])
                    .with_destination(NodeDestination::Unknown)
                    .with_tag(msg.tag)
                    .finish(),
                out_msg.clone(),
            )
            .await
            .unwrap()
            .resolve()
            .await
            .unwrap();
    }

    // This relies on the DHT being set with .with_dedup_discard_hit_count(3)
    receive_and_repropagate(&mut node_B, &out_msg).await;
    receive_and_repropagate(&mut node_C, &out_msg).await;
    receive_and_repropagate(&mut node_A, &out_msg).await;
    receive_and_repropagate(&mut node_B, &out_msg).await;
    receive_and_repropagate(&mut node_C, &out_msg).await;
    receive_and_repropagate(&mut node_A, &out_msg).await;
    receive_and_repropagate(&mut node_B, &out_msg).await;
    receive_and_repropagate(&mut node_C, &out_msg).await;

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

#[tokio::test]
#[allow(non_snake_case)]
async fn dht_propagate_message_contents_not_malleable_ban() {
    let node_C = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node B knows about Node C
    let mut node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;
    node_A.comms.peer_manager().add_peer(node_C.to_peer()).await.unwrap();
    log::info!(
        "NodeA = {}, NodeB = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    node_A
        .comms
        .connectivity()
        .dial_peer(node_B.node_identity().node_id().clone())
        .await
        .unwrap();

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    let out_msg = Person {
        name: "John Conway".into(),
        age: 82,
    };
    node_A
        .dht
        .outbound_requester()
        .send_message_no_header(
            SendMessageParams::new()
                .direct_node_id(node_B.node_identity().node_id().clone())
                .with_destination(node_A.node_identity().node_id().clone().into())
                .with_encryption(OutboundEncryption::ClearText)
                .force_origin()
                .finish(),
            out_msg,
        )
        .await
        .unwrap();

    let msg = node_B.next_inbound_message(Duration::from_secs(10)).await.unwrap();

    let mut bytes = msg.decryption_result.unwrap().to_encoded_bytes();
    // Change the message
    bytes.push(0x42);

    let mut connectivity_events = node_C.comms.connectivity().get_event_subscription();

    // Propagate the changed message (to node C)
    node_B
        .dht
        .outbound_requester()
        .send_raw(
            SendMessageParams::new()
                .propagate(node_B.node_identity().node_id().clone().into(), vec![msg
                    .source_peer
                    .node_id
                    .clone()])
                .with_dht_header(msg.dht_header)
                .finish(),
            bytes,
        )
        .await
        .unwrap();
    let node_B_node_id = node_B.node_identity().node_id().clone();

    // Node C should ban node B
    let banned_node_id = streams::assert_in_broadcast(
        &mut connectivity_events,
        |r| match r {
            ConnectivityEvent::PeerBanned(node_id) => Some(node_id),
            _ => None,
        },
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(banned_node_id, node_B_node_id);

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

#[tokio::test]
#[allow(non_snake_case)]
async fn dht_header_not_malleable() {
    env_logger::init();
    let node_C = make_node("node_C", PeerFeatures::COMMUNICATION_NODE, dht_config(), None).await;
    // Node B knows about Node C
    let mut node_B = make_node(
        "node_B",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_C.to_peer()),
    )
    .await;
    // Node A knows about Node B
    let node_A = make_node(
        "node_A",
        PeerFeatures::COMMUNICATION_NODE,
        dht_config(),
        Some(node_B.to_peer()),
    )
    .await;
    node_A.comms.peer_manager().add_peer(node_C.to_peer()).await.unwrap();
    log::info!(
        "NodeA = {}, NodeB = {}",
        node_A.node_identity().node_id().short_str(),
        node_B.node_identity().node_id().short_str(),
    );

    // Connect the peers that should be connected
    node_A
        .comms
        .connectivity()
        .dial_peer(node_B.node_identity().node_id().clone())
        .await
        .unwrap();

    #[derive(Clone, PartialEq, ::prost::Message)]
    struct Person {
        #[prost(string, tag = "1")]
        name: String,
        #[prost(uint32, tag = "2")]
        age: u32,
    }

    let out_msg = Person {
        name: "John Conway".into(),
        age: 82,
    };
    node_A
        .dht
        .outbound_requester()
        .send_message_no_header(
            SendMessageParams::new()
                .direct_node_id(node_B.node_identity().node_id().clone())
                .with_destination(node_A.node_identity().node_id().clone().into())
                .with_encryption(OutboundEncryption::ClearText)
                .force_origin()
                .finish(),
            out_msg,
        )
        .await
        .unwrap();

    let mut msg = node_B.next_inbound_message(Duration::from_secs(10)).await.unwrap();

    // Modify the header
    msg.dht_header.message_type = DhtMessageType::from_i32(21i32).unwrap();

    let bytes = msg.decryption_result.unwrap().to_encoded_bytes();
    let mut connectivity_events = node_C.comms.connectivity().get_event_subscription();

    // Propagate the changed message (to node C)
    node_B
        .dht
        .outbound_requester()
        .send_raw(
            SendMessageParams::new()
                .propagate(node_B.node_identity().node_id().clone().into(), vec![msg
                    .source_peer
                    .node_id
                    .clone()])
                .with_dht_header(msg.dht_header)
                .finish(),
            bytes,
        )
        .await
        .unwrap();
    let node_B_node_id = node_B.node_identity().node_id().clone();

    // Node C should ban node B
    let banned_node_id = streams::assert_in_broadcast(
        &mut connectivity_events,
        |r| match r {
            ConnectivityEvent::PeerBanned(node_id) => Some(node_id),
            _ => None,
        },
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(banned_node_id, node_B_node_id);

    node_A.shutdown().await;
    node_B.shutdown().await;
    node_C.shutdown().await;
}

fn filter_received(events: Vec<Arc<MessagingEvent>>) -> Vec<Arc<MessagingEvent>> {
    events
        .into_iter()
        .filter(|e| match &**e {
            MessagingEvent::MessageReceived(_, _) => true,
            _ => unreachable!(),
        })
        .collect()
}

fn count_messages_received(events: &[Arc<MessagingEvent>], node_ids: &[&NodeId]) -> usize {
    events
        .iter()
        .filter(|event| {
            unpack_enum!(MessagingEvent::MessageReceived(recv_node_id, _tag) = &***event);
            node_ids.iter().any(|n| &*recv_node_id == *n)
        })
        .count()
}

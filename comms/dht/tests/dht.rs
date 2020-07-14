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

use futures::{channel::mpsc, StreamExt};
use rand::rngs::OsRng;
use std::{sync::Arc, time::Duration};
use tari_comms::{
    backoff::ConstantBackoff,
    message::MessageExt,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures},
    pipeline,
    pipeline::SinkService,
    protocol::messaging::MessagingEvent,
    transports::MemoryTransport,
    types::CommsDatabase,
    wrap_in_envelope_body,
    CommsBuilder,
    CommsNode,
};
use tari_comms_dht::{
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    inbound::DecryptedDhtMessage,
    outbound::{OutboundEncryption, SendMessageParams},
    DbConnectionUrl,
    Dht,
    DhtBuilder,
};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tari_test_utils::{
    async_assert_eventually,
    collect_stream,
    paths::create_temporary_data_path,
    random,
    unpack_enum,
};
use tokio::time;
use tower::ServiceBuilder;

struct TestNode {
    comms: CommsNode,
    dht: Dht,
    ims_rx: mpsc::Receiver<DecryptedDhtMessage>,
}

impl TestNode {
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        self.comms.node_identity()
    }

    pub fn to_peer(&self) -> Peer {
        self.comms.node_identity().to_peer()
    }

    pub async fn next_inbound_message(&mut self, timeout: Duration) -> Option<DecryptedDhtMessage> {
        time::timeout(timeout, self.ims_rx.next()).await.ok()?
    }
}

fn make_node_identity(features: PeerFeatures) -> Arc<NodeIdentity> {
    let port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(NodeIdentity::random(&mut OsRng, format!("/memory/{}", port).parse().unwrap(), features).unwrap())
}

fn create_peer_storage() -> CommsDatabase {
    let database_name = random::string(8);
    let datastore = LMDBBuilder::new()
        .set_path(create_temporary_data_path())
        .set_environment_size(50)
        .set_max_number_of_databases(1)
        .add_database(&database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(&database_name).unwrap();
    LMDBWrapper::new(Arc::new(peer_database))
}

async fn make_node(features: PeerFeatures, seed_peer: Option<Peer>) -> TestNode {
    let node_identity = make_node_identity(features);
    make_node_with_node_identity(node_identity, seed_peer).await
}

async fn make_node_with_node_identity(node_identity: Arc<NodeIdentity>, seed_peer: Option<Peer>) -> TestNode {
    let (tx, ims_rx) = mpsc::channel(10);
    let (comms, dht) = setup_comms_dht(
        node_identity,
        create_peer_storage(),
        tx,
        seed_peer.into_iter().collect(),
    )
    .await;

    TestNode { comms, dht, ims_rx }
}

async fn setup_comms_dht(
    node_identity: Arc<NodeIdentity>,
    storage: CommsDatabase,
    inbound_tx: mpsc::Sender<DecryptedDhtMessage>,
    peers: Vec<Peer>,
) -> (CommsNode, Dht)
{
    // Create inbound and outbound channels
    let (outbound_tx, outbound_rx) = mpsc::channel(10);

    let comms = CommsBuilder::new()
        .allow_test_addresses()
        // In this case the listener address and the public address are the same (/memory/...)
        .with_listener_address(node_identity.public_address())
        .with_transport(MemoryTransport)
        .with_node_identity(node_identity)
        .with_peer_storage(storage)
        .with_dial_backoff(ConstantBackoff::new(Duration::from_millis(100)))
        .build()
        .unwrap();

    let dht = DhtBuilder::new(
        comms.node_identity(),
        comms.peer_manager(),
        outbound_tx,
        comms.connectivity(),
        comms.shutdown_signal(),
    )
    .local_test()
    .disable_auto_store_and_forward_requests()
    .with_database_url(DbConnectionUrl::MemoryShared(random::string(8)))
    .with_discovery_timeout(Duration::from_secs(60))
    .with_num_neighbouring_nodes(8)
    .finish()
    .await
    .unwrap();

    for peer in peers {
        comms.peer_manager().add_peer(peer).await.unwrap();
    }

    let dht_outbound_layer = dht.outbound_middleware_layer();

    let comms = comms
        .with_messaging_pipeline(
            pipeline::Builder::new()
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
                .finish(),
        )
        .spawn()
        .await
        .unwrap();

    (comms, dht)
}

#[tokio_macros::test]
#[allow(non_snake_case)]
async fn dht_join_propagation() {
    // Create 3 nodes where only Node B knows A and C, but A and C want to talk to each other

    // Node C knows no one
    let node_C = make_node(PeerFeatures::COMMUNICATION_NODE, None).await;
    // Node B knows about Node C
    let node_B = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_C.to_peer())).await;
    // Node A knows about Node B
    let node_A = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_B.to_peer())).await;

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

    let node_A_peer_manager = node_A.comms.peer_manager();
    let node_C_peer_manager = node_C.comms.peer_manager();

    // Check that Node A knows about Node C and vice versa
    async_assert_eventually!(
        node_A_peer_manager.exists(node_C.node_identity().public_key()).await,
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

    let node_C_peer = node_A_peer_manager
        .find_by_public_key(node_C.node_identity().public_key())
        .await
        .unwrap();
    assert_eq!(node_C_peer.features, node_C.comms.node_identity().features());

    node_A.comms.shutdown().await;
    node_B.comms.shutdown().await;
    node_C.comms.shutdown().await;
}

#[tokio_macros::test]
#[allow(non_snake_case)]
async fn dht_discover_propagation() {
    // Create 4 nodes where A knows B, B knows A and C, C knows B and D, and D knows C

    // Node D knows no one
    let node_D = make_node(PeerFeatures::COMMUNICATION_CLIENT, None).await;
    // Node C knows about Node D
    let node_C = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_D.to_peer())).await;
    // Node B knows about Node C
    let node_B = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_C.to_peer())).await;
    // Node A knows about Node B
    let node_A = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_B.to_peer())).await;
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
        .connection_manager()
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
            Box::new(node_D.node_identity().public_key().clone()),
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

    node_A.comms.shutdown().await;
    node_B.comms.shutdown().await;
    node_C.comms.shutdown().await;
    node_D.comms.shutdown().await;
}

#[tokio_macros::test]
#[allow(non_snake_case)]
async fn dht_store_forward() {
    let node_C_node_identity = make_node_identity(PeerFeatures::COMMUNICATION_NODE);
    // Node B knows about Node C
    let node_B = make_node(PeerFeatures::COMMUNICATION_NODE, None).await;
    // Node A knows about Node B
    let node_A = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_B.to_peer())).await;
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

    let dest_public_key = Box::new(node_C_node_identity.public_key().clone());
    let params = SendMessageParams::new()
        .broadcast(vec![])
        .with_encryption(OutboundEncryption::EncryptFor(dest_public_key))
        .with_destination(NodeDestination::NodeId(Box::new(
            node_C_node_identity.node_id().clone(),
        )))
        .finish();

    let secret_msg1 = b"NCZW VUSX PNYM INHZ XMQX SFWX WLKJ AHSH";
    let secret_msg2 = b"NMCO CCAK UQPM KCSM HKSE INJU SBLK";

    let mut node_B_msg_events = node_B.comms.subscribe_messaging_events();
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
    collect_stream!(node_B_msg_events, take = 2, timeout = Duration::from_secs(20));

    let mut node_C = make_node_with_node_identity(node_C_node_identity, Some(node_B.to_peer())).await;
    let mut node_C_msg_events = node_C.comms.subscribe_messaging_events();
    // Ask node B for messages
    node_C
        .dht
        .store_and_forward_requester()
        .request_saf_messages_from_peer(node_B.node_identity().node_id().clone())
        .await
        .unwrap();
    // Wait for node C to send 1 SAF request, and receive a response
    collect_stream!(node_C_msg_events, take = 2, timeout = Duration::from_secs(20));

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

    node_A.comms.shutdown().await;
    node_B.comms.shutdown().await;
    node_C.comms.shutdown().await;
}

#[tokio_macros::test]
#[allow(non_snake_case)]
async fn dht_propagate_dedup() {
    // Node D knows no one
    let mut node_D = make_node(PeerFeatures::COMMUNICATION_NODE, None).await;
    // Node C knows about Node D
    let mut node_C = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_D.to_peer())).await;
    // Node B knows about Node C
    let mut node_B = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_C.to_peer())).await;
    // Node A knows about Node B and C
    let mut node_A = make_node(PeerFeatures::COMMUNICATION_NODE, Some(node_B.to_peer())).await;
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
            .connection_manager()
            .dial_peer(node2.node_identity().node_id().clone())
            .await
            .unwrap();
    }
    // Pre-connect nodes, this helps message passing be more deterministic
    connect_nodes(&mut node_A, &mut node_B).await;
    connect_nodes(&mut node_A, &mut node_C).await;
    connect_nodes(&mut node_B, &mut node_C).await;
    connect_nodes(&mut node_C, &mut node_D).await;

    let mut node_A_messaging = node_A.comms.subscribe_messaging_events();
    let mut node_B_messaging = node_B.comms.subscribe_messaging_events();
    let mut node_C_messaging = node_C.comms.subscribe_messaging_events();
    let mut node_D_messaging = node_D.comms.subscribe_messaging_events();

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
            // Node D is a client node, so an destination is required for domain messages
            NodeDestination::Unknown, // NodeId(Box::new(node_D.node_identity().node_id().clone())),
            OutboundEncryption::EncryptFor(Box::new(node_D.node_identity().public_key().clone())),
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

    node_A.comms.shutdown().await;
    node_B.comms.shutdown().await;
    node_C.comms.shutdown().await;
    node_D.comms.shutdown().await;

    // Check the message flow BEFORE deduping
    let (sent, received) = partition_events(collect_stream!(node_A_messaging, timeout = Duration::from_secs(20)));
    assert_eq!(sent.len(), 2);
    // Expected race condition: If A->(B|C)->(C|B) before A->(C|B) then (C|B)->A
    if received.len() > 0 {
        assert_eq!(count_messages_received(&received, &[&node_B_id, &node_C_id]), 1);
    }

    let (sent, received) = partition_events(collect_stream!(node_B_messaging, timeout = Duration::from_secs(20)));
    assert_eq!(sent.len(), 1);
    let recv_count = count_messages_received(&received, &[&node_A_id, &node_C_id]);
    // Expected race condition: If A->B->C before A->C then C->B does not happen
    assert!(recv_count >= 1 && recv_count <= 2);

    let (sent, received) = partition_events(collect_stream!(node_C_messaging, timeout = Duration::from_secs(20)));
    let recv_count = count_messages_received(&received, &[&node_A_id, &node_B_id]);
    assert_eq!(recv_count, 2);
    assert_eq!(sent.len(), 2);
    assert_eq!(count_messages_received(&received, &[&node_D_id]), 0);

    let (sent, received) = partition_events(collect_stream!(node_D_messaging, timeout = Duration::from_secs(20)));
    assert_eq!(sent.len(), 0);
    assert_eq!(received.len(), 1);
    assert_eq!(count_messages_received(&received, &[&node_C_id]), 1);
}

fn partition_events(
    events: Vec<Result<Arc<MessagingEvent>, tokio::sync::broadcast::RecvError>>,
) -> (Vec<Arc<MessagingEvent>>, Vec<Arc<MessagingEvent>>) {
    events.into_iter().map(Result::unwrap).partition(|e| match &**e {
        MessagingEvent::MessageReceived(_, _) => false,
        MessagingEvent::MessageSent(_) => true,
        _ => unreachable!(),
    })
}

fn count_messages_received(events: &[Arc<MessagingEvent>], node_ids: &[&NodeId]) -> usize {
    events
        .into_iter()
        .filter(|event| {
            unpack_enum!(MessagingEvent::MessageReceived(recv_node_id, _tag) = &***event);
            node_ids.into_iter().any(|n| &**recv_node_id == *n)
        })
        .count()
}

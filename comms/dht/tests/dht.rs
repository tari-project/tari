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
    peer_manager::{NodeIdentity, Peer, PeerFeatures, PeerStorage},
    pipeline,
    pipeline::SinkService,
    transports::MemoryTransport,
    types::CommsDatabase,
    wrap_in_envelope_body,
    CommsBuilder,
    CommsNode,
};
use tari_comms_dht::{
    envelope::NodeDestination,
    inbound::DecryptedDhtMessage,
    outbound::{OutboundEncryption, SendMessageParams},
    Dht,
    DhtBuilder,
};
use tari_storage::{lmdb_store::LMDBBuilder, LMDBWrapper};
use tari_test_utils::{async_assert_eventually, paths::create_temporary_data_path, random};
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

fn create_peer_storage(peers: Vec<Peer>) -> CommsDatabase {
    let database_name = random::string(8);
    let datastore = LMDBBuilder::new()
        .set_path(create_temporary_data_path().to_str().unwrap())
        .set_environment_size(50)
        .set_max_number_of_databases(1)
        .add_database(&database_name, lmdb_zero::db::CREATE)
        .build()
        .unwrap();

    let peer_database = datastore.get_handle(&database_name).unwrap();
    let peer_database = LMDBWrapper::new(Arc::new(peer_database));
    let mut storage = PeerStorage::new_indexed(peer_database).unwrap();
    for peer in peers {
        storage.add_peer(peer).unwrap();
    }

    storage.into()
}

async fn make_node(features: PeerFeatures, seed_peer: Option<Peer>) -> TestNode {
    let node_identity = make_node_identity(features);
    make_node_with_node_identity(node_identity, seed_peer).await
}

async fn make_node_with_node_identity(node_identity: Arc<NodeIdentity>, seed_peer: Option<Peer>) -> TestNode {
    let (tx, ims_rx) = mpsc::channel(1);
    let (comms, dht) = setup_comms_dht(node_identity, create_peer_storage(seed_peer.into_iter().collect()), tx).await;

    TestNode { comms, dht, ims_rx }
}

async fn setup_comms_dht(
    node_identity: Arc<NodeIdentity>,
    storage: CommsDatabase,
    inbound_tx: mpsc::Sender<DecryptedDhtMessage>,
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
        comms.connection_manager_requester(),
        comms.shutdown_signal(),
    )
    .local_test()
    .with_discovery_timeout(Duration::from_secs(60))
    .with_num_neighbouring_nodes(8)
    .finish();

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

    // Send a discover request from Node A, through B and C, to D. Once Node D
    // receives the discover request from Node A, it should send a  discovery response
    // request back to A at which time this call will resolve (or timeout).
    node_A
        .dht
        .discovery_service_requester()
        .discover_peer(
            Box::new(node_D.node_identity().public_key().clone()),
            NodeDestination::Unknown,
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

    let dest_public_key = Box::new(node_C_node_identity.public_key().clone());
    let params = SendMessageParams::new()
        .neighbours(vec![])
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

    // Wait for node B to receive the 2 propagation messages
    node_B_msg_events.next().await.unwrap().unwrap();
    node_B_msg_events.next().await.unwrap().unwrap();

    let mut node_C = make_node_with_node_identity(node_C_node_identity, None).await;
    node_C.comms.peer_manager().add_peer(node_B.to_peer()).await.unwrap();
    node_C.dht.dht_requester().send_request_stored_messages().await.unwrap();

    let msg = node_C.next_inbound_message(Duration::from_secs(5)).await.unwrap();
    assert_eq!(
        msg.authenticated_origin.as_ref().unwrap(),
        node_A.comms.node_identity().public_key()
    );
    let secret = msg.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap();
    assert_eq!(secret, secret_msg1.to_vec());
    let msg = node_C.next_inbound_message(Duration::from_secs(5)).await.unwrap();
    assert_eq!(
        msg.authenticated_origin.as_ref().unwrap(),
        node_A.comms.node_identity().public_key()
    );
    let secret = msg.success().unwrap().decode_part::<Vec<u8>>(0).unwrap().unwrap();
    assert_eq!(secret, secret_msg2.to_vec());

    node_A.comms.shutdown().await;
    node_B.comms.shutdown().await;
    node_C.comms.shutdown().await;
}

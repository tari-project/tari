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
    base_node::{
        comms_interface::{CommsInterfaceError, OutboundNodeCommsInterface},
        service::{BaseNodeServiceConfig, BaseNodeServiceInitializer},
    },
    chain_storage::{BlockchainDatabase, MemoryDatabase},
    consts::BASE_NODE_SERVICE_REQUEST_TIMEOUT,
    test_utils::builders::{add_block_and_update_header, create_genesis_block},
    types::HashDigest,
};
use futures::Sink;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{error::Error, iter, sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsNode,
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, Peer, PeerFeatures, PeerFlags},
};
use tari_comms_dht::Dht;
use tari_p2p::{
    comms_connector::{pubsub_connector, InboundDomainConnector, PeerMessage},
    initialization::{initialize_comms, CommsConfig},
    services::comms_outbound::CommsOutboundServiceInitializer,
    tari_message::TariMessageType,
};
use tari_service_framework::StackBuilder;
use tempdir::TempDir;
use tokio::runtime::{Runtime, TaskExecutor};

fn random_string(len: usize) -> String {
    let mut rng = OsRng::new().unwrap();
    iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
}

fn setup_comms_services<TSink>(
    executor: TaskExecutor,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<NodeIdentity>,
    publisher: InboundDomainConnector<TariMessageType, TSink>,
) -> (CommsNode, Dht)
where
    TSink: Sink<Arc<PeerMessage<TariMessageType>>> + Clone + Unpin + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let comms_config = CommsConfig {
        node_identity: Arc::clone(&node_identity),
        host: "127.0.0.1".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listener_address: node_identity.control_service_address(),
            socks_proxy_address: None,
            requested_connection_timeout: Duration::from_millis(2000),
        },
        datastore_path: TempDir::new(random_string(8).as_str())
            .unwrap()
            .path()
            .to_str()
            .unwrap()
            .to_string(),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
    };

    let (comms, dht) = initialize_comms(executor, comms_config, publisher).unwrap();

    for p in peers {
        let addr = p.control_service_address();
        let NodeIdentity { identity, .. } = p;
        comms
            .peer_manager()
            .add_peer(Peer::new(
                identity.public_key,
                identity.node_id,
                addr.into(),
                PeerFlags::empty(),
                PeerFeatures::empty(),
            ))
            .unwrap();
    }

    (comms, dht)
}

pub fn setup_base_node_service(
    runtime: &Runtime,
    node_identity: NodeIdentity,
    peers: Vec<NodeIdentity>,
    blockchain_db: Arc<BlockchainDatabase<MemoryDatabase<HashDigest>>>,
    config: BaseNodeServiceConfig,
) -> (OutboundNodeCommsInterface, CommsNode)
{
    let node_identity = Arc::new(node_identity.clone());
    let (publisher, subscription_factory) = pubsub_connector(runtime.executor(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(runtime.executor(), node_identity.clone(), peers, publisher);

    let fut = StackBuilder::new(runtime.executor(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(BaseNodeServiceInitializer::new(
            subscription_factory,
            node_identity,
            blockchain_db,
            config,
        ))
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let outbound_nci = handles.get_handle::<OutboundNodeCommsInterface>().unwrap();

    (outbound_nci, comms)
}

#[test]
fn service_request_response_get_metadata() {
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let base_node_service_config = BaseNodeServiceConfig {
        request_timeout: BASE_NODE_SERVICE_REQUEST_TIMEOUT,
        broadcast_peer_count: 2,
        desired_response_count: 2,
    };

    let alice_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:30500".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let alice_blockchain_db = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());

    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:30501".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let bob_blockchain_db = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());

    let carol_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:30502".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let carol_blockchain_db = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());

    let (mut alice_outbound_nci, alice_comms) = setup_base_node_service(
        &runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        alice_blockchain_db,
        base_node_service_config.clone(),
    );
    let (_bob_outbound_nci, bob_comms) = setup_base_node_service(
        &runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone(), carol_node_identity.clone()],
        bob_blockchain_db.clone(),
        base_node_service_config.clone(),
    );
    let (_carol_outbound_nci, carol_comms) = setup_base_node_service(
        &runtime,
        carol_node_identity.clone(),
        vec![alice_node_identity.clone(), bob_node_identity.clone()],
        carol_blockchain_db,
        base_node_service_config.clone(),
    );

    add_block_and_update_header(&bob_blockchain_db, create_genesis_block().0);

    runtime.block_on(async {
        let received_metadata = alice_outbound_nci.get_metadata().await.unwrap();
        assert_eq!(received_metadata.len(), 2);
        assert!(
            (received_metadata[0].height_of_longest_chain == None) ||
                (received_metadata[1].height_of_longest_chain == None)
        );
        assert!(
            (received_metadata[0].height_of_longest_chain == Some(0)) ||
                (received_metadata[1].height_of_longest_chain == Some(0))
        );
    });

    alice_comms.shutdown().unwrap();
    bob_comms.shutdown().unwrap();
    carol_comms.shutdown().unwrap();
}

#[test]
fn service_request_timeout() {
    let runtime = Runtime::new().unwrap();
    let mut rng = OsRng::new().unwrap();
    let base_node_service_config = BaseNodeServiceConfig {
        request_timeout: Duration::from_millis(10),
        broadcast_peer_count: 2,
        desired_response_count: 2,
    };

    let alice_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:30503".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let alice_blockchain_db = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());

    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        "127.0.0.1:30504".parse().unwrap(),
        PeerFeatures::communication_node_default(),
    )
    .unwrap();
    let bob_blockchain_db = Arc::new(BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap());

    let (mut alice_outbound_nci, alice_comms) = setup_base_node_service(
        &runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone()],
        alice_blockchain_db,
        base_node_service_config.clone(),
    );
    let (_bob_outbound_nci, bob_comms) = setup_base_node_service(
        &runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone()],
        bob_blockchain_db.clone(),
        base_node_service_config,
    );

    runtime.block_on(async {
        assert_eq!(
            alice_outbound_nci.get_metadata().await,
            Err(CommsInterfaceError::RequestTimedOut)
        );
    });

    alice_comms.shutdown().unwrap();
    bob_comms.shutdown().unwrap();
}

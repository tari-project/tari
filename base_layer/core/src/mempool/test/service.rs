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
    chain_storage::{BlockchainDatabase, MemoryDatabase},
    mempool::{
        service::{MempoolServiceConfig, MempoolServiceInitializer, OutboundMempoolServiceInterface},
        Mempool,
        MempoolConfig,
        TxStorageResponse,
    },
    test_utils::builders::{add_block_and_update_header, create_genesis_block, spend_utxos},
    tx,
    txn_schema,
};
use futures::Sink;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{
    error::Error,
    iter,
    sync::Arc,
    time::{Duration, Instant},
};
use tari_comms::{
    builder::CommsNode,
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, Peer, PeerFeatures, PeerFlags},
};
use tari_comms_dht::{
    broadcast_strategy::BroadcastStrategy,
    domain_message::OutboundDomainMessage,
    envelope::NodeDestination,
    outbound::{OutboundEncryption, OutboundMessageRequester},
    Dht,
};
use tari_p2p::{
    comms_connector::{pubsub_connector, InboundDomainConnector, PeerMessage},
    initialization::{initialize_comms, CommsConfig},
    services::comms_outbound::CommsOutboundServiceInitializer,
    tari_message::TariMessageType,
};
use tari_service_framework::StackBuilder;
use tari_test_utils::{address::get_next_local_address, async_assert_eventually};
use tari_transactions::{
    proto::types as proto,
    tari_amount::{uT, T},
    types::{CryptoFactories, HashDigest},
};
use tempdir::TempDir;
use tokio::runtime::{Runtime, TaskExecutor};

// Todo: Some of the helper test functions are the same or similar to the Base node service test functions, these should
// be moved to the test_utils.

fn random_string(len: usize) -> String {
    let mut rng = OsRng::new().unwrap();
    iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
}

fn setup_comms_services<TSink>(
    executor: TaskExecutor,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<NodeIdentity>,
    publisher: InboundDomainConnector<TSink>,
) -> (CommsNode, Dht)
where
    TSink: Sink<Arc<PeerMessage>> + Clone + Unpin + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let comms_config = CommsConfig {
        node_identity: Arc::clone(&node_identity),
        peer_connection_listening_address: "127.0.0.1:0".parse().unwrap(),
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
        establish_connection_timeout: Duration::from_secs(5),
        peer_database_name: random_string(8),
        inbound_buffer_size: 100,
        outbound_buffer_size: 100,
        dht: Default::default(),
    };

    let (comms, dht) = initialize_comms(executor, comms_config, publisher).unwrap();

    for p in peers {
        let addr = p.control_service_address();
        comms
            .peer_manager()
            .add_peer(Peer::new(
                p.public_key().clone(),
                p.node_id().clone(),
                addr.into(),
                PeerFlags::empty(),
                p.features().clone(),
            ))
            .unwrap();
    }

    (comms, dht)
}

struct NodeInterfaces {
    pub node_identity: NodeIdentity,
    pub outbound_mp_interface: OutboundMempoolServiceInterface,
    pub outbound_message_service: OutboundMessageRequester,
    pub blockchain_db: BlockchainDatabase<MemoryDatabase<HashDigest>>,
    pub mempool: Mempool<MemoryDatabase<HashDigest>>,
    pub comms: CommsNode,
}

impl NodeInterfaces {
    fn new(
        node_identity: NodeIdentity,
        outbound_mp_interface: OutboundMempoolServiceInterface,
        outbound_message_service: OutboundMessageRequester,
        blockchain_db: BlockchainDatabase<MemoryDatabase<HashDigest>>,
        mempool: Mempool<MemoryDatabase<HashDigest>>,
        comms: CommsNode,
    ) -> Self
    {
        Self {
            node_identity,
            outbound_mp_interface,
            outbound_message_service,
            blockchain_db,
            mempool,
            comms,
        }
    }
}

pub fn setup_mempool_service(
    runtime: &Runtime,
    node_identity: NodeIdentity,
    peers: Vec<NodeIdentity>,
    mempool: Mempool<MemoryDatabase<HashDigest>>,
    config: MempoolServiceConfig,
) -> (OutboundMempoolServiceInterface, OutboundMessageRequester, CommsNode)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.executor(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(runtime.executor(), Arc::new(node_identity), peers, publisher);

    let fut = StackBuilder::new(runtime.executor(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(MempoolServiceInitializer::new(subscription_factory, mempool, config))
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");

    let outbound_mp_handle = handles.get_handle::<OutboundMempoolServiceInterface>().unwrap();
    let outbound_message_service = handles.get_handle::<OutboundMessageRequester>().unwrap();

    (outbound_mp_handle, outbound_message_service, comms)
}

fn create_network_with_3_mempools(
    runtime: &Runtime,
    config: MempoolServiceConfig,
) -> (NodeInterfaces, NodeInterfaces, NodeInterfaces)
{
    let mut rng = OsRng::new().unwrap();
    let alice_node_identity = NodeIdentity::random(
        &mut rng,
        get_next_local_address().parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let alice_blockchain_db = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let alice_mempool = Mempool::new(alice_blockchain_db.clone(), MempoolConfig::default());

    let bob_node_identity = NodeIdentity::random(
        &mut rng,
        get_next_local_address().parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let bob_blockchain_db = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let bob_mempool = Mempool::new(bob_blockchain_db.clone(), MempoolConfig::default());

    let carol_node_identity = NodeIdentity::random(
        &mut rng,
        get_next_local_address().parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    )
    .unwrap();
    let carol_blockchain_db = BlockchainDatabase::new(MemoryDatabase::<HashDigest>::default()).unwrap();
    let carol_mempool = Mempool::new(carol_blockchain_db.clone(), MempoolConfig::default());

    let (alice_outbound_mp_interface, alice_outbound_message_service, alice_comms) = setup_mempool_service(
        &runtime,
        alice_node_identity.clone(),
        vec![bob_node_identity.clone(), carol_node_identity.clone()],
        alice_mempool.clone(),
        config.clone(),
    );
    let (bob_outbound_mp_interface, bob_outbound_message_service, bob_comms) = setup_mempool_service(
        &runtime,
        bob_node_identity.clone(),
        vec![alice_node_identity.clone(), carol_node_identity.clone()],
        bob_mempool.clone(),
        config.clone(),
    );
    let (carol_outbound_mp_interface, carol_outbound_message_service, carol_comms) = setup_mempool_service(
        &runtime,
        carol_node_identity.clone(),
        vec![alice_node_identity.clone(), bob_node_identity.clone()],
        carol_mempool.clone(),
        config,
    );
    let alice_interfaces = NodeInterfaces::new(
        alice_node_identity,
        alice_outbound_mp_interface,
        alice_outbound_message_service,
        alice_blockchain_db,
        alice_mempool,
        alice_comms,
    );
    let bob_interfaces = NodeInterfaces::new(
        bob_node_identity,
        bob_outbound_mp_interface,
        bob_outbound_message_service,
        bob_blockchain_db,
        bob_mempool,
        bob_comms,
    );
    let carol_interfaces = NodeInterfaces::new(
        carol_node_identity,
        carol_outbound_mp_interface,
        carol_outbound_message_service,
        carol_blockchain_db,
        carol_mempool,
        carol_comms,
    );
    (alice_interfaces, bob_interfaces, carol_interfaces)
}

#[test]
fn request_response_get_stats() {
    let factories = CryptoFactories::default();
    let runtime = Runtime::new().unwrap();

    let (mut alice_interfaces, bob_interfaces, carol_interfaces) =
        create_network_with_3_mempools(&runtime, MempoolServiceConfig::default());

    let (block0, utxo) = create_genesis_block(&factories);
    add_block_and_update_header(&bob_interfaces.blockchain_db, block0.clone());
    add_block_and_update_header(&carol_interfaces.blockchain_db, block0);
    let (tx1, _, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![2 * T, 2 * T, 2 * T]));
    let tx1 = Arc::new(tx1);
    bob_interfaces.mempool.insert(tx1.clone()).unwrap();
    carol_interfaces.mempool.insert(tx1).unwrap();
    let (orphan1, _, _) = tx!(1*T, fee: 100*uT);
    let orphan1 = Arc::new(orphan1);
    bob_interfaces.mempool.insert(orphan1.clone()).unwrap();
    carol_interfaces.mempool.insert(orphan1).unwrap();
    let (orphan2, _, _) = tx!(2*T, fee: 200*uT);
    let orphan2 = Arc::new(orphan2);
    bob_interfaces.mempool.insert(orphan2.clone()).unwrap();
    carol_interfaces.mempool.insert(orphan2).unwrap();

    runtime.block_on(async {
        let received_stats = alice_interfaces.outbound_mp_interface.get_stats().await.unwrap();
        assert_eq!(received_stats.total_txs, 3);
        assert_eq!(received_stats.unconfirmed_txs, 1);
        assert_eq!(received_stats.orphan_txs, 2);
        assert_eq!(received_stats.timelocked_txs, 0);
        assert_eq!(received_stats.published_txs, 0);
        assert_eq!(received_stats.total_weight, 35);
    });

    alice_interfaces.comms.shutdown().unwrap();
    bob_interfaces.comms.shutdown().unwrap();
    carol_interfaces.comms.shutdown().unwrap();
}

#[test]
fn receive_and_propagate_transaction() {
    let factories = CryptoFactories::default();
    let runtime = Runtime::new().unwrap();

    let (mut alice_interfaces, mut bob_interfaces, mut carol_interfaces) =
        create_network_with_3_mempools(&runtime, MempoolServiceConfig::default());

    let (block0, utxo) = create_genesis_block(&factories);
    add_block_and_update_header(&alice_interfaces.blockchain_db, block0.clone());
    add_block_and_update_header(&bob_interfaces.blockchain_db, block0.clone());
    add_block_and_update_header(&carol_interfaces.blockchain_db, block0);
    let (tx, _, _) = spend_utxos(txn_schema!(from: vec![utxo], to: vec![2 * T, 2 * T, 2 * T]));
    let (orphan, _, _) = tx!(1*T, fee: 100*uT);
    let tx_excess_sig = tx.body.kernels()[0].excess_sig.clone();
    let orphan_excess_sig = orphan.body.kernels()[0].excess_sig.clone();
    assert!(alice_interfaces.mempool.insert(Arc::new(tx.clone())).is_ok());
    assert!(alice_interfaces.mempool.insert(Arc::new(orphan.clone())).is_ok());

    runtime.block_on(async {
        alice_interfaces
            .outbound_message_service
            .send_message(
                BroadcastStrategy::DirectPublicKey(bob_interfaces.node_identity.public_key().clone()),
                NodeDestination::Unknown,
                OutboundEncryption::EncryptForDestination,
                OutboundDomainMessage::new(TariMessageType::NewTransaction, proto::Transaction::from(tx)),
            )
            .await
            .unwrap();
        alice_interfaces
            .outbound_message_service
            .send_message(
                BroadcastStrategy::DirectPublicKey(carol_interfaces.node_identity.public_key().clone()),
                NodeDestination::Unknown,
                OutboundEncryption::EncryptForDestination,
                OutboundDomainMessage::new(TariMessageType::NewTransaction, proto::Transaction::from(orphan)),
            )
            .await
            .unwrap();

        async_assert_eventually!(
            bob_interfaces.mempool.has_tx_with_excess_sig(&tx_excess_sig).unwrap(),
            expect = TxStorageResponse::UnconfirmedPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            bob_interfaces
                .mempool
                .has_tx_with_excess_sig(&orphan_excess_sig)
                .unwrap(),
            expect = TxStorageResponse::OrphanPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            carol_interfaces.mempool.has_tx_with_excess_sig(&tx_excess_sig).unwrap(),
            expect = TxStorageResponse::UnconfirmedPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
        async_assert_eventually!(
            carol_interfaces
                .mempool
                .has_tx_with_excess_sig(&orphan_excess_sig)
                .unwrap(),
            expect = TxStorageResponse::OrphanPool,
            max_attempts = 10,
            interval = Duration::from_millis(1000)
        );
    });

    alice_interfaces.comms.shutdown().unwrap();
    bob_interfaces.comms.shutdown().unwrap();
    carol_interfaces.comms.shutdown().unwrap();
}

// TODO: add test for service_request_timeout

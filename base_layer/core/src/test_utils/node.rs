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
        service::{BaseNodeServiceConfig, BaseNodeServiceInitializer},
        LocalNodeCommsInterface,
        OutboundNodeCommsInterface,
    },
    blocks::Block,
    chain_storage::{BlockchainDatabase, MemoryDatabase, Validators},
    mempool::{
        Mempool,
        MempoolConfig,
        MempoolServiceConfig,
        MempoolServiceInitializer,
        OutboundMempoolServiceInterface,
    },
    proof_of_work::DiffAdjManager,
    validation::{mocks::MockValidator, Validation},
};
use futures::Sink;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{error::Error, iter, sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsNode,
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, Peer, PeerFeatures, PeerFlags},
};
use tari_comms_dht::{outbound::OutboundMessageRequester, Dht};
use tari_mmr::MerkleChangeTrackerConfig;
use tari_p2p::{
    comms_connector::{pubsub_connector, InboundDomainConnector, PeerMessage},
    initialization::{initialize_comms, CommsConfig},
    services::comms_outbound::CommsOutboundServiceInitializer,
};
use tari_service_framework::StackBuilder;
use tari_test_utils::address::get_next_local_address;
use tari_transactions::types::HashDigest;
use tempdir::TempDir;
use tokio::runtime::{Runtime, TaskExecutor};

/// The NodeInterfaces is used as a container for providing access to all the services and interfaces of a single node.
pub struct NodeInterfaces {
    pub node_identity: Arc<NodeIdentity>,
    pub outbound_nci: OutboundNodeCommsInterface,
    pub local_nci: LocalNodeCommsInterface,
    pub outbound_mp_interface: OutboundMempoolServiceInterface,
    pub outbound_message_service: OutboundMessageRequester,
    pub blockchain_db: BlockchainDatabase<MemoryDatabase<HashDigest>>,
    pub mempool: Mempool<MemoryDatabase<HashDigest>>,
    pub comms: CommsNode,
}

/// The BaseNodeBuilder can be used to construct a test Base Node with all its relevant services and interfaces for
/// testing.
pub struct BaseNodeBuilder {
    node_identity: Option<Arc<NodeIdentity>>,
    peers: Option<Vec<Arc<NodeIdentity>>>,
    base_node_service_config: Option<BaseNodeServiceConfig>,
    mct_config: Option<MerkleChangeTrackerConfig>,
    mempool_config: Option<MempoolConfig>,
    mempool_service_config: Option<MempoolServiceConfig>,
    validators: Option<Validators<MemoryDatabase<HashDigest>>>,
}

impl BaseNodeBuilder {
    /// Create a new BaseNodeBuilder
    pub fn new() -> Self {
        Self {
            node_identity: None,
            peers: None,
            base_node_service_config: None,
            mct_config: None,
            mempool_config: None,
            mempool_service_config: None,
            validators: None,
        }
    }

    /// Set node identity that should be used for the Base Node. If not specified a random identity will be used.
    pub fn with_node_identity(mut self, node_identity: Arc<NodeIdentity>) -> Self {
        self.node_identity = Some(node_identity);
        self
    }

    /// Set the initial peers that will be available in the peer manager.
    pub fn with_peers(mut self, peers: Vec<Arc<NodeIdentity>>) -> Self {
        self.peers = Some(peers);
        self
    }

    /// Set the configuration of the Base Node Service
    pub fn with_base_node_service_config(mut self, config: BaseNodeServiceConfig) -> Self {
        self.base_node_service_config = Some(config);
        self
    }

    /// Set the configuration of the MerkleChangeTracker of the Base Node Backend
    pub fn with_merkle_change_tracker_config(mut self, config: MerkleChangeTrackerConfig) -> Self {
        self.mct_config = Some(config);
        self
    }

    /// Set the configuration of the Mempool
    pub fn with_mempool_config(mut self, config: MempoolConfig) -> Self {
        self.mempool_config = Some(config);
        self
    }

    /// Set the configuration of the Mempool Service
    pub fn with_mempool_service_config(mut self, config: MempoolServiceConfig) -> Self {
        self.mempool_service_config = Some(config);
        self
    }

    pub fn with_validators(
        mut self,
        block: impl Validation<Block, MemoryDatabase<HashDigest>> + 'static,
        orphan: impl Validation<Block, MemoryDatabase<HashDigest>> + 'static,
    ) -> Self
    {
        let validators = Validators::new(block, orphan);
        self.validators = Some(validators);
        self
    }

    /// Build the test base node and start its services.
    pub fn start(self, runtime: &Runtime, data_path: &str) -> NodeInterfaces {
        let mct_config = self.mct_config.unwrap_or(MerkleChangeTrackerConfig {
            min_history_len: 10,
            max_history_len: 20,
        });
        let validators = self
            .validators
            .unwrap_or(Validators::new(MockValidator::new(true), MockValidator::new(true)));
        let db = MemoryDatabase::<HashDigest>::new(mct_config);
        let blockchain_db = BlockchainDatabase::new(db, validators).unwrap();
        let mempool = Mempool::new(
            blockchain_db.clone(),
            self.mempool_config.unwrap_or(MempoolConfig::default()),
        );
        let diff_adj_manager = DiffAdjManager::new(blockchain_db.clone()).unwrap();

        let node_identity = self.node_identity.unwrap_or(random_node_identity());
        let (outbound_nci, local_nci, outbound_mp_interface, outbound_message_service, comms) =
            setup_base_node_services(
                &runtime,
                node_identity.clone(),
                self.peers.unwrap_or(Vec::new()),
                blockchain_db.clone(),
                mempool.clone(),
                diff_adj_manager,
                self.base_node_service_config
                    .unwrap_or(BaseNodeServiceConfig::default()),
                self.mempool_service_config.unwrap_or(MempoolServiceConfig::default()),
                data_path,
            );

        NodeInterfaces {
            node_identity,
            outbound_nci,
            local_nci,
            outbound_mp_interface,
            outbound_message_service,
            blockchain_db,
            mempool,
            comms,
        }
    }
}

// Creates a network with two Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_2_base_nodes(runtime: &Runtime, data_path: &str) -> (NodeInterfaces, NodeInterfaces) {
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();

    let alice_node = BaseNodeBuilder::new()
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .start(&runtime, data_path);
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity)
        .with_peers(vec![alice_node_identity])
        .start(&runtime, data_path);

    (alice_node, bob_node)
}

// Creates a network with two Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_2_base_nodes_with_config(
    runtime: &Runtime,
    base_node_service_config: BaseNodeServiceConfig,
    mct_config: MerkleChangeTrackerConfig,
    mempool_service_config: MempoolServiceConfig,
    data_path: &str,
) -> (NodeInterfaces, NodeInterfaces)
{
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();

    let alice_node = BaseNodeBuilder::new()
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .with_base_node_service_config(base_node_service_config)
        .with_merkle_change_tracker_config(mct_config)
        .with_mempool_service_config(mempool_service_config)
        .start(&runtime, data_path);
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity)
        .with_peers(vec![alice_node_identity])
        .with_base_node_service_config(base_node_service_config)
        .with_merkle_change_tracker_config(mct_config)
        .with_mempool_service_config(mempool_service_config)
        .start(&runtime, data_path);

    (alice_node, bob_node)
}

// Creates a network with three Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_3_base_nodes(
    runtime: &Runtime,
    data_path: &str,
) -> (NodeInterfaces, NodeInterfaces, NodeInterfaces)
{
    let mct_config = MerkleChangeTrackerConfig {
        min_history_len: 10,
        max_history_len: 20,
    };
    create_network_with_3_base_nodes_with_config(
        runtime,
        BaseNodeServiceConfig::default(),
        mct_config,
        MempoolServiceConfig::default(),
        data_path,
    )
}

// Creates a network with three Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_3_base_nodes_with_config(
    runtime: &Runtime,
    base_node_service_config: BaseNodeServiceConfig,
    mct_config: MerkleChangeTrackerConfig,
    mempool_service_config: MempoolServiceConfig,
    data_path: &str,
) -> (NodeInterfaces, NodeInterfaces, NodeInterfaces)
{
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();

    let alice_node = BaseNodeBuilder::new()
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone(), carol_node_identity.clone()])
        .with_base_node_service_config(base_node_service_config)
        .with_merkle_change_tracker_config(mct_config)
        .with_mempool_service_config(mempool_service_config)
        .start(&runtime, data_path);
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![alice_node_identity.clone(), carol_node_identity.clone()])
        .with_base_node_service_config(base_node_service_config)
        .with_merkle_change_tracker_config(mct_config)
        .with_mempool_service_config(mempool_service_config)
        .start(&runtime, data_path);
    let carol_node = BaseNodeBuilder::new()
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![alice_node_identity, bob_node_identity.clone()])
        .with_base_node_service_config(base_node_service_config)
        .with_merkle_change_tracker_config(mct_config)
        .with_mempool_service_config(mempool_service_config)
        .start(&runtime, data_path);

    (alice_node, bob_node, carol_node)
}

fn random_string(len: usize) -> String {
    let mut rng = OsRng::new().unwrap();
    iter::repeat(()).map(|_| rng.sample(Alphanumeric)).take(len).collect()
}

// Helper function for creating a random node indentity.
fn random_node_identity() -> Arc<NodeIdentity> {
    let mut rng = OsRng::new().unwrap();
    Arc::new(
        NodeIdentity::random(
            &mut rng,
            get_next_local_address().parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap(),
    )
}

// Helper function for starting the comms stack.
fn setup_comms_services<TSink>(
    executor: TaskExecutor,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    publisher: InboundDomainConnector<TSink>,
    data_path: &str,
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
        datastore_path: data_path.to_string(),
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

// Helper function for starting the services of the Base node.
fn setup_base_node_services(
    runtime: &Runtime,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    blockchain_db: BlockchainDatabase<MemoryDatabase<HashDigest>>,
    mempool: Mempool<MemoryDatabase<HashDigest>>,
    diff_adj_manager: DiffAdjManager<MemoryDatabase<HashDigest>>,
    base_node_service_config: BaseNodeServiceConfig,
    mempool_service_config: MempoolServiceConfig,
    data_path: &str,
) -> (
    OutboundNodeCommsInterface,
    LocalNodeCommsInterface,
    OutboundMempoolServiceInterface,
    OutboundMessageRequester,
    CommsNode,
)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.executor(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(runtime.executor(), node_identity, peers, publisher, data_path);

    let fut = StackBuilder::new(runtime.executor(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(BaseNodeServiceInitializer::new(
            subscription_factory.clone(),
            blockchain_db,
            mempool.clone(),
            diff_adj_manager,
            base_node_service_config,
        ))
        .add_initializer(MempoolServiceInitializer::new(
            subscription_factory,
            mempool,
            mempool_service_config,
        ))
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");
    (
        handles.get_handle::<OutboundNodeCommsInterface>().unwrap(),
        handles.get_handle::<LocalNodeCommsInterface>().unwrap(),
        handles.get_handle::<OutboundMempoolServiceInterface>().unwrap(),
        handles.get_handle::<OutboundMessageRequester>().unwrap(),
        comms,
    )
}

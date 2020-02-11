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

use futures::Sink;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{error::Error, iter, sync::Arc, time::Duration};
use tari_comms::{
    builder::CommsNode,
    control_service::ControlServiceConfig,
    peer_manager::{NodeIdentity, Peer, PeerFeatures, PeerFlags},
};
use tari_comms_dht::{outbound::OutboundMessageRequester, Dht};
use tari_core::{
    base_node::{
        chain_metadata_service::{ChainMetadataHandle, ChainMetadataServiceInitializer},
        service::{BaseNodeServiceConfig, BaseNodeServiceInitializer},
        LocalNodeCommsInterface,
        OutboundNodeCommsInterface,
    },
    blocks::{Block, BlockHeader},
    chain_storage::{BlockchainDatabase, MemoryDatabase, Validators},
    consensus::ConsensusManager,
    mempool::{
        Mempool,
        MempoolConfig,
        MempoolServiceConfig,
        MempoolServiceInitializer,
        MempoolValidators,
        OutboundMempoolServiceInterface,
    },
    proof_of_work::DiffAdjManager,
    transactions::types::HashDigest,
    validation::{mocks::MockValidator, transaction_validators::TxInputAndMaturityValidator, Validation},
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::{
    comms_connector::{pubsub_connector, InboundDomainConnector, PeerMessage},
    initialization::{initialize_comms, CommsConfig},
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessConfig, LivenessInitializer},
    },
};
use tari_service_framework::StackBuilder;
use tari_test_utils::address::get_next_local_address;
use tokio::runtime::{self, Runtime};

/// The NodeInterfaces is used as a container for providing access to all the services and interfaces of a single node.
pub struct NodeInterfaces {
    pub node_identity: Arc<NodeIdentity>,
    pub outbound_nci: OutboundNodeCommsInterface,
    pub local_nci: LocalNodeCommsInterface,
    pub outbound_mp_interface: OutboundMempoolServiceInterface,
    pub outbound_message_service: OutboundMessageRequester,
    pub blockchain_db: BlockchainDatabase<MemoryDatabase<HashDigest>>,
    pub mempool: Mempool<MemoryDatabase<HashDigest>>,
    pub chain_metadata_handle: ChainMetadataHandle,
    pub comms: CommsNode,
}

/// The BaseNodeBuilder can be used to construct a test Base Node with all its relevant services and interfaces for
/// testing.
pub struct BaseNodeBuilder {
    node_identity: Option<Arc<NodeIdentity>>,
    peers: Option<Vec<Arc<NodeIdentity>>>,
    base_node_service_config: Option<BaseNodeServiceConfig>,
    mmr_cache_config: Option<MmrCacheConfig>,
    mempool_config: Option<MempoolConfig>,
    mempool_service_config: Option<MempoolServiceConfig>,
    liveness_service_config: Option<LivenessConfig>,
    validators: Option<Validators<MemoryDatabase<HashDigest>>>,
}

impl BaseNodeBuilder {
    /// Create a new BaseNodeBuilder
    pub fn new() -> Self {
        Self {
            node_identity: None,
            peers: None,
            base_node_service_config: None,
            mmr_cache_config: None,
            mempool_config: None,
            mempool_service_config: None,
            liveness_service_config: None,
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
    pub fn with_mmr_cache_config(mut self, config: MmrCacheConfig) -> Self {
        self.mmr_cache_config = Some(config);
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

    /// Set the configuration of the Liveness Service
    pub fn with_liveness_service_config(mut self, config: LivenessConfig) -> Self {
        self.liveness_service_config = Some(config);
        self
    }

    pub fn with_validators(
        mut self,
        block: impl Validation<Block, MemoryDatabase<HashDigest>> + 'static,
        orphan: impl Validation<Block, MemoryDatabase<HashDigest>> + 'static,
        chain_gb: impl Validation<BlockHeader, MemoryDatabase<HashDigest>> + 'static,
        chain_tip: impl Validation<BlockHeader, MemoryDatabase<HashDigest>> + 'static,
    ) -> Self
    {
        let validators = Validators::new(block, orphan, chain_gb, chain_tip);
        self.validators = Some(validators);
        self
    }

    /// Build the test base node and start its services.
    pub fn start(self, runtime: &mut Runtime, data_path: &str) -> NodeInterfaces {
        let mmr_cache_config = self.mmr_cache_config.unwrap_or(MmrCacheConfig { rewind_hist_len: 10 });
        let validators = self.validators.unwrap_or(Validators::new(
            MockValidator::new(true),
            MockValidator::new(true),
            MockValidator::new(true),
            MockValidator::new(true),
        ));
        let db = MemoryDatabase::<HashDigest>::new(mmr_cache_config);
        let mut blockchain_db = BlockchainDatabase::new(db).unwrap();
        blockchain_db.set_validators(validators);
        let mempool_validator = MempoolValidators::new(
            TxInputAndMaturityValidator::new(blockchain_db.clone()),
            TxInputAndMaturityValidator::new(blockchain_db.clone()),
        );
        let mempool = Mempool::new(
            blockchain_db.clone(),
            self.mempool_config.unwrap_or(MempoolConfig::default()),
            mempool_validator,
        );
        let diff_adj_manager = DiffAdjManager::new(blockchain_db.clone()).unwrap();
        let consensus_manager = ConsensusManager::default();
        consensus_manager.set_diff_manager(diff_adj_manager).unwrap();
        let node_identity = self.node_identity.unwrap_or(random_node_identity());
        let (outbound_nci, local_nci, outbound_mp_interface, outbound_message_service, chain_metadata_handle, comms) =
            setup_base_node_services(
                runtime,
                node_identity.clone(),
                self.peers.unwrap_or(Vec::new()),
                blockchain_db.clone(),
                mempool.clone(),
                consensus_manager,
                self.base_node_service_config
                    .unwrap_or(BaseNodeServiceConfig::default()),
                self.mempool_service_config.unwrap_or(MempoolServiceConfig::default()),
                self.liveness_service_config.unwrap_or(LivenessConfig::default()),
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
            chain_metadata_handle,
            comms,
        }
    }
}

// Creates a network with two Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_2_base_nodes(runtime: &mut Runtime, data_path: &str) -> (NodeInterfaces, NodeInterfaces) {
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();

    let alice_node = BaseNodeBuilder::new()
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .start(runtime, data_path);
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity)
        .with_peers(vec![alice_node_identity])
        .start(runtime, data_path);

    (alice_node, bob_node)
}

// Creates a network with two Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_2_base_nodes_with_config(
    runtime: &mut Runtime,
    base_node_service_config: BaseNodeServiceConfig,
    mmr_cache_config: MmrCacheConfig,
    mempool_service_config: MempoolServiceConfig,
    liveness_service_config: LivenessConfig,
    data_path: &str,
) -> (NodeInterfaces, NodeInterfaces)
{
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();

    let alice_node = BaseNodeBuilder::new()
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config)
        .start(runtime, data_path);
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity)
        .with_peers(vec![alice_node_identity])
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config)
        .start(runtime, data_path);

    (alice_node, bob_node)
}

// Creates a network with three Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_3_base_nodes(
    runtime: &mut Runtime,
    data_path: &str,
) -> (NodeInterfaces, NodeInterfaces, NodeInterfaces)
{
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 10 };
    create_network_with_3_base_nodes_with_config(
        runtime,
        BaseNodeServiceConfig::default(),
        mmr_cache_config,
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        data_path,
    )
}

// Creates a network with three Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_3_base_nodes_with_config(
    runtime: &mut Runtime,
    base_node_service_config: BaseNodeServiceConfig,
    mmr_cache_config: MmrCacheConfig,
    mempool_service_config: MempoolServiceConfig,
    liveness_service_config: LivenessConfig,
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
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config)
        .start(runtime, data_path);
    let bob_node = BaseNodeBuilder::new()
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![alice_node_identity.clone(), carol_node_identity.clone()])
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config)
        .start(runtime, data_path);
    let carol_node = BaseNodeBuilder::new()
        .with_node_identity(carol_node_identity.clone())
        .with_peers(vec![alice_node_identity, bob_node_identity.clone()])
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config)
        .start(runtime, data_path);

    (alice_node, bob_node, carol_node)
}

fn random_string(len: usize) -> String {
    iter::repeat(()).map(|_| OsRng.sample(Alphanumeric)).take(len).collect()
}

// Helper function for creating a random node indentity.
pub fn random_node_identity() -> Arc<NodeIdentity> {
    Arc::new(
        NodeIdentity::random(
            &mut OsRng,
            get_next_local_address().parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap(),
    )
}

// Helper function for starting the comms stack.
fn setup_comms_services<TSink>(
    executor: runtime::Handle,
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
        peer_connection_listening_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
        socks_proxy_address: None,
        control_service: ControlServiceConfig {
            listening_address: node_identity.public_address(),
            socks_proxy_address: None,
            public_peer_address: None,
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
        let addr = p.public_address();
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
    runtime: &mut Runtime,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    blockchain_db: BlockchainDatabase<MemoryDatabase<HashDigest>>,
    mempool: Mempool<MemoryDatabase<HashDigest>>,
    consensus_manager: ConsensusManager<MemoryDatabase<HashDigest>>,
    base_node_service_config: BaseNodeServiceConfig,
    mempool_service_config: MempoolServiceConfig,
    liveness_service_config: LivenessConfig,
    data_path: &str,
) -> (
    OutboundNodeCommsInterface,
    LocalNodeCommsInterface,
    OutboundMempoolServiceInterface,
    OutboundMessageRequester,
    ChainMetadataHandle,
    CommsNode,
)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.handle().clone(), 100);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = setup_comms_services(runtime.handle().clone(), node_identity, peers, publisher, data_path);

    let fut = StackBuilder::new(runtime.handle().clone(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(LivenessInitializer::new(
            liveness_service_config,
            Arc::clone(&subscription_factory),
            dht.dht_requester(),
        ))
        .add_initializer(BaseNodeServiceInitializer::new(
            subscription_factory.clone(),
            blockchain_db,
            mempool.clone(),
            consensus_manager,
            base_node_service_config,
        ))
        .add_initializer(MempoolServiceInitializer::new(
            subscription_factory,
            mempool,
            mempool_service_config,
        ))
        .add_initializer(ChainMetadataServiceInitializer)
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");
    (
        handles.get_handle::<OutboundNodeCommsInterface>().unwrap(),
        handles.get_handle::<LocalNodeCommsInterface>().unwrap(),
        handles.get_handle::<OutboundMempoolServiceInterface>().unwrap(),
        handles.get_handle::<OutboundMessageRequester>().unwrap(),
        handles.get_handle::<ChainMetadataHandle>().unwrap(),
        comms,
    )
}

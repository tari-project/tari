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

use crate::helpers::mock_state_machine::MockBaseNodeStateMachine;
use futures::Sink;
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use std::{error::Error, iter, path::Path, sync::Arc, time::Duration};
use tari_comms::{
    peer_manager::{NodeIdentity, PeerFeatures},
    transports::MemoryTransport,
    CommsNode,
};
use tari_comms_dht::{outbound::OutboundMessageRequester, Dht};
use tari_core::{
    base_node::{
        chain_metadata_service::{ChainMetadataHandle, ChainMetadataServiceInitializer},
        service::{BaseNodeServiceConfig, BaseNodeServiceInitializer},
        LocalNodeCommsInterface,
        OutboundNodeCommsInterface,
    },
    blocks::Block,
    chain_storage::{BlockchainDatabase, BlockchainDatabaseConfig, MemoryDatabase, Validators},
    consensus::{ConsensusManager, ConsensusManagerBuilder, Network},
    mempool::{
        service::LocalMempoolService,
        Mempool,
        MempoolConfig,
        MempoolServiceConfig,
        MempoolServiceInitializer,
        MempoolValidators,
        OutboundMempoolServiceInterface,
    },
    transactions::types::HashDigest,
    validation::{
        mocks::MockValidator,
        transaction_validators::TxInputAndMaturityValidator,
        StatefulValidation,
        Validation,
    },
};
use tari_mmr::MmrCacheConfig;
use tari_p2p::{
    comms_connector::{pubsub_connector, InboundDomainConnector, PeerMessage},
    initialization::initialize_local_test_comms,
    services::{
        comms_outbound::CommsOutboundServiceInitializer,
        liveness::{LivenessConfig, LivenessHandle, LivenessInitializer},
    },
};
use tari_service_framework::StackBuilder;
use tokio::runtime::Runtime;

/// The NodeInterfaces is used as a container for providing access to all the services and interfaces of a single node.
pub struct NodeInterfaces {
    pub node_identity: Arc<NodeIdentity>,
    pub outbound_nci: OutboundNodeCommsInterface,
    pub local_nci: LocalNodeCommsInterface,
    pub outbound_mp_interface: OutboundMempoolServiceInterface,
    pub outbound_message_service: OutboundMessageRequester,
    pub blockchain_db: BlockchainDatabase<MemoryDatabase<HashDigest>>,
    pub mempool: Mempool,
    pub local_mp_interface: LocalMempoolService,
    pub chain_metadata_handle: ChainMetadataHandle,
    pub liveness_handle: LivenessHandle,
    pub comms: CommsNode,
    pub mock_base_node_state_machine: MockBaseNodeStateMachine,
}

/// The BaseNodeBuilder can be used to construct a test Base Node with all its relevant services and interfaces for
/// testing.
pub struct BaseNodeBuilder {
    node_identity: Option<Arc<NodeIdentity>>,
    peers: Option<Vec<Arc<NodeIdentity>>>,
    blockchain_db_config: Option<BlockchainDatabaseConfig>,
    base_node_service_config: Option<BaseNodeServiceConfig>,
    mmr_cache_config: Option<MmrCacheConfig>,
    mempool_config: Option<MempoolConfig>,
    mempool_service_config: Option<MempoolServiceConfig>,
    liveness_service_config: Option<LivenessConfig>,
    validators: Option<Validators<MemoryDatabase<HashDigest>>>,
    consensus_manager: Option<ConsensusManager>,
    network: Network,
}

impl BaseNodeBuilder {
    /// Create a new BaseNodeBuilder
    pub fn new(network: Network) -> Self {
        Self {
            node_identity: None,
            peers: None,
            blockchain_db_config: None,
            base_node_service_config: None,
            mmr_cache_config: None,
            mempool_config: None,
            mempool_service_config: None,
            liveness_service_config: None,
            validators: None,
            consensus_manager: None,
            network,
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

    /// Set the configuration of the Blockchain db
    pub fn with_blockchain_db_config(mut self, config: BlockchainDatabaseConfig) -> Self {
        self.blockchain_db_config = Some(config);
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
        block: impl StatefulValidation<Block, MemoryDatabase<HashDigest>> + 'static,
        orphan: impl Validation<Block> + 'static,
    ) -> Self
    {
        let validators = Validators::new(block, orphan);
        self.validators = Some(validators);
        self
    }

    /// Set the configuration of the Consensus Manager
    pub fn with_consensus_manager(mut self, consensus_manager: ConsensusManager) -> Self {
        self.consensus_manager = Some(consensus_manager);
        self
    }

    /// Build the test base node and start its services.
    pub fn start(self, runtime: &mut Runtime, data_path: &str) -> (NodeInterfaces, ConsensusManager) {
        let mmr_cache_config = self.mmr_cache_config.unwrap_or(MmrCacheConfig { rewind_hist_len: 10 });
        let validators = self
            .validators
            .unwrap_or(Validators::new(MockValidator::new(true), MockValidator::new(true)));
        let consensus_manager = self
            .consensus_manager
            .unwrap_or(ConsensusManagerBuilder::new(self.network).build());
        let db = MemoryDatabase::<HashDigest>::new(mmr_cache_config);
        let blockchain_db_config = self.blockchain_db_config.unwrap_or(BlockchainDatabaseConfig::default());
        let blockchain_db = BlockchainDatabase::new(db, &consensus_manager, validators, blockchain_db_config).unwrap();
        let mempool_validator = MempoolValidators::new(
            TxInputAndMaturityValidator::new(blockchain_db.clone()),
            TxInputAndMaturityValidator::new(blockchain_db.clone()),
        );
        let mempool = Mempool::new(
            self.mempool_config.unwrap_or(MempoolConfig::default()),
            mempool_validator,
        );
        let node_identity = self.node_identity.unwrap_or(random_node_identity());
        let (
            outbound_nci,
            local_nci,
            outbound_mp_interface,
            local_mp_interface,
            outbound_message_service,
            chain_metadata_handle,
            liveness_handle,
            comms,
            mock_base_node_state_machine,
        ) = setup_base_node_services(
            runtime,
            node_identity.clone(),
            self.peers.unwrap_or(Vec::new()),
            blockchain_db.clone(),
            mempool.clone(),
            consensus_manager.clone(),
            self.base_node_service_config
                .unwrap_or(BaseNodeServiceConfig::default()),
            self.mempool_service_config.unwrap_or(MempoolServiceConfig::default()),
            self.liveness_service_config.unwrap_or(LivenessConfig::default()),
            data_path,
        );

        (
            NodeInterfaces {
                node_identity,
                outbound_nci,
                local_nci,
                outbound_mp_interface,
                outbound_message_service,
                blockchain_db,
                mempool,
                local_mp_interface,
                chain_metadata_handle,
                liveness_handle,
                comms,
                mock_base_node_state_machine,
            },
            consensus_manager,
        )
    }
}

pub fn wait_until_online(runtime: &mut Runtime, nodes: &[&NodeInterfaces]) {
    for node in nodes {
        runtime
            .block_on(node.comms.connectivity().wait_for_connectivity(Duration::from_secs(10)))
            .map_err(|err| format!("Node '{}' failed to go online {:?}", node.node_identity.node_id(), err))
            .unwrap();
    }
}

// Creates a network with two Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_2_base_nodes(
    runtime: &mut Runtime,
    data_path: &str,
) -> (NodeInterfaces, NodeInterfaces, ConsensusManager)
{
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();

    let network = Network::LocalNet;
    let (alice_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .start(runtime, data_path);
    let (bob_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(bob_node_identity)
        .with_peers(vec![alice_node_identity])
        .with_consensus_manager(consensus_manager)
        .start(runtime, data_path);

    wait_until_online(runtime, &[&alice_node, &bob_node]);

    (alice_node, bob_node, consensus_manager)
}

// Creates a network with two Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_2_base_nodes_with_config<P: AsRef<Path>>(
    runtime: &mut Runtime,
    blockchain_db_config: BlockchainDatabaseConfig,
    base_node_service_config: BaseNodeServiceConfig,
    mmr_cache_config: MmrCacheConfig,
    mempool_service_config: MempoolServiceConfig,
    liveness_service_config: LivenessConfig,
    consensus_manager: ConsensusManager,
    data_path: P,
) -> (NodeInterfaces, NodeInterfaces, ConsensusManager)
{
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let network = Network::LocalNet;
    let (alice_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(alice_node_identity.clone())
        .with_blockchain_db_config(blockchain_db_config)
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(runtime, data_path.as_ref().join("alice").as_os_str().to_str().unwrap());
    let (bob_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(bob_node_identity)
        .with_blockchain_db_config(blockchain_db_config)
        .with_peers(vec![alice_node_identity])
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(runtime, data_path.as_ref().join("bob").as_os_str().to_str().unwrap());

    wait_until_online(runtime, &[&alice_node, &bob_node]);

    (alice_node, bob_node, consensus_manager)
}

// Creates a network with three Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_3_base_nodes(
    runtime: &mut Runtime,
    data_path: &str,
) -> (NodeInterfaces, NodeInterfaces, NodeInterfaces, ConsensusManager)
{
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    let mmr_cache_config = MmrCacheConfig { rewind_hist_len: 10 };
    create_network_with_3_base_nodes_with_config(
        runtime,
        BlockchainDatabaseConfig::default(),
        BaseNodeServiceConfig::default(),
        mmr_cache_config,
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        data_path,
    )
}

// Creates a network with three Base Nodes where each node in the network knows the other nodes in the network.
pub fn create_network_with_3_base_nodes_with_config<P: AsRef<Path>>(
    runtime: &mut Runtime,
    blockchain_db_config: BlockchainDatabaseConfig,
    base_node_service_config: BaseNodeServiceConfig,
    mmr_cache_config: MmrCacheConfig,
    mempool_service_config: MempoolServiceConfig,
    liveness_service_config: LivenessConfig,
    consensus_manager: ConsensusManager,
    data_path: P,
) -> (NodeInterfaces, NodeInterfaces, NodeInterfaces, ConsensusManager)
{
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let carol_node_identity = random_node_identity();
    let network = Network::LocalNet;

    log::info!(
        "Alice = {}, Bob = {}, Carol = {}",
        alice_node_identity.node_id().short_str(),
        bob_node_identity.node_id().short_str(),
        carol_node_identity.node_id().short_str()
    );
    let (carol_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(carol_node_identity.clone())
        .with_blockchain_db_config(blockchain_db_config)
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(runtime, data_path.as_ref().join("carol").as_os_str().to_str().unwrap());
    let (bob_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![carol_node_identity.clone()])
        .with_blockchain_db_config(blockchain_db_config)
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(runtime, data_path.as_ref().join("bob").as_os_str().to_str().unwrap());
    let (alice_node, consensus_manager) = BaseNodeBuilder::new(network)
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone(), carol_node_identity.clone()])
        .with_blockchain_db_config(blockchain_db_config)
        .with_base_node_service_config(base_node_service_config)
        .with_mmr_cache_config(mmr_cache_config)
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(runtime, data_path.as_ref().join("alice").as_os_str().to_str().unwrap());

    wait_until_online(runtime, &[&alice_node, &bob_node, &carol_node]);

    (alice_node, bob_node, carol_node, consensus_manager)
}

fn random_string(len: usize) -> String {
    iter::repeat(()).map(|_| OsRng.sample(Alphanumeric)).take(len).collect()
}

// Helper function for creating a random node indentity.
pub fn random_node_identity() -> Arc<NodeIdentity> {
    let next_port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(
        NodeIdentity::random(
            &mut OsRng,
            format!("/memory/{}", next_port).parse().unwrap(),
            PeerFeatures::COMMUNICATION_NODE,
        )
        .unwrap(),
    )
}

// Helper function for starting the comms stack.
async fn setup_comms_services<TSink>(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    publisher: InboundDomainConnector<TSink>,
    data_path: &str,
) -> (CommsNode, Dht)
where
    TSink: Sink<Arc<PeerMessage>> + Clone + Unpin + Send + Sync + 'static,
    TSink::Error: Error + Send + Sync,
{
    let peers = peers.into_iter().map(|p| p.to_peer()).collect();
    let (comms, dht) =
        initialize_local_test_comms(node_identity, publisher, data_path, Duration::from_secs(2 * 60), peers)
            .await
            .unwrap();

    (comms, dht)
}

// Helper function for starting the services of the Base node.
fn setup_base_node_services(
    runtime: &mut Runtime,
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    blockchain_db: BlockchainDatabase<MemoryDatabase<HashDigest>>,
    mempool: Mempool,
    consensus_manager: ConsensusManager,
    base_node_service_config: BaseNodeServiceConfig,
    mempool_service_config: MempoolServiceConfig,
    liveness_service_config: LivenessConfig,
    data_path: &str,
) -> (
    OutboundNodeCommsInterface,
    LocalNodeCommsInterface,
    OutboundMempoolServiceInterface,
    LocalMempoolService,
    OutboundMessageRequester,
    ChainMetadataHandle,
    LivenessHandle,
    CommsNode,
    MockBaseNodeStateMachine,
)
{
    let (publisher, subscription_factory) = pubsub_connector(runtime.handle().clone(), 100, 20);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht) = runtime.block_on(setup_comms_services(node_identity, peers, publisher, data_path));

    let mock_state_machine = MockBaseNodeStateMachine::new();

    let fut = StackBuilder::new(runtime.handle().clone(), comms.shutdown_signal())
        .add_initializer(CommsOutboundServiceInitializer::new(dht.outbound_requester()))
        .add_initializer(LivenessInitializer::new(
            liveness_service_config,
            Arc::clone(&subscription_factory),
            dht.dht_requester(),
        ))
        .add_initializer(BaseNodeServiceInitializer::new(
            subscription_factory.clone(),
            blockchain_db.clone(),
            mempool.clone(),
            consensus_manager.clone(),
            base_node_service_config,
        ))
        .add_initializer(MempoolServiceInitializer::new(
            subscription_factory,
            mempool,
            mempool_service_config,
        ))
        .add_initializer(mock_state_machine.get_initializer())
        .add_initializer(ChainMetadataServiceInitializer)
        .finish();

    let handles = runtime.block_on(fut).expect("Service initialization failed");
    (
        handles.get_handle::<OutboundNodeCommsInterface>().unwrap(),
        handles.get_handle::<LocalNodeCommsInterface>().unwrap(),
        handles.get_handle::<OutboundMempoolServiceInterface>().unwrap(),
        handles.get_handle::<LocalMempoolService>().unwrap(),
        handles.get_handle::<OutboundMessageRequester>().unwrap(),
        handles.get_handle::<ChainMetadataHandle>().unwrap(),
        handles.get_handle::<LivenessHandle>().unwrap(),
        comms,
        mock_state_machine,
    )
}

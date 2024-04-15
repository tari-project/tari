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

use std::{
    path::Path,
    sync::{Arc, RwLock},
    time::Duration,
};

use rand::rngs::OsRng;
use tari_common::configuration::Network;
use tari_comms::{
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::{messaging::MessagingEventSender, rpc::RpcServer},
    transports::MemoryTransport,
    CommsNode,
    UnspawnedCommsNode,
};
use tari_comms_dht::{outbound::OutboundMessageRequester, Dht};
use tari_core::{
    base_node,
    base_node::{
        chain_metadata_service::{ChainMetadataHandle, ChainMetadataServiceInitializer},
        comms_interface::OutboundNodeCommsInterface,
        service::BaseNodeServiceInitializer,
        LocalNodeCommsInterface,
        StateMachineHandle,
    },
    chain_storage::{BlockchainDatabase, BlockchainDatabaseConfig, Validators},
    consensus::{ConsensusManager, ConsensusManagerBuilder, NetworkConsensus},
    mempool::{
        service::{LocalMempoolService, MempoolHandle},
        Mempool,
        MempoolConfig,
        MempoolServiceConfig,
        MempoolServiceInitializer,
        OutboundMempoolServiceInterface,
    },
    proof_of_work::randomx_factory::RandomXFactory,
    test_helpers::blockchain::{create_store_with_consensus_and_validators_and_config, TempDatabase},
    validation::{
        mocks::MockValidator,
        transaction::TransactionChainLinkedValidator,
        CandidateBlockValidator,
        HeaderChainLinkedValidator,
        InternalConsistencyValidator,
    },
    OutputSmt,
};
use tari_p2p::{
    comms_connector::{pubsub_connector, InboundDomainConnector},
    initialization::initialize_local_test_comms,
    services::liveness::{config::LivenessConfig, LivenessHandle, LivenessInitializer},
    P2pConfig,
};
use tari_service_framework::{RegisterHandle, StackBuilder};
use tari_shutdown::Shutdown;

use crate::helpers::mock_state_machine::MockBaseNodeStateMachine;

/// The NodeInterfaces is used as a container for providing access to all the services and interfaces of a single node.
pub struct NodeInterfaces {
    pub node_identity: Arc<NodeIdentity>,
    pub outbound_nci: OutboundNodeCommsInterface,
    pub local_nci: LocalNodeCommsInterface,
    pub outbound_mp_interface: OutboundMempoolServiceInterface,
    pub outbound_message_service: OutboundMessageRequester,
    pub blockchain_db: BlockchainDatabase<TempDatabase>,
    pub mempool: Mempool,
    pub mempool_handle: MempoolHandle,
    pub local_mp_interface: LocalMempoolService,
    pub chain_metadata_handle: ChainMetadataHandle,
    pub liveness_handle: LivenessHandle,
    pub comms: CommsNode,
    pub mock_base_node_state_machine: MockBaseNodeStateMachine,
    pub state_machine_handle: StateMachineHandle,
    pub messaging_events: MessagingEventSender,
    pub shutdown: Shutdown,
}

#[allow(dead_code)]
impl NodeInterfaces {
    pub async fn shutdown(mut self) {
        self.shutdown.trigger();
        self.comms.wait_until_shutdown().await;
    }
}

/// The BaseNodeBuilder can be used to construct a test Base Node with all its relevant services and interfaces for
/// testing.
pub struct BaseNodeBuilder {
    node_identity: Option<Arc<NodeIdentity>>,
    peers: Option<Vec<Arc<NodeIdentity>>>,
    mempool_config: Option<MempoolConfig>,
    mempool_service_config: Option<MempoolServiceConfig>,
    liveness_service_config: Option<LivenessConfig>,
    p2p_config: Option<P2pConfig>,
    validators: Option<Validators<TempDatabase>>,
    consensus_manager: Option<ConsensusManager>,
    network: NetworkConsensus,
}

#[allow(dead_code)]
impl BaseNodeBuilder {
    /// Create a new BaseNodeBuilder
    pub fn new(network: NetworkConsensus) -> Self {
        Self {
            node_identity: None,
            peers: None,
            mempool_config: None,
            mempool_service_config: None,
            liveness_service_config: None,
            p2p_config: None,
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

    /// Set the p2p configuration
    pub fn with_p2p_config(mut self, config: P2pConfig) -> Self {
        self.p2p_config = Some(config);
        self
    }

    pub fn with_validators(
        mut self,
        block: impl CandidateBlockValidator<TempDatabase> + 'static,
        header: impl HeaderChainLinkedValidator<TempDatabase> + 'static,
        orphan: impl InternalConsistencyValidator + 'static,
    ) -> Self {
        let validators = Validators::new(block, header, orphan);
        self.validators = Some(validators);
        self
    }

    /// Set the configuration of the Consensus Manager
    pub fn with_consensus_manager(mut self, consensus_manager: ConsensusManager) -> Self {
        self.consensus_manager = Some(consensus_manager);
        self
    }

    /// Build the test base node and start its services.
    #[allow(clippy::redundant_closure)]
    pub async fn start(
        self,
        data_path: &str,
        blockchain_db_config: BlockchainDatabaseConfig,
    ) -> (NodeInterfaces, ConsensusManager) {
        let validators = self.validators.unwrap_or_else(|| {
            Validators::new(
                MockValidator::new(true),
                MockValidator::new(true),
                MockValidator::new(true),
            )
        });
        let network = self.network.as_network();
        let consensus_manager = self
            .consensus_manager
            .unwrap_or_else(|| ConsensusManagerBuilder::new(network).build().unwrap());
        let smt = Arc::new(RwLock::new(OutputSmt::new()));
        let blockchain_db = create_store_with_consensus_and_validators_and_config(
            consensus_manager.clone(),
            validators,
            blockchain_db_config,
            smt.clone(),
        );
        let mempool_validator = TransactionChainLinkedValidator::new(blockchain_db.clone(), consensus_manager.clone());
        let mempool = Mempool::new(
            self.mempool_config.unwrap_or_default(),
            consensus_manager.clone(),
            Box::new(mempool_validator),
        );

        let node_identity = self.node_identity.unwrap_or_else(|| random_node_identity());
        let node_interfaces = setup_base_node_services(
            node_identity,
            self.peers.unwrap_or_default(),
            blockchain_db,
            mempool,
            consensus_manager.clone(),
            self.liveness_service_config.unwrap_or_default(),
            self.p2p_config.unwrap_or_default(),
            data_path,
        )
        .await;

        (node_interfaces, consensus_manager)
    }
}

pub async fn wait_until_online(nodes: &[&NodeInterfaces]) {
    for node in nodes {
        node.comms
            .connectivity()
            .wait_for_connectivity(Duration::from_secs(10))
            .await
            .map_err(|err| format!("Node '{}' failed to go online {:?}", node.node_identity.node_id(), err))
            .unwrap();
    }
}

// Creates a network with multiple Base Nodes where each node in the network knows the other nodes in the network.
pub async fn create_network_with_multiple_base_nodes_with_config<P: AsRef<Path>>(
    mempool_service_configs: Vec<MempoolServiceConfig>,
    liveness_service_configs: Vec<LivenessConfig>,
    blockchain_db_configs: Vec<BlockchainDatabaseConfig>,
    p2p_configs: Vec<P2pConfig>,
    consensus_manager: ConsensusManager,
    data_path: P,
    network: Network,
) -> (Vec<NodeInterfaces>, ConsensusManager) {
    let num_of_nodes = mempool_service_configs.len();
    if num_of_nodes != liveness_service_configs.len() ||
        num_of_nodes != blockchain_db_configs.len() ||
        num_of_nodes != p2p_configs.len()
    {
        panic!("create_network_with_multiple_base_nodes_with_config: All configs must be the same length");
    }
    let mut node_identities = Vec::with_capacity(num_of_nodes);
    for i in 0..num_of_nodes {
        node_identities.push(random_node_identity());
        log::info!(
            "node identity {} = `{}`",
            i + 1,
            node_identities[node_identities.len() - 1].node_id().short_str()
        );
    }
    let mut node_interfaces = Vec::with_capacity(num_of_nodes);
    for i in 0..num_of_nodes {
        let (node, _) = BaseNodeBuilder::new(network.into())
            .with_node_identity(node_identities[i].clone())
            .with_peers(node_identities.iter().take(i).cloned().collect())
            .with_mempool_service_config(mempool_service_configs[i].clone())
            .with_liveness_service_config(liveness_service_configs[i].clone())
            .with_p2p_config(p2p_configs[i].clone())
            .with_consensus_manager(consensus_manager.clone())
            .start(
                data_path.as_ref().join(i.to_string()).as_os_str().to_str().unwrap(),
                blockchain_db_configs[i],
            )
            .await;
        node_interfaces.push(node);
    }

    let node_interface_refs = node_interfaces.iter().collect::<Vec<&NodeInterfaces>>();
    wait_until_online(node_interface_refs.as_slice()).await;

    (node_interfaces, consensus_manager)
}

// Helper function for creating a random node indentity.
#[allow(dead_code)]
pub fn random_node_identity() -> Arc<NodeIdentity> {
    let next_port = MemoryTransport::acquire_next_memsocket_port();
    Arc::new(NodeIdentity::random(
        &mut OsRng,
        format!("/memory/{}", next_port).parse().unwrap(),
        PeerFeatures::COMMUNICATION_NODE,
    ))
}

// Helper function for starting the comms stack.
#[allow(dead_code)]
async fn setup_comms_services(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    publisher: InboundDomainConnector,
    data_path: &str,
    shutdown: &Shutdown,
) -> (UnspawnedCommsNode, Dht, MessagingEventSender) {
    let peers = peers.into_iter().map(|p| p.to_peer()).collect();

    let (comms, dht, messaging_events) = initialize_local_test_comms(
        node_identity,
        publisher,
        data_path,
        Duration::from_secs(2 * 60),
        peers,
        shutdown.to_signal(),
    )
    .await
    .unwrap();

    (comms, dht, messaging_events)
}

// Helper function for starting the services of the Base node.
async fn setup_base_node_services(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    blockchain_db: BlockchainDatabase<TempDatabase>,
    mempool: Mempool,
    consensus_manager: ConsensusManager,
    liveness_service_config: LivenessConfig,
    p2p_config: P2pConfig,
    data_path: &str,
) -> NodeInterfaces {
    let (publisher, subscription_factory) = pubsub_connector(100);
    let subscription_factory = Arc::new(subscription_factory);
    let shutdown = Shutdown::new();

    let (comms, dht, messaging_events) =
        setup_comms_services(node_identity.clone(), peers, publisher.clone(), data_path, &shutdown).await;

    let mock_state_machine = MockBaseNodeStateMachine::new();
    let randomx_factory = RandomXFactory::new(2);
    let handles = StackBuilder::new(shutdown.to_signal())
        .add_initializer(RegisterHandle::new(dht))
        .add_initializer(RegisterHandle::new(comms.connectivity()))
        .add_initializer(LivenessInitializer::new(
            liveness_service_config,
            Arc::clone(&subscription_factory),
        ))
        .add_initializer(BaseNodeServiceInitializer::new(
            subscription_factory.clone(),
            blockchain_db.clone().into(),
            mempool.clone(),
            consensus_manager,
            Duration::from_secs(60),
            randomx_factory,
            Default::default(),
        ))
        .add_initializer(MempoolServiceInitializer::new(mempool.clone(), subscription_factory))
        .add_initializer(mock_state_machine.get_initializer())
        .add_initializer(ChainMetadataServiceInitializer)
        .build()
        .await
        .unwrap();

    let base_node_service = handles.expect_handle::<LocalNodeCommsInterface>();
    let rpc_server = RpcServer::builder()
        .with_maximum_simultaneous_sessions(p2p_config.rpc_max_simultaneous_sessions)
        .with_maximum_sessions_per_client(p2p_config.rpc_max_sessions_per_peer)
        .finish();
    let rpc_server = rpc_server.add_service(base_node::create_base_node_sync_rpc_service(
        blockchain_db.clone().into(),
        base_node_service,
    ));
    let mut comms = comms
        .add_protocol_extension(rpc_server)
        .spawn_with_transport(MemoryTransport)
        .await
        .unwrap();
    // Set the public address for tests
    let address = comms
        .connection_manager_requester()
        .wait_until_listening()
        .await
        .unwrap();
    comms.node_identity().add_public_address(address.bind_address().clone());

    let outbound_nci = handles.expect_handle::<OutboundNodeCommsInterface>();
    let local_nci = handles.expect_handle::<LocalNodeCommsInterface>();
    let outbound_mp_interface = handles.expect_handle::<OutboundMempoolServiceInterface>();
    let local_mp_interface = handles.expect_handle::<LocalMempoolService>();
    let mempool_handle = handles.expect_handle::<MempoolHandle>();
    let outbound_message_service = handles.expect_handle::<Dht>().outbound_requester();
    let chain_metadata_handle = handles.expect_handle::<ChainMetadataHandle>();
    let liveness_handle = handles.expect_handle::<LivenessHandle>();
    let state_machine_handle = handles.expect_handle::<StateMachineHandle>();

    NodeInterfaces {
        node_identity,
        outbound_nci,
        local_nci,
        outbound_mp_interface,
        outbound_message_service,
        blockchain_db,
        mempool,
        local_mp_interface,
        mempool_handle,
        chain_metadata_handle,
        liveness_handle,
        comms,
        messaging_events,
        mock_base_node_state_machine: mock_state_machine,
        shutdown,
        state_machine_handle,
    }
}

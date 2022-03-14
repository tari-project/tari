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

use std::{path::Path, sync::Arc, time::Duration};

use rand::rngs::OsRng;
use tari_common::configuration::Network;
use tari_comms::{
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::messaging::MessagingEventSender,
    transports::MemoryTransport,
    CommsNode,
};
use tari_comms_dht::{outbound::OutboundMessageRequester, Dht};
use tari_core::{
    base_node::{
        chain_metadata_service::{ChainMetadataHandle, ChainMetadataServiceInitializer},
        comms_interface::OutboundNodeCommsInterface,
        service::BaseNodeServiceInitializer,
        LocalNodeCommsInterface,
        StateMachineHandle,
    },
    chain_storage::{BlockchainDatabase, Validators},
    consensus::{ConsensusManager, ConsensusManagerBuilder, NetworkConsensus},
    mempool::{
        service::{LocalMempoolService, MempoolHandle},
        Mempool,
        MempoolConfig,
        MempoolServiceConfig,
        MempoolServiceInitializer,
        OutboundMempoolServiceInterface,
    },
    test_helpers::blockchain::{create_store_with_consensus_and_validators, TempDatabase},
    validation::{
        mocks::MockValidator,
        transaction_validators::TxInputAndMaturityValidator,
        HeaderValidation,
        OrphanValidation,
        PostOrphanBodyValidation,
    },
};
use tari_p2p::{
    comms_connector::{pubsub_connector, InboundDomainConnector},
    initialization::initialize_local_test_comms,
    services::liveness::{LivenessConfig, LivenessHandle, LivenessInitializer},
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

    pub fn with_validators(
        mut self,
        block: impl PostOrphanBodyValidation<TempDatabase> + 'static,
        header: impl HeaderValidation<TempDatabase> + 'static,
        orphan: impl OrphanValidation + 'static,
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
    pub async fn start(self, data_path: &str) -> (NodeInterfaces, ConsensusManager) {
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
            .unwrap_or_else(|| ConsensusManagerBuilder::new(network).build());
        let blockchain_db = create_store_with_consensus_and_validators(consensus_manager.clone(), validators);
        let mempool_validator = TxInputAndMaturityValidator::new(blockchain_db.clone());
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

// Creates a network with two Base Nodes where each node in the network knows the other nodes in the network.
#[allow(dead_code)]
pub async fn create_network_with_2_base_nodes(data_path: &str) -> (NodeInterfaces, NodeInterfaces, ConsensusManager) {
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();

    let network = Network::LocalNet;
    let (alice_node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .with_node_identity(alice_node_identity.clone())
        .with_peers(vec![bob_node_identity.clone()])
        .start(data_path)
        .await;
    let (bob_node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .with_node_identity(bob_node_identity)
        .with_peers(vec![alice_node_identity])
        .with_consensus_manager(consensus_manager)
        .start(data_path)
        .await;

    wait_until_online(&[&alice_node, &bob_node]).await;

    (alice_node, bob_node, consensus_manager)
}

// Creates a network with two Base Nodes where each node in the network knows the other nodes in the network.
#[allow(dead_code)]
pub async fn create_network_with_2_base_nodes_with_config<P: AsRef<Path>>(
    mempool_service_config: MempoolServiceConfig,
    liveness_service_config: LivenessConfig,
    consensus_manager: ConsensusManager,
    data_path: P,
) -> (NodeInterfaces, NodeInterfaces, ConsensusManager) {
    let alice_node_identity = random_node_identity();
    let bob_node_identity = random_node_identity();
    let network = Network::LocalNet;
    let (alice_node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .with_node_identity(alice_node_identity.clone())
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(data_path.as_ref().join("alice").as_os_str().to_str().unwrap())
        .await;
    let (bob_node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .with_node_identity(bob_node_identity)
        .with_peers(vec![alice_node_identity])
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config)
        .with_consensus_manager(consensus_manager)
        .start(data_path.as_ref().join("bob").as_os_str().to_str().unwrap())
        .await;

    wait_until_online(&[&alice_node, &bob_node]).await;

    (alice_node, bob_node, consensus_manager)
}

// Creates a network with three Base Nodes where each node in the network knows the other nodes in the network.
#[allow(dead_code)]
pub async fn create_network_with_3_base_nodes(
    data_path: &str,
) -> (NodeInterfaces, NodeInterfaces, NodeInterfaces, ConsensusManager) {
    let network = Network::LocalNet;
    let consensus_manager = ConsensusManagerBuilder::new(network).build();
    create_network_with_3_base_nodes_with_config(
        MempoolServiceConfig::default(),
        LivenessConfig::default(),
        consensus_manager,
        data_path,
    )
    .await
}

// Creates a network with three Base Nodes where each node in the network knows the other nodes in the network.
#[allow(dead_code)]
pub async fn create_network_with_3_base_nodes_with_config<P: AsRef<Path>>(
    mempool_service_config: MempoolServiceConfig,
    liveness_service_config: LivenessConfig,
    consensus_manager: ConsensusManager,
    data_path: P,
) -> (NodeInterfaces, NodeInterfaces, NodeInterfaces, ConsensusManager) {
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
    let (carol_node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .with_node_identity(carol_node_identity.clone())
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(data_path.as_ref().join("carol").as_os_str().to_str().unwrap())
        .await;
    let (bob_node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .with_node_identity(bob_node_identity.clone())
        .with_peers(vec![carol_node_identity.clone()])
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config.clone())
        .with_consensus_manager(consensus_manager)
        .start(data_path.as_ref().join("bob").as_os_str().to_str().unwrap())
        .await;
    let (alice_node, consensus_manager) = BaseNodeBuilder::new(network.into())
        .with_node_identity(alice_node_identity)
        .with_peers(vec![bob_node_identity, carol_node_identity])
        .with_mempool_service_config(mempool_service_config)
        .with_liveness_service_config(liveness_service_config)
        .with_consensus_manager(consensus_manager)
        .start(data_path.as_ref().join("alice").as_os_str().to_str().unwrap())
        .await;

    wait_until_online(&[&alice_node, &bob_node, &carol_node]).await;

    (alice_node, bob_node, carol_node, consensus_manager)
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
) -> (CommsNode, Dht, MessagingEventSender, Shutdown) {
    let peers = peers.into_iter().map(|p| p.to_peer()).collect();
    let shutdown = Shutdown::new();
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

    (comms, dht, messaging_events, shutdown)
}

// Helper function for starting the services of the Base node.
async fn setup_base_node_services(
    node_identity: Arc<NodeIdentity>,
    peers: Vec<Arc<NodeIdentity>>,
    blockchain_db: BlockchainDatabase<TempDatabase>,
    mempool: Mempool,
    consensus_manager: ConsensusManager,
    liveness_service_config: LivenessConfig,
    data_path: &str,
) -> NodeInterfaces {
    let (publisher, subscription_factory) = pubsub_connector(100, 20);
    let subscription_factory = Arc::new(subscription_factory);
    let (comms, dht, messaging_events, shutdown) =
        setup_comms_services(node_identity.clone(), peers, publisher, data_path).await;

    let mock_state_machine = MockBaseNodeStateMachine::new();

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
        ))
        .add_initializer(MempoolServiceInitializer::new(mempool.clone(), subscription_factory))
        .add_initializer(mock_state_machine.get_initializer())
        .add_initializer(ChainMetadataServiceInitializer)
        .build()
        .await
        .unwrap();

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

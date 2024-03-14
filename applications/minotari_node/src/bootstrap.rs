//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{cmp, str::FromStr, sync::Arc, time::Duration};

use log::*;
use minotari_app_utilities::{consts, identity_management, identity_management::load_from_json};
use tari_common::{
    configuration::bootstrap::ApplicationType,
    exit_codes::{ExitCode, ExitError},
};
use tari_comms::{
    multiaddr::{Error as MultiaddrError, Multiaddr},
    peer_manager::Peer,
    protocol::rpc::RpcServer,
    tor::TorIdentity,
    NodeIdentity,
    UnspawnedCommsNode,
};
use tari_comms_dht::Dht;
use tari_core::{
    base_node,
    base_node::{
        chain_metadata_service::ChainMetadataServiceInitializer,
        service::BaseNodeServiceInitializer,
        state_machine_service::initializer::BaseNodeStateMachineInitializer,
        LocalNodeCommsInterface,
        StateMachineHandle,
    },
    chain_storage::{async_db::AsyncBlockchainDb, BlockchainBackend, BlockchainDatabase},
    consensus::ConsensusManager,
    mempool,
    mempool::{service::MempoolHandle, Mempool, MempoolServiceInitializer, MempoolSyncInitializer},
    proof_of_work::randomx_factory::RandomXFactory,
    transactions::CryptoFactories,
};
use tari_p2p::{
    auto_update::SoftwareUpdaterService,
    comms_connector::pubsub_connector,
    initialization,
    initialization::P2pInitializer,
    peer_seeds::SeedPeer,
    services::liveness::{config::LivenessConfig, LivenessInitializer},
    P2pConfig,
    TransportType,
};
use tari_service_framework::{ServiceHandles, StackBuilder};
use tari_shutdown::ShutdownSignal;

use crate::ApplicationConfig;

const LOG_TARGET: &str = "c::bn::initialization";
/// The minimum buffer size for the base node pubsub_connector channel
const BASE_NODE_BUFFER_MIN_SIZE: usize = 30;

pub struct BaseNodeBootstrapper<'a, B> {
    pub app_config: &'a ApplicationConfig,
    pub node_identity: Arc<NodeIdentity>,
    pub db: BlockchainDatabase<B>,
    pub mempool: Mempool,
    pub rules: ConsensusManager,
    pub factories: CryptoFactories,
    pub randomx_factory: RandomXFactory,
    pub interrupt_signal: ShutdownSignal,
}

impl<B> BaseNodeBootstrapper<'_, B>
where B: BlockchainBackend + 'static
{
    #[allow(clippy::too_many_lines)]
    pub async fn bootstrap(self) -> Result<ServiceHandles, ExitError> {
        let mut base_node_config = self.app_config.base_node.clone();
        let mut p2p_config = self.app_config.base_node.p2p.clone();
        let peer_seeds = &self.app_config.peer_seeds;

        let buf_size = cmp::max(BASE_NODE_BUFFER_MIN_SIZE, base_node_config.buffer_size);
        let (publisher, peer_message_subscriptions) = pubsub_connector(buf_size);
        let peer_message_subscriptions = Arc::new(peer_message_subscriptions);
        let mempool_config = base_node_config.mempool.service.clone();

        let sync_peers = base_node_config
            .force_sync_peers
            .iter()
            .map(|s| SeedPeer::from_str(s))
            .map(|r| r.map(Peer::from).map(|p| p.node_id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ExitError::new(ExitCode::ConfigError, e))?;

        base_node_config.state_machine.blockchain_sync_config.forced_sync_peers = sync_peers.clone();

        debug!(target: LOG_TARGET, "{} sync peer(s) configured", sync_peers.len());

        let mempool_sync = MempoolSyncInitializer::new(mempool_config, self.mempool.clone());
        let mempool_protocol = mempool_sync.get_protocol_extension();

        let tor_identity = load_from_json(&base_node_config.tor_identity_file)
            .map_err(|e| ExitError::new(ExitCode::ConfigError, e))?;
        p2p_config.transport.tor.identity = tor_identity;
        p2p_config.listener_liveness_check_interval = Some(Duration::from_secs(15));

        let mut handles = StackBuilder::new(self.interrupt_signal)
            .add_initializer(P2pInitializer::new(
                p2p_config.clone(),
                peer_seeds.clone(),
                base_node_config.network,
                self.node_identity.clone(),
                publisher,
            ))
            .add_initializer(SoftwareUpdaterService::new(
                ApplicationType::BaseNode,
                consts::APP_VERSION_NUMBER
                    .parse()
                    .expect("Unable to parse application version. Not valid semver"),
                self.app_config.auto_update.clone(),
            ))
            .add_initializer(BaseNodeServiceInitializer::new(
                peer_message_subscriptions.clone(),
                self.db.clone().into(),
                self.mempool.clone(),
                self.rules.clone(),
                base_node_config.messaging_request_timeout,
                self.randomx_factory.clone(),
                base_node_config.state_machine.clone(),
            ))
            .add_initializer(MempoolServiceInitializer::new(
                self.mempool.clone(),
                peer_message_subscriptions.clone(),
            ))
            .add_initializer(mempool_sync)
            .add_initializer(LivenessInitializer::new(
                LivenessConfig {
                    auto_ping_interval: Some(base_node_config.metadata_auto_ping_interval),
                    monitored_peers: sync_peers.clone(),
                    ..Default::default()
                },
                peer_message_subscriptions,
            ))
            .add_initializer(ChainMetadataServiceInitializer)
            .add_initializer(BaseNodeStateMachineInitializer::new(
                self.db.clone().into(),
                base_node_config.state_machine.clone(),
                self.rules,
                self.factories,
                self.randomx_factory,
                self.app_config.base_node.bypass_range_proof_verification,
            ))
            .build()
            .await?;

        let comms = handles
            .take_handle::<UnspawnedCommsNode>()
            .expect("P2pInitializer was not added to the stack or did not add UnspawnedCommsNode");

        let comms = comms.add_protocol_extension(mempool_protocol);
        let comms = Self::setup_rpc_services(comms, &handles, self.db.into(), &p2p_config);

        let comms = if p2p_config.transport.transport_type == TransportType::Tor {
            let tor_id_path = base_node_config.tor_identity_file.clone();
            let node_id_path = base_node_config.identity_file.clone();
            let node_id = comms.node_identity();
            let after_comms = move |identity: TorIdentity| {
                let address_string = format!("/onion3/{}:{}", identity.service_id, identity.onion_port);
                if let Err(e) = identity_management::save_as_json(&tor_id_path, &identity) {
                    error!(target: LOG_TARGET, "Failed to save tor identity{:?}", e);
                }
                trace!(target: LOG_TARGET, "resave the tor identity {:?}", identity);
                let result: Result<Multiaddr, MultiaddrError> = address_string.parse();
                if result.is_err() {
                    error!(target: LOG_TARGET, "Failed to parse tor identity as multiaddr{:?}", result);
                    return;
                }
                let address = result.unwrap();
                if !node_id.public_addresses().contains(&address) {
                    node_id.add_public_address(address);
                }
                if let Err(e) = identity_management::save_as_json(&node_id_path, &*node_id) {
                    error!(target: LOG_TARGET, "Failed to save node identity identity{:?}", e);
                }
            };
            initialization::spawn_comms_using_transport(comms, p2p_config.transport.clone(), after_comms).await
        } else {
            let after_comms = |_identity| {};
            initialization::spawn_comms_using_transport(comms, p2p_config.transport.clone(), after_comms).await
        };

        let comms = comms.map_err(|e| e.to_exit_error())?;
        // Save final node identity after comms has initialized. This is required because the public_address can be
        // changed by comms during initialization when using tor.
        match p2p_config.transport.transport_type {
            TransportType::Tcp => {}, // Do not overwrite TCP public_address in the base_node_id!
            _ => {
                identity_management::save_as_json(&base_node_config.identity_file, &*comms.node_identity())
                    .map_err(|e| ExitError::new(ExitCode::IdentityError, e))?;
            },
        };

        handles.register(comms);

        Ok(handles)
    }

    fn setup_rpc_services(
        comms: UnspawnedCommsNode,
        handles: &ServiceHandles,
        db: AsyncBlockchainDb<B>,
        config: &P2pConfig,
    ) -> UnspawnedCommsNode {
        let dht = handles.expect_handle::<Dht>();
        let base_node_service = handles.expect_handle::<LocalNodeCommsInterface>();
        let rpc_server = RpcServer::builder()
            .with_maximum_simultaneous_sessions(config.rpc_max_simultaneous_sessions)
            .with_maximum_sessions_per_client(config.rpc_max_sessions_per_peer)
            .finish();

        // Add your RPC services here ‍🏴‍☠️️☮️🌊
        let rpc_server = rpc_server
            .add_service(dht.rpc_service())
            .add_service(base_node::create_base_node_sync_rpc_service(
                db.clone(),
                base_node_service,
            ))
            .add_service(mempool::create_mempool_rpc_service(
                handles.expect_handle::<MempoolHandle>(),
            ))
            .add_service(base_node::rpc::create_base_node_wallet_rpc_service(
                db,
                handles.expect_handle::<MempoolHandle>(),
                handles.expect_handle::<StateMachineHandle>(),
            ));

        handles.register(rpc_server.get_handle());

        comms.add_protocol_extension(rpc_server)
    }
}

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

use std::{cmp, str::FromStr, sync::Arc};

use log::*;
use minotari_app_utilities::{consts, identity_management, identity_management::load_from_json};
use tari_common::{
    configuration::bootstrap::ApplicationType,
    exit_codes::{ExitCode, ExitError},
};
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
use tari_network::{
    identity,
    multiaddr::{Error as MultiaddrError, Multiaddr},
    MessageSpec,
    NetworkError,
    NetworkHandle,
    Peer,
};
use tari_p2p::{
    auto_update::SoftwareUpdaterService,
    connector::InboundMessaging,
    initialization,
    initialization::P2pInitializer,
    message::TariNodeMessageSpec,
    peer_seeds::SeedPeer,
    services::{
        dispatcher,
        liveness::{config::LivenessConfig, LivenessInitializer},
    },
    Dispatcher,
    P2pConfig,
};
use tari_rpc_framework::RpcServer;
use tari_service_framework::{RegisterHandle, ServiceHandles, StackBuilder};
use tari_shutdown::ShutdownSignal;
use tokio::sync::mpsc;

use crate::ApplicationConfig;

const LOG_TARGET: &str = "c::bn::initialization";
/// The minimum buffer size for the base node pubsub_connector channel
const BASE_NODE_BUFFER_MIN_SIZE: usize = 30;

pub struct BaseNodeBootstrapper<'a, B> {
    pub app_config: &'a ApplicationConfig,
    pub node_identity: Arc<identity::Keypair>,
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

        let mempool_config = base_node_config.mempool.service.clone();

        let sync_peers = base_node_config
            .force_sync_peers
            .iter()
            .map(|s| SeedPeer::from_str(s))
            .map(|r| r.map(Peer::from).map(|p| p.public_key().to_peer_id()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ExitError::new(ExitCode::ConfigError, e))?;

        base_node_config.state_machine.blockchain_sync_config.forced_sync_peers = sync_peers.clone();

        debug!(target: LOG_TARGET, "{} sync peer(s) configured", sync_peers.len());

        let dispatcher = Dispatcher::new();
        let user_agent = format!("tari/basenode/{}", consts::APP_VERSION_NUMBER);
        let mut handles = StackBuilder::new(self.interrupt_signal)
            .add_initializer(P2pInitializer::new(
                p2p_config.clone(),
                user_agent,
                peer_seeds.clone(),
                base_node_config.network,
                self.node_identity.clone(),
            ))
            .add_initializer(SoftwareUpdaterService::new(
                ApplicationType::BaseNode,
                consts::APP_VERSION_NUMBER
                    .parse()
                    .expect("Unable to parse application version. Not valid semver"),
                self.app_config.auto_update.clone(),
            ))
            .add_initializer(BaseNodeServiceInitializer::new(
                self.db.clone().into(),
                self.mempool.clone(),
                self.rules.clone(),
                base_node_config.messaging_request_timeout,
                self.randomx_factory.clone(),
                base_node_config.state_machine.clone(),
                dispatcher.clone(),
            ))
            .add_initializer(MempoolServiceInitializer::new(self.mempool.clone()))
            .add_initializer(MempoolSyncInitializer::new(mempool_config, self.mempool.clone()))
            .add_initializer(LivenessInitializer::new(
                LivenessConfig {
                    auto_ping_interval: Some(base_node_config.metadata_auto_ping_interval),
                    monitored_peers: sync_peers.clone(),
                    ..Default::default()
                },
                dispatcher.clone(),
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

        let network = handles.expect_handle::<NetworkHandle>();

        let inbound = handles
            .take_handle::<InboundMessaging<TariNodeMessageSpec>>()
            .expect("InboundMessaging not setup");
        dispatcher.spawn(inbound);

        Self::setup_rpc_services(network, &handles, self.db.into(), &p2p_config)
            .await
            .map_err(|e| ExitError::new(ExitCode::NetworkError, e))?;

        Ok(handles)
    }

    async fn setup_rpc_services(
        mut networking: NetworkHandle,
        handles: &ServiceHandles,
        db: AsyncBlockchainDb<B>,
        config: &P2pConfig,
    ) -> Result<(), NetworkError> {
        let base_node_service = handles.expect_handle::<LocalNodeCommsInterface>();
        let rpc_server = RpcServer::builder()
            .with_maximum_simultaneous_sessions(config.rpc_max_simultaneous_sessions)
            .with_maximum_sessions_per_client(config.rpc_max_sessions_per_peer)
            .finish();

        // Add your RPC services here ‍🏴‍☠️️☮️🌊
        let rpc_server = rpc_server
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

        let (notify_tx, notify_rx) = mpsc::unbounded_channel();
        networking
            .add_protocol_notifier(rpc_server.all_protocols().iter().cloned(), notify_tx)
            .await?;
        handles.register(rpc_server.get_handle());

        tokio::spawn(rpc_server.serve(notify_rx));
        Ok(())
    }
}

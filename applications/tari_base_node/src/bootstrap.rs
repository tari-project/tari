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
use tari_app_utilities::{consts, identity_management, identity_management::load_from_json};
use tari_common::{
    configuration::bootstrap::ApplicationType,
    exit_codes::{ExitCode, ExitError},
};
use tari_comms::{peer_manager::Peer, protocol::rpc::RpcServer, NodeIdentity, UnspawnedCommsNode};
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
    pub interrupt_signal: ShutdownSignal,
}

impl<B> BaseNodeBootstrapper<'_, B>
where B: BlockchainBackend + 'static
{
    pub async fn bootstrap(self) -> Result<ServiceHandles, ExitError> {
        let base_node_config = &self.app_config.base_node;
        let mut p2p_config = self.app_config.base_node.p2p.clone();
        let peer_seeds = &self.app_config.peer_seeds;

        let buf_size = cmp::max(BASE_NODE_BUFFER_MIN_SIZE, base_node_config.buffer_size);
        let (publisher, peer_message_subscriptions) = pubsub_connector(buf_size, base_node_config.buffer_rate_limit);
        let peer_message_subscriptions = Arc::new(peer_message_subscriptions);
        let mempool_config = base_node_config.mempool.service.clone();

        let sync_peers = base_node_config
            .force_sync_peers
            .iter()
            .map(|s| SeedPeer::from_str(s))
            .map(|r| r.map(Peer::from).map(|p| p.node_id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ExitError::new(ExitCode::ConfigError, &e))?;

        debug!(target: LOG_TARGET, "{} sync peer(s) configured", sync_peers.len());

        let mempool_sync = MempoolSyncInitializer::new(mempool_config, self.mempool.clone());
        let mempool_protocol = mempool_sync.get_protocol_extension();

        let tor_identity = load_from_json(&base_node_config.tor_identity_file)
            .map_err(|e| ExitError::new(ExitCode::ConfigError, &e))?;
        p2p_config.transport.tor.identity = tor_identity;

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
            ))
            .build()
            .await?;

        let comms = handles
            .take_handle::<UnspawnedCommsNode>()
            .expect("P2pInitializer was not added to the stack or did not add UnspawnedCommsNode");

        let comms = comms.add_protocol_extension(mempool_protocol);
        let comms = Self::setup_rpc_services(comms, &handles, self.db.into(), &p2p_config);
        let comms = initialization::spawn_comms_using_transport(comms, p2p_config.transport.clone())
            .await
            .map_err(|e| ExitError::new(ExitCode::NetworkError, &e))?;
        // Save final node identity after comms has initialized. This is required because the public_address can be
        // changed by comms during initialization when using tor.
        match p2p_config.transport.transport_type {
            TransportType::Tcp => {}, // Do not overwrite TCP public_address in the base_node_id!
            _ => {
                identity_management::save_as_json(&base_node_config.identity_file, &*comms.node_identity())
                    .map_err(|e| ExitError::new(ExitCode::IdentityError, &e))?;
            },
        };
        if let Some(hs) = comms.hidden_service() {
            identity_management::save_as_json(&base_node_config.tor_identity_file, hs.tor_identity())
                .map_err(|e| ExitError::new(ExitCode::IdentityError, &e))?;
        }

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

    // fn create_comms_config(&self) -> P2pConfig {
    //     P2pConfig {
    //         network: self.config.network,
    //         // node_identity: self.node_identity.clone(),
    //         // transport_type: ,
    //         auxilary_tcp_listener_address: self.config.auxilary_tcp_listener_address.clone(),
    //         datastore_path: self.config.comms_peer_db_path.clone(),
    //         peer_database_name: "peers".to_string(),
    //         max_concurrent_inbound_tasks: 50,
    //         max_concurrent_outbound_tasks: 100,
    //         outbound_buffer_size: 100,
    //         dht: DhtConfig {
    //             database_url: DbConnectionUrl::File(self.config.data_dir.join("dht.db")),
    //             auto_join: true,
    //             allow_test_addresses: self.config.comms_allow_test_addresses,
    //             flood_ban_max_msg_count: self.config.flood_ban_max_msg_count,
    //             saf_config: SafConfig {
    //                 msg_validity: self.config.saf_expiry_duration,
    //                 ..Default::default()
    //             },
    //             dedup_cache_capacity: self.config.dht_dedup_cache_capacity,
    //             ..Default::default()
    //         },
    //         allow_test_addresses: self.config.comms_allow_test_addresses,
    //         listener_liveness_allowlist_cidrs: self.config.comms_listener_liveness_allowlist_cidrs.clone(),
    //         listener_liveness_max_sessions: self.config.comms_listener_liveness_max_sessions,
    //         user_agent: format!("tari/basenode/{}", env!("CARGO_PKG_VERSION")),
    //         // Also add sync peers to the peer seed list. Duplicates are acceptable.
    //         peer_seeds: self
    //             .config
    //             .peer_seeds
    //             .iter()
    //             .cloned()
    //             .chain(self.config.force_sync_peers.clone())
    //             .collect(),
    //         dns_seeds: self.config.dns_seeds.clone(),
    //         dns_seeds_name_server: self.config.dns_seeds_name_server.clone(),
    //         dns_seeds_use_dnssec: self.config.dns_seeds_use_dnssec,
    //     }
    // }
}

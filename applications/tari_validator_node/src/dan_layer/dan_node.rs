// Copyright 2021. The Tari Project
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
    dan_layer::{
        models::{AssetDefinition, Committee},
        services::{
            infrastructure_services::{TariCommsInboundConnectionService, TariCommsOutboundService},
            ConcreteAssetProcessor,
            ConcreteCommitteeManager,
            GrpcBaseNodeClient,
            LoggingEventsPublisher,
            MemoryInstructionLog,
            MempoolService,
            MempoolServiceHandle,
            NodeIdentitySigningService,
            TariDanPayloadProcessor,
            TariDanPayloadProvider,
        },
        storage::{AssetDataStore, LmdbAssetStore},
        workers::ConsensusWorker,
    },
    p2p::create_validator_node_rpc_service,
    ExitCodes,
};
use log::*;
use std::{fs, fs::File, io::BufReader, path::Path, sync::Arc, time::Duration};
use tari_app_utilities::{
    identity_management,
    identity_management::{load_from_json, setup_node_identity},
    utilities::convert_socks_authentication,
};
use tari_common::{configuration::ValidatorNodeConfig, CommsTransport, GlobalConfig, TorControlAuthentication};
use tari_comms::{
    peer_manager::PeerFeatures,
    protocol::rpc::RpcServer,
    socks,
    tor,
    tor::TorIdentity,
    transports::{predicate::FalsePredicate, SocksConfig},
    types::CommsPublicKey,
    utils::multiaddr::multiaddr_to_socketaddr,
    NodeIdentity,
    UnspawnedCommsNode,
};
use tari_comms_dht::{store_forward::SafConfig, DbConnectionUrl, Dht, DhtConfig};
use tari_p2p::{
    comms_connector::{pubsub_connector, SubscriptionFactory},
    initialization::{spawn_comms_using_transport, P2pConfig, P2pInitializer},
    tari_message::TariMessageType,
    transport::{TorConfig, TransportType},
};
use tari_service_framework::{ServiceHandles, StackBuilder};
use tari_shutdown::ShutdownSignal;

const LOG_TARGET: &str = "tari::dan::dan_node";

pub struct DanNode {
    config: GlobalConfig,
}

impl DanNode {
    pub fn new(config: GlobalConfig) -> Self {
        Self { config }
    }

    pub async fn start(
        &self,
        create_id: bool,
        shutdown: ShutdownSignal,
        mempool_service: MempoolServiceHandle,
    ) -> Result<(), ExitCodes> {
        fs::create_dir_all(&self.config.peer_db_path).map_err(|err| ExitCodes::ConfigError(err.to_string()))?;
        let node_identity = setup_node_identity(
            &self.config.base_node_identity_file,
            &self.config.public_address,
            create_id,
            PeerFeatures::NONE,
        )?;

        info!(
            target: LOG_TARGET,
            "Node starting with pub key: {}, node_id: {}",
            node_identity.public_key(),
            node_identity.node_id()
        );
        let (handles, subscription_factory) = self
            .build_service_and_comms_stack(shutdown.clone(), node_identity.clone(), mempool_service.clone())
            .await?;

        let dan_config = self
            .config
            .validator_node
            .as_ref()
            .ok_or_else(|| ExitCodes::ConfigError("Missing dan section".to_string()))?;

        let asset_definitions = self.read_asset_definitions(&dan_config.asset_config_directory)?;
        if asset_definitions.is_empty() {
            warn!(target: LOG_TARGET, "No assets to process. Exiting");
        }
        let db_factory = SqliteDbFactory::new(&self.config);
        for asset in asset_definitions {
            // TODO: spawn into multiple processes. This requires some routing as well.
            self.start_asset_worker(
                asset,
                node_identity.as_ref().clone(),
                mempool_service.clone(),
                handles.clone(),
                subscription_factory.clone(),
                shutdown.clone(),
                dan_config,
                db_factory.clone(),
            )
            .await?;
        }
        Ok(())
    }

    fn read_asset_definitions(&self, path: &Path) -> Result<Vec<AssetDefinition>, ExitCodes> {
        if !path.exists() {
            fs::create_dir_all(path).expect("Could not create dir");
        }
        let paths = fs::read_dir(path).expect("Could not read asset definitions");

        let mut result = vec![];
        for path in paths {
            let path = path.expect("Not a valid file").path();
            if !path.is_dir() {
                let file = File::open(path).expect("could not open file");
                let reader = BufReader::new(file);

                let def: AssetDefinition = serde_json::from_reader(reader).expect("lol not a valid json");
                result.push(def);
            }
        }
        Ok(result)
    }

    async fn start_asset_worker<
        TMempoolService: MempoolService + Clone,
        TBackendAdapter: BackendAdapter + Send + Sync,
        TDbFactory: DbFactory<TBackendAdapter> + Clone + Send + Sync,
    >(
        &self,
        asset_definition: AssetDefinition,
        node_identity: NodeIdentity,
        mempool_service: TMempoolService,
        handles: ServiceHandles,
        subscription_factory: SubscriptionFactory,
        shutdown: ShutdownSignal,
        config: &ValidatorNodeConfig,
        db_factory: TDbFactory,
    ) -> Result<(), ExitCodes> {
        let timeout = Duration::from_secs(asset_definition.phase_timeout);
        // TODO: read from base layer get asset definition
        let committee = asset_definition
            .initial_committee
            .iter()
            .map(|s| {
                CommsPublicKey::from_hex(s)
                    .map_err(|e| ExitCodes::ConfigError(format!("could not convert to hex:{}", e)))
            })
            .collect::<Result<Vec<_>, _>>()?;

        // let committee: Vec<CommsPublicKey> = dan_config
        //     .committee
        //     .iter()
        //     .map(|s| {
        //         CommsPublicKey::from_hex(s)
        //             .map_err(|e| ExitCodes::ConfigError(format!("could not convert to hex:{}", e)))
        //     })
        //     .collect::<Result<Vec<_>, _>>()?;
        //
        let committee = Committee::new(committee);
        let committee_service = ConcreteCommitteeManager::new(committee);

        let payload_provider = TariDanPayloadProvider::new(mempool_service.clone());

        let events_publisher = LoggingEventsPublisher::new();
        let signing_service = NodeIdentitySigningService::new(node_identity.clone());

        let backend = LmdbAssetStore::initialize(self.config.data_dir.join("asset_data"), Default::default())
            .map_err(|err| ExitCodes::DatabaseError(err.to_string()))?;
        let data_store = AssetDataStore::new(backend);
        let instruction_log = MemoryInstructionLog::default();
        let asset_processor = ConcreteAssetProcessor::new(instruction_log, asset_definition.clone());

        let payload_processor = TariDanPayloadProcessor::new(asset_processor, mempool_service);
        let mut inbound = TariCommsInboundConnectionService::new();
        let receiver = inbound.take_receiver().unwrap();

        let loopback = inbound.clone_sender();
        let shutdown_2 = shutdown.clone();
        task::spawn(async move {
            let topic_subscription =
                subscription_factory.get_subscription(TariMessageType::DanConsensusMessage, "HotStufMessages");
            inbound.run(shutdown_2, topic_subscription).await
        });
        let dht = handles.expect_handle::<Dht>();
        let outbound = TariCommsOutboundService::new(dht.outbound_requester(), loopback);
        let base_node_client = GrpcBaseNodeClient::new(config.base_node_grpc_address);
        let chain_storage = SqliteStorageService {};
        let mut consensus_worker = ConsensusWorker::new(
            receiver,
            outbound,
            committee_service,
            node_identity.public_key().clone(),
            payload_provider,
            events_publisher,
            signing_service,
            payload_processor,
            asset_definition,
            base_node_client,
            timeout,
            db_factory,
            chain_storage,
        );
        consensus_worker
            .run(shutdown.clone(), None)
            .await
            .map_err(|err| ExitCodes::ConfigError(err.to_string()))?;

        Ok(())
    }

    async fn build_service_and_comms_stack(
        &self,
        shutdown: ShutdownSignal,
        node_identity: Arc<NodeIdentity>,
        mempool: MempoolServiceHandle,
    ) -> Result<(ServiceHandles, SubscriptionFactory), ExitCodes> {
        // this code is duplicated from the base node
        let comms_config = self.create_comms_config(node_identity.clone());

        let (publisher, peer_message_subscriptions) = pubsub_connector(100, self.config.buffer_rate_limit_base_node);

        let mut handles = StackBuilder::new(shutdown.clone())
            .add_initializer(P2pInitializer::new(comms_config, publisher))
            .build()
            .await
            .map_err(|err| ExitCodes::ConfigError(err.to_string()))?;

        let comms = handles
            .take_handle::<UnspawnedCommsNode>()
            .expect("P2pInitializer was not added to the stack or did not add UnspawnedCommsNode");

        let comms = self.setup_p2p_rpc(comms, &handles, mempool);

        let comms = spawn_comms_using_transport(comms, self.create_transport_type())
            .await
            .map_err(|e| ExitCodes::ConfigError(format!("Could not spawn using transport:{}", e)))?;

        // Save final node identity after comms has initialized. This is required because the public_address can be
        // changed by comms during initialization when using tor.
        identity_management::save_as_json(&self.config.base_node_identity_file, &*comms.node_identity())
            .map_err(|e| ExitCodes::ConfigError(format!("Failed to save node identity: {}", e)))?;
        if let Some(hs) = comms.hidden_service() {
            identity_management::save_as_json(&self.config.base_node_tor_identity_file, hs.tor_identity())
                .map_err(|e| ExitCodes::ConfigError(format!("Failed to save tor identity: {}", e)))?;
        }

        handles.register(comms);
        Ok((handles, peer_message_subscriptions))
    }

    fn setup_p2p_rpc(
        &self,
        comms: UnspawnedCommsNode,
        handles: &ServiceHandles,
        mempool: MempoolServiceHandle,
    ) -> UnspawnedCommsNode {
        let dht = handles.expect_handle::<Dht>();
        let builder = RpcServer::builder();
        let builder = match self.config.rpc_max_simultaneous_sessions {
            Some(limit) => builder.with_maximum_simultaneous_sessions(limit),
            None => {
                warn!(
                    target: LOG_TARGET,
                    "Node is configured to allow unlimited RPC sessions."
                );
                builder.with_unlimited_simultaneous_sessions()
            },
        };
        let rpc_server = builder.finish();

        // Add your RPC services here ‍🏴‍☠️️☮️🌊
        let rpc_server = rpc_server
            .add_service(dht.rpc_service())
            .add_service(create_validator_node_rpc_service(mempool));

        comms.add_protocol_extension(rpc_server)
    }

    fn create_comms_config(&self, node_identity: Arc<NodeIdentity>) -> P2pConfig {
        P2pConfig {
            network: self.config.network,
            node_identity,
            transport_type: self.create_transport_type(),
            datastore_path: self.config.peer_db_path.clone(),
            peer_database_name: "peers".to_string(),
            max_concurrent_inbound_tasks: 100,
            outbound_buffer_size: 100,
            dht: DhtConfig {
                database_url: DbConnectionUrl::File(self.config.data_dir.join("dht.db")),
                auto_join: true,
                allow_test_addresses: self.config.allow_test_addresses,
                flood_ban_max_msg_count: self.config.flood_ban_max_msg_count,
                saf_config: SafConfig {
                    msg_validity: self.config.saf_expiry_duration,
                    ..Default::default()
                },
                ..Default::default()
            },
            allow_test_addresses: self.config.allow_test_addresses,
            listener_liveness_allowlist_cidrs: self.config.listener_liveness_allowlist_cidrs.clone(),
            listener_liveness_max_sessions: self.config.listnener_liveness_max_sessions,
            user_agent: format!("tari/dannode/{}", env!("CARGO_PKG_VERSION")),
            // Also add sync peers to the peer seed list. Duplicates are acceptable.
            peer_seeds: self
                .config
                .peer_seeds
                .iter()
                .cloned()
                .chain(self.config.force_sync_peers.clone())
                .collect(),
            dns_seeds: self.config.dns_seeds.clone(),
            dns_seeds_name_server: self.config.dns_seeds_name_server,
            dns_seeds_use_dnssec: self.config.dns_seeds_use_dnssec,
            auxilary_tcp_listener_address: self.config.auxilary_tcp_listener_address.clone(),
        }
    }

    /// Creates a transport type from the given configuration
    ///
    /// ## Paramters
    /// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
    ///
    /// ##Returns
    /// TransportType based on the configuration
    fn create_transport_type(&self) -> TransportType {
        debug!(
            target: LOG_TARGET,
            "Transport is set to '{:?}'", self.config.comms_transport
        );
        match self.config.comms_transport.clone() {
            CommsTransport::Tcp {
                listener_address,
                tor_socks_address,
                tor_socks_auth,
            } => TransportType::Tcp {
                listener_address,
                tor_socks_config: tor_socks_address.map(|proxy_address| SocksConfig {
                    proxy_address,
                    authentication: tor_socks_auth.map(convert_socks_authentication).unwrap_or_default(),
                    proxy_bypass_predicate: Arc::new(FalsePredicate::new()),
                }),
            },
            CommsTransport::TorHiddenService {
                control_server_address,
                socks_address_override,
                forward_address,
                auth,
                onion_port,
                tor_proxy_bypass_addresses,
                tor_proxy_bypass_for_outbound_tcp,
            } => {
                let identity = Some(&self.config.base_node_tor_identity_file)
                    .filter(|p| p.exists())
                    .and_then(|p| {
                        // If this fails, we can just use another address
                        load_from_json::<_, TorIdentity>(p).ok()
                    });
                info!(
                    target: LOG_TARGET,
                    "Tor identity at path '{}' {:?}",
                    self.config.base_node_tor_identity_file.to_string_lossy(),
                    identity
                        .as_ref()
                        .map(|ident| format!("loaded for address '{}.onion'", ident.service_id))
                        .or_else(|| Some("not found".to_string()))
                        .unwrap()
                );

                let forward_addr = multiaddr_to_socketaddr(&forward_address).expect("Invalid tor forward address");
                TransportType::Tor(TorConfig {
                    control_server_addr: control_server_address,
                    control_server_auth: {
                        match auth {
                            TorControlAuthentication::None => tor::Authentication::None,
                            TorControlAuthentication::Password(password) => {
                                tor::Authentication::HashedPassword(password)
                            },
                        }
                    },
                    identity: identity.map(Box::new),
                    port_mapping: (onion_port, forward_addr).into(),
                    socks_address_override,
                    socks_auth: socks::Authentication::None,
                    tor_proxy_bypass_addresses,
                    tor_proxy_bypass_for_outbound_tcp,
                })
            },
            CommsTransport::Socks5 {
                proxy_address,
                listener_address,
                auth,
            } => TransportType::Socks {
                socks_config: SocksConfig {
                    proxy_address,
                    authentication: convert_socks_authentication(auth),
                    proxy_bypass_predicate: Arc::new(FalsePredicate::new()),
                },
                listener_address,
            },
        }
    }
}

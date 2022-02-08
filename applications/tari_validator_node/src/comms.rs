//  Copyright 2021. The Tari Project
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

use std::sync::Arc;

use log::*;
use tari_app_utilities::{
    identity_management,
    identity_management::load_from_json,
    utilities::convert_socks_authentication,
};
use tari_common::{
    exit_codes::{ExitCode, ExitError},
    CommsTransport,
    GlobalConfig,
    TorControlAuthentication,
};
use tari_comms::{
    protocol::rpc::RpcServer,
    socks,
    tor,
    tor::TorIdentity,
    transports::{predicate::FalsePredicate, SocksConfig},
    utils::multiaddr::multiaddr_to_socketaddr,
    NodeIdentity,
    UnspawnedCommsNode,
};
use tari_comms_dht::{store_forward::SafConfig, DbConnectionUrl, Dht, DhtConfig};
use tari_dan_core::services::{ConcreteAssetProcessor, MempoolServiceHandle};
use tari_dan_storage_sqlite::SqliteDbFactory;
use tari_p2p::{
    comms_connector::{pubsub_connector, SubscriptionFactory},
    initialization::{spawn_comms_using_transport, P2pConfig, P2pInitializer},
    transport::{TorConfig, TransportType},
};
use tari_service_framework::{ServiceHandles, StackBuilder};
use tari_shutdown::ShutdownSignal;

use crate::p2p::create_validator_node_rpc_service;

const LOG_TARGET: &str = "tari::validator_node::comms";

pub async fn build_service_and_comms_stack(
    config: &GlobalConfig,
    shutdown: ShutdownSignal,
    node_identity: Arc<NodeIdentity>,
    mempool: MempoolServiceHandle,
    db_factory: SqliteDbFactory,
    asset_processor: ConcreteAssetProcessor,
) -> Result<(ServiceHandles, SubscriptionFactory), ExitError> {
    // this code is duplicated from the base node
    let comms_config = create_comms_config(config, node_identity.clone());

    let (publisher, peer_message_subscriptions) = pubsub_connector(100, config.buffer_rate_limit_base_node);

    let mut handles = StackBuilder::new(shutdown.clone())
        .add_initializer(P2pInitializer::new(comms_config, publisher))
        .build()
        .await
        .map_err(|err| ExitError::new(ExitCode::ConfigError, err))?;

    let comms = handles
        .take_handle::<UnspawnedCommsNode>()
        .expect("P2pInitializer was not added to the stack or did not add UnspawnedCommsNode");

    let comms = setup_p2p_rpc(config, comms, &handles, mempool, db_factory, asset_processor);

    let comms = spawn_comms_using_transport(comms, create_transport_type(config))
        .await
        .map_err(|e| ExitError::new(ExitCode::ConfigError, format!("Could not spawn using transport:{}", e)))?;

    // Save final node identity after comms has initialized. This is required because the public_address can be
    // changed by comms during initialization when using tor.
    identity_management::save_as_json(&config.base_node_identity_file, &*comms.node_identity())
        .map_err(|e| ExitError::new(ExitCode::ConfigError, format!("Failed to save node identity: {}", e)))?;
    if let Some(hs) = comms.hidden_service() {
        identity_management::save_as_json(&config.base_node_tor_identity_file, hs.tor_identity())
            .map_err(|e| ExitError::new(ExitCode::ConfigError, format!("Failed to save tor identity: {}", e)))?;
    }

    handles.register(comms);
    Ok((handles, peer_message_subscriptions))
}

fn setup_p2p_rpc(
    config: &GlobalConfig,
    comms: UnspawnedCommsNode,
    handles: &ServiceHandles,
    mempool: MempoolServiceHandle,
    db_factory: SqliteDbFactory,
    asset_processor: ConcreteAssetProcessor,
) -> UnspawnedCommsNode {
    let dht = handles.expect_handle::<Dht>();
    let builder = RpcServer::builder();
    let builder = match config.comms_rpc_max_simultaneous_sessions {
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
        .add_service(create_validator_node_rpc_service(mempool, db_factory, asset_processor));

    comms.add_protocol_extension(rpc_server)
}

fn create_comms_config(config: &GlobalConfig, node_identity: Arc<NodeIdentity>) -> P2pConfig {
    P2pConfig {
        network: config.network,
        node_identity,
        transport_type: create_transport_type(config),
        datastore_path: config.comms_peer_db_path.clone(),
        peer_database_name: "peers".to_string(),
        max_concurrent_inbound_tasks: 50,
        max_concurrent_outbound_tasks: 100,
        outbound_buffer_size: 100,
        dht: DhtConfig {
            database_url: DbConnectionUrl::File(config.data_dir.join("dht.db")),
            auto_join: true,
            allow_test_addresses: config.comms_allow_test_addresses,
            flood_ban_max_msg_count: config.flood_ban_max_msg_count,
            saf_config: SafConfig {
                msg_validity: config.saf_expiry_duration,
                ..Default::default()
            },
            ..Default::default()
        },
        allow_test_addresses: config.comms_allow_test_addresses,
        listener_liveness_allowlist_cidrs: config.comms_listener_liveness_allowlist_cidrs.clone(),
        listener_liveness_max_sessions: config.comms_listener_liveness_max_sessions,
        user_agent: format!("tari/dannode/{}", env!("CARGO_PKG_VERSION")),
        // Also add sync peers to the peer seed list. Duplicates are acceptable.
        peer_seeds: config
            .peer_seeds
            .iter()
            .cloned()
            .chain(config.force_sync_peers.clone())
            .collect(),
        dns_seeds: config.dns_seeds.clone(),
        dns_seeds_name_server: config.dns_seeds_name_server.clone(),
        dns_seeds_use_dnssec: config.dns_seeds_use_dnssec,
        auxilary_tcp_listener_address: config.auxilary_tcp_listener_address.clone(),
    }
}

/// Creates a transport type from the given configuration
///
/// ## Paramters
/// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
///
/// ##Returns
/// TransportType based on the configuration
fn create_transport_type(config: &GlobalConfig) -> TransportType {
    debug!(target: LOG_TARGET, "Transport is set to '{:?}'", config.comms_transport);
    match config.comms_transport.clone() {
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
            let identity = Some(&config.base_node_tor_identity_file)
                .filter(|p| p.exists())
                .and_then(|p| {
                    // If this fails, we can just use another address
                    load_from_json::<_, TorIdentity>(p).ok()
                });
            debug!(
                target: LOG_TARGET,
                "Tor identity at path '{}' {}",
                config.base_node_tor_identity_file.to_string_lossy(),
                identity
                    .as_ref()
                    .map(|ident| format!("loaded for address '{}.onion'", ident.service_id))
                    .unwrap_or_else(|| "not found".to_string())
            );

            let forward_addr = multiaddr_to_socketaddr(&forward_address).expect("Invalid tor forward address");
            TransportType::Tor(TorConfig {
                control_server_addr: control_server_address,
                control_server_auth: {
                    match auth {
                        TorControlAuthentication::None => tor::Authentication::None,
                        TorControlAuthentication::Password(password) => tor::Authentication::HashedPassword(password),
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

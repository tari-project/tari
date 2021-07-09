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

use crate::digital_assets_error::DigitalAssetError;
use tari_app_utilities::identity_management::setup_node_identity;
use tari_common::{GlobalConfig, ConfigBootstrap, CommsTransport, TorControlAuthentication};
use crate::ExitCodes;
use std::sync::Arc;
use tari_comms::peer_manager::PeerFeatures;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::runtime::Handle;
use tari_service_framework::StackBuilder;
use tari_p2p::initialization::{P2pInitializer, CommsConfig};
use tari_p2p::comms_connector::pubsub_connector;
use tari_comms_dht::{DhtConfig, DbConnectionUrl};
use tari_comms::{NodeIdentity, tor, socks};
use tari_p2p::transport::{TransportType, TorConfig};
use tari_comms::transports::SocksConfig;
use tari_app_utilities::{utilities, identity_management};
use tari_comms::tor::TorIdentity;
use tari_comms::utils::multiaddr::multiaddr_to_socketaddr;
use log::*;
use std::fs;


const LOG_TARGET: &str = "tari::dan::dan_node";

pub struct DanNode {
    config: GlobalConfig
}

impl DanNode {
    pub fn new(config: GlobalConfig) -> Self {
        Self {config}
    }

    pub async fn start(&self, create_id: bool, shutdown: ShutdownSignal) -> Result<(), ExitCodes> {
        fs::create_dir_all(&self.config.peer_db_path).map_err(|err| ExitCodes::ConfigError(err.to_string()))?;
        let node_identity = setup_node_identity(
            &self.config.base_node_identity_file,
            &self.config.public_address,
            create_id,
            PeerFeatures::NONE
        )?;

        let comms_config = self.create_comms_config(node_identity);

        let (publisher, peer_message_subscriptions) =
            pubsub_connector(Handle::current(), 100, self.config.buffer_rate_limit_base_node);
        let mut handles = StackBuilder::new(shutdown).add_initializer(P2pInitializer::new(comms_config, publisher)).build().await.map_err(|err| ExitCodes::ConfigError(err.to_string()))?;

        todo!("Finish this impl")
    }

    fn create_comms_config(&self, node_identity: Arc<NodeIdentity>) -> CommsConfig {
        CommsConfig {
            network: self.config.network,
            node_identity: node_identity.clone(),
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
                saf_msg_validity: self.config.saf_expiry_duration,
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
        }
    }



    // COPIed from base node
    /// Creates a transport type from the given configuration
    ///
    /// ## Paramters
    /// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
    ///
    /// ##Returns
    /// TransportType based on the configuration
    fn create_transport_type(&self) -> TransportType {

        debug!(target: LOG_TARGET, "Transport is set to '{:?}'", self.config.comms_transport);
        match self.config.comms_transport.clone() {
            CommsTransport::Tcp {
                listener_address,
                tor_socks_address,
                tor_socks_auth,
            } => TransportType::Tcp {
                listener_address,
                tor_socks_config: tor_socks_address.map(|proxy_address| SocksConfig {
                    proxy_address,
                    authentication: tor_socks_auth
                        .map(utilities::convert_socks_authentication)
                        .unwrap_or_default(),
                }),
            },
            CommsTransport::TorHiddenService {
                control_server_address,
                socks_address_override,
                forward_address,
                auth,
                onion_port,
            } => {
                let identity = Some(&self.config.base_node_tor_identity_file)
                    .filter(|p| p.exists())
                    .and_then(|p| {
                        // If this fails, we can just use another address
                        identity_management::load_from_json::<_, TorIdentity>(p).ok()
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
                    // TODO: make configurable
                    socks_address_override,
                    socks_auth: socks::Authentication::None,
                })
            },
            CommsTransport::Socks5 {
                proxy_address,
                listener_address,
                auth,
            } => TransportType::Socks {
                socks_config: SocksConfig {
                    proxy_address,
                    authentication: utilities::convert_socks_authentication(auth),
                },
                listener_address,
            },
        }
    }
}

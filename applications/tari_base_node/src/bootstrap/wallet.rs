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

use anyhow::anyhow;
use log::*;
use std::{cmp, fs, net::SocketAddr, sync::Arc, time::Duration};
use tari_app_utilities::{identity_management, utilities};
use tari_common::{CommsTransport, GlobalConfig, TorControlAuthentication};
use tari_comms::{
    multiaddr::{Multiaddr, Protocol},
    peer_manager::Peer,
    socks,
    tor,
    tor::TorIdentity,
    transports::SocksConfig,
    NodeIdentity,
    UnspawnedCommsNode,
};
use tari_comms_dht::{DbConnectionUrl, DhtConfig};
use tari_core::transactions::types::CryptoFactories;
use tari_p2p::{
    comms_connector::pubsub_connector,
    initialization,
    initialization::{CommsConfig, P2pInitializer},
    services::liveness::{LivenessConfig, LivenessInitializer},
    transport::{TorConfig, TransportType},
};
use tari_service_framework::{ServiceHandles, StackBuilder};
use tari_shutdown::ShutdownSignal;
use tari_wallet::{
    output_manager_service::{
        config::OutputManagerServiceConfig,
        storage::sqlite_db::OutputManagerSqliteDatabase,
        OutputManagerServiceInitializer,
    },
    storage::{sqlite_utilities, sqlite_utilities::WalletDbConnection},
    transaction_service::{
        config::TransactionServiceConfig,
        storage::sqlite_db::TransactionServiceSqliteDatabase,
        TransactionServiceInitializer,
    },
};
use tokio::{runtime, task};

const LOG_TARGET: &str = "c::bn::initialization";

/// The minimum buffer size for the base node wallet pubsub_connector channel
const BASE_NODE_WALLET_BUFFER_MIN_SIZE: usize = 300;

pub struct WalletBootstrapper {
    pub config: GlobalConfig,
    pub node_identity: Arc<NodeIdentity>,
    pub interrupt_signal: ShutdownSignal,
    pub factories: CryptoFactories,
    pub base_node_peer: Peer,
}

impl WalletBootstrapper {
    async fn connect_wallet_db(&self) -> Result<WalletDbConnection, anyhow::Error> {
        fs::create_dir_all(
            self.config
                .wallet_db_file
                .parent()
                .ok_or_else(|| anyhow!("wallet_db_file cannot be set to a root directory"))?,
        )?;
        let wallet_conn = sqlite_utilities::run_migration_and_create_sqlite_connection(&self.config.wallet_db_file)?;
        Ok(wallet_conn)
    }

    async fn setup_transaction_db(
        &self,
        conn: WalletDbConnection,
    ) -> Result<TransactionServiceSqliteDatabase, anyhow::Error>
    {
        let transaction_db = TransactionServiceSqliteDatabase::new(conn, None);
        task::spawn_blocking({
            let transaction_db = transaction_db.clone();
            let node_identity = self.node_identity.clone();

            move || transaction_db.migrate(node_identity.public_key().clone())
        })
        .await?;
        Ok(transaction_db)
    }

    pub async fn bootstrap(mut self) -> Result<ServiceHandles, anyhow::Error> {
        self.change_config_for_wallet();

        let config = &self.config;
        let wallet_db_conn = self.connect_wallet_db().await?;
        let transaction_db = self.setup_transaction_db(wallet_db_conn.clone()).await?;

        let buf_size = cmp::max(BASE_NODE_WALLET_BUFFER_MIN_SIZE, config.buffer_size_base_node_wallet);
        let (publisher, peer_message_subscriptions) = pubsub_connector(
            runtime::Handle::current(),
            buf_size,
            config.buffer_rate_limit_base_node_wallet,
        );
        let peer_message_subscriptions = Arc::new(peer_message_subscriptions);
        fs::create_dir_all(&config.wallet_peer_db_path)?;
        let base_node_node_id = self.base_node_peer.node_id.clone();

        let comms_config = self.create_comms_config();
        let transport_type = comms_config.transport_type.clone();

        let mut seed_peers = utilities::parse_peer_seeds(&config.peer_seeds);
        seed_peers.push(self.base_node_peer);

        let mut handles = StackBuilder::new( self.interrupt_signal)
            .add_initializer(P2pInitializer::new(comms_config, publisher, seed_peers))
            .add_initializer(LivenessInitializer::new(
                LivenessConfig{
                    auto_ping_interval: Some(Duration::from_secs(60)),
                    ..Default::default()
                },
                peer_message_subscriptions.clone(),
            ))
            // Wallet services
            .add_initializer(OutputManagerServiceInitializer::new(
                OutputManagerServiceConfig{
                    base_node_query_timeout: config.base_node_query_timeout,
                    prevent_fee_gt_amount: config.prevent_fee_gt_amount,
                    ..Default::default()
                },
               peer_message_subscriptions.clone(),
                OutputManagerSqliteDatabase::new(wallet_db_conn.clone(),None),
                self.factories.clone(),
                config.network.into()
            ))
            .add_initializer(TransactionServiceInitializer::new(
                TransactionServiceConfig {
                    broadcast_monitoring_timeout: config.transaction_broadcast_monitoring_timeout,
                    chain_monitoring_timeout: config.transaction_chain_monitoring_timeout,
                    direct_send_timeout: config.transaction_direct_send_timeout,
                    broadcast_send_timeout: config.transaction_broadcast_send_timeout,
                    ..Default::default()
                },
                peer_message_subscriptions,
                transaction_db,
                self.node_identity.clone(),
                self.factories,
                config.network.into()
            ))
            .build()
            .await?;

        let comms = handles
            .take_handle::<UnspawnedCommsNode>()
            .expect("P2pInitializer was not added to the stack or did not add UnspawnedCommsNode");

        // Ensure connection to base node
        comms.connectivity().add_managed_peers(vec![base_node_node_id]).await?;
        let comms = initialization::spawn_comms_using_transport(comms, transport_type).await?;

        // Save final node identity after comms has initialized. This is required because the public_address can be
        // changed by comms during initialization when using tor.
        identity_management::save_as_json(&config.wallet_identity_file, &*comms.node_identity())
            .map_err(|e| anyhow!("Failed to save node identity: {:?}", e))?;
        if let Some(hs) = comms.hidden_service() {
            identity_management::save_as_json(&config.wallet_tor_identity_file, hs.tor_identity())
                .map_err(|e| anyhow!("Failed to save tor identity: {:?}", e))?;
        }
        handles.register(comms);

        Ok(handles)
    }

    fn create_comms_config(&self) -> CommsConfig {
        CommsConfig {
            node_identity: self.node_identity.clone(),
            user_agent: format!("tari/wallet/{}", env!("CARGO_PKG_VERSION")),
            transport_type: self.create_transport_type(),
            datastore_path: self.config.wallet_peer_db_path.clone(),
            peer_database_name: "peers".to_string(),
            max_concurrent_inbound_tasks: 100,
            outbound_buffer_size: 100,
            dht: DhtConfig {
                database_url: DbConnectionUrl::File(self.config.data_dir.join("dht-wallet.db")),
                auto_join: true,
                ..Default::default()
            },
            allow_test_addresses: false,
            listener_liveness_allowlist_cidrs: Vec::new(),
            listener_liveness_max_sessions: 0,
        }
    }

    fn change_config_for_wallet(&mut self) {
        // TODO: These are temporary fixes until the wallet can be split from the base node.
        fn add_to_port(addr: &Multiaddr, n: u16) -> Multiaddr {
            addr.iter()
                .map(|p| match p {
                    Protocol::Tcp(port) => Protocol::Tcp(port + n),
                    p => p,
                })
                .collect()
        }

        match self.config.comms_transport {
            CommsTransport::Tcp {
                ref mut listener_address,
                ..
            } |
            CommsTransport::Socks5 {
                ref mut listener_address,
                ..
            } => {
                *listener_address = add_to_port(listener_address, 1);
                let public_addr = self.node_identity.public_address();
                self.node_identity.set_public_address(add_to_port(&public_addr, 1));
            },
            _ => {},
        }
    }

    /// Creates a transport type for the base node's wallet using the provided configuration
    ///
    /// ## Parameters
    /// `config` - The reference to the configuration in which to set up the comms stack, see [GlobalConfig]
    ///
    /// ##Returns
    /// TransportType based on the configuration
    fn create_transport_type(&self) -> TransportType {
        let config = &self.config;
        debug!(
            target: LOG_TARGET,
            "Base node wallet transport is set to '{:?}'", config.comms_transport
        );

        match config.comms_transport.clone() {
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
                auth,
                ..
            } => {
                // The wallet should always use an OS-assigned forwarding port and an onion port number of 18101
                // to ensure that different wallet implementations cannot be differentiated by their port.
                let port_mapping = (18101u16, "127.0.0.1:0".parse::<SocketAddr>().unwrap()).into();

                let identity = Some(&config.wallet_tor_identity_file)
                    .filter(|p| p.exists())
                    .and_then(|p| {
                        // If this fails, we can just use another address
                        identity_management::load_from_json::<_, TorIdentity>(p).ok()
                    });
                info!(
                    target: LOG_TARGET,
                    "Base node wallet tor identity at path '{}' {:?}",
                    self.config.wallet_tor_identity_file.to_string_lossy(),
                    identity
                        .as_ref()
                        .map(|ident| format!("loaded for address '{}.onion'", ident.service_id))
                        .or_else(|| Some("not found".to_string()))
                        .unwrap()
                );

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
                    port_mapping,
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

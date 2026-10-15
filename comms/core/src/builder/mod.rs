//  Copyright 2019 The Tari Project
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

mod comms_node;
pub use comms_node::{CommsNode, UnspawnedCommsNode};

mod shutdown;
pub use shutdown::CommsShutdown;

mod error;
pub use error::CommsBuilderError;

mod consts;

#[cfg(test)]
mod tests;

use std::{sync::Arc, time::Duration};

use tari_shutdown::ShutdownSignal;
use tokio::sync::{broadcast, mpsc};

use crate::{
    backoff::{Backoff, BoxedBackoff, ConstantBackoff},
    connection_manager::{ConnectionManagerConfig, ConnectionManagerRequester},
    connectivity::{ConnectivityConfig, ConnectivityRequester},
    multiaddr::Multiaddr,
    net_address::MultiaddrRange,
    peer_manager::{NodeIdentity, PeerManager, PeerManagerConfig},
    peer_validator::PeerValidatorConfig,
    protocol::{NodeNetworkInfo, ProtocolExtensions},
    tor,
    types::{CommsDatabase, TransportProtocol},
};

/// # CommsBuilder
///
/// [CommsBuilder] is used to customize and spawn Tari comms core.
///
/// The following example will get a node customized for your own network up and running.
///
/// ```rust
/// # use std::{sync::Arc, time::Duration};
/// # use tari_shutdown::Shutdown;
/// # use tari_comms::{
/// #     {CommsBuilder, NodeIdentity},
/// #    peer_manager::{PeerStorage, PeerFeatures},
/// #    transports::TcpTransport,
/// # };
/// # #[tokio::main]
/// # async fn main() {
/// use tari_common_sqlite::connection::DbConnection;
/// use tari_comms::peer_manager::database::{PeerDatabaseSql, MIGRATIONS};
///
/// let node_identity = Arc::new(NodeIdentity::random(
///     &mut rand::rng(),
///     "/dns4/basenodezforhire.com/tcp/18000".parse().unwrap(),
///     PeerFeatures::COMMUNICATION_NODE,
/// ));
/// node_identity.sign();
/// let shutdown = Shutdown::new();
/// let db_connection = DbConnection::connect_temp_file_and_migrate(MIGRATIONS).unwrap();
/// // The peer database is seeded with this node's own identity.
/// let peer_database = PeerDatabaseSql::new(db_connection, &node_identity.to_peer()).unwrap();
///
/// let unspawned_node = CommsBuilder::new()
///   // .with_listener_address("/ip4/0.0.0.0/tcp/18000".parse().unwrap())
///   .with_node_identity(node_identity)
///   .with_peer_storage(peer_database)
///   .with_shutdown_signal(shutdown.to_signal())
///   .build()
///   .unwrap();
/// // This is your chance to add customizations that may require comms components for e.g. PeerManager.
/// // let my_peer = Peer::new(...);
/// // unspawned_node.peer_manager().add_or_update_peer(my_peer.clone());
/// // Add custom extensions implementing `ProtocolExtension`
/// // unspawned_node = unspawned_node.add_protocol_extension(MyCustomProtocol::new(unspawned_node.peer_manager()));
///
/// let transport = TcpTransport::new();
/// let node = unspawned_node.spawn_with_transport(transport).await.unwrap();
/// // Node is alive for 2 seconds
/// tokio::time::sleep(Duration::from_secs(2)).await;
/// shutdown.trigger();
/// node.wait_until_shutdown().await;
/// // let peer_conn = node.connectivity().dial_peer(my_peer.node_id).await.unwrap();
/// # }
/// ```
///
/// [CommsBuilder]: crate::CommsBuilder
pub struct CommsBuilder {
    peer_storage: Option<CommsDatabase>,
    node_identity: Option<Arc<NodeIdentity>>,
    dial_backoff: BoxedBackoff,
    hidden_service_ctl: Option<tor::HiddenServiceController>,
    connection_manager_config: ConnectionManagerConfig,
    connectivity_config: ConnectivityConfig,
    shutdown_signal: Option<ShutdownSignal>,
    maintain_n_closest_connections_only: Option<usize>,
    transport_protocols: Vec<TransportProtocol>,
    peer_manager_config: PeerManagerConfig,
}

impl Default for CommsBuilder {
    fn default() -> Self {
        Self {
            peer_storage: None,
            node_identity: None,
            dial_backoff: Box::new(ConstantBackoff::new(Duration::from_millis(500))),
            hidden_service_ctl: None,
            connection_manager_config: ConnectionManagerConfig::default(),
            connectivity_config: ConnectivityConfig::default(),
            shutdown_signal: None,
            maintain_n_closest_connections_only: None,
            transport_protocols: TransportProtocol::get_all(),
            peer_manager_config: PeerManagerConfig::default(),
        }
    }
}

impl CommsBuilder {
    /// Create a new `CommsBuilder` instance
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the [NodeIdentity] for this comms instance. This is required.
    ///
    /// [OutboundMessagePool]: ../../outbound_message_service/index.html#outbound-message-pool
    pub fn with_node_identity(mut self, node_identity: Arc<NodeIdentity>) -> Self {
        self.node_identity = Some(node_identity);
        self
    }

    pub fn with_connection_pool_refresh_interval(mut self, interval: Duration) -> Self {
        self.connectivity_config.connection_pool_refresh_interval = interval;
        self
    }

    pub fn with_max_seed_peer_age(mut self, max_age: Duration) -> Self {
        self.connectivity_config.max_seed_peer_age = max_age;
        self
    }

    /// Return the node identity for this comms instance.
    pub fn node_identity(&self) -> Option<Arc<NodeIdentity>> {
        self.node_identity.clone()
    }

    /// Set the shutdown signal for this comms instance
    pub fn with_shutdown_signal(mut self, shutdown_signal: ShutdownSignal) -> Self {
        self.shutdown_signal = Some(shutdown_signal);
        self
    }

    /// Set the user agent string for this comms node. This string is sent once when establishing a connection.
    pub fn with_user_agent<T: ToString>(mut self, user_agent: &T) -> Self {
        self.connection_manager_config.network_info.user_agent = user_agent.to_string();
        self
    }

    /// Set a network byte as per [RFC-173 Versioning](https://rfc.tari.com/RFC-0173_Versioning.html)
    pub fn with_network_byte(mut self, network_byte: u8) -> Self {
        self.connection_manager_config.network_info.network_wire_byte = network_byte;
        self
    }

    /// Set a network info (versions etc) as per [RFC-173 Versioning](https://rfc.tari.com/RFC-0173_Versioning.html)
    pub fn with_node_info(mut self, node_info: NodeNetworkInfo) -> Self {
        self.connection_manager_config.network_info = node_info;
        self
    }

    /// Set a network major and minor version as per [RFC-173 Versioning](https://rfc.tari.com/RFC-0173_Versioning.html)
    pub fn with_node_version(mut self, major_version: u8, minor_version: u8) -> Self {
        self.connection_manager_config.network_info.major_version = major_version;
        self.connection_manager_config.network_info.minor_version = minor_version;
        self
    }

    /// Allow test addresses (memory addresses, local loopback etc). This should only be activated for tests.
    pub fn allow_test_addresses(mut self) -> Self {
        #[cfg(not(debug_assertions))]
        log::warn!(
            target: "comms::builder",
            "Test addresses are enabled! This is invalid and potentially insecure when running a production node."
        );
        self.connection_manager_config
            .peer_validation_config
            .allow_test_addresses = true;
        self
    }

    /// Sets the PeerValidatorConfig - this will override previous calls to allow_test_addresses() with the value in
    /// peer_validator_config.allow_test_addresses
    pub fn with_peer_validator_config(mut self, config: PeerValidatorConfig) -> Self {
        #[cfg(not(debug_assertions))]
        if config.allow_test_addresses {
            log::warn!(
                target: "comms::builder",
                "Test addresses are enabled! This is invalid and potentially insecure when running a production node."
            );
        }
        self.connection_manager_config.peer_validation_config = config;
        self
    }

    /// Returns the PeerValidatorConfig set in this builder
    pub fn peer_validator_config(&self) -> &PeerValidatorConfig {
        &self.connection_manager_config.peer_validation_config
    }

    /// Sets the address that the transport will listen on. The address must be compatible with the transport.
    pub fn with_listener_address(mut self, listener_address: Multiaddr) -> Self {
        self.connection_manager_config.listener_address = listener_address;
        self
    }

    /// Sets an auxiliary TCP listener address that can accept peer connections. This is optional.
    pub fn with_auxiliary_tcp_listener_address(mut self, listener_address: Multiaddr) -> Self {
        self.connection_manager_config.auxiliary_tcp_listener_address = Some(listener_address);
        self
    }

    /// Sets the maximum allowed liveness sessions. Liveness is typically used by tools like docker or kubernetes to
    /// detect that the node is live. Defaults to 0 (disabled)
    pub fn with_listener_liveness_max_sessions(mut self, max_sessions: usize) -> Self {
        self.connection_manager_config.liveness_max_sessions = max_sessions;
        self
    }

    pub fn with_excluded_dial_addresses(mut self, excluded_addresses: Vec<MultiaddrRange>) -> Self {
        self.connection_manager_config.excluded_dial_addresses = excluded_addresses;
        self
    }

    /// Restrict liveness sessions to certain address ranges (CIDR format).
    pub fn with_listener_liveness_allowlist_cidrs(mut self, cidrs: Vec<cidr::AnyIpCidr>) -> Self {
        self.connection_manager_config.liveness_cidr_allowlist = cidrs;
        self
    }

    /// The maximum number of connection tasks that will be spawned at the same time. Once this limit is reached, peers
    /// attempting to connect will have to wait for another connection attempt to complete.
    pub fn with_max_simultaneous_inbound_connects(mut self, max_simultaneous_inbound_connects: usize) -> Self {
        self.connection_manager_config.max_simultaneous_inbound_connects = max_simultaneous_inbound_connects;
        self
    }

    /// The longest the unauthenticated stages of an inbound connection (wire byte, noise and identity exchange) may
    /// take in total. Must be non-zero; `build` rejects zero.
    pub fn with_listener_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.connection_manager_config.inbound_handshake_timeout = timeout;
        self
    }

    /// The maximum number of inbound handshakes in flight at once from a single source (IPv4 address or IPv6 /64).
    /// Loopback, `listener_per_ip_exempt_cidrs` and non-IP sources are exempt. Must be non-zero.
    pub fn with_listener_max_handshakes_per_ip(mut self, max_handshakes_per_ip: usize) -> Self {
        self.connection_manager_config.max_inbound_handshakes_per_ip = max_handshakes_per_ip;
        self
    }

    /// The maximum number of inbound connections, established and in-handshake combined. Inbound sockets over this
    /// limit are closed before any handshake. Must be non-zero.
    pub fn with_max_inbound_connections(mut self, max_inbound_connections: usize) -> Self {
        self.connection_manager_config.max_inbound_connections = max_inbound_connections;
        self
    }

    /// The maximum number of inbound connections, established and in-handshake combined, from a single source (IPv4
    /// address or IPv6 /64). Same exemptions as the per-source handshake limit. Must be non-zero.
    pub fn with_max_inbound_connections_per_ip(mut self, max_inbound_connections_per_ip: usize) -> Self {
        self.connection_manager_config.max_inbound_connections_per_ip = max_inbound_connections_per_ip;
        self
    }

    /// Sources exempt from the per-source inbound limits, in addition to loopback. Add a remote Tor forwarder or proxy
    /// here, since every connection it forwards appears to come from its address.
    pub fn with_listener_per_ip_exempt_cidrs(mut self, cidrs: Vec<cidr::AnyIpCidr>) -> Self {
        self.connection_manager_config.listener_per_ip_exempt_cidrs = cidrs;
        self
    }

    /// The number of dial attempts to make before giving up.
    pub fn with_max_dial_attempts(mut self, max_dial_attempts: usize) -> Self {
        self.connection_manager_config.max_dial_attempts = max_dial_attempts;
        self
    }

    /// Sets the minimum required connectivity as a percentage of peers added to the connectivity manager peer set.
    pub fn with_min_connectivity(mut self, min_connectivity: usize) -> Self {
        self.connectivity_config.min_connectivity = min_connectivity;
        self
    }

    /// Call to disable connection reaping. Usually you would want to have this enabled, however there are some test
    /// cases where disabling this is desirable.
    pub fn disable_connection_reaping(mut self) -> Self {
        self.connectivity_config.is_connection_reaping_enabled = false;
        self
    }

    /// Set the peer storage database to use.
    pub fn with_peer_storage(mut self, peer_storage: CommsDatabase) -> Self {
        self.peer_storage = Some(peer_storage);
        self
    }

    /// Set the backoff to use when a dial to a remote peer fails. This is optional. If omitted the default
    /// [ConstantBackoff](crate::backoff::ConstantBackoff) of 500ms is used.
    pub fn with_dial_backoff<T>(mut self, backoff: T) -> Self
    where T: Backoff + Send + Sync + 'static {
        self.dial_backoff = Box::new(backoff);
        self
    }

    /// Enable and set interval for self-liveness checks, or None to disable it (default)
    pub fn set_self_liveness_check(mut self, check_interval: Option<Duration>) -> Self {
        self.connection_manager_config.self_liveness_self_check_interval = check_interval;
        self
    }

    /// The closest number of peer connections to maintain; connections above the threshold will be removed
    pub fn with_minimize_connections(mut self, connections: Option<usize>) -> Self {
        self.maintain_n_closest_connections_only = connections;
        self.connectivity_config.maintain_n_closest_connections_only = connections;
        self
    }

    /// The number of outbound connections above which idle outbound connections are reaped. See
    /// [`ConnectivityConfig::reaper_min_connection_threshold`].
    pub fn with_reaper_min_connection_thresholds(mut self, connections: usize) -> Self {
        self.connectivity_config.reaper_min_connection_threshold = connections;
        self
    }

    /// Once more than this many inbound connections are open, idle inbound connections are closed, oldest first,
    /// until the count is back at this limit. See [`ConnectivityConfig::reaper_inbound_soft_limit`].
    pub fn with_reaper_inbound_soft_limit(mut self, limit: usize) -> Self {
        self.connectivity_config.reaper_inbound_soft_limit = limit;
        self
    }

    /// Set the peer manager configuration (peer database row cap and dial candidate spreading)
    pub fn with_peer_manager_config(mut self, config: PeerManagerConfig) -> Self {
        self.peer_manager_config = config;
        self
    }

    /// Enable or disable proactive (recovery) dialing. Disabling it leaves the DHT peer pool as the only thing
    /// dialing new peers, which is fine on a node that stays connected and removes its only unaided way back
    /// from total isolation.
    pub fn with_proactive_dialing_enabled(mut self, enabled: bool) -> Self {
        self.connectivity_config.proactive_dialing_enabled = enabled;
        self
    }

    /// The connection count below which the proactive dialer is allowed to run. This is a floor, not a target -
    /// see [`ConnectivityConfig::proactive_dialing_floor`]. It must stay strictly below the DHT peer pool size;
    /// `P2pInitializer` clamps it to enforce that.
    pub fn with_proactive_dialing_floor(mut self, floor: usize) -> Self {
        self.connectivity_config.proactive_dialing_floor = floor;
        self
    }

    /// Set the transport protocols to use for communication
    /// if not provided defaults to IP4, IP6, TOR and memory address
    pub fn with_transport_protocols(mut self, protocols: Vec<TransportProtocol>) -> Self {
        self.transport_protocols = protocols;
        self
    }

    fn make_peer_manager(&mut self) -> Result<Arc<PeerManager>, CommsBuilderError> {
        match self.peer_storage.take() {
            Some(storage) => {
                let peer_manager =
                    PeerManager::new_with_config(storage, self.transport_protocols.clone(), self.peer_manager_config)
                        .map_err(CommsBuilderError::PeerManagerError)?;
                Ok(Arc::new(peer_manager))
            },
            None => Err(CommsBuilderError::PeerStorageNotProvided),
        }
    }

    /// Build comms services and handles. Services will not be started.
    pub fn build(mut self) -> Result<UnspawnedCommsNode, CommsBuilderError> {
        self.connection_manager_config.validate()?;
        let node_identity = self.node_identity.take().ok_or(CommsBuilderError::NodeIdentityNotSet)?;
        let shutdown_signal = self
            .shutdown_signal
            .take()
            .ok_or(CommsBuilderError::ShutdownSignalNotSet)?;

        let peer_manager = self.make_peer_manager()?;

        //---------------------------------- Connection Manager --------------------------------------------//
        let (conn_man_tx, connection_manager_request_rx) =
            mpsc::channel(consts::CONNECTION_MANAGER_REQUEST_BUFFER_SIZE);
        let (connection_manager_event_tx, _) = broadcast::channel(consts::CONNECTION_MANAGER_EVENTS_BUFFER_SIZE);
        let connection_manager_requester = ConnectionManagerRequester::new(conn_man_tx, connection_manager_event_tx);

        //---------------------------------- ConnectivityManager --------------------------------------------//
        let (connectivity_tx, connectivity_rx) = mpsc::channel(consts::CONNECTIVITY_MANAGER_REQUEST_BUFFER_SIZE);
        let (event_tx, _) = broadcast::channel(consts::CONNECTIVITY_MANAGER_EVENTS_BUFFER_SIZE);
        let connectivity_requester = ConnectivityRequester::new(connectivity_tx, event_tx);

        Ok(UnspawnedCommsNode {
            protocols: Default::default(),
            node_identity,
            connection_manager_requester,
            connection_manager_request_rx,
            shutdown_signal,
            builder: self,
            connectivity_requester,
            connectivity_rx,
            peer_manager,
            protocol_extensions: ProtocolExtensions::new(),
        })
    }
}

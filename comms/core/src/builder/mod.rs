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
mod placeholder;

#[cfg(test)]
mod tests;

use std::{fs::File, sync::Arc, time::Duration};

use tari_shutdown::ShutdownSignal;
use tokio::sync::{broadcast, mpsc};

use crate::{
    backoff::{Backoff, BoxedBackoff, ConstantBackoff},
    connection_manager::{ConnectionManagerConfig, ConnectionManagerRequester},
    connectivity::{ConnectivityConfig, ConnectivityRequester},
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerManager},
    peer_validator::PeerValidatorConfig,
    protocol::{NodeNetworkInfo, ProtocolExtensions},
    tor,
    types::CommsDatabase,
};

/// # CommsBuilder
///
/// [CommsBuilder] is used to customize and spawn Tari comms core.
///
/// The following example will get a node customized for your own network up and running.
///
/// ```rust
/// # use std::{sync::Arc, time::Duration};
/// # use rand::rngs::OsRng;
/// # use tari_shutdown::Shutdown;
/// # use tari_comms::{
/// #     {CommsBuilder, NodeIdentity},
/// #    peer_manager::{PeerStorage, PeerFeatures},
/// #    transports::TcpTransport,
/// # };
/// # #[tokio::main]
/// # async fn main() {
/// use std::env::temp_dir;
/// use tari_comms::connectivity::ConnectivityConfig;
///
/// use tari_storage::{
///     lmdb_store::{LMDBBuilder, LMDBConfig},
///     LMDBWrapper,
/// };
/// let node_identity = Arc::new(NodeIdentity::random(
///     &mut OsRng,
///     "/dns4/basenodezforhire.com/tcp/18000".parse().unwrap(),
///     PeerFeatures::COMMUNICATION_NODE,
/// ));
/// node_identity.sign();
/// let mut shutdown = Shutdown::new();
/// let datastore = LMDBBuilder::new()
///     .set_path(temp_dir())
///     .set_env_config(LMDBConfig::default())
///     .set_max_number_of_databases(1)
///     .add_database("peers", lmdb_zero::db::CREATE)
///     .build()
///     .unwrap();
///
/// let peer_database = datastore.get_handle("peers").unwrap();
/// let peer_database = LMDBWrapper::new(Arc::new(peer_database));
///
/// let unspawned_node = CommsBuilder::new()
///   // .with_listener_address("/ip4/0.0.0.0/tcp/18000".parse().unwrap())
///   .with_node_identity(node_identity)
///   .with_peer_storage(peer_database, None)
///   .with_shutdown_signal(shutdown.to_signal())
///   .build()
///   .unwrap();
/// // This is your chance to add customizations that may require comms components for e.g. PeerManager.
/// // let my_peer = Peer::new(...);
/// // unspawned_node.peer_manager().add_peer(my_peer.clone());
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
    peer_storage_file_lock: Option<File>,
    node_identity: Option<Arc<NodeIdentity>>,
    dial_backoff: BoxedBackoff,
    hidden_service_ctl: Option<tor::HiddenServiceController>,
    connection_manager_config: ConnectionManagerConfig,
    connectivity_config: ConnectivityConfig,
    shutdown_signal: Option<ShutdownSignal>,
    maintain_n_closest_connections_only: Option<usize>,
}

impl Default for CommsBuilder {
    fn default() -> Self {
        Self {
            peer_storage: None,
            peer_storage_file_lock: None,
            node_identity: None,
            dial_backoff: Box::new(ConstantBackoff::new(Duration::from_millis(500))),
            hidden_service_ctl: None,
            connection_manager_config: ConnectionManagerConfig::default(),
            connectivity_config: ConnectivityConfig::default(),
            shutdown_signal: None,
            maintain_n_closest_connections_only: None,
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
    pub fn with_peer_storage(mut self, peer_storage: CommsDatabase, file_lock: Option<File>) -> Self {
        self.peer_storage = Some(peer_storage);
        self.peer_storage_file_lock = file_lock;
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
        if let Some(val) = connections {
            self.connectivity_config.reaper_min_connection_threshold = val;
        }
        self.connectivity_config.connection_pool_refresh_interval = Duration::from_secs(180);
        self
    }

    fn make_peer_manager(&mut self) -> Result<Arc<PeerManager>, CommsBuilderError> {
        let file_lock = self.peer_storage_file_lock.take();

        match self.peer_storage.take() {
            Some(storage) => {
                #[cfg(not(test))]
                PeerManager::migrate_lmdb(&storage.inner())?;

                let peer_manager = PeerManager::new(storage, file_lock).map_err(CommsBuilderError::PeerManagerError)?;
                Ok(Arc::new(peer_manager))
            },
            None => Err(CommsBuilderError::PeerStorageNotProvided),
        }
    }

    /// Build comms services and handles. Services will not be started.
    pub fn build(mut self) -> Result<UnspawnedCommsNode, CommsBuilderError> {
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

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

//! # CommsBuilder
//!
//! The [CommsBuilder] provides a simple builder API for getting Tari comms p2p messaging up and running.
//!
//! [CommsBuilder]: ./builder/struct.CommsBuilder.html

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

use crate::{
    backoff::{Backoff, BoxedBackoff, ExponentialBackoff},
    connection_manager::{ConnectionManagerConfig, ConnectionManagerRequester},
    connectivity::{ConnectivityConfig, ConnectivityRequester},
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerManager},
    protocol::ProtocolExtensions,
    tor,
    types::CommsDatabase,
};
use futures::channel::mpsc;
use std::sync::Arc;
use tari_shutdown::ShutdownSignal;
use tokio::sync::broadcast;

/// The `CommsBuilder` provides a simple builder API for getting Tari comms p2p messaging up and running.
pub struct CommsBuilder {
    peer_storage: Option<CommsDatabase>,
    node_identity: Option<Arc<NodeIdentity>>,
    dial_backoff: BoxedBackoff,
    hidden_service_ctl: Option<tor::HiddenServiceController>,
    connection_manager_config: ConnectionManagerConfig,
    connectivity_config: ConnectivityConfig,

    shutdown_signal: Option<ShutdownSignal>,
}

impl Default for CommsBuilder {
    fn default() -> Self {
        Self {
            peer_storage: None,
            node_identity: None,
            dial_backoff: Box::new(ExponentialBackoff::default()),
            hidden_service_ctl: None,
            connection_manager_config: ConnectionManagerConfig::default(),
            connectivity_config: ConnectivityConfig::default(),
            shutdown_signal: None,
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
    pub fn with_user_agent<T: ToString>(mut self, user_agent: T) -> Self {
        self.connection_manager_config.user_agent = user_agent.to_string();
        self
    }

    /// Allow test addresses (memory addresses, local loopback etc). This should only be activated for tests.
    pub fn allow_test_addresses(mut self) -> Self {
        #[cfg(not(debug_assertions))]
        log::warn!(
            target: "comms::builder",
            "Test addresses are enabled! This is invalid and potentially insecure when running a production node."
        );
        self.connection_manager_config.allow_test_addresses = true;
        self
    }

    pub fn with_listener_address(mut self, listener_address: Multiaddr) -> Self {
        self.connection_manager_config.listener_address = listener_address;
        self
    }

    pub fn with_listener_liveness_max_sessions(mut self, max_sessions: usize) -> Self {
        self.connection_manager_config.liveness_max_sessions = max_sessions;
        self
    }

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
    pub fn with_min_connectivity(mut self, min_connectivity: f32) -> Self {
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

    /// Set the backoff that [ConnectionManager] uses when dialing peers. This is optional. If omitted the default
    /// ExponentialBackoff is used. [ConnectionManager]: crate::connection_manager::next::ConnectionManager
    pub fn with_dial_backoff<T>(mut self, backoff: T) -> Self
    where T: Backoff + Send + Sync + 'static {
        self.dial_backoff = Box::new(backoff);
        self
    }

    fn make_peer_manager(&mut self) -> Result<Arc<PeerManager>, CommsBuilderError> {
        match self.peer_storage.take() {
            Some(storage) => {
                // TODO: Peer manager should be refactored to be backend agnostic
                #[cfg(not(test))]
                PeerManager::migrate_lmdb(&storage.inner())?;

                let peer_manager = PeerManager::new(storage).map_err(CommsBuilderError::PeerManagerError)?;
                Ok(Arc::new(peer_manager))
            },
            None => Err(CommsBuilderError::PeerStorageNotProvided),
        }
    }

    /// Build comms services and handles. Services will not be started.
    pub fn build(mut self) -> Result<UnspawnedCommsNode, CommsBuilderError> {
        let node_identity = self
            .node_identity
            .take()
            .ok_or_else(|| CommsBuilderError::NodeIdentityNotSet)?;
        let shutdown_signal = self
            .shutdown_signal
            .take()
            .ok_or_else(|| CommsBuilderError::ShutdownSignalNotSet)?;

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

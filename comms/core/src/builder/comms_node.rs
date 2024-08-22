// Copyright 2020, The Tari Project
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

use std::{iter, sync::Arc};

use log::*;
use tari_shutdown::ShutdownSignal;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc, watch},
};

use super::{CommsBuilderError, CommsShutdown};
use crate::{
    connection_manager::{
        ConnectionManager,
        ConnectionManagerEvent,
        ConnectionManagerRequest,
        ConnectionManagerRequester,
        SelfLivenessCheck,
        SelfLivenessStatus,
    },
    connectivity::{ConnectivityEventRx, ConnectivityManager, ConnectivityRequest, ConnectivityRequester},
    multiaddr::Multiaddr,
    peer_manager::{NodeIdentity, PeerManager},
    protocol::{
        ProtocolExtension,
        ProtocolExtensionContext,
        ProtocolExtensions,
        ProtocolId,
        ProtocolNotificationTx,
        Protocols,
    },
    tor,
    transports::Transport,
    CommsBuilder,
    Substream,
};

const LOG_TARGET: &str = "comms::node";

/// Contains the built comms services
pub struct UnspawnedCommsNode {
    pub(super) node_identity: Arc<NodeIdentity>,
    pub(super) builder: CommsBuilder,
    pub(super) connection_manager_request_rx: mpsc::Receiver<ConnectionManagerRequest>,
    pub(super) connection_manager_requester: ConnectionManagerRequester,
    pub(super) connectivity_requester: ConnectivityRequester,
    pub(super) connectivity_rx: mpsc::Receiver<ConnectivityRequest>,
    pub(super) peer_manager: Arc<PeerManager>,
    pub(super) protocol_extensions: ProtocolExtensions,
    pub(super) protocols: Protocols<Substream>,
    pub(super) shutdown_signal: ShutdownSignal,
}

impl UnspawnedCommsNode {
    /// Add an RPC server/router in this instance of Tari comms.
    ///
    /// ```compile_fail
    /// # use tari_comms::CommsBuilder;
    /// # use tari_comms::protocol::rpc::RpcServer;
    /// let server = RpcServer::new().add_service(MyService).add_service(AnotherService);
    /// CommsBuilder::new().add_rpc_service(server).build();
    /// ```
    #[cfg(feature = "rpc")]
    pub fn add_rpc_server<T: ProtocolExtension + 'static>(mut self, rpc: T) -> Self {
        // Rpc router is treated the same as any other `ProtocolExtension` however this method may make it clearer for
        // users that this is the correct way to add the RPC server
        self.protocol_extensions.add(rpc);
        self
    }

    /// Adds [ProtocolExtensions](crate::protocol::ProtocolExtensions) to this node.
    pub fn add_protocol_extensions(mut self, extensions: ProtocolExtensions) -> Self {
        self.protocol_extensions.extend(extensions);
        self
    }

    /// Adds an implementation of [ProtocolExtension](crate::protocol::ProtocolExtension) to this node.
    /// This is used to add custom protocols to Tari comms.
    pub fn add_protocol_extension<T: ProtocolExtension + 'static>(mut self, extension: T) -> Self {
        self.protocol_extensions.add(extension);
        self
    }

    /// Registers custom ProtocolIds and mpsc notifier. A [ProtocolNotification](crate::protocol::ProtocolNotification)
    /// will be sent on that channel whenever a remote peer requests to speak the given protocols.
    pub fn add_protocol<I: AsRef<[ProtocolId]>>(
        mut self,
        protocol: I,
        notifier: &ProtocolNotificationTx<Substream>,
    ) -> Self {
        self.protocols.add(protocol, notifier);
        self
    }

    /// Set the listener address. This is an alias to `CommsBuilder::with_listener_address`.
    pub fn with_listener_address(mut self, listener_address: Multiaddr) -> Self {
        self.builder = self.builder.with_listener_address(listener_address);
        self
    }

    /// Set the tor hidden service controller to associate with this comms instance
    pub fn with_hidden_service_controller(mut self, hidden_service_ctl: tor::HiddenServiceController) -> Self {
        self.builder.hidden_service_ctl = Some(hidden_service_ctl);
        self
    }

    /// Spawn a new node using the specified [Transport](crate::transports::Transport).
    #[allow(clippy::too_many_lines)]
    pub async fn spawn_with_transport<TTransport>(self, transport: TTransport) -> Result<CommsNode, CommsBuilderError>
    where
        TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
        TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        let noise_prologue = self.builder.get_noise_prologue();
        let UnspawnedCommsNode {
            builder,
            connection_manager_request_rx,
            connection_manager_requester,
            connectivity_requester,
            connectivity_rx,
            node_identity,
            shutdown_signal,
            peer_manager,
            protocol_extensions,
            protocols,
        } = self;

        let CommsBuilder {
            dial_backoff,
            connection_manager_config,
            connectivity_config,
            ..
        } = builder;

        //---------------------------------- Connectivity Manager --------------------------------------------//
        let connectivity_manager = ConnectivityManager {
            config: connectivity_config,
            request_rx: connectivity_rx,
            event_tx: connectivity_requester.get_event_publisher(),
            connection_manager: connection_manager_requester.clone(),
            node_identity: node_identity.clone(),
            peer_manager: peer_manager.clone(),
            shutdown_signal: shutdown_signal.clone(),
        };

        let mut ext_context = ProtocolExtensionContext::new(
            connectivity_requester.clone(),
            peer_manager.clone(),
            shutdown_signal.clone(),
        );

        debug!(
            target: LOG_TARGET,
            "Installing {} protocol extension(s)",
            protocol_extensions.len()
        );
        protocol_extensions.install_all(&mut ext_context)?;

        //---------------------------------- Connection Manager --------------------------------------------//

        let mut connection_manager = ConnectionManager::new(
            connection_manager_config.clone(),
            transport.clone(),
            dial_backoff,
            connection_manager_request_rx,
            node_identity.clone(),
            peer_manager.clone(),
            connection_manager_requester.get_event_publisher(),
            shutdown_signal.clone(),
            &noise_prologue,
        );

        ext_context.register_complete_signal(connection_manager.complete_signal());
        connection_manager.add_protocols(ext_context.take_protocols().expect("Protocols already taken"));
        connection_manager.add_protocols(protocols);

        //---------------------------------- Spawn Actors --------------------------------------------//
        connectivity_manager.spawn();
        connection_manager.spawn();

        debug!(target: LOG_TARGET, "Hello from comms!");
        info!(
            target: LOG_TARGET,
            "Your node's public key is '{}'",
            node_identity.public_key()
        );
        info!(
            target: LOG_TARGET,
            "Your node's network ID is '{}'",
            node_identity.node_id()
        );
        info!(
            target: LOG_TARGET,
            "Your node's public addresses are '{}'",
            node_identity
                .public_addresses()
                .iter()
                .map(|a| a.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );

        // Spawn liveness check now that we have the final address
        let public_addresses = node_identity.public_addresses();
        let liveness_watch = if public_addresses.is_empty() {
            watch::channel(SelfLivenessStatus::Disabled).1
        } else {
            connection_manager_config
                .self_liveness_self_check_interval
                .map(|interval| {
                    SelfLivenessCheck::spawn(transport, public_addresses, interval, shutdown_signal.clone())
                })
                .unwrap_or_else(|| watch::channel(SelfLivenessStatus::Disabled).1)
        };

        Ok(CommsNode {
            shutdown_signal,
            connection_manager_requester,
            connectivity_requester,
            node_identity,
            peer_manager,
            liveness_watch,
            complete_signals: ext_context.drain_complete_signals(),
        })
    }

    /// Return a cloned atomic reference of the PeerManager
    pub fn peer_manager(&self) -> Arc<PeerManager> {
        Arc::clone(&self.peer_manager)
    }

    /// Return a cloned atomic reference of the NodeIdentity
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        Arc::clone(&self.node_identity)
    }

    /// Return an owned copy of a ConnectivityRequester. This is the async interface to the ConnectivityManager
    pub fn connectivity(&self) -> ConnectivityRequester {
        self.connectivity_requester.clone()
    }

    /// Returns an owned copy`ShutdownSignal`
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown_signal.clone()
    }
}

/// CommsNode is a handle to a comms node.
///
/// It allows communication with the internals of tari_comms.
#[derive(Clone)]
pub struct CommsNode {
    /// The `ShutdownSignal` for this node. Use `wait_until_shutdown` to asynchronously block until the
    /// shutdown signal is triggered.
    shutdown_signal: ShutdownSignal,
    /// Requester object for the ConnectionManager
    connection_manager_requester: ConnectionManagerRequester,
    /// Requester for the ConnectivityManager
    connectivity_requester: ConnectivityRequester,
    /// Node identity for this node
    node_identity: Arc<NodeIdentity>,
    /// Shared PeerManager instance
    peer_manager: Arc<PeerManager>,
    /// Current liveness status
    liveness_watch: watch::Receiver<SelfLivenessStatus>,
    /// The 'reciprocal' shutdown signals for each comms service
    complete_signals: Vec<ShutdownSignal>,
}

impl CommsNode {
    /// Get a subscription to `ConnectionManagerEvent`s
    pub fn subscribe_connection_manager_events(&self) -> broadcast::Receiver<Arc<ConnectionManagerEvent>> {
        self.connection_manager_requester.get_event_subscription()
    }

    pub fn connection_manager_requester(&mut self) -> &mut ConnectionManagerRequester {
        &mut self.connection_manager_requester
    }

    /// Get a subscription to `ConnectivityEvent`s
    pub fn subscribe_connectivity_events(&self) -> ConnectivityEventRx {
        self.connectivity_requester.get_event_subscription()
    }

    /// Return a cloned atomic reference of the PeerManager
    pub fn peer_manager(&self) -> Arc<PeerManager> {
        Arc::clone(&self.peer_manager)
    }

    /// Return a cloned atomic reference of the NodeIdentity
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        Arc::clone(&self.node_identity)
    }

    /// Return a reference to the NodeIdentity
    pub fn node_identity_ref(&self) -> &NodeIdentity {
        &self.node_identity
    }

    /// Returns the current liveness status
    pub fn liveness_status(&self) -> SelfLivenessStatus {
        *self.liveness_watch.borrow()
    }

    /// Return a handle that is used to call the connectivity service.
    pub fn connectivity(&self) -> ConnectivityRequester {
        self.connectivity_requester.clone()
    }

    /// Returns a new `ShutdownSignal`
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown_signal.clone()
    }

    /// Wait for comms to shutdown once the shutdown signal is triggered and for comms services to shut down.
    /// The object is consumed to ensure that no handles/channels are kept after shutdown
    pub fn wait_until_shutdown(self) -> CommsShutdown {
        CommsShutdown::new(iter::once(self.shutdown_signal).chain(self.complete_signals))
    }
}

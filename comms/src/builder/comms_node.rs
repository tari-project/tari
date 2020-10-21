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

use super::{CommsBuilderError, CommsShutdown};
use crate::{
    connection_manager::{
        ConnectionManager,
        ConnectionManagerEvent,
        ConnectionManagerRequest,
        ConnectionManagerRequester,
    },
    connectivity::{ConnectivityEventRx, ConnectivityManager, ConnectivityRequest, ConnectivityRequester},
    multiaddr::Multiaddr,
    noise::NoiseConfig,
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
use futures::{channel::mpsc, AsyncRead, AsyncWrite, StreamExt};
use log::*;
use std::{iter, sync::Arc, time::Duration};
use tari_shutdown::ShutdownSignal;
use tokio::{sync::broadcast, time};

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

    pub fn add_protocol_extensions(mut self, extensions: ProtocolExtensions) -> Self {
        self.protocol_extensions.extend(extensions);
        self
    }

    /// Add a protocol extension
    pub fn add_protocol_extension<T: ProtocolExtension + 'static>(mut self, extension: T) -> Self {
        self.protocol_extensions.add(extension);
        self
    }

    pub fn add_protocol<I: AsRef<[ProtocolId]>>(
        mut self,
        protocol: I,
        notifier: ProtocolNotificationTx<Substream>,
    ) -> Self
    {
        self.protocols.add(protocol, notifier);
        self
    }

    /// Set the listener address
    pub fn with_listener_address(mut self, listener_address: Multiaddr) -> Self {
        self.builder = self.builder.with_listener_address(listener_address);
        self
    }

    /// Set the tor hidden service controller to associate with this comms instance
    pub fn with_hidden_service_controller(mut self, hidden_service_ctl: tor::HiddenServiceController) -> Self {
        self.builder.hidden_service_ctl = Some(hidden_service_ctl);
        self
    }

    /// Wait until the ConnectionManager emits a Listening event. This is the signal that comms is ready.
    async fn wait_listening(
        mut events: broadcast::Receiver<Arc<ConnectionManagerEvent>>,
    ) -> Result<Multiaddr, CommsBuilderError> {
        loop {
            let event = time::timeout(Duration::from_secs(10), events.next())
                .await
                .map_err(|_| CommsBuilderError::ConnectionManagerEventStreamTimeout)?
                .ok_or(CommsBuilderError::ConnectionManagerEventStreamClosed)?
                .map_err(|_| CommsBuilderError::ConnectionManagerEventStreamLagged)?;

            match &*event {
                ConnectionManagerEvent::Listening(addr) => return Ok(addr.clone()),
                ConnectionManagerEvent::ListenFailed(err) => return Err(err.clone().into()),
                _ => {},
            }
        }
    }

    pub async fn spawn_with_transport<TTransport>(self, transport: TTransport) -> Result<CommsNode, CommsBuilderError>
    where
        TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
        TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
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
            hidden_service_ctl,
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

        let noise_config = NoiseConfig::new(node_identity.clone());

        let mut connection_manager = ConnectionManager::new(
            connection_manager_config,
            transport,
            noise_config,
            dial_backoff,
            connection_manager_request_rx,
            node_identity.clone(),
            peer_manager.clone(),
            connection_manager_requester.get_event_publisher(),
            shutdown_signal.clone(),
        );

        ext_context.register_complete_signal(connection_manager.complete_signal());
        connection_manager.add_protocols(ext_context.take_protocols().expect("Protocols already taken"));
        connection_manager.add_protocols(protocols);
        // Subscribe to events before spawning the actor to ensure that no events are missed
        let connection_manager_event_subscription = connection_manager_requester.get_event_subscription();

        //---------------------------------- Spawn Actors --------------------------------------------//
        connectivity_manager.create().spawn();
        connection_manager.spawn();

        info!(target: LOG_TARGET, "Hello from comms!");
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
            "Your node's public address is '{}'",
            node_identity.public_address()
        );

        let listening_addr = Self::wait_listening(connection_manager_event_subscription).await?;
        let mut hidden_service = None;
        if let Some(mut ctl) = hidden_service_ctl {
            ctl.set_proxied_addr(listening_addr.clone());
            let hs = ctl.create_hidden_service().await?;
            node_identity.set_public_address(hs.get_onion_address());
            hidden_service = Some(hs);
        }

        Ok(CommsNode {
            shutdown_signal,
            connection_manager_requester,
            connectivity_requester,
            listening_addr,
            node_identity,
            peer_manager,
            hidden_service,
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

    /// Return an owned copy of a ConnectionManagerRequester. Used to initiate connections to peers.
    pub fn connection_manager(&self) -> ConnectionManagerRequester {
        self.connection_manager_requester.clone()
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
/// It allows communication with the internals of tari_comms. Note that if this handle is dropped, tari_comms will shut
/// down.
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
    /// The resolved Ip-Tcp listening address.
    listening_addr: Multiaddr,
    /// `Some` if the comms node is configured to run via a hidden service, otherwise `None`
    hidden_service: Option<tor::HiddenService>,
    /// The 'reciprocal' shutdown signals for each comms service
    complete_signals: Vec<ShutdownSignal>,
}

impl CommsNode {
    /// Get a subscription to `ConnectionManagerEvent`s
    pub fn subscribe_connection_manager_events(&self) -> broadcast::Receiver<Arc<ConnectionManagerEvent>> {
        self.connection_manager_requester.get_event_subscription()
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

    /// Return the Ip/Tcp address that this node is listening on
    pub fn listening_address(&self) -> &Multiaddr {
        &self.listening_addr
    }

    /// Return the Ip/Tcp address that this node is listening on
    pub fn hidden_service(&self) -> Option<&tor::HiddenService> {
        self.hidden_service.as_ref()
    }

    /// Return an owned copy of a ConnectionManagerRequester. Used to initiate connections to peers.
    pub fn connection_manager(&self) -> ConnectionManagerRequester {
        self.connection_manager_requester.clone()
    }

    /// Return an owned copy of a ConnectivityRequester. This is the async interface to the ConnectivityManager
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

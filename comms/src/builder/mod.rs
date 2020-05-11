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
pub use comms_node::{BuiltCommsNode, CommsNode};

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
    connection_manager::{
        ConnectionManager,
        ConnectionManagerConfig,
        ConnectionManagerEvent,
        ConnectionManagerRequest,
        ConnectionManagerRequester,
    },
    message::InboundMessage,
    multiaddr::Multiaddr,
    multiplexing::Substream,
    noise::NoiseConfig,
    peer_manager::{NodeIdentity, PeerManager},
    protocol::{messaging, messaging::MessagingProtocol, ProtocolNotification, Protocols},
    tor,
    transports::{SocksTransport, TcpWithTorTransport, Transport},
    types::CommsDatabase,
};
use futures::{channel::mpsc, AsyncRead, AsyncWrite};
use log::*;
use std::sync::Arc;
use tari_shutdown::Shutdown;
use tokio::{runtime, sync::broadcast};

const LOG_TARGET: &str = "comms::builder";

/// The `CommsBuilder` provides a simple builder API for getting Tari comms p2p messaging up and running.
pub struct CommsBuilder<TTransport> {
    peer_storage: Option<CommsDatabase>,
    node_identity: Option<Arc<NodeIdentity>>,
    transport: Option<TTransport>,
    executor: Option<runtime::Handle>,
    protocols: Option<Protocols<Substream>>,
    dial_backoff: Option<BoxedBackoff>,
    hidden_service: Option<tor::HiddenService>,
    connection_manager_config: ConnectionManagerConfig,
    shutdown: Shutdown,
}

impl CommsBuilder<TcpWithTorTransport> {
    /// Create a new CommsBuilder
    pub fn new() -> Self {
        Default::default()
    }

    fn default_tcp_transport() -> TcpWithTorTransport {
        let mut tcp_with_tor = TcpWithTorTransport::new();
        tcp_with_tor.tcp_transport_mut().set_nodelay(true);
        tcp_with_tor
    }
}

impl Default for CommsBuilder<TcpWithTorTransport> {
    fn default() -> Self {
        Self {
            peer_storage: None,
            node_identity: None,
            transport: Some(Self::default_tcp_transport()),
            dial_backoff: Some(Box::new(ExponentialBackoff::default())),
            executor: None,
            protocols: None,
            hidden_service: None,
            connection_manager_config: ConnectionManagerConfig::default(),
            shutdown: Shutdown::new(),
        }
    }
}

impl<TTransport> CommsBuilder<TTransport>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
{
    /// Set the runtime handle to use for spawning tasks. If this is not set the handle that is executing
    /// `CommsBuilder::spawn` will be used, so this will rarely need to be explicitly set.
    pub fn with_executor(mut self, handle: runtime::Handle) -> Self {
        self.executor = Some(handle);
        self
    }

    /// Set the [NodeIdentity] for this comms instance. This is required.
    ///
    /// [OutboundMessagePool]: ../../outbound_message_service/index.html#outbound-message-pool
    pub fn with_node_identity(mut self, node_identity: Arc<NodeIdentity>) -> Self {
        self.node_identity = Some(node_identity);
        self
    }

    /// Allow test addresses (memory addresses, local loopback etc). This should only be activated for tests.
    pub fn allow_test_addresses(mut self) -> Self {
        #[cfg(not(debug_assertions))]
        warn!(
            target: LOG_TARGET,
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

    pub fn with_listener_liveness_whitelist_cidrs(mut self, cidrs: Vec<cidr::AnyIpCidr>) -> Self {
        self.connection_manager_config.liveness_cidr_whitelist = cidrs;
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

    /// Set the peer storage database to use.
    pub fn with_peer_storage(mut self, peer_storage: CommsDatabase) -> Self {
        self.peer_storage = Some(peer_storage);
        self
    }

    /// Configure the `CommsBuilder` to build a node which communicates using the given `tor::HiddenService`.
    pub fn configure_from_hidden_service(mut self, hidden_service: tor::HiddenService) -> CommsBuilder<SocksTransport> {
        // Set the listener address to be the address (usually local) to which tor will forward all traffic
        self.connection_manager_config.listener_address = hidden_service.proxied_address().clone();

        CommsBuilder {
            // Set the socks transport configured for this hidden service
            transport: Some(hidden_service.get_transport()),
            // Set the hidden service.
            hidden_service: Some(hidden_service),
            peer_storage: self.peer_storage,
            node_identity: self.node_identity,
            executor: self.executor,
            protocols: self.protocols,
            dial_backoff: self.dial_backoff,
            connection_manager_config: self.connection_manager_config,
            shutdown: self.shutdown,
        }
    }

    /// Set the backoff that [ConnectionManager] uses when dialing peers. This is optional. If omitted the default
    /// ExponentialBackoff is used. [ConnectionManager]: crate::connection_manager::next::ConnectionManager
    pub fn with_dial_backoff<T>(mut self, backoff: T) -> Self
    where T: Backoff + Send + Sync + 'static {
        self.dial_backoff = Some(Box::new(backoff));
        self
    }

    pub fn with_transport<T>(self, transport: T) -> CommsBuilder<T>
    where
        T: Transport + Unpin + Send + Sync + Clone + 'static,
        T::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    {
        CommsBuilder {
            transport: Some(transport),
            peer_storage: self.peer_storage,
            node_identity: self.node_identity,
            hidden_service: self.hidden_service,
            executor: self.executor,
            protocols: self.protocols,
            dial_backoff: self.dial_backoff,
            connection_manager_config: self.connection_manager_config,
            shutdown: self.shutdown,
        }
    }

    pub fn with_protocols(mut self, protocols: Protocols<Substream>) -> Self {
        self.protocols = Some(protocols);
        self
    }

    pub fn on_shutdown<F>(mut self, on_shutdown: F) -> Self
    where F: FnOnce() + Send + Sync + 'static {
        self.shutdown.on_triggered(on_shutdown);
        self
    }

    fn make_messaging(
        &self,
        conn_man_requester: ConnectionManagerRequester,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
    ) -> (
        messaging::MessagingProtocol,
        mpsc::Sender<ProtocolNotification<Substream>>,
        mpsc::Sender<messaging::MessagingRequest>,
        mpsc::Receiver<InboundMessage>,
        messaging::MessagingEventSender,
    )
    {
        let (proto_tx, proto_rx) = mpsc::channel(consts::MESSAGING_PROTOCOL_EVENTS_BUFFER_SIZE);
        let (messaging_request_tx, messaging_request_rx) = mpsc::channel(consts::MESSAGING_REQUEST_BUFFER_SIZE);
        let (inbound_message_tx, inbound_message_rx) = mpsc::channel(consts::INBOUND_MESSAGE_BUFFER_SIZE);
        let (event_tx, _) = broadcast::channel(consts::MESSAGING_EVENTS_BUFFER_SIZE);
        let messaging = MessagingProtocol::new(
            conn_man_requester,
            peer_manager,
            node_identity,
            proto_rx,
            messaging_request_rx,
            event_tx.clone(),
            inbound_message_tx,
            consts::MESSAGING_MAX_SEND_RETRIES,
            self.shutdown.to_signal(),
        );

        (messaging, proto_tx, messaging_request_tx, inbound_message_rx, event_tx)
    }

    fn make_peer_manager(&mut self) -> Result<Arc<PeerManager>, CommsBuilderError> {
        match self.peer_storage.take() {
            Some(storage) => {
                let peer_manager = PeerManager::new(storage).map_err(CommsBuilderError::PeerManagerError)?;
                Ok(Arc::new(peer_manager))
            },
            None => Err(CommsBuilderError::PeerStorageNotProvided),
        }
    }

    fn make_connection_manager(
        &mut self,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        protocols: Protocols<Substream>,
        request_rx: mpsc::Receiver<ConnectionManagerRequest>,
        connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    ) -> ConnectionManager<TTransport, BoxedBackoff>
    {
        let backoff = self.dial_backoff.take().expect("always set");
        let noise_config = NoiseConfig::new(Arc::clone(&node_identity));
        let config = self.connection_manager_config.clone();

        ConnectionManager::new(
            config,
            self.transport.take().expect("transport has already been taken"),
            noise_config,
            backoff,
            request_rx,
            node_identity,
            peer_manager,
            protocols,
            connection_manager_events_tx,
            self.shutdown.to_signal(),
        )
    }

    /// Build the required comms services. Services will not be started.
    pub fn build(mut self) -> Result<BuiltCommsNode<TTransport>, CommsBuilderError> {
        debug!(target: LOG_TARGET, "Building comms");
        let node_identity = self.node_identity.take().ok_or(CommsBuilderError::NodeIdentityNotSet)?;

        let peer_manager = self.make_peer_manager()?;

        //---------------------------------- Messaging --------------------------------------------//

        let (conn_man_tx, conn_man_rx) = mpsc::channel(consts::CONNECTION_MANAGER_REQUEST_BUFFER_SIZE);
        let (connection_manager_event_tx, _) = broadcast::channel(consts::CONNECTION_MANAGER_EVENTS_BUFFER_SIZE);
        let connection_manager_requester =
            ConnectionManagerRequester::new(conn_man_tx, connection_manager_event_tx.clone());

        let (messaging, messaging_proto_tx, messaging_request_tx, inbound_message_rx, messaging_event_tx) = self
            .make_messaging(
                connection_manager_requester.clone(),
                peer_manager.clone(),
                node_identity.clone(),
            );

        //---------------------------------- Protocols --------------------------------------------//
        let protocols = self
            .protocols
            .take()
            .or_else(|| Some(Protocols::new()))
            .map(move |protocols| protocols.add(&[messaging::MESSAGING_PROTOCOL.clone()], messaging_proto_tx))
            .expect("cannot fail");

        //---------------------------------- ConnectionManager --------------------------------------------//
        let connection_manager = self.make_connection_manager(
            node_identity.clone(),
            peer_manager.clone(),
            protocols,
            conn_man_rx,
            connection_manager_event_tx.clone(),
        );

        Ok(BuiltCommsNode {
            connection_manager,
            connection_manager_requester,
            connection_manager_event_tx,
            messaging_request_tx,
            messaging_pipeline: None,
            messaging,
            messaging_event_tx,
            inbound_message_rx,
            node_identity,
            peer_manager,
            hidden_service: self.hidden_service,
            shutdown: self.shutdown,
        })
    }
}

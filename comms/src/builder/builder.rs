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

use super::{consts, placeholder::PlaceholderService, CommsShutdown};
use crate::{
    backoff::{Backoff, BoxedBackoff, ExponentialBackoff},
    bounded_executor::BoundedExecutor,
    connection_manager::{
        ConnectionManager,
        ConnectionManagerConfig,
        ConnectionManagerError,
        ConnectionManagerEvent,
        ConnectionManagerRequest,
        ConnectionManagerRequester,
    },
    message::InboundMessage,
    multiaddr::Multiaddr,
    noise::NoiseConfig,
    peer_manager::{AsyncPeerManager, NodeIdentity, PeerManager, PeerManagerError},
    pipeline,
    protocol::{messaging, messaging::MessagingProtocol, ProtocolNotification, Protocols},
    tor,
    transports::{SocksTransport, TcpTransport, Transport},
    types::{CommsDatabase, CommsSubstream},
};
use derive_error::Error;
use futures::{channel::mpsc, AsyncRead, AsyncWrite, StreamExt};
use log::*;
use std::{fmt, fmt::Debug, sync::Arc, time::Duration};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{runtime, sync::broadcast, time};
use tower::Service;

const LOG_TARGET: &str = "comms::builder";

#[derive(Debug, Error)]
pub enum CommsBuilderError {
    PeerManagerError(PeerManagerError),
    ConnectionManagerError(ConnectionManagerError),
    /// Node identity not set. Call `with_node_identity(node_identity)` on [CommsBuilder]
    NodeIdentityNotSet,
    /// The PeerStorage was not provided to the CommsBuilder. Use `with_peer_storage` to set it.
    PeerStorageNotProvided,
    /// The messaging pipeline was not provided to the CommsBuilder. Use `with_messaging_pipeline` to set it.
    /// pipeline.
    MessagingPiplineNotProvided,
    /// Unable to receive a ConnectionManagerEvent within timeout
    ConnectionManagerEventStreamTimeout,
    /// ConnectionManagerEvent stream unexpectedly closed
    ConnectionManagerEventStreamClosed,
    /// Receiving on ConnectionManagerEvent stream lagged unexpectedly
    ConnectionManagerEventStreamLagged,
}

/// The `CommsBuilder` provides a simple builder API for getting Tari comms p2p messaging up and running.
pub struct CommsBuilder<TTransport> {
    peer_storage: Option<CommsDatabase>,
    node_identity: Option<Arc<NodeIdentity>>,
    transport: Option<TTransport>,
    executor: Option<runtime::Handle>,
    protocols: Option<Protocols<CommsSubstream>>,
    dial_backoff: Option<BoxedBackoff>,
    hidden_service: Option<tor::HiddenService>,
    connection_manager_config: ConnectionManagerConfig,
    shutdown: Shutdown,
}

impl CommsBuilder<TcpTransport> {
    /// Create a new CommsBuilder
    pub fn new() -> Self {
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

    fn default_tcp_transport() -> TcpTransport {
        let mut tcp = TcpTransport::new();
        tcp.set_nodelay(true);
        tcp
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

    pub fn with_listener_address(mut self, listener_address: Multiaddr) -> Self {
        self.connection_manager_config.listener_address = listener_address;
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

    pub fn with_protocols(mut self, protocols: Protocols<yamux::Stream>) -> Self {
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
        executor: runtime::Handle,
        conn_man_requester: ConnectionManagerRequester,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
    ) -> (
        messaging::MessagingProtocol,
        mpsc::Sender<ProtocolNotification<CommsSubstream>>,
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
            executor,
            conn_man_requester,
            peer_manager.into(),
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
        executor: runtime::Handle,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        protocols: Protocols<CommsSubstream>,
        request_rx: mpsc::Receiver<ConnectionManagerRequest>,
        connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    ) -> ConnectionManager<TTransport, BoxedBackoff>
    {
        let backoff = self.dial_backoff.take().expect("always set");
        let noise_config = NoiseConfig::new(Arc::clone(&node_identity));
        let config = self.connection_manager_config.clone();

        ConnectionManager::new(
            config,
            executor,
            self.transport.take().expect("transport has already been taken"),
            noise_config,
            backoff,
            request_rx,
            node_identity,
            peer_manager.into(),
            protocols,
            connection_manager_events_tx,
            self.shutdown.to_signal(),
        )
    }

    /// Build the required comms services. Services will not be started.
    pub fn build(mut self) -> Result<BuiltCommsNode<TTransport>, CommsBuilderError> {
        let node_identity = self.node_identity.take().ok_or(CommsBuilderError::NodeIdentityNotSet)?;
        let executor = self
            .executor
            .take()
            .or_else(|| Some(runtime::Handle::current()))
            .unwrap();

        let peer_manager = self.make_peer_manager()?;

        //---------------------------------- Messaging --------------------------------------------//

        let (conn_man_tx, conn_man_rx) = mpsc::channel(consts::CONNECTION_MANAGER_REQUEST_BUFFER_SIZE);
        let (connection_manager_event_tx, _) = broadcast::channel(consts::CONNECTION_MANAGER_EVENTS_BUFFER_SIZE);
        let connection_manager_requester =
            ConnectionManagerRequester::new(conn_man_tx, connection_manager_event_tx.clone());

        let (messaging, messaging_proto_tx, messaging_request_tx, inbound_message_rx, messaging_event_tx) = self
            .make_messaging(
                executor.clone(),
                connection_manager_requester.clone(),
                peer_manager.clone(),
                node_identity.clone(),
            );

        //---------------------------------- Protocols --------------------------------------------//
        let protocols = self
            .protocols
            .take()
            .or_else(|| Some(Protocols::new()))
            .map(move |protocols| protocols.add([messaging::MESSAGING_PROTOCOL], messaging_proto_tx))
            .expect("cannot fail");

        //---------------------------------- ConnectionManager --------------------------------------------//
        let connection_manager = self.make_connection_manager(
            executor.clone(),
            node_identity.clone(),
            peer_manager.clone(),
            protocols,
            conn_man_rx,
            connection_manager_event_tx.clone(),
        );

        Ok(BuiltCommsNode {
            executor,
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

/// Contains the built comms services
pub struct BuiltCommsNode<
    TTransport,
    TInPipe = PlaceholderService<InboundMessage, (), ()>,
    TOutPipe = PlaceholderService<(), (), ()>,
    TOutReq = (),
> {
    connection_manager: ConnectionManager<TTransport, BoxedBackoff>,
    connection_manager_requester: ConnectionManagerRequester,
    connection_manager_event_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    messaging_pipeline: Option<pipeline::Config<TInPipe, TOutPipe, TOutReq>>,
    executor: runtime::Handle,
    node_identity: Arc<NodeIdentity>,
    messaging: MessagingProtocol,
    messaging_event_tx: messaging::MessagingEventSender,
    inbound_message_rx: mpsc::Receiver<InboundMessage>,

    hidden_service: Option<tor::HiddenService>,
    messaging_request_tx: mpsc::Sender<messaging::MessagingRequest>,
    shutdown: Shutdown,
    peer_manager: Arc<PeerManager>,
}

impl<TTransport, TInPipe, TOutPipe, TOutReq> BuiltCommsNode<TTransport, TInPipe, TOutPipe, TOutReq>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TOutPipe: Service<TOutReq, Response = ()> + Clone + Send + 'static,
    TOutPipe::Error: fmt::Debug + Send,
    TOutPipe::Future: Send + 'static,
    TInPipe: Service<InboundMessage> + Clone + Send + 'static,
    TInPipe::Error: fmt::Debug + Send,
    TInPipe::Future: Send + 'static,
    TOutReq: Send + 'static,
{
    pub fn with_messaging_pipeline<I, O, R>(
        self,
        messaging_pipeline: pipeline::Config<I, O, R>,
    ) -> BuiltCommsNode<TTransport, I, O, R>
    where
        O: Service<R, Response = ()> + Clone + Send + 'static,
        O::Error: fmt::Debug + Send,
        O::Future: Send + 'static,
        I: Service<InboundMessage> + Clone + Send + 'static,
        I::Error: fmt::Debug + Send,
        I::Future: Send + 'static,
    {
        BuiltCommsNode {
            messaging_pipeline: Some(messaging_pipeline),

            connection_manager: self.connection_manager,
            connection_manager_requester: self.connection_manager_requester,
            connection_manager_event_tx: self.connection_manager_event_tx,
            node_identity: self.node_identity,
            messaging: self.messaging,
            messaging_event_tx: self.messaging_event_tx,
            inbound_message_rx: self.inbound_message_rx,
            executor: self.executor,
            shutdown: self.shutdown,
            messaging_request_tx: self.messaging_request_tx,
            hidden_service: self.hidden_service,
            peer_manager: self.peer_manager,
        }
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

    pub async fn spawn(self) -> Result<CommsNode, CommsBuilderError> {
        let BuiltCommsNode {
            connection_manager,
            connection_manager_requester,
            connection_manager_event_tx,
            messaging_pipeline,
            messaging_request_tx,
            inbound_message_rx,
            executor,
            node_identity,
            shutdown,
            peer_manager,
            messaging,
            messaging_event_tx,
            hidden_service,
        } = self;

        let messaging_pipeline = messaging_pipeline.ok_or(CommsBuilderError::MessagingPiplineNotProvided)?;

        let events_stream = connection_manager_event_tx.subscribe();
        let conn_man_shutdown_signal = connection_manager.complete_signal();

        executor.spawn(connection_manager.run());

        // Spawn messaging protocol
        let messaging_signal = messaging.complete_signal();
        executor.spawn(messaging.run());

        // Spawn inbound pipeline
        let bounded_executor = BoundedExecutor::new(executor.clone(), messaging_pipeline.max_concurrent_inbound_tasks);
        let inbound = pipeline::Inbound::new(bounded_executor, inbound_message_rx, messaging_pipeline.inbound);
        executor.spawn(inbound.run());

        // Spawn outbound pipeline
        let outbound = pipeline::Outbound::new(executor.clone(), messaging_pipeline.outbound, messaging_request_tx);
        executor.spawn(outbound.run());

        let listening_addr = Self::wait_listening(events_stream).await?;

        Ok(CommsNode {
            shutdown,
            connection_manager_event_tx,
            connection_manager_requester,
            listening_addr,
            node_identity,
            peer_manager,
            messaging_event_tx,
            hidden_service,
            executor,
            complete_signals: vec![messaging_signal, conn_man_shutdown_signal],
        })
    }

    /// Return a cloned atomic reference of the PeerManager
    pub fn peer_manager(&self) -> Arc<PeerManager> {
        Arc::clone(&self.peer_manager)
    }

    /// Return an asynchronous PeerManager
    pub fn async_peer_manager(&self) -> AsyncPeerManager {
        Arc::clone(&self.peer_manager).into()
    }

    /// Return a cloned atomic reference of the NodeIdentity
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        Arc::clone(&self.node_identity)
    }

    /// Return a cloned atomic reference of the NodeIdentity
    pub fn executor(&self) -> &runtime::Handle {
        &self.executor
    }

    /// Return a subscription to OMS events. This will emit events sent _after_ this subscription was created.
    pub fn subscribe_messaging_events(&self) -> messaging::MessagingEventReceiver {
        self.messaging_event_tx.subscribe()
    }

    /// Return an owned copy of a ConnectionManagerRequester. Used to initiate connections to peers.
    pub fn connection_manager_requester(&self) -> ConnectionManagerRequester {
        self.connection_manager_requester.clone()
    }

    /// Returns a new `ShutdownSignal`
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown.to_signal()
    }
}

/// CommsNode is a handle to a comms node.
///
/// It allows communication with the internals of tari_comms. Note that if this handle is dropped, tari_comms will shut
/// down.
pub struct CommsNode {
    /// The Shutdown instance for this node. All applicable internal services will use this as a signal to shutdown.
    shutdown: Shutdown,
    /// Connection manager broadcast event channel. A `broadcast::Sender` is kept because it can create subscriptions
    /// as needed.
    connection_manager_event_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    /// Requester object for the ConnectionManager
    connection_manager_requester: ConnectionManagerRequester,
    /// Node identity for this node
    node_identity: Arc<NodeIdentity>,
    /// Shared PeerManager instance
    peer_manager: Arc<PeerManager>,
    /// Tari messaging broadcast event channel. A `broadcast::Sender` is kept because it can create subscriptions as
    /// needed.
    messaging_event_tx: messaging::MessagingEventSender,
    /// The resolved Ip-Tcp listening address.
    listening_addr: Multiaddr,
    /// The executor handle used to run the comms stack
    executor: runtime::Handle,
    /// `Some` if the comms node is configured to run via a hidden service, otherwise `None`
    hidden_service: Option<tor::HiddenService>,
    /// The 'reciprocal' shutdown signals for each comms service
    complete_signals: Vec<ShutdownSignal>,
}

impl CommsNode {
    pub fn subscribe_connection_manager_events(&self) -> broadcast::Receiver<Arc<ConnectionManagerEvent>> {
        self.connection_manager_event_tx.subscribe()
    }

    /// Return a cloned atomic reference of the PeerManager
    pub fn peer_manager(&self) -> Arc<PeerManager> {
        Arc::clone(&self.peer_manager)
    }

    /// Return an asynchronous PeerManager
    pub fn async_peer_manager(&self) -> AsyncPeerManager {
        Arc::clone(&self.peer_manager).into()
    }

    /// Return a cloned atomic reference of the NodeIdentity
    pub fn node_identity(&self) -> Arc<NodeIdentity> {
        Arc::clone(&self.node_identity)
    }

    /// Return a cloned atomic reference of the NodeIdentity
    pub fn executor(&self) -> &runtime::Handle {
        &self.executor
    }

    /// Return the Ip/Tcp address that this node is listening on
    pub fn listening_address(&self) -> &Multiaddr {
        &self.listening_addr
    }

    /// Return the Ip/Tcp address that this node is listening on
    pub fn hidden_service(&self) -> Option<&tor::HiddenService> {
        self.hidden_service.as_ref()
    }

    /// Return a subscription to OMS events. This will emit events sent _after_ this subscription was created.
    pub fn subscribe_messaging_events(&self) -> messaging::MessagingEventReceiver {
        self.messaging_event_tx.subscribe()
    }

    /// Return an owned copy of a ConnectionManagerRequester. Used to initiate connections to peers.
    pub fn connection_manager(&self) -> ConnectionManagerRequester {
        self.connection_manager_requester.clone()
    }

    /// Returns a new `ShutdownSignal`
    pub fn shutdown_signal(&self) -> ShutdownSignal {
        self.shutdown.to_signal()
    }

    /// Shuts comms down. The object is consumed to ensure that no handles/channels are kept after shutdown
    pub fn shutdown(mut self) -> CommsShutdown {
        info!(target: LOG_TARGET, "Comms is shutting down");
        self.shutdown.trigger().expect("Shutdown failed to trigger signal");
        CommsShutdown::new(self.complete_signals)
    }
}

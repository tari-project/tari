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

use super::{placeholder::PlaceholderService, CommsBuilderError, CommsShutdown};
use crate::{
    backoff::BoxedBackoff,
    bounded_executor::BoundedExecutor,
    builder::consts,
    connection_manager::{ConnectionManager, ConnectionManagerEvent, ConnectionManagerRequester},
    connectivity::{ConnectivityEventRx, ConnectivityManager, ConnectivityRequester},
    message::InboundMessage,
    multiaddr::Multiaddr,
    multiplexing::Substream,
    peer_manager::{NodeIdentity, PeerManager},
    pipeline,
    protocol::{messaging, messaging::MessagingProtocol, ProtocolNotificationTx, Protocols},
    runtime,
    runtime::task,
    tor,
    transports::Transport,
};
use futures::{channel::mpsc, AsyncRead, AsyncWrite, StreamExt};
use log::*;
use std::{fmt, sync::Arc, time::Duration};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{sync::broadcast, time};
use tower::Service;

const LOG_TARGET: &str = "comms::node";

/// Contains the built comms services
pub struct BuiltCommsNode<
    TTransport,
    TInPipe = PlaceholderService<InboundMessage, (), ()>,
    TOutPipe = PlaceholderService<(), (), ()>,
    TOutReq = (),
> {
    pub connection_manager: ConnectionManager<TTransport, BoxedBackoff>,
    pub connection_manager_requester: ConnectionManagerRequester,
    pub connection_manager_event_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    pub connectivity_manager: ConnectivityManager,
    pub connectivity_requester: ConnectivityRequester,
    pub messaging_pipeline: Option<pipeline::Config<TInPipe, TOutPipe, TOutReq>>,
    pub node_identity: Arc<NodeIdentity>,
    pub hidden_service_ctl: Option<tor::HiddenServiceController>,
    pub peer_manager: Arc<PeerManager>,
    pub protocols: Protocols<Substream>,
    pub shutdown: Shutdown,
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
            connectivity_manager: self.connectivity_manager,
            connectivity_requester: self.connectivity_requester,
            node_identity: self.node_identity,
            shutdown: self.shutdown,
            protocols: self.protocols,
            hidden_service_ctl: self.hidden_service_ctl,
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
            mut connection_manager,
            connection_manager_requester,
            connection_manager_event_tx,
            connectivity_manager,
            connectivity_requester,
            messaging_pipeline,
            node_identity,
            shutdown,
            peer_manager,
            mut protocols,
            hidden_service_ctl,
        } = self;

        let mut complete_signals = Vec::new();
        let events_stream = connection_manager_event_tx.subscribe();
        complete_signals.push(connection_manager.complete_signal());

        // Connectivity manager
        task::spawn(connectivity_manager.create().run());

        let mut messaging_event_tx = None;
        if let Some(messaging_pipeline) = messaging_pipeline {
            let (messaging, notifier, messaging_request_tx, inbound_message_rx, messaging_event_sender) =
                initialize_messaging(connection_manager_requester.clone(), shutdown.to_signal());
            messaging_event_tx = Some(messaging_event_sender);
            protocols.add(&[messaging::MESSAGING_PROTOCOL.clone()], notifier);
            // Spawn messaging protocol
            complete_signals.push(messaging.complete_signal());
            task::spawn(messaging.run());

            // Spawn inbound pipeline
            let bounded_executor =
                BoundedExecutor::new(runtime::current(), messaging_pipeline.max_concurrent_inbound_tasks);
            let inbound = pipeline::Inbound::new(
                bounded_executor,
                inbound_message_rx,
                messaging_pipeline.inbound,
                shutdown.to_signal(),
            );
            task::spawn(inbound.run());

            // Spawn outbound pipeline
            let outbound =
                pipeline::Outbound::new(runtime::current(), messaging_pipeline.outbound, messaging_request_tx);
            task::spawn(outbound.run());
        }

        connection_manager.set_protocols(protocols);
        task::spawn(connection_manager.run());

        let listening_addr = Self::wait_listening(events_stream).await?;
        let mut hidden_service = None;
        if let Some(mut ctl) = hidden_service_ctl {
            ctl.set_proxied_addr(listening_addr.clone());
            let hs = ctl.create_hidden_service().await?;
            node_identity.set_public_address(hs.get_onion_address());
            hidden_service = Some(hs);
        }

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

        Ok(CommsNode {
            shutdown,
            connection_manager_event_tx,
            connection_manager_requester,
            connectivity_requester,
            listening_addr,
            node_identity,
            peer_manager,
            messaging_event_tx: messaging_event_tx.unwrap_or_else(|| broadcast::channel(1).0),
            hidden_service,
            complete_signals,
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
    pub fn connection_manager_requester(&self) -> ConnectionManagerRequester {
        self.connection_manager_requester.clone()
    }

    /// Return an owned copy of a ConnectivityRequester. This is the async interface to the ConnectivityManager
    pub fn connectivity(&self) -> ConnectivityRequester {
        self.connectivity_requester.clone()
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
    /// Requester for the ConnectivityManager
    connectivity_requester: ConnectivityRequester,
    /// Node identity for this node
    node_identity: Arc<NodeIdentity>,
    /// Shared PeerManager instance
    peer_manager: Arc<PeerManager>,
    /// Tari messaging broadcast event channel. A `broadcast::Sender` is kept because it can create subscriptions as
    /// needed.
    messaging_event_tx: messaging::MessagingEventSender,
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
        self.connection_manager_event_tx.subscribe()
    }

    /// Get a subscription to `ConnectivityEvent`s
    pub fn subscribe_connectivity_events(&self) -> ConnectivityEventRx {
        self.connectivity_requester.subscribe_event_stream()
    }

    /// Return a subscription to OMS events. This will emit events sent _after_ this subscription was created.
    pub fn subscribe_messaging_events(&self) -> messaging::MessagingEventReceiver {
        self.messaging_event_tx.subscribe()
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
        self.shutdown.to_signal()
    }

    /// Shuts comms down. The object is consumed to ensure that no handles/channels are kept after shutdown
    pub fn shutdown(mut self) -> CommsShutdown {
        info!(target: LOG_TARGET, "Comms is shutting down");
        self.shutdown.trigger().expect("Shutdown failed to trigger signal");
        CommsShutdown::new(self.complete_signals)
    }
}

fn initialize_messaging(
    connection_manager_requester: ConnectionManagerRequester,
    shutdown_signal: ShutdownSignal,
) -> (
    messaging::MessagingProtocol,
    ProtocolNotificationTx<Substream>,
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
        Default::default(),
        connection_manager_requester,
        proto_rx,
        messaging_request_rx,
        event_tx.clone(),
        inbound_message_tx,
        shutdown_signal,
    );

    (messaging, proto_tx, messaging_request_tx, inbound_message_rx, event_tx)
}

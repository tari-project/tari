// Copyright 2019, The Tari Project
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

use super::{
    dialer::{Dialer, DialerRequest},
    error::ConnectionManagerError,
    listener::PeerListener,
    peer_connection::PeerConnection,
    requester::ConnectionManagerRequest,
};
use crate::{
    backoff::Backoff,
    multiplexing::Substream,
    noise::NoiseConfig,
    peer_manager::{NodeId, NodeIdentity},
    protocol::{ProtocolEvent, ProtocolId, Protocols},
    runtime,
    transports::Transport,
    types::DEFAULT_LISTENER_ADDRESS,
    PeerManager,
};
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    AsyncRead,
    AsyncWrite,
    SinkExt,
    StreamExt,
};
use log::*;
use multiaddr::Multiaddr;
use std::{fmt, sync::Arc};
use tari_shutdown::{Shutdown, ShutdownSignal};
use time::Duration;
use tokio::{sync::broadcast, task, time};

const LOG_TARGET: &str = "comms::connection_manager::manager";

const EVENT_CHANNEL_SIZE: usize = 32;
const DIALER_REQUEST_CHANNEL_SIZE: usize = 32;

#[derive(Debug)]
pub enum ConnectionManagerEvent {
    // Peer connection
    PeerConnected(PeerConnection),
    PeerDisconnected(Box<NodeId>),
    PeerConnectFailed(Box<NodeId>, ConnectionManagerError),
    PeerInboundConnectFailed(ConnectionManagerError),

    // Listener
    Listening(Multiaddr),
    ListenFailed(ConnectionManagerError),

    // Substreams
    NewInboundSubstream(Box<NodeId>, ProtocolId, Substream),
}

impl fmt::Display for ConnectionManagerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use ConnectionManagerEvent::*;
        match self {
            PeerConnected(conn) => write!(f, "PeerConnected({})", conn),
            PeerDisconnected(node_id) => write!(f, "PeerDisconnected({})", node_id.short_str()),
            PeerConnectFailed(node_id, err) => write!(f, "PeerConnectFailed({}, {:?})", node_id.short_str(), err),
            PeerInboundConnectFailed(err) => write!(f, "PeerInboundConnectFailed({:?})", err),
            Listening(addr) => write!(f, "Listening({})", addr),
            ListenFailed(err) => write!(f, "ListenFailed({:?})", err),
            NewInboundSubstream(node_id, protocol, _) => write!(
                f,
                "NewInboundSubstream({}, {}, Stream)",
                node_id.short_str(),
                String::from_utf8_lossy(protocol)
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConnectionManagerConfig {
    /// The address to listen on for incoming connections. This address must be supported by the transport.
    /// Default: DEFAULT_LISTENER_ADDRESS constant
    pub listener_address: Multiaddr,
    /// The number of dial attempts to make before giving up. Default: 3
    pub max_dial_attempts: usize,
    /// The maximum number of connection tasks that will be spawned at the same time. Once this limit is reached, peers
    /// attempting to connect will have to wait for another connection attempt to complete. Default: 20
    pub max_simultaneous_inbound_connects: usize,
    /// Set to true to allow peers to send loopback, local-link and other addresses normally not considered valid for
    /// peer-to-peer comms. Default: false
    pub allow_test_addresses: bool,
    /// The maximum time to wait for the first byte before closing the connection. Default: 7s
    pub time_to_first_byte: Duration,
    /// The number of liveness check sessions to allow. Default: 0
    pub liveness_max_sessions: usize,
    /// CIDR blocks that allowlist liveness checks. Default: Localhost only (127.0.0.1/32)
    pub liveness_cidr_allowlist: Vec<cidr::AnyIpCidr>,
    /// The user agent string for this node
    pub user_agent: String,
}

impl Default for ConnectionManagerConfig {
    fn default() -> Self {
        Self {
            listener_address: DEFAULT_LISTENER_ADDRESS
                .parse()
                .expect("DEFAULT_LISTENER_ADDRESS is malformed"),
            max_dial_attempts: 3,
            max_simultaneous_inbound_connects: 20,
            #[cfg(not(test))]
            allow_test_addresses: false,
            // This must always be true for internal crate tests
            #[cfg(test)]
            allow_test_addresses: true,
            liveness_max_sessions: 0,
            time_to_first_byte: Duration::from_secs(7),
            liveness_cidr_allowlist: vec![cidr::AnyIpCidr::V4("127.0.0.1/32".parse().unwrap())],
            user_agent: Default::default(),
        }
    }
}

pub struct ConnectionManager<TTransport, TBackoff> {
    request_rx: Fuse<mpsc::Receiver<ConnectionManagerRequest>>,
    internal_event_rx: Fuse<mpsc::Receiver<ConnectionManagerEvent>>,
    dialer_tx: mpsc::Sender<DialerRequest>,
    dialer: Option<Dialer<TTransport, TBackoff>>,
    listener: Option<PeerListener<TTransport>>,
    peer_manager: Arc<PeerManager>,
    shutdown_signal: Option<ShutdownSignal>,
    protocols: Protocols<Substream>,
    listener_address: Option<Multiaddr>,
    listening_notifiers: Vec<oneshot::Sender<Multiaddr>>,
    connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    complete_trigger: Shutdown,
}

impl<TTransport, TBackoff> ConnectionManager<TTransport, TBackoff>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TBackoff: Backoff + Send + Sync + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: ConnectionManagerConfig,
        transport: TTransport,
        noise_config: NoiseConfig,
        backoff: TBackoff,
        request_rx: mpsc::Receiver<ConnectionManagerRequest>,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        let (internal_event_tx, internal_event_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);

        let (dialer_tx, dialer_rx) = mpsc::channel(DIALER_REQUEST_CHANNEL_SIZE);

        let listener = PeerListener::new(
            config.clone(),
            transport.clone(),
            noise_config.clone(),
            internal_event_tx.clone(),
            peer_manager.clone(),
            Arc::clone(&node_identity),
            shutdown_signal.clone(),
        );

        let dialer = Dialer::new(
            config,
            node_identity,
            peer_manager.clone(),
            transport,
            noise_config,
            backoff,
            dialer_rx,
            internal_event_tx,
            shutdown_signal.clone(),
        );

        Self {
            shutdown_signal: Some(shutdown_signal),
            request_rx: request_rx.fuse(),
            peer_manager,
            protocols: Protocols::new(),
            internal_event_rx: internal_event_rx.fuse(),
            dialer_tx,
            dialer: Some(dialer),
            listener: Some(listener),
            listener_address: None,
            listening_notifiers: Vec::new(),
            connection_manager_events_tx,
            complete_trigger: Shutdown::new(),
        }
    }

    pub fn add_protocols(&mut self, protocols: Protocols<Substream>) -> &mut Self {
        self.protocols.extend(protocols);
        self
    }

    pub fn complete_signal(&self) -> ShutdownSignal {
        self.complete_trigger.to_signal()
    }

    pub fn spawn(self) -> task::JoinHandle<()> {
        task::spawn(self.run())
    }

    pub async fn run(mut self) {
        let mut shutdown = self
            .shutdown_signal
            .take()
            .expect("ConnectionManager initialized without a shutdown");

        self.run_listener();
        self.run_dialer();

        debug!(
            target: LOG_TARGET,
            "Connection manager started. Protocols supported by this node: {}",
            self.protocols
                .iter()
                .map(|p| String::from_utf8_lossy(p))
                .collect::<Vec<_>>()
                .join(", ")
        );
        loop {
            futures::select! {
                event = self.internal_event_rx.select_next_some() => {
                    self.handle_event(event).await;
                },

                request = self.request_rx.select_next_some() => {
                    self.handle_request(request).await;
                },

                _ = shutdown => {
                    info!(target: LOG_TARGET, "ConnectionManager is shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
    }

    fn run_listener(&mut self) {
        let mut listener = self
            .listener
            .take()
            .expect("ConnectionManager initialized without a listener");

        listener.set_supported_protocols(self.protocols.get_supported_protocols());
        runtime::current().spawn(listener.run());
    }

    fn run_dialer(&mut self) {
        let mut dialer = self
            .dialer
            .take()
            .expect("ConnectionManager initialized without a dialer");

        dialer.set_supported_protocols(self.protocols.get_supported_protocols());
        runtime::current().spawn(dialer.run());
    }

    async fn handle_request(&mut self, request: ConnectionManagerRequest) {
        use ConnectionManagerRequest::*;
        trace!(target: LOG_TARGET, "Connection manager got request: {:?}", request);
        match request {
            DialPeer(node_id, reply) => self.dial_peer(node_id, reply).await,
            CancelDial(node_id) => {
                if let Err(err) = self.dialer_tx.send(DialerRequest::CancelPendingDial(node_id)).await {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send cancel dial request to dialer: {}", err
                    );
                }
            },
            NotifyListening(reply) => match self.listener_address.as_ref() {
                Some(addr) => {
                    let _ = reply.send(addr.clone());
                },
                None => {
                    self.listening_notifiers.push(reply);
                },
            },
        }
    }

    async fn handle_event(&mut self, event: ConnectionManagerEvent) {
        use ConnectionManagerEvent::*;

        match event {
            Listening(addr) => {
                self.listener_address = Some(addr.clone());
                self.publish_event(ConnectionManagerEvent::Listening(addr.clone()));
                for notifier in self.listening_notifiers.drain(..) {
                    let _ = notifier.send(addr.clone());
                }
            },
            NewInboundSubstream(node_id, protocol, stream) => {
                let proto_str = String::from_utf8_lossy(&protocol);
                debug!(
                    target: LOG_TARGET,
                    "New inbound substream for peer '{}' speaking protocol '{}'",
                    node_id.short_str(),
                    proto_str
                );
                if let Err(err) = self
                    .protocols
                    .notify(&protocol, ProtocolEvent::NewInboundSubstream(*node_id, stream))
                    .await
                {
                    error!(
                        target: LOG_TARGET,
                        "Error sending NewSubstream notification for protocol '{}' because '{:?}'", proto_str, err
                    );
                }
            },

            event => {
                self.publish_event(event);
            },
        }
    }

    #[inline]
    async fn send_dialer_request(&mut self, req: DialerRequest) {
        if let Err(err) = self.dialer_tx.send(req).await {
            error!(target: LOG_TARGET, "Failed to send request to dialer because '{}'", err);
        }
    }

    fn publish_event(&self, event: ConnectionManagerEvent) {
        // Error on no subscribers can be ignored
        let _ = self.connection_manager_events_tx.send(Arc::new(event));
    }

    async fn dial_peer(
        &mut self,
        node_id: NodeId,
        reply: oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
    )
    {
        match self.peer_manager.find_by_node_id(&node_id).await {
            Ok(peer) => {
                self.send_dialer_request(DialerRequest::Dial(Box::new(peer), reply))
                    .await;
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "Failed to fetch peer to dial because '{}'", err);
                let _ = reply.send(Err(ConnectionManagerError::PeerManagerError(err)));
            },
        }
    }
}

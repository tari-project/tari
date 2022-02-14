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

use std::{fmt, sync::Arc};

use log::*;
use multiaddr::Multiaddr;
use tari_shutdown::{Shutdown, ShutdownSignal};
use time::Duration;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc, oneshot},
    task,
    time,
};
use tracing::{span, Instrument, Level};

use super::{
    dialer::{Dialer, DialerRequest},
    error::ConnectionManagerError,
    listener::PeerListener,
    peer_connection::PeerConnection,
    requester::ConnectionManagerRequest,
};
use crate::{
    backoff::Backoff,
    connection_manager::{metrics, ConnectionDirection, ConnectionId},
    multiplexing::Substream,
    noise::NoiseConfig,
    peer_manager::{NodeId, NodeIdentity, PeerManagerError},
    protocol::{NodeNetworkInfo, ProtocolEvent, ProtocolId, Protocols},
    transports::{TcpTransport, Transport},
    PeerManager,
};

const LOG_TARGET: &str = "comms::connection_manager::manager";

const EVENT_CHANNEL_SIZE: usize = 32;
const DIALER_REQUEST_CHANNEL_SIZE: usize = 32;

#[derive(Debug)]
pub enum ConnectionManagerEvent {
    // Peer connection
    PeerConnected(PeerConnection),
    PeerDisconnected(ConnectionId, NodeId),
    PeerConnectFailed(NodeId, ConnectionManagerError),
    PeerInboundConnectFailed(ConnectionManagerError),

    // Substreams
    NewInboundSubstream(NodeId, ProtocolId, Substream),
}

impl fmt::Display for ConnectionManagerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        use ConnectionManagerEvent::*;
        match self {
            PeerConnected(conn) => write!(f, "PeerConnected({})", conn),
            PeerDisconnected(id, node_id) => write!(f, "PeerDisconnected({}, {})", id, node_id.short_str()),
            PeerConnectFailed(node_id, err) => write!(f, "PeerConnectFailed({}, {:?})", node_id.short_str(), err),
            PeerInboundConnectFailed(err) => write!(f, "PeerInboundConnectFailed({:?})", err),
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
    /// attempting to connect will have to wait for another connection attempt to complete. Default: 100
    pub max_simultaneous_inbound_connects: usize,
    /// Set to true to allow peers to send loopback, local-link and other addresses normally not considered valid for
    /// peer-to-peer comms. Default: false
    pub allow_test_addresses: bool,
    /// Version information for this node
    pub network_info: NodeNetworkInfo,
    /// The maximum time to wait for the first byte before closing the connection. Default: 45s
    pub time_to_first_byte: Duration,
    /// The number of liveness check sessions to allow. Default: 0
    pub liveness_max_sessions: usize,
    /// CIDR blocks that allowlist liveness checks. Default: Localhost only (127.0.0.1/32)
    pub liveness_cidr_allowlist: Vec<cidr::AnyIpCidr>,
    /// If set, an additional TCP-only p2p listener will be started. This is useful for local wallet connections.
    /// Default: None (disabled)
    pub auxilary_tcp_listener_address: Option<Multiaddr>,
}

impl Default for ConnectionManagerConfig {
    fn default() -> Self {
        Self {
            #[cfg(not(test))]
            listener_address: "/ip4/0.0.0.0/tcp/7898"
                .parse()
                .expect("DEFAULT_LISTENER_ADDRESS is malformed"),
            #[cfg(test)]
            listener_address: "/memory/0".parse().unwrap(),
            max_dial_attempts: 1,
            max_simultaneous_inbound_connects: 100,
            network_info: Default::default(),
            #[cfg(not(test))]
            allow_test_addresses: false,
            // This must always be true for internal crate tests
            #[cfg(test)]
            allow_test_addresses: true,
            liveness_max_sessions: 0,
            time_to_first_byte: Duration::from_secs(45),
            liveness_cidr_allowlist: vec![cidr::AnyIpCidr::V4("127.0.0.1/32".parse().unwrap())],
            auxilary_tcp_listener_address: None,
        }
    }
}

/// Container struct for the listener addresses
#[derive(Debug, Clone)]
pub struct ListenerInfo {
    bind_address: Multiaddr,
    aux_bind_address: Option<Multiaddr>,
}

impl ListenerInfo {
    pub fn bind_address(&self) -> &Multiaddr {
        &self.bind_address
    }

    pub fn auxilary_bind_address(&self) -> Option<&Multiaddr> {
        self.aux_bind_address.as_ref()
    }
}

pub struct ConnectionManager<TTransport, TBackoff> {
    request_rx: mpsc::Receiver<ConnectionManagerRequest>,
    internal_event_rx: mpsc::Receiver<ConnectionManagerEvent>,
    dialer_tx: mpsc::Sender<DialerRequest>,
    dialer: Option<Dialer<TTransport, TBackoff>>,
    listener: Option<PeerListener<TTransport>>,
    aux_listener: Option<PeerListener<TcpTransport>>,
    peer_manager: Arc<PeerManager>,
    shutdown_signal: Option<ShutdownSignal>,
    protocols: Protocols<Substream>,
    listener_info: Option<ListenerInfo>,
    listening_notifiers: Vec<oneshot::Sender<ListenerInfo>>,
    connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    complete_trigger: Shutdown,
}

impl<TTransport, TBackoff> ConnectionManager<TTransport, TBackoff>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TBackoff: Backoff + Send + Sync + 'static,
{
    pub fn new(
        mut config: ConnectionManagerConfig,
        transport: TTransport,
        noise_config: NoiseConfig,
        backoff: TBackoff,
        request_rx: mpsc::Receiver<ConnectionManagerRequest>,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        let (internal_event_tx, internal_event_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let (dialer_tx, dialer_rx) = mpsc::channel(DIALER_REQUEST_CHANNEL_SIZE);

        let listener = PeerListener::new(
            config.clone(),
            config.listener_address.clone(),
            transport.clone(),
            noise_config.clone(),
            internal_event_tx.clone(),
            peer_manager.clone(),
            node_identity.clone(),
            shutdown_signal.clone(),
        );

        let aux_listener = config.auxilary_tcp_listener_address.take().map(|addr| {
            PeerListener::new(
                config.clone(),
                addr,
                TcpTransport::new(),
                noise_config.clone(),
                internal_event_tx.clone(),
                peer_manager.clone(),
                node_identity.clone(),
                shutdown_signal.clone(),
            )
        });

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
            request_rx,
            peer_manager,
            protocols: Protocols::new(),
            internal_event_rx,
            dialer_tx,
            dialer: Some(dialer),
            listener: Some(listener),
            listener_info: None,
            aux_listener,
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
        let span = span!(Level::DEBUG, "comms::connection_manager::run");
        let _enter = span.enter();
        let mut shutdown = self
            .shutdown_signal
            .take()
            .expect("ConnectionManager initialized without a shutdown");

        // Runs the listeners. Sockets are bound and ready once this resolves
        match self.run_listeners().await {
            Ok(info) => {
                self.listener_info = Some(info);
            },
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "Failed to start listener(s). {}. Connection manager is quitting.", err
                );
                return;
            },
        };
        self.run_dialer();
        // Notify any awaiting tasks that the listener(s) are ready to receive connections
        self.notify_all_ready();

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
            tokio::select! {
                Some(event) = self.internal_event_rx.recv() => {
                    self.handle_event(event).await;
                },

                Some(request) = self.request_rx.recv() => {
                    self.handle_request(request).await;
                },

                _ = &mut shutdown => {
                    info!(target: LOG_TARGET, "ConnectionManager is shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
    }

    async fn run_listeners(&mut self) -> Result<ListenerInfo, ConnectionManagerError> {
        let mut listener = self
            .listener
            .take()
            .expect("ConnectionManager initialized without a listener");

        listener.set_supported_protocols(self.protocols.get_supported_protocols());

        let mut listener_info = ListenerInfo {
            bind_address: Multiaddr::empty(),
            aux_bind_address: None,
        };
        match listener.listen().await {
            Ok(addr) => {
                listener_info.bind_address = addr;
            },
            Err(err) => return Err(err),
        }

        if let Some(mut listener) = self.aux_listener.take() {
            listener.set_supported_protocols(self.protocols.get_supported_protocols());
            let addr = listener.listen().await?;
            debug!(target: LOG_TARGET, "TCP listener bound to address {}", addr);
            listener_info.aux_bind_address = Some(addr);
        }

        Ok(listener_info)
    }

    fn run_dialer(&mut self) {
        let mut dialer = self
            .dialer
            .take()
            .expect("ConnectionManager initialized without a dialer");

        dialer.set_supported_protocols(self.protocols.get_supported_protocols());
        dialer.spawn();
    }

    async fn handle_request(&mut self, request: ConnectionManagerRequest) {
        use ConnectionManagerRequest::*;
        trace!(target: LOG_TARGET, "Connection manager got request: {:?}", request);
        match request {
            DialPeer { node_id, reply_tx } => {
                let tracing_id = tracing::Span::current().id();
                let span = span!(Level::TRACE, "connection_manager::handle_request");
                span.follows_from(tracing_id);
                self.dial_peer(node_id, reply_tx).instrument(span).await
            },
            CancelDial(node_id) => {
                if let Err(err) = self.dialer_tx.send(DialerRequest::CancelPendingDial(node_id)).await {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send cancel dial request to dialer: {}", err
                    );
                }
            },
            NotifyListening(reply) => match self.listener_info.as_ref() {
                Some(info) => {
                    let _ = reply.send(info.clone());
                },
                None => {
                    self.listening_notifiers.push(reply);
                },
            },
        }
    }

    fn notify_all_ready(&mut self) {
        let info = self
            .listener_info
            .as_ref()
            .expect("notify_all_ready called before listeners were successfully bound");
        for notifier in self.listening_notifiers.drain(..) {
            let _ = notifier.send(info.clone());
        }
    }

    async fn handle_event(&mut self, event: ConnectionManagerEvent) {
        use ConnectionManagerEvent::*;

        match event {
            NewInboundSubstream(node_id, protocol, stream) => {
                let proto_str = String::from_utf8_lossy(&protocol);
                debug!(
                    target: LOG_TARGET,
                    "New inbound substream for peer '{}' speaking protocol '{}'",
                    node_id.short_str(),
                    proto_str
                );
                metrics::inbound_substream_counter(&node_id, &protocol).inc();
                let notify_fut = self
                    .protocols
                    .notify(&protocol, ProtocolEvent::NewInboundSubstream(node_id, stream));
                match time::timeout(Duration::from_secs(10), notify_fut).await {
                    Ok(Ok(_)) => {
                        debug!(target: LOG_TARGET, "Protocol notification for '{}' sent", proto_str);
                    },
                    Ok(Err(err)) => {
                        error!(
                            target: LOG_TARGET,
                            "Error sending NewSubstream notification for protocol '{}' because '{:?}'", proto_str, err
                        );
                    },
                    Err(err) => {
                        error!(
                            target: LOG_TARGET,
                            "Error sending NewSubstream notification for protocol '{}' because {}", proto_str, err
                        );
                    },
                }
            },

            PeerConnected(conn) => {
                if conn.direction().is_inbound() {
                    // Notify the dialer that we have an inbound connection, so that is can resolve any pending dials.
                    let _ = self
                        .dialer_tx
                        .send(DialerRequest::NotifyNewInboundConnection(conn.clone()))
                        .await;
                }
                metrics::successful_connections(conn.peer_node_id(), conn.direction()).inc();
                self.publish_event(PeerConnected(conn));
            },
            PeerConnectFailed(peer, err) => {
                metrics::failed_connections(&peer, ConnectionDirection::Outbound).inc();
                self.publish_event(PeerConnectFailed(peer, err));
            },
            PeerInboundConnectFailed(err) => {
                metrics::failed_connections(&Default::default(), ConnectionDirection::Inbound).inc();
                self.publish_event(PeerInboundConnectFailed(err));
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

    #[tracing::instrument(level = "trace", skip(self, reply))]
    async fn dial_peer(
        &mut self,
        node_id: NodeId,
        reply: Option<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>,
    ) {
        match self.peer_manager.find_by_node_id(&node_id).await {
            Ok(Some(peer)) => {
                self.send_dialer_request(DialerRequest::Dial(Box::new(peer), reply))
                    .await;
            },
            Ok(None) => {
                warn!(target: LOG_TARGET, "Peer not found for dial");
                if let Some(reply) = reply {
                    let _ = reply.send(Err(ConnectionManagerError::PeerManagerError(
                        PeerManagerError::PeerNotFoundError,
                    )));
                }
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "Failed to fetch peer to dial because '{}'", err);
                if let Some(reply) = reply {
                    let _ = reply.send(Err(ConnectionManagerError::PeerManagerError(err)));
                }
            },
        }
    }
}

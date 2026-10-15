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
use tari_shutdown::{
    ShutdownSignal,
    oneshot_trigger::{OneshotSignal, OneshotTrigger},
};
use time::Duration;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc, oneshot},
    task,
    time,
};
use tracing::{Instrument, Level, span};

use super::{
    dialer::{Dialer, DialerRequest},
    error::ConnectionManagerError,
    listener::{ListenerSharedState, PeerListener, effective_liveness_sessions},
    peer_connection::PeerConnection,
    requester::ConnectionManagerRequest,
    self_liveness::LivenessTokens,
};
#[cfg(feature = "metrics")]
use crate::connection_manager::ConnectionDirection;
#[cfg(feature = "metrics")]
use crate::connection_manager::metrics;
use crate::{
    Minimized,
    PeerManager,
    backoff::Backoff,
    connection_manager::ConnectionId,
    multiplexing::Substream,
    net_address::MultiaddrRange,
    noise::NoiseConfig,
    peer_manager::{NodeId, NodeIdentity, PEER_LOOKUP_TIMEOUT, PeerManagerError},
    peer_validator::PeerValidatorConfig,
    protocol::{NodeNetworkInfo, ProtocolError, ProtocolEvent, ProtocolId, Protocols},
    transports::{TcpTransport, Transport},
};

const LOG_TARGET: &str = "comms::connection_manager::manager";

const EVENT_CHANNEL_SIZE: usize = 32;
const DIALER_REQUEST_CHANNEL_SIZE: usize = 60;

/// Maximum time the ConnectionManager will wait for space on the dialer's request channel before it
/// gives up and sheds the request.
///
/// Mirrors `CONN_MAN_NOTIFY_TIMEOUT` on the dialer side: neither actor may park indefinitely on the
/// other's bounded channel. A burst of dial requests travelling down meeting a burst of dial results
/// travelling up will otherwise fill both channels at once and deadlock the pair permanently.
const DIALER_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Connection events
#[derive(Debug)]
pub enum ConnectionManagerEvent {
    // Peer connection
    PeerConnected(Box<PeerConnection>),
    PeerDisconnected(ConnectionId, NodeId, Minimized),
    PeerConnectFailed(NodeId, ConnectionManagerError),
    PeerInboundConnectFailed(ConnectionManagerError),

    // Substreams
    NewInboundSubstream(NodeId, ProtocolId, Substream),

    // Other
    PeerViolation { peer_node_id: NodeId, details: String },
}

impl fmt::Display for ConnectionManagerEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        #[allow(clippy::enum_glob_use)]
        use ConnectionManagerEvent::*;
        match self {
            PeerConnected(conn) => write!(f, "PeerConnected({conn})"),
            PeerDisconnected(id, node_id, minimized) => {
                write!(f, "PeerDisconnected({}, {}, {:?})", id, node_id.short_str(), minimized)
            },
            PeerConnectFailed(node_id, err) => write!(f, "PeerConnectFailed({}, {:?})", node_id.short_str(), err),
            PeerInboundConnectFailed(err) => write!(f, "PeerInboundConnectFailed({err:?})",),
            NewInboundSubstream(node_id, protocol, _) => write!(
                f,
                "NewInboundSubstream({}, {}, Stream)",
                node_id.short_str(),
                String::from_utf8_lossy(protocol)
            ),
            PeerViolation { peer_node_id, details } => {
                write!(f, "PeerViolation({}, {})", peer_node_id.short_str(), details)
            },
        }
    }
}

/// Configuration for ConnectionManager
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
    /// Version information for this node
    pub network_info: NodeNetworkInfo,
    /// The maximum time to wait for the first byte before closing the connection. Default: 6s
    pub time_to_first_byte: Duration,
    /// The maximum time to wait for a noise protocol handshake message before timing out. For 1.5 RTT XX handshake,
    /// the responder will wait 2 x this value (1 per receive) before timing out.
    /// Default: 6s
    pub noise_handshake_recv_timeout: Duration,
    /// The maximum time to wait for a peer to respond on a noise protocol dial.
    /// Default: 60s
    pub noise_dial_timeout: Duration,
    /// The number of liveness check sessions to allow, across all listeners of the node. Capped at half of
    /// `max_simultaneous_inbound_connects`, since each session holds a handshake slot while open. The node's own
    /// self-check does not need a session. Default: 1
    pub liveness_max_sessions: usize,
    /// CIDR blocks that allowlist liveness checks. Default: Localhost only (127.0.0.1/32)
    pub liveness_cidr_allowlist: Vec<cidr::AnyIpCidr>,
    /// Interval to perform self-liveness ping-pong tests. Default: None/disabled
    pub self_liveness_self_check_interval: Option<Duration>,
    /// If set, an additional TCP-only p2p listener will be started. This is useful for local wallet connections.
    /// Default: None (disabled)
    pub auxiliary_tcp_listener_address: Option<Multiaddr>,
    /// Peer validation configuration. See [PeerValidatorConfig]
    pub peer_validation_config: PeerValidatorConfig,
    /// Addresses that should never be dialed
    pub excluded_dial_addresses: Vec<MultiaddrRange>,
    /// The longest the unauthenticated stages of an inbound connection may take in total: the wire byte, noise and
    /// the identity exchange. Peer database calls and the multiplexer upgrade are not counted. The per-stage
    /// timeouts still apply inside this deadline. Must be non-zero. Default: 15s
    pub inbound_handshake_timeout: Duration,
    /// The maximum number of inbound handshakes in flight at once from a single source (an IPv4 address or an IPv6
    /// /64), counted per listener. Loopback sources (including Tor inbound forwarded over loopback), sources in
    /// `listener_per_ip_exempt_cidrs` and non-IP sources are exempt. Because loopback is always exempt, a clearnet
    /// node behind a local TCP proxy that forwards over loopback gets no per-source protection; prefer forwarding that
    /// preserves the source address. Must be non-zero. Default: 4
    pub max_inbound_handshakes_per_ip: usize,
    /// The maximum number of inbound connections, established and in-handshake combined. Inbound sockets over this
    /// limit are closed before any handshake. Outbound connections are not counted. Each listener applies this
    /// separately: connections to the auxiliary TCP listener are not counted against the main listener's limit, nor
    /// the other way round. Must be non-zero. Default: 500
    pub max_inbound_connections: usize,
    /// The maximum number of inbound connections, established and in-handshake combined, from a single source (an
    /// IPv4 address or an IPv6 /64), with the same exemptions as `max_inbound_handshakes_per_ip`. Counted per
    /// listener. Many hosts can share one egress address (VPN, Tor exit, CGNAT, SNAT), and one of them can hold all of
    /// that address's slots and lock the others out; `listener_per_ip_exempt_cidrs` is the escape hatch for proxies
    /// and SNAT you control. Loopback sources are always exempt, so a clearnet node behind a proxy on the same host
    /// that forwards over loopback (nginx or haproxy stream, socat) gets no per-source protection. Behind docker's
    /// userland-proxy (or any SNAT, NAT64 or Teredo translator) all peers appear to come from one non-loopback address
    /// (for docker, the bridge gateway, e.g. 172.17.0.1) and share its limit; add that address to
    /// `listener_per_ip_exempt_cidrs`. Prefer forwarding that preserves the source address. Must be non-zero.
    /// Default: 32
    pub max_inbound_connections_per_ip: usize,
    /// Sources exempt from the per-source inbound limits, in addition to loopback. If Tor (or another proxy, or SNAT)
    /// forwards inbound connections to this node from a non-loopback address, every such connection appears to come
    /// from that address; add it here or the per-source limits will throttle all of them. IPv4-mapped IPv6 CIDRs are
    /// treated as their IPv4 form. Default: empty
    pub listener_per_ip_exempt_cidrs: Vec<cidr::AnyIpCidr>,
}

impl ConnectionManagerConfig {
    /// Rejects inbound listener limits that would refuse every inbound connection.
    pub fn validate(&self) -> Result<(), ConnectionManagerError> {
        if self.inbound_handshake_timeout.is_zero() {
            return Err(ConnectionManagerError::InvalidConfig(
                "inbound_handshake_timeout must be greater than zero".to_string(),
            ));
        }
        if self.max_inbound_connections == 0 {
            return Err(ConnectionManagerError::InvalidConfig(
                "max_inbound_connections must be greater than zero".to_string(),
            ));
        }
        if self.max_inbound_handshakes_per_ip == 0 {
            return Err(ConnectionManagerError::InvalidConfig(
                "max_inbound_handshakes_per_ip must be greater than zero".to_string(),
            ));
        }
        if self.max_inbound_connections_per_ip == 0 {
            return Err(ConnectionManagerError::InvalidConfig(
                "max_inbound_connections_per_ip must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
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
            liveness_max_sessions: 1,
            time_to_first_byte: Duration::from_secs(6),
            liveness_cidr_allowlist: vec![cidr::AnyIpCidr::V4("127.0.0.1/32".parse().unwrap())],
            self_liveness_self_check_interval: None,
            auxiliary_tcp_listener_address: None,
            peer_validation_config: PeerValidatorConfig::default(),
            noise_handshake_recv_timeout: Duration::from_secs(6),
            noise_dial_timeout: Duration::from_secs(60),
            excluded_dial_addresses: vec![],
            inbound_handshake_timeout: Duration::from_secs(15),
            max_inbound_handshakes_per_ip: 4,
            max_inbound_connections: 500,
            max_inbound_connections_per_ip: 32,
            listener_per_ip_exempt_cidrs: vec![],
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
    /// The address that was bound on. In the case of TCP, if the OS has decided which port to bind on (0.0.0.0:0), this
    /// address contains the actual port that was used.
    pub fn bind_address(&self) -> &Multiaddr {
        &self.bind_address
    }

    /// The auxiliary TCP address that was bound on if enabled.
    pub fn auxiliary_bind_address(&self) -> Option<&Multiaddr> {
        self.aux_bind_address.as_ref()
    }
}

/// The actor responsible for connection management.
pub(crate) struct ConnectionManager<TTransport, TBackoff> {
    request_rx: mpsc::Receiver<ConnectionManagerRequest>,
    internal_event_rx: mpsc::Receiver<ConnectionManagerEvent>,
    dialer_tx: mpsc::Sender<DialerRequest>,
    dialer: Option<Dialer<TTransport, TBackoff>>,
    listener: Option<PeerListener<TTransport>>,
    aux_listener: Option<PeerListener<TcpTransport>>,
    listener_shared_state: ListenerSharedState,
    peer_manager: Arc<PeerManager>,
    shutdown_signal: Option<ShutdownSignal>,
    protocols: Protocols<Substream>,
    listener_info: Option<ListenerInfo>,
    listening_notifiers: Vec<oneshot::Sender<ListenerInfo>>,
    connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
    /// Never broadcast: dropped when this service exits, which resolves `complete_signal` with `None`.
    complete_trigger: OneshotTrigger<()>,
}

impl<TTransport, TBackoff> ConnectionManager<TTransport, TBackoff>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TBackoff: Backoff + Send + Sync + 'static,
{
    pub(crate) fn new(
        mut config: ConnectionManagerConfig,
        transport: TTransport,
        backoff: TBackoff,
        request_rx: mpsc::Receiver<ConnectionManagerRequest>,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        let (internal_event_tx, internal_event_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let (dialer_tx, dialer_rx) = mpsc::channel(DIALER_REQUEST_CHANNEL_SIZE);

        let noise_config = NoiseConfig::new(node_identity.clone())
            .with_recv_timeout(config.noise_handshake_recv_timeout)
            .with_dial_timeout(config.noise_dial_timeout);

        // Both listeners share the self-check tokens and the liveness session permits node-wide
        let liveness_sessions = effective_liveness_sessions(&config);
        if liveness_sessions < config.liveness_max_sessions {
            warn!(
                target: LOG_TARGET,
                "liveness_max_sessions ({}) is limited to {} (half of max_simultaneous_inbound_connects) so that \
                 liveness sessions cannot hold every inbound handshake slot",
                config.liveness_max_sessions,
                liveness_sessions
            );
        }
        let listener_shared_state = ListenerSharedState::new(&config);
        let mut listener = PeerListener::new(
            config.clone(),
            config.listener_address.clone(),
            transport.clone(),
            noise_config.clone(),
            internal_event_tx.clone(),
            peer_manager.clone(),
            node_identity.clone(),
            shutdown_signal.clone(),
        );
        listener.set_shared_state(listener_shared_state.clone());

        let aux_listener = config.auxiliary_tcp_listener_address.take().map(|addr| {
            info!(target: LOG_TARGET, "Starting auxiliary listener on {addr}");
            let aux_config = ConnectionManagerConfig {
                // Disable liveness checks on the auxiliary listener
                self_liveness_self_check_interval: None,
                ..config.clone()
            };
            let mut aux_listener = PeerListener::new(
                aux_config,
                addr,
                TcpTransport::new(),
                noise_config.clone(),
                internal_event_tx.clone(),
                peer_manager.clone(),
                node_identity.clone(),
                shutdown_signal.clone(),
            );
            // The auxiliary listener shares the liveness state but keeps its own inbound counts, so it is neither
            // refused by, nor counted against, the public listener's inbound limits
            aux_listener.set_shared_state(listener_shared_state.with_own_counts());
            aux_listener
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
            listener_shared_state,
            listening_notifiers: Vec::new(),
            connection_manager_events_tx,
            complete_trigger: OneshotTrigger::new(),
        }
    }

    pub fn add_protocols(&mut self, protocols: Protocols<Substream>) -> &mut Self {
        self.protocols.extend(protocols);
        self
    }

    /// The one-time tokens the listeners accept for this node's own liveness self-check. Pass these to
    /// `SelfLivenessCheck` so its connections are recognised.
    pub fn liveness_tokens(&self) -> LivenessTokens {
        self.listener_shared_state.liveness_tokens.clone()
    }

    pub fn complete_signal(&self) -> OneshotSignal<()> {
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
                    "Failed to start listener(s). {err}. Connection manager is quitting."
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

        let mut listener_info = match listener.listen().await {
            Ok(bind_address) => ListenerInfo {
                bind_address,
                aux_bind_address: None,
            },
            Err(err) => return Err(err),
        };

        if let Some(mut listener) = self.aux_listener.take() {
            listener.set_supported_protocols(self.protocols.get_supported_protocols());
            let addr = listener.listen().await?;
            debug!(target: LOG_TARGET, "Aux TCP listener bound to address {addr}");
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
        use ConnectionManagerRequest::{CancelDial, DialPeer, NotifyListening};
        trace!(target: LOG_TARGET, "Connection manager got request: {request:?}" );
        match request {
            DialPeer { node_id, reply_tx } => {
                let tracing_id = tracing::Span::current().id();
                let span = span!(Level::TRACE, "connection_manager::handle_request");
                span.follows_from(tracing_id);
                self.dial_peer(node_id, reply_tx).instrument(span).await
            },
            CancelDial(node_id) => {
                let _result = self
                    .send_dialer_request(DialerRequest::CancelPendingDial(node_id))
                    .await;
            },
            NotifyListening(reply) => match self.listener_info.as_ref() {
                Some(info) => {
                    let _result = reply.send(info.clone());
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
            let _result = notifier.send(info.clone());
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
                #[cfg(feature = "metrics")]
                metrics::inbound_substream_counter(&protocol).inc();
                // Waits at most a second for the protocol to take the substream, then drops it (yamux resets it, which
                // the remote sees as a failed negotiation). This loop serves every peer, so one stalled protocol must
                // not hold it up for longer than that.
                let result = self
                    .protocols
                    .notify(&protocol, ProtocolEvent::NewInboundSubstream(node_id.clone(), stream))
                    .await;
                match result {
                    Ok(()) => {
                        debug!(target: LOG_TARGET, "Protocol notification for '{proto_str}' sent" );
                    },
                    // Already warned about (rate limited, without the peer) and counted where it was dropped
                    Err(ProtocolError::NotificationDropped) => {
                        debug!(
                            target: LOG_TARGET,
                            "Dropped an inbound substream from peer `{node_id}` because protocol '{proto_str}' did not \
                             take it in time"
                        );
                    },
                    Err(err) => {
                        error!(
                            target: LOG_TARGET,
                            "Error sending NewSubstream notification for protocol '{proto_str}' because '{err:?}'"
                        );
                    },
                }
            },

            PeerConnected(conn) => {
                if conn.direction().is_inbound() {
                    // Notify the dialer that we have an inbound connection, so that is can resolve any pending dials.
                    let _result = self
                        .send_dialer_request(DialerRequest::NotifyNewInboundConnection(conn.clone()))
                        .await;
                }
                #[cfg(feature = "metrics")]
                metrics::successful_connections(conn.direction()).inc();
                self.publish_event(PeerConnected(conn));
            },
            PeerConnectFailed(peer, err) => {
                #[cfg(feature = "metrics")]
                metrics::failed_connections(ConnectionDirection::Outbound).inc();
                self.publish_event(PeerConnectFailed(peer, err));
            },
            PeerInboundConnectFailed(err) => {
                #[cfg(feature = "metrics")]
                metrics::failed_connections(ConnectionDirection::Inbound).inc();
                self.publish_event(PeerInboundConnectFailed(err));
            },
            event => {
                self.publish_event(event);
            },
        }
    }

    /// Hand a request to the dialer without parking the ConnectionManager indefinitely.
    ///
    /// See [`DIALER_REQUEST_TIMEOUT`]. On failure the request — and with it any reply channel it
    /// carries — is dropped, so the caller observes a cancelled reply rather than hanging forever.
    #[inline]
    async fn send_dialer_request(&mut self, req: DialerRequest) -> Result<(), ConnectionManagerError> {
        let req = match self.dialer_tx.try_send(req) {
            Ok(_) => return Ok(()),
            Err(mpsc::error::TrySendError::Full(req)) => {
                warn!(
                    target: LOG_TARGET,
                    "Dialer request channel is full. Waiting up to {DIALER_REQUEST_TIMEOUT:.0?} to send {req:?}"
                );
                req
            },
            Err(mpsc::error::TrySendError::Closed(_)) => {
                error!(
                    target: LOG_TARGET,
                    "Failed to send request to dialer because the channel is closed"
                );
                return Err(ConnectionManagerError::SendToActorFailed);
            },
        };

        self.dialer_tx
            .send_timeout(req, DIALER_REQUEST_TIMEOUT)
            .await
            .map_err(|err| {
                error!(
                    target: LOG_TARGET,
                    "Shed dialer request after waiting {DIALER_REQUEST_TIMEOUT:.0?}: {err}. The dialer is not draining \
                     its request channel."
                );
                ConnectionManagerError::DialQueueFull
            })
    }

    fn publish_event(&self, event: ConnectionManagerEvent) {
        // Error on no subscribers can be ignored
        let _result = self.connection_manager_events_tx.send(Arc::new(event));
    }

    async fn dial_peer(
        &mut self,
        node_id: NodeId,
        reply: Option<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>,
    ) {
        // Shed rather than block. This lookup is the first thing the ConnectionManager's request loop
        // does for a dial, ahead of any log line or timeout guard, so a slow peer database used to
        // stall the whole actor here. The query itself runs on the blocking pool and completes
        // regardless of this timeout; we simply stop waiting for it.
        let lookup = time::timeout(PEER_LOOKUP_TIMEOUT, self.peer_manager.find_by_node_id(&node_id)).await;
        let lookup = match lookup {
            Ok(result) => result,
            Err(_) => {
                warn!(
                    target: LOG_TARGET,
                    "Peer database lookup for dial to '{}' exceeded {PEER_LOOKUP_TIMEOUT:.0?}. Shedding the dial \
                     rather than stalling the connection manager.",
                    node_id.short_str()
                );
                if let Some(reply) = reply {
                    let _result = reply.send(Err(ConnectionManagerError::PeerLookupTimeout));
                }
                return;
            },
        };
        match lookup {
            Ok(Some(peer)) => {
                // The reply channel travels inside the request, so a shed request drops it and the
                // caller sees a cancelled reply — never an indefinite wait.
                let _result = self
                    .send_dialer_request(DialerRequest::Dial(Box::new(peer), reply))
                    .await;
            },
            Ok(None) => {
                warn!(target: LOG_TARGET, "Peer not found for dial");
                if let Some(reply) = reply {
                    let _result = reply.send(Err(ConnectionManagerError::PeerManagerError(
                        PeerManagerError::peer_not_found(&node_id),
                    )));
                }
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "Failed to fetch peer to dial because '{err}'");
                if let Some(reply) = reply {
                    let _result = reply.send(Err(ConnectionManagerError::PeerManagerError(err)));
                }
            },
        }
    }
}

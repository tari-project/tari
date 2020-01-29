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

use super::dialer::DialerRequest;
use crate::{
    backoff::Backoff,
    connection::ConnectionDirection,
    connection_manager::{
        dialer::Dialer,
        error::ConnectionManagerError,
        listener::PeerListener,
        peer_connection::PeerConnection,
        requester::ConnectionManagerRequest,
    },
    noise::NoiseConfig,
    peer_manager::{AsyncPeerManager, NodeId, NodeIdentity},
    protocol::{ProtocolEvent, ProtocolId, ProtocolNotifier},
    transports::Transport,
    types::DEFAULT_LISTENER_ADDRESS,
};
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    AsyncRead,
    AsyncWrite,
    StreamExt,
};
use log::*;
use multiaddr::Multiaddr;
use std::{collections::HashMap, sync::Arc};
use tari_shutdown::ShutdownSignal;
use tokio::{runtime, sync::broadcast};

const LOG_TARGET: &str = "comms::connection_manager::manager";

const EVENT_CHANNEL_SIZE: usize = 32;
const ESTABLISHER_CHANNEL_SIZE: usize = 32;

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
    NewInboundSubstream(Box<NodeId>, ProtocolId, yamux::Stream),
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
}

impl Default for ConnectionManagerConfig {
    fn default() -> Self {
        Self {
            listener_address: DEFAULT_LISTENER_ADDRESS
                .parse()
                .expect("DEFAULT_LISTENER_ADDRESS is malformed"),
            max_dial_attempts: 3,
            max_simultaneous_inbound_connects: 20,
        }
    }
}

pub struct ConnectionManager<TTransport, TBackoff> {
    executor: runtime::Handle,
    request_rx: Fuse<mpsc::Receiver<ConnectionManagerRequest>>,
    internal_event_rx: Fuse<mpsc::Receiver<ConnectionManagerEvent>>,
    dialer_tx: mpsc::Sender<DialerRequest>,
    dialer: Option<Dialer<TTransport, TBackoff>>,
    listener: Option<PeerListener<TTransport>>,
    peer_manager: AsyncPeerManager,
    node_identity: Arc<NodeIdentity>,
    active_connections: HashMap<NodeId, PeerConnection>,
    shutdown_signal: Option<ShutdownSignal>,
    protocol_notifier: ProtocolNotifier<yamux::Stream>,
    listener_address: Option<Multiaddr>,
    listening_notifiers: Vec<oneshot::Sender<Multiaddr>>,
    connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
}

impl<TTransport, TBackoff> ConnectionManager<TTransport, TBackoff>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TBackoff: Backoff + Send + Sync + 'static,
{
    pub fn new(
        config: ConnectionManagerConfig,
        executor: runtime::Handle,
        transport: TTransport,
        noise_config: NoiseConfig,
        backoff: Arc<TBackoff>,
        request_rx: mpsc::Receiver<ConnectionManagerRequest>,
        node_identity: Arc<NodeIdentity>,
        peer_manager: AsyncPeerManager,
        protocol_notifier: ProtocolNotifier<yamux::Stream>,
        connection_manager_events_tx: broadcast::Sender<Arc<ConnectionManagerEvent>>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        let (internal_event_tx, internal_event_rx) = mpsc::channel(EVENT_CHANNEL_SIZE);

        let (establisher_tx, establisher_rx) = mpsc::channel(ESTABLISHER_CHANNEL_SIZE);

        let supported_protocols = protocol_notifier.get_supported_protocols();

        let listener = PeerListener::new(
            executor.clone(),
            config.clone(),
            transport.clone(),
            noise_config.clone(),
            internal_event_tx.clone(),
            peer_manager.clone(),
            Arc::clone(&node_identity),
            supported_protocols.clone(),
            shutdown_signal.clone(),
        );

        let dialer = Dialer::new(
            executor.clone(),
            config,
            Arc::clone(&node_identity),
            peer_manager.clone(),
            transport,
            noise_config,
            backoff,
            establisher_rx,
            internal_event_tx,
            supported_protocols,
            shutdown_signal.clone(),
        );

        Self {
            executor,
            shutdown_signal: Some(shutdown_signal),
            request_rx: request_rx.fuse(),
            node_identity,
            peer_manager,
            protocol_notifier,
            internal_event_rx: internal_event_rx.fuse(),
            dialer_tx: establisher_tx,
            dialer: Some(dialer),
            listener: Some(listener),
            active_connections: Default::default(),
            listener_address: None,
            listening_notifiers: Vec::new(),
            connection_manager_events_tx,
        }
    }

    pub async fn run(mut self) {
        let mut shutdown = self
            .shutdown_signal
            .take()
            .expect("ConnectionManager initialized without a shutdown");

        self.run_listener();
        self.run_dialer();

        debug!(target: LOG_TARGET, "Connection manager started");
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
                    self.disconnect_all().await;
                    break;
                }
            }
        }
    }

    async fn disconnect_all(&mut self) {
        let mut node_ids = Vec::with_capacity(self.active_connections.len());
        for (node_id, mut conn) in self.active_connections.drain() {
            if log_if_error!(
                target: LOG_TARGET,
                conn.disconnect_silent().await,
                "Failed to disconnect because '{error}'",
            )
            .is_some()
            {
                node_ids.push(node_id);
            }
        }

        for node_id in node_ids {
            self.publish_event(ConnectionManagerEvent::PeerDisconnected(Box::new(node_id)));
        }
    }

    fn run_listener(&mut self) {
        let listener = self
            .listener
            .take()
            .expect("ConnnectionManager initialized without a Listener");

        self.executor.spawn(listener.run());
    }

    fn run_dialer(&mut self) {
        let dialer = self
            .dialer
            .take()
            .expect("ConnnectionManager initialized without an Establisher");

        self.executor.spawn(dialer.run());
    }

    async fn handle_request(&mut self, request: ConnectionManagerRequest) {
        use ConnectionManagerRequest::*;
        match request {
            DialPeer(node_id, reply_tx) => match self.get_active_connection(&node_id) {
                Some(conn) => {
                    log_if_error_fmt!(
                        target: LOG_TARGET,
                        reply_tx.send(Ok(conn.clone())),
                        "Failed to send reply for dial request for peer '{}'",
                        node_id.short_str()
                    );
                },
                None => self.dial_peer(node_id, reply_tx).await,
            },
            NotifyListening(reply_tx) => match self.listener_address.as_ref() {
                Some(addr) => {
                    let _ = reply_tx.send(addr.clone());
                },
                None => {
                    self.listening_notifiers.push(reply_tx);
                },
            },
            GetActiveConnection(node_id, reply_tx) => {
                let _ = reply_tx.send(self.active_connections.get(&node_id).map(Clone::clone));
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
                debug!(
                    target: LOG_TARGET,
                    "New inbound substream for peer '{}'",
                    node_id.short_str()
                );
                log_if_error!(
                    target: LOG_TARGET,
                    self.protocol_notifier
                        .notify(&protocol, ProtocolEvent::NewInboundSubstream(node_id, stream))
                        .await,
                    "Error sending NewSubstream notification because '{error}'",
                );
            },
            PeerConnected(mut new_conn) => {
                let node_id = new_conn.peer_node_id().clone();
                match self.active_connections.remove(&node_id) {
                    Some(mut existing_conn) => {
                        if self.tie_break_existing_connection(&existing_conn, &new_conn) {
                            log_if_error!(
                                target: LOG_TARGET,
                                existing_conn.disconnect_silent().await,
                                "Failed to disconnect (tie break) existing connection because '{error}'",
                            );
                            debug!(
                                "Disconnecting existing connection to Peer {} because of simultaneous dial",
                                existing_conn.peer_node_id()
                            );

                            self.publish_event(ConnectionManagerEvent::PeerConnected(new_conn.clone()));
                            self.active_connections.insert(node_id, new_conn);
                        } else {
                            log_if_error!(
                                target: LOG_TARGET,
                                new_conn.disconnect_silent().await,
                                "Failed to disconnect (tie break) new connection because '{error}'",
                            );
                            debug!(
                                "Disconnecting new connection to Peer {} because of simultaneous dial",
                                new_conn.peer_node_id()
                            );
                            self.active_connections.insert(node_id, existing_conn);
                        }
                    },
                    None => {
                        self.publish_event(ConnectionManagerEvent::PeerConnected(new_conn.clone()));
                        self.active_connections.insert(node_id, new_conn);
                    },
                }
            },
            PeerDisconnected(node_id) => {
                if self.active_connections.remove(&node_id).is_some() {
                    self.publish_event(ConnectionManagerEvent::PeerDisconnected(node_id));
                }
            },
            _ => {},
        }
    }

    /// Two connections to the same peer have been created. This function deterministically determines which peer
    /// connection to close. It does this by comparing our NodeId to that of the peer. This rule enables both sides to
    /// agree which connection to disconnect
    ///
    /// Returns true if the existing connection should close, otherwise false if the new connection should be closed.
    fn tie_break_existing_connection(&self, existing_conn: &PeerConnection, new_conn: &PeerConnection) -> bool {
        debug_assert_eq!(existing_conn.peer_node_id(), new_conn.peer_node_id());
        let peer_node_id = existing_conn.peer_node_id();
        let our_node_id = self.node_identity.node_id();

        use ConnectionDirection::*;
        match (existing_conn.direction(), new_conn.direction()) {
            // They connected to us twice for some reason. Drop the existing (older) connection
            (Inbound, Inbound) => true,
            // They connected to us at the same time we connected to them
            (Inbound, Outbound) => peer_node_id > our_node_id,
            // We connected to them at the same time as they connected to us
            (Outbound, Inbound) => our_node_id > peer_node_id,
            // We connected to them twice for some reason. Drop the newer connection.
            (Outbound, Outbound) => false,
        }
    }

    fn publish_event(&self, event: ConnectionManagerEvent) {
        log_if_error_fmt!(
            target: LOG_TARGET,
            self.connection_manager_events_tx.send(Arc::new(event)),
            "Failed to send ConnectionManagerEvent",
        );
    }

    #[inline]
    fn get_active_connection(&self, node_id: &NodeId) -> Option<&PeerConnection> {
        self.active_connections.get(node_id)
    }

    async fn dial_peer(
        &mut self,
        node_id: NodeId,
        reply_tx: oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
    )
    {
        match self.peer_manager.find_by_node_id(&node_id).await {
            Ok(peer) => {
                if let Err(err) = self.dialer_tx.try_send(DialerRequest::Dial(Box::new(peer), reply_tx)) {
                    error!(
                        target: LOG_TARGET,
                        "Failed to send request to establisher because '{}'", err
                    );

                    match err.into_inner() {
                        DialerRequest::Dial(_, reply_tx) => {
                            log_if_error_fmt!(
                                target: LOG_TARGET,
                                reply_tx.send(Err(ConnectionManagerError::EstablisherChannelError)),
                                "Failed to send dial peer result for peer '{}'",
                                node_id.short_str()
                            );
                        },
                    }
                }
            },
            Err(err) => {
                error!(target: LOG_TARGET, "Failed to fetch peer to dial because '{}'", err);
                log_if_error_fmt!(
                    level: warn,
                    target: LOG_TARGET,
                    reply_tx.send(Err(ConnectionManagerError::PeerManagerError(err))),
                    "Failed to send error reply when dialing peer '{}'",
                    node_id.short_str()
                );
            },
        }
    }
}

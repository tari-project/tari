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

use std::{
    fmt,
    future::Future,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use futures::{future::BoxFuture, stream::FuturesUnordered};
use log::*;
use multiaddr::Multiaddr;
use tari_shutdown::oneshot_trigger::OneshotTrigger;
use tokio::{
    sync::{mpsc, oneshot},
    time,
};
use tokio_stream::StreamExt;
use tracing::{span, Instrument, Level};

use super::{direction::ConnectionDirection, error::PeerConnectionError, manager::ConnectionManagerEvent};
#[cfg(feature = "rpc")]
use crate::protocol::rpc::{
    pool::RpcClientPool,
    pool::RpcPoolClient,
    NamedProtocolService,
    RpcClient,
    RpcClientBuilder,
    RpcError,
    RPC_MAX_FRAME_SIZE,
};
use crate::{
    framing,
    framing::CanonicalFraming,
    multiplexing::{Control, IncomingSubstreams, Substream, Yamux, YamuxControlError},
    peer_manager::{NodeId, PeerFeatures},
    protocol::{ProtocolId, ProtocolNegotiation},
    utils::atomic_ref_counter::AtomicRefCounter,
    Minimized,
};

const LOG_TARGET: &str = "comms::connection_manager::peer_connection";

const PROTOCOL_NEGOTIATION_TIMEOUT: Duration = Duration::from_secs(10);

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn create(
    connection: Yamux,
    peer_addr: Multiaddr,
    peer_node_id: NodeId,
    drop_old_connections: bool,
    peer_features: PeerFeatures,
    direction: ConnectionDirection,
    event_notifier: mpsc::Sender<ConnectionManagerEvent>,
    our_supported_protocols: Arc<Vec<ProtocolId>>,
    their_supported_protocols: Vec<ProtocolId>,
) -> PeerConnection {
    trace!(
        target: LOG_TARGET,
        "(Peer={}) Socket successfully upgraded to multiplexed socket",
        peer_node_id.short_str()
    );
    // All requests are request/response, so a channel size of 1 is all that is needed
    let (peer_tx, peer_rx) = mpsc::channel(1);
    let id = ID_COUNTER.fetch_add(1, Ordering::SeqCst); // Monotonic
    let substream_counter = connection.substream_counter();
    let peer_conn = PeerConnection::new(
        id,
        peer_tx,
        peer_node_id.clone(),
        drop_old_connections,
        peer_features,
        peer_addr,
        direction,
        substream_counter,
    );
    let peer_actor = PeerConnectionActor::new(
        id,
        peer_node_id,
        direction,
        connection,
        peer_rx,
        event_notifier,
        our_supported_protocols,
        their_supported_protocols,
    );
    tokio::spawn(peer_actor.run());

    peer_conn
}

/// Request types for the PeerConnection actor.
#[derive(Debug)]
pub enum PeerConnectionRequest {
    /// Open a new substream and negotiate the given protocol
    OpenSubstream {
        protocol_id: ProtocolId,
        reply_tx: oneshot::Sender<Result<NegotiatedSubstream<Substream>, PeerConnectionError>>,
    },
    /// Disconnect all substreams and close the transport connection
    Disconnect(bool, oneshot::Sender<Result<(), PeerConnectionError>>, Minimized),
}

/// ID type for peer connections
pub type ConnectionId = usize;

/// Request handle for an active peer connection
#[derive(Debug, Clone)]
pub struct PeerConnection {
    id: ConnectionId,
    peer_node_id: NodeId,
    drop_old_connections: bool,
    peer_features: PeerFeatures,
    request_tx: mpsc::Sender<PeerConnectionRequest>,
    address: Arc<Multiaddr>,
    direction: ConnectionDirection,
    started_at: Instant,
    substream_counter: AtomicRefCounter,
    handle_counter: Arc<()>,
    drop_notifier: OneshotTrigger<NodeId>,
    number_of_rpc_clients: Option<usize>,
}

impl PeerConnection {
    pub(crate) fn new(
        id: ConnectionId,
        request_tx: mpsc::Sender<PeerConnectionRequest>,
        peer_node_id: NodeId,
        drop_old_connections: bool,
        peer_features: PeerFeatures,
        address: Multiaddr,
        direction: ConnectionDirection,
        substream_counter: AtomicRefCounter,
    ) -> Self {
        Self {
            id,
            request_tx,
            peer_node_id,
            drop_old_connections,
            peer_features,
            address: Arc::new(address),
            direction,
            started_at: Instant::now(),
            substream_counter,
            handle_counter: Arc::new(()),
            drop_notifier: OneshotTrigger::<NodeId>::new(),
            number_of_rpc_clients: None,
        }
    }

    pub fn peer_node_id(&self) -> &NodeId {
        &self.peer_node_id
    }

    pub fn peer_features(&self) -> PeerFeatures {
        self.peer_features
    }

    pub fn direction(&self) -> ConnectionDirection {
        self.direction
    }

    pub fn known_address(&self) -> Option<&Multiaddr> {
        if self.direction.is_outbound() {
            Some(self.address())
        } else {
            None
        }
    }

    pub fn address(&self) -> &Multiaddr {
        &self.address
    }

    pub fn id(&self) -> ConnectionId {
        self.id
    }

    pub fn is_connected(&self) -> bool {
        !self.request_tx.is_closed()
    }

    /// Returns a owned future that resolves on disconnection
    pub fn on_disconnect(&self) -> impl Future<Output = ()> + 'static {
        let request_tx = self.request_tx.clone();
        async move { request_tx.closed().await }
    }

    pub fn age(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn substream_count(&self) -> usize {
        self.substream_counter.get()
    }

    pub fn handle_count(&self) -> usize {
        Arc::strong_count(&self.handle_counter)
    }

    pub async fn open_substream(
        &mut self,
        protocol_id: &ProtocolId,
    ) -> Result<NegotiatedSubstream<Substream>, PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::OpenSubstream {
                protocol_id: protocol_id.clone(),
                reply_tx,
            })
            .await?;
        reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?
    }

    pub async fn open_framed_substream(
        &mut self,
        protocol_id: &ProtocolId,
        max_frame_size: usize,
    ) -> Result<CanonicalFraming<Substream>, PeerConnectionError> {
        let substream = self.open_substream(protocol_id).await?;
        Ok(framing::canonical(substream.stream, max_frame_size))
    }

    #[cfg(feature = "rpc")]
    pub async fn connect_rpc<T>(&mut self) -> Result<T, RpcError>
    where T: From<RpcClient> + NamedProtocolService {
        self.connect_rpc_using_builder(Default::default()).await
    }

    #[cfg(feature = "rpc")]
    pub async fn connect_rpc_using_builder<T>(&mut self, builder: RpcClientBuilder<T>) -> Result<T, RpcError>
    where T: From<RpcClient> + NamedProtocolService {
        let protocol = ProtocolId::from_static(T::PROTOCOL_NAME);
        debug!(
            target: LOG_TARGET,
            "Attempting to establish RPC protocol `{}` to peer `{}` (drop_old_connections {})",
            String::from_utf8_lossy(&protocol), self.peer_node_id, self.drop_old_connections
        );
        let framed = self.open_framed_substream(&protocol, RPC_MAX_FRAME_SIZE).await?;

        let rpc_client = builder
            .with_protocol_id(protocol)
            .with_node_id(self.peer_node_id.clone())
            .with_drop_old_connections(self.drop_old_connections)
            .with_drop_receiver(self.drop_notifier.clone())
            .connect(framed)
            .await?;
        self.number_of_rpc_clients = Some(self.number_of_rpc_clients.unwrap_or(0) + 1);

        Ok(rpc_client)
    }

    /// Creates a new RpcClientPool that can be shared between tasks. The client pool will lazily establish up to
    /// `max_sessions` sessions and provides client session that is least used.
    #[cfg(feature = "rpc")]
    pub fn create_rpc_client_pool<T>(
        &self,
        max_sessions: usize,
        client_config: RpcClientBuilder<T>,
    ) -> RpcClientPool<T>
    where
        T: RpcPoolClient + From<RpcClient> + NamedProtocolService + Clone,
    {
        RpcClientPool::new(self.clone(), max_sessions, client_config)
    }

    /// Immediately disconnects the peer connection. This can only fail if the peer connection worker
    /// is shut down (and the peer is already disconnected)
    pub async fn disconnect(&mut self, minimized: Minimized) -> Result<(), PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::Disconnect(false, reply_tx, minimized))
            .await?;
        reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?
    }

    pub(crate) async fn disconnect_silent(&mut self, minimized: Minimized) -> Result<(), PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::Disconnect(true, reply_tx, minimized))
            .await?;
        reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?
    }
}

impl Drop for PeerConnection {
    fn drop(&mut self) {
        trace!(
            target: LOG_TARGET,
            "PeerConnection `{}` drop called, still has {} sub-streams and {} handles open",
            self.peer_node_id, self.substream_count(), self.handle_count(),
        );
        if let Some(number_of_rpc_clients) = self.number_of_rpc_clients {
            self.drop_notifier.broadcast(self.peer_node_id.clone());
            trace!(
                target: LOG_TARGET,
                "PeerConnection `{}` on drop notified {} RPC clients to drop connection",
                self.peer_node_id.clone(), number_of_rpc_clients,
            );
        }
    }
}

impl fmt::Display for PeerConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "Id: {}, Node ID: {}, Direction: {}, Peer Address: {}, Age: {:.0?}, #Substreams: {}, #Refs: {}",
            self.id,
            self.peer_node_id.short_str(),
            self.direction,
            self.address,
            self.age(),
            self.substream_count(),
            self.handle_count()
        )
    }
}

impl PartialEq for PeerConnection {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

/// Actor for an active connection to a peer.
struct PeerConnectionActor {
    id: ConnectionId,
    peer_node_id: NodeId,
    request_rx: mpsc::Receiver<PeerConnectionRequest>,
    direction: ConnectionDirection,
    incoming_substreams: IncomingSubstreams,
    control: Control,
    event_notifier: mpsc::Sender<ConnectionManagerEvent>,
    our_supported_protocols: Arc<Vec<ProtocolId>>,
    inbound_protocol_negotiations:
        FuturesUnordered<BoxFuture<'static, Result<(ProtocolId, Substream), PeerConnectionError>>>,
    their_supported_protocols: Vec<ProtocolId>,
}

impl PeerConnectionActor {
    fn new(
        id: ConnectionId,
        peer_node_id: NodeId,
        direction: ConnectionDirection,
        connection: Yamux,
        request_rx: mpsc::Receiver<PeerConnectionRequest>,
        event_notifier: mpsc::Sender<ConnectionManagerEvent>,
        our_supported_protocols: Arc<Vec<ProtocolId>>,
        their_supported_protocols: Vec<ProtocolId>,
    ) -> Self {
        Self {
            id,
            peer_node_id,
            direction,
            control: connection.get_yamux_control(),
            incoming_substreams: connection.into_incoming(),
            request_rx,
            event_notifier,
            our_supported_protocols,
            inbound_protocol_negotiations: FuturesUnordered::new(),
            their_supported_protocols,
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                maybe_request = self.request_rx.recv() => {
                    match maybe_request {
                        Some(request) => self.handle_request(request).await,
                        None => {
                            debug!(target: LOG_TARGET, "[{}] All peer connection handles dropped closing the connection", self);
                            break;
                        }
                    }
                },

                maybe_substream = self.incoming_substreams.next() => {
                    match maybe_substream {
                        Some(substream) => self.handle_incoming_substream(substream).await,
                        None => {
                            debug!(target: LOG_TARGET, "[{}] Peer '{}' closed the connection", self, self.peer_node_id.short_str());
                            break;
                        },
                    }
                },

                Some(result) = self.inbound_protocol_negotiations.next() => {
                    self.handle_inbound_protocol_negotiation_result(result).await;
                }
            }
        }

        if let Err(err) = self.disconnect(false, Minimized::No).await {
            warn!(
                target: LOG_TARGET,
                "[{}] Failed to politely close connection to peer '{}' because '{}'",
                self,
                self.peer_node_id.short_str(),
                err
            );
        }
    }

    async fn handle_request(&mut self, request: PeerConnectionRequest) {
        use PeerConnectionRequest::{Disconnect, OpenSubstream};
        match request {
            OpenSubstream { protocol_id, reply_tx } => {
                let tracing_id = tracing::Span::current().id();
                let span = span!(Level::TRACE, "handle_request");
                span.follows_from(tracing_id);
                let result = self.open_negotiated_protocol_stream(protocol_id).instrument(span).await;
                log_if_error_fmt!(
                    target: LOG_TARGET,
                    reply_tx.send(result),
                    "Reply oneshot closed when sending reply",
                );
            },
            Disconnect(silent, reply_tx, minimized) => {
                debug!(
                    target: LOG_TARGET,
                    "[{}] Disconnect{}requested for {} connection to peer '{}'",
                    self,
                    if silent { " (silent) " } else { " " },
                    self.direction,
                    self.peer_node_id.short_str()
                );
                let _result = reply_tx.send(self.disconnect(silent, minimized).await);
            },
        }
    }

    async fn handle_incoming_substream(&mut self, mut stream: Substream) {
        let our_supported_protocols = self.our_supported_protocols.clone();
        self.inbound_protocol_negotiations.push(Box::pin(async move {
            let mut protocol_negotiation = ProtocolNegotiation::new(&mut stream);

            let selected_protocol = time::timeout(
                PROTOCOL_NEGOTIATION_TIMEOUT,
                protocol_negotiation.negotiate_protocol_inbound(&our_supported_protocols),
            )
            .await
            .map_err(|_| PeerConnectionError::ProtocolNegotiationTimeout)??;
            Ok((selected_protocol, stream))
        }));
    }

    async fn handle_inbound_protocol_negotiation_result(
        &mut self,
        result: Result<(ProtocolId, Substream), PeerConnectionError>,
    ) {
        match result {
            Ok((selected_protocol, stream)) => {
                self.notify_event(ConnectionManagerEvent::NewInboundSubstream(
                    self.peer_node_id.clone(),
                    selected_protocol,
                    stream,
                ))
                .await;
            },
            Err(PeerConnectionError::ProtocolError(err)) if err.is_ban_offence() => {
                error!(
                    target: LOG_TARGET,
                    "[{}] PEER VIOLATION: Incoming substream for peer '{}' failed to open because '{}'",
                    self,
                    self.peer_node_id.short_str(),
                    err
                );

                self.notify_event(ConnectionManagerEvent::PeerViolation {
                    peer_node_id: self.peer_node_id.clone(),
                    details: err.to_string(),
                })
                .await;
            },
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "[{}] Incoming substream for peer '{}' failed to open because '{error}'",
                    self,
                    self.peer_node_id.short_str(),
                    error = err
                );
            },
        }
    }

    async fn open_negotiated_protocol_stream(
        &mut self,
        protocol: ProtocolId,
    ) -> Result<NegotiatedSubstream<Substream>, PeerConnectionError> {
        debug!(
            target: LOG_TARGET,
            "[{}] Negotiating protocol '{}' on new substream for peer '{}'",
            self,
            String::from_utf8_lossy(&protocol),
            self.peer_node_id.short_str()
        );
        let mut stream = self.control.open_stream().await?;

        let mut negotiation = ProtocolNegotiation::new(&mut stream);

        let selected_protocol = if self.their_supported_protocols.contains(&protocol) {
            let fut = negotiation.negotiate_protocol_outbound_optimistic(&protocol);
            time::timeout(PROTOCOL_NEGOTIATION_TIMEOUT, fut).await??
        } else {
            let selected_protocols = [protocol];
            let fut = negotiation.negotiate_protocol_outbound(&selected_protocols);
            time::timeout(PROTOCOL_NEGOTIATION_TIMEOUT, fut).await??
        };

        Ok(NegotiatedSubstream::new(selected_protocol, stream))
    }

    async fn notify_event(&mut self, event: ConnectionManagerEvent) {
        let _result = self.event_notifier.send(event).await;
    }

    /// Disconnect this peer connection.
    ///
    /// # Arguments
    ///
    /// silent - true to suppress the PeerDisconnected event, false to publish the event
    async fn disconnect(&mut self, silent: bool, minimized: Minimized) -> Result<(), PeerConnectionError> {
        self.request_rx.close();

        // Only emit closed event once
        if let Err(e) = self.control.close().await {
            match e {
                YamuxControlError::ConnectionClosed => {
                    trace!(
                        target: LOG_TARGET,
                        "On disconnect: (Peer = {}) Connection already closed ({})",
                        self.peer_node_id.short_str(),
                        e
                    );
                },
                e => trace!(target: LOG_TARGET, "On disconnect: ({})", e),
            }
        }

        if !silent {
            self.notify_event(ConnectionManagerEvent::PeerDisconnected(
                self.id,
                self.peer_node_id.clone(),
                minimized,
            ))
            .await;
        }
        trace!(target: LOG_TARGET, "(Peer = {}) Connection closed", self.peer_node_id.short_str());

        Ok(())
    }
}

impl fmt::Display for PeerConnectionActor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "PeerConnection(id={}, peer_node_id={}, direction={})",
            self.id,
            self.peer_node_id.short_str(),
            self.direction,
        )
    }
}

/// Contains the substream and the ProtocolId that was successfully negotiated.
pub struct NegotiatedSubstream<TSubstream> {
    pub protocol: ProtocolId,
    pub stream: TSubstream,
}

impl<TSubstream> NegotiatedSubstream<TSubstream> {
    pub fn new(protocol: ProtocolId, stream: TSubstream) -> Self {
        Self { protocol, stream }
    }
}

impl<TSubstream> fmt::Debug for NegotiatedSubstream<TSubstream> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NegotiatedSubstream")
            .field("protocol", &format!("{:?}", self.protocol))
            .field("stream", &"...".to_string())
            .finish()
    }
}

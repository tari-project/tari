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

#[cfg(feature = "rpc")]
use crate::protocol::rpc::{NamedProtocolService, RpcClient, RpcClientBuilder, RpcError, RPC_MAX_FRAME_SIZE};

use super::{
    error::{ConnectionManagerError, PeerConnectionError},
    manager::ConnectionManagerEvent,
    types::ConnectionDirection,
};
use crate::{
    framing,
    framing::CanonicalFraming,
    multiplexing::{Control, IncomingSubstreams, Substream, SubstreamCounter, Yamux},
    peer_manager::{NodeId, PeerFeatures},
    protocol::{ProtocolId, ProtocolNegotiation},
    runtime,
};
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    SinkExt,
    StreamExt,
};
use log::*;
use multiaddr::Multiaddr;
use std::{
    fmt,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use tari_shutdown::Shutdown;

const LOG_TARGET: &str = "comms::connection_manager::peer_connection";

const PEER_REQUEST_BUFFER_SIZE: usize = 64;

static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub fn create(
    connection: Yamux,
    peer_addr: Multiaddr,
    peer_node_id: NodeId,
    peer_features: PeerFeatures,
    direction: ConnectionDirection,
    event_notifier: mpsc::Sender<ConnectionManagerEvent>,
    our_supported_protocols: Vec<ProtocolId>,
) -> Result<PeerConnection, ConnectionManagerError>
{
    trace!(
        target: LOG_TARGET,
        "(Peer={}) Socket successfully upgraded to multiplexed socket",
        peer_node_id.short_str()
    );
    let (peer_tx, peer_rx) = mpsc::channel(PEER_REQUEST_BUFFER_SIZE);
    let id = ID_COUNTER.fetch_add(1, Ordering::Relaxed); // Monotonic
    let substream_counter = connection.substream_counter();
    let peer_conn = PeerConnection::new(
        id,
        peer_tx,
        peer_node_id.clone(),
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
    );
    runtime::current().spawn(peer_actor.run());

    Ok(peer_conn)
}

#[derive(Debug)]
pub enum PeerConnectionRequest {
    /// Open a new substream and negotiate the given protocol
    OpenSubstream(
        ProtocolId,
        oneshot::Sender<Result<NegotiatedSubstream<Substream>, PeerConnectionError>>,
    ),
    /// Disconnect all substreams and close the transport connection
    Disconnect(bool, oneshot::Sender<Result<(), PeerConnectionError>>),
}

pub type ConnectionId = usize;

/// Request handle for an active peer connection
#[derive(Clone, Debug)]
pub struct PeerConnection {
    id: ConnectionId,
    peer_node_id: NodeId,
    peer_features: PeerFeatures,
    request_tx: mpsc::Sender<PeerConnectionRequest>,
    address: Multiaddr,
    direction: ConnectionDirection,
    started_at: Instant,
    substream_counter: SubstreamCounter,
}

impl PeerConnection {
    pub(crate) fn new(
        id: ConnectionId,
        request_tx: mpsc::Sender<PeerConnectionRequest>,
        peer_node_id: NodeId,
        peer_features: PeerFeatures,
        address: Multiaddr,
        direction: ConnectionDirection,
        substream_counter: SubstreamCounter,
    ) -> Self
    {
        Self {
            id,
            request_tx,
            peer_node_id,
            peer_features,
            address,
            direction,
            started_at: Instant::now(),
            substream_counter,
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

    pub fn address(&self) -> &Multiaddr {
        &self.address
    }

    pub fn id(&self) -> ConnectionId {
        self.id
    }

    pub fn is_connected(&self) -> bool {
        !self.request_tx.is_closed()
    }

    pub fn age(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn substream_count(&self) -> usize {
        self.substream_counter.get()
    }

    pub async fn open_substream(
        &mut self,
        protocol_id: &ProtocolId,
    ) -> Result<NegotiatedSubstream<Substream>, PeerConnectionError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::OpenSubstream(protocol_id.clone(), reply_tx))
            .await?;
        reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?
    }

    pub async fn open_framed_substream(
        &mut self,
        protocol_id: &ProtocolId,
        max_frame_size: usize,
    ) -> Result<CanonicalFraming<Substream>, PeerConnectionError>
    {
        let substream = self.open_substream(protocol_id).await?;
        Ok(framing::canonical(substream.stream, max_frame_size))
    }

    #[cfg(feature = "rpc")]
    pub async fn connect_rpc<T>(&mut self) -> Result<T, RpcError>
    where T: From<RpcClient> + NamedProtocolService {
        self.connect_rpc_builder(Default::default()).await
    }

    #[cfg(feature = "rpc")]
    pub async fn connect_rpc_builder<T>(&mut self, builder: RpcClientBuilder<T>) -> Result<T, RpcError>
    where T: From<RpcClient> + NamedProtocolService {
        let framed = self
            .open_framed_substream(&T::PROTOCOL_NAME.into(), RPC_MAX_FRAME_SIZE)
            .await?;
        builder.connect(framed).await
    }

    /// Immediately disconnects the peer connection. This can only fail if the peer connection worker
    /// is shut down (and the peer is already disconnected)
    pub async fn disconnect(&mut self) -> Result<(), PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::Disconnect(false, reply_tx))
            .await?;
        reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?
    }

    pub(crate) async fn disconnect_silent(&mut self) -> Result<(), PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::Disconnect(true, reply_tx))
            .await?;
        reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?
    }
}

impl fmt::Display for PeerConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        write!(
            f,
            "Id = {}, Node ID = {}, Direction = {}, Peer Address = {}, Features = {:?}",
            self.id,
            self.peer_node_id.short_str(),
            self.direction,
            self.address,
            self.peer_features,
        )
    }
}

/// Actor for an active connection to a peer.
pub struct PeerConnectionActor {
    id: ConnectionId,
    peer_node_id: NodeId,
    request_rx: Fuse<mpsc::Receiver<PeerConnectionRequest>>,
    direction: ConnectionDirection,
    incoming_substreams: Fuse<IncomingSubstreams>,
    substream_shutdown: Option<Shutdown>,
    control: Control,
    event_notifier: mpsc::Sender<ConnectionManagerEvent>,
    supported_protocols: Vec<ProtocolId>,
    shutdown: bool,
}

impl PeerConnectionActor {
    fn new(
        id: ConnectionId,
        peer_node_id: NodeId,
        direction: ConnectionDirection,
        connection: Yamux,
        request_rx: mpsc::Receiver<PeerConnectionRequest>,
        event_notifier: mpsc::Sender<ConnectionManagerEvent>,
        supported_protocols: Vec<ProtocolId>,
    ) -> Self
    {
        Self {
            id,
            peer_node_id,
            direction,
            control: connection.get_yamux_control(),
            incoming_substreams: connection.incoming().fuse(),
            substream_shutdown: None,
            request_rx: request_rx.fuse(),
            event_notifier,
            shutdown: false,
            supported_protocols,
        }
    }

    pub async fn run(mut self) {
        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => self.handle_request(request).await,

                maybe_substream = self.incoming_substreams.next() => {
                    match maybe_substream {
                        Some(substream) => {
                            if let Err(err) = self.handle_incoming_substream(substream).await {
                                error!(
                                    target: LOG_TARGET,
                                    "[{}] Incoming substream for peer '{}' failed to open because '{error}'",
                                    self,
                                    self.peer_node_id.short_str(),
                                    error = err
                                )
                            }
                        },
                        None => {
                            debug!(target: LOG_TARGET, "[{}] Peer '{}' closed the connection", self, self.peer_node_id.short_str());
                            let _ = self.disconnect(false).await;
                        },
                    }
                }
            }

            if self.shutdown {
                break;
            }
        }
    }

    async fn handle_request(&mut self, request: PeerConnectionRequest) {
        use PeerConnectionRequest::*;
        match request {
            OpenSubstream(proto, reply_tx) => {
                let result = self.open_negotiated_protocol_stream(proto).await;
                log_if_error_fmt!(
                    target: LOG_TARGET,
                    reply_tx.send(result),
                    "Reply oneshot closed when sending reply",
                );
            },
            Disconnect(silent, reply_tx) => {
                debug!(
                    target: LOG_TARGET,
                    "[{}] Disconnect{}requested for {} connection to peer '{}'",
                    self,
                    if silent { " (silent) " } else { " " },
                    self.direction,
                    self.peer_node_id.short_str()
                );
                let _ = reply_tx.send(self.disconnect(silent).await);
            },
        }
    }

    async fn handle_incoming_substream(&mut self, mut stream: Substream) -> Result<(), PeerConnectionError> {
        let selected_protocol = ProtocolNegotiation::new(&mut stream)
            .negotiate_protocol_inbound(&self.supported_protocols)
            .await?;

        self.notify_event(ConnectionManagerEvent::NewInboundSubstream(
            Box::new(self.peer_node_id.clone()),
            selected_protocol,
            stream,
        ))
        .await;

        Ok(())
    }

    async fn open_negotiated_protocol_stream(
        &mut self,
        protocol: ProtocolId,
    ) -> Result<NegotiatedSubstream<Substream>, PeerConnectionError>
    {
        debug!(
            target: LOG_TARGET,
            "[{}] Negotiating protocol '{}' on new substream for peer '{}'",
            self,
            String::from_utf8_lossy(&protocol),
            self.peer_node_id.short_str()
        );
        let mut stream = self.control.open_stream().await?;

        let mut negotiation = ProtocolNegotiation::new(&mut stream);

        let selected_protocol = if self.supported_protocols.contains(&protocol) {
            negotiation.negotiate_protocol_outbound_optimistic(&protocol).await?
        } else {
            negotiation.negotiate_protocol_outbound(&[protocol]).await?
        };

        Ok(NegotiatedSubstream::new(selected_protocol, stream))
    }

    async fn notify_event(&mut self, event: ConnectionManagerEvent) {
        log_if_error!(
            target: LOG_TARGET,
            self.event_notifier.send(event).await,
            "Failed to send connection manager notification because '{}'",
        );
    }

    /// Disconnect this peer connection.
    ///
    /// # Arguments
    ///
    /// silent - true to suppress the PeerDisconnected event, false to publish the event
    async fn disconnect(&mut self, silent: bool) -> Result<(), PeerConnectionError> {
        let mut error = None;
        if let Err(err) = self.control.close().await {
            warn!(
                target: LOG_TARGET,
                "[{}] Failed to politely close connection to peer '{}' because '{}'",
                self,
                self.peer_node_id.short_str(),
                err
            );
            error = Some(err);
        }
        debug!(
            target: LOG_TARGET,
            "(Peer = {}) Connection closed",
            self.peer_node_id.short_str()
        );

        self.shutdown = true;
        // Shut down the incoming substream task
        if let Some(shutdown) = self.substream_shutdown.as_mut() {
            let _ = shutdown.trigger();
        }

        if !silent {
            self.notify_event(ConnectionManagerEvent::PeerDisconnected(Box::new(
                self.peer_node_id.clone(),
            )))
            .await;
        }

        error.map(Into::into).map(Err).unwrap_or(Ok(()))
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

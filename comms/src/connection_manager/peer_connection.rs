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

use super::error::ConnectionManagerError;
use crate::{
    connection::ConnectionDirection,
    connection_manager::{error::PeerConnectionError, manager::ConnectionManagerEvent},
    multiplexing::yamux::{Incoming, Yamux},
    peer_manager::NodeId,
    protocol::{ProtocolId, ProtocolNegotiation},
};
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    SinkExt,
    StreamExt,
};
use log::*;
use multiaddr::Multiaddr;
use std::fmt;
use tari_shutdown::Shutdown;
use tokio::runtime;

const LOG_TARGET: &str = "comms::connection_manager::peer_connection";

const PEER_REQUEST_BUFFER_SIZE: usize = 64;

pub fn create(
    executor: runtime::Handle,
    connection: Yamux,
    peer_addr: Multiaddr,
    peer_node_id: NodeId,
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
    let peer_conn = PeerConnection::new(peer_tx, peer_node_id.clone(), peer_addr, direction);
    let peer_actor = PeerConnectionActor::new(
        peer_node_id,
        connection,
        peer_rx,
        event_notifier,
        our_supported_protocols,
    );
    executor.spawn(peer_actor.run());

    Ok(peer_conn)
}

#[derive(Debug)]
pub enum PeerConnectionRequest {
    /// Open a new substream and negotiate the given protocol
    OpenSubstream(
        ProtocolId,
        oneshot::Sender<Result<NegotiatedSubstream, PeerConnectionError>>,
    ),
    /// Disconnect all substreams and close the transport connection
    Disconnect(bool, oneshot::Sender<()>),
}

/// Request handle for an active peer connection
#[derive(Clone, Debug)]
pub struct PeerConnection {
    peer_node_id: NodeId,
    request_tx: mpsc::Sender<PeerConnectionRequest>,
    address: Multiaddr,
    direction: ConnectionDirection,
}

impl PeerConnection {
    pub fn new(
        request_tx: mpsc::Sender<PeerConnectionRequest>,
        peer_node_id: NodeId,
        address: Multiaddr,
        direction: ConnectionDirection,
    ) -> Self
    {
        Self {
            request_tx,
            peer_node_id,
            address,
            direction,
        }
    }

    pub fn peer_node_id(&self) -> &NodeId {
        &self.peer_node_id
    }

    pub fn direction(&self) -> ConnectionDirection {
        self.direction
    }

    pub async fn open_substream<P: Into<ProtocolId>>(
        &mut self,
        protocol_id: P,
    ) -> Result<NegotiatedSubstream, PeerConnectionError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::OpenSubstream(protocol_id.into(), reply_tx))
            .await?;
        reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?
    }

    pub async fn disconnect(&mut self) -> Result<(), PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::Disconnect(false, reply_tx))
            .await?;
        Ok(reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?)
    }

    pub(crate) async fn disconnect_silent(&mut self) -> Result<(), PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.request_tx
            .send(PeerConnectionRequest::Disconnect(true, reply_tx))
            .await?;
        Ok(reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?)
    }
}

/// Actor for an active connection to a peer.
pub struct PeerConnectionActor {
    peer_node_id: NodeId,
    request_rx: Fuse<mpsc::Receiver<PeerConnectionRequest>>,
    incoming_substreams: Fuse<Incoming>,
    substream_shutdown: Option<Shutdown>,
    control: yamux::Control,
    event_notifier: mpsc::Sender<ConnectionManagerEvent>,
    supported_protocols: Vec<ProtocolId>,
    shutdown: bool,
}

impl PeerConnectionActor {
    pub fn new(
        peer_node_id: NodeId,
        connection: Yamux,
        request_rx: mpsc::Receiver<PeerConnectionRequest>,
        event_notifier: mpsc::Sender<ConnectionManagerEvent>,
        supported_protocols: Vec<ProtocolId>,
    ) -> Self
    {
        Self {
            peer_node_id,
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
                        Some(Ok(substream)) => {
                            if let Err(err) = self.handle_incoming_substream(substream).await {
                                error!(
                                    target: LOG_TARGET,
                                    "Incoming substream for peer '{}' failed to open because '{error}'",
                                    self.peer_node_id.short_str(),
                                    error = err
                                )
                            }
                        },
                        Some(Err(err)) => {
                            warn!(target: LOG_TARGET, "Incoming substream error '{}'. Closing connection for peer '{}'", err, self.peer_node_id.short_str());
                            self.disconnect(false).await;
                        },
                        None => {
                            warn!(target: LOG_TARGET, "Peer '{}' closed the connection", self.peer_node_id.short_str());
                            self.disconnect(false).await;
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
                    "Disconnect requested for peer with public key '{}'",
                    self.peer_node_id.short_str()
                );
                self.disconnect(silent).await;
                let _ = reply_tx.send(());
            },
        }
    }

    async fn handle_incoming_substream(&mut self, mut stream: yamux::Stream) -> Result<(), PeerConnectionError> {
        let selected_protocol = ProtocolNegotiation::new(&mut stream)
            // TODO: Will always fail. Get supported protocols when the protocol registry is implemented
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
    ) -> Result<NegotiatedSubstream, PeerConnectionError>
    {
        debug!(
            target: LOG_TARGET,
            "Negotiating protocol '{}' on new substream for peer '{}'",
            String::from_utf8_lossy(&protocol),
            self.peer_node_id.short_str()
        );
        let mut stream = self.control.open_stream().await?;
        let selected_protocol = ProtocolNegotiation::new(&mut stream)
            .negotiate_protocol_outbound(&[protocol])
            .await?;
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
    /// silent - true to supress the PeerDisconnected event, false to publish the event
    async fn disconnect(&mut self, silent: bool) {
        if let Err(err) = self.control.close().await {
            error!(
                target: LOG_TARGET,
                "Failed to politely close connection to peer '{}' because '{}'", self.peer_node_id, err
            );
        }
        trace!(target: LOG_TARGET, "Connection closed");

        self.shutdown = true;
        // Shut down the incoming substream task
        self.substream_shutdown.as_mut().and_then(|shutdown| {
            let _ = shutdown.trigger();
            Some(())
        });

        if !silent {
            self.notify_event(ConnectionManagerEvent::PeerDisconnected(Box::new(
                self.peer_node_id.clone(),
            )))
            .await;
        }
    }
}

pub struct NegotiatedSubstream {
    pub protocol: ProtocolId,
    pub stream: yamux::Stream,
}

impl NegotiatedSubstream {
    pub fn new(protocol: ProtocolId, stream: yamux::Stream) -> Self {
        Self { protocol, stream }
    }
}

impl fmt::Debug for NegotiatedSubstream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "NegotiatedSubstream {{ protocol: {:?}, substream: ... }}",
            self.protocol,
        )
    }
}

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
    connection_manager::{error::PeerConnectionError, manager::ConnectionManagerEvent, utils::short_str},
    multiplexing::yamux::{IncomingSubstream, Yamux},
    protocol::{ProtocolId, ProtocolNegotiation},
    types::CommsPublicKey,
};
use futures::{
    channel::{mpsc, oneshot},
    future,
    future::Either,
    stream::Fuse,
    AsyncRead,
    AsyncWrite,
    SinkExt,
    StreamExt,
};
use log::*;
use multiaddr::Multiaddr;
use std::{fmt, sync::Arc};
use tari_shutdown::Shutdown;
use tokio::runtime;

const LOG_TARGET: &str = "comms::connection_manager::peer_connection";

const PEER_REQUEST_BUFFER_SIZE: usize = 64;

pub async fn create_peer_connection<TSocket>(
    executor: runtime::Handle,
    socket: TSocket,
    peer_addr: Multiaddr,
    public_key: CommsPublicKey,
    direction: ConnectionDirection,
    event_notifier: mpsc::Sender<ConnectionManagerEvent>,
    supported_protocols: Vec<ProtocolId>,
) -> Result<PeerConnection, ConnectionManagerError>
where
    TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    match Yamux::upgrade_connection(socket, direction).await {
        Ok(connection) => {
            trace!(
                target: LOG_TARGET,
                "(Peer={}) Socket successfully upgraded to multiplexed socket",
                short_str(&public_key)
            );
            let (peer_tx, peer_rx) = mpsc::channel(PEER_REQUEST_BUFFER_SIZE);
            let peer_public_key = Arc::new(public_key);
            let peer_conn = PeerConnection::new(peer_tx, Arc::clone(&peer_public_key), peer_addr);
            let peer_actor = PeerConnectionActor::new(
                executor.clone(),
                peer_public_key,
                connection,
                peer_rx,
                event_notifier,
                supported_protocols,
            );
            executor.spawn(peer_actor.run());

            Ok(peer_conn)
        },
        Err(err) => Err(ConnectionManagerError::YamuxUpgradeFailure(err.to_string())),
    }
}

#[derive(Debug)]
pub enum PeerConnectionRequest {
    /// Open a new substream and negotiate the given protocol
    OpenSubstream(
        ProtocolId,
        oneshot::Sender<Result<NegotiatedSubstream, PeerConnectionError>>,
    ),
    /// Disconnect all substreams and close the transport connection
    Disconnect(oneshot::Sender<()>),
}

/// Request handle for an active peer connection
#[derive(Clone, Debug)]
pub struct PeerConnection {
    peer_public_key: Arc<CommsPublicKey>,
    request_tx: mpsc::Sender<PeerConnectionRequest>,
    address: Multiaddr,
}

impl PeerConnection {
    pub fn new(
        request_tx: mpsc::Sender<PeerConnectionRequest>,
        peer_public_key: Arc<CommsPublicKey>,
        address: Multiaddr,
    ) -> Self
    {
        Self {
            request_tx,
            peer_public_key,
            address,
        }
    }

    pub fn peer_public_key(&self) -> &CommsPublicKey {
        &self.peer_public_key
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
            .send(PeerConnectionRequest::Disconnect(reply_tx))
            .await?;
        Ok(reply_rx
            .await
            .map_err(|_| PeerConnectionError::InternalReplyCancelled)?)
    }
}

/// Actor for an active connection to a peer.
pub struct PeerConnectionActor {
    executor: runtime::Handle,
    peer_public_key: Arc<CommsPublicKey>,
    request_rx: Fuse<mpsc::Receiver<PeerConnectionRequest>>,
    incoming_substreams: Option<Fuse<IncomingSubstream<'static>>>,
    substream_shutdown: Option<Shutdown>,
    control: yamux::Control,
    event_notifier: mpsc::Sender<ConnectionManagerEvent>,
    supported_protocols: Vec<ProtocolId>,
    shutdown: bool,
}

impl PeerConnectionActor {
    pub fn new<TSocket>(
        executor: runtime::Handle,
        peer_public_key: Arc<CommsPublicKey>,
        connection: Yamux<TSocket>,
        request_rx: mpsc::Receiver<PeerConnectionRequest>,
        event_notifier: mpsc::Sender<ConnectionManagerEvent>,
        supported_protocols: Vec<ProtocolId>,
    ) -> Self
    where
        TSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
    {
        Self {
            executor,
            peer_public_key,
            control: connection.get_yamux_control(),
            incoming_substreams: Some(connection.incoming().fuse()),
            substream_shutdown: None,
            request_rx: request_rx.fuse(),
            event_notifier,
            shutdown: false,
            supported_protocols,
        }
    }

    pub async fn run(mut self) {
        let mut incoming_substreams = self.run_substream_task();

        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => self.handle_request(request).await,

                maybe_substream = incoming_substreams.next() => {
                    match maybe_substream {
                        Some(Ok(substream)) => {
                            if let Err(err) = self.handle_incoming_substream(substream).await {
                                error!(
                                    target: LOG_TARGET,
                                    "Incoming substream for peer '{}' failed to open because '{error}'",
                                    short_str(&*self.peer_public_key),
                                    error = err
                                )
                            }
                        },
                        Some(Err(err)) => {
                            warn!(target: LOG_TARGET, "Incoming substream error '{}'. Closing connection for peer '{}'", err, short_str(&*self.peer_public_key));
                            self.disconnect().await;
                        },
                        None => {
                            warn!(target: LOG_TARGET, "Peer '{}' closed the connection", self.peer_public_key);
                            self.disconnect().await;
                        },
                    }
                }
            }

            if self.shutdown {
                break;
            }
        }
    }

    fn run_substream_task(&mut self) -> mpsc::Receiver<Result<yamux::Stream, yamux::ConnectionError>> {
        let (mut incoming_substreams_tx, incoming_substreams_rx) = mpsc::channel(100);
        let mut incoming_substreams = self
            .incoming_substreams
            .take()
            .expect("PeerManagerActor initialized without incoming_substreams");
        let shutdown = Shutdown::new();
        let shutdown_signal = shutdown.to_signal();
        self.substream_shutdown = Some(shutdown);

        // yamux@0.4.0 requires the incoming substream stream be polled in order to make progress on requests from it's
        // Control api
        self.executor.spawn(async move {
            let mut signal = Some(shutdown_signal);
            loop {
                let either = future::select(incoming_substreams.next(), signal.take().expect("cannot fail")).await;
                match either {
                    Either::Left((Some(substream_result), sig)) => {
                        signal = Some(sig);
                        let _ = incoming_substreams_tx.send(substream_result).await;
                    },
                    Either::Left((None, _)) => {
                        info!(
                            target: LOG_TARGET,
                            "Incoming peer substream task is shutting down because the stream ended"
                        );
                        break;
                    },
                    Either::Right((_, _)) => {
                        info!(
                            target: LOG_TARGET,
                            "Incoming peer substream task is shutting down because the shutdown signal was received"
                        );
                        break;
                    },
                }
            }
        });

        incoming_substreams_rx
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
            Disconnect(reply_tx) => {
                debug!(
                    target: LOG_TARGET,
                    "Disconnect requested for peer with public key '{}'", self.peer_public_key
                );
                self.disconnect().await;
                let _ = reply_tx.send(());
            },
        }
    }

    async fn handle_incoming_substream(&mut self, mut stream: yamux::Stream) -> Result<(), PeerConnectionError> {
        let selected_protocol = ProtocolNegotiation::new(&mut stream)
            // TODO: Will always fail. Get supported protocols when the protocol registry is implemented
            .negotiate_protocol_inbound(&self.supported_protocols)
            .await?;

        self.event_notifier
            .send(ConnectionManagerEvent::NewInboundSubstream(
                Arc::clone(&self.peer_public_key),
                selected_protocol,
                stream,
            ))
            .await?;

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
            short_str(&*self.peer_public_key)
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

    async fn disconnect(&mut self) {
        if let Err(err) = self.control.close().await {
            error!(
                target: LOG_TARGET,
                "Failed to politely close connection to peer '{}' because '{}'", self.peer_public_key, err
            );
        }
        trace!(target: LOG_TARGET, "Connection closed");

        self.shutdown = true;
        // Shut down the incoming substream task
        self.substream_shutdown.as_mut().and_then(|shutdown| {
            let _ = shutdown.trigger();
            Some(())
        });

        self.notify_event(ConnectionManagerEvent::PeerDisconnected(Box::new(
            (*self.peer_public_key).clone(),
        )))
        .await;
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

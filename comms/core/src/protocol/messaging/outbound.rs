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

use std::time::Instant;

use futures::{future, SinkExt, StreamExt};
use tokio::{pin, sync::mpsc};
use tracing::{debug, error, span, Instrument, Level};

use super::{error::MessagingProtocolError, metrics, MessagingEvent, MessagingProtocol, SendFailReason};
use crate::{
    connection_manager::{NegotiatedSubstream, PeerConnection},
    connectivity::{ConnectivityError, ConnectivityRequester},
    message::OutboundMessage,
    multiplexing::Substream,
    peer_manager::NodeId,
    protocol::ProtocolId,
    stream_id::StreamId,
};

const LOG_TARGET: &str = "comms::protocol::messaging::outbound";
/// The number of times to retry sending a failed message before publishing a SendMessageFailed event.
/// This should only need to be 1 to handle the case where the pending dial is cancelled due to to tie breaking
/// and because the connection manager already retries dialing a number of times for each requested dial.
const MAX_SEND_RETRIES: usize = 1;

/// Actor for outbound messaging for a peer. This is spawned lazily when an outbound message must be sent.
pub struct OutboundMessaging {
    connectivity: ConnectivityRequester,
    messages_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    messaging_events_tx: mpsc::Sender<MessagingEvent>,
    retry_queue_tx: mpsc::UnboundedSender<OutboundMessage>,
    peer_node_id: NodeId,
    protocol_id: ProtocolId,
}

impl OutboundMessaging {
    pub fn new(
        connectivity: ConnectivityRequester,
        messaging_events_tx: mpsc::Sender<MessagingEvent>,
        messages_rx: mpsc::UnboundedReceiver<OutboundMessage>,
        retry_queue_tx: mpsc::UnboundedSender<OutboundMessage>,
        peer_node_id: NodeId,
        protocol_id: ProtocolId,
    ) -> Self {
        Self {
            connectivity,
            messages_rx,
            messaging_events_tx,
            retry_queue_tx,
            peer_node_id,
            protocol_id,
        }
    }

    pub async fn run(self) {
        let span = span!(
            Level::DEBUG,
            "comms::messaging::outbound",
            node_id = self.peer_node_id.to_string().as_str()
        );
        metrics::num_sessions().inc();
        async move {
            debug!(
                target: LOG_TARGET,
                "Attempting to dial peer '{}' if required", self.peer_node_id
            );
            let peer_node_id = self.peer_node_id.clone();
            let messaging_events_tx = self.messaging_events_tx.clone();
            match self.run_inner().await {
                Ok(_) => {
                    debug!(
                        target: LOG_TARGET,
                        "Outbound messaging for peer '{}' has stopped because the stream was closed", peer_node_id
                    );
                },
                Err(MessagingProtocolError::PeerDialFailed(err)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Outbound messaging protocol was unable to dial peer {}: {}", peer_node_id, err
                    );
                },
                Err(MessagingProtocolError::ConnectionClosed(err)) => {
                    // Not sure about the metrics, but feels safer to keep on registering the error in metrics for now
                    metrics::error_count(&peer_node_id).inc();
                    debug!(
                        target: LOG_TARGET,
                        "Connection closed {}: {} {}",
                        peer_node_id,
                        err.kind(),
                        err
                    );
                },
                Err(err) => {
                    metrics::error_count(&peer_node_id).inc();
                    error!(
                        target: LOG_TARGET,
                        "Outbound messaging protocol failed for peer {}: {}", peer_node_id, err
                    );
                },
            }

            metrics::num_sessions().dec();
            let _ignore = messaging_events_tx
                .send(MessagingEvent::OutboundProtocolExited(peer_node_id))
                .await;
        }
        .instrument(span)
        .await
    }

    async fn run_inner(mut self) -> Result<(), MessagingProtocolError> {
        let mut attempts = 0;

        let (conn, substream) = loop {
            match self.try_establish().await {
                Ok(conn_and_substream) => {
                    break conn_and_substream;
                },
                Err(err) => {
                    if attempts >= MAX_SEND_RETRIES {
                        debug!(
                            target: LOG_TARGET,
                            "Error establishing messaging protocol: {}. Aborting because maximum retries reached.", err
                        );
                        self.fail_all_pending_messages(SendFailReason::PeerDialFailed).await;
                        return Err(err);
                    }
                    debug!(
                        target: LOG_TARGET,
                        "Error establishing messaging protocol: {}. Retrying...", err
                    );
                    attempts += 1;
                },
            }
        };
        self.start_forwarding_messages(conn, substream).await?;

        Ok(())
    }

    async fn try_dial_peer(&mut self) -> Result<PeerConnection, MessagingProtocolError> {
        loop {
            match self.connectivity.dial_peer(self.peer_node_id.clone()).await {
                Ok(conn) => break Ok(conn),
                Err(ConnectivityError::DialCancelled) => {
                    debug!(
                        target: LOG_TARGET,
                        "Dial was cancelled for peer '{}'. This is probably because of connection tie-breaking. \
                         Retrying...",
                        self.peer_node_id,
                    );
                    continue;
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "MessagingProtocol failed to dial peer '{}' because '{:?}'", self.peer_node_id, err
                    );

                    break Err(MessagingProtocolError::PeerDialFailed(err));
                },
            }
        }
    }

    async fn try_establish(
        &mut self,
    ) -> Result<(PeerConnection, NegotiatedSubstream<Substream>), MessagingProtocolError> {
        let span = span!(
            Level::DEBUG,
            "establish_connection",
            node_id = self.peer_node_id.to_string().as_str()
        );
        async move {
            debug!(
                target: LOG_TARGET,
                "Attempting to establish messaging protocol connection to peer `{}`", self.peer_node_id
            );
            let start = Instant::now();
            let mut conn = self.try_dial_peer().await?;
            debug!(
                target: LOG_TARGET,
                "Connection succeeded for peer `{}` in {:.0?}",
                self.peer_node_id,
                start.elapsed()
            );
            let substream = self.try_open_substream(&mut conn).await?;
            debug!(
                target: LOG_TARGET,
                "Substream established for peer `{}`", self.peer_node_id,
            );
            Ok((conn, substream))
        }
        .instrument(span)
        .await
    }

    async fn try_open_substream(
        &mut self,
        conn: &mut PeerConnection,
    ) -> Result<NegotiatedSubstream<Substream>, MessagingProtocolError> {
        match conn.open_substream(&self.protocol_id).await {
            Ok(substream) => Ok(substream),
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "MessagingProtocol failed to open a substream to peer '{}' because '{}'", self.peer_node_id, err
                );
                Err(err.into())
            },
        }
    }

    async fn start_forwarding_messages(
        self,
        conn: PeerConnection,
        substream: NegotiatedSubstream<Substream>,
    ) -> Result<(), MessagingProtocolError> {
        let Self {
            mut messages_rx,
            peer_node_id,
            ..
        } = self;
        let span = span!(
            Level::DEBUG,
            "start_forwarding_messages",
            node_id = peer_node_id.to_string().as_str()
        );
        let _enter = span.enter();
        let stream_id = substream.stream.stream_id();
        debug!(
            target: LOG_TARGET,
            "Starting direct message forwarding for peer `{}` (stream: {})", peer_node_id, stream_id
        );

        let (sink, mut remote_stream) = MessagingProtocol::framed(substream.stream).split();

        // Convert unbounded channel to a stream
        let outbound_stream = futures::stream::unfold(&mut messages_rx, |rx| async move {
            let v = rx.recv().await;
            v.map(|v| (v, rx))
        });

        let outbound_count = metrics::outbound_message_count(&peer_node_id);
        let stream = outbound_stream.map(|mut out_msg| {
            outbound_count.inc();
            debug!(
                target: LOG_TARGET,
                "Message for peer '{}' sending {} on stream {}", peer_node_id, out_msg, stream_id
            );

            out_msg.reply_success();
            Result::<_, MessagingProtocolError>::Ok(out_msg.body)
        });

        // Stop the stream as soon as the disconnection occurs, this allows the outbound stream to terminate as soon as
        // the connection terminates rather than detecting the disconnect on the next message send.
        let stream = stream.take_until(async move {
            let on_disconnect = conn.on_disconnect();
            let peer_node_id = conn.peer_node_id().clone();
            // We drop the conn handle here BEFORE awaiting a disconnect to ensure that the outbound messaging isn't
            // holding onto the handle keeping the connection alive
            drop(conn);
            // Read from the yamux socket to determine if it is closed.
            let close_detect = remote_stream.next();
            pin!(on_disconnect);
            pin!(close_detect);
            future::select(on_disconnect, close_detect).await;
            debug!(
                target: LOG_TARGET,
                "Outbound messaging stream {} ended for peer {}.", stream_id, peer_node_id
            )
        });

        super::forward::Forward::new(stream, sink.sink_map_err(Into::into)).await?;

        // Close so that the protocol handler does not resend to this session
        messages_rx.close();
        // The stream ended, perhaps due to a disconnect, but there could be more messages left on the queue. Collect
        // any messages and queue them up for retry. If we cannot reconnect to the peer, the queued messages will be
        // dropped.
        let mut retried_messages_count = 0;
        while let Some(msg) = messages_rx.recv().await {
            if self.retry_queue_tx.send(msg).is_err() {
                // The messaging protocol has shut down, so let's exit too
                break;
            }
            retried_messages_count += 1;
        }

        if retried_messages_count > 0 {
            debug!(
                target: LOG_TARGET,
                "{} pending message(s) were still queued after disconnect. Retrying them.", retried_messages_count
            );
        }

        debug!(
            target: LOG_TARGET,
            "Direct message forwarding successfully completed for peer `{}` (stream: {}).", peer_node_id, stream_id
        );
        Ok(())
    }

    async fn fail_all_pending_messages(&mut self, reason: SendFailReason) {
        // Close the request channel so that we can read all the remaining messages and flush them
        // to a failed event
        self.messages_rx.close();
        while let Some(mut out_msg) = self.messages_rx.recv().await {
            out_msg.reply_fail(reason);
        }
    }
}

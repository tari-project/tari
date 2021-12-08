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

use std::time::{Duration, Instant};

use futures::{future::Either, SinkExt, StreamExt, TryStreamExt};
use tokio::sync::mpsc as tokiompsc;
use tracing::{debug, error, event, span, Instrument, Level};

use super::{error::MessagingProtocolError, metrics, MessagingEvent, MessagingProtocol, SendFailReason};
use crate::{
    connection_manager::{NegotiatedSubstream, PeerConnection},
    connectivity::{ConnectivityError, ConnectivityRequester},
    message::OutboundMessage,
    multiplexing::Substream,
    peer_manager::NodeId,
    protocol::messaging::protocol::MESSAGING_PROTOCOL,
};

const LOG_TARGET: &str = "comms::protocol::messaging::outbound";
/// The number of times to retry sending a failed message before publishing a SendMessageFailed event.
/// This should only need to be 1 to handle the case where the pending dial is cancelled due to to tie breaking
/// and because the connection manager already retries dialing a number of times for each requested dial.
const MAX_SEND_RETRIES: usize = 1;

pub struct OutboundMessaging {
    connectivity: ConnectivityRequester,
    request_rx: tokiompsc::UnboundedReceiver<OutboundMessage>,
    messaging_events_tx: tokiompsc::Sender<MessagingEvent>,
    peer_node_id: NodeId,
    inactivity_timeout: Option<Duration>,
}

impl OutboundMessaging {
    pub fn new(
        connectivity: ConnectivityRequester,
        messaging_events_tx: tokiompsc::Sender<MessagingEvent>,
        request_rx: tokiompsc::UnboundedReceiver<OutboundMessage>,
        peer_node_id: NodeId,
        inactivity_timeout: Option<Duration>,
    ) -> Self {
        Self {
            connectivity,
            request_rx,
            messaging_events_tx,
            peer_node_id,
            inactivity_timeout,
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
                "Attempting to dial peer '{}' if required",
                self.peer_node_id.short_str()
            );
            let peer_node_id = self.peer_node_id.clone();
            let messaging_events_tx = self.messaging_events_tx.clone();
            match self.run_inner().await {
                Ok(_) => {
                    event!(
                        Level::DEBUG,
                        "Outbound messaging for peer '{}' has stopped because the stream was closed",
                        peer_node_id.short_str()
                    );

                    debug!(
                        target: LOG_TARGET,
                        "Outbound messaging for peer '{}' has stopped because the stream was closed",
                        peer_node_id.short_str()
                    );
                },
                Err(MessagingProtocolError::Inactivity) => {
                    event!(
                        Level::DEBUG,
                        "Outbound messaging for peer  '{}' has stopped because it was inactive",
                        peer_node_id.short_str()
                    );
                    debug!(
                        target: LOG_TARGET,
                        "Outbound messaging for peer '{}' has stopped because it was inactive",
                        peer_node_id.short_str()
                    );
                },
                Err(MessagingProtocolError::PeerDialFailed(err)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Outbound messaging protocol was unable to dial peer {}: {}", peer_node_id, err
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
            let _ = messaging_events_tx
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
                    event!(Level::DEBUG, "Substream established");
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
        let span = span!(
            Level::DEBUG,
            "dial_peer",
            node_id = self.peer_node_id.to_string().as_str()
        );
        async move {
            loop {
                match self.connectivity.dial_peer(self.peer_node_id.clone()).await {
                    Ok(conn) => break Ok(conn),
                    Err(ConnectivityError::DialCancelled) => {
                        debug!(
                            target: LOG_TARGET,
                            "Dial was cancelled for peer '{}'. This is probably because of connection tie-breaking. \
                             Retrying...",
                            self.peer_node_id.short_str(),
                        );
                        continue;
                    },
                    Err(err) => {
                        debug!(
                            target: LOG_TARGET,
                            "MessagingProtocol failed to dial peer '{}' because '{:?}'",
                            self.peer_node_id.short_str(),
                            err
                        );

                        break Err(MessagingProtocolError::PeerDialFailed(err));
                    },
                }
            }
        }
        .instrument(span)
        .await
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
                "Attempting to establish messaging protocol connection to peer `{}`",
                self.peer_node_id.short_str()
            );
            let start = Instant::now();
            let mut conn = self.try_dial_peer().await?;
            debug!(
                target: LOG_TARGET,
                "Connection succeeded for peer `{}` in {:.0?}",
                self.peer_node_id.short_str(),
                start.elapsed()
            );
            let substream = self.try_open_substream(&mut conn).await?;
            debug!(
                target: LOG_TARGET,
                "Substream established for peer `{}`",
                self.peer_node_id.short_str(),
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
        let span = span!(
            Level::DEBUG,
            "open_substream",
            node_id = self.peer_node_id.to_string().as_str()
        );
        async {
            match conn.open_substream(&MESSAGING_PROTOCOL).await {
                Ok(substream) => Ok(substream),
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "MessagingProtocol failed to open a substream to peer '{}' because '{}'",
                        self.peer_node_id.short_str(),
                        err
                    );
                    Err(err.into())
                },
            }
        }
        .instrument(span)
        .await
    }

    async fn start_forwarding_messages(
        self,
        conn: PeerConnection,
        substream: NegotiatedSubstream<Substream>,
    ) -> Result<(), MessagingProtocolError> {
        let peer_node_id = self.peer_node_id;
        let span = span!(
            Level::DEBUG,
            "start_forwarding_messages",
            node_id = peer_node_id.to_string().as_str()
        );
        let _enter = span.enter();
        debug!(
            target: LOG_TARGET,
            "Starting direct message forwarding for peer `{}`", peer_node_id
        );

        let framed = MessagingProtocol::framed(substream.stream);

        let Self {
            mut request_rx,
            inactivity_timeout,
            ..
        } = self;

        // Convert unbounded channel to a stream
        let outbound_stream = futures::stream::unfold(&mut request_rx, |rx| async move {
            let v = rx.recv().await;
            v.map(|v| (v, rx))
        });

        let stream = match inactivity_timeout {
            Some(timeout) => Either::Left(
                tokio_stream::StreamExt::timeout(outbound_stream, timeout)
                    .map_err(|_| MessagingProtocolError::Inactivity),
            ),
            None => Either::Right(outbound_stream.map(Ok)),
        };

        let outbound_count = metrics::outbound_message_count(&peer_node_id);
        let stream = stream.map(|msg| {
            outbound_count.inc();
            msg.map(|mut out_msg| {
                event!(Level::DEBUG, "Message buffered for sending {}", out_msg);
                out_msg.reply_success();
                out_msg.body
            })
        });

        // Stop the stream as soon as the disconnection occurs, this allows the outbound stream to terminate as soon as
        // the connection terminates rather than detecting the disconnect on the next message send.
        let stream = stream.take_until(async move {
            let fut = conn.disconnected();
            let peer_node_id = conn.peer_node_id().clone();
            // We drop the conn handle here BEFORE awaiting a disconnect to ensure that the outbound messaging isn't
            // holding onto the handle keeping the connection alive
            drop(conn);
            fut.await;
            debug!(
                target: LOG_TARGET,
                "Peer connection closed. Ending outbound messaging stream for peer {}.", peer_node_id
            )
        });

        super::forward::Forward::new(stream, framed.sink_map_err(Into::into)).await?;

        debug!(
            target: LOG_TARGET,
            "Direct message forwarding successfully completed for peer `{}`.", peer_node_id
        );
        Ok(())
    }

    async fn fail_all_pending_messages(&mut self, reason: SendFailReason) {
        // Close the request channel so that we can read all the remaining messages and flush them
        // to a failed event
        self.request_rx.close();
        while let Some(mut out_msg) = self.request_rx.recv().await {
            out_msg.reply_fail(reason);
            let _ = self
                .messaging_events_tx
                .send(MessagingEvent::SendMessageFailed(out_msg, reason))
                .await;
        }
    }
}

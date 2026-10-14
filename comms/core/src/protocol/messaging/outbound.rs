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

use std::{fmt, time::Duration};

use futures::{SinkExt, StreamExt};
use tokio::{
    sync::{mpsc, mpsc::error::TrySendError},
    time,
    time::Instant,
};
use tracing::{Instrument, Level, debug, error, span, trace};

#[cfg(feature = "metrics")]
use super::metrics;
use super::{MessagingEvent, MessagingProtocol, SendFailReason, error::MessagingProtocolError};
use crate::{
    RefKind,
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
/// How long writing one message to the peer may take. A peer that stops reading leaves our write waiting for yamux
/// window forever, so once this passes the message fails, the session exits (resetting the substream) and every
/// message still queued for the peer fails with it.
pub(super) const OUTBOUND_SEND_TIMEOUT: Duration = Duration::from_secs(30);

/// An outbound message waiting in a peer's queue (or the retry queue), with the instant after which it is no longer
/// sent or retried. The deadline is set once, when the message is first queued, and is kept across retries.
#[derive(Debug)]
pub(super) struct QueuedOutboundMessage {
    pub message: OutboundMessage,
    pub deadline: Instant,
}

impl QueuedOutboundMessage {
    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

impl fmt::Display for QueuedOutboundMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}

/// Actor for outbound messaging for a peer. This is spawned lazily when an outbound message must be sent.
pub struct OutboundMessaging {
    connectivity: ConnectivityRequester,
    messages_rx: mpsc::Receiver<QueuedOutboundMessage>,
    messaging_events_tx: mpsc::Sender<MessagingEvent>,
    retry_queue_tx: mpsc::Sender<QueuedOutboundMessage>,
    peer_node_id: NodeId,
    protocol_id: ProtocolId,
}

impl OutboundMessaging {
    pub(super) fn new(
        connectivity: ConnectivityRequester,
        messaging_events_tx: mpsc::Sender<MessagingEvent>,
        messages_rx: mpsc::Receiver<QueuedOutboundMessage>,
        retry_queue_tx: mpsc::Sender<QueuedOutboundMessage>,
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
        #[cfg(feature = "metrics")]
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
                    #[cfg(feature = "metrics")]
                    metrics::error_count().inc();
                    debug!(
                        target: LOG_TARGET,
                        "Connection closed {}: {} {}",
                        peer_node_id,
                        err.kind(),
                        err
                    );
                },
                Err(err) => {
                    #[cfg(feature = "metrics")]
                    metrics::error_count().inc();
                    error!(
                        target: LOG_TARGET,
                        "Outbound messaging protocol failed for peer {}: {}", peer_node_id, err
                    );
                },
            }

            #[cfg(feature = "metrics")]
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
                    attempts = attempts.saturating_add(1);
                },
            }
        };
        self.start_forwarding_messages(conn, substream).await?;

        Ok(())
    }

    async fn try_dial_peer(&mut self) -> Result<PeerConnection, MessagingProtocolError> {
        loop {
            // Outbound messaging tolerates the connection being reaped — it will redial on
            // demand if the underlying connection has been torn down. Weak is correct.
            match self
                .connectivity
                .dial_peer(self.peer_node_id.clone(), RefKind::Weak)
                .await
            {
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

    #[allow(clippy::too_many_lines)]
    async fn start_forwarding_messages(
        self,
        conn: PeerConnection,
        substream: NegotiatedSubstream<Substream>,
    ) -> Result<(), MessagingProtocolError> {
        let Self {
            mut messages_rx,
            peer_node_id,
            retry_queue_tx,
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

        let (mut sink, mut remote_stream) = MessagingProtocol::framed(substream.stream).split();

        // We drop the `conn` handle before awaiting a disconnect below to ensure that outbound messaging isn't
        // itself holding onto the handle keeping the connection alive.
        let mut on_disconnect = Box::pin(conn.on_disconnect());
        drop(conn);

        #[cfg(feature = "metrics")]
        let outbound_count = metrics::outbound_message_count();

        // Set when a write took longer than `OUTBOUND_SEND_TIMEOUT`. The peer is not reading, so the messages still
        // queued for it are failed rather than retried on a new substream that would stall in the same way.
        let mut send_timed_out = false;

        // A message is only ever reported as sent (`reply_success`) once the write to the sink has actually
        // succeeded. Pulling a message off `messages_rx` and reporting success before attempting to write it -
        // the previous behaviour - meant that a message which was popped right as the connection tore down
        // (routine during simultaneous-dial tie breaking, which can cycle a peer's connection several times in
        // quick succession) was reported as delivered while never being written at all: it wasn't on the wire,
        // and having already been popped off `messages_rx`, it was also invisible to the retry drain below. Do
        // not restructure this back into a pipelined `Stream`/`Sink` forward without preserving "attempt before
        // ack" - that ordering is the fix.
        loop {
            let mut queued = tokio::select! {
                biased;
                _ = &mut on_disconnect => {
                    debug!(
                        target: LOG_TARGET,
                        "Outbound messaging stream {} ended for peer {} (connection disconnected).",
                        stream_id, peer_node_id
                    );
                    break;
                },
                // Read from the yamux socket to determine if it is closed. This is a send-only relationship for
                // this protocol, so any activity (including a clean close) on the read side means we're done.
                _ = remote_stream.next() => {
                    debug!(
                        target: LOG_TARGET,
                        "Outbound messaging stream {} ended for peer {} (remote closed).", stream_id, peer_node_id
                    );
                    break;
                },
                maybe_msg = messages_rx.recv() => {
                    match maybe_msg {
                        Some(msg) => msg,
                        None => {
                            debug!(
                                target: LOG_TARGET,
                                "Outbound messaging stream {} for peer {} ending: no more messages.",
                                stream_id, peer_node_id
                            );
                            break;
                        },
                    }
                },
            };

            if queued.is_expired() {
                debug!(
                    target: LOG_TARGET,
                    "Message for peer '{}' expired before it could be sent: {}", peer_node_id, queued
                );
                #[cfg(feature = "metrics")]
                metrics::outbound_dropped("expired").inc();
                queued.message.reply_fail(SendFailReason::Expired);
                continue;
            }

            #[cfg(feature = "metrics")]
            outbound_count.inc();
            trace!(
                target: LOG_TARGET,
                "Message for peer '{}' sending {} on stream {}", peer_node_id, queued, stream_id
            );

            // The write is raced against the disconnect signal, so a write waiting on a dead connection is noticed
            // straight away rather than after the timeout.
            let body = queued.message.body.clone();
            let send_result = tokio::select! {
                biased;
                _ = &mut on_disconnect => None,
                result = time::timeout(OUTBOUND_SEND_TIMEOUT, sink.send(body)) => Some(result),
            };
            match send_result {
                Some(Ok(Ok(()))) => queued.message.reply_success(),
                Some(Ok(Err(err))) => {
                    // The write genuinely failed - most commonly because the connection died between this
                    // message being popped off the channel and the write being attempted. It was never
                    // delivered, so it must not be acked as a success, and - since it is no longer in
                    // `messages_rx` for the drain below to find - it must be requeued here or it is lost for
                    // good.
                    debug!(
                        target: LOG_TARGET,
                        "Failed to send message to peer '{}' on stream {}: {}. Queuing for retry.",
                        peer_node_id, stream_id, err
                    );
                    #[cfg(feature = "metrics")]
                    metrics::error_count().inc();
                    Self::requeue_for_retry(&retry_queue_tx, queued);
                    break;
                },
                Some(Err(_elapsed)) => {
                    debug!(
                        target: LOG_TARGET,
                        "Sending a message to peer '{}' on stream {} took longer than {:.0?}. Closing the session.",
                        peer_node_id, stream_id, OUTBOUND_SEND_TIMEOUT
                    );
                    #[cfg(feature = "metrics")]
                    metrics::outbound_dropped("send_timeout").inc();
                    queued.message.reply_fail(SendFailReason::SendTimeout);
                    send_timed_out = true;
                    break;
                },
                None => {
                    debug!(
                        target: LOG_TARGET,
                        "Outbound messaging stream {} ended for peer {} while sending (connection disconnected). \
                         Queuing for retry.",
                        stream_id, peer_node_id
                    );
                    Self::requeue_for_retry(&retry_queue_tx, queued);
                    break;
                },
            }
        }

        // Dropping the substream resets it, so the peer's inbound session sees the end of this session
        drop(sink);
        drop(remote_stream);

        // Close so that the protocol handler does not resend to this session
        messages_rx.close();
        if send_timed_out {
            let mut failed_messages_count = 0usize;
            while let Some(mut queued) = messages_rx.recv().await {
                #[cfg(feature = "metrics")]
                metrics::outbound_dropped("send_timeout").inc();
                queued.message.reply_fail(SendFailReason::SendTimeout);
                failed_messages_count = failed_messages_count.saturating_add(1);
            }
            if failed_messages_count > 0 {
                debug!(
                    target: LOG_TARGET,
                    "{} pending message(s) for peer '{}' failed because a send timed out.",
                    failed_messages_count,
                    peer_node_id
                );
            }
            return Ok(());
        }

        // The stream ended, perhaps due to a disconnect, but there could be more messages left on the queue. Collect
        // the ones still inside their deadline and queue them up for retry, so a redial backlog cannot build up. If we
        // cannot reconnect to the peer, the queued messages will be dropped.
        let mut retried_messages_count = 0usize;
        while let Some(queued) = messages_rx.recv().await {
            if retry_queue_tx.is_closed() {
                // The messaging protocol has shut down, so let's exit too
                break;
            }
            if Self::requeue_for_retry(&retry_queue_tx, queued) {
                retried_messages_count = retried_messages_count.saturating_add(1);
            }
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

    /// Puts a message that was not sent on the retry queue, unless it has expired or the retry queue is full, in which
    /// case it fails now. Returns true if the message was queued.
    fn requeue_for_retry(
        retry_queue_tx: &mpsc::Sender<QueuedOutboundMessage>,
        mut queued: QueuedOutboundMessage,
    ) -> bool {
        if queued.is_expired() {
            #[cfg(feature = "metrics")]
            metrics::outbound_dropped("expired").inc();
            queued.message.reply_fail(SendFailReason::Expired);
            return false;
        }
        match retry_queue_tx.try_send(queued) {
            Ok(()) => true,
            Err(TrySendError::Full(mut queued)) => {
                debug!(
                    target: LOG_TARGET,
                    "Retry queue is full. Failing message {}", queued
                );
                #[cfg(feature = "metrics")]
                metrics::outbound_dropped("retry_queue_full").inc();
                queued.message.reply_fail(SendFailReason::QueueFull);
                false
            },
            // The messaging protocol has shut down. Dropping the message fails its reply.
            Err(TrySendError::Closed(_)) => false,
        }
    }

    async fn fail_all_pending_messages(&mut self, reason: SendFailReason) {
        // Close the request channel so that we can read all the remaining messages and flush them
        // to a failed event
        self.messages_rx.close();
        while let Some(mut queued) = self.messages_rx.recv().await {
            queued.message.reply_fail(reason);
        }
    }
}

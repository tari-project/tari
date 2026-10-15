//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{io, sync::Arc, task::Poll, time::Duration};

use futures::StreamExt;
use log::*;
use tari_shutdown::ShutdownSignal;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{OwnedSemaphorePermit, Semaphore, broadcast, mpsc, oneshot},
    time,
};

#[cfg(feature = "metrics")]
use super::metrics;
use super::{MessagingEvent, MessagingProtocol};
use crate::{PeerConnection, message::InboundMessage, peer_manager::NodeId};

const LOG_TARGET: &str = "comms::protocol::messaging::inbound";

/// The most messages one session may have delivered to the node-wide inbound channel (see
/// `INBOUND_MESSAGE_BUFFER_SIZE`) that the consumer has not yet finished processing. A message keeps its slot while it
/// waits in the channel and for as long as the consumer holds it - for the DHT, until its whole inbound pipeline call
/// has finished. A session holding all of them stops reading its substream, so a flooding peer is held back by its
/// own yamux window rather than filling the shared channel and starving every other peer.
pub(super) const INBOUND_PER_SESSION_INFLIGHT: usize = 4;
/// How long a stopping (replaced or disconnected) session may keep draining the frames it still has buffered, in total,
/// counted from when it starts stopping. Those frames were already reported as sent to the peer, so they are worth a
/// short wait for an in-flight slot, but the session must not be kept alive by a slow consumer, nor by a peer that
/// keeps refilling the old substream (reading it sends the peer more window).
pub(super) const STOPPING_SESSION_PERMIT_WAIT: Duration = Duration::from_secs(2);

/// Inbound messaging actor. This is lazily spawned per peer when a peer requests a messaging session.
pub struct InboundMessaging {
    connection: PeerConnection,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
    messaging_events_tx: broadcast::Sender<MessagingEvent>,
    /// The `MessagingProtocol` actor's internal channel - distinct from `messaging_events_tx` above, which is
    /// the external broadcast. Used to report this session's exit, keyed by `session_id`, so the actor can
    /// prune its `active_inbound`/`replacement_budgets` bookkeeping precisely (see
    /// `MessagingEvent::InboundSessionExited`), and to report protocol violations, which the actor bans for.
    internal_events_tx: mpsc::Sender<MessagingEvent>,
    session_id: u64,
    /// This session's in-flight slots. See [INBOUND_PER_SESSION_INFLIGHT].
    inflight_permits: Arc<Semaphore>,
    enable_message_received_event: bool,
    shutdown_signal: ShutdownSignal,
    replaced: oneshot::Receiver<()>,
}

impl InboundMessaging {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connection: PeerConnection,
        inbound_message_tx: mpsc::Sender<InboundMessage>,
        messaging_events_tx: broadcast::Sender<MessagingEvent>,
        internal_events_tx: mpsc::Sender<MessagingEvent>,
        session_id: u64,
        enable_message_received_event: bool,
        shutdown_signal: ShutdownSignal,
        replaced: oneshot::Receiver<()>,
    ) -> Self {
        Self {
            connection,
            inbound_message_tx,
            messaging_events_tx,
            internal_events_tx,
            session_id,
            inflight_permits: Arc::new(Semaphore::new(INBOUND_PER_SESSION_INFLIGHT)),
            enable_message_received_event,
            shutdown_signal,
            replaced,
        }
    }

    pub async fn run<S>(mut self, socket: S)
    where S: AsyncRead + AsyncWrite + Unpin {
        let peer = self.connection.peer_node_id().clone();

        #[cfg(feature = "metrics")]
        metrics::num_sessions().inc();
        debug!(
            target: LOG_TARGET,
            "Starting inbound messaging protocol for peer '{}'",
            peer.short_str()
        );

        let mut stream = MessagingProtocol::framed(socket);
        let on_disconnect = self.connection.on_disconnect();
        tokio::pin!(on_disconnect);
        // Set once the connection is reported as disconnected, or this session has been replaced by a fresher
        // substream (see `MessagingProtocol::spawn_inbound_handler`). A message can be fully written and
        // delivered by the sender before either happens (e.g. during simultaneous-dial tie breaking, which can
        // cycle a peer's connection several times in quick succession) and sit in the substream's read buffer,
        // already local and ready to read. Racing `stream.next()` against the stop signal directly (as
        // `StreamExt::take_until` does) never gives the stream a chance to be polled once that fires, discarding
        // that buffered data outright. Once stopping, keep draining whatever is immediately available - without
        // waiting for more data - until nothing is left or `stop_deadline` passes, whichever is first: reading frees
        // yamux window, so a peer could otherwise keep the old substream full forever. Note this only ever races
        // *between* frames: once a frame has been read off the wire,
        // `handle_frame_result` below runs to completion outside the select, so a replacement can never abort a
        // message that has already been decoded - at worst it delays taking over until the in-flight
        // `inbound_message_tx.send` finishes.
        let mut disconnected = false;
        // When a stopping session must exit, drained or not. Set once, when it starts stopping.
        let mut stop_deadline: Option<time::Instant> = None;

        loop {
            // Take one of this session's in-flight slots before reading the next frame. While the consumer is still
            // processing (or holding) messages for all of them, nothing more is read from the substream, which leaves
            // the peer's yamux window to push back on it. Once stopping (disconnected or replaced), only what is
            // already buffered is drained - the sender was told those frames were sent - and only until
            // `stop_deadline`, `STOPPING_SESSION_PERMIT_WAIT` after it started stopping, which also bounds the wait for
            // slots, so a stopping session never outlives that by waiting on the consumer or on a refilling peer.
            let permit = if disconnected {
                let deadline = *stop_deadline.get_or_insert_with(|| {
                    time::Instant::now()
                        .checked_add(STOPPING_SESSION_PERMIT_WAIT)
                        .unwrap_or_else(time::Instant::now)
                });
                match self.wait_for_permit_while_stopping(&peer, deadline).await {
                    Some(permit) => permit,
                    None => break,
                }
            } else {
                tokio::select! {
                    biased;
                    _ = self.shutdown_signal.wait() => break,
                    permit = Arc::clone(&self.inflight_permits).acquire_owned() => match permit {
                        Ok(permit) => permit,
                        // The semaphore is never closed
                        Err(_) => break,
                    },
                    // Stopping while waiting for a slot: go round again to wait (for a bounded time) as a stopping
                    // session. These futures are never polled again once `disconnected` is set.
                    _ = &mut on_disconnect => {
                        disconnected = true;
                        continue;
                    },
                    _ = &mut self.replaced => {
                        debug!(
                            target: LOG_TARGET,
                            "Inbound messaging session for peer '{}' replaced by a newer substream while waiting for \
                             an in-flight slot",
                            peer.short_str()
                        );
                        disconnected = true;
                        continue;
                    },
                }
            };

            let maybe_result = if disconnected {
                match futures::poll!(stream.next()) {
                    Poll::Ready(item) => item,
                    Poll::Pending => None,
                }
            } else {
                tokio::select! {
                    biased;
                    item = stream.next() => item,
                    _ = &mut on_disconnect => {
                        disconnected = true;
                        match futures::poll!(stream.next()) {
                            Poll::Ready(item) => item,
                            Poll::Pending => None,
                        }
                    },
                    _ = &mut self.replaced => {
                        debug!(
                            target: LOG_TARGET,
                            "Inbound messaging session for peer '{}' replaced by a newer substream", peer.short_str()
                        );
                        disconnected = true;
                        match futures::poll!(stream.next()) {
                            Poll::Ready(item) => item,
                            Poll::Pending => None,
                        }
                    },
                    _ = self.shutdown_signal.wait() => None,
                }
            };
            let Some(result) = maybe_result else { break };

            if !self.handle_frame_result(result, &peer, permit).await {
                break;
            }
        }

        // Deliberately dropped rather than gracefully closed (`SinkExt::close`): this actor only ever reads
        // from `stream`, so its write half is never used, and closing it is a no-op from the remote peer's
        // point of view - it does not tell their writer anything. Dropping the substream, by contrast, resets
        // it at the transport layer, which is what actually unblocks a peer whose `OutboundMessaging` is still
        // writing to it (e.g. because this session is exiting due to being replaced by a newer substream on
        // the same still-live connection - see `MessagingProtocol::spawn_inbound_handler` - rather than the
        // whole connection going away). Calling `close()` here was observed to leave such a peer blocked
        // indefinitely on its next write.
        drop(stream);

        let _ignore = self
            .messaging_events_tx
            .send(MessagingEvent::InboundProtocolExited(peer.clone()));
        // Session-keyed exit report for the actor's own `active_inbound`/`replacement_budgets` pruning (see
        // `MessagingEvent::InboundSessionExited`). If the actor has already shut down, its receiver is gone and
        // this simply has nothing to prune for any more.
        let _ignore = self
            .internal_events_tx
            .send(MessagingEvent::InboundSessionExited(peer.clone(), self.session_id))
            .await;
        #[cfg(feature = "metrics")]
        metrics::num_sessions().dec();
        debug!(
            target: LOG_TARGET,
            "Inbound messaging handler exited for peer `{}`",
            peer.short_str()
        );
    }

    /// Waits until `deadline` (at most) for an in-flight slot for a frame this stopping session still has buffered.
    /// Returns `None` if the deadline has passed or no slot came free before it, or on shutdown.
    async fn wait_for_permit_while_stopping(
        &mut self,
        peer: &NodeId,
        deadline: time::Instant,
    ) -> Option<OwnedSemaphorePermit> {
        // Checked first: a slot that is free right away would otherwise be taken even past the deadline
        if time::Instant::now() >= deadline {
            debug!(
                target: LOG_TARGET,
                "Stopping inbound messaging session for peer '{}' without draining the rest of its substream: it has \
                 been draining for {:.0?}",
                peer.short_str(),
                STOPPING_SESSION_PERMIT_WAIT
            );
            return None;
        }
        let wait = time::timeout_at(deadline, Arc::clone(&self.inflight_permits).acquire_owned());
        tokio::select! {
            biased;
            _ = self.shutdown_signal.wait() => None,
            result = wait => match result {
                Ok(Ok(permit)) => Some(permit),
                // No slot came free in time (or the semaphore closed, which it never does)
                _ => {
                    debug!(
                        target: LOG_TARGET,
                        "Stopping inbound messaging session for peer '{}' without draining the rest of its \
                         substream: no in-flight slot came free within {:.0?} of it stopping",
                        peer.short_str(),
                        STOPPING_SESSION_PERMIT_WAIT
                    );
                    None
                },
            },
        }
    }

    /// Handles a single decoded frame (or read error). Returns `false` if the read loop should stop.
    async fn handle_frame_result(
        &mut self,
        result: io::Result<bytes::BytesMut>,
        peer: &NodeId,
        permit: OwnedSemaphorePermit,
    ) -> bool {
        match result {
            Ok(raw_msg) => {
                #[cfg(feature = "metrics")]
                metrics::inbound_message_count().inc();
                let msg_len = raw_msg.len();
                let inbound_msg = InboundMessage::new(peer.clone(), raw_msg.freeze()).with_inflight_permit(permit);
                debug!(
                    target: LOG_TARGET,
                    "Received message {} from peer '{}' ({} bytes)",
                    inbound_msg.tag,
                    peer.short_str(),
                    msg_len
                );

                let message_tag = inbound_msg.tag;

                if self.inbound_message_tx.send(inbound_msg).await.is_err() {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to send InboundMessage {} for peer '{}' because inbound message channel closed",
                        message_tag,
                        peer.short_str(),
                    );

                    return false;
                }

                if self.enable_message_received_event {
                    let _result = self
                        .messaging_events_tx
                        .send(MessagingEvent::MessageReceived(peer.clone(), message_tag));
                }
                true
            },
            // LengthDelimitedCodec emits a InvalidData io error when the message length exceeds the maximum allowed
            Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                #[cfg(feature = "metrics")]
                metrics::error_count().inc();
                debug!(
                    target: LOG_TARGET,
                    "Failed to receive from peer '{}' because '{}'",
                    peer.short_str(),
                    err
                );
                // Sent to the actor, which bans the peer and then forwards the event to the external broadcast for
                // subscribers. Nothing bans for events sent straight to the external broadcast.
                let _result = self
                    .internal_events_tx
                    .send(MessagingEvent::ProtocolViolation {
                        peer_node_id: peer.clone(),
                        details: err.to_string(),
                    })
                    .await;
                false
            },
            Err(err) => {
                #[cfg(feature = "metrics")]
                metrics::error_count().inc();
                error!(
                    target: LOG_TARGET,
                    "Failed to receive from peer '{}' because '{}'",
                    peer.short_str(),
                    err
                );
                false
            },
        }
    }
}

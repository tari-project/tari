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

use std::{
    collections::{HashMap, hash_map::Entry},
    fmt,
    fmt::Display,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use log::*;
use tari_shutdown::{
    ShutdownSignal,
    oneshot_trigger::{OneshotSignal, OneshotTrigger},
};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Semaphore, broadcast, mpsc, mpsc::error::TrySendError, oneshot},
    task::JoinHandle,
    time,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[cfg(feature = "metrics")]
use super::metrics;
use super::{
    error::MessagingProtocolError,
    outbound::{QueueAccounting, QueuedBytes, QueuedOutboundMessage},
};
use crate::{
    PeerConnection,
    RefKind,
    connectivity::ConnectivityRequester,
    framing,
    message::{InboundMessage, MessageTag, OutboundMessage},
    multiplexing::Substream,
    peer_manager::NodeId,
    protocol::{
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
        messaging::{inbound::InboundMessaging, outbound::OutboundMessaging},
    },
};

const LOG_TARGET: &str = "comms::protocol::messaging";
const INTERNAL_MESSAGING_EVENT_CHANNEL_SIZE: usize = 10;

/// The most messages that may wait in one peer's outbound queue. A message sent to a peer whose queue is full fails
/// immediately (so the DHT can pick another route) instead of growing the queue.
pub(super) const OUTBOUND_QUEUE_CAPACITY: usize = 500;
/// The most body bytes that may wait in one peer's outbound queue. Counting messages alone lets a peer that never reads
/// hold 500 large messages (e.g. block replies it keeps asking for). A message that would go over this fails
/// immediately, unless the queue's reserve applies (see [`OUTBOUND_QUEUE_RESERVED_BYTES`]).
pub(super) const OUTBOUND_QUEUE_MAX_BYTES: usize = 64 * 1024 * 1024;
/// The most body bytes that may wait in all peers' outbound queues and the retry queue together, which bounds what many
/// peers that never read can make us hold. A message that would go over this fails immediately, unless its queue's
/// reserve applies (see [`OUTBOUND_QUEUE_RESERVED_BYTES`]). With the reserve, the worst case is this budget plus,
/// per established queue, the reserve and one frame, plus the dialing budget:
/// 512 MiB + established peers x (1 MiB + 8 MiB) + 64 MiB. Each dialing queue holds at most
/// [`OUTBOUND_DIALING_QUEUE_MAX_BYTES`] of that 64 MiB.
pub(super) const OUTBOUND_TOTAL_MAX_BYTES: usize = 512 * 1024 * 1024;
/// A queue whose session has established a substream and that holds less than this always takes another message that
/// fits in a frame ([`MAX_FRAME_LENGTH`]), whatever the peer and node-wide budgets say. Without it, a few peers that
/// never read could pin the node-wide budget and cut every connected peer down to one message at a time.
pub(super) const OUTBOUND_QUEUE_RESERVED_BYTES: usize = 1024 * 1024;
/// The most body bytes that may wait in queues whose session has not yet established a substream (still dialing),
/// across all peers. Replies to peers we cannot reach would otherwise be held for the whole dial, each in a fresh
/// queue. A message for a dialing queue that would go over this fails immediately; dialing queues get no reserve.
pub(super) const OUTBOUND_DIALING_MAX_BYTES: usize = 64 * 1024 * 1024;
/// The most body bytes of new messages one queue may hold while its session is still dialing, so one unreachable peer
/// cannot take the whole dialing budget. A single frame always fits, so a queue's first message is only held to the
/// dialing budget. Retried messages (a previous session's backlog) are not held to this, only to the dialing budget,
/// so a backlog survives a reconnect.
pub(super) const OUTBOUND_DIALING_QUEUE_MAX_BYTES: usize = MAX_FRAME_LENGTH;
/// The most messages that may wait to be retried, across all peers. A message that does not fit fails immediately.
const RETRY_QUEUE_CAPACITY: usize = 500;
/// How long after it was queued a message may still be sent. An expired message fails instead of being sent, and is
/// not requeued when its session ends, so a redial backlog cannot accumulate.
pub(super) const OUTBOUND_MESSAGE_TTL: Duration = Duration::from_secs(60);

/// Maximum frame size of a messaging protocol frame. The RPC frame size is derived from this so that anything that can
/// be propagated over messaging (e.g. a block) can also be fetched over RPC (e.g. during sync).
pub const MAX_FRAME_LENGTH: usize = 8 * 1_024 * 1_024;

/// A freshly negotiated inbound substream can outrace the ConnectivityManager's own processing of the
/// `PeerConnected` event for the same connection - most visibly during simultaneous-dial tie-breaking, where
/// the winning connection's pool entry briefly reads back as absent or `Disconnected` while the tie break is
/// still being resolved. The connection is not actually missing; only the pool bookkeeping is lagging by a
/// beat. Poll for it rather than giving up on the first miss - see `handle_protocol_notification`.
const CONNECTION_LOOKUP_RETRY_INTERVAL: Duration = Duration::from_millis(20);
const CONNECTION_LOOKUP_TIMEOUT: Duration = Duration::from_secs(2);

/// Caps how many `NewInboundSubstream` notifications may have a connection-resolution wait (see
/// `wait_for_connection`) in flight at once. Each wait is a *spawned task*, not inline in the actor's `select!`;
/// see the comment on `handle_protocol_notification` for why that distinction matters. A spawned task is still
/// a resource though, and a peer that keeps negotiating substreams for a NodeId that never resolves to a live
/// connection (banned, a tie-break loser that never reconnects, spoofed) should not be able to grow that
/// resource without bound. Once the cap is hit, new substreams needing a wait are shed immediately rather than
/// queued - the actor stays responsive and the shed substream is simply not handled, same as any other
/// unresolvable-connection case.
const MAX_PENDING_SUBSTREAM_RESOLUTIONS: usize = 128;

/// Bounds how often a peer's live inbound session may be replaced by a fresh substream (see
/// `spawn_inbound_handler`). Replacing is legitimate and expected - `OutboundMessaging` opens a brand new
/// substream every time it (re)establishes, including when retrying after a failed write - but nothing about a
/// new substream arriving proves the peer is behaving; an already-connected peer could otherwise force
/// unbounded spawn/stop churn just by opening substreams back to back. The window is generous enough that it
/// should never bind a legitimate retry burst, which is inherently bounded by how many messages were queued
/// when the failure happened.
const REPLACEMENT_RATE_WINDOW: Duration = Duration::from_secs(1);
const MAX_REPLACEMENTS_PER_WINDOW: u32 = 10;

/// Minimum gap between `warn!`-level logs for the same shed reason (per peer, or globally for the
/// resolution-permit shed below). Before this branch, a flood of unresolvable/rejected substreams was
/// naturally throttled to roughly one log line per `CONNECTION_LOOKUP_TIMEOUT` (2s) by the inline wait that
/// used to sit in front of every notification. Now that rejection is immediate, nothing paces the logging any
/// more even though the rejection itself is already bounded (by the rate limit, the stopping-session cap, or
/// the resolution-permit semaphore) - so repeats within the interval are logged at `debug!` instead.
const SHED_LOG_INTERVAL: Duration = Duration::from_secs(2);

/// Caps how many superseded sessions for one peer may still be cooperatively stopping - i.e. blocked
/// delivering a message they had already decoded at the moment they were replaced (see
/// `InboundMessaging::run`) - at the same time.
///
/// The rate limit above bounds how *fast* replacements happen, not how many outstanding stops can
/// accumulate: a superseded session only observes its stop signal *between* frames, and cannot observe it at
/// all while parked inside `inbound_message_tx.send`. That channel is shared node-wide
/// (`INBOUND_MESSAGE_BUFFER_SIZE` in `extension.rs`), so while it is saturated - a sync burst, or a briefly
/// slow downstream consumer - a peer replacing at the full allowed rate would otherwise accumulate one
/// live-but-stopping session per replacement for as long as the saturation lasts. This cap makes that bounded
/// too: once it is hit, further replacements are refused (the existing session is kept) until at least one
/// stopping session actually finishes.
const MAX_STOPPING_SESSIONS_PER_PEER: usize = 4;

pub type MessagingEventSender = broadcast::Sender<MessagingEvent>;
pub type MessagingEventReceiver = broadcast::Receiver<MessagingEvent>;

/// The reason for dial failure. This enum should contain simple variants which describe the kind of failure that
/// occurred
#[derive(Debug, Error, Copy, Clone)]
pub enum SendFailReason {
    #[error("Dial was attempted, but failed")]
    PeerDialFailed,
    /// Not currently produced by the messaging protocol
    #[error("Failed to open a messaging substream to peer")]
    SubstreamOpenFailed,
    /// Not currently produced by the messaging protocol
    #[error("Failed to send on substream channel")]
    SubstreamSendFailed,
    #[error("Message was dropped before sending")]
    Dropped,
    #[error("Message is larger than the maximum frame length")]
    MessageTooLarge,
    #[error("Message could not send after {0} attempt(s)")]
    MaxRetriesReached(usize),
    /// Like `MaxRetriesReached`, but at least one of the retries was used up because the whole connection to the peer
    /// went away (which we may have caused, e.g. by tie-breaking or reaping connections), not because the peer closed
    /// the substream
    #[error("Message could not send after {0} attempt(s) because the connection to the peer was lost")]
    ConnectionLost(usize),
    #[error("The outbound queue for the peer is full")]
    QueueFull,
    #[error("Message expired before it could be sent")]
    Expired,
    #[error("Timed out sending the message to the peer")]
    SendTimeout,
    /// The message was queued behind another message whose write to the peer timed out, so it was never attempted
    #[error("Message was queued behind a message that timed out sending to the peer")]
    QueuedBehindTimeout,
}

/// Events emitted by the messaging protocol.
#[derive(Debug, Clone)]
pub enum MessagingEvent {
    MessageReceived(NodeId, MessageTag),
    OutboundProtocolExited(NodeId),
    InboundProtocolExited(NodeId),
    /// Internal-only: a specific `InboundMessaging` session, identified by the `u64` session id assigned to it
    /// in `spawn_inbound_handler`, has exited. Sent only on the actor's internal `internal_messaging_event_tx`
    /// channel - never on the external `MessagingEventSender` broadcast (`InboundProtocolExited` above already
    /// covers that) - and never forwarded onto it either; see `handle_internal_messaging_event`. Keying this to
    /// a session rather than just a `NodeId` is deliberate: a peer can have a `current` session and several
    /// still-`stopping` superseded ones live at once (see `PeerInboundSessions`), and pruning by `NodeId` alone
    /// would remove whichever of those the map happens to be holding - including a `current` session that is
    /// still very much alive - whenever *any* of them exits.
    InboundSessionExited(NodeId, u64),
    ProtocolViolation {
        peer_node_id: NodeId,
        details: String,
    },
}

impl fmt::Display for MessagingEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MessagingEvent::*;
        match self {
            MessageReceived(node_id, tag) => write!(f, "MessageReceived({node_id}, {tag})"),
            OutboundProtocolExited(node_id) => write!(f, "OutboundProtocolExited({node_id})"),
            InboundProtocolExited(node_id) => write!(f, "InboundProtocolExited({node_id})"),
            InboundSessionExited(node_id, session_id) => {
                write!(f, "InboundSessionExited({node_id}, {session_id})")
            },
            ProtocolViolation { peer_node_id, details } => {
                write!(f, "ProtocolViolation({peer_node_id}, {details})")
            },
        }
    }
}

/// A live `InboundMessaging` session together with the means to stop it gracefully: `stop_tx` fires only
/// between frames (see `InboundMessaging::run`), so a replacement can never abort a message that has already
/// been decoded off the wire. `id` is this session's identity for `MessagingEvent::InboundSessionExited` - see
/// that variant's doc comment for why pruning must be keyed to it rather than to the peer alone.
struct ActiveInboundSession {
    id: u64,
    handle: JoinHandle<()>,
    stop_tx: oneshot::Sender<()>,
}

/// A peer's current inbound session, plus any earlier ones that were told to stop but have not yet confirmed
/// they finished (see `MAX_STOPPING_SESSIONS_PER_PEER`). `stopping` entries hold only their session id and
/// `JoinHandle` - `stop_tx` has already been sent for each of them by the time they land here, there is nothing
/// further to signal.
#[derive(Default)]
struct PeerInboundSessions {
    current: Option<ActiveInboundSession>,
    stopping: Vec<(u64, JoinHandle<()>)>,
}

impl PeerInboundSessions {
    /// Drops any stopping handles that have actually finished. Does not touch `current`.
    fn reap_finished(&mut self) {
        self.stopping.retain(|(_, handle)| !handle.is_finished());
    }
}

/// Per-peer rate limit on how often a live inbound session may be replaced. See
/// [`MAX_REPLACEMENTS_PER_WINDOW`].
#[derive(Default)]
struct ReplacementBudget {
    window_start: Option<time::Instant>,
    count: u32,
    /// Shared with the stopping-session cap in `spawn_inbound_handler`: both are "this peer's substream was
    /// shed" outcomes, so they throttle their `warn!` logging together rather than each keeping (and
    /// resetting) their own clock.
    last_shed_log: Option<time::Instant>,
}

impl ReplacementBudget {
    /// Records a replacement attempt now and returns whether it is within budget.
    fn try_consume(&mut self) -> bool {
        let now = time::Instant::now();
        let within_window = self
            .window_start
            .is_some_and(|start| now.saturating_duration_since(start) < REPLACEMENT_RATE_WINDOW);
        if within_window {
            if self.count >= MAX_REPLACEMENTS_PER_WINDOW {
                return false;
            }
            self.count = self.count.saturating_add(1);
        } else {
            self.window_start = Some(now);
            self.count = 1;
        }
        true
    }

    /// Whether a substream-shed reason for this peer should be logged at `warn!` right now, rather than
    /// `debug!`. Bounds the log rate to about once per [`SHED_LOG_INTERVAL`] per peer, regardless of how often
    /// substreams are actually being shed for it.
    fn should_warn_on_shed(&mut self) -> bool {
        let now = time::Instant::now();
        let should = match self.last_shed_log {
            Some(last) => now.saturating_duration_since(last) >= SHED_LOG_INTERVAL,
            None => true,
        };
        if should {
            self.last_shed_log = Some(now);
        }
        should
    }
}

/// Which peer queue, if any, a warning about a full outbound budget names
#[derive(Clone, Copy, PartialEq, Eq)]
enum LargestQueue {
    NotNamed,
    /// The largest queue of all
    Any,
    /// The largest queue that is still dialing
    Dialing,
}

/// The outbound queue of one outbound session for a peer, with its byte accounting. The accounting belongs to the
/// queue rather than the peer: a fresh session gets a fresh queue and accounting, and a message retried into it is
/// charged to the new queue and released from the old one. The old session's remaining messages are bounded by the
/// node-wide budget, which they stay charged to until they are dropped.
struct PeerQueue {
    sender: mpsc::Sender<QueuedOutboundMessage>,
    accounting: Arc<QueueAccounting>,
}

/// Actor responsible for lazily spawning inbound (protocol notifications) and outbound (mpsc channel) messaging actors.
pub struct MessagingProtocol {
    protocol_id: ProtocolId,
    connectivity: ConnectivityRequester,
    proto_notification: mpsc::Receiver<ProtocolNotification<Substream>>,
    active_queues: HashMap<NodeId, PeerQueue>,
    /// Body bytes of all messages waiting in the outbound queues and the retry queue. See
    /// [`OUTBOUND_TOTAL_MAX_BYTES`].
    total_queued_bytes: Arc<AtomicUsize>,
    /// Body bytes of the messages in queues whose session has not established a substream yet. See
    /// [`OUTBOUND_DIALING_MAX_BYTES`].
    dialing_queued_bytes: Arc<AtomicUsize>,
    active_inbound: HashMap<NodeId, PeerInboundSessions>,
    replacement_budgets: HashMap<NodeId, ReplacementBudget>,
    /// Monotonically increasing id assigned to each spawned `InboundMessaging` session (see
    /// `spawn_inbound_handler`), used to key `MessagingEvent::InboundSessionExited` pruning to the specific
    /// session that exited rather than to the peer.
    next_inbound_session_id: u64,
    outbound_message_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    messaging_events_tx: MessagingEventSender,
    enable_message_received_event: bool,
    ban_duration: Option<Duration>,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
    internal_messaging_event_tx: mpsc::Sender<MessagingEvent>,
    internal_messaging_event_rx: mpsc::Receiver<MessagingEvent>,
    retry_queue_tx: mpsc::Sender<QueuedOutboundMessage>,
    retry_queue_rx: mpsc::Receiver<QueuedOutboundMessage>,
    /// Substreams that resolved to a connection off the actor's `select!` loop (see
    /// `handle_protocol_notification`). The actor drains this and calls `spawn_inbound_handler` synchronously.
    resolved_substream_tx: mpsc::Sender<(PeerConnection, Substream)>,
    resolved_substream_rx: mpsc::Receiver<(PeerConnection, Substream)>,
    /// Bounds the number of connection-resolution waits spawned concurrently. See
    /// [`MAX_PENDING_SUBSTREAM_RESOLUTIONS`].
    pending_resolution_permits: Arc<Semaphore>,
    /// Throttles the `warn!` logged when [`Self::pending_resolution_permits`] is exhausted - this condition
    /// is node-wide, not per-peer, so unlike the `ReplacementBudget` throttles it needs only one clock. See
    /// [`SHED_LOG_INTERVAL`].
    last_pending_resolution_shed_log: Option<time::Instant>,
    /// Throttles the `warn!` logged when a message fails because a peer's outbound queue is full. See
    /// [`SHED_LOG_INTERVAL`].
    last_queue_full_log: Option<time::Instant>,
    shutdown_signal: ShutdownSignal,
    /// Never broadcast: dropped when this service exits, which resolves `complete_signal` with `None`.
    complete_trigger: OneshotTrigger<()>,
}

impl MessagingProtocol {
    /// Create a new messaging protocol actor.
    pub(super) fn new(
        protocol_id: ProtocolId,
        connectivity: ConnectivityRequester,
        proto_notification: mpsc::Receiver<ProtocolNotification<Substream>>,
        outbound_message_rx: mpsc::UnboundedReceiver<OutboundMessage>,
        messaging_events_tx: MessagingEventSender,
        inbound_message_tx: mpsc::Sender<InboundMessage>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        let (internal_messaging_event_tx, internal_messaging_event_rx) =
            mpsc::channel(INTERNAL_MESSAGING_EVENT_CHANNEL_SIZE);
        let (retry_queue_tx, retry_queue_rx) = mpsc::channel(RETRY_QUEUE_CAPACITY);
        let (resolved_substream_tx, resolved_substream_rx) = mpsc::channel(MAX_PENDING_SUBSTREAM_RESOLUTIONS);

        Self {
            protocol_id,
            connectivity,
            proto_notification,
            outbound_message_rx,
            active_inbound: Default::default(),
            active_queues: Default::default(),
            total_queued_bytes: Arc::new(AtomicUsize::new(0)),
            dialing_queued_bytes: Arc::new(AtomicUsize::new(0)),
            replacement_budgets: Default::default(),
            next_inbound_session_id: 0,
            messaging_events_tx,
            enable_message_received_event: false,
            internal_messaging_event_rx,
            internal_messaging_event_tx,
            ban_duration: None,
            retry_queue_tx,
            retry_queue_rx,
            inbound_message_tx,
            resolved_substream_tx,
            resolved_substream_rx,
            pending_resolution_permits: Arc::new(Semaphore::new(MAX_PENDING_SUBSTREAM_RESOLUTIONS)),
            last_pending_resolution_shed_log: None,
            last_queue_full_log: None,
            shutdown_signal,
            complete_trigger: OneshotTrigger::new(),
        }
    }

    /// Set to true to enable emitting the MessageReceived event for each message received. Typically only useful in
    /// tests.
    pub fn set_message_received_event_enabled(mut self, enabled: bool) -> Self {
        self.enable_message_received_event = enabled;
        self
    }

    /// Sets a custom ban duration. Banning is disabled by default.
    pub fn with_ban_duration(mut self, ban_duration: Duration) -> Self {
        self.ban_duration = Some(ban_duration);
        self
    }

    /// Returns a signal that resolves when this actor exits.
    pub fn complete_signal(&self) -> OneshotSignal<()> {
        self.complete_trigger.to_signal()
    }

    /// Runs the messaging protocol actor.
    pub async fn run(mut self) {
        let mut shutdown_signal = self.shutdown_signal.clone();

        loop {
            tokio::select! {
                Some(event) = self.internal_messaging_event_rx.recv() => {
                    self.handle_internal_messaging_event(event);
                },

                Some(msg) = self.retry_queue_rx.recv() => {
                    if let Err(err) = self.handle_retry_queue_messages(msg) {
                        error!(
                            target: LOG_TARGET,
                            "Failed to retry outbound message because '{err}'"

                        );
                    }
                },

                Some(msg) = self.outbound_message_rx.recv() => {
                    if let Err(err) = self.send_message(msg) {
                        error!(
                            target: LOG_TARGET,
                            "Failed to handle request because '{err}'"
                        );
                    }
                },

                Some(notification) = self.proto_notification.recv() => {
                    if let Err(err) = self.handle_protocol_notification(notification).await {
                        error!(target: LOG_TARGET, "handle_protocol_notification failed: {err}");
                    }
                },

                // A connection resolved off-actor for a substream received above (see
                // `handle_protocol_notification`). Handing it off here keeps `spawn_inbound_handler` - and the
                // `active_inbound`/`replacement_budgets` bookkeeping it touches - single-threaded through the
                // actor, without making the actor itself wait for the resolution.
                Some((conn, substream)) = self.resolved_substream_rx.recv() => {
                    self.spawn_inbound_handler(conn, substream);
                },

                _ = &mut shutdown_signal => {
                    info!(target: LOG_TARGET, "MessagingProtocol is shutting down because the shutdown signal was triggered");
                    break;
                }
            }
        }
    }

    #[inline]
    pub(super) fn framed<TSubstream>(socket: TSubstream) -> Framed<TSubstream, LengthDelimitedCodec>
    where TSubstream: AsyncRead + AsyncWrite + Unpin {
        framing::canonical(socket, MAX_FRAME_LENGTH)
    }

    fn handle_internal_messaging_event(&mut self, event: MessagingEvent) {
        use MessagingEvent::*;
        trace!(target: LOG_TARGET, "Internal messaging event '{event}'" );
        match &event {
            OutboundProtocolExited(node_id) => {
                debug!(
                    target: LOG_TARGET,
                    "Outbound protocol handler exited for peer `{}`",
                    node_id.short_str()
                );
                // Only remove the peer's queue if it belongs to the session that exited. Every outbound session
                // closes or drops its own queue receiver before it reports its exit, so that session's sender reads
                // as closed here. An open sender belongs to a newer session, spawned after the exited one closed its
                // queue (e.g. for a retry sent while the exited session waited out `CONNECTION_CLOSE_GRACE`), and
                // removing it would orphan that live session and let the next message spawn a second one.
                match self.active_queues.get(node_id) {
                    Some(queue) if queue.sender.is_closed() => {
                        self.active_queues.remove(node_id);
                    },
                    Some(_) => {
                        debug!(
                            target: LOG_TARGET,
                            "OutboundProtocolExited event for peer `{}`, but a newer outbound session is running; \
                             keeping it",
                            node_id.short_str()
                        );
                    },
                    None => {
                        debug!(
                            target: LOG_TARGET,
                            "OutboundProtocolExited event, but MessagingProtocol has no record of the outbound \
                             protocol for peer `{}`",
                            node_id.short_str()
                        );
                    },
                }
            },
            // `InboundMessaging` reports its exit to external subscribers on the broadcast
            // `messaging_events_tx` (see `InboundMessaging::run`), not on this internal channel - so this arm
            // never actually fires. `MessagingEvent::InboundSessionExited` below is this actor's own,
            // session-keyed signal for the same event and is what drives `active_inbound`/`replacement_budgets`
            // pruning.
            InboundProtocolExited(_) => {},
            InboundSessionExited(node_id, session_id) => {
                self.prune_inbound_session(node_id, *session_id);
                // Internal-only bookkeeping event - never forwarded to external subscribers.
                return;
            },
            ProtocolViolation { peer_node_id, details } => {
                self.ban_peer(peer_node_id.clone(), details.to_string());
            },
            _ => {},
        }

        // Forward the event
        let _result = self.messaging_events_tx.send(event);
    }

    /// Prunes bookkeeping for one inbound session that has exited, identified by `session_id` rather than just
    /// `node_id`. This must never simply remove the peer's whole `active_inbound` entry: a peer can have a
    /// `current` session and several still-`stopping` superseded ones at once (see `PeerInboundSessions`), and a
    /// superseded session finishing its cooperative stop is routine while `current` is still live - removing the
    /// entry on that alone would tear down a healthy session, undoing what the cooperative-handover stop signal
    /// (`ActiveInboundSession::stop_tx`) was written to protect.
    fn prune_inbound_session(&mut self, node_id: &NodeId, session_id: u64) {
        let Entry::Occupied(mut entry) = self.active_inbound.entry(node_id.clone()) else {
            return;
        };
        let sessions = entry.get_mut();
        if sessions
            .current
            .as_ref()
            .is_some_and(|current| current.id == session_id)
        {
            // The peer's live session itself exited (e.g. the connection dropped), not a superseded one.
            sessions.current = None;
        } else {
            sessions.stopping.retain(|(id, _)| *id != session_id);
        }
        if sessions.current.is_none() && sessions.stopping.is_empty() {
            entry.remove();
            // Only safe to drop the replacement-rate budget here - once the peer has no live or stopping
            // inbound session left at all - and not on every individual session exit: while `current` is still
            // live, a stopping session finishing is routine, and clearing the budget then would hand the peer a
            // fresh replacement-rate window each time one drains, defeating `MAX_REPLACEMENTS_PER_WINDOW`.
            self.replacement_budgets.remove(node_id);
        }
    }

    fn handle_retry_queue_messages(&mut self, msg: QueuedOutboundMessage) -> Result<(), MessagingProtocolError> {
        debug!(target: LOG_TARGET, "Retrying outbound message ({msg})");
        self.enqueue_message(msg, true)?;
        Ok(())
    }

    /// Queues a new message for its peer, with a deadline [`OUTBOUND_MESSAGE_TTL`] from now.
    pub(super) fn send_message(&mut self, out_msg: OutboundMessage) -> Result<(), MessagingProtocolError> {
        trace!(target: LOG_TARGET, "Received request to send message ({out_msg})");
        let deadline = time::Instant::now()
            .checked_add(OUTBOUND_MESSAGE_TTL)
            .unwrap_or_else(time::Instant::now);
        self.enqueue_message(
            QueuedOutboundMessage {
                message: out_msg,
                deadline,
                retries: 0,
                attempts: 0,
                retried_after_connection_loss: false,
                bytes: None,
            },
            false,
        )
    }

    /// Puts a message on its peer's outbound queue, spawning the peer's outbound handler if it has none. If the message
    /// is larger than a frame, the queue is full, or the message would go over a byte budget, it fails immediately.
    /// `is_retry` is set for a message coming off the retry queue: a session's backlog requeued into a fresh (dialing)
    /// queue is not held to [`OUTBOUND_DIALING_QUEUE_MAX_BYTES`], so it survives a reconnect, but it still counts
    /// against every other budget, including [`OUTBOUND_DIALING_MAX_BYTES`].
    #[allow(clippy::too_many_lines)]
    fn enqueue_message(
        &mut self,
        mut queued: QueuedOutboundMessage,
        is_retry: bool,
    ) -> Result<(), MessagingProtocolError> {
        let peer_node_id = queued.message.peer_node_id.clone();
        let len = queued.message.body.len();
        // A message that cannot be framed would only fail when it is written, holding a session and bytes until then
        if len > MAX_FRAME_LENGTH {
            #[cfg(feature = "metrics")]
            metrics::outbound_dropped("too_large").inc();
            queued.message.reply_fail(SendFailReason::MessageTooLarge);
            warn!(
                target: LOG_TARGET,
                "Message ({}) for peer '{}' is {} bytes, larger than the {} byte frame limit; failing it",
                queued.message.tag,
                peer_node_id.short_str(),
                len,
                MAX_FRAME_LENGTH
            );
            return Ok(());
        }

        let queue = loop {
            match self.active_queues.entry(peer_node_id.clone()) {
                Entry::Occupied(entry) => {
                    if entry.get().sender.is_closed() {
                        entry.remove();
                        continue;
                    }
                    break entry.into_mut();
                },
                Entry::Vacant(entry) => {
                    let accounting = Arc::new(QueueAccounting::new(
                        Arc::clone(&self.total_queued_bytes),
                        Arc::clone(&self.dialing_queued_bytes),
                    ));
                    let sender = Self::spawn_outbound_handler(
                        self.connectivity.clone(),
                        self.internal_messaging_event_tx.clone(),
                        peer_node_id.clone(),
                        self.retry_queue_tx.clone(),
                        self.protocol_id.clone(),
                        Arc::clone(&accounting),
                    );
                    break entry.insert(PeerQueue { sender, accounting });
                },
            }
        };
        let sender = queue.sender.clone();
        let accounting = Arc::clone(&queue.accounting);

        // A retried message is still charged to the queue it came from. Release that first, so it is charged once,
        // to this queue.
        queued.bytes = None;
        let (queued_for_peer, established) = accounting.snapshot();
        let queued_in_total = self.total_queued_bytes.load(Ordering::SeqCst);
        let queued_dialing = self.dialing_queued_bytes.load(Ordering::SeqCst);
        // Bytes are only ever added here, on the actor, so these checks cannot race with another charge; sessions
        // only release bytes or become established, which can only make room. An established queue below its reserve
        // always takes one more message (which fits in a frame, checked above), whatever the budgets say.
        let reserve_applies = established && queued_for_peer < OUTBOUND_QUEUE_RESERVED_BYTES;
        if !reserve_applies && queued_for_peer.saturating_add(len) > OUTBOUND_QUEUE_MAX_BYTES {
            #[cfg(feature = "metrics")]
            metrics::outbound_dropped("queue_bytes_full").inc();
            queued.message.reply_fail(SendFailReason::QueueFull);
            let msg = format!(
                "Outbound queue for peer '{}' holds {} bytes, over the {} byte budget with this message; failing \
                 message ({})",
                peer_node_id.short_str(),
                queued_for_peer,
                OUTBOUND_QUEUE_MAX_BYTES,
                queued.message.tag
            );
            self.log_queue_full(msg, LargestQueue::NotNamed);
            return Ok(());
        }
        if !established && !is_retry && queued_for_peer.saturating_add(len) > OUTBOUND_DIALING_QUEUE_MAX_BYTES {
            #[cfg(feature = "metrics")]
            metrics::outbound_dropped("dialing_queue_bytes_full").inc();
            queued.message.reply_fail(SendFailReason::QueueFull);
            let msg = format!(
                "Outbound queue for peer '{}' is still dialing and holds {} bytes, over the {} byte budget for a \
                 dialing queue with this message; failing message ({})",
                peer_node_id.short_str(),
                queued_for_peer,
                OUTBOUND_DIALING_QUEUE_MAX_BYTES,
                queued.message.tag
            );
            self.log_queue_full(msg, LargestQueue::NotNamed);
            return Ok(());
        }
        if !established && queued_dialing.saturating_add(len) > OUTBOUND_DIALING_MAX_BYTES {
            #[cfg(feature = "metrics")]
            metrics::outbound_dropped("dialing_bytes_full").inc();
            queued.message.reply_fail(SendFailReason::QueueFull);
            let msg = format!(
                "Outbound queues still dialing hold {} bytes, over the {} byte dialing budget with this message; \
                 failing message ({}) for peer '{}'",
                queued_dialing,
                OUTBOUND_DIALING_MAX_BYTES,
                queued.message.tag,
                peer_node_id.short_str()
            );
            self.log_queue_full(msg, LargestQueue::Dialing);
            return Ok(());
        }
        if !reserve_applies && queued_in_total.saturating_add(len) > OUTBOUND_TOTAL_MAX_BYTES {
            #[cfg(feature = "metrics")]
            metrics::outbound_dropped("total_bytes_full").inc();
            queued.message.reply_fail(SendFailReason::QueueFull);
            let msg = format!(
                "Outbound queues hold {} bytes, over the node-wide {} byte budget with this message; failing message \
                 ({}) for peer '{}'",
                queued_in_total,
                OUTBOUND_TOTAL_MAX_BYTES,
                queued.message.tag,
                peer_node_id.short_str()
            );
            self.log_queue_full(msg, LargestQueue::Any);
            return Ok(());
        }
        queued.bytes = Some(QueuedBytes::charge(len, accounting));

        trace!(target: LOG_TARGET, "Sending message {queued}");
        let tag = queued.message.tag;
        match sender.try_send(queued) {
            Ok(_) => {
                trace!(target: LOG_TARGET, "Message ({tag}) dispatched to outbound handler");
                Ok(())
            },
            Err(TrySendError::Full(mut queued)) => {
                #[cfg(feature = "metrics")]
                metrics::outbound_dropped("queue_full").inc();
                queued.message.reply_fail(SendFailReason::QueueFull);
                let msg = format!(
                    "Outbound queue for peer '{}' is full ({} messages); failing message ({})",
                    peer_node_id.short_str(),
                    OUTBOUND_QUEUE_CAPACITY,
                    tag
                );
                self.log_queue_full(msg, LargestQueue::NotNamed);
                Ok(())
            },
            Err(err @ TrySendError::Closed(_)) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to send message on channel because '{err:?}'"
                );
                Err(MessagingProtocolError::MessageSendFailed)
            },
        }
    }

    /// Logs a message that failed because an outbound queue or byte budget was full, at `warn!` at most once per
    /// [`SHED_LOG_INTERVAL`] and at `debug!` otherwise. For a node-wide budget, the `warn!` also names the peer whose
    /// queue holds the most bytes (among the queues that count against that budget), which is most likely the cause.
    fn log_queue_full(&mut self, msg: String, largest_queue: LargestQueue) {
        let now = time::Instant::now();
        let should_warn = match self.last_queue_full_log {
            Some(last) => now.saturating_duration_since(last) >= SHED_LOG_INTERVAL,
            None => true,
        };
        if !should_warn {
            debug!(target: LOG_TARGET, "{msg}");
            return;
        }
        self.last_queue_full_log = Some(now);
        if largest_queue == LargestQueue::NotNamed {
            warn!(target: LOG_TARGET, "{msg}");
            return;
        }
        // At most once per SHED_LOG_INTERVAL, so a scan of all queues is fine
        let mut largest: Option<(&NodeId, usize)> = None;
        for (node_id, queue) in &self.active_queues {
            let (queued_bytes, established) = queue.accounting.snapshot();
            if largest_queue == LargestQueue::Dialing && established {
                continue;
            }
            if largest.is_none_or(|(_, most)| queued_bytes > most) {
                largest = Some((node_id, queued_bytes));
            }
        }
        match largest {
            Some((node_id, queued_bytes)) => warn!(
                target: LOG_TARGET,
                "{msg}. The largest such queue is for peer '{}' with {} bytes",
                node_id.short_str(),
                queued_bytes
            ),
            None => warn!(target: LOG_TARGET, "{msg}"),
        }
    }

    /// A sender for this actor's internal event channel, for tests that inject events such as protocol violations
    #[cfg(test)]
    pub(super) fn internal_event_sender(&self) -> mpsc::Sender<MessagingEvent> {
        self.internal_messaging_event_tx.clone()
    }

    /// Waits for the next internal event, handles it as the running actor would, and returns it
    #[cfg(test)]
    pub(super) async fn handle_next_internal_event(&mut self) -> MessagingEvent {
        let event = self
            .internal_messaging_event_rx
            .recv()
            .await
            .expect("internal event channel closed");
        self.handle_internal_messaging_event(event.clone());
        event
    }

    /// Waits for the next message on the retry queue and handles it as the running actor would
    #[cfg(test)]
    pub(super) async fn handle_next_retry(&mut self) {
        let msg = self.retry_queue_rx.recv().await.expect("retry queue closed");
        self.handle_retry_queue_messages(msg).unwrap();
    }

    /// The number of messages waiting in the outbound queue for `peer_node_id`, if it has one
    #[cfg(test)]
    pub(super) fn outbound_queue_len(&self, peer_node_id: &NodeId) -> Option<usize> {
        self.active_queues
            .get(peer_node_id)
            .map(|queue| queue.sender.max_capacity().saturating_sub(queue.sender.capacity()))
    }

    /// The body bytes charged to the current outbound queue for `peer_node_id`, if it has one
    #[cfg(test)]
    pub(super) fn outbound_queue_bytes(&self, peer_node_id: &NodeId) -> Option<usize> {
        self.active_queues
            .get(peer_node_id)
            .map(|queue| queue.accounting.snapshot().0)
    }

    /// The body bytes charged across all outbound queues and the retry queue
    #[cfg(test)]
    pub(super) fn total_outbound_bytes(&self) -> usize {
        self.total_queued_bytes.load(Ordering::SeqCst)
    }

    /// The body bytes charged to queues whose session has not established a substream yet
    #[cfg(test)]
    pub(super) fn dialing_outbound_bytes(&self) -> usize {
        self.dialing_queued_bytes.load(Ordering::SeqCst)
    }

    /// Whether the session of the current outbound queue for `peer_node_id` has established a substream
    #[cfg(test)]
    pub(super) fn outbound_queue_established(&self, peer_node_id: &NodeId) -> Option<bool> {
        self.active_queues
            .get(peer_node_id)
            .map(|queue| queue.accounting.snapshot().1)
    }

    /// Bans `peer_node_id` as a protocol violation would
    #[cfg(test)]
    pub(super) fn ban_peer_for_test(&mut self, peer_node_id: NodeId) {
        self.ban_peer(peer_node_id, "test");
    }

    fn spawn_outbound_handler(
        connectivity: ConnectivityRequester,
        events_tx: mpsc::Sender<MessagingEvent>,
        peer_node_id: NodeId,
        retry_queue_tx: mpsc::Sender<QueuedOutboundMessage>,
        protocol_id: ProtocolId,
        queue_accounting: Arc<QueueAccounting>,
    ) -> mpsc::Sender<QueuedOutboundMessage> {
        let (msg_tx, msg_rx) = mpsc::channel(OUTBOUND_QUEUE_CAPACITY);
        let outbound_messaging = OutboundMessaging::new(
            connectivity,
            events_tx,
            msg_rx,
            retry_queue_tx,
            peer_node_id,
            protocol_id,
            queue_accounting,
        );
        tokio::spawn(outbound_messaging.run());
        msg_tx
    }

    fn spawn_inbound_handler(&mut self, conn: PeerConnection, substream: Substream) {
        let peer = conn.peer_node_id().clone();

        // Reap any earlier sessions that have actually finished stopping before deciding anything below -
        // this is what lets `MAX_STOPPING_SESSIONS_PER_PEER` reflect reality rather than only ever growing.
        if let Some(sessions) = self.active_inbound.get_mut(&peer) {
            sessions.reap_finished();
        }

        if let Some(sessions) = self.active_inbound.get(&peer) {
            match &sessions.current {
                Some(current) if !current.handle.is_finished() => {
                    // A new inbound substream for a peer that already has a live session is not necessarily a
                    // protocol violation: the peer's own `OutboundMessaging` opens a brand new substream
                    // every time it (re)establishes, including when it is retrying after its previous attempt
                    // appeared to fail (most commonly simultaneous-dial tie-breaking, which can cycle a
                    // connection several times in quick succession). Rejecting the new substream while the old
                    // session lingers used to be actively harmful here: the peer would see its fresh substream
                    // closed immediately, retry again, get rejected again, and so on - a tight loop that
                    // starved real traffic of a working session for the rest of the test. Whatever the reason,
                    // the *newest* substream is the one the peer is actually trying to use right now, so
                    // replace the old session with it rather than reject it - but only up to
                    // `MAX_REPLACEMENTS_PER_WINDOW` times per `REPLACEMENT_RATE_WINDOW`, and only while fewer
                    // than `MAX_STOPPING_SESSIONS_PER_PEER` earlier sessions are still cooperatively stopping;
                    // nothing here proves the peer is behaving, and a live session must not be replaceable, or
                    // its predecessors accumulate, without bound just because the peer keeps opening
                    // substreams.
                    let budget = self.replacement_budgets.entry(peer.clone()).or_default();
                    if !budget.try_consume() {
                        let msg = format!(
                            "Peer '{}' exceeded the inbound session replacement rate ({} within {:.0?}); dropping \
                             this substream and keeping the existing session.",
                            peer.short_str(),
                            MAX_REPLACEMENTS_PER_WINDOW,
                            REPLACEMENT_RATE_WINDOW
                        );
                        if budget.should_warn_on_shed() {
                            warn!(target: LOG_TARGET, "{msg}");
                        } else {
                            debug!(target: LOG_TARGET, "{msg}");
                        }
                        return;
                    }
                    if sessions.stopping.len() >= MAX_STOPPING_SESSIONS_PER_PEER {
                        // Every one of these is a session cooperatively draining an in-flight
                        // `inbound_message_tx.send` (see `InboundMessaging::run`) - most likely because the
                        // node-wide inbound channel is saturated. Replacing further would let this peer
                        // accumulate an unbounded number of live sessions for as long as that lasts; refuse
                        // instead and let the ones already stopping finish draining first.
                        let msg = format!(
                            "Peer '{}' already has {} inbound session(s) still stopping; dropping this substream \
                             rather than letting them accumulate further.",
                            peer.short_str(),
                            MAX_STOPPING_SESSIONS_PER_PEER
                        );
                        if budget.should_warn_on_shed() {
                            warn!(target: LOG_TARGET, "{msg}");
                        } else {
                            debug!(target: LOG_TARGET, "{msg}");
                        }
                        return;
                    }
                    debug!(
                        target: LOG_TARGET,
                        "Replacing InboundMessaging session for peer '{}' with a session for its newest \
                         substream",
                        peer.short_str()
                    );
                },
                _ => {},
            }
        }

        let messaging_events_tx = self.messaging_events_tx.clone();
        let inbound_message_tx = self.inbound_message_tx.clone();
        let (stop_tx, stop_rx) = oneshot::channel();
        // Each session gets its own id so its eventual `MessagingEvent::InboundSessionExited` can be pruned
        // precisely - see that variant's doc comment.
        let session_id = self.next_inbound_session_id;
        self.next_inbound_session_id = self.next_inbound_session_id.wrapping_add(1);
        let inbound_messaging = InboundMessaging::new(
            conn,
            inbound_message_tx,
            messaging_events_tx,
            self.internal_messaging_event_tx.clone(),
            session_id,
            self.enable_message_received_event,
            self.shutdown_signal.clone(),
            stop_rx,
        );
        let handle = tokio::spawn(inbound_messaging.run(substream));
        let new_session = ActiveInboundSession {
            id: session_id,
            handle,
            stop_tx,
        };

        let sessions = self.active_inbound.entry(peer).or_default();
        // Ask the outgoing session to stop rather than aborting it: `stop_tx` is only observed *between*
        // frames (see `InboundMessaging::run`), so this can never cut off a message that has already been
        // decoded off the wire and is being handed to `inbound_message_tx` - the abort-based version of this
        // could and did lose fully-received messages. Track its handle until it actually finishes (see the
        // `reap_finished` call above) so it counts against `MAX_STOPPING_SESSIONS_PER_PEER`.
        if let Some(outgoing) = sessions.current.replace(new_session) {
            let _ignore = outgoing.stop_tx.send(());
            sessions.stopping.push((outgoing.id, outgoing.handle));
        }
    }

    /// Looks up the active connection for `node_id`, tolerating the brief window where the
    /// ConnectivityManager's pool has not yet caught up with a connection that already exists on the wire (see
    /// [`CONNECTION_LOOKUP_TIMEOUT`]). Bounded so a peer that is genuinely not connected is not held open
    /// indefinitely.
    ///
    /// Deliberately takes `&ConnectivityRequester` rather than `&self`/`&mut self`: this runs inside a spawned
    /// task (see `handle_protocol_notification`), not inline in the actor, so it must not need the actor at
    /// all while it waits.
    async fn wait_for_connection(
        connectivity: &mut ConnectivityRequester,
        node_id: &NodeId,
    ) -> Result<Option<PeerConnection>, MessagingProtocolError> {
        let deadline = time::Instant::now()
            .checked_add(CONNECTION_LOOKUP_TIMEOUT)
            .unwrap_or_else(time::Instant::now);
        loop {
            if let Some(conn) = connectivity.get_connection(node_id.clone(), RefKind::Weak).await? {
                return Ok(Some(conn));
            }
            if time::Instant::now() >= deadline {
                return Ok(None);
            }
            time::sleep(CONNECTION_LOOKUP_RETRY_INTERVAL).await;
        }
    }

    async fn handle_protocol_notification(
        &mut self,
        notification: ProtocolNotification<Substream>,
    ) -> Result<(), MessagingProtocolError> {
        match notification.event {
            // Peer negotiated to speak the messaging protocol with us
            ProtocolEvent::NewInboundSubstream(node_id, substream) => {
                trace!(
                    target: LOG_TARGET,
                    "NewInboundSubstream for peer '{}'",
                    node_id.short_str()
                );
                // Resolving the connection for this substream can take up to `CONNECTION_LOOKUP_TIMEOUT` (2s):
                // the ConnectivityManager's pool briefly lagging a connection that already exists on the wire
                // is routine during tie-break resolution (see `wait_for_connection`), but a peer that is banned,
                // has a spoofed NodeId, or is a tie-break loser that never reconnects will never resolve at
                // all. Awaiting that inline here would park this actor's `select!` loop - and with it every
                // other peer's outbound messages, retries, and substreams - for up to 2s per such notification,
                // with no cap on how many could be queued back to back. Spawn the wait instead, bounded by
                // `pending_resolution_permits` so a flood of unresolvable substreams sheds rather than growing
                // the task count without limit, and hand the result back to the actor over
                // `resolved_substream_tx` so `spawn_inbound_handler` still only ever runs on the actor itself.
                match Arc::clone(&self.pending_resolution_permits).try_acquire_owned() {
                    Ok(permit) => {
                        let mut connectivity = self.connectivity.clone();
                        let resolved_tx = self.resolved_substream_tx.clone();
                        let mut shutdown_signal = self.shutdown_signal.clone();
                        tokio::spawn(async move {
                            let _permit = permit;
                            let wait = Self::wait_for_connection(&mut connectivity, &node_id);
                            tokio::pin!(wait);
                            tokio::select! {
                                biased;
                                _ = &mut shutdown_signal => {},
                                result = &mut wait => match result {
                                    Ok(Some(conn)) => {
                                        if resolved_tx.send((conn, substream)).await.is_err() {
                                            debug!(
                                                target: LOG_TARGET,
                                                "MessagingProtocol shut down before a resolved substream for \
                                                 peer '{}' could be handed off",
                                                node_id.short_str()
                                            );
                                        }
                                    },
                                    Ok(None) => {
                                        info!(
                                            target: LOG_TARGET,
                                            "No active connection for new inbound substream for node {node_id}"
                                        );
                                    },
                                    Err(err) => {
                                        error!(
                                            target: LOG_TARGET,
                                            "Failed to resolve connection for new inbound substream for node \
                                             {node_id}: {err}"
                                        );
                                    },
                                },
                            }
                        });
                    },
                    Err(_) => {
                        let msg = format!(
                            "Already resolving {MAX_PENDING_SUBSTREAM_RESOLUTIONS} inbound substream(s); dropping the \
                             new substream for peer '{}' rather than growing that further unbounded.",
                            node_id.short_str()
                        );
                        let now = time::Instant::now();
                        let should_warn = match self.last_pending_resolution_shed_log {
                            Some(last) => now.saturating_duration_since(last) >= SHED_LOG_INTERVAL,
                            None => true,
                        };
                        if should_warn {
                            self.last_pending_resolution_shed_log = Some(now);
                            warn!(target: LOG_TARGET, "{msg}");
                        } else {
                            debug!(target: LOG_TARGET, "{msg}");
                        }
                    },
                }
            },
        }
        Ok(())
    }

    /// Tears down everything this actor holds for the peer and asks the ConnectivityManager to ban it. The ban
    /// request is sent from a spawned task: it can wait up to the requester's timeout (10s) when the
    /// ConnectivityManager's request buffer is full, and awaiting it here would stall this actor - and every other
    /// peer's messaging - once per banned peer, which a stream of fresh identities sending oversized frames could
    /// repeat indefinitely.
    fn ban_peer<T: Display>(&mut self, peer_node_id: NodeId, reason: T) {
        warn!(
            target: LOG_TARGET,
            "Banning peer '{}' because it violated the messaging protocol: {}", peer_node_id.short_str(), reason
        );
        if let Some(sessions) = self.active_inbound.remove(&peer_node_id) {
            // A ban is a deliberate, rare response to a protocol violation, not routine churn - a hard abort
            // here is correct and intentional, unlike the graceful stop used for ordinary replacement. Abort
            // the current session and every session still cooperatively stopping - none of them get to
            // finish delivering once the peer has been judged to have violated the protocol.
            if let Some(current) = sessions.current {
                current.handle.abort();
            }
            for (_, handle) in sessions.stopping {
                handle.abort();
            }
        }
        self.replacement_budgets.remove(&peer_node_id);
        drop(self.active_queues.remove(&peer_node_id));
        match self.ban_duration {
            Some(ban_duration) => {
                let mut connectivity = self.connectivity.clone();
                let reason = reason.to_string();
                tokio::spawn(async move {
                    if let Err(err) = connectivity
                        .ban_peer_until(peer_node_id.clone(), ban_duration, reason)
                        .await
                    {
                        error!(
                            target: LOG_TARGET,
                            "Failed to ban peer '{}' because '{:?}'", peer_node_id.short_str(), err
                        );
                    }
                });
            },
            None => {
                warn!(
                    target: LOG_TARGET,
                    "Banning disabled in MessagingProtocol, so peer '{peer_node_id}' will not be banned (reason: {reason})",
                );
            },
        }
    }
}

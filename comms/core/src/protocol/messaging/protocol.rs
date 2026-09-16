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
    sync::Arc,
    time::Duration,
};

use log::*;
use tari_shutdown::{Shutdown, ShutdownSignal};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{Semaphore, broadcast, mpsc, oneshot},
    task::JoinHandle,
    time,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use super::error::MessagingProtocolError;
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

const MAX_FRAME_LENGTH: usize = 8 * 1_024 * 1_024;

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
    #[error("Failed to open a messaging substream to peer")]
    SubstreamOpenFailed,
    #[error("Failed to send on substream channel")]
    SubstreamSendFailed,
    #[error("Message was dropped before sending")]
    Dropped,
    #[error("Message could not send after {0} attempt(s)")]
    MaxRetriesReached(usize),
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

/// Actor responsible for lazily spawning inbound (protocol notifications) and outbound (mpsc channel) messaging actors.
pub struct MessagingProtocol {
    protocol_id: ProtocolId,
    connectivity: ConnectivityRequester,
    proto_notification: mpsc::Receiver<ProtocolNotification<Substream>>,
    active_queues: HashMap<NodeId, mpsc::UnboundedSender<OutboundMessage>>,
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
    retry_queue_tx: mpsc::UnboundedSender<OutboundMessage>,
    retry_queue_rx: mpsc::UnboundedReceiver<OutboundMessage>,
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
    shutdown_signal: ShutdownSignal,
    complete_trigger: Shutdown,
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
        let (retry_queue_tx, retry_queue_rx) = mpsc::unbounded_channel();
        let (resolved_substream_tx, resolved_substream_rx) = mpsc::channel(MAX_PENDING_SUBSTREAM_RESOLUTIONS);

        Self {
            protocol_id,
            connectivity,
            proto_notification,
            outbound_message_rx,
            active_inbound: Default::default(),
            active_queues: Default::default(),
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
            shutdown_signal,
            complete_trigger: Shutdown::new(),
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
    pub fn complete_signal(&self) -> ShutdownSignal {
        self.complete_trigger.to_signal()
    }

    /// Runs the messaging protocol actor.
    pub async fn run(mut self) {
        let mut shutdown_signal = self.shutdown_signal.clone();

        loop {
            tokio::select! {
                Some(event) = self.internal_messaging_event_rx.recv() => {
                    self.handle_internal_messaging_event(event).await;
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

    async fn handle_internal_messaging_event(&mut self, event: MessagingEvent) {
        use MessagingEvent::*;
        trace!(target: LOG_TARGET, "Internal messaging event '{event}'" );
        match &event {
            OutboundProtocolExited(node_id) => {
                debug!(
                    target: LOG_TARGET,
                    "Outbound protocol handler exited for peer `{}`",
                    node_id.short_str()
                );
                if self.active_queues.remove(node_id).is_none() {
                    debug!(
                        target: LOG_TARGET,
                        "OutboundProtocolExited event, but MessagingProtocol has no record of the outbound protocol \
                         for peer `{}`",
                        node_id.short_str()
                    );
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
                self.ban_peer(peer_node_id.clone(), details.to_string()).await;
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

    fn handle_retry_queue_messages(&mut self, msg: OutboundMessage) -> Result<(), MessagingProtocolError> {
        debug!(target: LOG_TARGET, "Retrying outbound message ({msg})");
        self.send_message(msg)?;
        Ok(())
    }

    fn send_message(&mut self, out_msg: OutboundMessage) -> Result<(), MessagingProtocolError> {
        trace!(target: LOG_TARGET, "Received request to send message ({out_msg})");
        let peer_node_id = out_msg.peer_node_id.clone();
        let sender = loop {
            match self.active_queues.entry(peer_node_id.clone()) {
                Entry::Occupied(entry) => {
                    if entry.get().is_closed() {
                        entry.remove();
                        continue;
                    }
                    break entry.into_mut();
                },
                Entry::Vacant(entry) => {
                    let sender = Self::spawn_outbound_handler(
                        self.connectivity.clone(),
                        self.internal_messaging_event_tx.clone(),
                        peer_node_id,
                        self.retry_queue_tx.clone(),
                        self.protocol_id.clone(),
                    );
                    break entry.insert(sender);
                },
            }
        };

        trace!(target: LOG_TARGET, "Sending message {out_msg}");
        let tag = out_msg.tag;
        match sender.send(out_msg) {
            Ok(_) => {
                trace!(target: LOG_TARGET, "Message ({tag}) dispatched to outbound handler");
                Ok(())
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to send message on channel because '{err:?}'"
                );
                Err(MessagingProtocolError::MessageSendFailed)
            },
        }
    }

    fn spawn_outbound_handler(
        connectivity: ConnectivityRequester,
        events_tx: mpsc::Sender<MessagingEvent>,
        peer_node_id: NodeId,
        retry_queue_tx: mpsc::UnboundedSender<OutboundMessage>,
        protocol_id: ProtocolId,
    ) -> mpsc::UnboundedSender<OutboundMessage> {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let outbound_messaging = OutboundMessaging::new(
            connectivity,
            events_tx,
            msg_rx,
            retry_queue_tx,
            peer_node_id,
            protocol_id,
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

    async fn ban_peer<T: Display>(&mut self, peer_node_id: NodeId, reason: T) {
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
                if let Err(err) = self
                    .connectivity
                    .ban_peer_until(peer_node_id.clone(), ban_duration, reason.to_string())
                    .await
                {
                    error!(
                        target: LOG_TARGET,
                        "Failed to ban peer '{}' because '{:?}'", peer_node_id.short_str(), err
                    );
                }
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

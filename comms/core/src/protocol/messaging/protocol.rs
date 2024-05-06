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
    collections::{hash_map::Entry, HashMap},
    fmt,
    fmt::Display,
    time::Duration,
};

use log::*;
use tari_shutdown::{Shutdown, ShutdownSignal};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
    task::JoinHandle,
};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

use super::error::MessagingProtocolError;
use crate::{
    connectivity::ConnectivityRequester,
    framing,
    message::{InboundMessage, MessageTag, OutboundMessage},
    multiplexing::Substream,
    peer_manager::NodeId,
    protocol::{
        messaging::{inbound::InboundMessaging, outbound::OutboundMessaging},
        ProtocolEvent,
        ProtocolId,
        ProtocolNotification,
    },
};

const LOG_TARGET: &str = "comms::protocol::messaging";
const INTERNAL_MESSAGING_EVENT_CHANNEL_SIZE: usize = 10;

const MAX_FRAME_LENGTH: usize = 8 * 1_024 * 1_024;

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
    ProtocolViolation { peer_node_id: NodeId, details: String },
}

impl fmt::Display for MessagingEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MessagingEvent::*;
        match self {
            MessageReceived(node_id, tag) => write!(f, "MessageReceived({}, {})", node_id, tag),
            OutboundProtocolExited(node_id) => write!(f, "OutboundProtocolExited({})", node_id),
            InboundProtocolExited(node_id) => write!(f, "InboundProtocolExited({})", node_id),
            ProtocolViolation { peer_node_id, details } => {
                write!(f, "ProtocolViolation({}, {})", peer_node_id, details)
            },
        }
    }
}

/// Actor responsible for lazily spawning inbound (protocol notifications) and outbound (mpsc channel) messaging actors.
pub struct MessagingProtocol {
    protocol_id: ProtocolId,
    connectivity: ConnectivityRequester,
    proto_notification: mpsc::Receiver<ProtocolNotification<Substream>>,
    active_queues: HashMap<NodeId, mpsc::UnboundedSender<OutboundMessage>>,
    active_inbound: HashMap<NodeId, JoinHandle<()>>,
    outbound_message_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    messaging_events_tx: MessagingEventSender,
    enable_message_received_event: bool,
    ban_duration: Option<Duration>,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
    internal_messaging_event_tx: mpsc::Sender<MessagingEvent>,
    internal_messaging_event_rx: mpsc::Receiver<MessagingEvent>,
    retry_queue_tx: mpsc::UnboundedSender<OutboundMessage>,
    retry_queue_rx: mpsc::UnboundedReceiver<OutboundMessage>,
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

        Self {
            protocol_id,
            connectivity,
            proto_notification,
            outbound_message_rx,
            active_inbound: Default::default(),
            active_queues: Default::default(),
            messaging_events_tx,
            enable_message_received_event: false,
            internal_messaging_event_rx,
            internal_messaging_event_tx,
            ban_duration: None,
            retry_queue_tx,
            retry_queue_rx,
            inbound_message_tx,
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
                            "Failed to retry outbound message because '{}'",
                            err
                        );
                    }
                },

                Some(msg) = self.outbound_message_rx.recv() => {
                    if let Err(err) = self.send_message(msg) {
                        error!(
                            target: LOG_TARGET,
                            "Failed to handle request because '{}'",
                            err
                        );
                    }
                },

                Some(notification) = self.proto_notification.recv() => {
                    self.handle_protocol_notification(notification);
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
        trace!(target: LOG_TARGET, "Internal messaging event '{}'", event);
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
            InboundProtocolExited(node_id) => {
                debug!(
                    target: LOG_TARGET,
                    "Inbound protocol handler exited for peer `{}`",
                    node_id.short_str()
                );
                if self.active_inbound.remove(node_id).is_none() {
                    debug!(
                        target: LOG_TARGET,
                        "InboundProtocolExited event, but MessagingProtocol has no record of the inbound protocol \
                         for peer `{}`",
                        node_id.short_str()
                    );
                }
            },
            ProtocolViolation { peer_node_id, details } => {
                self.ban_peer(peer_node_id.clone(), details.to_string()).await;
            },
            _ => {},
        }

        // Forward the event
        let _result = self.messaging_events_tx.send(event);
    }

    fn handle_retry_queue_messages(&mut self, msg: OutboundMessage) -> Result<(), MessagingProtocolError> {
        debug!(target: LOG_TARGET, "Retrying outbound message ({})", msg);
        self.send_message(msg)?;
        Ok(())
    }

    fn send_message(&mut self, out_msg: OutboundMessage) -> Result<(), MessagingProtocolError> {
        trace!(target: LOG_TARGET, "Received request to send message ({})", out_msg);
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

        debug!(target: LOG_TARGET, "Sending message {}", out_msg);
        let tag = out_msg.tag;
        match sender.send(out_msg) {
            Ok(_) => {
                debug!(target: LOG_TARGET, "Message ({}) dispatched to outbound handler", tag,);
                Ok(())
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to send message on channel because '{:?}'", err
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

    fn spawn_inbound_handler(&mut self, peer: NodeId, substream: Substream) {
        if let Some(handle) = self.active_inbound.get(&peer) {
            if handle.is_finished() {
                self.active_inbound.remove(&peer);
            } else {
                debug!(
                    target: LOG_TARGET,
                    "InboundMessaging for peer '{}' already exists", peer.short_str()
                );
                return;
            }
        }
        let messaging_events_tx = self.messaging_events_tx.clone();
        let inbound_message_tx = self.inbound_message_tx.clone();
        let inbound_messaging = InboundMessaging::new(
            peer.clone(),
            inbound_message_tx,
            messaging_events_tx,
            self.enable_message_received_event,
            self.shutdown_signal.clone(),
        );
        let handle = tokio::spawn(inbound_messaging.run(substream));
        self.active_inbound.insert(peer, handle);
    }

    fn handle_protocol_notification(&mut self, notification: ProtocolNotification<Substream>) {
        match notification.event {
            // Peer negotiated to speak the messaging protocol with us
            ProtocolEvent::NewInboundSubstream(node_id, substream) => {
                debug!(
                    target: LOG_TARGET,
                    "NewInboundSubstream for peer '{}'",
                    node_id.short_str()
                );

                self.spawn_inbound_handler(node_id, substream);
            },
        }
    }

    async fn ban_peer<T: Display>(&mut self, peer_node_id: NodeId, reason: T) {
        warn!(
            target: LOG_TARGET,
            "Banning peer '{}' because it violated the messaging protocol: {}", peer_node_id.short_str(), reason
        );
        if let Some(handle) = self.active_inbound.remove(&peer_node_id) {
            handle.abort();
        }
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

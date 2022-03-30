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
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use log::*;
use tari_shutdown::{Shutdown, ShutdownSignal};
use thiserror::Error;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
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
        ProtocolNotification,
    },
    runtime::task,
};

const LOG_TARGET: &str = "comms::protocol::messaging";
pub(super) static MESSAGING_PROTOCOL: Bytes = Bytes::from_static(b"t/msg/0.1");
const INTERNAL_MESSAGING_EVENT_CHANNEL_SIZE: usize = 150;

/// The maximum amount of inbound messages to accept within the `RATE_LIMIT_RESTOCK_INTERVAL` window
const RATE_LIMIT_CAPACITY: usize = 10;
const RATE_LIMIT_RESTOCK_INTERVAL: Duration = Duration::from_millis(100);
const MAX_FRAME_LENGTH: usize = 8 * 1_024 * 1_024;

pub type MessagingEventSender = broadcast::Sender<Arc<MessagingEvent>>;
pub type MessagingEventReceiver = broadcast::Receiver<Arc<MessagingEvent>>;

/// Request types for MessagingProtocol
#[derive(Debug)]
pub enum MessagingRequest {
    SendMessage(OutboundMessage),
}

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

#[derive(Debug)]
pub enum MessagingEvent {
    MessageReceived(NodeId, MessageTag),
    InvalidMessageReceived(NodeId),
    OutboundProtocolExited(NodeId),
}

impl fmt::Display for MessagingEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MessagingEvent::*;
        match self {
            MessageReceived(node_id, tag) => write!(f, "MessageReceived({}, {})", node_id.short_str(), tag),
            InvalidMessageReceived(node_id) => write!(f, "InvalidMessageReceived({})", node_id.short_str()),
            OutboundProtocolExited(node_id) => write!(f, "OutboundProtocolExited({})", node_id),
        }
    }
}

pub struct MessagingProtocol {
    connectivity: ConnectivityRequester,
    proto_notification: mpsc::Receiver<ProtocolNotification<Substream>>,
    active_queues: HashMap<NodeId, mpsc::UnboundedSender<OutboundMessage>>,
    request_rx: mpsc::Receiver<MessagingRequest>,
    messaging_events_tx: MessagingEventSender,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
    internal_messaging_event_tx: mpsc::Sender<MessagingEvent>,
    internal_messaging_event_rx: mpsc::Receiver<MessagingEvent>,
    retry_queue_tx: mpsc::UnboundedSender<OutboundMessage>,
    retry_queue_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    shutdown_signal: ShutdownSignal,
    complete_trigger: Shutdown,
}

impl MessagingProtocol {
    pub fn new(
        connectivity: ConnectivityRequester,
        proto_notification: mpsc::Receiver<ProtocolNotification<Substream>>,
        request_rx: mpsc::Receiver<MessagingRequest>,
        messaging_events_tx: MessagingEventSender,
        inbound_message_tx: mpsc::Sender<InboundMessage>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        let (internal_messaging_event_tx, internal_messaging_event_rx) =
            mpsc::channel(INTERNAL_MESSAGING_EVENT_CHANNEL_SIZE);
        let (retry_queue_tx, retry_queue_rx) = mpsc::unbounded_channel();

        Self {
            connectivity,
            proto_notification,
            request_rx,
            active_queues: Default::default(),
            messaging_events_tx,
            internal_messaging_event_rx,
            internal_messaging_event_tx,
            retry_queue_tx,
            retry_queue_rx,
            inbound_message_tx,
            shutdown_signal,
            complete_trigger: Shutdown::new(),
        }
    }

    pub fn complete_signal(&self) -> ShutdownSignal {
        self.complete_trigger.to_signal()
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self.shutdown_signal.clone();

        loop {
            tokio::select! {
                Some(event) = self.internal_messaging_event_rx.recv() => {
                    self.handle_internal_messaging_event(event).await;
                },

                Some(msg) = self.retry_queue_rx.recv() => {
                    if let Err(err) = self.handle_retry_queue_messages(msg).await {
                        error!(
                            target: LOG_TARGET,
                            "Failed to retry outbound message because '{}'",
                            err
                        );
                    }
                },

                Some(req) = self.request_rx.recv() => {
                    if let Err(err) = self.handle_request(req).await {
                        error!(
                            target: LOG_TARGET,
                            "Failed to handle request because '{}'",
                            err
                        );
                    }
                },

                Some(notification) = self.proto_notification.recv() => {
                    self.handle_protocol_notification(notification).await;
                },

                _ = &mut shutdown_signal => {
                    info!(target: LOG_TARGET, "MessagingProtocol is shutting down because the shutdown signal was triggered");
                    break;
                }
            }
        }
    }

    #[inline]
    pub fn framed<TSubstream>(socket: TSubstream) -> Framed<TSubstream, LengthDelimitedCodec>
    where TSubstream: AsyncRead + AsyncWrite + Unpin {
        framing::canonical(socket, MAX_FRAME_LENGTH)
    }

    async fn handle_internal_messaging_event(&mut self, event: MessagingEvent) {
        use MessagingEvent::*;
        trace!(target: LOG_TARGET, "Internal messaging event '{}'", event);
        match event {
            OutboundProtocolExited(node_id) => {
                debug!(
                    target: LOG_TARGET,
                    "Outbound protocol handler exited for peer `{}`",
                    node_id.short_str()
                );
                if self.active_queues.remove(&node_id).is_none() {
                    debug!(
                        target: LOG_TARGET,
                        "OutboundProtocolExited event, but MessagingProtocol has no record of the outbound protocol \
                         for peer `{}`",
                        node_id.short_str()
                    );
                }
                let _ = self.messaging_events_tx.send(Arc::new(OutboundProtocolExited(node_id)));
            },
            evt => {
                // Forward the event
                let _ = self.messaging_events_tx.send(Arc::new(evt));
            },
        }
    }

    async fn handle_request(&mut self, req: MessagingRequest) -> Result<(), MessagingProtocolError> {
        use MessagingRequest::*;
        match req {
            SendMessage(msg) => {
                trace!(target: LOG_TARGET, "Received request to send message ({})", msg);
                self.send_message(msg).await?;
            },
        }

        Ok(())
    }

    async fn handle_retry_queue_messages(&mut self, msg: OutboundMessage) -> Result<(), MessagingProtocolError> {
        debug!(target: LOG_TARGET, "Retrying outbound message ({})", msg);
        self.send_message(msg).await?;
        Ok(())
    }

    // #[tracing::instrument(skip(self, out_msg), err)]
    async fn send_message(&mut self, out_msg: OutboundMessage) -> Result<(), MessagingProtocolError> {
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
    ) -> mpsc::UnboundedSender<OutboundMessage> {
        let (msg_tx, msg_rx) = mpsc::unbounded_channel();
        let outbound_messaging = OutboundMessaging::new(connectivity, events_tx, msg_rx, retry_queue_tx, peer_node_id);
        task::spawn(outbound_messaging.run());
        msg_tx
    }

    fn spawn_inbound_handler(&mut self, peer: NodeId, substream: Substream) {
        let messaging_events_tx = self.messaging_events_tx.clone();
        let inbound_message_tx = self.inbound_message_tx.clone();
        let inbound_messaging = InboundMessaging::new(
            peer,
            inbound_message_tx,
            messaging_events_tx,
            RATE_LIMIT_CAPACITY,
            RATE_LIMIT_RESTOCK_INTERVAL,
        );
        task::spawn(inbound_messaging.run(substream));
    }

    async fn handle_protocol_notification(&mut self, notification: ProtocolNotification<Substream>) {
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
}

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

use super::error::MessagingProtocolError;
use crate::{
    compat::IoCompat,
    connectivity::{ConnectivityEvent, ConnectivityRequester},
    framing,
    message::{InboundMessage, MessageTag, OutboundMessage},
    multiplexing::Substream,
    peer_manager::NodeId,
    protocol::{
        messaging::{inbound::InboundMessaging, outbound::OutboundMessaging, MessagingConfig},
        ProtocolEvent,
        ProtocolNotification,
    },
    runtime::task,
};
use bytes::Bytes;
use futures::{channel::mpsc, stream::Fuse, AsyncRead, AsyncWrite, SinkExt, StreamExt};
use log::*;
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt,
    sync::Arc,
    time::Duration,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use thiserror::Error;
use tokio::sync::broadcast;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

const LOG_TARGET: &str = "comms::protocol::messaging";
pub(super) static MESSAGING_PROTOCOL: Bytes = Bytes::from_static(b"/tari/messaging/0.1.0");
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
    SendMessageFailed(OutboundMessage, SendFailReason),
    OutboundProtocolExited(NodeId),
}

impl fmt::Display for MessagingEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MessagingEvent::*;
        match self {
            MessageReceived(node_id, tag) => write!(f, "MessageReceived({}, {})", node_id.short_str(), tag),
            InvalidMessageReceived(node_id) => write!(f, "InvalidMessageReceived({})", node_id.short_str()),
            SendMessageFailed(out_msg, reason) => write!(f, "SendMessageFailed({}, Reason = {})", out_msg, reason),
            OutboundProtocolExited(node_id) => write!(f, "OutboundProtocolExited({})", node_id),
        }
    }
}

pub struct MessagingProtocol {
    config: MessagingConfig,
    connectivity: ConnectivityRequester,
    proto_notification: Fuse<mpsc::Receiver<ProtocolNotification<Substream>>>,
    active_queues: HashMap<NodeId, mpsc::UnboundedSender<OutboundMessage>>,
    request_rx: Fuse<mpsc::Receiver<MessagingRequest>>,
    messaging_events_tx: MessagingEventSender,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
    internal_messaging_event_tx: mpsc::Sender<MessagingEvent>,
    internal_messaging_event_rx: Fuse<mpsc::Receiver<MessagingEvent>>,
    shutdown_signal: ShutdownSignal,
    complete_trigger: Shutdown,
}

impl MessagingProtocol {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: MessagingConfig,
        connectivity: ConnectivityRequester,
        proto_notification: mpsc::Receiver<ProtocolNotification<Substream>>,
        request_rx: mpsc::Receiver<MessagingRequest>,
        messaging_events_tx: MessagingEventSender,
        inbound_message_tx: mpsc::Sender<InboundMessage>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        let (internal_messaging_event_tx, internal_messaging_event_rx) =
            mpsc::channel(INTERNAL_MESSAGING_EVENT_CHANNEL_SIZE);
        Self {
            config,
            connectivity,
            proto_notification: proto_notification.fuse(),
            request_rx: request_rx.fuse(),
            active_queues: Default::default(),
            messaging_events_tx,
            internal_messaging_event_rx: internal_messaging_event_rx.fuse(),
            internal_messaging_event_tx,
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
        let mut connectivity_events = self.connectivity.get_event_subscription().fuse();

        loop {
            futures::select! {
                event = self.internal_messaging_event_rx.select_next_some() => {
                    self.handle_internal_messaging_event(event).await;
                },

                req = self.request_rx.select_next_some() => {
                    if let Err(err) = self.handle_request(req).await {
                        error!(
                            target: LOG_TARGET,
                            "Failed to handle request because '{}'",
                            err
                        );
                    }
                },

                event = connectivity_events.select_next_some() => {
                    if let Ok(event) = event {
                        self.handle_connectivity_event(&event);
                    }
                }

                notification = self.proto_notification.select_next_some() => {
                    self.handle_protocol_notification(notification).await;
                },

                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "MessagingProtocol is shutting down because the shutdown signal was triggered");
                    break;
                }
            }
        }
    }

    #[inline]
    pub fn framed<TSubstream>(socket: TSubstream) -> Framed<IoCompat<TSubstream>, LengthDelimitedCodec>
    where TSubstream: AsyncRead + AsyncWrite + Unpin {
        framing::canonical(socket, MAX_FRAME_LENGTH)
    }

    fn handle_connectivity_event(&mut self, event: &ConnectivityEvent) {
        use ConnectivityEvent::*;
        #[allow(clippy::single_match)]
        match event {
            PeerConnectionWillClose(node_id, _) => {
                // If the peer connection will close, cut off the pipe to send further messages.
                // Any messages in the channel will be sent (hopefully) before the connection is disconnected.
                if let Some(sender) = self.active_queues.remove(node_id) {
                    sender.close_channel();
                }
            },
            _ => {},
        }
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
                        peer_node_id.clone(),
                        self.config.inactivity_timeout,
                    );
                    break entry.insert(sender);
                },
            }
        };

        debug!(target: LOG_TARGET, "Sending message {}", out_msg);
        let tag = out_msg.tag;
        match sender.send(out_msg).await {
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
        inactivity_timeout: Option<Duration>,
    ) -> mpsc::UnboundedSender<OutboundMessage>
    {
        let (msg_tx, msg_rx) = mpsc::unbounded();
        let outbound_messaging =
            OutboundMessaging::new(connectivity, events_tx, msg_rx, peer_node_id, inactivity_timeout);
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
            self.config.inactivity_timeout,
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

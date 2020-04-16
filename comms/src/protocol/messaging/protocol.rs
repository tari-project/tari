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
    connection_manager::{ConnectionManagerEvent, ConnectionManagerRequester},
    message::{InboundMessage, MessageTag, OutboundMessage},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerManagerError},
    protocol::{messaging::outbound::OutboundMessaging, ProtocolEvent, ProtocolNotification},
    runtime::current_executor,
    types::CommsSubstream,
    PeerManager,
};
use bytes::Bytes;
use derive_error::Error;
use futures::{channel::mpsc, stream::Fuse, AsyncRead, AsyncWrite, SinkExt, StreamExt};
use log::*;
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt,
    sync::Arc,
};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{runtime, sync::broadcast};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

const LOG_TARGET: &str = "comms::protocol::messaging";
pub static MESSAGING_PROTOCOL: Bytes = Bytes::from_static(b"/tari/messaging/0.1.0");
const INTERNAL_MESSAGING_EVENT_CHANNEL_SIZE: usize = 50;

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
    /// Dial was not attempted because the peer is offline
    PeerOffline,
    /// Dial was attempted, but failed
    PeerDialFailed,
    /// Failed to open a messaging substream to peer
    SubstreamOpenFailed,
    /// Failed to send on substream channel
    SubstreamSendFailed,
}

#[derive(Debug)]
pub enum MessagingEvent {
    MessageReceived(Box<NodeId>, MessageTag),
    InvalidMessageReceived(Box<NodeId>),
    SendMessageFailed(OutboundMessage, SendFailReason),
    MessageSent(MessageTag),
}

impl fmt::Display for MessagingEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use MessagingEvent::*;
        match self {
            MessageReceived(node_id, tag) => write!(f, "MessageReceived({}, {})", node_id.short_str(), tag),
            InvalidMessageReceived(node_id) => write!(f, "InvalidMessageReceived({})", node_id.short_str()),
            SendMessageFailed(out_msg, reason) => write!(f, "SendMessageFailed({}, Reason = {})", out_msg, reason),
            MessageSent(tag) => write!(f, "SendMessageSucceeded({})", tag),
        }
    }
}

pub struct MessagingProtocol {
    executor: runtime::Handle,
    connection_manager_requester: ConnectionManagerRequester,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    proto_notification: Fuse<mpsc::Receiver<ProtocolNotification<CommsSubstream>>>,
    active_queues: HashMap<Box<NodeId>, mpsc::UnboundedSender<OutboundMessage>>,
    request_rx: Fuse<mpsc::Receiver<MessagingRequest>>,
    messaging_events_tx: MessagingEventSender,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
    internal_messaging_event_tx: mpsc::Sender<MessagingEvent>,
    internal_messaging_event_rx: Fuse<mpsc::Receiver<MessagingEvent>>,
    retry_queue_tx: mpsc::UnboundedSender<OutboundMessage>,
    retry_queue_rx: Fuse<mpsc::UnboundedReceiver<OutboundMessage>>,
    attempts: HashMap<MessageTag, usize>,
    max_attempts: usize,
    shutdown_signal: Option<ShutdownSignal>,
    complete_trigger: Shutdown,
}

impl MessagingProtocol {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        connection_manager_requester: ConnectionManagerRequester,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        proto_notification: mpsc::Receiver<ProtocolNotification<CommsSubstream>>,
        request_rx: mpsc::Receiver<MessagingRequest>,
        messaging_events_tx: MessagingEventSender,
        inbound_message_tx: mpsc::Sender<InboundMessage>,
        max_attempts: usize,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        let (internal_messaging_event_tx, internal_messaging_event_rx) =
            mpsc::channel(INTERNAL_MESSAGING_EVENT_CHANNEL_SIZE);
        let (retry_queue_tx, retry_queue_rx) = mpsc::unbounded();
        Self {
            executor: current_executor(),
            connection_manager_requester,
            peer_manager,
            node_identity,
            proto_notification: proto_notification.fuse(),
            request_rx: request_rx.fuse(),
            active_queues: Default::default(),
            messaging_events_tx,
            internal_messaging_event_rx: internal_messaging_event_rx.fuse(),
            internal_messaging_event_tx,
            inbound_message_tx,
            retry_queue_rx: retry_queue_rx.fuse(),
            retry_queue_tx,
            shutdown_signal: Some(shutdown_signal),
            max_attempts,
            attempts: Default::default(),
            complete_trigger: Shutdown::new(),
        }
    }

    pub fn complete_signal(&self) -> ShutdownSignal {
        self.complete_trigger.to_signal()
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("Messaging initialized without shutdown_signal");

        let mut conn_man_events = self.connection_manager_requester.get_event_subscription().fuse();

        loop {
            futures::select! {
                event = conn_man_events.select_next_some() => {
                    if let Some(event) = log_if_error!(target: LOG_TARGET, event, "Event error: '{error}'",) {
                        self.handle_conn_man_event(event).await;
                    }
                },
                event = self.internal_messaging_event_rx.select_next_some() => {
                    self.handle_internal_messaging_event(event).await;
                },
                out_msg = self.retry_queue_rx.select_next_some() => {
                    log_if_error!(target: LOG_TARGET, self.send_message(out_msg).await, "Failed to send message {error}",);
                },
                req = self.request_rx.select_next_some() => {
                    log_if_error!(
                        target: LOG_TARGET,
                        self.handle_request(req).await,
                        "Failed to handle request because '{error}'",
                    );
                },
                notification = self.proto_notification.select_next_some() => {
                    self.handle_notification(notification).await;
                },
                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "MessagingProtocol is shutting down because the shutdown signal was triggered");
                    break;
                }
                complete => {
                    info!(target: LOG_TARGET, "MessagingProtocol is shutting down because all streams have completed");
                    break;
                }
            }
        }
    }

    pub fn framed<TSubstream>(socket: TSubstream) -> Framed<IoCompat<TSubstream>, LengthDelimitedCodec>
    where TSubstream: AsyncRead + AsyncWrite + Unpin {
        Framed::new(IoCompat::new(socket), LengthDelimitedCodec::new())
    }

    async fn handle_internal_messaging_event(&mut self, event: MessagingEvent) {
        use MessagingEvent::*;
        trace!(target: LOG_TARGET, "Internal messaging event '{}'", event);
        match event {
            SendMessageFailed(out_msg, reason) => match self.attempts.entry(out_msg.tag) {
                Entry::Occupied(mut entry) => match *entry.get() {
                    n if n >= self.max_attempts => {
                        warn!(
                            target: LOG_TARGET,
                            "Failed to send message '{}' to peer '{}' because '{}'.",
                            out_msg.tag,
                            out_msg.peer_node_id.short_str(),
                            reason
                        );
                        let _ = self
                            .messaging_events_tx
                            .send(Arc::new(SendMessageFailed(out_msg, reason)));
                    },
                    n => {
                        self.retry_queue_tx.send(out_msg).await.expect(
                            "retry_queue send cannot fail because the channel sender and receiver are contained in \
                             and dropped with MessagingProtocol",
                        );
                        *entry.get_mut() = n + 1;
                    },
                },
                Entry::Vacant(entry) => {
                    if self.max_attempts == 0 {
                        let _ = self
                            .messaging_events_tx
                            .send(Arc::new(SendMessageFailed(out_msg, reason)));
                    } else {
                        self.retry_queue_tx.send(out_msg).await.expect(
                            "retry_queue send cannot fail because the channel sender and receiver are contained in \
                             and dropped with MessagingProtocol",
                        );
                        // 2 = 1 first attempt + 1 attempt added to the queue
                        entry.insert(2);
                    }
                },
            },
            MessageSent(tag) => {
                self.attempts.remove(&tag);
                let _ = self.messaging_events_tx.send(Arc::new(MessageSent(tag)));
            },
            evt => {
                // Forward the event
                let _ = self.messaging_events_tx.send(Arc::new(evt));
            },
        }
    }

    async fn handle_conn_man_event(&mut self, event: Arc<ConnectionManagerEvent>) {
        trace!(target: LOG_TARGET, "ConnectionManagerEvent: {:?}", event);
        use ConnectionManagerEvent::*;
        match &*event {
            PeerDisconnected(node_id) => {
                if self.active_queues.remove(node_id).is_some() {
                    debug!(
                        target: LOG_TARGET,
                        "Removing active queue because peer '{}' disconnected",
                        node_id.short_str()
                    );
                }
            },
            PeerConnectWillClose(_, node_id, direction) => {
                if let Some(sender) = self.active_queues.remove(node_id) {
                    sender.close_channel();
                    debug!(
                        target: LOG_TARGET,
                        "Removing active queue because {} peer connection '{}' will close",
                        direction,
                        node_id.short_str()
                    );
                }
            },

            _ => {},
        }
    }

    async fn handle_request(&mut self, req: MessagingRequest) -> Result<(), MessagingProtocolError> {
        use MessagingRequest::*;
        match req {
            SendMessage(msg) => {
                if let Err(err) = self.send_message(msg).await {
                    debug!(
                        target: LOG_TARGET,
                        "MessagingProtocol encountered an error when sending a message: {}", err
                    );
                }
            },
        }

        Ok(())
    }

    async fn send_message(&mut self, out_msg: OutboundMessage) -> Result<(), MessagingProtocolError> {
        let peer_node_id = out_msg.peer_node_id.clone();
        let sender = loop {
            match self.active_queues.entry(Box::new(peer_node_id.clone())) {
                Entry::Occupied(entry) => {
                    if entry.get().is_closed() {
                        entry.remove();
                        continue;
                    }
                    break entry.into_mut();
                },
                Entry::Vacant(entry) => {
                    let sender = Self::spawn_outbound_handler(
                        self.executor.clone(),
                        self.node_identity.clone(),
                        self.connection_manager_requester.clone(),
                        self.internal_messaging_event_tx.clone(),
                        peer_node_id.clone(),
                    )
                    .await?;
                    break entry.insert(sender);
                },
            }
        };

        match sender.send(out_msg).await {
            Ok(_) => Ok(()),
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to send message on channel because '{:?}'", err
                );
                // Lazily remove Senders from the active queue if the `OutboundMessaging` task has shut down
                if err.is_disconnected() {
                    self.active_queues.remove(&peer_node_id);
                }
                Err(MessagingProtocolError::MessageSendFailed)
            },
        }
    }

    async fn spawn_outbound_handler(
        executor: runtime::Handle,
        our_node_identity: Arc<NodeIdentity>,
        conn_man_requester: ConnectionManagerRequester,
        events_tx: mpsc::Sender<MessagingEvent>,
        peer_node_id: NodeId,
    ) -> Result<mpsc::UnboundedSender<OutboundMessage>, MessagingProtocolError>
    {
        let (msg_tx, msg_rx) = mpsc::unbounded();
        executor.spawn(
            OutboundMessaging::new(conn_man_requester, our_node_identity, events_tx, msg_rx, peer_node_id).run(),
        );
        Ok(msg_tx)
    }

    async fn spawn_inbound_handler(&mut self, peer: Arc<Peer>, substream: CommsSubstream) {
        let messaging_events_tx = self.messaging_events_tx.clone();
        let mut inbound_message_tx = self.inbound_message_tx.clone();
        let mut framed_substream = Self::framed(substream);

        self.executor.spawn(async move {
            while let Some(result) = framed_substream.next().await {
                match result {
                    Ok(raw_msg) => {
                        trace!(
                            target: LOG_TARGET,
                            "Received message from peer '{}' ({} bytes)",
                            peer.node_id.short_str(),
                            raw_msg.len()
                        );

                        let inbound_msg = InboundMessage::new(Arc::clone(&peer), raw_msg.freeze());

                        let event = MessagingEvent::MessageReceived(
                            Box::new(inbound_msg.source_peer.node_id.clone()),
                            inbound_msg.tag,
                        );

                        if let Err(err) = inbound_message_tx.send(inbound_msg).await {
                            warn!(
                                target: LOG_TARGET,
                                "Failed to send InboundMessage for peer '{}' because '{}'",
                                peer.node_id.short_str(),
                                err
                            );

                            if err.is_disconnected() {
                                break;
                            }
                        }

                        trace!(target: LOG_TARGET, "Inbound handler sending event '{}'", event);
                        if let Err(err) = messaging_events_tx.send(Arc::new(event)) {
                            debug!(
                                target: LOG_TARGET,
                                "Messaging event '{}' not sent for peer '{}' because there are no subscribers. \
                                 MessagingEvent dropped",
                                err.0,
                                peer.node_id.short_str(),
                            );
                        }
                    },
                    Err(err) => {
                        error!(
                            target: LOG_TARGET,
                            "Failed to receive from peer '{}' because '{}'",
                            peer.node_id.short_str(),
                            err
                        );
                        break;
                    },
                }
            }

            debug!(
                target: LOG_TARGET,
                "Inbound messaging handler for peer '{}' has stopped",
                peer.node_id.short_str()
            );
        });
    }

    async fn handle_notification(&mut self, notification: ProtocolNotification<CommsSubstream>) {
        debug_assert_eq!(notification.protocol, MESSAGING_PROTOCOL);
        match notification.event {
            // Peer negotiated to speak the messaging protocol with us
            ProtocolEvent::NewInboundSubstream(node_id, substream) => {
                debug!(
                    target: LOG_TARGET,
                    "NewInboundSubstream for peer '{}'",
                    node_id.short_str()
                );
                match self.peer_manager.find_by_node_id(&node_id).await {
                    Ok(peer) => {
                        // For an inbound substream, read messages from the peer and forward on the incoming_messages
                        // channel
                        self.spawn_inbound_handler(Arc::new(peer), substream).await;
                    },
                    Err(PeerManagerError::PeerNotFoundError) => {
                        // This should never happen if everything is working correctly

                        error!(
                            target: LOG_TARGET,
                            "[ThisNode={}] *** Could not find verified node_id '{}' in peer list. This should not \
                             happen ***",
                            self.node_identity.node_id().short_str(),
                            node_id.short_str()
                        );
                    },
                    Err(err) => {
                        // This should never happen if everything is working correctly
                        error!(
                            target: LOG_TARGET,
                            "Peer manager error when handling protocol notification: '{:?}'", err
                        );
                    },
                }
            },
        }
    }
}

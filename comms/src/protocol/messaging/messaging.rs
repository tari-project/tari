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
    connection_manager::next::ConnectionManagerRequester,
    message::InboundMessage,
    outbound_message_service::{MessageTag, OutboundMessage},
    peer_manager::{AsyncPeerManager, NodeId, NodeIdentity},
    protocol::{
        messaging::{inbound::InboundMessaging, outbound::OutboundMessaging},
        ProtocolEvent,
        ProtocolNotification,
    },
    types::CommsSubstream,
};
use bitflags::_core::fmt::{Error, Formatter};
use bytes::Bytes;
use futures::{channel::mpsc, stream::Fuse, AsyncRead, AsyncWrite, SinkExt, StreamExt};
use log::*;
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt,
    sync::Arc,
    time::Duration,
};
use tokio::{runtime, sync::broadcast, time::delay_for};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

const LOG_TARGET: &str = "comms::protocol::messaging";
pub const MESSAGING_PROTOCOL: Bytes = Bytes::from_static(b"/tari/messaging/0.1.0");
const MAX_SEND_MSG_ATTEMPTS: usize = 4;
/// The size of the buffered channel used for _each_ peer's message queue
const MESSAGE_QUEUE_BUF_SIZE: usize = 20;

pub type MessagingEventSender = broadcast::Sender<Arc<MessagingEvent>>;
pub type MessagingEventReceiver = broadcast::Receiver<Arc<MessagingEvent>>;

/// Request types for MessagingProtocol
#[derive(Debug)]
pub enum MessagingRequest {
    SendMessage(OutboundMessage),
}

#[derive(Debug)]
pub enum MessagingEvent {
    MessageReceived(Box<NodeId>, MessageTag),
    InvalidMessageReceived(Box<NodeId>),
    SendMessageFailed(OutboundMessage),
    MessageSent(MessageTag),
}

impl fmt::Display for MessagingEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        use MessagingEvent::*;
        match self {
            MessageReceived(node_id, tag) => {
                write!(f, "MessagingEvent::MessageReceived({}, {})", node_id.short_str(), tag)
            },
            InvalidMessageReceived(node_id) => {
                write!(f, "MessagingEvent::InvalidMessageReceived({})", node_id.short_str())
            },
            SendMessageFailed(out_msg) => write!(f, "MessagingEvent::SendMessageFailed({})", out_msg),
            MessageSent(tag) => write!(f, "MessagingEvent::SendMessageSucceeded({})", tag),
        }
    }
}

pub struct MessagingProtocol {
    executor: runtime::Handle,
    connection_manager_requester: ConnectionManagerRequester,
    node_identity: Arc<NodeIdentity>,
    peer_manager: AsyncPeerManager,
    proto_notification: Fuse<mpsc::Receiver<ProtocolNotification<CommsSubstream>>>,
    active_queues: HashMap<Box<NodeId>, mpsc::Sender<OutboundMessage>>,
    request_rx: Fuse<mpsc::Receiver<MessagingRequest>>,
    messaging_events_tx: MessagingEventSender,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
}

impl MessagingProtocol {
    pub fn new(
        executor: runtime::Handle,
        connection_manager_requester: ConnectionManagerRequester,
        peer_manager: AsyncPeerManager,
        node_identity: Arc<NodeIdentity>,
        proto_notification: mpsc::Receiver<ProtocolNotification<CommsSubstream>>,
        request_rx: mpsc::Receiver<MessagingRequest>,
        messaging_events_tx: MessagingEventSender,
        inbound_message_tx: mpsc::Sender<InboundMessage>,
    ) -> Self
    {
        Self {
            executor,
            connection_manager_requester,
            peer_manager,
            node_identity,
            proto_notification: proto_notification.fuse(),
            request_rx: request_rx.fuse(),
            active_queues: HashMap::new(),
            messaging_events_tx,
            inbound_message_tx,
        }
    }

    pub async fn run(mut self) {
        loop {
            futures::select! {
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
        let sender = match self.active_queues.entry(Box::new(out_msg.peer_node_id.clone())) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let sender = Self::spawn_outbound_handler(
                    self.executor.clone(),
                    self.node_identity.clone(),
                    self.connection_manager_requester.clone(),
                    self.messaging_events_tx.clone(),
                    out_msg.peer_node_id.clone(),
                )
                .await?;
                entry.insert(sender)
            },
        };

        let mut attempts = 0;
        loop {
            match sender.send(out_msg.clone()).await {
                Ok(_) => {
                    return Ok(());
                },
                Err(err) => {
                    // Lazily remove Senders from the active queue if the MessagingProtocolHandler has shut down
                    if err.is_disconnected() {
                        self.active_queues.remove(&out_msg.peer_node_id);
                        break;
                    }

                    attempts += 1;
                    if attempts > MAX_SEND_MSG_ATTEMPTS {
                        break;
                    }

                    // The queue is full. Retry after a slight delay
                    delay_for(Duration::from_millis(100)).await;
                },
            }
        }

        Err(MessagingProtocolError::MessageSendFailed(out_msg))
    }

    async fn spawn_outbound_handler(
        executor: runtime::Handle,
        our_node_identity: Arc<NodeIdentity>,
        conn_man_requester: ConnectionManagerRequester,
        events_tx: MessagingEventSender,
        peer_node_id: NodeId,
    ) -> Result<mpsc::Sender<OutboundMessage>, MessagingProtocolError>
    {
        let (msg_tx, msg_rx) = mpsc::channel(MESSAGE_QUEUE_BUF_SIZE);
        executor.spawn(
            OutboundMessaging::new(conn_man_requester, our_node_identity, events_tx, msg_rx, peer_node_id).run(),
        );
        Ok(msg_tx)
    }

    async fn spawn_inbound_handler(&mut self, node_id: Box<NodeId>, substream: CommsSubstream) {
        let messaging_events_tx = self.messaging_events_tx.clone();
        let mut inbound_message_tx = self.inbound_message_tx.clone();
        let mut framed_substream = Self::framed(substream);
        let mut inbound = InboundMessaging::new(self.peer_manager.clone());

        self.executor.spawn(async move {
            while let Some(result) = framed_substream.next().await {
                match result {
                    Ok(raw_msg) => {
                        let mut raw_msg = raw_msg.freeze();
                        let (event, in_msg) = match inbound.process_message(&node_id, &mut raw_msg).await {
                            Ok(inbound_msg) => (
                                MessagingEvent::MessageReceived(
                                    Box::new(inbound_msg.source_peer.node_id.clone()),
                                    inbound_msg.tag,
                                ),
                                Some(inbound_msg),
                            ),
                            Err(err) => {
                                // TODO: #banheuristic
                                warn!(
                                    target: LOG_TARGET,
                                    "Received invalid message from peer '{}' ({})",
                                    node_id.short_str(),
                                    err
                                );
                                (MessagingEvent::InvalidMessageReceived(node_id.clone()), None)
                            },
                        };

                        if let Some(in_msg) = in_msg {
                            if let Err(err) = inbound_message_tx.send(in_msg).await {
                                warn!(
                                    target: LOG_TARGET,
                                    "Failed to send InboundMessage for peer '{}' because '{}'",
                                    node_id.short_str(),
                                    err
                                );

                                if err.is_disconnected() {
                                    break;
                                }
                            }
                        }

                        if let Err(err) = messaging_events_tx.send(Arc::new(event)) {
                            debug!(
                                target: LOG_TARGET,
                                "Messaging event '{}' not sent for peer '{}' because there are no subscribers. \
                                 MessagingEvent dropped",
                                err.0,
                                node_id.short_str(),
                            );
                        }
                    },
                    Err(err) => debug!(
                        target: LOG_TARGET,
                        "Failed to receive from peer '{}' because '{}'",
                        node_id.short_str(),
                        err
                    ),
                }
            }

            trace!(
                target: LOG_TARGET,
                "Inbound messaging handler for peer '{}' has stopped",
                node_id.short_str()
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
                // For an inbound substream, read messages from the peer and forward on the incoming_messages channel
                self.spawn_inbound_handler(node_id, substream).await;
            },
        }
    }
}

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
use bytes::Bytes;
use futures::{
    channel::{mpsc, oneshot},
    stream::Fuse,
    AsyncRead,
    AsyncWrite,
    SinkExt,
    StreamExt,
};
use log::*;
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::Arc,
    time::Duration,
};
use tokio::{runtime, time::delay_for};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

const LOG_TARGET: &str = "comms::protocol::messaging";
pub const PROTOCOL_MESSAGING: Bytes = Bytes::from_static(b"/tari/messaging/0.1.0");
const MAX_SEND_MSG_ATTEMPTS: usize = 4;
/// The size of the buffered channel used for _each_ peer's message queue
const MESSAGE_QUEUE_BUF_SIZE: usize = 20;

/// Request types for MessagingProtocol
pub enum MessagingRequest {
    SendMessage(OutboundMessage, oneshot::Sender<Result<(), MessagingProtocolError>>),
}

pub enum MessagingEvent {
    MessageReceived(Box<NodeId>, InboundMessage),
    InvalidMessageReceived(Box<NodeId>),
    SendMessageFailed(OutboundMessage),
    SendMessageSucceeded(MessageTag),
}

pub struct Messaging {
    executor: runtime::Handle,
    connection_manager_requester: ConnectionManagerRequester,
    node_identity: Arc<NodeIdentity>,
    peer_manager: AsyncPeerManager,
    proto_notification: Fuse<mpsc::Receiver<ProtocolNotification<CommsSubstream>>>,
    active_queues: HashMap<Box<NodeId>, mpsc::Sender<OutboundMessage>>,
    request_rx: Fuse<mpsc::Receiver<MessagingRequest>>,
    messaging_events_tx: mpsc::Sender<MessagingEvent>,
}

impl Messaging {
    pub fn new(
        executor: runtime::Handle,
        connection_manager_requester: ConnectionManagerRequester,
        peer_manager: AsyncPeerManager,
        node_identity: Arc<NodeIdentity>,
        proto_notification: mpsc::Receiver<ProtocolNotification<CommsSubstream>>,
        request_rx: mpsc::Receiver<MessagingRequest>,
        messaging_events_tx: mpsc::Sender<MessagingEvent>,
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
            SendMessage(msg, reply_tx) => {
                let _ = reply_tx.send(self.send_message(msg).await);
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
        events_tx: mpsc::Sender<MessagingEvent>,
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
        let mut incoming_events_tx = self.messaging_events_tx.clone();
        let mut framed_substream = Self::framed(substream);
        let mut inbound = InboundMessaging::new(self.peer_manager.clone());

        self.executor.spawn(async move {
            while let Some(result) = framed_substream.next().await {
                match result {
                    Ok(raw_msg) => {
                        let mut raw_msg = raw_msg.freeze();
                        let event = match inbound.process_message(&node_id, &mut raw_msg).await {
                            Ok(inbound_msg) => MessagingEvent::MessageReceived(node_id.clone(), inbound_msg),
                            Err(err) => {
                                // TODO: #banheuristic
                                warn!(
                                    target: LOG_TARGET,
                                    "Received invalid message from peer '{}' ({})",
                                    node_id.short_str(),
                                    err
                                );
                                MessagingEvent::InvalidMessageReceived(node_id.clone())
                            },
                        };

                        if let Err(err) = incoming_events_tx.send(event).await {
                            warn!(
                                target: LOG_TARGET,
                                "Failed to forward incoming message for peer '{}' because '{}'",
                                node_id.short_str(),
                                err
                            );
                            if err.is_disconnected() {
                                break;
                            }
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

            debug!(
                target: LOG_TARGET,
                "Inbound messaging handler for peer '{}' has stopped",
                node_id.short_str()
            );
        });
    }

    async fn handle_notification(&mut self, notification: ProtocolNotification<CommsSubstream>) {
        debug_assert_eq!(notification.protocol, PROTOCOL_MESSAGING);
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

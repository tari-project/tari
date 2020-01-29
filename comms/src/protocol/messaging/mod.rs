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

use crate::{
    compat::IoCompat,
    connection_manager::{
        next::{ConnectionManagerError, ConnectionManagerRequester, PeerConnectionError},
        PeerConnection,
    },
    peer_manager::NodeId,
    protocol::{ProtocolError, ProtocolEvent, ProtocolNotification},
    types::CommsSubstream,
};
use bytes::Bytes;
use derive_error::Error;
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
    time::Duration,
};
use tokio::{runtime, time::delay_for};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

const LOG_TARGET: &str = "comms::protocol::messaging";
pub const PROTOCOL_MESSAGING: Bytes = Bytes::from_static(b"/tari/messaging/0.1.0");
const MAX_SEND_MSG_ATTEMPTS: usize = 4;
/// The size of the buffered channel used for _each_ peer's message queue
const MESSAGE_QUEUE_BUF_SIZE: usize = 10;

pub enum MessagingRequest {
    SendMessage(Box<NodeId>, Bytes, oneshot::Sender<Result<(), MessagingProtocolError>>),
}

pub enum MessagingEvent {
    MessageReceived(Box<NodeId>, Bytes),
}

#[derive(Debug, Error)]
pub enum MessagingProtocolError {
    /// Attempt to establish messaging protocol with a peer to which this node is not connected
    PeerNotConnected,
    #[error(no_from)]
    UnexpectedConnectionManagerError(ConnectionManagerError),
    /// Failed to send message
    MessageSendFailed,
    ProtocolError(ProtocolError),
    PeerConnectionError(PeerConnectionError),
}

pub struct MessagingProtocol {
    executor: runtime::Handle,
    connection_manager_requester: ConnectionManagerRequester,
    proto_notification: Fuse<mpsc::Receiver<ProtocolNotification<CommsSubstream>>>,
    active_queues: HashMap<Box<NodeId>, mpsc::Sender<Bytes>>,
    request_rx: Fuse<mpsc::Receiver<MessagingRequest>>,
    messaging_events_tx: mpsc::Sender<MessagingEvent>,
}

impl MessagingProtocol {
    pub fn new(
        executor: runtime::Handle,
        connection_manager_requester: ConnectionManagerRequester,
        proto_notification: mpsc::Receiver<ProtocolNotification<CommsSubstream>>,
        request_rx: mpsc::Receiver<MessagingRequest>,
        messaging_events_tx: mpsc::Sender<MessagingEvent>,
    ) -> Self
    {
        Self {
            executor,
            connection_manager_requester,
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
            SendMessage(node_id, msg, reply_tx) => {
                let _ = reply_tx.send(self.send_message(node_id, msg).await);
            },
        }

        Ok(())
    }

    async fn send_message(&mut self, node_id: Box<NodeId>, msg: Bytes) -> Result<(), MessagingProtocolError> {
        let sender = match self.active_queues.entry(node_id.clone()) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                match self
                    .connection_manager_requester
                    .get_active_connection(*node_id.clone())
                    .await
                    .map_err(MessagingProtocolError::UnexpectedConnectionManagerError)?
                {
                    Some(conn) => {
                        let sender = Self::spawn_outbound_handler(self.executor.clone(), conn).await?;
                        entry.insert(sender)
                    },
                    None => {
                        // Should have an active connection before sending a message
                        error!(
                            target: LOG_TARGET,
                            "Attempted to send a message to peer '{}' that is not connected", node_id
                        );
                        return Err(MessagingProtocolError::PeerNotConnected);
                    },
                }
            },
        };

        let mut attempts = 0;
        loop {
            match sender.send(msg.clone()).await {
                Ok(_) => {
                    return Ok(());
                },
                Err(err) => {
                    // Lazily remove Senders from the active queue if the MessagingProtocolHandler has shut down
                    if err.is_disconnected() {
                        self.active_queues.remove(&node_id);
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

        Err(MessagingProtocolError::MessageSendFailed)
    }

    async fn spawn_outbound_handler(
        executor: runtime::Handle,
        mut conn: PeerConnection,
    ) -> Result<mpsc::Sender<Bytes>, MessagingProtocolError>
    {
        let substream = conn.open_substream(PROTOCOL_MESSAGING).await?;
        debug_assert_eq!(substream.protocol, PROTOCOL_MESSAGING);
        let (msg_tx, msg_rx) = mpsc::channel(MESSAGE_QUEUE_BUF_SIZE);
        let framed_substream = Self::framed(substream.stream);
        let node_id = conn.peer_node_id().clone();

        // Forward all messages on the channel to the peer
        executor.spawn(async move {
            if let Err(err) = msg_rx.map(Ok).forward(framed_substream).await {
                debug!(
                    target: LOG_TARGET,
                    "Failed to send message to peer '{}' because '{}'",
                    node_id.short_str(),
                    err
                )
            }
        });
        Ok(msg_tx)
    }

    async fn spawn_inbound_handler(&mut self, node_id: Box<NodeId>, substream: CommsSubstream) {
        let mut incoming_events_tx = self.messaging_events_tx.clone();
        let mut framed_substream = Framed::new(IoCompat::new(substream), LengthDelimitedCodec::new());

        self.executor.spawn(async move {
            while let Some(result) = framed_substream.next().await {
                match result {
                    Ok(msg) => {
                        let event = MessagingEvent::MessageReceived(node_id.clone(), msg.freeze());
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
                "Inbound messaging handler for peer '{}' stopped",
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

#[cfg(test)]
mod test;

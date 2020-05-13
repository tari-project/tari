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

use super::{error::MessagingProtocolError, MessagingEvent, MessagingProtocol, SendFailReason, MESSAGING_PROTOCOL};
use crate::{
    connection_manager::{ConnectionManagerError, ConnectionManagerRequester, NegotiatedSubstream, PeerConnection},
    message::OutboundMessage,
    multiplexing::Substream,
    peer_manager::{NodeId, NodeIdentity},
};
use futures::{channel::mpsc, SinkExt, StreamExt};
use log::*;
use std::sync::Arc;

const LOG_TARGET: &str = "comms::protocol::messaging::outbound";

pub struct OutboundMessaging {
    conn_man_requester: ConnectionManagerRequester,
    node_identity: Arc<NodeIdentity>,
    request_rx: mpsc::UnboundedReceiver<OutboundMessage>,
    messaging_events_tx: mpsc::Sender<MessagingEvent>,
    peer_node_id: NodeId,
}

impl OutboundMessaging {
    pub fn new(
        conn_man_requester: ConnectionManagerRequester,
        node_identity: Arc<NodeIdentity>,
        messaging_events_tx: mpsc::Sender<MessagingEvent>,
        request_rx: mpsc::UnboundedReceiver<OutboundMessage>,
        peer_node_id: NodeId,
    ) -> Self
    {
        Self {
            conn_man_requester,
            node_identity,
            request_rx,
            messaging_events_tx,
            peer_node_id,
        }
    }

    pub async fn run(mut self) -> Result<(), MessagingProtocolError> {
        debug!(
            target: LOG_TARGET,
            "Attempting to dial peer '{}' if required",
            self.peer_node_id.short_str()
        );
        let conn = self.try_dial_peer().await?;
        let substream = self.try_open_substream(conn).await?;
        debug_assert_eq!(substream.protocol, MESSAGING_PROTOCOL);
        self.start_forwarding_messages(substream.stream).await?;

        Ok(())
    }

    async fn try_dial_peer(&mut self) -> Result<PeerConnection, MessagingProtocolError> {
        loop {
            match self.conn_man_requester.dial_peer(self.peer_node_id.clone()).await {
                Ok(conn) => break Ok(conn),
                Err(ConnectionManagerError::DialCancelled) => {
                    error!(
                        target: LOG_TARGET,
                        "Dial was cancelled for peer '{}'. This is probably because of connection tie-breaking. \
                         Retrying...",
                        self.peer_node_id.short_str(),
                    );
                    continue;
                },
                Err(err) => {
                    error!(
                        target: LOG_TARGET,
                        "MessagingProtocol failed to dial peer '{}' because '{:?}'",
                        self.peer_node_id.short_str(),
                        err
                    );
                    self.flush_all_messages_to_failed_event(SendFailReason::PeerDialFailed)
                        .await;
                    break Err(MessagingProtocolError::PeerDialFailed);
                },
            }
        }
    }

    async fn try_open_substream(
        &mut self,
        mut conn: PeerConnection,
    ) -> Result<NegotiatedSubstream<Substream>, MessagingProtocolError>
    {
        match conn.open_substream(&MESSAGING_PROTOCOL).await {
            Ok(substream) => Ok(substream),
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "MessagingProtocol failed to open a substream to peer '{}' because '{:?}'",
                    self.peer_node_id.short_str(),
                    err
                );
                self.flush_all_messages_to_failed_event(SendFailReason::SubstreamOpenFailed)
                    .await;
                Err(err.into())
            },
        }
    }

    async fn start_forwarding_messages(mut self, substream: Substream) -> Result<(), MessagingProtocolError> {
        let mut framed = MessagingProtocol::framed(substream);
        while let Some(mut out_msg) = self.request_rx.next().await {
            trace!(
                target: LOG_TARGET,
                "Sending message ({} bytes) ({:?}) on outbound messaging substream",
                out_msg.body.len(),
                out_msg.tag,
            );
            match framed.send(out_msg.body.clone()).await {
                Ok(_) => {
                    out_msg.reply_success();
                    let _ = self
                        .messaging_events_tx
                        .send(MessagingEvent::MessageSent(out_msg.tag))
                        .await;
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "[ThisNode={}] OutboundMessaging failed to send message to peer '{}' because '{}'",
                        self.node_identity.node_id().short_str(),
                        self.peer_node_id.short_str(),
                        err
                    );
                    out_msg.reply_fail();
                    let _ = self
                        .messaging_events_tx
                        .send(MessagingEvent::SendMessageFailed(
                            out_msg,
                            SendFailReason::SubstreamSendFailed,
                        ))
                        .await;
                    // FATAL: Failed to send on the substream
                    self.flush_all_messages_to_failed_event(SendFailReason::SubstreamSendFailed)
                        .await;
                    return Err(MessagingProtocolError::OutboundSubstreamFailure);
                },
            }
        }

        Ok(())
    }

    async fn flush_all_messages_to_failed_event(&mut self, reason: SendFailReason) {
        // Close the request channel so that we can read all the remaining messages and flush them
        // to a failed event
        self.request_rx.close();
        while let Some(out_msg) = self.request_rx.next().await {
            let _ = self
                .messaging_events_tx
                .send(MessagingEvent::SendMessageFailed(out_msg, reason))
                .await;
        }
    }
}

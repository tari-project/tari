//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::io;

use futures::{future, future::Either, SinkExt, StreamExt};
use log::*;
use tari_shutdown::ShutdownSignal;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
};

#[cfg(feature = "metrics")]
use super::metrics;
use super::{MessagingEvent, MessagingProtocol};
use crate::{message::InboundMessage, peer_manager::NodeId};

const LOG_TARGET: &str = "comms::protocol::messaging::inbound";

/// Inbound messaging actor. This is lazily spawned per peer when a peer requests a messaging session.
pub struct InboundMessaging {
    peer: NodeId,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
    messaging_events_tx: broadcast::Sender<MessagingEvent>,
    enable_message_received_event: bool,
    shutdown_signal: ShutdownSignal,
}

impl InboundMessaging {
    pub fn new(
        peer: NodeId,
        inbound_message_tx: mpsc::Sender<InboundMessage>,
        messaging_events_tx: broadcast::Sender<MessagingEvent>,
        enable_message_received_event: bool,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            peer,
            inbound_message_tx,
            messaging_events_tx,
            enable_message_received_event,
            shutdown_signal,
        }
    }

    pub async fn run<S>(mut self, socket: S)
    where S: AsyncRead + AsyncWrite + Unpin {
        let peer = &self.peer;
        #[cfg(feature = "metrics")]
        metrics::num_sessions().inc();
        debug!(
            target: LOG_TARGET,
            "Starting inbound messaging protocol for peer '{}'",
            peer.short_str()
        );

        let stream = MessagingProtocol::framed(socket);
        tokio::pin!(stream);

        while let Either::Right((Some(result), _)) = future::select(self.shutdown_signal.wait(), stream.next()).await {
            match result {
                Ok(raw_msg) => {
                    #[cfg(feature = "metrics")]
                    metrics::inbound_message_count(&self.peer).inc();
                    let msg_len = raw_msg.len();
                    let inbound_msg = InboundMessage::new(peer.clone(), raw_msg.freeze());
                    debug!(
                        target: LOG_TARGET,
                        "Received message {} from peer '{}' ({} bytes)",
                        inbound_msg.tag,
                        peer.short_str(),
                        msg_len
                    );

                    let message_tag = inbound_msg.tag;

                    if self.inbound_message_tx.send(inbound_msg).await.is_err() {
                        warn!(
                            target: LOG_TARGET,
                            "Failed to send InboundMessage {} for peer '{}' because inbound message channel closed",
                            message_tag,
                            peer.short_str(),
                        );

                        break;
                    }

                    if self.enable_message_received_event {
                        let _result = self
                            .messaging_events_tx
                            .send(MessagingEvent::MessageReceived(peer.clone(), message_tag));
                    }
                },
                // LengthDelimitedCodec emits a InvalidData io error when the message length exceeds the maximum allowed
                Err(err) if err.kind() == io::ErrorKind::InvalidData => {
                    #[cfg(feature = "metrics")]
                    metrics::error_count(peer).inc();
                    debug!(
                        target: LOG_TARGET,
                        "Failed to receive from peer '{}' because '{}'",
                        peer.short_str(),
                        err
                    );
                    let _result = self.messaging_events_tx.send(MessagingEvent::ProtocolViolation {
                        peer_node_id: peer.clone(),
                        details: err.to_string(),
                    });
                    break;
                },
                Err(err) => {
                    #[cfg(feature = "metrics")]
                    metrics::error_count(peer).inc();
                    error!(
                        target: LOG_TARGET,
                        "Failed to receive from peer '{}' because '{}'",
                        peer.short_str(),
                        err
                    );
                    break;
                },
            }
        }

        let _ignore = stream.close().await;

        let _ignore = self
            .messaging_events_tx
            .send(MessagingEvent::InboundProtocolExited(peer.clone()));
        #[cfg(feature = "metrics")]
        metrics::num_sessions().dec();
        debug!(
            target: LOG_TARGET,
            "Inbound messaging handler exited for peer `{}`",
            peer.short_str()
        );
    }
}

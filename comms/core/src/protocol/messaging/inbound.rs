//  Copyright 2020, The Taiji Project
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

use std::{sync::Arc, time::Duration};

use futures::StreamExt;
use log::*;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{broadcast, mpsc},
};

use super::{metrics, MessagingEvent, MessagingProtocol};
use crate::{message::InboundMessage, peer_manager::NodeId, rate_limit::RateLimit};

const LOG_TARGET: &str = "comms::protocol::messaging::inbound";

/// Inbound messaging actor. This is lazily spawned per peer when a peer requests a messaging session.
pub struct InboundMessaging {
    peer: NodeId,
    inbound_message_tx: mpsc::Sender<InboundMessage>,
    messaging_events_tx: broadcast::Sender<Arc<MessagingEvent>>,
    rate_limit_capacity: usize,
    rate_limit_restock_interval: Duration,
}

impl InboundMessaging {
    pub fn new(
        peer: NodeId,
        inbound_message_tx: mpsc::Sender<InboundMessage>,
        messaging_events_tx: broadcast::Sender<Arc<MessagingEvent>>,
        rate_limit_capacity: usize,
        rate_limit_restock_interval: Duration,
    ) -> Self {
        Self {
            peer,
            inbound_message_tx,
            messaging_events_tx,
            rate_limit_capacity,
            rate_limit_restock_interval,
        }
    }

    pub async fn run<S>(self, socket: S)
    where S: AsyncRead + AsyncWrite + Unpin {
        let peer = &self.peer;
        metrics::num_sessions().inc();
        debug!(
            target: LOG_TARGET,
            "Starting inbound messaging protocol for peer '{}'",
            peer.short_str()
        );

        let stream =
            MessagingProtocol::framed(socket).rate_limit(self.rate_limit_capacity, self.rate_limit_restock_interval);

        tokio::pin!(stream);

        let inbound_count = metrics::inbound_message_count(&self.peer);
        while let Some(result) = stream.next().await {
            match result {
                Ok(raw_msg) => {
                    inbound_count.inc();
                    let msg_len = raw_msg.len();
                    let inbound_msg = InboundMessage::new(peer.clone(), raw_msg.freeze());
                    debug!(
                        target: LOG_TARGET,
                        "Received message {} from peer '{}' ({} bytes)",
                        inbound_msg.tag,
                        peer.short_str(),
                        msg_len
                    );

                    let event = MessagingEvent::MessageReceived(inbound_msg.source_peer.clone(), inbound_msg.tag);

                    if let Err(err) = self.inbound_message_tx.send(inbound_msg).await {
                        let tag = err.0.tag;
                        warn!(
                            target: LOG_TARGET,
                            "Failed to send InboundMessage {} for peer '{}' because inbound message channel closed",
                            tag,
                            peer.short_str(),
                        );

                        break;
                    }

                    let _result = self.messaging_events_tx.send(Arc::new(event));
                },
                Err(err) => {
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

        metrics::num_sessions().dec();
        debug!(
            target: LOG_TARGET,
            "Inbound messaging handler exited for peer `{}`",
            peer.short_str()
        );
    }
}

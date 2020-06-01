// Copyright 2019, The Tari Project
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

use super::peer_message::PeerMessage;
use crate::{comms_connector::InboundDomainConnector, tari_message::TariMessageType};
use futures::{channel::mpsc, FutureExt, SinkExt, StreamExt};
use log::*;
use std::sync::Arc;
use tari_pubsub::{pubsub_channel, TopicPayload, TopicSubscriptionFactory};
use tokio::runtime::Handle;

const LOG_TARGET: &str = "comms::middleware::pubsub";

/// Alias for a pubsub-type domain connector
pub type PubsubDomainConnector = InboundDomainConnector<mpsc::Sender<Arc<PeerMessage>>>;
pub type SubscriptionFactory = TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>;

/// Connects `InboundDomainConnector` to a `tari_pubsub::TopicPublisher` through a buffered channel
pub fn pubsub_connector(executor: Handle, buf_size: usize) -> (PubsubDomainConnector, SubscriptionFactory) {
    let (publisher, subscription_factory) = pubsub_channel(buf_size);
    let (sender, receiver) = mpsc::channel(buf_size);

    // Spawn a task which forwards messages from the pubsub service to the TopicPublisher
    let forwarder = receiver
        // Map DomainMessage into a TopicPayload
        .map(|msg: Arc<PeerMessage>| {
            TariMessageType::from_i32(msg.message_header.message_type)
                .map(|msg_type| {
                    let message_tag_trace = msg.dht_header.message_tag;
                    let payload = TopicPayload::new(msg_type, msg);
                    trace!(
                        target: LOG_TARGET,
                        "Created topic payload message {:?}, Trace: {}",
                        &payload.topic(), message_tag_trace
                    );
                    payload
                })
                .ok_or_else(|| "Invalid or unrecognised Tari message type".to_string())
        })
        // Forward TopicPayloads to the publisher
        .forward(publisher.sink_map_err(|err| err.to_string()))
        // Log error and return unit
        .map(|result| {
            if let Err(err) = result {
                warn!(
                    target: LOG_TARGET,
                    "Error forwarding pubsub messages to publisher: {}", err
                );
            }
        });
    executor.spawn(forwarder);

    (InboundDomainConnector::new(sender), subscription_factory)
}

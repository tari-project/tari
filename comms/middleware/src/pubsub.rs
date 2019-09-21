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

use crate::{inbound_connector::InboundDomainConnector, message::DomainMessage};
use futures::{channel::mpsc, FutureExt, StreamExt};
use log::*;
use std::sync::Arc;
use tari_pubsub::{pubsub_channel, TopicPayload, TopicSubscriptionFactory};
use tokio::runtime::TaskExecutor;

const LOG_TARGET: &'static str = "comms::middleware::pubsub";

/// Connects `InboundDomainConnector` to a `tari_pubsub::TopicPublisher` through a buffered channel
pub fn pubsub_service<MType>(
    executor: TaskExecutor,
    buf_size: usize,
) -> (
    InboundDomainConnector<MType, mpsc::Sender<Arc<DomainMessage<MType>>>>,
    TopicSubscriptionFactory<MType, Arc<DomainMessage<MType>>>,
)
where
    MType: Eq + Sync + Send + Clone + 'static,
{
    let (publisher, subscription_factory) = pubsub_channel(buf_size);
    let (sender, receiver) = mpsc::channel(buf_size);

    // Spawn a task which forwards messages from the pubsub service to the TopicPublisher
    let forwarder = receiver
        // Map DomainMessage into a TopicPayload
        .map(|msg: Arc<DomainMessage<MType>>| Ok(TopicPayload::new(msg.message_header.message_type.clone(), msg)))
        // Forward TopicPayloads to the publisher
        .forward(publisher)
        // Log error and return unit
        .map(|result| {
            if let Err(err) = result {
                error!(
                    target: LOG_TARGET,
                    "Error forwarding pubsub messages to publisher: {}", err
                );
            }
            ()
        });
    executor.spawn(forwarder);

    (InboundDomainConnector::new(sender), subscription_factory)
}

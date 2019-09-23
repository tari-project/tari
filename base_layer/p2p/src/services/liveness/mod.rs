//  Copyright 2019 The Tari Project
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

//! # Liveness Service
//!
//! This service is responsible for sending pings to any peer as well as maintaining
//! some very basic counters for the number of ping/pongs sent and received.
//!
//! It is responsible for:
//! - handling requests to the Liveness backend. Types of requests can be found in the [LivenessRequest] enum, and
//! - reading incoming [PingPong] messages and processing them.
//!
//! In future, this service may be expanded to included periodic pings to maintain
//! latency and availability statistics for peers.
//!
//! [LivenessRequest]: ./messages/enum.LivenessRequets.html
//! [PingPong]: ./messages/enum.PingPong.html

mod error;
mod messages;
mod service;
mod state;

use self::{error::LivenessError, service::LivenessService, state::LivenessState};
use crate::{
    domain_message::DomainMessage,
    services::comms_outbound::CommsOutboundHandle,
    tari_message::{NetMessage, TariMessageType},
};
use futures::{future, Future, Stream, StreamExt};
use std::{fmt::Debug, sync::Arc};
use tari_comms_middleware::message::PeerMessage;
use tari_pubsub::TopicSubscriptionFactory;
use tari_service_framework::{
    handles::ServiceHandlesFuture,
    reply_channel::{self, SenderService},
    ServiceInitializationError,
    ServiceInitializer,
};
use tari_utilities::message_format::{MessageFormat, MessageFormatError};
use tokio::runtime::TaskExecutor;

pub use self::messages::{LivenessRequest, LivenessResponse, PingPong};

pub type LivenessHandle = SenderService<LivenessRequest, Result<LivenessResponse, LivenessError>>;

const LOG_TARGET: &'static str = "base_layer::p2p::services::liveness";

/// Initializer for the Liveness service handle and service future.
pub struct LivenessInitializer {
    inbound_message_subscription_factory:
        Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage<TariMessageType>>>>,
}

impl LivenessInitializer {
    /// Create a new LivenessInitializer from the inbound message subscriber
    pub fn new(
        inbound_message_subscription_factory: Arc<
            TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage<TariMessageType>>>,
        >,
    ) -> Self
    {
        Self {
            inbound_message_subscription_factory,
        }
    }

    /// Get a stream of inbound PingPong messages
    fn ping_stream(&self) -> impl Stream<Item = DomainMessage<PingPong>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::new(NetMessage::PingPong))
            .map(map_deserialized::<PingPong>)
            .filter_map(ok_or_skip_result)
    }
}

impl ServiceInitializer for LivenessInitializer {
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(&mut self, executor: TaskExecutor, handles_fut: ServiceHandlesFuture) -> Self::Future {
        let (sender, receiver) = reply_channel::unbounded();

        // Register handle before waiting for handles to be ready
        handles_fut.register(sender);

        // Create a stream which receives PingPong messages from comms
        let ping_stream = self.ping_stream();

        executor.spawn(async move {
            // Wait for all handles to become available
            let handles = handles_fut.await;

            let outbound_handle = handles
                .get_handle::<CommsOutboundHandle>()
                .expect("Liveness service requires CommsOutbound service handle");

            let state = Arc::new(LivenessState::new());

            // Spawn the Liveness service on the executor
            let service = LivenessService::new(receiver, ping_stream, state, outbound_handle);
            service.run().await;
        });

        future::ready(Ok(()))
    }
}

/// For use with `StreamExt::filter_map`. Log and filter any errors.
async fn ok_or_skip_result<T, E>(res: Result<T, E>) -> Option<T>
where E: Debug {
    match res {
        Ok(t) => Some(t),
        Err(err) => {
            tracing::error!(target: LOG_TARGET, "{:?}", err);
            None
        },
    }
}

fn map_deserialized<T>(serialized: Arc<PeerMessage<TariMessageType>>) -> Result<DomainMessage<T>, MessageFormatError>
where T: MessageFormat {
    Ok(DomainMessage {
        source_peer: serialized.source_peer.clone(),
        origin_pubkey: serialized.envelope_header.origin_pubkey.clone(),
        inner: serialized.deserialize_message()?,
    })
}

#[cfg(test)]
mod test {
    use futures::executor::block_on;

    #[test]
    fn ok_or_skip_result() {
        block_on(async {
            let res = Result::<_, ()>::Ok(());
            assert_eq!(super::ok_or_skip_result(res).await.unwrap(), ());

            let res = Result::<(), _>::Err(());
            assert!(super::ok_or_skip_result(res).await.is_none());
        });
    }
}

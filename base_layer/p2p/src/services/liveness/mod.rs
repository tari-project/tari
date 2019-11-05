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

mod config;
pub mod error;
pub mod handle;
mod service;
mod state;

use self::{service::LivenessService, state::LivenessState};
use crate::{
    comms_connector::PeerMessage,
    domain_message::DomainMessage,
    proto::{liveness::PingPongMessage, TariMessageType},
    services::{
        liveness::handle::LivenessHandle,
        utils::{map_decode, ok_or_skip_result},
    },
};
use futures::{future, Future, Stream, StreamExt};
use log::*;
use std::sync::Arc;
use tari_broadcast_channel::bounded;
use tari_comms_dht::outbound::OutboundMessageRequester;
use tari_pubsub::TopicSubscriptionFactory;
use tari_service_framework::{
    handles::ServiceHandlesFuture,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
};
use tari_shutdown::ShutdownSignal;
use tokio::runtime::TaskExecutor;

// Public exports
pub use self::{
    config::LivenessConfig,
    handle::{LivenessRequest, LivenessResponse},
};
use tari_comms_dht::DhtRequester;

const LOG_TARGET: &'static str = "p2p::services::liveness";

/// Initializer for the Liveness service handle and service future.
pub struct LivenessInitializer {
    config: Option<LivenessConfig>,
    inbound_message_subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
    dht_requester: Option<DhtRequester>,
}

impl LivenessInitializer {
    /// Create a new LivenessInitializer from the inbound message subscriber
    pub fn new(
        config: LivenessConfig,
        inbound_message_subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
        dht_requester: DhtRequester,
    ) -> Self
    {
        Self {
            config: Some(config),
            inbound_message_subscription_factory,
            dht_requester: Some(dht_requester),
        }
    }

    /// Get a stream of inbound PingPong messages
    fn ping_stream(&self) -> impl Stream<Item = DomainMessage<PingPongMessage>> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::PingPong)
            .map(map_decode::<PingPongMessage>)
            .filter_map(ok_or_skip_result)
    }
}

impl ServiceInitializer for LivenessInitializer {
    type Future = impl Future<Output = Result<(), ServiceInitializationError>>;

    fn initialize(
        &mut self,
        executor: TaskExecutor,
        handles_fut: ServiceHandlesFuture,
        shutdown: ShutdownSignal,
    ) -> Self::Future
    {
        let (sender, receiver) = reply_channel::unbounded();

        let (publisher, subscriber) = bounded(100);

        let liveness_handle = LivenessHandle::new(sender, subscriber);

        // Saving a clone
        let config = self
            .config
            .take()
            .expect("Liveness service initialized more than once.");

        let mut dht_requester = self
            .dht_requester
            .take()
            .expect("Liveness service initialized more than once.");

        // Register handle before waiting for handles to be ready
        handles_fut.register(liveness_handle);

        // Create a stream which receives PingPong messages from comms
        let ping_stream = self.ping_stream();

        // Spawn the Liveness service on the executor
        executor.spawn(async move {
            // Wait for all handles to become available
            let handles = handles_fut.await;

            let outbound_handle = handles
                .get_handle::<OutboundMessageRequester>()
                .expect("Liveness service requires CommsOutbound service handle");

            if config.enable_auto_join {
                match dht_requester.send_join().await {
                    Ok(_) => {
                        trace!(target: LOG_TARGET, "Join message has been sent to closest peers",);
                    },
                    Err(err) => {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send join message on startup because '{}'", err
                        );
                    },
                }
            }

            if config.enable_auto_stored_message_request {
                // TODO: Record when store message request was last requested
                //       and request messages from after that time
                match dht_requester.send_request_stored_messages().await {
                    Ok(_) => {
                        trace!(
                            target: LOG_TARGET,
                            "Stored message request has been sent to closest peers",
                        );
                    },
                    Err(err) => {
                        error!(
                            target: LOG_TARGET,
                            "Failed to send stored message on startup because '{}'", err
                        );
                    },
                }
            }

            let state = LivenessState::new();

            let service = LivenessService::new(
                config,
                receiver,
                ping_stream,
                state,
                dht_requester,
                outbound_handle,
                publisher,
                shutdown,
            );
            service.run().await;
            debug!(target: LOG_TARGET, "Liveness service has shut down");
        });

        future::ready(Ok(()))
    }
}

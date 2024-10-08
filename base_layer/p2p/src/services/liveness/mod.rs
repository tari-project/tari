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

pub mod config;
pub use self::config::LivenessConfig;

pub mod error;

mod handle;
pub use handle::{LivenessEvent, LivenessHandle, LivenessRequest, LivenessResponse, PingPongEvent};

mod message;
mod service;

mod state;
pub use state::Metadata;

#[cfg(feature = "test-mocks")]
pub mod mock;

use log::*;
use tari_network::{NetworkHandle, OutboundMessaging};
use tari_service_framework::{
    async_trait,
    reply_channel,
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
};
use tokio::sync::{broadcast, mpsc};

use self::service::LivenessService;
use crate::{
    message::TariNodeMessageSpec,
    services::{dispatcher::Dispatcher, liveness::state::LivenessState},
    tari_message::TariMessageType,
};

const LOG_TARGET: &str = "p2p::services::liveness";

/// Initializer for the Liveness service handle and service future.
pub struct LivenessInitializer {
    config: Option<LivenessConfig>,
}

impl LivenessInitializer {
    /// Create a new LivenessInitializer from the inbound message subscriber
    pub fn new(config: LivenessConfig) -> Self {
        Self { config: Some(config) }
    }
}

#[async_trait]
impl ServiceInitializer for LivenessInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Liveness Service");
        let (sender, receiver) = reply_channel::unbounded();

        let (publisher, _) = broadcast::channel(200);

        // Register handle before waiting for handles to be ready
        context.register_handle(LivenessHandle::new(sender, publisher.clone()));

        // Saving a clone
        let config = self
            .config
            .take()
            .expect("Liveness service initialized more than once.");

        // Spawn the Liveness service on the executor
        context.spawn_when_ready(|handles| async move {
            let dispatcher = handles.expect_handle::<Dispatcher>();
            let (ping_tx, ping_rx) = mpsc::unbounded_channel();
            dispatcher.register(TariMessageType::PingPong, ping_tx);

            let network = handles.expect_handle::<NetworkHandle>();
            let outbound_messaging = handles.expect_handle::<OutboundMessaging<TariNodeMessageSpec>>();

            let service = LivenessService::new(
                config,
                receiver,
                ping_rx,
                LivenessState::new(),
                network,
                outbound_messaging,
                publisher,
                handles.get_shutdown_signal(),
            );
            service.run().await;
            debug!(target: LOG_TARGET, "Liveness service has shut down");
        });

        debug!(target: LOG_TARGET, "Liveness service initialized");
        Ok(())
    }
}

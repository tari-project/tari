//  Copyright 2022, The Tari Project
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

mod service;

use std::{cmp::max, time::Duration};

use log::debug;
use tari_comms::{async_trait, connectivity::ConnectivityRequester};
use tari_service_framework::{ServiceInitializationError, ServiceInitializer, ServiceInitializerContext};

use crate::services::{
    liveness::{LivenessHandle, MAX_INFLIGHT_TTL},
    monitor_peers::service::MonitorPeersService,
};

const LOG_TARGET: &str = "p2p::services::monitor_peers";

/// Initializer for the MonitorPeers service handle and service future.
pub struct MonitorPeersInitializer {
    auto_ping_interval: Option<Duration>,
}

impl MonitorPeersInitializer {
    /// Create a new MonitorPeersInitializer from the inbound message subscriber
    pub fn new(auto_ping_interval: Duration) -> Self {
        Self {
            auto_ping_interval: Some(auto_ping_interval),
        }
    }
}

impl Default for MonitorPeersInitializer {
    fn default() -> Self {
        Self {
            auto_ping_interval: Some(MAX_INFLIGHT_TTL),
        }
    }
}

#[async_trait]
impl ServiceInitializer for MonitorPeersInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Peer Monitoring Service");

        let auto_ping_interval = max(
            self.auto_ping_interval
                .take()
                .expect("Monitor peers service initialized more than once."),
            MAX_INFLIGHT_TTL,
        );

        // Spawn the MonitorPeers service on the executor
        context.spawn_when_ready(move |handles| async move {
            let liveness = handles.expect_handle::<LivenessHandle>();
            let connectivity = handles.expect_handle::<ConnectivityRequester>();

            let service = MonitorPeersService::new(
                connectivity,
                liveness,
                handles.get_shutdown_signal(),
                auto_ping_interval,
            );
            service.run().await;
            debug!(target: LOG_TARGET, "Monitor peers service has shut down");
        });

        debug!(target: LOG_TARGET, "Monitor peers service initialized");
        Ok(())
    }
}

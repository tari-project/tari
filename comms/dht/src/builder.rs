// Copyright 2019, The Taiji Project
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

//! A builder for customizing and constructing the DHT

use std::{sync::Arc, time::Duration};

use taiji_comms::{connectivity::ConnectivityRequester, NodeIdentity, PeerManager};
use taiji_shutdown::ShutdownSignal;
use tokio::sync::mpsc;

use crate::{
    dht::DhtInitializationError,
    outbound::DhtOutboundRequest,
    version::DhtProtocolVersion,
    DbConnectionUrl,
    Dht,
    DhtConfig,
};

/// Builder for the DHT.
///
/// ```rust
/// # use taiji_comms_dht::{DbConnectionUrl, Dht};
/// let builder = Dht::builder()
///     .mainnet()
///     .with_database_url(DbConnectionUrl::Memory);
/// // let dht = builder.build(...).unwrap();
/// ```
#[derive(Debug, Clone, Default)]
pub struct DhtBuilder {
    config: DhtConfig,
    outbound_tx: Option<mpsc::Sender<DhtOutboundRequest>>,
}

impl DhtBuilder {
    pub(crate) fn new() -> Self {
        Self {
            #[cfg(test)]
            config: DhtConfig::default_local_test(),
            #[cfg(not(test))]
            config: Default::default(),
            outbound_tx: None,
        }
    }

    /// Specify a complete [DhtConfig](crate::DhtConfig).
    pub fn with_config(&mut self, config: DhtConfig) -> &mut Self {
        self.config = config;
        self
    }

    /// Default configuration for local test environments.
    pub fn local_test(&mut self) -> &mut Self {
        self.config = DhtConfig::default_local_test();
        self
    }

    /// Sets the DHT protocol version.
    pub fn with_protocol_version(&mut self, protocol_version: DhtProtocolVersion) -> &mut Self {
        self.config.protocol_version = protocol_version;
        self
    }

    /// Sets whether SAF messages are automatically requested on every new connection to a SAF node.
    pub fn set_auto_store_and_forward_requests(&mut self, enabled: bool) -> &mut Self {
        self.config.saf.auto_request = enabled;
        self
    }

    /// Sets the mpsc sender that is hooked up to the outbound messaging pipeline.
    pub fn with_outbound_sender(&mut self, outbound_tx: mpsc::Sender<DhtOutboundRequest>) -> &mut Self {
        self.outbound_tx = Some(outbound_tx);
        self
    }

    /// Use the default testnet configuration.
    pub fn testnet(&mut self) -> &mut Self {
        self.config = DhtConfig::default_testnet();
        self
    }

    /// Use the default mainnet configuration.
    pub fn mainnet(&mut self) -> &mut Self {
        self.config = DhtConfig::default_mainnet();
        self
    }

    /// Sets the [DbConnectionUrl](crate::DbConnectionUrl).
    pub fn with_database_url(&mut self, database_url: DbConnectionUrl) -> &mut Self {
        self.config.database_url = database_url;
        self
    }

    /// The number of connections to random peers that should be maintained.
    /// Connections to random peers are reshuffled every `DhtConfig::connectivity::random_pool_refresh_interval`.
    pub fn with_num_random_nodes(&mut self, n: usize) -> &mut Self {
        self.config.num_random_nodes = n;
        self
    }

    /// The number of neighbouring peers that the DHT should try maintain connections to.
    pub fn with_num_neighbouring_nodes(&mut self, n: usize) -> &mut Self {
        self.config.num_neighbouring_nodes = n;
        self.config.saf.num_neighbouring_nodes = n;
        self
    }

    /// The number of peers to send a message using the
    /// [Broadcast](crate::broadcast_strategy::BroadcastStrategy::Propagate) strategy.
    pub fn with_propagation_factor(&mut self, propagation_factor: usize) -> &mut Self {
        self.config.propagation_factor = propagation_factor;
        self
    }

    /// The number of peers to send a message broadcast using the
    /// [Broadcast](crate::broadcast_strategy::BroadcastStrategy::Broadcast) strategy.
    pub fn with_broadcast_factor(&mut self, broadcast_factor: usize) -> &mut Self {
        self.config.broadcast_factor = broadcast_factor;
        self
    }

    /// The length of time to wait for a discovery reply after a discovery message has been sent.
    pub fn with_discovery_timeout(&mut self, timeout: Duration) -> &mut Self {
        self.config.discovery_request_timeout = timeout;
        self
    }

    /// Enables automatically sending a join/announce message when connected to enough peers on the network.
    pub fn enable_auto_join(&mut self) -> &mut Self {
        self.config.auto_join = true;
        self
    }

    /// Build and initialize a Dht object.
    ///
    /// Will panic if not in a tokio runtime context
    pub async fn build(
        &mut self,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        connectivity: ConnectivityRequester,
        shutdown_signal: ShutdownSignal,
    ) -> Result<Dht, DhtInitializationError> {
        let outbound_tx = self
            .outbound_tx
            .take()
            .ok_or(DhtInitializationError::BuilderNoOutboundMessageSender)?;

        Dht::initialize(
            self.config.clone(),
            node_identity,
            peer_manager,
            outbound_tx,
            connectivity,
            shutdown_signal,
        )
        .await
    }
}

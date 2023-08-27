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

use std::sync::Arc;

use taiji_comms::peer_manager::NodeIdentity;
use tokio::sync::mpsc;
use tower::layer::Layer;

use super::middleware::MessageHandlerMiddleware;
use crate::{
    actor::DhtRequester,
    outbound::OutboundMessageRequester,
    store_forward::{SafConfig, StoreAndForwardRequester},
};

/// Layer responsible for handling SAF protocol messages.
pub struct MessageHandlerLayer {
    config: SafConfig,
    saf_requester: StoreAndForwardRequester,
    dht_requester: DhtRequester,
    node_identity: Arc<NodeIdentity>,
    outbound_service: OutboundMessageRequester,
    saf_response_signal_sender: mpsc::Sender<()>,
}

impl MessageHandlerLayer {
    pub fn new(
        config: SafConfig,
        saf_requester: StoreAndForwardRequester,
        dht_requester: DhtRequester,
        node_identity: Arc<NodeIdentity>,
        outbound_service: OutboundMessageRequester,
        saf_response_signal_sender: mpsc::Sender<()>,
    ) -> Self {
        Self {
            config,
            saf_requester,
            dht_requester,
            node_identity,

            outbound_service,
            saf_response_signal_sender,
        }
    }
}

impl<S> Layer<S> for MessageHandlerLayer {
    type Service = MessageHandlerMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        MessageHandlerMiddleware::new(
            self.config.clone(),
            service,
            self.saf_requester.clone(),
            self.dht_requester.clone(),
            Arc::clone(&self.node_identity),
            self.outbound_service.clone(),
            self.saf_response_signal_sender.clone(),
        )
    }
}

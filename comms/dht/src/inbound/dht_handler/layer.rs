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

use super::middleware::DhtHandlerMiddleware;
use crate::{discovery::DhtDiscoveryRequester, outbound::OutboundMessageRequester};
use std::sync::Arc;
use tari_comms::peer_manager::{NodeIdentity, PeerManager};
use tower::layer::Layer;

pub struct DhtHandlerLayer {
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    outbound_service: OutboundMessageRequester,
    discovery_requester: DhtDiscoveryRequester,
}

impl DhtHandlerLayer {
    pub fn new(
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        discovery_requester: DhtDiscoveryRequester,
        outbound_service: OutboundMessageRequester,
    ) -> Self
    {
        Self {
            node_identity,
            peer_manager,
            discovery_requester,
            outbound_service,
        }
    }
}

impl<S> Layer<S> for DhtHandlerLayer {
    type Service = DhtHandlerMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        DhtHandlerMiddleware::new(
            service,
            Arc::clone(&self.node_identity),
            Arc::clone(&self.peer_manager),
            self.outbound_service.clone(),
            self.discovery_requester.clone(),
        )
    }
}

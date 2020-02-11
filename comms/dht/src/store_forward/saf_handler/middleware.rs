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

use super::task::MessageHandlerTask;
use crate::{
    actor::DhtRequester,
    config::DhtConfig,
    inbound::DecryptedDhtMessage,
    outbound::OutboundMessageRequester,
    store_forward::SafStorage,
    PipelineError,
};
use futures::{task::Context, Future};
use std::{sync::Arc, task::Poll};
use tari_comms::peer_manager::{NodeIdentity, PeerManager};
use tower::Service;

#[derive(Clone)]
pub struct MessageHandlerMiddleware<S> {
    config: DhtConfig,
    next_service: S,
    store: Arc<SafStorage>,
    dht_requester: DhtRequester,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    outbound_service: OutboundMessageRequester,
}

impl<S> MessageHandlerMiddleware<S> {
    pub fn new(
        config: DhtConfig,
        next_service: S,
        store: Arc<SafStorage>,
        dht_requester: DhtRequester,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        outbound_service: OutboundMessageRequester,
    ) -> Self
    {
        Self {
            config,
            store,
            dht_requester,
            next_service,
            node_identity,
            peer_manager,
            outbound_service,
        }
    }
}

impl<S> Service<DecryptedDhtMessage> for MessageHandlerMiddleware<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + Sync + Send
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.next_service.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, message: DecryptedDhtMessage) -> Self::Future {
        MessageHandlerTask::new(
            self.config.clone(),
            self.next_service.clone(),
            Arc::clone(&self.store),
            self.dht_requester.clone(),
            Arc::clone(&self.peer_manager),
            self.outbound_service.clone(),
            Arc::clone(&self.node_identity),
            message,
        )
        .run()
    }
}

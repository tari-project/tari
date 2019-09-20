// Copyright 2019. The Tari Project
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

use crate::{encryption::DecryptedInboundMessage, error::MiddlewareError};
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::{
    message::MessageEnvelope,
    outbound_message_service::{BroadcastStrategy, OutboundServiceRequester},
    peer_manager::{NodeIdentity, PeerManager},
};
use tower::{layer::Layer, Service};

const LOG_TARGET: &'static str = "comms::middleware::forward";

/// This layer is responsible for forwarding messages which have failed to decrypt
pub struct ForwardLayer {
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    outbound_service: OutboundServiceRequester,
}

impl ForwardLayer {
    pub fn new(
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        outbound_service: OutboundServiceRequester,
    ) -> Self
    {
        Self {
            peer_manager,
            node_identity,
            outbound_service,
        }
    }
}

impl<S> Layer<S> for ForwardLayer {
    type Service = ForwardService<S>;

    fn layer(&self, service: S) -> Self::Service {
        ForwardService::new(
            service,
            Arc::clone(&self.peer_manager),
            Arc::clone(&self.node_identity),
            self.outbound_service.clone(),
        )
    }
}

#[derive(Clone)]
pub struct ForwardService<S> {
    inner: S,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    outbound_service: OutboundServiceRequester,
}

impl<S> ForwardService<S> {
    pub fn new(
        service: S,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        outbound_service: OutboundServiceRequester,
    ) -> Self
    {
        Self {
            inner: service,
            peer_manager,
            node_identity,
            outbound_service,
        }
    }
}

impl<S> Service<DecryptedInboundMessage> for ForwardService<S>
where
    S: Service<DecryptedInboundMessage, Response = ()> + Clone + 'static,
    S::Error: Into<MiddlewareError>,
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, msg: DecryptedInboundMessage) -> Self::Future {
        Forwarder::new(
            self.inner.clone(),
            Arc::clone(&self.peer_manager),
            Arc::clone(&self.node_identity),
            self.outbound_service.clone(),
        )
        .handle(msg)
    }
}

struct Forwarder<S> {
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    inner: S,
    outbound_service: OutboundServiceRequester,
}

impl<S> Forwarder<S> {
    pub fn new(
        service: S,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        outbound_service: OutboundServiceRequester,
    ) -> Self
    {
        Self {
            peer_manager,
            node_identity,
            inner: service,
            outbound_service,
        }
    }
}

impl<S> Forwarder<S>
where
    S: Service<DecryptedInboundMessage, Response = ()>,
    S::Error: Into<MiddlewareError>,
{
    async fn handle(mut self, message: DecryptedInboundMessage) -> Result<(), MiddlewareError> {
        if message.decryption_succeeded() {
            self.inner.call(message).await.map_err(Into::into)
        } else {
            self.forward(message).await
        }
    }

    async fn forward(&mut self, message: DecryptedInboundMessage) -> Result<(), MiddlewareError> {
        let DecryptedInboundMessage {
            envelope_header,
            decryption_result,
            ..
        } = message;

        let body = decryption_result.err().expect("previous check that decryption failed");

        let broadcast_strategy = BroadcastStrategy::forward(
            self.node_identity.identity.node_id.clone(),
            &self.peer_manager,
            envelope_header.destination.clone(),
            vec![envelope_header.origin_source.clone(), envelope_header.peer_source],
        )?;

        let envelope = MessageEnvelope::construct(
            &self.node_identity,
            envelope_header.origin_source,
            envelope_header.destination,
            body,
            envelope_header.flags,
        )?;

        debug!(target: LOG_TARGET, "Forwarding message");
        self.outbound_service
            .forward_message(broadcast_strategy, envelope)
            .await?;

        Ok(())
    }
}

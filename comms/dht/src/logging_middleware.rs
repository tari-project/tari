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

use crate::{crypt, inbound::DhtInboundMessage, outbound::message::DhtOutboundMessage, PipelineError};
use futures::{task::Context, Future};
use log::*;
use std::{sync::Arc, task::Poll};
use tari_comms::peer_manager::NodeIdentity;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::middleware::message_logging";

/// This layer is responsible for logging messages for debugging. It should not be used in
/// production
pub struct MessageLoggingLayer {}

impl MessageLoggingLayer {
    pub fn new() -> Self {
        Self {}
    }
}

impl<S> Layer<S> for MessageLoggingLayer {
    type Service = MessageLoggingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        MessageLoggingService::new(service)
    }
}

#[derive(Clone)]
pub struct MessageLoggingService<S> {
    inner: S,
}

impl<S> MessageLoggingService<S> {
    pub fn new(service: S) -> Self {
        Self { inner: service }
    }
}

impl<S> Service<DhtOutboundMessage> for MessageLoggingService<S>
where
    S: Service<DhtOutboundMessage, Response = ()> + Clone,
    S::Error: Into<PipelineError>,
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtOutboundMessage) -> Self::Future {
        Self::handle_message(self.inner.clone(), msg)
    }
}

impl<S> MessageLoggingService<S>
where
    S: Service<DhtOutboundMessage, Response = ()>,
    S::Error: Into<PipelineError>,
{
    async fn handle_message(mut next_service: S, mut message: DhtOutboundMessage) -> Result<(), PipelineError> {
        trace!(target: LOG_TARGET, "Outbound message: {}", message);

        next_service.oneshot(message).await.map_err(Into::into)
    }
}

impl<S> Service<DhtInboundMessage> for MessageLoggingService<S>
where
    S: Service<DhtInboundMessage, Response = ()> + Clone,
    S::Error: Into<PipelineError>,
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtInboundMessage) -> Self::Future {
        Self::handle_message_inbound(self.inner.clone(), msg)
    }
}

impl<S> MessageLoggingService<S>
where
    S: Service<DhtInboundMessage, Response = ()>,
    S::Error: Into<PipelineError>,
{
    async fn handle_message_inbound(mut next_service: S, mut message: DhtInboundMessage) -> Result<(), PipelineError> {
        trace!(target: LOG_TARGET, "Inbound message: {}", message);

        next_service.ready().await.map_err(Into::into)?;
        next_service.call(message).await.map_err(Into::into)
    }
}

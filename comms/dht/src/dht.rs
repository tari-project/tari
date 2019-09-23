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

use crate::messages::{DhtInboundMessage, DhtMessageType};
use futures::{task::Context, Future, Poll};
use tower::{layer::Layer, Service};

pub struct DhtMiddleware<S> {
    inner: S,
}

impl<S> DhtMiddleware<S> {
    pub fn new(service: S) -> Self {
        Self { inner: service }
    }
}

impl<S> Service<DhtInboundMessage> for DhtMiddleware<S>
where S: Service<DhtInboundMessage, Response = ()> + Clone
{
    type Error = S::Error;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, message: DhtInboundMessage) -> Self::Future {
        Self::handle_message(self.inner.clone(), message)
    }
}

impl<S> DhtMiddleware<S>
where S: Service<DhtInboundMessage, Response = ()> + Clone
{
    async fn handle_message(mut service: S, message: DhtInboundMessage) -> Result<(), S::Error> {
        match message.dht_header.message_type {
            DhtMessageType::Join => Self::handle_join(service, message).await?,
            DhtMessageType::Discover => Self::handle_discover(service, message).await?,
            // Not a DHT message, call downstream middleware
            DhtMessageType::None => service.call(message).await?,
        }

        Ok(())
    }

    async fn handle_join(service: S, message: DhtInboundMessage) -> Result<(), S::Error> {
        unimplemented!()
    }

    async fn handle_discover(service: S, message: DhtInboundMessage) -> Result<(), S::Error> {
        unimplemented!()
    }
}

pub struct DhtLayer;
impl DhtLayer {
    pub fn new() -> Self {
        DhtLayer
    }
}

impl<S> Layer<S> for DhtLayer {
    type Service = DhtMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        DhtMiddleware::new(service)
    }
}

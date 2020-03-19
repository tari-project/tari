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

use futures::{task::Context, Future};
use log::*;
use std::{fmt::Display, marker::PhantomData, task::Poll};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::middleware::message_logging";

/// This layer is responsible for logging messages for debugging.
pub struct MessageLoggingLayer<R> {
    prefix_msg: &'static str,
    _r: PhantomData<R>,
}

impl<R> MessageLoggingLayer<R> {
    pub fn new(prefix_msg: &'static str) -> Self {
        Self {
            prefix_msg,
            _r: PhantomData,
        }
    }
}

impl<S, R> Layer<S> for MessageLoggingLayer<R>
where
    S: Service<R>,
    S::Error: std::error::Error + Send + Sync + 'static,
    R: Display,
{
    type Service = MessageLoggingService<S>;

    fn layer(&self, service: S) -> Self::Service {
        MessageLoggingService::new(self.prefix_msg, service)
    }
}

#[derive(Clone)]
pub struct MessageLoggingService<S> {
    prefix_msg: &'static str,
    inner: S,
}

impl<S> MessageLoggingService<S> {
    pub fn new(prefix_msg: &'static str, service: S) -> Self {
        Self {
            inner: service,
            prefix_msg,
        }
    }
}

impl<S, R> Service<R> for MessageLoggingService<S>
where
    S: Service<R> + Clone,
    S::Error: std::error::Error + Send + Sync + 'static,
    R: Display,
{
    type Error = S::Error;
    type Response = S::Response;

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: R) -> Self::Future {
        trace!(target: LOG_TARGET, "{}{}", self.prefix_msg, msg);
        let mut inner = self.inner.clone();
        async move {
            inner.ready().await?;
            inner.call(msg).await
        }
    }
}

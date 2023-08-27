// Copyright 2020, The Taiji Project
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

use std::{future, task::Poll};

use futures::{future::BoxFuture, task::Context, FutureExt};
use tower::Service;

use super::PipelineError;

/// A service which forwards and messages it gets to the given Sink
#[derive(Clone)]
pub struct SinkService<TSink>(TSink);

impl<TSink> SinkService<TSink> {
    /// Creates a new service that forwards to the given sink.
    pub fn new(sink: TSink) -> Self {
        SinkService(sink)
    }
}

impl<T> Service<T> for SinkService<tokio::sync::mpsc::Sender<T>>
where T: Send + 'static
{
    type Error = PipelineError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, item: T) -> Self::Future {
        let sink = self.0.clone();
        async move {
            sink.send(item)
                .await
                .map_err(|_| anyhow::anyhow!("sink closed in sink service"))
        }
        .boxed()
    }
}
impl<T> Service<T> for SinkService<tokio::sync::mpsc::UnboundedSender<T>>
where T: Send + 'static
{
    type Error = PipelineError;
    type Future = future::Ready<Result<Self::Response, Self::Error>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, item: T) -> Self::Future {
        let sink = self.0.clone();
        let result = sink
            .send(item)
            .map_err(|_| anyhow::anyhow!("sink closed in sink service"));
        future::ready(result)
    }
}

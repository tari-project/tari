// Copyright 2020, The Tari Project
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

use crate::bounded_executor::BoundedExecutor;
use futures::{future::FusedFuture, stream::FusedStream, Stream, StreamExt};
use log::*;
use std::fmt::Debug;
use tari_shutdown::ShutdownSignal;
use tower::{Service, ServiceExt};

const LOG_TARGET: &str = "comms::pipeline::inbound";

/// Calls a Service with every item received from a Stream.
/// The difference between this can ServiceExt::call_all is
/// that ServicePipeline doesn't keep the result of the service
/// call and that it spawns a task for each incoming item.
pub struct Inbound<TSvc, TStream> {
    executor: BoundedExecutor,
    service: TSvc,
    stream: TStream,
    shutdown_signal: ShutdownSignal,
}

impl<TSvc, TStream> Inbound<TSvc, TStream>
where
    TStream: Stream + FusedStream + Unpin + Send + 'static,
    TStream::Item: Send + 'static,
    TSvc: Service<TStream::Item> + Clone + Send + 'static,
    TSvc::Error: Debug + Send,
    TSvc::Future: Send,
{
    pub fn new(executor: BoundedExecutor, stream: TStream, service: TSvc, shutdown_signal: ShutdownSignal) -> Self {
        Self {
            executor,
            stream,
            service,
            shutdown_signal,
        }
    }

    pub async fn run(mut self) {
        while let Some(item) = self.stream.next().await {
            // Check if the shutdown signal has been triggered.
            // If there are messages in the stream, drop them. Otherwise the stream is empty,
            // it will return None and the while loop will end.
            if self.shutdown_signal.is_terminated() {
                info!(
                    target: LOG_TARGET,
                    "Inbound pipeline is terminating because the shutdown signal is triggered"
                );
                return;
            }
            let service = self.service.clone();
            // Call the service in it's own spawned task
            self.executor
                .spawn(async move {
                    if let Err(err) = service.oneshot(item).await {
                        warn!(target: LOG_TARGET, "Inbound pipeline returned an error: '{:?}'", err);
                    }
                })
                .await;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::runtime;
    use futures::{channel::mpsc, future, stream};
    use std::time::Duration;
    use tari_shutdown::Shutdown;
    use tari_test_utils::collect_stream;
    use tokio::{runtime::Handle, time};
    use tower::service_fn;

    #[runtime::test_basic]
    async fn run() {
        let items = vec![1, 2, 3, 4, 5, 6];
        let stream = stream::iter(items.clone()).fuse();

        let (mut out_tx, mut out_rx) = mpsc::channel(items.len());

        let executor = Handle::current();
        let shutdown = Shutdown::new();
        let pipeline = Inbound::new(
            BoundedExecutor::new(executor.clone(), 1),
            stream,
            service_fn(move |req| {
                out_tx.try_send(req).unwrap();
                future::ready(Result::<_, ()>::Ok(()))
            }),
            shutdown.to_signal(),
        );
        let spawned_task = executor.spawn(pipeline.run());

        let received = collect_stream!(out_rx, take = items.len(), timeout = Duration::from_secs(10));
        assert!(received.iter().all(|i| items.contains(i)));

        // Check that this task ends because the stream has closed
        time::timeout(Duration::from_secs(5), spawned_task)
            .await
            .unwrap()
            .unwrap();
    }
}

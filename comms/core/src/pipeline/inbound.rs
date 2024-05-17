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

use std::{
    fmt::Display,
    time::{Duration, Instant},
};

use futures::future::FusedFuture;
use log::*;
use tari_shutdown::ShutdownSignal;
use tokio::{sync::mpsc, time};
use tower::{Service, ServiceExt};

use crate::bounded_executor::BoundedExecutor;

const LOG_TARGET: &str = "comms::pipeline::inbound";

/// Calls a Service with every item received from a Stream.
/// The difference between this can ServiceExt::call_all is
/// that ServicePipeline doesn't keep the result of the service
/// call and that it spawns a task for each incoming item.
pub struct Inbound<TSvc, TMsg> {
    executor: BoundedExecutor,
    service: TSvc,
    stream: mpsc::Receiver<TMsg>,
    shutdown_signal: ShutdownSignal,
}

impl<TSvc, TMsg> Inbound<TSvc, TMsg>
where
    TMsg: Send + 'static,
    TSvc: Service<TMsg> + Clone + Send + 'static,
    TSvc::Error: Display + Send,
    TSvc::Future: Send,
{
    /// New inbound pipeline.
    pub fn new(
        executor: BoundedExecutor,
        stream: mpsc::Receiver<TMsg>,
        service: TSvc,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            executor,
            service,
            stream,

            shutdown_signal,
        }
    }

    /// Run the inbounde pipeline. This returns a future that resolves once the stream has ended. Typically, you would
    /// spawn this in a new task.
    pub async fn run(mut self) {
        let mut current_id = 0;
        while let Some(item) = self.stream.recv().await {
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

            let num_available = self.executor.num_available();
            let max_available = self.executor.max_available();
            log!(
                target: LOG_TARGET,
                if num_available < max_available {
                    Level::Debug
                } else {
                    Level::Trace
                },
                "Inbound pipeline usage: {}/{}",
                max_available - num_available,
                max_available
            );

            let id = current_id;
            current_id = (current_id + 1) % u64::MAX;

            // Call the service in it's own spawned task
            self.executor
                .spawn(async move {
                    let timer = Instant::now();
                    trace!(target: LOG_TARGET, "Start inbound pipeline {}", id);
                    match time::timeout(Duration::from_secs(10), service.oneshot(item)).await {
                        Ok(Ok(_)) => {},
                        Ok(Err(err)) => {
                            warn!(target: LOG_TARGET, "Inbound pipeline returned an error: '{}'", err);
                        },
                        Err(_) => {
                            error!(
                                target: LOG_TARGET,
                                "Inbound pipeline {} timed out and was aborted. THIS SHOULD NOT HAPPEN: there was a \
                                 deadlock or excessive delay in processing this pipeline.",
                                id
                            );
                        },
                    }
                    trace!(
                        target: LOG_TARGET,
                        "Finished inbound pipeline {} in {:.2?}",
                        id,
                        timer.elapsed()
                    );
                })
                .await;
        }
        info!(target: LOG_TARGET, "Inbound pipeline terminated: the stream completed");
    }
}

#[cfg(test)]
mod test {
    use futures::future;
    use tari_shutdown::Shutdown;
    use tari_test_utils::collect_recv;
    use tower::service_fn;

    use super::*;

    #[tokio::test]
    async fn run() {
        let items = vec![1, 2, 3, 4, 5, 6];
        let (tx, mut stream) = mpsc::channel(items.len());
        for i in items.clone() {
            tx.send(i).await.unwrap();
        }
        stream.close();

        let (out_tx, mut out_rx) = mpsc::channel(items.len());

        let shutdown = Shutdown::new();
        let pipeline = Inbound::new(
            BoundedExecutor::new(1),
            stream,
            service_fn(move |req| {
                out_tx.try_send(req).unwrap();
                future::ready(Result::<_, String>::Ok(()))
            }),
            shutdown.to_signal(),
        );

        let spawned_task = tokio::spawn(pipeline.run());

        let received = collect_recv!(out_rx, take = items.len(), timeout = Duration::from_secs(10));
        assert!(received.iter().all(|i| items.contains(i)));

        // Check that this task ends because the stream has closed
        time::timeout(Duration::from_secs(5), spawned_task)
            .await
            .unwrap()
            .unwrap();
    }
}

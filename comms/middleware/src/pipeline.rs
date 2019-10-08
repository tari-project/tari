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

use futures::{
    channel::oneshot,
    future,
    future::Either,
    stream::FusedStream,
    FutureExt,
    Stream,
    StreamExt,
    TryFutureExt,
};
use log::*;
use std::fmt::Debug;
use tokio::runtime::TaskExecutor;
use tower::{Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::middleware::pipeline";

/// Calls a Service with every item received from a Stream.
/// The difference between this can ServiceExt::call_all is
/// that ServicePipeline doesn't keep the result of the service
/// call and that it spawns a task for each incoming item.
pub struct ServicePipeline<TSvc, TStream> {
    service: TSvc,
    stream: TStream,
    shutdown_signal: Option<oneshot::Receiver<()>>,
}

impl<TSvc, TStream> ServicePipeline<TSvc, TStream>
where
    TStream: Stream + FusedStream + Unpin + Send + 'static,
    TStream::Item: Send + 'static,
    TSvc: Service<TStream::Item> + Clone + Send + 'static,
    TSvc::Error: Debug + Send,
    TSvc::Future: Send,
{
    pub fn new(stream: TStream, service: TSvc) -> Self {
        Self {
            stream,
            service,
            shutdown_signal: None,
        }
    }

    pub fn with_shutdown_signal(mut self, shutdown_signal: oneshot::Receiver<()>) -> Self {
        self.shutdown_signal = Some(shutdown_signal);
        self
    }

    pub fn spawn_with(self, executor: TaskExecutor) {
        executor.spawn(self.run(executor.clone()).unwrap_or_else(|err| {
            error!(target: LOG_TARGET, "ServicePipeline error: {:?}", err);
            ()
        }));
    }

    pub async fn run(mut self, executor: TaskExecutor) -> Result<(), TSvc::Error> {
        // Check if the service is ready before reading the stream
        // to create back pressure on the stream if there is some
        // hold up with the service
        self.service.ready().await?;
        let mut stream = self.stream.fuse();
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .map(|fut| fut.map(|_| true))
            .map(FutureExt::fuse)
            .map(Either::Left)
            // By default, ready(false) is used to indicate that the pipeline
            // shouldn't shutdown. This is to make the shutdown signal optional.
            .unwrap_or(Either::Right(future::ready(false)));

        loop {
            futures::select! {
                item = stream.select_next_some() => {
                    let mut service = self.service.clone();
                    // Call the service on it's own spawned task
                    executor.spawn(async move {
                        match service.ready().await {
                            Ok(_) => {
                                if let Err(err) = service.call(item).await {
                                    // TODO: might want to dispatch this to tracing or provide an on_error callback
                                    error!(target: LOG_TARGET, "ServicePipeline error: {:?}", err);
                                }
                            },
                            Err(err) => {
                                // TODO: we shouldn't call the service again if poll_ready errors
                                error!(target: LOG_TARGET, "ServicePipeline error: {:?}", err);
                            },
                        }
                    })
                },

                should_shutdown = shutdown_signal => {
                    if should_shutdown {
                        debug!(target: LOG_TARGET, "ServicePipeline shut down");
                        break;
                    }
                },
                complete => {
                    debug!(target: LOG_TARGET, "ServicePipeline completed");
                    break;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::service_fn;
    use futures::{future, stream};
    use std::sync::{Arc, Mutex};
    use tokio::runtime::Runtime;

    #[test]
    fn run() {
        let rt = Runtime::new().unwrap();
        let items = vec![1, 2, 3, 4, 5, 6];
        let st = stream::iter(items.clone()).fuse();
        let collection = Arc::new(Mutex::new(Vec::new()));
        let cloned = Arc::clone(&collection);
        let pipeline = ServicePipeline::new(
            st,
            service_fn(move |req| {
                cloned.lock().unwrap().push(req);
                future::ready(Result::<_, ()>::Ok(()))
            }),
        );
        rt.block_on(pipeline.run(rt.executor())).unwrap();
        rt.shutdown_on_idle();
        {
            let c = collection.lock().unwrap();
            assert_eq!(c.len(), items.len());
            assert!(c.iter().all(|i| items.contains(i)));
        }
    }
}

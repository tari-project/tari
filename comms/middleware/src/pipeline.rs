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

use futures::{stream::FusedStream, Stream, StreamExt, TryFutureExt};
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
        Self { stream, service }
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
        while let Some(item) = self.stream.next().await {
            let mut service = self.service.clone();
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

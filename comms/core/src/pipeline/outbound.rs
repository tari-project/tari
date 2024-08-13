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

use log::*;
use tokio::time;
use tower::{Service, ServiceExt};

use crate::{bounded_executor::BoundedExecutor, pipeline::builder::OutboundPipelineConfig};

const LOG_TARGET: &str = "comms::pipeline::outbound";

/// Calls a service in a new task whenever a message is received by the configured channel and forwards the resulting
/// message as a [MessageRequest](crate::protocol::messaging::MessageRequest).
pub struct Outbound<TPipeline, TItem> {
    /// Executor used to spawn a pipeline for each received item on the stream
    executor: BoundedExecutor,
    /// Outbound pipeline configuration containing the pipeline and it's in and out streams
    config: OutboundPipelineConfig<TItem, TPipeline>,
}

impl<TPipeline, TItem> Outbound<TPipeline, TItem>
where
    TItem: Send + 'static,
    TPipeline: Service<TItem, Response = ()> + Clone + Send + 'static,
    TPipeline::Error: Display + Send,
    TPipeline::Future: Send,
{
    /// New outbound pipeline.
    pub fn new(executor: BoundedExecutor, config: OutboundPipelineConfig<TItem, TPipeline>) -> Self {
        Self { executor, config }
    }

    /// Run the outbound pipeline.
    pub async fn run(mut self) {
        let mut current_id = 0;

        while let Some(msg) = self.config.in_receiver.recv().await {
            // Pipeline IN received a message. Spawn a new task for the pipeline
            let num_available = self.executor.num_available();
            let max_available = self.executor.max_available();
            log!(
                target: LOG_TARGET,
                if num_available < max_available {
                    Level::Debug
                } else {
                    Level::Trace
                },
                "Outbound pipeline usage: {}/{}",
                max_available - num_available,
                max_available
            );

            let pipeline = self.config.pipeline.clone();
            let id = current_id;
            current_id = (current_id + 1) % u64::MAX;
            self.executor
                .spawn(async move {
                    let timer = Instant::now();
                    trace!(target: LOG_TARGET, "Start outbound pipeline {}", id);
                    match time::timeout(Duration::from_secs(10), pipeline.oneshot(msg)).await {
                        Ok(Ok(_)) => {},
                        Ok(Err(err)) => {
                            error!(
                                target: LOG_TARGET,
                                "Outbound pipeline {} returned an error: '{}'", id, err
                            );
                        },
                        Err(err) => {
                            error!(
                                target: LOG_TARGET,
                                "Outbound pipeline {} timed out and was aborted. THIS SHOULD NOT HAPPEN: there was a \
                                 deadlock or excessive delay in processing this pipeline. {}",
                                id,
                                err
                            );
                        },
                    }

                    trace!(
                        target: LOG_TARGET,
                        "Finished outbound pipeline {} in {:.2?}",
                        id,
                        timer.elapsed()
                    );
                })
                .await;
        }

        info!(
            target: LOG_TARGET,
            "Outbound pipeline is shutting down because the in channel closed"
        );
    }
}

#[cfg(test)]
mod test {
    use bytes::Bytes;
    use tari_test_utils::collect_recv;
    use tokio::sync::mpsc;

    use super::*;
    use crate::{message::OutboundMessage, pipeline::SinkService, utils};

    #[tokio::test]
    async fn run() {
        const NUM_ITEMS: usize = 10;
        let (tx, mut in_receiver) = mpsc::channel(NUM_ITEMS);
        utils::mpsc::send_all(
            &tx,
            (0..NUM_ITEMS).map(|i| OutboundMessage::new(Default::default(), Bytes::copy_from_slice(&i.to_be_bytes()))),
        )
        .await
        .unwrap();
        in_receiver.close();

        let (out_tx, mut out_rx) = mpsc::unbounded_channel();
        let executor = BoundedExecutor::new(100);

        let pipeline = Outbound::new(executor, OutboundPipelineConfig {
            in_receiver,
            out_receiver: None,
            pipeline: SinkService::new(out_tx),
        });

        let spawned_task = tokio::spawn(pipeline.run());

        let requests = collect_recv!(out_rx, timeout = Duration::from_millis(5));
        assert_eq!(requests.len(), NUM_ITEMS);

        // Check that this task ends because the stream has closed
        time::timeout(Duration::from_secs(5), spawned_task)
            .await
            .unwrap()
            .expect("Task should end")
    }
}

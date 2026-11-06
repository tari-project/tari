//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    task::Poll,
};

use futures::{future::BoxFuture, task::Context};
use log::*;
use tari_comms::{message::InboundMessage, peer_manager::NodeId, pipeline::PipelineError};
use tower::{Service, ServiceExt, layer::Layer};

use crate::connectivity::MetricsCollectorHandle;

const LOG_TARGET: &str = "comms::dht::metrics";

tokio::task_local! {
    /// The flood-ban sample of the inbound message being handled
    static FLOOD_SAMPLE: Arc<FloodSample>;
}

/// Records the flood-ban sample of the inbound message being handled. Called by the dedup layer as soon as it knows
/// whether the message counts against its peer: a duplicate that another peer delivered first only counts in the
/// node-wide total, so relays are not blamed for what they pass on.
pub(crate) async fn record_flood_sample(counts: bool) {
    if let Ok(sample) = FLOOD_SAMPLE.try_with(Arc::clone) {
        sample.record(counts).await;
    }
}

/// `FloodSample::state`: nothing decided yet
const PENDING: u8 = 0;
/// `FloodSample::state`: counts against the peer, not written yet
const COUNT: u8 = 1;
/// `FloodSample::state`: a duplicate another peer delivered first, not written yet
const SKIP: u8 = 2;
/// `FloodSample::state`: written
const RECORDED: u8 = 3;

/// One inbound message's flood-ban sample, recorded at most once: by the dedup layer, else when the inner layers
/// return, else (if they are cancelled, e.g. by the pipeline timeout, or panic) when it is dropped.
struct FloodSample {
    collector: MetricsCollectorHandle,
    source_peer: NodeId,
    state: AtomicU8,
}

impl FloodSample {
    async fn record(&self, counts: bool) {
        if self.state.load(Ordering::Relaxed) != PENDING {
            return;
        }
        // Decided before waiting, so a write finished from `Drop` keeps the decision
        self.state.store(if counts { COUNT } else { SKIP }, Ordering::Relaxed);
        // Waits for room in the collector's queue, so that samples are not dropped (and a flood under-counted) when
        // the collector falls behind
        let mut collector = self.collector.clone();
        let result = if counts {
            collector.write_metric_message_received(self.source_peer.clone()).await
        } else {
            collector.write_metric_relayed_duplicate_received().await
        };
        if result.is_err() {
            debug!(target: LOG_TARGET, "Unable to write metric: the metric collector is shut down");
        }
        self.state.store(RECORDED, Ordering::Relaxed);
    }
}

impl Drop for FloodSample {
    fn drop(&mut self) {
        let state = *self.state.get_mut();
        if state == RECORDED {
            return;
        }
        let mut collector = self.collector.clone();
        let source_peer = self.source_peer.clone();
        // Drop cannot wait for room in the queue, so the write is handed to a task
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ignore = if state == SKIP {
                    collector.write_metric_relayed_duplicate_received().await
                } else {
                    collector.write_metric_message_received(source_peer).await
                };
            });
        }
    }
}

/// Counts every inbound message against the peer that delivered it, for the flood ban, including messages the inner
/// layers drop, except duplicates first delivered by another peer.
#[derive(Clone)]
pub struct Metrics<S> {
    next_service: S,
    metric_collector: MetricsCollectorHandle,
}

impl<S> Metrics<S> {
    pub fn new(service: S, metric_collector: MetricsCollectorHandle) -> Self {
        Self {
            next_service: service,
            metric_collector,
        }
    }
}

impl<S> Service<InboundMessage> for Metrics<S>
where
    S: Service<InboundMessage, Response = (), Error = PipelineError> + Clone + Send + 'static,
    S::Future: Send,
{
    type Error = PipelineError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: InboundMessage) -> Self::Future {
        let next_service = self.next_service.clone();
        let sample = Arc::new(FloodSample {
            collector: self.metric_collector.clone(),
            source_peer: message.source_peer.clone(),
            state: AtomicU8::new(PENDING),
        });
        Box::pin(async move {
            // The inner layers run on this task, so the dedup layer reaches the sample through the task-local
            let result = FLOOD_SAMPLE.scope(sample.clone(), next_service.oneshot(message)).await;
            // The message never reached dedup (or dedup failed), so it counts
            sample.record(true).await;
            result
        })
    }
}

pub struct MetricsLayer {
    metric_collector: MetricsCollectorHandle,
}

impl MetricsLayer {
    pub fn new(metric_collector: MetricsCollectorHandle) -> Self {
        Self { metric_collector }
    }
}

impl<S> Layer<S> for MetricsLayer {
    type Service = Metrics<S>;

    fn layer(&self, service: S) -> Self::Service {
        Metrics::new(service, self.metric_collector.clone())
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use futures::future;
    use tari_comms::Bytes;
    use tari_test_utils::async_assert;
    use tokio::time;
    use tower::service_fn;

    use super::*;
    use crate::{
        connectivity::{MetricsCollector, metrics::METRICS_QUEUE_SIZE},
        test_utils::make_node_identity,
    };

    /// A message whose handling is cancelled (here: by a timeout, like the pipeline's) before it is counted still
    /// counts
    #[tokio::test]
    async fn a_cancelled_message_still_counts() {
        let mut collector = MetricsCollector::spawn(11, Duration::from_secs(60));
        let mut service = MetricsLayer::new(collector.clone()).layer(service_fn(|_: InboundMessage| {
            future::pending::<Result<(), PipelineError>>()
        }));
        let source_peer = NodeId::default();

        let call = service.call(InboundMessage::new(source_peer.clone(), Bytes::new()));
        assert!(time::timeout(Duration::from_millis(10), call).await.is_err());

        async_assert!(
            collector
                .get_messages_received_timeseries(source_peer.clone())
                .await
                .unwrap()
                .count() ==
                1,
            max_attempts = 20,
            interval = Duration::from_millis(10),
        );
    }

    /// A relayed duplicate whose write is still waiting for room in the queue when its handling is cancelled keeps the
    /// skip decision: it counts in the node-wide total, not against the relay
    #[tokio::test]
    async fn a_cancelled_relayed_duplicate_is_not_charged_to_the_relay() {
        let (mut collector, runner) = MetricsCollector::new(11, Duration::from_secs(60));
        // The collector is not running yet, so these fill its queue
        let filler = NodeId::default();
        for _ in 0..METRICS_QUEUE_SIZE {
            collector.write_metric_message_received(filler.clone()).await.unwrap();
        }
        let mut service = MetricsLayer::new(collector.clone()).layer(service_fn(|_: InboundMessage| async {
            record_flood_sample(false).await;
            Ok::<_, PipelineError>(())
        }));
        let relay = make_node_identity().node_id().clone();

        let call = service.call(InboundMessage::new(relay.clone(), Bytes::new()));
        assert!(time::timeout(Duration::from_millis(10), call).await.is_err());
        tokio::spawn(runner.run());

        async_assert!(
            collector
                .get_total_message_count_in_timespan(Duration::from_secs(60))
                .await
                .unwrap() ==
                METRICS_QUEUE_SIZE + 1,
            max_attempts = 20,
            interval = Duration::from_millis(10),
        );
        let ts = collector.get_messages_received_timeseries(relay).await.unwrap();
        assert_eq!(ts.count(), 0);
    }
}

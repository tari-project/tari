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
        atomic::{AtomicBool, Ordering},
    },
    task::Poll,
};

use futures::{future::BoxFuture, task::Context};
use log::*;
use tari_comms::{message::InboundMessage, pipeline::PipelineError};
use tower::{Service, ServiceExt, layer::Layer};

use crate::connectivity::MetricsCollectorHandle;

const LOG_TARGET: &str = "comms::dht::metrics";

tokio::task_local! {
    /// Whether the inbound message being handled counts towards the flood ban of the peer that delivered it
    static COUNTS_TOWARDS_FLOOD_BAN: Arc<AtomicBool>;
}

/// Stops the inbound message being handled from counting towards the flood ban. Called by the dedup layer for a
/// duplicate that another peer delivered first, so relays are not blamed for what they pass on.
pub(crate) fn skip_flood_ban_count() {
    let _ignore = COUNTS_TOWARDS_FLOOD_BAN.try_with(|counts| counts.store(false, Ordering::Relaxed));
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
        let mut metric_collector = self.metric_collector.clone();
        Box::pin(async move {
            let source_peer = message.source_peer.clone();
            let counts = Arc::new(AtomicBool::new(true));
            // The inner layers run on this task, so the dedup layer can clear `counts` through the task-local
            let result = COUNTS_TOWARDS_FLOOD_BAN
                .scope(counts.clone(), next_service.oneshot(message))
                .await;
            // Waits for room in the collector's queue, so that samples are not dropped (and a flood under-counted)
            // when the collector falls behind
            if counts.load(Ordering::Relaxed) &&
                metric_collector
                    .write_metric_message_received(source_peer)
                    .await
                    .is_err()
            {
                debug!(target: LOG_TARGET, "Unable to write metric: the metric collector is shut down");
            }
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

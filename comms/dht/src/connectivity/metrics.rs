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

use futures::{
    channel::{mpsc, mpsc::SendError, oneshot, oneshot::Canceled},
    future,
    SinkExt,
    StreamExt,
};
use log::*;
use std::{
    collections::{hash_map::Entry, HashMap, VecDeque},
    future::Future,
    time::{Duration, Instant},
};
use tari_comms::peer_manager::NodeId;
use tokio::task;

const LOG_TARGET: &str = "comms::dht::metrics";

#[derive(Debug)]
enum MetricOp {
    Write(MetricWrite),
    Read(MetricRead),
}

#[derive(Debug)]
pub enum MetricWrite {
    MessageReceived(NodeId),
    ClearMetrics(NodeId),
}

#[derive(Debug)]
pub enum MetricRead {
    MessagesReceivedGetTimeseries(NodeId, oneshot::Sender<TimeSeries<()>>),
    MessagesReceivedRateExceeding((usize, Duration), oneshot::Sender<Vec<(NodeId, f32)>>),
    MessagesReceivedTotalCountInTimespan(Duration, oneshot::Sender<usize>),
}

#[derive(Debug)]
struct MetricsState {
    messages_recv: HashMap<NodeId, TimeSeries<()>>,
    all_messages_recv: TimeSeries<()>,
}

impl Default for MetricsState {
    fn default() -> Self {
        Self {
            all_messages_recv: TimeSeries::new(100000),
            messages_recv: HashMap::<NodeId, TimeSeries<()>>::new(),
        }
    }
}

impl MetricsState {
    pub fn add_message_received(&mut self, node_id: NodeId) {
        match self.messages_recv.entry(node_id) {
            Entry::Occupied(mut entry) => {
                let node_id = entry.key();
                let ts = entry.get();
                debug!(
                    target: LOG_TARGET,
                    "Received {} messages in {:.0?} from `{}`",
                    ts.count() + 1,
                    ts.timespan().expect("Time series did not contain a data point"),
                    node_id,
                );
                entry.get_mut().inc();
            },
            Entry::Vacant(entry) => {
                let mut t = TimeSeries::new(11_000);
                t.inc();
                entry.insert(t);
            },
        }
        self.all_messages_recv.inc();
    }

    pub fn drop_metrics(&mut self, node_id: &NodeId) {
        self.messages_recv.remove(node_id);
    }

    pub fn message_received_get_timeseries(&self, node_id: &NodeId) -> TimeSeries<()> {
        self.messages_recv.get(node_id).cloned().unwrap_or_default()
    }

    pub fn message_received_get_nodes_exceeding(&self, counts: usize, timespan: Duration) -> Vec<(NodeId, f32)> {
        let since = Instant::now() - timespan;
        self.messages_recv
            .iter()
            .filter_map(|(node_id, t)| {
                if t.count_since(since) > counts {
                    Some((node_id.clone(), t.rate()))
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn message_received_get_total_count_in_timespan(&self, timespan: Duration) -> usize {
        let since = Instant::now() - timespan;
        self.all_messages_recv.count_since(since)
    }
}

pub struct MetricsCollector {
    stream: Option<mpsc::Receiver<MetricOp>>,
    state: MetricsState,
}

impl MetricsCollector {
    pub fn spawn() -> MetricsCollectorHandle {
        let (metrics_tx, metrics_rx) = mpsc::channel(500);
        let metrics_collector = MetricsCollectorHandle::new(metrics_tx);
        let collector = Self {
            stream: Some(metrics_rx),
            state: Default::default(),
        };
        task::spawn(collector.run());
        metrics_collector
    }

    fn run(mut self) -> impl Future<Output = ()> {
        self.stream.take().unwrap().for_each(move |op| {
            self.handle(op);
            future::ready(())
        })
    }

    fn handle(&mut self, op: MetricOp) {
        use MetricOp::*;
        match op {
            Write(write) => self.handle_write(write),
            Read(read) => self.handle_read(read),
        }
    }

    fn handle_write(&mut self, write: MetricWrite) {
        use MetricWrite::*;
        match write {
            MessageReceived(node_id) => {
                self.state.add_message_received(node_id);
            },
            ClearMetrics(node_id) => {
                self.state.drop_metrics(&node_id);
            },
        }
    }

    fn handle_read(&mut self, query: MetricRead) {
        use MetricRead::*;
        match query {
            MessagesReceivedGetTimeseries(node_id, reply) => {
                let _ = reply.send(self.state.message_received_get_timeseries(&node_id));
            },
            MessagesReceivedRateExceeding((counts, timespan), reply) => {
                let _ = reply.send(self.state.message_received_get_nodes_exceeding(counts, timespan));
            },
            MessagesReceivedTotalCountInTimespan(timespan, reply) => {
                let _ = reply.send(self.state.message_received_get_total_count_in_timespan(timespan));
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimeSeries<T> {
    values: VecDeque<(Instant, T)>,
    capacity: usize,
}

impl<T> TimeSeries<T> {
    pub fn new(capacity: usize) -> Self {
        Self {
            values: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    pub fn empty() -> Self {
        Self {
            values: VecDeque::new(),
            capacity: 100,
        }
    }

    pub fn push(&mut self, item: T) {
        if self.capacity == 0 {
            return;
        }
        debug_assert!(
            self.values.len() <= self.capacity,
            "TimeSeries::values exceeded capacity"
        );
        if self.values.len() == self.capacity {
            self.values.pop_front();
        }
        self.values.push_back((Instant::now(), item));
    }

    pub fn count_since(&self, when: Instant) -> usize {
        self.values.iter().filter(|(t, _)| t >= &when).count()
    }

    pub fn count(&self) -> usize {
        self.values.len()
    }

    pub fn timespan(&self) -> Option<Duration> {
        self.values.front().map(|(i, _)| i.elapsed())
    }

    /// Return the rate at which samples occur within this timeseries
    pub fn rate(&self) -> f32 {
        self.timespan()
            .map(|timespan| {
                let timespan = timespan.as_secs();
                if timespan == 0 {
                    return 0f32;
                }

                let num_samples = self.values.len();
                num_samples as f32 / timespan as f32
            })
            .unwrap_or(0f32)
    }
}

impl TimeSeries<()> {
    pub fn inc(&mut self) {
        self.push(());
    }
}

impl<T> Default for TimeSeries<T> {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone)]
pub struct MetricsCollectorHandle {
    inner: mpsc::Sender<MetricOp>,
}

impl MetricsCollectorHandle {
    fn new(sender: mpsc::Sender<MetricOp>) -> Self {
        Self { inner: sender }
    }

    /// Write the MessageReceived metric for the given `NodeId`. Returning true if the metric was queued for collection,
    /// otherwise false. A metric may not be collected when there are many writes.
    pub fn write_metric_message_received(&mut self, node_id: NodeId) -> bool {
        self.write(MetricWrite::MessageReceived(node_id))
    }

    /// Clear the metrics for a `NodeId`. Err is returned if the metric collector has been shut down.
    pub async fn clear_metrics(&mut self, node_id: NodeId) -> Result<(), MetricsError> {
        self.inner
            .send(MetricOp::Write(MetricWrite::ClearMetrics(node_id)))
            .await
            .map_err(Into::into)
    }

    fn write(&mut self, write: MetricWrite) -> bool {
        match self.inner.try_send(MetricOp::Write(write)) {
            Ok(_) => true,
            Err(err) => {
                warn!(target: LOG_TARGET, "Failed to write metric: {}", err.into_send_error());
                false
            },
        }
    }

    /// Get (NodeId, messages per second) tuples that exceed the given number of messages within the given time
    pub async fn get_messages_received_timeseries(&mut self, node_id: NodeId) -> Result<TimeSeries<()>, MetricsError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .send(MetricOp::Read(MetricRead::MessagesReceivedGetTimeseries(
                node_id, reply_tx,
            )))
            .await?;
        reply_rx.await.map_err(Into::into)
    }

    /// Get (NodeId, messages per second) tuples that exceed the given number of messages within the given time
    pub async fn get_message_rates_exceeding(
        &mut self,
        counts: usize,
        timespan: Duration,
    ) -> Result<Vec<(NodeId, f32)>, MetricsError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .send(MetricOp::Read(MetricRead::MessagesReceivedRateExceeding(
                (counts, timespan),
                reply_tx,
            )))
            .await?;
        reply_rx.await.map_err(Into::into)
    }

    pub async fn get_total_message_count_in_timespan(&mut self, timespan: Duration) -> Result<usize, MetricsError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .send(MetricOp::Read(MetricRead::MessagesReceivedTotalCountInTimespan(
                timespan, reply_tx,
            )))
            .await?;
        reply_rx.await.map_err(Into::into)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MetricsError {
    #[error("Internal channel closed unexpectedly")]
    ChannelClosedUnexpectedly,
    #[error("Reply unexpectedly cancelled")]
    ReplyCancelled,
}

impl From<mpsc::SendError> for MetricsError {
    fn from(_: SendError) -> Self {
        MetricsError::ChannelClosedUnexpectedly
    }
}

impl From<oneshot::Canceled> for MetricsError {
    fn from(_: Canceled) -> Self {
        MetricsError::ReplyCancelled
    }
}

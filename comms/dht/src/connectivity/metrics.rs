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
    collections::{HashMap, VecDeque, hash_map::Entry},
    time::Duration,
};

use log::*;
use tari_comms::peer_manager::NodeId;
use tokio::{
    sync::{mpsc, oneshot},
    task,
    time,
    time::{Instant, MissedTickBehavior},
};

const LOG_TARGET: &str = "comms::dht::metrics";

/// Number of samples kept for the node-wide message count. A single instance, so it is allocated up front.
const ALL_MESSAGES_RECV_CAPACITY: usize = 100_000;

#[derive(Debug)]
enum MetricOp {
    Write(MetricWrite),
    Read(MetricRead),
}

#[derive(Debug)]
pub enum MetricWrite {
    MessageReceived(NodeId),
}

#[derive(Debug)]
pub enum MetricRead {
    GetMessagesReceivedTimeseries(NodeId, oneshot::Sender<TimeSeries<()>>),
    ExceedingMessagesReceivedRate((usize, Duration), oneshot::Sender<Vec<(NodeId, f32)>>),
    MessagesReceivedTotalCountInTimespan(Duration, oneshot::Sender<usize>),
}

#[derive(Debug)]
struct MetricsState {
    messages_recv: HashMap<NodeId, TimeSeries<()>>,
    all_messages_recv: TimeSeries<()>,
    /// Maximum number of samples kept per peer
    per_peer_capacity: usize,
}

impl MetricsState {
    fn new(per_peer_capacity: usize) -> Self {
        Self {
            messages_recv: HashMap::new(),
            all_messages_recv: TimeSeries::new(ALL_MESSAGES_RECV_CAPACITY),
            per_peer_capacity,
        }
    }

    pub fn add_message_received(&mut self, node_id: NodeId) {
        match self.messages_recv.entry(node_id) {
            Entry::Occupied(mut entry) => {
                let node_id = entry.key();
                let ts = entry.get();
                debug!(
                    target: LOG_TARGET,
                    "Received {} messages in {:.0?} from `{}`",
                    ts.count().saturating_add(1),
                    ts.timespan().expect("Time series did not contain a data point"),
                    node_id,
                );
                entry.get_mut().inc();
            },
            Entry::Vacant(entry) => {
                // Most peers send few messages, so the series grows as samples arrive instead of being allocated
                // to its full capacity up front
                let mut t = TimeSeries::new_unallocated(self.per_peer_capacity);
                t.inc();
                entry.insert(t);
            },
        }
        self.all_messages_recv.inc();
    }

    /// Drop every per-peer sample older than `max_age`, and the entries left without samples.
    pub fn prune_stale(&mut self, max_age: Duration) {
        let since = Instant::now().checked_sub(max_age).unwrap_or_else(Instant::now);
        let num_before = self.messages_recv.len();
        self.messages_recv.retain(|_, t| {
            t.drop_older_than(since);
            t.count() > 0
        });
        let num_pruned = num_before.saturating_sub(self.messages_recv.len());
        if num_pruned > 0 {
            debug!(
                target: LOG_TARGET,
                "Pruned message metrics for {num_pruned} peer(s) with no messages in {max_age:.0?}",
            );
        }
    }

    pub fn message_received_get_timeseries(&self, node_id: &NodeId) -> TimeSeries<()> {
        self.messages_recv.get(node_id).cloned().unwrap_or_default()
    }

    pub fn message_received_get_nodes_exceeding(&self, counts: usize, timespan: Duration) -> Vec<(NodeId, f32)> {
        let since = Instant::now().checked_sub(timespan).unwrap_or_else(Instant::now);
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
        let since = Instant::now().checked_sub(timespan).unwrap_or_else(Instant::now);
        self.all_messages_recv.count_since(since)
    }
}

pub struct MetricsCollector {
    stream: mpsc::Receiver<MetricOp>,
    state: MetricsState,
    /// The flood-ban window. Per-peer samples older than this are pruned, once per window.
    flood_ban_timespan: Duration,
}

impl MetricsCollector {
    /// Spawn the collector. `per_peer_capacity` is the maximum number of message samples kept per peer. It must be
    /// more than the flood-ban message count, so that a flooding peer can be detected. `flood_ban_timespan` is the
    /// flood-ban window: samples are kept for at least this long, across disconnects, so that a peer cannot reset its
    /// count by reconnecting.
    pub fn spawn(per_peer_capacity: usize, flood_ban_timespan: Duration) -> MetricsCollectorHandle {
        let (metrics_tx, metrics_rx) = mpsc::channel(500);
        let metrics_collector = MetricsCollectorHandle::new(metrics_tx);
        let collector = Self {
            stream: metrics_rx,
            state: MetricsState::new(per_peer_capacity),
            flood_ban_timespan,
        };
        task::spawn(collector.run());
        metrics_collector
    }

    async fn run(mut self) {
        let first_prune = Instant::now()
            .checked_add(self.flood_ban_timespan)
            .unwrap_or_else(Instant::now);
        let mut prune_ticker = time::interval_at(first_prune, self.flood_ban_timespan);
        prune_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                maybe_op = self.stream.recv() => {
                    match maybe_op {
                        Some(op) => self.handle(op),
                        None => break,
                    }
                },
                _ = prune_ticker.tick() => {
                    self.state.prune_stale(self.flood_ban_timespan);
                },
            }
        }
    }

    fn handle(&mut self, op: MetricOp) {
        use MetricOp::{Read, Write};
        match op {
            Write(write) => self.handle_write(write),
            Read(read) => self.handle_read(read),
        }
    }

    fn handle_write(&mut self, write: MetricWrite) {
        match write {
            MetricWrite::MessageReceived(node_id) => {
                self.state.add_message_received(node_id);
            },
        }
    }

    fn handle_read(&mut self, query: MetricRead) {
        #[allow(clippy::enum_glob_use)]
        use MetricRead::*;
        match query {
            GetMessagesReceivedTimeseries(node_id, reply) => {
                let _result = reply.send(self.state.message_received_get_timeseries(&node_id));
            },
            ExceedingMessagesReceivedRate((counts, timespan), reply) => {
                let _result = reply.send(self.state.message_received_get_nodes_exceeding(counts, timespan));
            },
            MessagesReceivedTotalCountInTimespan(timespan, reply) => {
                let _result = reply.send(self.state.message_received_get_total_count_in_timespan(timespan));
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

    /// Create a series that holds at most `capacity` samples, allocating as samples arrive.
    pub fn new_unallocated(capacity: usize) -> Self {
        Self {
            values: VecDeque::new(),
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

    /// Drop the samples older than `when`, releasing their memory.
    pub fn drop_older_than(&mut self, when: Instant) {
        let num_before = self.values.len();
        while self.values.front().is_some_and(|(t, _)| *t < when) {
            self.values.pop_front();
        }
        if self.values.len() < num_before {
            self.values.shrink_to_fit();
        }
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

    /// Write the MessageReceived metric for the given `NodeId`, waiting for room in the queue if the collector is
    /// behind. Err is returned if the metric collector has been shut down.
    pub async fn write_metric_message_received(&mut self, node_id: NodeId) -> Result<(), MetricsError> {
        self.inner
            .send(MetricOp::Write(MetricWrite::MessageReceived(node_id)))
            .await
            .map_err(Into::into)
    }

    /// Get (NodeId, messages per second) tuples that exceed the given number of messages within the given time
    pub async fn get_messages_received_timeseries(&mut self, node_id: NodeId) -> Result<TimeSeries<()>, MetricsError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .send(MetricOp::Read(MetricRead::GetMessagesReceivedTimeseries(
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
    ) -> Result<Vec<(NodeId, f32)>, MetricsError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .send(MetricOp::Read(MetricRead::ExceedingMessagesReceivedRate(
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

impl<T> From<mpsc::error::SendError<T>> for MetricsError {
    fn from(_: mpsc::error::SendError<T>) -> Self {
        MetricsError::ChannelClosedUnexpectedly
    }
}

impl From<oneshot::error::RecvError> for MetricsError {
    fn from(_: oneshot::error::RecvError) -> Self {
        MetricsError::ReplyCancelled
    }
}

#[cfg(test)]
mod test {
    use tari_utilities::ByteArray;

    use super::*;

    const FLOOD_BAN_MAX_MSG_COUNT: usize = 1000;
    const PER_PEER_CAPACITY: usize = FLOOD_BAN_MAX_MSG_COUNT + 1;
    const FLOOD_BAN_TIMESPAN: Duration = Duration::from_secs(60);

    fn node_id(byte: u8) -> NodeId {
        NodeId::from_canonical_bytes(&[byte; NodeId::byte_size()]).unwrap()
    }

    #[test]
    fn it_allocates_peer_entries_lazily() {
        let mut state = MetricsState::new(PER_PEER_CAPACITY);
        state.add_message_received(node_id(1));
        let ts = state.messages_recv.get(&node_id(1)).unwrap();
        assert_eq!(ts.count(), 1);
        assert!(
            ts.values.capacity() < PER_PEER_CAPACITY,
            "per-peer series was preallocated to {} samples",
            ts.values.capacity()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn it_evicts_the_oldest_sample_beyond_the_cap() {
        let mut state = MetricsState::new(PER_PEER_CAPACITY);
        state.add_message_received(node_id(1));
        time::advance(Duration::from_secs(1)).await;
        let after_first = Instant::now();
        for _ in 0..PER_PEER_CAPACITY {
            state.add_message_received(node_id(1));
        }
        let ts = state.messages_recv.get(&node_id(1)).unwrap();
        assert_eq!(ts.count(), PER_PEER_CAPACITY);
        // The first sample was evicted, all remaining samples are newer
        assert_eq!(ts.count_since(after_first), PER_PEER_CAPACITY);
    }

    #[tokio::test(start_paused = true)]
    async fn it_detects_flooding_at_max_count_plus_one() {
        let timespan = Duration::from_secs(60);
        let mut state = MetricsState::new(PER_PEER_CAPACITY);
        for _ in 0..FLOOD_BAN_MAX_MSG_COUNT {
            state.add_message_received(node_id(1));
        }
        assert!(
            state
                .message_received_get_nodes_exceeding(FLOOD_BAN_MAX_MSG_COUNT, timespan)
                .is_empty()
        );

        state.add_message_received(node_id(1));
        let exceeding = state.message_received_get_nodes_exceeding(FLOOD_BAN_MAX_MSG_COUNT, timespan);
        assert_eq!(exceeding.len(), 1);
        assert_eq!(exceeding.first().map(|(n, _)| n), Some(&node_id(1)));

        // Still detected once the cap is reached and old samples are being evicted
        for _ in 0..500 {
            state.add_message_received(node_id(1));
        }
        let exceeding = state.message_received_get_nodes_exceeding(FLOOD_BAN_MAX_MSG_COUNT, timespan);
        assert_eq!(exceeding.len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn it_prunes_samples_outside_the_window_at_each_tick() {
        let mut collector = MetricsCollector::spawn(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        let old_peer = node_id(1);
        let recent_peer = node_id(2);

        // t = 10s
        time::sleep(Duration::from_secs(10)).await;
        collector.write_metric_message_received(old_peer.clone()).await.unwrap();
        collector
            .write_metric_message_received(recent_peer.clone())
            .await
            .unwrap();

        // t = 100s. The 60s tick kept both samples: they were inside the window
        time::sleep(Duration::from_secs(90)).await;
        let ts = collector
            .get_messages_received_timeseries(old_peer.clone())
            .await
            .unwrap();
        assert_eq!(ts.count(), 1);
        collector
            .write_metric_message_received(recent_peer.clone())
            .await
            .unwrap();

        // t = 125s. The 120s tick dropped the samples from t = 10s, and with them the old peer's entry
        time::sleep(Duration::from_secs(25)).await;
        let ts = collector.get_messages_received_timeseries(old_peer).await.unwrap();
        assert_eq!(ts.count(), 0);
        let ts = collector.get_messages_received_timeseries(recent_peer).await.unwrap();
        assert_eq!(ts.count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn pruning_releases_the_memory_of_old_samples() {
        let mut state = MetricsState::new(PER_PEER_CAPACITY);
        for _ in 0..FLOOD_BAN_MAX_MSG_COUNT {
            state.add_message_received(node_id(1));
        }
        time::advance(FLOOD_BAN_TIMESPAN + Duration::from_secs(1)).await;
        state.add_message_received(node_id(1));

        state.prune_stale(FLOOD_BAN_TIMESPAN);
        let ts = state.messages_recv.get(&node_id(1)).unwrap();
        assert_eq!(ts.count(), 1);
        assert!(ts.values.capacity() < 10, "capacity {}", ts.values.capacity());
    }
}

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
    collections::{HashMap, VecDeque},
    fmt,
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

use crate::config::FORWARDED_FLOOD_BAN_FACTOR;

const LOG_TARGET: &str = "comms::dht::metrics";

/// Number of samples kept for the node-wide message count. A single instance, so it is allocated up front.
const ALL_MESSAGES_RECV_CAPACITY: usize = 100_000;

/// Capacity of the collector's request queue
pub(crate) const METRICS_QUEUE_SIZE: usize = 500;

#[derive(Debug)]
enum MetricOp {
    Write(MetricWrite),
    Read(MetricRead),
}

/// The per-peer flood-ban series a message counts in
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FloodSeries {
    /// Signed by the delivering peer, or invalid in a way only it is to blame for. Limit: the flood-ban count.
    Own,
    /// Signed by another peer, or not attributable: the delivering peer may only be relaying it. Limit:
    /// `FORWARDED_FLOOD_BAN_FACTOR` x the flood-ban count.
    Forwarded,
}

impl fmt::Display for FloodSeries {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FloodSeries::Own => write!(f, "own"),
            FloodSeries::Forwarded => write!(f, "forwarded"),
        }
    }
}

#[derive(Debug)]
pub enum MetricWrite {
    /// A message from `NodeId` that counts in `FloodSeries`, received (entered the inbound pipeline) at `Instant`
    MessageReceived(NodeId, FloodSeries, Instant),
    /// A message counted in the node-wide total only, not against the peer: a duplicate another peer delivered first,
    /// or one whose dedup result is unknown
    UncountedMessageReceived,
}

#[derive(Debug)]
pub enum MetricRead {
    GetMessagesReceivedTimeseries(NodeId, FloodSeries, oneshot::Sender<TimeSeries<()>>),
    TakeFloodingPeers(oneshot::Sender<Vec<(NodeId, f32, FloodSeries)>>),
    MessagesReceivedTotalCountInTimespan(Duration, oneshot::Sender<usize>),
}

#[derive(Debug)]
struct MetricsState {
    messages_recv: HashMap<NodeId, TimeSeries<()>>,
    forwarded_messages_recv: HashMap<NodeId, TimeSeries<()>>,
    all_messages_recv: TimeSeries<()>,
    /// Maximum number of own-series samples kept per peer, one more than the flood-ban message count
    per_peer_capacity: usize,
    flood_ban_timespan: Duration,
    /// Peers that went over a limit within `flood_ban_timespan` since the last flood check, with their message rate
    /// and the series. Kept apart from the samples, so pruning cannot lose a detection.
    flooding: HashMap<NodeId, (f32, FloodSeries)>,
}

impl MetricsState {
    fn new(per_peer_capacity: usize, flood_ban_timespan: Duration) -> Self {
        Self {
            messages_recv: HashMap::new(),
            forwarded_messages_recv: HashMap::new(),
            all_messages_recv: TimeSeries::new(ALL_MESSAGES_RECV_CAPACITY),
            per_peer_capacity,
            flood_ban_timespan,
            flooding: HashMap::new(),
        }
    }

    /// Records a message and checks the series it lands in. The series keeps only its newest `capacity` samples, in
    /// time order, so it holds more than the limit within one window exactly when it is full and its newest and
    /// oldest samples are at most `flood_ban_timespan` apart. Checking on every arrival sees every flood, however
    /// late the flood check runs.
    pub fn add_message_received(&mut self, node_id: NodeId, series: FloodSeries, received_at: Instant) {
        let (peers, capacity) = match series {
            FloodSeries::Own => (&mut self.messages_recv, self.per_peer_capacity),
            FloodSeries::Forwarded => (
                &mut self.forwarded_messages_recv,
                self.per_peer_capacity
                    .saturating_sub(1)
                    .saturating_mul(FORWARDED_FLOOD_BAN_FACTOR)
                    .saturating_add(1),
            ),
        };
        // Most peers send few messages, so a series grows as samples arrive instead of being allocated to its full
        // capacity up front
        let ts = peers
            .entry(node_id.clone())
            .or_insert_with(|| TimeSeries::new_unallocated(capacity));
        ts.push_at(received_at, ());
        if ts.count() >= capacity && ts.span().is_some_and(|span| span <= self.flood_ban_timespan) {
            self.flooding.insert(node_id, (ts.rate(), series));
        }
        self.all_messages_recv.inc();
    }

    /// Drop every per-peer sample older than `max_age`, and the entries left without samples.
    pub fn prune_stale(&mut self, max_age: Duration) {
        // The clock started less than `max_age` ago (as it does at boot on Windows): nothing is old enough to prune
        let Some(since) = Instant::now().checked_sub(max_age) else {
            return;
        };
        let num_before = self
            .messages_recv
            .len()
            .saturating_add(self.forwarded_messages_recv.len());
        for peers in [&mut self.messages_recv, &mut self.forwarded_messages_recv] {
            peers.retain(|_, t| {
                t.drop_older_than(since);
                t.count() > 0
            });
        }
        let num_pruned = num_before
            .saturating_sub(self.messages_recv.len())
            .saturating_sub(self.forwarded_messages_recv.len());
        if num_pruned > 0 {
            debug!(
                target: LOG_TARGET,
                "Pruned message metrics for {num_pruned} peer(s) with no messages in {max_age:.0?}",
            );
        }
    }

    pub fn message_received_get_timeseries(&self, node_id: &NodeId, series: FloodSeries) -> TimeSeries<()> {
        let peers = match series {
            FloodSeries::Own => &self.messages_recv,
            FloodSeries::Forwarded => &self.forwarded_messages_recv,
        };
        peers.get(node_id).cloned().unwrap_or_default()
    }

    /// The peers that exceeded a flood-ban limit since the last call, with their message rate and the series
    pub fn take_flooding_peers(&mut self) -> Vec<(NodeId, f32, FloodSeries)> {
        self.flooding
            .drain()
            .map(|(node_id, (rate, series))| (node_id, rate, series))
            .collect()
    }

    pub fn message_received_get_total_count_in_timespan(&self, timespan: Duration) -> usize {
        // The clock started less than `timespan` ago (as it does at boot on Windows): every sample is inside it
        match Instant::now().checked_sub(timespan) {
            Some(since) => self.all_messages_recv.count_since(since),
            None => self.all_messages_recv.count(),
        }
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
        let (metrics_collector, collector) = Self::new(per_peer_capacity, flood_ban_timespan);
        task::spawn(collector.run());
        metrics_collector
    }

    /// Create the collector without running it. See [`MetricsCollector::spawn`].
    pub(crate) fn new(per_peer_capacity: usize, flood_ban_timespan: Duration) -> (MetricsCollectorHandle, Self) {
        let (metrics_tx, metrics_rx) = mpsc::channel(METRICS_QUEUE_SIZE);
        let collector = Self {
            stream: metrics_rx,
            state: MetricsState::new(per_peer_capacity, flood_ban_timespan),
            flood_ban_timespan,
        };
        (MetricsCollectorHandle::new(metrics_tx), collector)
    }

    pub(crate) async fn run(mut self) {
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
            MetricWrite::MessageReceived(node_id, series, received_at) => {
                self.state.add_message_received(node_id, series, received_at);
            },
            MetricWrite::UncountedMessageReceived => {
                self.state.all_messages_recv.inc();
            },
        }
    }

    fn handle_read(&mut self, query: MetricRead) {
        #[allow(clippy::enum_glob_use)]
        use MetricRead::*;
        match query {
            GetMessagesReceivedTimeseries(node_id, series, reply) => {
                let _result = reply.send(self.state.message_received_get_timeseries(&node_id, series));
            },
            TakeFloodingPeers(reply) => {
                let _result = reply.send(self.state.take_flooding_peers());
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
        self.push_at(Instant::now(), item);
    }

    /// Add a sample taken at `at`, keeping the samples in time order and dropping the oldest beyond the capacity.
    /// Samples are taken when messages enter the inbound pipeline and written as concurrent pipeline tasks get to
    /// them, so they arrive nearly in order and the search from the newest end is short.
    pub fn push_at(&mut self, at: Instant, item: T) {
        if self.capacity == 0 {
            return;
        }
        let pos = self
            .values
            .iter()
            .rposition(|(t, _)| *t <= at)
            .map_or(0, |i| i.saturating_add(1));
        self.values.insert(pos, (at, item));
        if self.values.len() > self.capacity {
            self.values.pop_front();
        }
    }

    /// The time between the oldest and the newest sample
    pub fn span(&self) -> Option<Duration> {
        let oldest = self.values.front()?.0;
        let newest = self.values.back()?.0;
        Some(newest.saturating_duration_since(oldest))
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

    /// Write a message from `node_id` that counts in `series`, received at `received_at`, waiting for room in the
    /// queue if the collector is behind. Err is returned if the metric collector has been shut down.
    pub async fn write_metric_message_received(
        &mut self,
        node_id: NodeId,
        series: FloodSeries,
        received_at: Instant,
    ) -> Result<(), MetricsError> {
        self.inner
            .send(MetricOp::Write(MetricWrite::MessageReceived(
                node_id,
                series,
                received_at,
            )))
            .await
            .map_err(Into::into)
    }

    /// Write a message that counts in the node-wide message total only, not against the peer that delivered it
    pub async fn write_metric_uncounted_message_received(&mut self) -> Result<(), MetricsError> {
        self.inner
            .send(MetricOp::Write(MetricWrite::UncountedMessageReceived))
            .await
            .map_err(Into::into)
    }

    /// Get the message samples of the given `NodeId` in `series`
    pub async fn get_messages_received_timeseries(
        &mut self,
        node_id: NodeId,
        series: FloodSeries,
    ) -> Result<TimeSeries<()>, MetricsError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .send(MetricOp::Read(MetricRead::GetMessagesReceivedTimeseries(
                node_id, series, reply_tx,
            )))
            .await?;
        reply_rx.await.map_err(Into::into)
    }

    /// Get (NodeId, messages per second, series) tuples of the peers that went over a flood-ban limit since the last
    /// call
    pub async fn take_flooding_peers(&mut self) -> Result<Vec<(NodeId, f32, FloodSeries)>, MetricsError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.inner
            .send(MetricOp::Read(MetricRead::TakeFloodingPeers(reply_tx)))
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
        let mut state = MetricsState::new(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
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
        let mut state = MetricsState::new(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        time::advance(Duration::from_secs(1)).await;
        let after_first = Instant::now();
        for _ in 0..PER_PEER_CAPACITY {
            state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        }
        let ts = state.messages_recv.get(&node_id(1)).unwrap();
        assert_eq!(ts.count(), PER_PEER_CAPACITY);
        // The first sample was evicted, all remaining samples are newer
        assert_eq!(ts.count_since(after_first), PER_PEER_CAPACITY);
    }

    #[tokio::test(start_paused = true)]
    async fn it_detects_flooding_at_max_count_plus_one() {
        let mut state = MetricsState::new(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        for _ in 0..FLOOD_BAN_MAX_MSG_COUNT {
            state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        }
        assert!(state.take_flooding_peers().is_empty());

        state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        let flooding = state.take_flooding_peers();
        assert_eq!(flooding.first().map(|(n, ..)| n), Some(&node_id(1)));
        assert_eq!(flooding.len(), 1);

        // Still detected once the cap is reached and old samples are being evicted
        for _ in 0..500 {
            state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        }
        assert_eq!(state.take_flooding_peers().len(), 1);
    }

    /// A burst split across two flood-ban timespans is still one window of more than the max count
    #[tokio::test(start_paused = true)]
    async fn it_detects_a_burst_across_a_window_boundary() {
        let mut state = MetricsState::new(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        time::advance(Duration::from_secs(55)).await;
        for _ in 0..PER_PEER_CAPACITY / 2 {
            state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        }
        time::advance(Duration::from_secs(10)).await;
        for _ in 0..PER_PEER_CAPACITY / 2 {
            state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        }
        assert!(state.take_flooding_peers().is_empty());
        state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        assert_eq!(state.take_flooding_peers().len(), 1);
    }

    /// Messages spread more thinly than the flood-ban rate are not flooding, even once the cap is reached
    #[tokio::test(start_paused = true)]
    async fn it_does_not_flag_a_slow_sender() {
        let mut state = MetricsState::new(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        for _ in 0..(PER_PEER_CAPACITY * 2) {
            state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
            time::advance(Duration::from_millis(100)).await;
        }
        assert!(state.take_flooding_peers().is_empty());
    }

    /// Samples are kept in the order they were taken, not written: a late-written old sample does not make a slow
    /// sender look like a flood, and a late-written sample inside a burst still completes it
    #[tokio::test(start_paused = true)]
    async fn out_of_order_samples_are_judged_by_when_they_were_taken() {
        let start = Instant::now();
        time::advance(Duration::from_secs(100)).await;
        let mut state = MetricsState::new(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        for _ in 1..PER_PEER_CAPACITY {
            state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        }
        // Taken 100s ago: the samples span more than the window
        state.add_message_received(node_id(1), FloodSeries::Own, start);
        assert!(state.take_flooding_peers().is_empty());

        // Taken a second ago, inside the burst: the newest samples now span less than the window
        let one_second_ago = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
        state.add_message_received(node_id(1), FloodSeries::Own, one_second_ago);
        assert_eq!(state.take_flooding_peers().len(), 1);
    }

    /// The forwarded series allows `FORWARDED_FLOOD_BAN_FACTOR` x the limit, and does not touch the own series
    #[tokio::test(start_paused = true)]
    async fn the_forwarded_series_has_its_own_higher_limit() {
        let mut state = MetricsState::new(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        for _ in 0..FLOOD_BAN_MAX_MSG_COUNT * FORWARDED_FLOOD_BAN_FACTOR {
            state.add_message_received(node_id(1), FloodSeries::Forwarded, Instant::now());
        }
        assert!(state.take_flooding_peers().is_empty());
        assert_eq!(
            state
                .message_received_get_timeseries(&node_id(1), FloodSeries::Own)
                .count(),
            0
        );

        state.add_message_received(node_id(1), FloodSeries::Forwarded, Instant::now());
        let flooding = state.take_flooding_peers();
        assert_eq!(
            flooding.first().map(|(n, _, series)| (n, *series)),
            Some((&node_id(1), FloodSeries::Forwarded))
        );
    }

    /// A flood is reported when the flood check finally runs, however late, even after its samples were pruned
    #[tokio::test(start_paused = true)]
    async fn a_late_flood_check_still_sees_the_burst() {
        let mut collector = MetricsCollector::spawn(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        time::sleep(Duration::from_secs(10)).await;
        for _ in 0..PER_PEER_CAPACITY {
            collector
                .write_metric_message_received(node_id(1), FloodSeries::Own, Instant::now())
                .await
                .unwrap();
        }
        // Several prune ticks pass before the check runs
        time::sleep(FLOOD_BAN_TIMESPAN * 3).await;
        let flooding = collector.take_flooding_peers().await.unwrap();
        assert_eq!(flooding.first().map(|(n, ..)| n), Some(&node_id(1)));
        assert!(collector.take_flooding_peers().await.unwrap().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn it_prunes_samples_outside_the_window_at_each_tick() {
        let mut collector = MetricsCollector::spawn(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        let old_peer = node_id(1);
        let recent_peer = node_id(2);

        // t = 10s
        time::sleep(Duration::from_secs(10)).await;
        collector
            .write_metric_message_received(old_peer.clone(), FloodSeries::Own, Instant::now())
            .await
            .unwrap();
        collector
            .write_metric_message_received(recent_peer.clone(), FloodSeries::Own, Instant::now())
            .await
            .unwrap();

        // t = 100s. The 60s tick kept both samples: they were inside the window
        time::sleep(Duration::from_secs(90)).await;
        let ts = collector
            .get_messages_received_timeseries(old_peer.clone(), FloodSeries::Own)
            .await
            .unwrap();
        assert_eq!(ts.count(), 1);
        collector
            .write_metric_message_received(recent_peer.clone(), FloodSeries::Own, Instant::now())
            .await
            .unwrap();

        // t = 125s. The 120s tick dropped the samples from t = 10s, and with them the old peer's entry
        time::sleep(Duration::from_secs(25)).await;
        let ts = collector
            .get_messages_received_timeseries(old_peer, FloodSeries::Own)
            .await
            .unwrap();
        assert_eq!(ts.count(), 0);
        let ts = collector
            .get_messages_received_timeseries(recent_peer, FloodSeries::Own)
            .await
            .unwrap();
        assert_eq!(ts.count(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn pruning_releases_the_memory_of_old_samples() {
        let mut state = MetricsState::new(PER_PEER_CAPACITY, FLOOD_BAN_TIMESPAN);
        for _ in 0..FLOOD_BAN_MAX_MSG_COUNT {
            state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());
        }
        time::advance(FLOOD_BAN_TIMESPAN + Duration::from_secs(1)).await;
        state.add_message_received(node_id(1), FloodSeries::Own, Instant::now());

        state.prune_stale(FLOOD_BAN_TIMESPAN);
        let ts = state.messages_recv.get(&node_id(1)).unwrap();
        assert_eq!(ts.count(), 1);
        assert!(ts.values.capacity() < 10, "capacity {}", ts.values.capacity());
    }
}

// Copyright 2019 The Tari Project
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

use crate::{proto::liveness::MetadataKey, services::liveness::error::LivenessError};
use chrono::{NaiveDateTime, Utc};
use std::{
    collections::{hash_map::RandomState, HashMap},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use tari_comms::peer_manager::NodeId;

const LATENCY_SAMPLE_WINDOW_SIZE: usize = 25;
const MAX_INFLIGHT_TTL: Duration = Duration::from_secs(20);

/// Represents metadata in a ping/pong message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    inner: HashMap<i32, Vec<u8>>,
}

impl Metadata {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn insert(&mut self, key: MetadataKey, value: Vec<u8>) {
        self.inner.insert(key as i32, value);
    }

    pub fn get(&self, key: MetadataKey) -> Option<&Vec<u8>> {
        self.inner.get(&(key as i32))
    }
}

impl From<HashMap<i32, Vec<u8>>> for Metadata {
    fn from(inner: HashMap<i32, Vec<u8>>) -> Self {
        Self { inner }
    }
}

impl From<Metadata> for HashMap<i32, Vec<u8>, RandomState> {
    fn from(metadata: Metadata) -> Self {
        metadata.inner
    }
}

/// State for the LivenessService.
#[derive(Default)]
pub struct LivenessState {
    inflight_pings: HashMap<u64, (NodeId, NaiveDateTime)>,
    peer_latency: HashMap<NodeId, AverageLatency>,

    pings_received: AtomicUsize,
    pongs_received: AtomicUsize,
    pings_sent: AtomicUsize,
    pongs_sent: AtomicUsize,
    num_active_peers: AtomicUsize,

    pong_metadata: Metadata,
    nodes_to_monitor: HashMap<NodeId, NodeStats>,
}

impl LivenessState {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn inc_pings_sent(&self) -> usize {
        self.pings_sent.fetch_add(1, Ordering::Relaxed)
    }

    pub fn inc_pongs_sent(&self) -> usize {
        self.pongs_sent.fetch_add(1, Ordering::Relaxed)
    }

    pub fn inc_pings_received(&self) -> usize {
        self.pings_received.fetch_add(1, Ordering::Relaxed)
    }

    pub fn inc_pongs_received(&self) -> usize {
        self.pongs_received.fetch_add(1, Ordering::Relaxed)
    }

    pub fn pings_received(&self) -> usize {
        self.pings_received.load(Ordering::Relaxed)
    }

    pub fn pongs_received(&self) -> usize {
        self.pongs_received.load(Ordering::Relaxed)
    }

    pub fn set_num_active_peers(&self, n: usize) {
        self.num_active_peers.store(n, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn pings_sent(&self) -> usize {
        self.pings_sent.load(Ordering::Relaxed)
    }

    #[cfg(test)]
    pub fn pongs_sent(&self) -> usize {
        self.pongs_sent.load(Ordering::Relaxed)
    }

    /// Returns a reference to pong metadata
    pub fn pong_metadata(&self) -> &Metadata {
        &self.pong_metadata
    }

    /// Set a pong metadata entry. Duplicate entries are replaced.
    pub fn set_pong_metadata_entry(&mut self, key: MetadataKey, value: Vec<u8>) {
        self.pong_metadata.insert(key, value);
    }

    /// Adds a ping to the inflight ping list, while noting the current time that a ping was sent.
    pub fn add_inflight_ping(&mut self, nonce: u64, node_id: &NodeId) {
        let now = Utc::now().naive_utc();
        self.inflight_pings.insert(nonce, ((*node_id).clone(), now));
        if let Some(ns) = self.nodes_to_monitor.get_mut(node_id) {
            ns.last_ping_sent = Some(now);
        }
        self.clear_stale_inflight_pings();
    }

    /// Clears inflight ping requests which have not responded
    fn clear_stale_inflight_pings(&mut self) {
        self.inflight_pings = self
            .inflight_pings
            .drain()
            .filter(|(_, (_, time))| convert_to_std_duration(Utc::now().naive_utc() - *time) <= MAX_INFLIGHT_TTL)
            .collect();
    }

    /// Returns true if the nonce is inflight, otherwise false
    pub fn is_inflight(&self, nonce: u64) -> bool {
        self.inflight_pings.get(&nonce).is_some()
    }

    /// Records a pong. Specifically, the pong counter is incremented and
    /// a latency sample is added and calculated.
    pub fn record_pong(&mut self, nonce: u64) -> Option<u32> {
        self.inc_pongs_received();

        match self.inflight_pings.remove_entry(&nonce) {
            Some((_, (node_id, sent_time))) => {
                let now = Utc::now().naive_utc();
                if let Some(ns) = self.nodes_to_monitor.get_mut(&node_id) {
                    ns.last_pong_received = Some(sent_time);
                    ns.average_latency.add_sample(convert_to_std_duration(now - sent_time));
                }
                let latency = self
                    .add_latency_sample(node_id, convert_to_std_duration(now - sent_time))
                    .calc_average();
                Some(latency)
            },
            None => None,
        }
    }

    fn add_latency_sample(&mut self, node_id: NodeId, duration: Duration) -> &mut AverageLatency {
        let latency = self
            .peer_latency
            .entry(node_id)
            .or_insert_with(|| AverageLatency::new(LATENCY_SAMPLE_WINDOW_SIZE));

        latency.add_sample(duration);
        latency
    }

    pub fn get_avg_latency_ms(&self, node_id: &NodeId) -> Option<u32> {
        self.peer_latency.get(node_id).map(|latency| latency.calc_average())
    }

    pub fn add_node_id(&mut self, node_id: &NodeId) {
        if self.nodes_to_monitor.contains_key(node_id) {
            return;
        }
        let _ = self.nodes_to_monitor.insert(node_id.clone(), NodeStats::new());
    }

    pub fn get_num_monitored_nodes(&self) -> usize {
        self.nodes_to_monitor.len()
    }

    pub fn get_monitored_node_ids(&self) -> Vec<NodeId> {
        self.nodes_to_monitor.keys().cloned().collect()
    }

    pub fn is_monitored_node_id(&self, node_id: &NodeId) -> bool {
        self.nodes_to_monitor.contains_key(node_id)
    }

    pub fn get_node_id_stats(&self, node_id: &NodeId) -> Result<NodeStats, LivenessError> {
        match self.nodes_to_monitor.get(node_id) {
            None => Err(LivenessError::NodeIdDoesNotExist),
            Some(s) => Ok((*s).clone()),
        }
    }
}

/// Convert `chrono::Duration` to `std::time::Duration`
pub(super) fn convert_to_std_duration(old_duration: chrono::Duration) -> Duration {
    Duration::from_millis(old_duration.num_milliseconds() as u64)
}

/// A very simple implementation for calculating average latency. Samples are added in milliseconds and the mean average
/// is calculated for those samples. If more than [LATENCY_SAMPLE_WINDOW_SIZE](self::LATENCY_SAMPLE_WINDOW_SIZE) samples
/// are added the oldest sample is discarded.
#[derive(Clone, Debug, Default)]
pub struct AverageLatency {
    samples: Vec<u32>,
}

impl AverageLatency {
    /// Create a new AverageLatency
    pub fn new(num_samples: usize) -> Self {
        Self {
            samples: Vec::with_capacity(num_samples),
        }
    }

    /// Add a sample `Duration`. The number of milliseconds is capped at `u32::MAX`.
    pub fn add_sample(&mut self, sample: Duration) {
        if self.samples.len() == self.samples.capacity() {
            self.samples.remove(0);
        }
        self.samples.push(sample.as_millis() as u32)
    }

    /// Calculate the average of the recorded samples
    pub fn calc_average(&self) -> u32 {
        let samples = &self.samples;
        if samples.is_empty() {
            return 0;
        }

        samples.iter().fold(0, |sum, x| sum + *x) / samples.len() as u32
    }
}

/// This struct contains the stats about a Node that is being monitored by the Liveness Service
#[derive(Clone, Debug, Default)]
pub struct NodeStats {
    last_ping_sent: Option<NaiveDateTime>,
    last_pong_received: Option<NaiveDateTime>,
    average_latency: AverageLatency,
}

impl NodeStats {
    pub fn new() -> NodeStats {
        Self {
            last_ping_sent: None,
            last_pong_received: None,
            average_latency: AverageLatency::new(LATENCY_SAMPLE_WINDOW_SIZE),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new() {
        let state = LivenessState::new();
        assert_eq!(state.pings_received.load(Ordering::SeqCst), 0);
        assert_eq!(state.pongs_received.load(Ordering::SeqCst), 0);
        assert_eq!(state.pings_sent.load(Ordering::SeqCst), 0);
        assert_eq!(state.pongs_sent.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn getters() {
        let state = LivenessState::new();
        state.pings_received.store(5, Ordering::SeqCst);
        assert_eq!(state.pings_received(), 5);
        assert_eq!(state.pongs_received(), 0);
        assert_eq!(state.pings_sent(), 0);
        assert_eq!(state.pongs_sent(), 0);
    }

    #[test]
    fn inc_pings_sent() {
        let state = LivenessState::new();
        assert_eq!(state.pings_sent.load(Ordering::SeqCst), 0);
        assert_eq!(state.inc_pings_sent(), 0);
        assert_eq!(state.pings_sent.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn inc_pongs_sent() {
        let state = LivenessState::new();
        assert_eq!(state.pongs_sent.load(Ordering::SeqCst), 0);
        assert_eq!(state.inc_pongs_sent(), 0);
        assert_eq!(state.pongs_sent.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn inc_pings_received() {
        let state = LivenessState::new();
        assert_eq!(state.pings_received.load(Ordering::SeqCst), 0);
        assert_eq!(state.inc_pings_received(), 0);
        assert_eq!(state.pings_received.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn inc_pongs_received() {
        let state = LivenessState::new();
        assert_eq!(state.pongs_received.load(Ordering::SeqCst), 0);
        assert_eq!(state.inc_pongs_received(), 0);
        assert_eq!(state.pongs_received.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn record_pong() {
        let mut state = LivenessState::new();

        let node_id = NodeId::default();
        state.add_inflight_ping(123, &node_id);

        let latency = state.record_pong(123).unwrap();
        assert!(latency < 50);
    }

    #[test]
    fn set_pong_metadata_entry() {
        let mut state = LivenessState::new();
        state.set_pong_metadata_entry(MetadataKey::ChainMetadata, b"dummy-data".to_vec());
        assert_eq!(
            state.pong_metadata().get(MetadataKey::ChainMetadata).unwrap(),
            b"dummy-data"
        );
    }

    #[test]
    fn monitor_node_id() {
        let node_id = NodeId::default();
        let mut state = LivenessState::new();
        state.add_node_id(&node_id);

        state.add_inflight_ping(123, &node_id);

        let latency = state.record_pong(123).unwrap();
        assert!(latency < 50);

        assert_eq!(state.get_num_monitored_nodes(), 1);
        assert_eq!(state.get_monitored_node_ids().len(), 1);
        assert!(state.is_monitored_node_id(&node_id));
        let stats = state.get_node_id_stats(&node_id).unwrap();

        assert_eq!(stats.average_latency.calc_average(), latency);
    }
}

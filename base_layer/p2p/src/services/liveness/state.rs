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

use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use log::*;
use tari_comms::peer_manager::NodeId;

use super::LOG_TARGET;
use crate::proto::liveness::MetadataKey;

const LATENCY_SAMPLE_WINDOW_SIZE: usize = 25;
const MAX_INFLIGHT_TTL: Duration = Duration::from_secs(40);

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

    pub fn has(&self, key: MetadataKey) -> bool {
        self.inner.contains_key(&(key as i32))
    }
}

impl From<HashMap<i32, Vec<u8>>> for Metadata {
    fn from(inner: HashMap<i32, Vec<u8>>) -> Self {
        Self { inner }
    }
}

impl From<Metadata> for HashMap<i32, Vec<u8>> {
    fn from(metadata: Metadata) -> Self {
        metadata.inner
    }
}

/// State for the LivenessService.
#[derive(Default, Debug)]
pub struct LivenessState {
    inflight_pings: HashMap<u64, (NodeId, Instant)>,
    peer_latency: HashMap<NodeId, AverageLatency>,
    failed_pings: HashMap<NodeId, usize>,

    pings_received: usize,
    pongs_received: usize,
    pings_sent: usize,
    pongs_sent: usize,

    local_metadata: Metadata,
}

impl LivenessState {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn inc_pings_sent(&mut self) {
        self.pings_sent += 1;
    }

    pub fn inc_pongs_sent(&mut self) {
        self.pongs_sent += 1;
    }

    pub fn inc_pings_received(&mut self) {
        self.pings_received += 1;
    }

    pub fn inc_pongs_received(&mut self) {
        self.pongs_received += 1;
    }

    pub fn pings_received(&self) -> usize {
        self.pings_received
    }

    pub fn pongs_received(&self) -> usize {
        self.pongs_received
    }

    #[cfg(test)]
    pub fn pings_sent(&self) -> usize {
        self.pings_sent
    }

    #[cfg(test)]
    pub fn pongs_sent(&self) -> usize {
        self.pongs_sent
    }

    /// Returns a reference to local metadata
    pub fn metadata(&self) -> &Metadata {
        &self.local_metadata
    }

    /// Set a metadata entry for the local node. Duplicate entries are replaced.
    pub fn set_metadata_entry(&mut self, key: MetadataKey, value: Vec<u8>) {
        self.local_metadata.insert(key, value);
    }

    /// Adds a ping to the inflight ping list, while noting the current time that a ping was sent.
    pub fn add_inflight_ping(&mut self, nonce: u64, node_id: NodeId) {
        self.inflight_pings.insert(nonce, (node_id, Instant::now()));
        self.clear_stale_inflight_pings();
    }

    /// Clears inflight ping requests which have not responded and adds them to failed_ping counter
    fn clear_stale_inflight_pings(&mut self) {
        let (inflight, expired) = self
            .inflight_pings
            .drain()
            .partition(|(_, (_, time))| time.elapsed() <= MAX_INFLIGHT_TTL);

        self.inflight_pings = inflight;

        for (_, (node_id, _)) in expired {
            self.failed_pings
                .entry(node_id)
                .and_modify(|v| {
                    *v += 1;
                })
                .or_insert(1);
        }
    }

    /// Returns true if the nonce is inflight, otherwise false
    pub fn is_inflight(&self, nonce: u64) -> bool {
        self.inflight_pings.get(&nonce).is_some()
    }

    /// Records a pong. Specifically, the pong counter is incremented and
    /// a latency sample is added and calculated. The given `peer` must match the recorded peer
    pub fn record_pong(&mut self, nonce: u64, sent_by: &NodeId) -> Option<Duration> {
        self.inc_pongs_received();
        self.failed_pings.remove_entry(sent_by);

        let (node_id, _) = self.inflight_pings.get(&nonce)?;
        if node_id == sent_by {
            self.inflight_pings
                .remove(&nonce)
                .map(|(node_id, sent_time)| self.add_latency_sample(node_id, sent_time.elapsed()).calc_average())
        } else {
            warn!(
                target: LOG_TARGET,
                "Peer {} sent an nonce for another peer {}. This could indicate malicious behaviour or a bug. \
                 Ignoring.",
                sent_by,
                node_id
            );
            None
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

    pub fn get_avg_latency(&self, node_id: &NodeId) -> Option<Duration> {
        self.peer_latency.get(node_id).map(|latency| latency.calc_average())
    }

    pub fn get_network_avg_latency(&self) -> Option<Duration> {
        let num_peers = self.peer_latency.len();
        self.peer_latency
            .values()
            .map(|latency| latency.calc_average())
            .fold(Option::<Duration>::None, |acc, latency| {
                let current = acc.unwrap_or_default();
                Some(current + latency)
            })
            // num_peers in map will always be > 0
            .map(|latency| Duration::from_millis(latency.as_millis() as u64 / num_peers as u64))
    }

    pub fn failed_pings_iter(&self) -> impl Iterator<Item = (&NodeId, &usize)> {
        self.failed_pings.iter()
    }

    pub fn clear_failed_pings(&mut self) {
        self.failed_pings.clear();
    }
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
    pub fn calc_average(&self) -> Duration {
        self.samples
            .iter()
            .map(|x| u64::from(*x))
            .fold(0, u64::saturating_add)
            .checked_div(self.samples.len() as u64)
            .map(Duration::from_millis)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn new() {
        let state = LivenessState::new();
        assert_eq!(state.pings_received(), 0);
        assert_eq!(state.pongs_received(), 0);
        assert_eq!(state.pings_sent(), 0);
        assert_eq!(state.pongs_sent(), 0);
    }

    #[test]
    fn getters() {
        let mut state = LivenessState::new();
        state.pings_received = 5;
        assert_eq!(state.pings_received(), 5);
        assert_eq!(state.pongs_received(), 0);
        assert_eq!(state.pings_sent(), 0);
        assert_eq!(state.pongs_sent(), 0);
    }

    #[test]
    fn inc_pings_sent() {
        let mut state = LivenessState::new();
        assert_eq!(state.pings_sent(), 0);
        state.inc_pings_sent();
        assert_eq!(state.pings_sent(), 1);
    }

    #[test]
    fn inc_pongs_sent() {
        let mut state = LivenessState::new();
        assert_eq!(state.pongs_sent(), 0);
        state.inc_pongs_sent();
        assert_eq!(state.pongs_sent(), 1);
    }

    #[test]
    fn inc_pings_received() {
        let mut state = LivenessState::new();
        assert_eq!(state.pings_received(), 0);
        state.inc_pings_received();
        assert_eq!(state.pings_received(), 1);
    }

    #[test]
    fn inc_pongs_received() {
        let mut state = LivenessState::new();
        assert_eq!(state.pongs_received(), 0);
        state.inc_pongs_received();
        assert_eq!(state.pongs_received(), 1);
    }

    #[test]
    fn record_pong() {
        let mut state = LivenessState::new();

        let node_id = NodeId::default();
        state.add_inflight_ping(123, node_id.clone());

        let latency = state.record_pong(123, &node_id).unwrap();
        assert!(latency < Duration::from_millis(50));
    }

    #[test]
    fn set_metadata_entry() {
        let mut state = LivenessState::new();
        state.set_metadata_entry(MetadataKey::ChainMetadata, b"dummy-data".to_vec());
        assert_eq!(state.metadata().get(MetadataKey::ChainMetadata).unwrap(), b"dummy-data");
    }

    #[test]
    fn clear_stale_inflight_pings() {
        let mut state = LivenessState::new();

        let peer1 = NodeId::default();
        state.add_inflight_ping(1, peer1.clone());
        let peer2 = NodeId::from_public_key(&Default::default());
        state.add_inflight_ping(2, peer2.clone());
        state.add_inflight_ping(3, peer2.clone());

        assert!(state.failed_pings.get(&peer1).is_none());
        assert!(state.failed_pings.get(&peer2).is_none());

        // MAX_INFLIGHT_TTL passes
        for n in [1, 2, 3] {
            let (_, time) = state.inflight_pings.get_mut(&n).unwrap();
            *time = Instant::now() - (MAX_INFLIGHT_TTL + Duration::from_secs(1));
        }

        state.clear_stale_inflight_pings();
        let n = state.failed_pings.get(&peer1).unwrap();
        assert_eq!(*n, 1);
        let n = state.failed_pings.get(&peer2).unwrap();
        assert_eq!(*n, 2);

        assert!(state.record_pong(2, &peer2).is_none());
        let n = state.failed_pings.get(&peer1).unwrap();
        assert_eq!(*n, 1);
        assert!(state.failed_pings.get(&peer2).is_none());
    }
}

// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    cmp,
    cmp::{Ord, Ordering},
    convert::{TryFrom, TryInto},
    fmt,
    fmt::{Display, Formatter},
    hash::{Hash, Hasher},
    time::Duration,
};

use chrono::{NaiveDateTime, Utc};
use log::trace;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

use crate::{peer_manager::PeerIdentityClaim, types::CommsPublicKey};

const LOG_TARGET: &str = "comms::net_address::multiaddr_with_stats";

const MAX_LATENCY_SAMPLE_COUNT: u32 = 100;
const MAX_INITIAL_DIAL_TIME_SAMPLE_COUNT: u32 = 100;

#[derive(Debug, Eq, Clone, Deserialize, Serialize)]
pub struct MultiaddrWithStats {
    address: Multiaddr,
    last_seen: Option<NaiveDateTime>,
    connection_attempts: u32,
    avg_initial_dial_time: Option<Duration>,
    initial_dial_time_sample_count: u32,
    avg_latency: Option<Duration>,
    latency_sample_count: u32,
    last_attempted: Option<NaiveDateTime>,
    last_failed_reason: Option<String>,
    quality_score: Option<i32>,
    source: PeerAddressSource,
}

impl MultiaddrWithStats {
    /// Constructs a new net address with zero stats
    pub fn new(address: Multiaddr, source: PeerAddressSource) -> Self {
        let mut addr = Self {
            address,
            last_seen: None,
            connection_attempts: 0,
            avg_initial_dial_time: None,
            initial_dial_time_sample_count: 0,
            avg_latency: None,
            latency_sample_count: 0,
            last_attempted: None,
            last_failed_reason: None,
            quality_score: None,
            source,
        };
        addr.update_quality_score();
        addr
    }

    pub fn merge(&mut self, other: &Self) {
        if self.address == other.address {
            trace!(
                target: LOG_TARGET, "merge: '{}, {:?}, {:?}' and '{}, {:?}, {:?}'",
                self.address.to_string(),
                self.last_seen,
                self.quality_score,
                other.address.to_string(),
                other.last_seen,
                other.quality_score
            );
            self.last_seen = cmp::max(other.last_seen, self.last_seen);
            self.connection_attempts = cmp::max(self.connection_attempts, other.connection_attempts);
            match self.latency_sample_count.cmp(&other.latency_sample_count) {
                Ordering::Less => {
                    self.avg_latency = other.avg_latency;
                    self.latency_sample_count = other.latency_sample_count;
                },
                Ordering::Equal | Ordering::Greater => {},
            }
            match self
                .initial_dial_time_sample_count
                .cmp(&other.initial_dial_time_sample_count)
            {
                Ordering::Less => {
                    self.avg_initial_dial_time = other.avg_initial_dial_time;
                    self.initial_dial_time_sample_count = other.initial_dial_time_sample_count;
                },
                Ordering::Equal | Ordering::Greater => {},
            }
            self.last_attempted = cmp::max(self.last_attempted, other.last_attempted);
            self.last_failed_reason = other.last_failed_reason.clone();
            self.update_source_if_better(&other.source);
        }
    }

    pub fn update_source_if_better(&mut self, source: &PeerAddressSource) {
        match (self.source.peer_identity_claim(), source.peer_identity_claim()) {
            (None, None) => (),
            (None, Some(_)) => {
                self.source = source.clone();
            },
            (Some(_), None) => (),
            (Some(self_source), Some(other_source)) => {
                if other_source.signature.updated_at() > self_source.signature.updated_at() {
                    self.source = source.clone();
                }
            },
        }
        self.update_quality_score();
    }

    pub fn address(&self) -> &Multiaddr {
        &self.address
    }

    pub fn offline_at(&self) -> Option<NaiveDateTime> {
        if self.last_failed_reason.is_some() {
            self.last_attempted
        } else {
            None
        }
    }

    /// Updates the average latency by including another measured latency sample. The historical average is updated by
    /// allowing the new measurement to provide a weighted contribution to the historical average. As more samples are
    /// received the historical average will have a larger weight compare to the new measurement, this will have a
    /// filtering effect similar to a sliding window without needing previous measurements to be stored. When a new
    /// latency measurement is received and the latency_sample_count is equal or have surpassed the
    /// MAX_LATENCY_SAMPLE_COUNT then the current avg_latency is scaled so that the new latency_measurement only makes a
    /// small weighted change to the avg_latency. The previous avg_latency will have a weight of
    /// MAX_LATENCY_SAMPLE_COUNT and the new latency_measurement will have a weight of 1.
    pub fn update_latency(&mut self, latency_measurement: Duration) {
        self.last_seen = Some(Utc::now().naive_utc());

        self.avg_latency = Some(
            ((self
                .avg_latency
                .unwrap_or_default()
                .saturating_mul(self.latency_sample_count))
            .saturating_add(latency_measurement)) /
                (self.latency_sample_count + 1),
        );
        if self.latency_sample_count < MAX_LATENCY_SAMPLE_COUNT {
            self.latency_sample_count += 1;
        }

        self.update_quality_score();
    }

    #[cfg(test)]
    fn get_averag_latency(&self) -> Option<Duration> {
        self.avg_latency
    }

    pub fn update_initial_dial_time(&mut self, initial_dial_time: Duration) {
        self.last_seen = Some(Utc::now().naive_utc());

        self.avg_initial_dial_time = Some(
            ((self.avg_initial_dial_time.unwrap_or_default() * self.initial_dial_time_sample_count) +
                initial_dial_time) /
                (self.initial_dial_time_sample_count + 1),
        );
        if self.initial_dial_time_sample_count < MAX_INITIAL_DIAL_TIME_SAMPLE_COUNT {
            self.initial_dial_time_sample_count += 1;
        }
        self.update_quality_score();
    }

    /// Mark that a successful interaction occurred with this address
    pub fn mark_last_seen_now(&mut self) -> &mut Self {
        trace!(
            target: LOG_TARGET, "mark_last_seen_now: from {}, address '{}', previous {:?}",
            self.source, self.address.to_string(), self.last_seen
        );
        self.last_seen = Some(Utc::now().naive_utc());
        self.last_failed_reason = None;
        self.reset_connection_attempts();
        self.update_quality_score();
        self
    }

    /// Reset the connection attempts on this net address for a later session of retries
    pub fn reset_connection_attempts(&mut self) {
        self.connection_attempts = 0;
    }

    /// Mark that a connection could not be established with this net address
    pub fn mark_failed_connection_attempt(&mut self, error_string: String) -> &mut Self {
        self.connection_attempts += 1;
        self.last_failed_reason = Some(error_string);
        self.update_quality_score();
        self
    }

    #[cfg(test)]
    pub fn mark_last_attempted(&mut self, timestamp: NaiveDateTime) -> &mut Self {
        self.last_attempted = Some(timestamp);
        self.update_quality_score();
        self
    }

    pub fn mark_last_attempted_now(&mut self) -> &mut Self {
        self.last_attempted = Some(Utc::now().naive_utc());
        self.update_quality_score();
        self
    }

    /// Get as a Multiaddr
    pub fn as_net_address(&self) -> Multiaddr {
        self.clone().address
    }

    // The quality score is a measure of the reliability of the net address. It is calculated based on the following:
    // - The maximum score is 'Some(1000)' points (seen within the last 1s and latency < 100ms).
    // - The minimum score without any connection errors is 'Some(100)' points (seen >= 800s ago and latency >= 10s).
    // - For any sort of connection error the score is 'Some(0)' points.
    // - A score of `None` means it has not been tried.
    fn calculate_quality_score(&self) -> Option<i32> {
        if self.last_seen.is_none() && self.last_attempted.is_none() {
            return None;
        }

        // The starting score
        let mut score_self = 800;

        // Latency score:
        // - If there is no average yet, add '100' points
        // - If the average latency is
        //   - less than 100ms, add '100' points
        //   - 100ms to 10,000ms', add '99' to '1' point on a sliding scale
        //   - 10s or more, add '0' points
        if let Some(val) = self.avg_latency {
            // Explicitly truncate the latency to avoid casting problems
            let avg_latency_millis = i32::try_from(val.as_millis()).unwrap_or(i32::MAX);
            score_self += cmp::max(0, 100i32.saturating_sub(avg_latency_millis / 100));
        } else {
            score_self += 100;
        }

        // Last seen score:
        // - If the last seen time is:
        //   - 800s or more, subtract '700' points
        //   - 799s to 101s, subtract '699' to '1' point on a sliding scale
        //   - 100s, add or subtract nothing
        //   - 99s to 1s, add '1' to '99' points on a sliding scale
        //   - less than 1s, add '100' points
        let last_seen_seconds: i32 = self
            .last_seen
            .map(|x| Utc::now().naive_utc() - x)
            .map(|x| x.num_seconds())
            .unwrap_or(i64::MAX / 2)
            .try_into()
            .unwrap_or(i32::MAX);
        score_self += cmp::max(-700, 100i32.saturating_sub(last_seen_seconds));

        // Any failure to connect results in a score of '0' points
        if self.last_failed_reason.is_some() {
            score_self = 0;
        }

        Some(score_self)
    }

    fn update_quality_score(&mut self) {
        self.quality_score = self.calculate_quality_score();
    }

    pub fn source(&self) -> &PeerAddressSource {
        &self.source
    }

    pub fn last_seen(&self) -> Option<NaiveDateTime> {
        self.last_seen
    }

    pub fn connection_attempts(&self) -> u32 {
        self.connection_attempts
    }

    pub fn avg_initial_dial_time(&self) -> Option<Duration> {
        self.avg_initial_dial_time
    }

    pub fn initial_dial_time_sample_count(&self) -> u32 {
        self.initial_dial_time_sample_count
    }

    pub fn avg_latency(&self) -> Option<Duration> {
        self.avg_latency
    }

    pub fn latency_sample_count(&self) -> u32 {
        self.latency_sample_count
    }

    pub fn last_attempted(&self) -> Option<NaiveDateTime> {
        self.last_attempted
    }

    pub fn last_failed_reason(&self) -> Option<&str> {
        self.last_failed_reason.as_deref()
    }

    pub fn quality_score(&self) -> Option<i32> {
        self.quality_score
    }
}

// Reliability ordering of net addresses: prioritize net addresses according to previous successful connections,
// connection attempts, latency and last seen A lower ordering has a higher priority and a higher ordering has a lower
// priority, this ordering switch allows searching for, and updating of net addresses to be performed more efficiently
impl Ord for MultiaddrWithStats {
    fn cmp(&self, other: &MultiaddrWithStats) -> Ordering {
        self.quality_score.cmp(&other.quality_score)
    }
}

impl PartialOrd for MultiaddrWithStats {
    fn partial_cmp(&self, other: &MultiaddrWithStats) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for MultiaddrWithStats {
    fn eq(&self, other: &MultiaddrWithStats) -> bool {
        self.address == other.address
    }
}

impl Hash for MultiaddrWithStats {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address.hash(state)
    }
}

impl Display for MultiaddrWithStats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.address)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq)]
pub enum PeerAddressSource {
    Config,
    FromNodeIdentity {
        peer_identity_claim: PeerIdentityClaim,
    },
    FromPeerConnection {
        peer_identity_claim: PeerIdentityClaim,
    },
    FromDiscovery {
        peer_identity_claim: PeerIdentityClaim,
    },
    FromAnotherPeer {
        peer_identity_claim: PeerIdentityClaim,
        source_peer: CommsPublicKey,
    },
    FromJoinMessage {
        peer_identity_claim: PeerIdentityClaim,
    },
}

impl PeerAddressSource {
    pub fn is_config(&self) -> bool {
        matches!(self, PeerAddressSource::Config)
    }

    pub fn peer_identity_claim(&self) -> Option<&PeerIdentityClaim> {
        match self {
            PeerAddressSource::Config => None,
            PeerAddressSource::FromNodeIdentity { peer_identity_claim } => Some(peer_identity_claim),
            PeerAddressSource::FromPeerConnection { peer_identity_claim } => Some(peer_identity_claim),
            PeerAddressSource::FromDiscovery { peer_identity_claim } => Some(peer_identity_claim),
            PeerAddressSource::FromAnotherPeer {
                peer_identity_claim, ..
            } => Some(peer_identity_claim),
            PeerAddressSource::FromJoinMessage { peer_identity_claim } => Some(peer_identity_claim),
        }
    }
}

impl Display for PeerAddressSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            PeerAddressSource::Config => write!(f, "Config"),
            PeerAddressSource::FromNodeIdentity { .. } => {
                write!(f, "FromNodeIdentity")
            },
            PeerAddressSource::FromPeerConnection { .. } => write!(f, "FromPeerConnection"),
            PeerAddressSource::FromDiscovery { .. } => write!(f, "FromDiscovery"),
            PeerAddressSource::FromAnotherPeer { .. } => write!(f, "FromAnotherPeer"),
            PeerAddressSource::FromJoinMessage { .. } => write!(f, "FromJoinMessage"),
        }
    }
}

impl PartialEq for PeerAddressSource {
    fn eq(&self, other: &Self) -> bool {
        match self {
            PeerAddressSource::Config => {
                matches!(other, PeerAddressSource::Config)
            },
            PeerAddressSource::FromNodeIdentity { .. } => {
                matches!(other, PeerAddressSource::FromNodeIdentity { .. })
            },
            PeerAddressSource::FromPeerConnection { .. } => {
                matches!(other, PeerAddressSource::FromPeerConnection { .. })
            },
            PeerAddressSource::FromAnotherPeer { .. } => {
                matches!(other, PeerAddressSource::FromAnotherPeer { .. })
            },
            PeerAddressSource::FromDiscovery { .. } => {
                matches!(other, PeerAddressSource::FromDiscovery { .. })
            },
            PeerAddressSource::FromJoinMessage { .. } => {
                matches!(other, PeerAddressSource::FromJoinMessage { .. })
            },
        }
    }
}
#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_update_latency() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MultiaddrWithStats::new(net_address, PeerAddressSource::Config);
        let latency_measurement1 = Duration::from_millis(100);
        let latency_measurement2 = Duration::from_millis(200);
        let latency_measurement3 = Duration::from_millis(60);
        let latency_measurement4 = Duration::from_millis(140);
        net_address_with_stats.update_latency(latency_measurement1);
        assert_eq!(net_address_with_stats.avg_latency.unwrap(), latency_measurement1);
        net_address_with_stats.update_latency(latency_measurement2);
        assert_eq!(net_address_with_stats.avg_latency.unwrap(), Duration::from_millis(150));
        net_address_with_stats.update_latency(latency_measurement3);
        assert_eq!(net_address_with_stats.avg_latency.unwrap(), Duration::from_millis(120));
        net_address_with_stats.update_latency(latency_measurement4);
        assert_eq!(net_address_with_stats.avg_latency.unwrap(), Duration::from_millis(125));
    }

    #[test]
    fn test_successful_and_failed_connection_attempts() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MultiaddrWithStats::new(net_address, PeerAddressSource::Config);
        net_address_with_stats.mark_failed_connection_attempt("Error".to_string());
        net_address_with_stats.mark_failed_connection_attempt("Error".to_string());
        assert!(net_address_with_stats.last_seen.is_none());
        assert_eq!(net_address_with_stats.connection_attempts, 2);
        net_address_with_stats.mark_last_seen_now();
        assert!(net_address_with_stats.last_seen.is_some());
        assert_eq!(net_address_with_stats.connection_attempts, 0);
    }

    #[test]
    fn test_reseting_connection_attempts() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MultiaddrWithStats::new(net_address, PeerAddressSource::Config);
        net_address_with_stats.mark_failed_connection_attempt("asdf".to_string());
        net_address_with_stats.mark_failed_connection_attempt("asdf".to_string());
        assert_eq!(net_address_with_stats.connection_attempts, 2);
        net_address_with_stats.reset_connection_attempts();
        assert_eq!(net_address_with_stats.connection_attempts, 0);
    }

    #[test]
    fn test_calculate_quality_score() {
        let address_raw: Multiaddr = "/ip4/123.0.0.123/tcp/8000".parse().unwrap();
        let mut address = MultiaddrWithStats::new(address_raw.clone(), PeerAddressSource::Config);
        assert_eq!(address.quality_score, None);

        address.mark_last_seen_now();
        assert!(address.quality_score.unwrap() >= 990); // 1000 with a margin of 10s (10) delayed last seen

        let mut address = MultiaddrWithStats::new(address_raw.clone(), PeerAddressSource::Config);
        address.update_latency(Duration::from_millis(1000));
        assert_eq!(address.get_averag_latency().unwrap(), Duration::from_millis(1000));
        assert!(address.quality_score.unwrap() >= 980); // 990 with a margin of 10s (10) delayed last seen

        let mut address = MultiaddrWithStats::new(address_raw.clone(), PeerAddressSource::Config);
        address.update_latency(Duration::from_millis(1500));
        address.update_latency(Duration::from_millis(2500));
        address.update_latency(Duration::from_millis(3500));
        assert_eq!(address.get_averag_latency().unwrap(), Duration::from_millis(2500));
        assert!(address.quality_score.unwrap() >= 965); // 975 with a margin of 10s (10) delayed last seen

        let mut address = MultiaddrWithStats::new(address_raw.clone(), PeerAddressSource::Config);
        address.update_latency(Duration::from_millis(3500));
        address.update_latency(Duration::from_millis(4500));
        address.update_latency(Duration::from_millis(5500));
        assert_eq!(address.get_averag_latency().unwrap(), Duration::from_millis(4500));
        assert!(address.quality_score.unwrap() >= 945); // 955 with a margin of 10s (10) delayed last seen

        let mut address = MultiaddrWithStats::new(address_raw.clone(), PeerAddressSource::Config);
        address.update_latency(Duration::from_millis(5500));
        address.update_latency(Duration::from_millis(6500));
        address.update_latency(Duration::from_millis(7500));
        assert_eq!(address.get_averag_latency().unwrap(), Duration::from_millis(6500));
        assert!(address.quality_score.unwrap() >= 925); // 935 with a margin of 10s (10) delayed last seen

        let mut address = MultiaddrWithStats::new(address_raw.clone(), PeerAddressSource::Config);
        address.update_latency(Duration::from_millis(9000));
        address.update_latency(Duration::from_millis(10000));
        address.update_latency(Duration::from_millis(11000));
        assert_eq!(address.get_averag_latency().unwrap(), Duration::from_millis(10000));
        assert!(address.quality_score.unwrap() >= 890); // 900 with a margin of 10s (10) delayed last seen

        address.mark_failed_connection_attempt("Testing".to_string());
        assert_eq!(address.quality_score.unwrap(), 0);

        let another_addr = "/ip4/1.0.0.1/tcp/8000".parse().unwrap();
        let another_addr = MultiaddrWithStats::new(another_addr, PeerAddressSource::Config);
        assert_eq!(another_addr.quality_score, None);

        assert_eq!(another_addr.cmp(&address), Ordering::Less);
    }
}

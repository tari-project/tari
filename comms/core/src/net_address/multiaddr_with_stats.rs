// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    cmp,
    cmp::{Ord, Ordering},
    fmt,
    hash::{Hash, Hasher},
    time::Duration,
};

use chrono::{NaiveDateTime, Utc};
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

const MAX_LATENCY_SAMPLE_COUNT: u32 = 100;
const MAX_INITIAL_DIAL_TIME_SAMPLE_COUNT: u32 = 100;

#[derive(Debug, Eq, Clone, Deserialize, Serialize)]
pub struct MultiaddrWithStats {
    address: Multiaddr,
    pub last_seen: Option<NaiveDateTime>,
    pub connection_attempts: u32,
    pub avg_initial_dial_time: Duration,
    initial_dial_time_sample_count: u32,
    pub avg_latency: Duration,
    latency_sample_count: u32,
    pub last_attempted: Option<NaiveDateTime>,
    pub last_failed_reason: Option<String>,
    pub quality_score: i32,
}

impl MultiaddrWithStats {
    /// Constructs a new net address with zero stats
    pub fn new(address: Multiaddr) -> Self {
        Self {
            address,
            last_seen: None,
            connection_attempts: 0,
            avg_initial_dial_time: Duration::from_secs(0),
            initial_dial_time_sample_count: 0,
            avg_latency: Duration::from_millis(0),
            latency_sample_count: 0,
            last_attempted: None,
            last_failed_reason: None,
            quality_score: 0,
        }
    }

    /// Constructs a new net address with usage stats
    pub fn new_with_stats(
        address: Multiaddr,
        last_seen: Option<NaiveDateTime>,
        connection_attempts: u32,
        avg_initial_dial_time: Duration,
        initial_dial_time_sample_count: u32,
        avg_latency: Duration,
        latency_sample_count: u32,
        last_attempted: Option<NaiveDateTime>,
        last_failed_reason: Option<String>,
        quality_score: i32,
    ) -> Self {
        Self {
            address,
            last_seen,
            connection_attempts,
            avg_initial_dial_time,
            initial_dial_time_sample_count,
            avg_latency,
            latency_sample_count,
            last_attempted,
            last_failed_reason,
            quality_score,
        }
    }

    pub fn merge(&mut self, other: &Self) {
        if self.address == other.address {
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
            self.calculate_quality_score();
        }
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

        self.avg_latency =
            ((self.avg_latency * self.latency_sample_count) + latency_measurement) / (self.latency_sample_count + 1);
        if self.latency_sample_count < MAX_LATENCY_SAMPLE_COUNT {
            self.latency_sample_count += 1;
        }

        self.calculate_quality_score();
    }

    pub fn update_initial_dial_time(&mut self, initial_dial_time: Duration) {
        self.last_seen = Some(Utc::now().naive_utc());

        self.avg_initial_dial_time = ((self.avg_initial_dial_time * self.initial_dial_time_sample_count) +
            initial_dial_time) /
            (self.initial_dial_time_sample_count + 1);
        if self.initial_dial_time_sample_count < MAX_INITIAL_DIAL_TIME_SAMPLE_COUNT {
            self.initial_dial_time_sample_count += 1;
        }
        self.calculate_quality_score();
    }

    /// Mark that a successful interaction occurred with this address
    pub fn mark_last_seen_now(&mut self) {
        self.last_seen = Some(Utc::now().naive_utc());
        self.last_failed_reason = None;
        self.reset_connection_attempts();
        self.calculate_quality_score();
    }

    /// Reset the connection attempts on this net address for a later session of retries
    pub fn reset_connection_attempts(&mut self) {
        self.connection_attempts = 0;
    }

    /// Mark that a connection could not be established with this net address
    pub fn mark_failed_connection_attempt(&mut self, error_string: String) {
        self.connection_attempts += 1;
        self.last_failed_reason = Some(error_string);
        self.calculate_quality_score();
    }

    /// Get as a Multiaddr
    pub fn as_net_address(&self) -> Multiaddr {
        self.clone().address
    }

    fn calculate_quality_score(&mut self) {
        // Try these first
        if self.last_seen.is_none() && self.last_attempted.is_none() {
            self.quality_score = 1000;
            return;
        }

        let mut score_self = 0;

        score_self += cmp::max(0, 100 - (self.avg_latency.as_millis() as i32 / 100));

        score_self += cmp::max(
            0,
            100 - self
                .last_seen
                .map(|x| Utc::now().naive_utc() - x)
                .map(|x| x.num_seconds())
                .unwrap_or(0) as i32,
        );

        if self.last_failed_reason.is_some() {
            score_self -= 100;
        }

        self.quality_score = score_self;
    }
}

impl From<Multiaddr> for MultiaddrWithStats {
    /// Constructs a new net address with usage stats from a net address
    fn from(net_address: Multiaddr) -> Self {
        Self {
            address: net_address,
            last_seen: None,
            connection_attempts: 0,
            avg_initial_dial_time: Duration::from_secs(0),
            initial_dial_time_sample_count: 0,
            avg_latency: Duration::new(0, 0),
            latency_sample_count: 0,
            last_attempted: None,
            last_failed_reason: None,
            quality_score: 0,
        }
    }
}

// Reliability ordering of net addresses: prioritize net addresses according to previous successful connections,
// connection attempts, latency and last seen A lower ordering has a higher priority and a higher ordering has a lower
// priority, this ordering switch allows searching for, and updating of net addresses to be performed more efficiently
impl Ord for MultiaddrWithStats {
    fn cmp(&self, other: &MultiaddrWithStats) -> Ordering {
        self.quality_score.cmp(&other.quality_score).reverse()
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

impl fmt::Display for MultiaddrWithStats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.address)
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_update_latency() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MultiaddrWithStats::from(net_address);
        let latency_measurement1 = Duration::from_millis(100);
        let latency_measurement2 = Duration::from_millis(200);
        let latency_measurement3 = Duration::from_millis(60);
        let latency_measurement4 = Duration::from_millis(140);
        net_address_with_stats.update_latency(latency_measurement1);
        assert_eq!(net_address_with_stats.avg_latency, latency_measurement1);
        net_address_with_stats.update_latency(latency_measurement2);
        assert_eq!(net_address_with_stats.avg_latency, Duration::from_millis(150));
        net_address_with_stats.update_latency(latency_measurement3);
        assert_eq!(net_address_with_stats.avg_latency, Duration::from_millis(120));
        net_address_with_stats.update_latency(latency_measurement4);
        assert_eq!(net_address_with_stats.avg_latency, Duration::from_millis(125));
    }

    #[test]
    fn test_successful_and_failed_connection_attempts() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MultiaddrWithStats::from(net_address);
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
        let mut net_address_with_stats = MultiaddrWithStats::from(net_address);
        net_address_with_stats.mark_failed_connection_attempt("asdf".to_string());
        net_address_with_stats.mark_failed_connection_attempt("asdf".to_string());
        assert_eq!(net_address_with_stats.connection_attempts, 2);
        net_address_with_stats.reset_connection_attempts();
        assert_eq!(net_address_with_stats.connection_attempts, 0);
    }
}

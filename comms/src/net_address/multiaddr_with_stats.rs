use chrono::{DateTime, Utc};
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};
use std::{
    cmp::{Ord, Ordering},
    fmt,
    time::Duration,
};

const MAX_LATENCY_SAMPLE_COUNT: u32 = 100;

#[derive(Debug, Eq, Clone, Deserialize, Serialize)]
pub struct MutliaddrWithStats {
    pub address: Multiaddr,
    pub last_seen: Option<DateTime<Utc>>,
    pub connection_attempts: u32,
    pub rejected_message_count: u32,
    pub avg_latency: Duration,
    latency_sample_count: u32,
}

impl MutliaddrWithStats {
    /// Constructs a new net address with zero stats
    pub fn new(address: Multiaddr) -> Self {
        Self {
            address,
            last_seen: None,
            connection_attempts: 0,
            rejected_message_count: 0,
            avg_latency: Duration::from_millis(0),
            latency_sample_count: 0,
        }
    }

    /// Constructs a new net address with usage stats
    pub fn new_with_stats(
        address: Multiaddr,
        last_seen: Option<DateTime<Utc>>,
        connection_attempts: u32,
        rejected_message_count: u32,
        avg_latency: Duration,
        latency_sample_count: u32,
    ) -> Self
    {
        Self {
            address,
            last_seen,
            connection_attempts,
            rejected_message_count,
            avg_latency,
            latency_sample_count,
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
        self.last_seen = Some(Utc::now());

        self.avg_latency =
            ((self.avg_latency * self.latency_sample_count) + latency_measurement) / (self.latency_sample_count + 1);
        if self.latency_sample_count < MAX_LATENCY_SAMPLE_COUNT {
            self.latency_sample_count += 1;
        }
    }

    /// Mark that a message was received from this net address
    pub fn mark_message_received(&mut self) {
        self.last_seen = Some(Utc::now());
    }

    /// Mark that a rejected message was received from this net address
    pub fn mark_message_rejected(&mut self) {
        self.last_seen = Some(Utc::now());
        self.rejected_message_count += 1;
    }

    /// Mark that a successful connection was established with this net address
    pub fn mark_successful_connection_attempt(&mut self) {
        self.last_seen = Some(Utc::now());
        self.connection_attempts = 0;
    }

    /// Reset the connection attempts on this net address for a later session of retries
    pub fn reset_connection_attempts(&mut self) {
        self.connection_attempts = 0;
    }

    /// Mark that a connection could not be established with this net address
    pub fn mark_failed_connection_attempt(&mut self) {
        self.connection_attempts += 1;
    }

    /// Get as a Multiaddr
    pub fn as_net_address(&self) -> Multiaddr {
        self.clone().address
    }
}

impl From<Multiaddr> for MutliaddrWithStats {
    /// Constructs a new net address with usage stats from a net address
    fn from(net_address: Multiaddr) -> Self {
        Self {
            address: net_address,
            last_seen: None,
            connection_attempts: 0,
            rejected_message_count: 0,
            avg_latency: Duration::new(0, 0),
            latency_sample_count: 0,
        }
    }
}

// Reliability ordering of net addresses: prioritize net addresses according to previous successful connections,
// connection attempts, latency and last seen A lower ordering has a higher priority and a higher ordering has a lower
// priority, this ordering switch allows searching for, and updating of net addresses to be performed more efficiently
impl Ord for MutliaddrWithStats {
    fn cmp(&self, other: &MutliaddrWithStats) -> Ordering {
        if self.last_seen.is_some() && other.last_seen.is_none() {
            return Ordering::Less;
        }

        if self.last_seen.is_none() && other.last_seen.is_some() {
            return Ordering::Greater;
        }
        if self.connection_attempts < other.connection_attempts {
            return Ordering::Less;
        }

        if self.connection_attempts > other.connection_attempts {
            return Ordering::Greater;
        }
        if self.latency_sample_count > 0 && other.latency_sample_count > 0 {
            if self.avg_latency < other.avg_latency {
                return Ordering::Less;
            }

            if self.avg_latency > other.avg_latency {
                return Ordering::Greater;
            }
        }
        if self.last_seen.is_some() && other.last_seen.is_some() {
            let self_last_seen = self.last_seen.unwrap();
            let other_last_seen = other.last_seen.unwrap();
            if self_last_seen > other_last_seen {
                return Ordering::Less;
            }

            if self_last_seen < other_last_seen {
                return Ordering::Greater;
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for MutliaddrWithStats {
    fn partial_cmp(&self, other: &MutliaddrWithStats) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for MutliaddrWithStats {
    fn eq(&self, other: &MutliaddrWithStats) -> bool {
        self.address == other.address
    }
}

impl fmt::Display for MutliaddrWithStats {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.address)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn test_update_latency() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MutliaddrWithStats::from(net_address);
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
    fn test_message_received_and_rejected() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MutliaddrWithStats::from(net_address);
        assert!(net_address_with_stats.last_seen.is_none());
        net_address_with_stats.mark_message_received();
        assert!(net_address_with_stats.last_seen.is_some());
        let last_seen = net_address_with_stats.last_seen.unwrap();
        net_address_with_stats.mark_message_rejected();
        net_address_with_stats.mark_message_rejected();
        assert_eq!(net_address_with_stats.rejected_message_count, 2);
        assert!(last_seen <= net_address_with_stats.last_seen.unwrap());
    }

    #[test]
    fn test_successful_and_failed_connection_attempts() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MutliaddrWithStats::from(net_address);
        net_address_with_stats.mark_failed_connection_attempt();
        net_address_with_stats.mark_failed_connection_attempt();
        assert!(net_address_with_stats.last_seen.is_none());
        assert_eq!(net_address_with_stats.connection_attempts, 2);
        net_address_with_stats.mark_successful_connection_attempt();
        assert!(net_address_with_stats.last_seen.is_some());
        assert_eq!(net_address_with_stats.connection_attempts, 0);
    }

    #[test]
    fn test_reseting_connection_attempts() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_address_with_stats = MutliaddrWithStats::from(net_address);
        net_address_with_stats.mark_failed_connection_attempt();
        net_address_with_stats.mark_failed_connection_attempt();
        assert_eq!(net_address_with_stats.connection_attempts, 2);
        net_address_with_stats.reset_connection_attempts();
        assert_eq!(net_address_with_stats.connection_attempts, 0);
    }

    #[test]
    fn test_net_address_reliability_ordering() {
        let net_address = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut na1 = MutliaddrWithStats::from(net_address.clone());
        let mut na2 = MutliaddrWithStats::from(net_address);
        thread::sleep(Duration::from_millis(1));
        na1.mark_successful_connection_attempt();
        assert!(na1 < na2);
        thread::sleep(Duration::from_millis(1));
        na2.mark_successful_connection_attempt();
        assert!(na1 > na2);
        thread::sleep(Duration::from_millis(1));
        na1.mark_message_rejected();
        assert!(na1 < na2);
        na1.update_latency(Duration::from_millis(200));
        na2.update_latency(Duration::from_millis(100));
        assert!(na1 > na2);
        na1.mark_failed_connection_attempt();
        assert!(na1 > na2);
    }
}

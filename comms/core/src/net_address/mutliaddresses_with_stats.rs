// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    ops::Index,
    time::Duration,
};

use chrono::{DateTime, NaiveDateTime, Utc};
use log::trace;
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

use crate::net_address::{MultiaddrWithStats, multiaddr_with_stats::PeerAddressSource};

const LOG_TARGET: &str = "comms::net_address::multiaddresses_with_stats";

const MAX_ADDRESSES: usize = 10;

/// This struct is used to store a set of different net addresses such as IPv4, IPv6, Tor or I2P for a single peer.
#[derive(Debug, Clone, Deserialize, Serialize, Default, Eq)]
pub struct MultiaddressesWithStats {
    addresses: Vec<MultiaddrWithStats>,
}

impl MultiaddressesWithStats {
    pub fn from_addresses_with_source(
        addresses: Vec<Multiaddr>,
        source: &PeerAddressSource,
    ) -> MultiaddressesWithStats {
        let mut addresses_with_stats = Vec::with_capacity(addresses.len());
        for address in addresses {
            addresses_with_stats.push(MultiaddrWithStats::new(address, source.clone()));
        }
        let mut addresses = MultiaddressesWithStats {
            addresses: addresses_with_stats,
        };
        addresses.sort_and_truncate_addresses();
        addresses
    }

    /// Returns the newest `updated_at` among signed address sources in this set - this can only be `None` if the source
    /// is from config.
    pub fn newest_claim_updated_at(&self) -> Option<DateTime<Utc>> {
        self.addresses
            .iter()
            .filter_map(|a| a.source().peer_identity_claim().map(|c| c.signature.updated_at()))
            .max()
    }

    pub fn empty() -> Self {
        MultiaddressesWithStats { addresses: Vec::new() }
    }

    /// Constructs a new list of addresses with usage stats from a list of net addresses
    pub fn new(addresses: Vec<MultiaddrWithStats>) -> MultiaddressesWithStats {
        MultiaddressesWithStats { addresses }
    }

    pub fn best(&self) -> Option<&MultiaddrWithStats> {
        self.addresses.first()
    }

    /// Provides the date and time of the last successful communication with this peer
    pub fn last_seen(&self) -> Option<NaiveDateTime> {
        self.addresses
            .iter()
            .max_by_key(|a| a.last_seen())
            .and_then(|a| a.last_seen())
    }

    pub fn offline_at(&self) -> Option<NaiveDateTime> {
        self.addresses
            .iter()
            .min_by_key(|a| a.offline_at())
            .and_then(|a| a.offline_at())
    }

    /// Return the time of last attempted connection to this collection of addresses
    pub fn last_attempted(&self) -> Option<NaiveDateTime> {
        self.addresses
            .iter()
            .max_by_key(|a| a.last_attempted())
            .and_then(|a| a.last_attempted())
    }

    pub fn contains(&self, net_address: &Multiaddr) -> bool {
        self.addresses.iter().any(|x| x.address() == net_address)
    }

    /// Compares the existing set of addresses to the provided address set and remove missing addresses and add new
    /// addresses without discarding the usage stats of the existing and remaining addresses. Where the source contains
    /// a claim, only the most recent claim set will be retained. [Delegates to `merge()`]
    pub fn add_or_update_addresses(&mut self, addresses: &[Multiaddr], source: &PeerAddressSource) {
        let incoming = Self::from_addresses_with_source(addresses.to_vec(), source);
        self.merge(&incoming);
    }

    /// Returns an iterator of addresses with states ordered from 'best' to 'worst' according to heuristics such as
    /// failed connections and latency.
    pub fn iter(&self) -> impl Iterator<Item = &MultiaddrWithStats> {
        self.addresses.iter()
    }

    /// Returns an iterator of addresses ordered from 'best' to 'worst' according to heuristics such as failed
    /// connections and latency.
    pub fn address_iter(&self) -> impl Iterator<Item = &Multiaddr> {
        self.addresses.iter().map(|addr| addr.address())
    }

    /// Merges another set of addresses into this set, preserving usage stats where possible.
    ///
    /// Merge logic:
    /// - If the incoming set has a strictly newer signed claim, all non-config addresses are replaced by the incoming
    ///   set.
    /// - If the incoming set is older, only usage stats for known addresses are updated.
    /// - If neither set has a signed claim (i.e. source from config) or timestamps are equal, addresses are merged
    ///   individually.
    ///
    /// After merging, addresses are sorted by quality and truncated to the maximum allowed.
    pub fn merge(&mut self, incoming: &MultiaddressesWithStats) {
        // Only allow the newest signed advertised addresses.
        let this_newest = self.newest_claim_updated_at();
        let other_newest = incoming.newest_claim_updated_at();

        match (this_newest, other_newest) {
            // Incoming claim is strictly newer -> replace non-config addrs with incoming set
            (Some(this), Some(other)) if other > this => {
                // 1) Keep config addresses
                let mut by_addr: HashMap<Multiaddr, MultiaddrWithStats> = self
                    .addresses
                    .iter()
                    .filter(|a| a.source().is_config())
                    .cloned()
                    .map(|a| (a.address().clone(), a))
                    .collect();

                // 2) Insert/override with incoming advertised set
                for item in &incoming.addresses {
                    match by_addr.get_mut(item.address()) {
                        Some(existing) => {
                            // Merge stats and prefer newer/better source
                            existing.merge(item);
                        },
                        None => {
                            by_addr.insert(item.address().clone(), item.clone());
                        },
                    }
                }

                self.addresses = by_addr.into_values().collect();
                self.sort_and_truncate_addresses();
            },

            // Incoming set is older -> only update stats for known addresses; don't append stale ones
            (Some(this), Some(other)) if other < this => {
                for addr in &incoming.addresses {
                    if let Some(existing) = self.find_address_mut(addr.address()) {
                        existing.merge(addr);
                    }
                }
                self.sort_and_truncate_addresses();
            },

            // Fall back to address-by-address merge:
            //   `this_newest(DateTime)` == `other_newest(DateTime)`, or
            //   `this_newest` and/or `other_newest` is config
            _ => {
                for addr in &incoming.addresses {
                    if let Some(existing) = self.find_address_mut(addr.address()) {
                        existing.merge(addr);
                    } else {
                        self.addresses.push(addr.clone());
                    }
                }
                self.sort_and_truncate_addresses();
            },
        }
    }

    /// Finds the specified address in the set and allow updating of its variables such as its usage stats
    fn find_address_mut(&mut self, address: &Multiaddr) -> Option<&mut MultiaddrWithStats> {
        self.addresses.iter_mut().find(|a| a.address() == address)
    }

    #[cfg(test)]
    pub fn reset_stats_to_default(&mut self, address: &Multiaddr) {
        if let Some(addr) = self.find_address_mut(address) {
            addr.reset_stats_to_default();
        }
    }

    /// The average connection latency of the provided net address will be updated to include the current measured
    /// latency sample.
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn update_latency(&mut self, address: &Multiaddr, latency_measurement: Duration) -> bool {
        match self.find_address_mut(address) {
            Some(addr) => {
                addr.update_latency(latency_measurement);
                self.sort_and_truncate_addresses();
                true
            },
            None => false,
        }
    }

    pub fn update_address_stats<F>(&mut self, address: &Multiaddr, f: F)
    where F: FnOnce(&mut MultiaddrWithStats) {
        if let Some(addr) = self.find_address_mut(address) {
            f(addr);
            self.sort_and_truncate_addresses();
        }
    }

    /// Mark that a successful interaction occurred with the specified address
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn mark_last_seen_now(&mut self, address: &Multiaddr) -> bool {
        match self.find_address_mut(address) {
            Some(addr) => {
                addr.mark_last_seen_now().mark_last_attempted_now();
                self.sort_and_truncate_addresses();
                true
            },
            None => false,
        }
    }

    /// Mark all addresses as seen with latency. Returns true if all addresses are contained in this instance, otherwise
    /// false
    pub fn mark_all_addresses_as_last_seen_now_with_latency(
        &mut self,
        addresses: &[Multiaddr],
        latency_measurement: Duration,
    ) -> bool {
        let mut all_exist = true;
        for address in addresses {
            match self.find_address_mut(address) {
                Some(addr) => {
                    addr.mark_last_seen_now().mark_last_attempted_now();
                    addr.update_latency(latency_measurement);
                },
                None => {
                    trace!(target: LOG_TARGET, "Peer address '{address}' not in claim, stats not updated");
                    all_exist = false
                },
            }
        }
        self.sort_and_truncate_addresses();
        all_exist
    }

    /// Mark that a connection could not be established with the specified net address
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn mark_failed_connection_attempt(&mut self, address: &Multiaddr, failed_reason: String) -> bool {
        match self.find_address_mut(address) {
            Some(addr) => {
                addr.mark_failed_connection_attempt(failed_reason);
                addr.mark_last_attempted_now();
                self.sort_and_truncate_addresses();
                true
            },
            None => {
                trace!(target: LOG_TARGET, "Peer address '{address}' not in claim, stats not updated");
                false
            },
        }
    }

    /// Reset the connection attempts stat on all of this Peers net addresses to retry connection
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn reset_connection_attempts(&mut self) {
        for a in &mut self.addresses {
            a.reset_connection_attempts();
        }
        self.sort_and_truncate_addresses();
    }

    /// Returns the number of addresses
    pub fn len(&self) -> usize {
        self.addresses.len()
    }

    /// Returns if there are addresses or not
    pub fn is_empty(&self) -> bool {
        self.addresses.is_empty()
    }

    pub fn into_vec(self) -> Vec<Multiaddr> {
        self.addresses.into_iter().map(|addr| addr.address().clone()).collect()
    }

    pub fn addresses(&self) -> &[MultiaddrWithStats] {
        &self.addresses
    }

    /// Sort the addresses with the greatest quality score first. In case of a tie, advertised
    /// addresses are preferred. Truncates the list to the maximum allowed number of addresses.
    fn sort_and_truncate_addresses(&mut self) {
        self.addresses.sort_by(|a, b| {
            let qual_a = a.quality_score().unwrap_or_default();
            let qual_b = b.quality_score().unwrap_or_default();
            match qual_b.cmp(&qual_a) {
                std::cmp::Ordering::Equal => {
                    // Advertised > Config on tie
                    let a_if_config = if a.source().is_config() { 0 } else { 1 };
                    let b_if_config = if b.source().is_config() { 0 } else { 1 };
                    b_if_config.cmp(&a_if_config)
                },
                ord => ord,
            }
        });
        self.addresses.truncate(MAX_ADDRESSES)
    }
}

impl PartialEq for MultiaddressesWithStats {
    fn eq(&self, other: &Self) -> bool {
        self.addresses == other.addresses
    }
}

impl Index<usize> for MultiaddressesWithStats {
    type Output = MultiaddrWithStats;

    /// Returns the NetAddressWithStats at the given index
    fn index(&self, index: usize) -> &Self::Output {
        self.addresses.get(index).expect("Index out of bounds")
    }
}

impl From<Vec<MultiaddrWithStats>> for MultiaddressesWithStats {
    /// Constructs NetAddressesWithStats from a list of addresses with usage stats
    fn from(addresses: Vec<MultiaddrWithStats>) -> Self {
        MultiaddressesWithStats { addresses }
    }
}

impl From<MultiaddressesWithStats> for Vec<String> {
    fn from(value: MultiaddressesWithStats) -> Self {
        value
            .addresses
            .into_iter()
            .map(|addr| addr.address().to_string())
            .collect()
    }
}

impl Display for MultiaddressesWithStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.addresses
                .iter()
                .map(|a| a.address().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use digest::crypto_common::rand_core::OsRng;
    use tari_crypto::keys::SecretKey;

    use super::*;
    use crate::{
        peer_manager::{IdentitySignature, PeerFeatures, PeerIdentityClaim},
        types::{CommsPublicKey, CommsSecretKey},
    };

    #[test]
    fn test_index_impl() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_addresses: MultiaddressesWithStats = MultiaddressesWithStats::from_addresses_with_source(
            vec![net_address1.clone(), net_address2.clone(), net_address3.clone()],
            &PeerAddressSource::Config,
        );

        assert_eq!(net_addresses[0].address(), &net_address1);
        assert_eq!(net_addresses[1].address(), &net_address2);
        assert_eq!(net_addresses[2].address(), &net_address3);
    }

    #[test]
    fn test_max_number() {
        let net_address1 = "/ip4/121.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/122.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/123.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address4 = "/ip4/124.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address5 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address6 = "/ip4/126.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address7 = "/ip4/127.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address8 = "/ip4/128.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address9 = "/ip4/129.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address10 = "/ip4/130.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address11 = "/ip4/131.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address12 = "/ip4/132.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let mut net_addresses: MultiaddressesWithStats = MultiaddressesWithStats::from_addresses_with_source(
            vec![
                net_address1.clone(),
                net_address2,
                net_address3,
                net_address4,
                net_address5,
                net_address6,
                net_address7,
                net_address8,
                net_address9,
                net_address10,
                net_address11.clone(),
            ],
            &PeerAddressSource::Config,
        );
        assert_eq!(net_addresses.addresses().len(), 10);
        // because qaulity is the same, the last address will be trimmed
        assert!(!net_addresses.contains(&net_address11));
        // let mark down the quality of the first address
        net_addresses
            .find_address_mut(&net_address1)
            .unwrap()
            .update_latency(Duration::from_millis(0));
        net_addresses
            .find_address_mut(&net_address1)
            .unwrap()
            .mark_last_attempted_now();
        assert_eq!(
            net_addresses.find_address_mut(&net_address1).unwrap().quality_score(),
            Some(1000)
        );
        let address_12: MultiaddressesWithStats = MultiaddressesWithStats::from_addresses_with_source(
            vec![net_address12.clone()],
            &PeerAddressSource::Config,
        );
        net_addresses.merge(&address_12);
        assert!(net_addresses.contains(&net_address1));
        assert!(!net_addresses.contains(&net_address12));
    }

    #[test]
    fn test_last_seen() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address1.clone()], &PeerAddressSource::Config);
        net_addresses.add_or_update_addresses(std::slice::from_ref(&net_address2), &PeerAddressSource::Config);
        net_addresses.add_or_update_addresses(std::slice::from_ref(&net_address3), &PeerAddressSource::Config);

        assert!(net_addresses.mark_last_seen_now(&net_address3));
        assert!(net_addresses.mark_last_seen_now(&net_address1));
        assert!(net_addresses.mark_last_seen_now(&net_address2));
        let desired_last_seen = net_addresses
            .addresses
            .iter()
            .max_by_key(|a| a.last_seen())
            .map(|a| a.last_seen().unwrap());
        let last_seen = net_addresses.last_seen();
        assert_eq!(desired_last_seen.unwrap(), last_seen.unwrap());
    }

    #[test]
    fn test_add_net_address() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address1.clone()], &PeerAddressSource::Config);
        net_addresses.add_or_update_addresses(std::slice::from_ref(&net_address2), &PeerAddressSource::Config);
        net_addresses.add_or_update_addresses(std::slice::from_ref(&net_address3), &PeerAddressSource::Config);
        // Add duplicate address, this resets the quality score
        net_addresses.add_or_update_addresses(std::slice::from_ref(&net_address2), &PeerAddressSource::Config);
        assert_eq!(net_addresses.addresses.len(), 3);
        assert_eq!(net_addresses.addresses[0].address(), &net_address1);
        assert_eq!(net_addresses.addresses[1].address(), &net_address2);
        assert_eq!(net_addresses.addresses[2].address(), &net_address3);
    }

    #[test]
    fn test_get_net_address() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses =
            MultiaddressesWithStats::from_addresses_with_source(vec![net_address1.clone()], &PeerAddressSource::Config);
        net_addresses.add_or_update_addresses(std::slice::from_ref(&net_address2), &PeerAddressSource::Config);
        net_addresses.add_or_update_addresses(std::slice::from_ref(&net_address3), &PeerAddressSource::Config);

        let priority_address = net_addresses.address_iter().next().unwrap();
        assert_eq!(priority_address, &net_address1);

        net_addresses.mark_last_seen_now(&net_address1);
        net_addresses.mark_last_seen_now(&net_address2);
        net_addresses.mark_last_seen_now(&net_address3);
        assert!(net_addresses.update_latency(&net_address1, Duration::from_millis(250)));
        assert!(net_addresses.update_latency(&net_address2, Duration::from_millis(50)));
        assert!(net_addresses.update_latency(&net_address3, Duration::from_millis(100)));
        let priority_address = net_addresses.address_iter().next().unwrap();
        assert_eq!(priority_address, &net_address2);

        assert!(net_addresses.mark_failed_connection_attempt(&net_address2, "error".to_string()));
        let priority_address = net_addresses.address_iter().next().unwrap();
        assert_eq!(priority_address, &net_address3);
    }

    #[test]
    fn test_resetting_all_connection_attempts() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let addresses: Vec<MultiaddrWithStats> = vec![
            MultiaddrWithStats::new(net_address1.clone(), PeerAddressSource::Config),
            MultiaddrWithStats::new(net_address2.clone(), PeerAddressSource::Config),
            MultiaddrWithStats::new(net_address3.clone(), PeerAddressSource::Config),
        ];
        let mut net_addresses = MultiaddressesWithStats::new(addresses);
        assert!(net_addresses.mark_failed_connection_attempt(&net_address1, "error".to_string()));
        assert!(net_addresses.mark_failed_connection_attempt(&net_address2, "error".to_string()));
        assert!(net_addresses.mark_failed_connection_attempt(&net_address3, "error".to_string()));
        assert!(net_addresses.mark_failed_connection_attempt(&net_address1, "error".to_string()));

        assert_eq!(net_addresses.addresses[0].connection_attempts(), 2);
        assert_eq!(net_addresses.addresses[1].connection_attempts(), 1);
        assert_eq!(net_addresses.addresses[2].connection_attempts(), 1);
        assert!(net_addresses.addresses[0].last_failed_reason().is_some());
        assert!(net_addresses.addresses[1].last_failed_reason().is_some());
        assert!(net_addresses.addresses[2].last_failed_reason().is_some());
        net_addresses.reset_connection_attempts();
        assert_eq!(net_addresses.addresses[0].connection_attempts(), 0);
        assert_eq!(net_addresses.addresses[1].connection_attempts(), 0);
        assert_eq!(net_addresses.addresses[2].connection_attempts(), 0);
        assert!(net_addresses.addresses[0].last_failed_reason().is_none());
        assert!(net_addresses.addresses[1].last_failed_reason().is_none());
        assert!(net_addresses.addresses[2].last_failed_reason().is_none());
    }

    #[test]
    fn it_merges_addresses_with_stats_correctly() {
        use crate::net_address::multiaddr_with_stats::PeerAddressSource;

        let addr1 = "/ip4/1.1.1.1/tcp/1000".parse::<Multiaddr>().unwrap();
        let addr2 = "/ip4/2.2.2.2/tcp/2000".parse::<Multiaddr>().unwrap();
        let addr3 = "/ip4/3.3.3.3/tcp/3000".parse::<Multiaddr>().unwrap();

        let addr_set_1 = vec![addr1.clone(), addr2.clone()];
        let set1 = MultiaddressesWithStats::from_addresses_with_source(
            addr_set_1.clone(),
            &PeerAddressSource::FromPeerConnection {
                peer_identity_claim: PeerIdentityClaim {
                    addresses: addr_set_1.clone(),
                    features: PeerFeatures::COMMUNICATION_NODE,
                    signature: create_identity_signature(&addr_set_1),
                },
            },
        );
        assert!(set1.contains(&addr1));
        assert!(set1.contains(&addr2));
        assert_eq!(set1.len(), 2);

        // Sleep to ensure different timestamp
        std::thread::sleep(Duration::from_millis(150));
        let addr_set_2 = vec![addr2.clone(), addr3.clone()];
        let set2 = MultiaddressesWithStats::from_addresses_with_source(
            addr_set_2.clone(),
            &PeerAddressSource::FromPeerConnection {
                peer_identity_claim: PeerIdentityClaim {
                    addresses: addr_set_2.clone(),
                    features: PeerFeatures::COMMUNICATION_NODE,
                    signature: create_identity_signature(&addr_set_2),
                },
            },
        );

        let set3 = MultiaddressesWithStats::from_addresses_with_source(
            vec![addr1.clone(), addr3.clone()],
            &PeerAddressSource::Config,
        );

        // Test case 1 - set1 is older, so only set should be retained

        let mut set1_merge = set1.clone();
        set1_merge.merge(&set2);

        // Only addr2 and addr3 from set_2 should be present
        assert_eq!(set1_merge.len(), 2);
        assert!(set1_merge.contains(&addr2));
        assert!(set1_merge.contains(&addr3));

        // Test case 2 - set1 is older, only update stats for known addresses; don't append stale ones

        let mut set1_update = set1.clone();
        let addr_1_mut = set1_update.find_address_mut(&addr1).unwrap();
        addr_1_mut.update_latency(Duration::from_secs(1));
        let addr_2_mut = set1_update.find_address_mut(&addr2).unwrap();
        addr_2_mut.update_latency(Duration::from_secs(2));

        let mut set2_merge = set2.clone();
        set2_merge.merge(&set1_update);

        // Only addr2 and addr3 should be present

        assert_eq!(set2_merge.len(), 2);
        assert!(!set2_merge.contains(&addr1));
        assert!(set2_merge.contains(&addr2));
        assert!(set2_merge.contains(&addr3));

        // Latency should be updated for addr2

        assert_eq!(
            set2_merge.find_address_mut(&addr2).unwrap().avg_latency(),
            Some(Duration::from_secs(2))
        );

        // Test case 3 - merge with config addresses, which are always accepted

        let mut set3_update = set3.clone();
        let addr_1_mut = set3_update.find_address_mut(&addr1).unwrap();
        addr_1_mut.update_latency(Duration::from_secs(4));
        let addr_3_mut = set3_update.find_address_mut(&addr3).unwrap();
        addr_3_mut.update_latency(Duration::from_secs(3));
        set2_merge.merge(&set3_update);

        // Only addr1, addr2 and addr3 should be present

        assert_eq!(set2_merge.len(), 3);
        assert!(set2_merge.contains(&addr1));
        assert!(set2_merge.contains(&addr2));
        assert!(set2_merge.contains(&addr3));

        // Latency should be updated for addr1 and addr3

        assert_eq!(
            set2_merge.find_address_mut(&addr1).unwrap().avg_latency(),
            Some(Duration::from_secs(4))
        );
        assert_eq!(
            set2_merge.find_address_mut(&addr2).unwrap().avg_latency(),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            set2_merge.find_address_mut(&addr3).unwrap().avg_latency(),
            Some(Duration::from_secs(3))
        );
    }

    fn create_identity_signature(addresses: &[Multiaddr]) -> IdentitySignature {
        let secret = CommsSecretKey::random(&mut OsRng);
        let public_key = CommsPublicKey::from_secret_key(&secret);
        let updated_at = Utc::now();
        let identity = IdentitySignature::sign_new(&secret, PeerFeatures::COMMUNICATION_NODE, addresses, updated_at);
        assert!(
            identity
                .is_valid(&public_key, PeerFeatures::COMMUNICATION_NODE, addresses)
                .unwrap(),
            "Signature is not valid"
        );
        identity
    }

    #[test]
    fn it_merges_when_config_overlaps() {
        let addr_1 = "/ip4/9.9.9.9/tcp/9999".parse::<Multiaddr>().unwrap();
        let addr_2 = "/ip4/7.7.7.7/tcp/7777".parse::<Multiaddr>().unwrap();

        // Base has address as Config
        let mut base = MultiaddressesWithStats::from_addresses_with_source(
            vec![addr_1.clone(), addr_2.clone()],
            &PeerAddressSource::Config,
        );

        // Incoming has the same address as base, but newer with an advertised source
        let incoming = MultiaddressesWithStats::from_addresses_with_source(
            vec![addr_1.clone()],
            &PeerAddressSource::FromPeerConnection {
                peer_identity_claim: PeerIdentityClaim {
                    addresses: vec![addr_1.clone()],
                    features: PeerFeatures::COMMUNICATION_NODE,
                    signature: create_identity_signature(std::slice::from_ref(&addr_1)),
                },
            },
        );

        base.merge(&incoming);
        let a1 = base.find_address_mut(&addr_1).unwrap();
        assert!(!a1.source().is_config());
        let a2 = base.find_address_mut(&addr_2).unwrap();
        assert!(a2.source().is_config());
    }

    #[test]
    fn it_prefers_advertised_addresses_over_config_on_tie() {
        use crate::net_address::multiaddr_with_stats::PeerAddressSource;

        let addr_1 = "/ip4/1.1.1.1/tcp/1000".parse::<Multiaddr>().unwrap();
        let addr_2 = "/ip4/2.2.2.2/tcp/2000".parse::<Multiaddr>().unwrap();

        // Both addresses have the same quality score
        let mut stats_1 = MultiaddrWithStats::new(addr_1.clone(), PeerAddressSource::Config);
        let mut stats_2 = MultiaddrWithStats::new(addr_2.clone(), PeerAddressSource::FromPeerConnection {
            peer_identity_claim: PeerIdentityClaim {
                addresses: vec![addr_2.clone()],
                features: PeerFeatures::COMMUNICATION_NODE,
                signature: create_identity_signature(std::slice::from_ref(&addr_2)),
            },
        });

        // Set the same quality score for both
        stats_1.update_latency(Duration::from_millis(100));
        stats_2.update_latency(Duration::from_millis(100));

        let mut addresses = MultiaddressesWithStats::new(vec![stats_1, stats_2]);

        assert_eq!(addresses.addresses()[0].address(), &addr_1);
        assert_eq!(addresses.addresses()[1].address(), &addr_2);

        addresses.sort_and_truncate_addresses();

        // Advertised (addr2) should come before config (addr1)
        assert_eq!(addresses.addresses()[0].address(), &addr_2);
        assert_eq!(addresses.addresses()[1].address(), &addr_1);
    }
}

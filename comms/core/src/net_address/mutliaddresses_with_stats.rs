// Copyright 2022 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt::{Display, Formatter},
    ops::Index,
    time::Duration,
};

use chrono::{DateTime, Utc};
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};

use crate::net_address::MutliaddrWithStats;

/// This struct is used to store a set of different net addresses such as IPv4, IPv6, Tor or I2P for a single peer.
#[derive(Debug, Clone, Deserialize, Serialize, Default, Eq)]
pub struct MultiaddressesWithStats {
    pub addresses: Vec<MutliaddrWithStats>,
    last_attempted: Option<DateTime<Utc>>,
}

impl MultiaddressesWithStats {
    /// Constructs a new list of addresses with usage stats from a list of net addresses
    pub fn new(addresses: Vec<MutliaddrWithStats>) -> MultiaddressesWithStats {
        MultiaddressesWithStats {
            addresses,
            last_attempted: None,
        }
    }

    pub fn first(&self) -> Option<&MutliaddrWithStats> {
        self.addresses.first()
    }

    /// Provides the date and time of the last successful communication with this peer
    pub fn last_seen(&self) -> Option<DateTime<Utc>> {
        let mut latest_valid_datetime: Option<DateTime<Utc>> = None;
        for curr_address in &self.addresses {
            if curr_address.last_seen.is_none() {
                continue;
            }
            match latest_valid_datetime {
                Some(latest_datetime) => {
                    if latest_datetime < curr_address.last_seen.unwrap() {
                        latest_valid_datetime = curr_address.last_seen;
                    }
                },
                None => latest_valid_datetime = curr_address.last_seen,
            }
        }
        latest_valid_datetime
    }

    /// Return the time of last attempted connection to this collection of addresses
    pub fn last_attempted(&self) -> Option<DateTime<Utc>> {
        self.last_attempted
    }

    /// Adds a new net address to the peer. This function will not add a duplicate if the address
    /// already exists.
    pub fn add_address(&mut self, net_address: &Multiaddr) {
        if !self.addresses.iter().any(|x| x.address == *net_address) {
            self.addresses.push(net_address.clone().into());
            self.addresses.sort();
        }
    }

    /// Compares the existing set of addresses to the provided address set and remove missing addresses and
    /// add new addresses without discarding the usage stats of the existing and remaining addresses.
    pub fn update_addresses(&mut self, addresses: Vec<Multiaddr>) {
        self.addresses = self
            .addresses
            .drain(..)
            .filter(|addr| addresses.contains(&addr.address))
            .collect();

        let to_add = addresses
            .into_iter()
            .filter(|addr| !self.addresses.iter().any(|a| a.address == *addr))
            .collect::<Vec<_>>();

        for address in to_add {
            self.addresses.push(address.into());
        }

        self.addresses.sort();
    }

    /// Returns an iterator of addresses ordered from 'best' to 'worst' according to heuristics such as failed
    /// connections and latency.
    pub fn iter(&self) -> impl Iterator<Item = &Multiaddr> {
        self.addresses.iter().map(|addr| &addr.address)
    }

    pub fn to_lexicographically_sorted(&self) -> Vec<Multiaddr> {
        let mut addresses = self.iter().cloned().collect::<Vec<_>>();
        addresses.sort_by(|a, b| {
            let bytes_a = a.as_ref();
            let bytes_b = b.as_ref();
            bytes_a.cmp(bytes_b)
        });
        addresses
    }

    /// Finds the specified address in the set and allow updating of its variables such as its usage stats
    fn find_address_mut(&mut self, address: &Multiaddr) -> Option<&mut MutliaddrWithStats> {
        self.addresses.iter_mut().find(|a| &a.address == address)
    }

    /// The average connection latency of the provided net address will be updated to include the current measured
    /// latency sample.
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn update_latency(&mut self, address: &Multiaddr, latency_measurement: Duration) -> bool {
        match self.find_address_mut(address) {
            Some(addr) => {
                addr.update_latency(latency_measurement);
                self.addresses.sort();
                true
            },
            None => false,
        }
    }

    /// Mark that a message was received from the specified net address
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn mark_message_received(&mut self, address: &Multiaddr) -> bool {
        match self.find_address_mut(address) {
            Some(addr) => {
                addr.mark_message_received();
                self.addresses.sort();
                true
            },
            None => false,
        }
    }

    /// Mark that a rejected message was received from the specified net address
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn mark_message_rejected(&mut self, address: &Multiaddr) -> bool {
        match self.find_address_mut(address) {
            Some(addr) => {
                addr.mark_message_rejected();
                self.addresses.sort();
                true
            },
            None => false,
        }
    }

    /// Mark that a successful interaction occurred with the specified address
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn mark_last_seen_now(&mut self, address: &Multiaddr) -> bool {
        match self.find_address_mut(address) {
            Some(addr) => {
                addr.mark_last_seen_now();
                self.last_attempted = Some(Utc::now());
                self.addresses.sort();
                true
            },
            None => false,
        }
    }

    /// Mark that a connection could not be established with the specified net address
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn mark_failed_connection_attempt(&mut self, address: &Multiaddr) -> bool {
        match self.find_address_mut(address) {
            Some(addr) => {
                addr.mark_failed_connection_attempt();
                self.last_attempted = Some(Utc::now());
                self.addresses.sort();
                true
            },
            None => false,
        }
    }

    /// Reset the connection attempts stat on all of this Peers net addresses to retry connection
    ///
    /// Returns true if the address is contained in this instance, otherwise false
    pub fn reset_connection_attempts(&mut self) {
        for a in &mut self.addresses {
            a.reset_connection_attempts();
        }
        self.addresses.sort();
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
        self.addresses.into_iter().map(|addr| addr.address).collect()
    }
}

impl PartialEq for MultiaddressesWithStats {
    fn eq(&self, other: &Self) -> bool {
        self.addresses == other.addresses
    }
}

impl Index<usize> for MultiaddressesWithStats {
    type Output = MutliaddrWithStats;

    /// Returns the NetAddressWithStats at the given index
    fn index(&self, index: usize) -> &Self::Output {
        &self.addresses[index]
    }
}

impl From<Multiaddr> for MultiaddressesWithStats {
    /// Constructs a new list of addresses with usage stats from a single net address
    fn from(net_address: Multiaddr) -> Self {
        MultiaddressesWithStats {
            addresses: vec![MutliaddrWithStats::from(net_address)],
            last_attempted: None,
        }
    }
}

impl From<Vec<Multiaddr>> for MultiaddressesWithStats {
    /// Constructs a new list of addresses with usage stats from a Vec<Multiaddr>
    fn from(net_addresses: Vec<Multiaddr>) -> Self {
        MultiaddressesWithStats {
            addresses: net_addresses
                .into_iter()
                .map(MutliaddrWithStats::from)
                .collect::<Vec<MutliaddrWithStats>>(),
            last_attempted: None,
        }
    }
}

impl From<Vec<MutliaddrWithStats>> for MultiaddressesWithStats {
    /// Constructs NetAddressesWithStats from a list of addresses with usage stats
    fn from(addresses: Vec<MutliaddrWithStats>) -> Self {
        MultiaddressesWithStats {
            addresses,
            last_attempted: None,
        }
    }
}

impl Display for MultiaddressesWithStats {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.addresses
                .iter()
                .map(|a| a.address.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[cfg(test)]
mod test {
    use multiaddr::Multiaddr;

    use super::*;

    #[test]
    fn test_index_impl() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_addresses: MultiaddressesWithStats =
            vec![net_address1.clone(), net_address2.clone(), net_address3.clone()].into();

        assert_eq!(net_addresses[0].address, net_address1);
        assert_eq!(net_addresses[1].address, net_address2);
        assert_eq!(net_addresses[2].address, net_address3);
    }

    #[test]
    fn test_last_seen() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses = MultiaddressesWithStats::from(net_address1.clone());
        net_addresses.add_address(&net_address2);
        net_addresses.add_address(&net_address3);

        assert!(net_addresses.mark_last_seen_now(&net_address3));
        assert!(net_addresses.mark_last_seen_now(&net_address1));
        assert!(net_addresses.mark_last_seen_now(&net_address2));
        let desired_last_seen = net_addresses.addresses[0].last_seen;
        let last_seen = net_addresses.last_seen();
        assert_eq!(desired_last_seen.unwrap(), last_seen.unwrap());
    }

    #[test]
    fn test_add_net_address() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses = MultiaddressesWithStats::from(net_address1.clone());
        net_addresses.add_address(&net_address2);
        net_addresses.add_address(&net_address3);
        // Add duplicate address, test add_net_address is idempotent
        net_addresses.add_address(&net_address2);
        assert_eq!(net_addresses.addresses.len(), 3);
        assert_eq!(net_addresses.addresses[0].address, net_address1);
        assert_eq!(net_addresses.addresses[1].address, net_address2);
        assert_eq!(net_addresses.addresses[2].address, net_address3);
    }

    #[test]
    fn test_get_net_address() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let mut net_addresses = MultiaddressesWithStats::from(net_address1.clone());
        net_addresses.add_address(&net_address2);
        net_addresses.add_address(&net_address3);

        let priority_address = net_addresses.iter().next().unwrap();
        assert_eq!(priority_address, &net_address1);

        assert!(net_addresses.update_latency(&net_address1, Duration::from_millis(250)));
        assert!(net_addresses.update_latency(&net_address2, Duration::from_millis(50)));
        assert!(net_addresses.update_latency(&net_address3, Duration::from_millis(100)));
        let priority_address = net_addresses.iter().next().unwrap();
        assert_eq!(priority_address, &net_address2);

        assert!(net_addresses.mark_failed_connection_attempt(&net_address2));
        let priority_address = net_addresses.iter().next().unwrap();
        assert_eq!(priority_address, &net_address3);
    }

    // TODO: Broken in release mode - investigate and fix
    //    #[test]
    //    fn test_stats_updates_on_addresses() {
    //        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
    //        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
    //        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
    //        let mut addresses: Vec<NetAddressWithStats> = Vec::new();
    //        addresses.push(NetAddressWithStats::from(net_address1.clone()));
    //        addresses.push(NetAddressWithStats::from(net_address2.clone()));
    //        addresses.push(NetAddressWithStats::from(net_address3.clone()));
    //        let mut net_addresses = NetAddressesWithStats::new(addresses);
    //
    //        assert!(net_addresses.update_latency(&net_address2, Duration::from_millis(200)));
    //        assert_eq!(net_addresses.addresses[0].avg_latency, Duration::from_millis(200));
    //        assert_eq!(net_addresses.addresses[1].avg_latency, Duration::from_millis(0));
    //        assert_eq!(net_addresses.addresses[2].avg_latency, Duration::from_millis(0));
    //
    //        thread::sleep(Duration::from_millis(1));
    //        assert!(net_addresses.mark_message_received(&net_address1));
    //        assert!(net_addresses.addresses[0].last_seen.is_some());
    //        assert!(net_addresses.addresses[1].last_seen.is_some());
    //        assert!(net_addresses.addresses[2].last_seen.is_none());
    //        assert!(net_addresses.addresses[0].last_seen.unwrap() > net_addresses.addresses[1].last_seen.unwrap());
    //
    //        assert!(net_addresses.mark_message_rejected(&net_address2));
    //        assert!(net_addresses.mark_message_rejected(&net_address3));
    //        assert!(net_addresses.mark_message_rejected(&net_address3));
    //        assert_eq!(net_addresses.addresses[0].rejected_message_count, 2);
    //        assert_eq!(net_addresses.addresses[1].rejected_message_count, 1);
    //        assert_eq!(net_addresses.addresses[2].rejected_message_count, 0);
    //
    //        assert!(net_addresses.mark_failed_connection_attempt(&net_address1));
    //        assert!(net_addresses.mark_failed_connection_attempt(&net_address2));
    //        assert!(net_addresses.mark_failed_connection_attempt(&net_address3));
    //        assert!(net_addresses.mark_failed_connection_attempt(&net_address1));
    //        assert!(net_addresses.mark_last_seen_now(&net_address2));
    //        assert_eq!(net_addresses.addresses[0].connection_attempts, 0);
    //        assert_eq!(net_addresses.addresses[1].connection_attempts, 1);
    //        assert_eq!(net_addresses.addresses[2].connection_attempts, 2);
    //    }

    #[test]
    fn test_resetting_all_connection_attempts() {
        let net_address1 = "/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address2 = "/ip4/125.1.54.254/tcp/7999".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/175.6.3.145/tcp/8000".parse::<Multiaddr>().unwrap();
        let addresses: Vec<MutliaddrWithStats> = vec![
            MutliaddrWithStats::from(net_address1.clone()),
            MutliaddrWithStats::from(net_address2.clone()),
            MutliaddrWithStats::from(net_address3.clone()),
        ];
        let mut net_addresses = MultiaddressesWithStats::new(addresses);
        assert!(net_addresses.mark_failed_connection_attempt(&net_address1));
        assert!(net_addresses.mark_failed_connection_attempt(&net_address2));
        assert!(net_addresses.mark_failed_connection_attempt(&net_address3));
        assert!(net_addresses.mark_failed_connection_attempt(&net_address1));

        assert_eq!(net_addresses.addresses[0].connection_attempts, 1);
        assert_eq!(net_addresses.addresses[1].connection_attempts, 1);
        assert_eq!(net_addresses.addresses[2].connection_attempts, 2);
        net_addresses.reset_connection_attempts();
        assert_eq!(net_addresses.addresses[0].connection_attempts, 0);
        assert_eq!(net_addresses.addresses[1].connection_attempts, 0);
        assert_eq!(net_addresses.addresses[2].connection_attempts, 0);
    }
}

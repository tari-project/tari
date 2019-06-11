use crate::connection::{
    net_address::{net_address_with_stats::NetAddressWithStats, NetAddressError},
    NetAddress,
};
use chrono::prelude::*;
use serde::{Deserialize, Serialize};
use std::time::Duration;

pub const MAX_CONNECTION_ATTEMPTS: u32 = 3;

/// This struct is used to store a set of different net addresses such as IPv4, IPv6, Tor or I2P for a single peer.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Default)]
pub struct NetAddressesWithStats {
    pub addresses: Vec<NetAddressWithStats>,
    last_attempted: Option<DateTime<Utc>>,
}

impl NetAddressesWithStats {
    /// Constructs a new list of addresses with usage stats from a list of net addresses
    pub fn new(addresses: Vec<NetAddressWithStats>) -> NetAddressesWithStats {
        NetAddressesWithStats {
            addresses,
            last_attempted: None,
        }
    }

    /// Finds the specified address in the set and allow updating of its variables such as its usage stats
    pub fn find_address_mut(&mut self, address: &NetAddress) -> Result<&mut NetAddressWithStats, NetAddressError> {
        for (i, curr_address) in &mut self.addresses.iter().enumerate() {
            if curr_address.net_address == *address {
                return self.addresses.get_mut(i).ok_or(NetAddressError::AddressNotFound);
            }
        }
        Err(NetAddressError::AddressNotFound)
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

    /// Adds a new net address to the peer if it doesn't yet exist
    pub fn add_net_address(&mut self, net_address: &NetAddress) -> Result<(), NetAddressError> {
        let mut found_flag = false;
        for curr_address in &self.addresses {
            if curr_address.net_address == *net_address {
                found_flag = true;
                break;
            }
        }
        if !found_flag {
            self.addresses.push(NetAddressWithStats::from(net_address.clone()));
            Ok(())
        } else {
            Err(NetAddressError::DuplicateAddress)
        }
    }

    /// Finds and returns the highest priority net address until all connection attempts for each net address have been
    /// reached
    pub fn get_best_net_address(&mut self) -> Result<NetAddress, NetAddressError> {
        if self.addresses.len() >= 1 {
            let any_reachable = self
                .addresses
                .iter()
                .any(|a| a.connection_attempts < MAX_CONNECTION_ATTEMPTS);
            if any_reachable {
                self.addresses.sort();
                Ok(self.addresses[0].net_address.clone())
            } else {
                Err(NetAddressError::ConnectionAttemptsExceeded)
            }
        } else {
            Err(NetAddressError::NoValidAddresses)
        }
    }

    /// The average connection latency of the provided net address will be updated to include the current measured
    /// latency sample
    pub fn update_latency(
        &mut self,
        address: &NetAddress,
        latency_measurement: Duration,
    ) -> Result<(), NetAddressError>
    {
        let updatable_address = self.find_address_mut(address)?;
        updatable_address.update_latency(latency_measurement);
        Ok(())
    }

    /// Mark that a message was received from the specified net address
    pub fn mark_message_received(&mut self, address: &NetAddress) -> Result<(), NetAddressError> {
        let updatable_address = self.find_address_mut(address)?;
        updatable_address.mark_message_received();
        Ok(())
    }

    /// Mark that a rejected message was received from the specified net address
    pub fn mark_message_rejected(&mut self, address: &NetAddress) -> Result<(), NetAddressError> {
        let updatable_address = self.find_address_mut(address)?;
        updatable_address.mark_message_rejected();
        Ok(())
    }

    /// Mark that a successful connection was established with the specified net address
    pub fn mark_successful_connection_attempt(&mut self, address: &NetAddress) -> Result<(), NetAddressError> {
        let updatable_address = self.find_address_mut(address)?;
        updatable_address.mark_successful_connection_attempt();
        self.last_attempted = Some(Utc::now());
        Ok(())
    }

    /// Mark that a connection could not be established with the specified net address
    pub fn mark_failed_connection_attempt(&mut self, address: &NetAddress) -> Result<(), NetAddressError> {
        let updatable_address = self.find_address_mut(address)?;
        updatable_address.mark_failed_connection_attempt();
        self.last_attempted = Some(Utc::now());
        Ok(())
    }

    /// Reset the connection attempts stat on all of this Peers net addresses to retry connection
    pub fn reset_connection_attempts(&mut self) {
        for a in self.addresses.iter_mut() {
            a.reset_connection_attempts();
        }
    }

    /// Returns the number of addresses
    pub fn len(&self) -> usize {
        self.addresses.len()
    }
}

impl From<NetAddress> for NetAddressesWithStats {
    /// Constructs a new list of addresses with usage stats from a single net address
    fn from(net_address: NetAddress) -> Self {
        NetAddressesWithStats {
            addresses: vec![NetAddressWithStats::from(net_address)],
            last_attempted: None,
        }
    }
}

impl From<Vec<NetAddress>> for NetAddressesWithStats {
    /// Constructs a new list of addresses with usage stats from a Vec<NetAddress>
    fn from(net_addresses: Vec<NetAddress>) -> Self {
        NetAddressesWithStats {
            addresses: net_addresses
                .into_iter()
                .map(|addr| NetAddressWithStats::from(addr))
                .collect::<Vec<NetAddressWithStats>>(),
            last_attempted: None,
        }
    }
}

impl From<Vec<NetAddressWithStats>> for NetAddressesWithStats {
    /// Constructs NetAddressesWithStats from a list of addresses with usage stats
    fn from(addresses: Vec<NetAddressWithStats>) -> Self {
        NetAddressesWithStats {
            addresses,
            last_attempted: None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::{
        net_address::{net_address_with_stats::NetAddressWithStats, net_addresses::NetAddressesWithStats},
        NetAddress,
    };
    use std::thread;

    #[test]
    fn test_last_seen() {
        let net_address1 = "123.0.0.123:8000".parse::<NetAddress>().unwrap();
        let net_address2 = "125.1.54.254:7999".parse::<NetAddress>().unwrap();
        let net_address3 = "175.6.3.145:8000".parse::<NetAddress>().unwrap();
        let mut net_addresses = NetAddressesWithStats::from(net_address1.clone());
        assert!(net_addresses.add_net_address(&net_address2).is_ok());
        assert!(net_addresses.add_net_address(&net_address3).is_ok());

        assert!(net_addresses.mark_successful_connection_attempt(&net_address3).is_ok());
        assert!(net_addresses.mark_successful_connection_attempt(&net_address1).is_ok());
        assert!(net_addresses.mark_successful_connection_attempt(&net_address2).is_ok());
        let desired_last_seen = net_addresses.addresses[1].last_seen;
        let last_seen = net_addresses.last_seen();
        assert!(desired_last_seen.is_some());
        assert!(last_seen.is_some());
        assert_eq!(desired_last_seen.unwrap(), last_seen.unwrap());
    }

    #[test]
    fn test_add_net_address() {
        let net_address1 = "123.0.0.123:8000".parse::<NetAddress>().unwrap();
        let net_address2 = "125.1.54.254:7999".parse::<NetAddress>().unwrap();
        let net_address3 = "175.6.3.145:8000".parse::<NetAddress>().unwrap();
        let mut net_addresses = NetAddressesWithStats::from(net_address1.clone());
        assert!(net_addresses.add_net_address(&net_address2).is_ok());
        assert!(net_addresses.add_net_address(&net_address3).is_ok());
        assert!(net_addresses.add_net_address(&net_address2).is_err()); // Add duplicate address
        assert_eq!(net_addresses.addresses.len(), 3);
        assert_eq!(net_addresses.addresses[0].net_address, net_address1);
        assert_eq!(net_addresses.addresses[1].net_address, net_address2);
        assert_eq!(net_addresses.addresses[2].net_address, net_address3);
    }

    #[test]
    fn test_get_net_address() {
        let net_address1 = "123.0.0.123:8000".parse::<NetAddress>().unwrap();
        let net_address2 = "125.1.54.254:7999".parse::<NetAddress>().unwrap();
        let net_address3 = "175.6.3.145:8000".parse::<NetAddress>().unwrap();
        let mut net_addresses = NetAddressesWithStats::from(net_address1.clone());
        assert!(net_addresses.add_net_address(&net_address2).is_ok());
        assert!(net_addresses.add_net_address(&net_address3).is_ok());

        let mut priority_address = net_addresses.get_best_net_address();
        assert!(priority_address.is_ok());
        assert_eq!(priority_address.unwrap(), net_address1);

        assert!(net_addresses
            .update_latency(&net_address1, Duration::from_millis(250))
            .is_ok());
        assert!(net_addresses
            .update_latency(&net_address2, Duration::from_millis(50))
            .is_ok());
        assert!(net_addresses
            .update_latency(&net_address3, Duration::from_millis(100))
            .is_ok());
        priority_address = net_addresses.get_best_net_address();
        assert!(priority_address.is_ok());
        assert_eq!(priority_address.unwrap(), net_address2);

        assert!(net_addresses.mark_failed_connection_attempt(&net_address2).is_ok());
        priority_address = net_addresses.get_best_net_address();
        assert!(priority_address.is_ok());
        assert_eq!(priority_address.unwrap(), net_address3);

        for _i in 0..MAX_CONNECTION_ATTEMPTS {
            assert!(net_addresses.mark_failed_connection_attempt(&net_address1).is_ok());
            assert!(net_addresses.mark_failed_connection_attempt(&net_address2).is_ok());
            assert!(net_addresses.mark_failed_connection_attempt(&net_address3).is_ok());
        }
        assert!(net_addresses.get_best_net_address().is_err());
    }

    #[test]
    fn test_stats_updates_on_addresses() {
        let net_address1 = "123.0.0.123:8000".parse::<NetAddress>().unwrap();
        let net_address2 = "125.1.54.254:7999".parse::<NetAddress>().unwrap();
        let net_address3 = "175.6.3.145:8000".parse::<NetAddress>().unwrap();
        let mut addresses: Vec<NetAddressWithStats> = Vec::new();
        addresses.push(NetAddressWithStats::from(net_address1.clone()));
        addresses.push(NetAddressWithStats::from(net_address2.clone()));
        addresses.push(NetAddressWithStats::from(net_address3.clone()));
        let mut net_addresses = NetAddressesWithStats::new(addresses);

        assert!(net_addresses
            .update_latency(&net_address2, Duration::from_millis(200))
            .is_ok());
        assert_eq!(net_addresses.addresses[0].avg_latency, Duration::from_millis(0));
        assert_eq!(net_addresses.addresses[1].avg_latency, Duration::from_millis(200));
        assert_eq!(net_addresses.addresses[2].avg_latency, Duration::from_millis(0));

        thread::sleep(Duration::from_millis(1));
        assert!(net_addresses.mark_message_received(&net_address1).is_ok());
        assert!(net_addresses.addresses[0].last_seen.is_some());
        assert!(net_addresses.addresses[1].last_seen.is_some());
        assert!(net_addresses.addresses[2].last_seen.is_none());
        assert!(net_addresses.addresses[0].last_seen.unwrap() > net_addresses.addresses[1].last_seen.unwrap());

        assert!(net_addresses.mark_message_rejected(&net_address2).is_ok());
        assert!(net_addresses.mark_message_rejected(&net_address3).is_ok());
        assert!(net_addresses.mark_message_rejected(&net_address3).is_ok());
        assert_eq!(net_addresses.addresses[0].rejected_message_count, 0);
        assert_eq!(net_addresses.addresses[1].rejected_message_count, 1);
        assert_eq!(net_addresses.addresses[2].rejected_message_count, 2);

        assert!(net_addresses.mark_failed_connection_attempt(&net_address1).is_ok());
        assert!(net_addresses.mark_failed_connection_attempt(&net_address2).is_ok());
        assert!(net_addresses.mark_failed_connection_attempt(&net_address3).is_ok());
        assert!(net_addresses.mark_failed_connection_attempt(&net_address1).is_ok());
        assert!(net_addresses.mark_successful_connection_attempt(&net_address2).is_ok());
        assert_eq!(net_addresses.addresses[0].connection_attempts, 2);
        assert_eq!(net_addresses.addresses[1].connection_attempts, 0);
        assert_eq!(net_addresses.addresses[2].connection_attempts, 1);
    }

    #[test]
    fn test_resetting_all_connection_attempts() {
        let net_address1 = "123.0.0.123:8000".parse::<NetAddress>().unwrap();
        let net_address2 = "125.1.54.254:7999".parse::<NetAddress>().unwrap();
        let net_address3 = "175.6.3.145:8000".parse::<NetAddress>().unwrap();
        let mut addresses: Vec<NetAddressWithStats> = Vec::new();
        addresses.push(NetAddressWithStats::from(net_address1.clone()));
        addresses.push(NetAddressWithStats::from(net_address2.clone()));
        addresses.push(NetAddressWithStats::from(net_address3.clone()));
        let mut net_addresses = NetAddressesWithStats::new(addresses);
        assert!(net_addresses.mark_failed_connection_attempt(&net_address1).is_ok());
        assert!(net_addresses.mark_failed_connection_attempt(&net_address2).is_ok());
        assert!(net_addresses.mark_failed_connection_attempt(&net_address3).is_ok());
        assert!(net_addresses.mark_failed_connection_attempt(&net_address1).is_ok());

        assert_eq!(net_addresses.addresses[0].connection_attempts, 2);
        assert_eq!(net_addresses.addresses[1].connection_attempts, 1);
        assert_eq!(net_addresses.addresses[2].connection_attempts, 1);
        net_addresses.reset_connection_attempts();
        assert_eq!(net_addresses.addresses[0].connection_attempts, 0);
        assert_eq!(net_addresses.addresses[1].connection_attempts, 0);
        assert_eq!(net_addresses.addresses[2].connection_attempts, 0);
    }

}

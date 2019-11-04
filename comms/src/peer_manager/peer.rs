//  Copyright 2019 The Tari Project
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

use super::node_id::deserialize_node_id_from_hex;
use crate::{
    connection::{net_address::net_addresses::NetAddressesWithStats, NetAddress},
    peer_manager::{node_id::NodeId, PeerFeatures},
    types::CommsPublicKey,
};
use bitflags::bitflags;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use tari_utilities::hex::serialize_to_hex;

bitflags! {
    #[derive(Default, Deserialize, Serialize)]
    pub struct PeerFlags: u8 {
        const BANNED = 0b0000_0001;
    }
}

/// Peer connection statistics
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialOrd, PartialEq)]
pub struct PeerConnectionStats {
    /// The last time that a successful connection was made, None if this has never happened
    pub connected_at: Option<NaiveDateTime>,
    /// The time that the last connection attempt failed. If the last connection attempt succeeded
    /// this is None
    pub last_connect_failed_at: Option<NaiveDateTime>,
}

/// A Peer represents a communication peer that is identified by a Public Key and NodeId. The Peer struct maintains a
/// collection of the NetAddressesWithStats that this Peer can be reached by. The struct also maintains a set of flags
/// describing the status of the Peer.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct Peer {
    pub public_key: CommsPublicKey,
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    pub node_id: NodeId,
    pub addresses: NetAddressesWithStats,
    pub flags: PeerFlags,
    pub features: PeerFeatures,
    pub connection_stats: PeerConnectionStats,
}

impl Peer {
    /// Constructs a new peer
    pub fn new(
        public_key: CommsPublicKey,
        node_id: NodeId,
        addresses: NetAddressesWithStats,
        flags: PeerFlags,
        features: PeerFeatures,
    ) -> Peer
    {
        Peer {
            public_key,
            node_id,
            addresses,
            flags,
            features,
            connection_stats: Default::default(),
        }
    }

    pub fn update(
        &mut self,
        node_id: Option<NodeId>,
        net_addresses: Option<Vec<NetAddress>>,
        flags: Option<PeerFlags>,
        features: Option<PeerFeatures>,
        connection_stats: Option<PeerConnectionStats>,
    )
    {
        if let Some(new_node_id) = node_id {
            self.node_id = new_node_id
        };
        if let Some(new_net_addresses) = net_addresses {
            self.addresses.update_net_addresses(new_net_addresses)
        };
        if let Some(new_flags) = flags {
            self.flags = new_flags
        };
        if let Some(new_features) = features {
            self.features = new_features;
        }
        if let Some(connection_stats) = connection_stats {
            self.connection_stats = connection_stats;
        }
    }

    /// Provides that date time of the last successful interaction with the peer
    pub fn last_seen(&self) -> Option<DateTime<Utc>> {
        self.addresses.last_seen()
    }

    /// Returns true if this peer has the given feature, otherwise false
    pub fn has_features(&self, features: PeerFeatures) -> bool {
        self.features.contains(features)
    }

    /// Returns the ban status of the peer
    pub fn is_banned(&self) -> bool {
        self.flags.contains(PeerFlags::BANNED)
    }

    /// Changes the ban flag bit of the peer
    pub fn set_banned(&mut self, ban_flag: bool) {
        self.flags.set(PeerFlags::BANNED, ban_flag);
    }

    pub fn set_connection_stats(&mut self, connection_stats: PeerConnectionStats) {
        self.connection_stats = connection_stats;
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::{net_address::net_addresses::NetAddressesWithStats, NetAddress},
        peer_manager::node_id::NodeId,
        types::CommsPublicKey,
    };
    use serde_json::Value;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_utilities::{hex::Hex, message_format::MessageFormat};

    #[test]
    fn test_is_and_set_banned() {
        let mut rng = rand::OsRng::new().unwrap();
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let addresses = NetAddressesWithStats::from("123.0.0.123:8000".parse::<NetAddress>().unwrap());
        let mut peer: Peer = Peer::new(pk, node_id, addresses, PeerFlags::default(), PeerFeatures::empty());
        assert_eq!(peer.is_banned(), false);
        peer.set_banned(true);
        assert_eq!(peer.is_banned(), true);
        peer.set_banned(false);
        assert_eq!(peer.is_banned(), false);
    }

    #[test]
    fn test_update() {
        let mut rng = rand::OsRng::new().unwrap();
        let (_sk, public_key1) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&public_key1).unwrap();
        let net_address1 = "124.0.0.124:7000".parse::<NetAddress>().unwrap();
        let mut peer: Peer = Peer::new(
            public_key1.clone(),
            node_id,
            NetAddressesWithStats::from(net_address1.clone()),
            PeerFlags::default(),
            PeerFeatures::empty(),
        );

        let (_sk, public_key2) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id2 = NodeId::from_key(&public_key2).unwrap();
        let net_address2 = "125.0.0.125:8000".parse::<NetAddress>().unwrap();
        let net_address3 = "126.0.0.126:9000".parse::<NetAddress>().unwrap();

        peer.update(
            Some(node_id2.clone()),
            Some(vec![net_address2.clone(), net_address3.clone()]),
            Some(PeerFlags::BANNED),
            Some(PeerFeatures::MESSAGE_PROPAGATION),
            Some(PeerConnectionStats {
                last_connect_failed_at: None,
                connected_at: Some(Utc::now().naive_utc()),
            }),
        );

        assert_eq!(peer.public_key, public_key1);
        assert_eq!(peer.node_id, node_id2);
        assert!(!peer
            .addresses
            .addresses
            .iter()
            .any(|net_address_with_stats| net_address_with_stats.net_address == net_address1));
        assert!(peer
            .addresses
            .addresses
            .iter()
            .any(|net_address_with_stats| net_address_with_stats.net_address == net_address2));
        assert!(peer
            .addresses
            .addresses
            .iter()
            .any(|net_address_with_stats| net_address_with_stats.net_address == net_address3));
        assert_eq!(peer.flags, PeerFlags::BANNED);
        assert_eq!(peer.has_features(PeerFeatures::MESSAGE_PROPAGATION), true);
    }

    #[test]
    fn json_ser_der() {
        let expected_pk_hex = "02622ace8f7303a31cafc63f8fc48fdc16e1c8c8d234b2f0d6685282a9076031";
        let expected_nodeid_hex = "5f517508fdaeef0aeae7b577336731dfb6fe60bbbde363a5712100109b5d0f69";
        let pk = CommsPublicKey::from_hex(expected_pk_hex).unwrap();
        let node_id = NodeId::from_key(&pk).unwrap();
        let peer = Peer::new(
            pk,
            node_id,
            "127.0.0.1:9000".parse::<NetAddress>().unwrap().into(),
            PeerFlags::empty(),
            PeerFeatures::empty(),
        );

        let json = peer.to_json().unwrap();
        let json: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(json["public_key"], expected_pk_hex);
        assert_eq!(json["node_id"], expected_nodeid_hex);
    }
}

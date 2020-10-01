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

use super::{
    connection_stats::PeerConnectionStats,
    node_id::{deserialize_node_id_from_hex, NodeId},
    peer_id::PeerId,
    PeerFeatures,
};
use crate::{
    consts::PEER_OFFLINE_COOLDOWN_PERIOD,
    net_address::MultiaddressesWithStats,
    protocol::ProtocolId,
    types::CommsPublicKey,
    utils::datetime::safe_future_datetime_from_duration,
};
use bitflags::bitflags;
use chrono::{DateTime, NaiveDateTime, Utc};
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, time::Duration};
use tari_crypto::tari_utilities::hex::serialize_to_hex;

bitflags! {
    #[derive(Default, Deserialize, Serialize)]
    pub struct PeerFlags: u8 {
        const NONE = 0x00;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIdentity {
    pub node_id: NodeId,
    pub public_key: CommsPublicKey,
}

/// A Peer represents a communication peer that is identified by a Public Key and NodeId. The Peer struct maintains a
/// collection of the NetAddressesWithStats that this Peer can be reached by. The struct also maintains a set of flags
/// describing the status of the Peer.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Peer {
    /// The local id of the peer. If this is None, the peer has never been persisted
    pub(super) id: Option<PeerId>,
    /// Public key of the peer
    pub public_key: CommsPublicKey,
    /// NodeId of the peer
    #[serde(serialize_with = "serialize_to_hex")]
    #[serde(deserialize_with = "deserialize_node_id_from_hex")]
    pub node_id: NodeId,
    /// Peer's addresses
    pub addresses: MultiaddressesWithStats,
    /// Flags for the peer.
    pub flags: PeerFlags,
    pub banned_until: Option<NaiveDateTime>,
    pub banned_reason: String,
    pub offline_at: Option<NaiveDateTime>,
    /// Features supported by the peer
    pub features: PeerFeatures,
    /// Connection statics for the peer
    pub connection_stats: PeerConnectionStats,
    /// Protocols supported by the peer. This should not be considered a definitive list of supported protocols and is
    /// used as information for more efficient protocol negotiation.
    pub supported_protocols: Vec<ProtocolId>,
    /// Timestamp of when the peer was added to this nodes peer list
    pub added_at: NaiveDateTime,
    /// User agent advertised by the peer
    pub user_agent: String,
}

impl Peer {
    /// Constructs a new peer.
    pub fn new<'p, P: IntoIterator<Item = &'p ProtocolId>>(
        public_key: CommsPublicKey,
        node_id: NodeId,
        addresses: MultiaddressesWithStats,
        flags: PeerFlags,
        features: PeerFeatures,
        supported_protocols: P,
        user_agent: String,
    ) -> Peer
    {
        Peer {
            id: None,
            public_key,
            node_id,
            addresses,
            flags,
            features,
            banned_until: None,
            banned_reason: "".to_string(),
            offline_at: None,
            connection_stats: Default::default(),
            added_at: Utc::now().naive_utc(),
            supported_protocols: supported_protocols.into_iter().cloned().collect(),
            user_agent,
        }
    }

    /// Returns the peers local id if this peer is persisted.
    ///
    /// This method panics if the peer does not have a PeerId, and therefore is not persisted.
    /// If the caller should be sure that the peer is persisted before calling this function.
    /// This can be checked by using `Peer::is_persisted`.
    #[inline]
    pub fn id(&self) -> PeerId {
        self.id.expect("call to Peer::id() when peer is not persisted")
    }

    pub fn is_persisted(&self) -> bool {
        self.id.is_some()
    }

    /// Returns a slice of Protocols _known to be_ supported by the peer. This should not be considered a definitive
    /// list of supported protocols
    pub fn supported_protocols(&self) -> &[ProtocolId] {
        &self.supported_protocols
    }

    /// Returns true if the last connection attempt has failed within the constant
    /// [PEER_OFFLINE_COOLDOWN_PERIOD](crate::consts::PEER_OFFLINE_COOLDOWN_PERIOD).
    pub fn is_recently_offline(&self) -> bool {
        self.connection_stats.failed_attempts() > 1 &&
            self.connection_stats
                .time_since_last_failure()
                .map(|last_failure| last_failure <= PEER_OFFLINE_COOLDOWN_PERIOD)
                .unwrap_or(false)
    }

    /// Returns true if the peer is marked as offline
    pub fn is_offline(&self) -> bool {
        self.offline_at.is_some()
    }

    /// TODO: Remove once we don't have to sync wallet and base node db
    pub fn unset_id(&mut self) {
        self.id = None;
    }

    pub(super) fn set_id(&mut self, id: PeerId) {
        self.id = Some(id);
    }

    #[cfg(test)]
    pub(crate) fn set_id_for_test(&mut self, id: PeerId) {
        self.id = Some(id);
    }

    #[allow(clippy::option_option)]
    pub fn update(
        &mut self,
        net_addresses: Option<Vec<Multiaddr>>,
        flags: Option<PeerFlags>,
        banned_until: Option<Option<Duration>>,
        banned_reason: Option<String>,
        is_offline: Option<bool>,
        features: Option<PeerFeatures>,
        supported_protocols: Option<Vec<ProtocolId>>,
    )
    {
        if let Some(new_net_addresses) = net_addresses {
            self.addresses.update_net_addresses(new_net_addresses)
        }
        if let Some(new_flags) = flags {
            self.flags = new_flags
        }
        if let Some(banned_until) = banned_until {
            self.banned_until = banned_until
                .map(safe_future_datetime_from_duration)
                .map(|dt| dt.naive_utc());
        }
        if let Some(banned_reason) = banned_reason {
            self.banned_reason = banned_reason;
        }
        if let Some(is_offline) = is_offline {
            self.set_offline(is_offline);
        }
        if let Some(new_features) = features {
            self.features = new_features;
        }
        if let Some(supported_protocols) = supported_protocols {
            self.supported_protocols = supported_protocols;
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
        self.banned_until().is_some()
    }

    /// Bans the peer for a specified duration
    pub fn ban_for(&mut self, duration: Duration, reason: String) {
        let dt = safe_future_datetime_from_duration(duration);
        self.banned_until = Some(dt.naive_utc());
        self.banned_reason = reason;
    }

    /// Unban the peer
    pub fn unban(&mut self) {
        self.banned_until = None;
        self.banned_reason = "".to_string();
    }

    pub fn banned_until(&self) -> Option<&NaiveDateTime> {
        self.banned_until.as_ref().filter(|dt| *dt > &Utc::now().naive_utc())
    }

    /// Marks the peer as offline
    pub fn set_offline(&mut self, is_offline: bool) {
        if is_offline {
            self.offline_at = Some(Utc::now().naive_utc());
        } else {
            self.offline_at = None;
        }
    }
}

/// Display Peer as `[peer_id]: <pubkey>`
impl Display for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let flags_str = if self.flags == PeerFlags::empty() {
            "".to_string()
        } else {
            format!("{:?}", self.flags)
        };

        let status_str = {
            let mut s = Vec::new();
            if let Some(offline_at) = self.offline_at.as_ref() {
                s.push(format!("OFFLINE since {}", offline_at));
            }

            if let Some(dt) = self.banned_until() {
                s.push(format!("BANNED until {}", dt));
                s.push(format!("Banned because: {}", self.banned_reason))
            }
            s.join(", ")
        };
        f.write_str(&format!(
            "{}[{}] PK={} ({}) {} {:?} {}",
            flags_str,
            self.node_id.short_str(),
            self.public_key,
            self.addresses
                .addresses
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
            status_str,
            match self.features {
                PeerFeatures::COMMUNICATION_NODE => "BASE_NODE".to_string(),
                PeerFeatures::COMMUNICATION_CLIENT => "WALLET".to_string(),
                f => format!("{:?}", f),
            },
            self.connection_stats,
        ))
    }
}

impl PartialEq for Peer {
    fn eq(&self, other: &Self) -> bool {
        self.public_key == other.public_key
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{net_address::MultiaddressesWithStats, peer_manager::NodeId, protocol, types::CommsPublicKey};
    use serde_json::Value;
    use tari_crypto::{
        keys::PublicKey,
        ristretto::RistrettoPublicKey,
        tari_utilities::{hex::Hex, message_format::MessageFormat},
    };

    #[test]
    fn test_is_banned_and_ban_for() {
        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let addresses = MultiaddressesWithStats::from("/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap());
        let mut peer: Peer = Peer::new(
            pk,
            node_id,
            addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            &[],
            Default::default(),
        );
        assert_eq!(peer.is_banned(), false);
        peer.ban_for(Duration::from_millis(std::u64::MAX));
        assert_eq!(peer.is_banned(), true);
        peer.ban_for(Duration::from_millis(0));
        assert_eq!(peer.is_banned(), false);
    }

    #[test]
    fn test_update() {
        let mut rng = rand::rngs::OsRng;
        let (_sk, public_key1) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&public_key1).unwrap();
        let net_address1 = "/ip4/124.0.0.124/tcp/7000".parse::<Multiaddr>().unwrap();
        let mut peer: Peer = Peer::new(
            public_key1.clone(),
            node_id.clone(),
            MultiaddressesWithStats::from(net_address1.clone()),
            PeerFlags::default(),
            PeerFeatures::empty(),
            &[],
            Default::default(),
        );

        let net_address2 = "/ip4/125.0.0.125/tcp/8000".parse::<Multiaddr>().unwrap();
        let net_address3 = "/ip4/126.0.0.126/tcp/9000".parse::<Multiaddr>().unwrap();

        peer.update(
            Some(vec![net_address2.clone(), net_address3.clone()]),
            None,
            Some(Some(Duration::from_secs(1000))),
            None,
            Some(PeerFeatures::MESSAGE_PROPAGATION),
            Some(vec![protocol::IDENTITY_PROTOCOL.clone()]),
        );

        assert_eq!(peer.public_key, public_key1);
        assert_eq!(peer.node_id, node_id);
        assert!(!peer
            .addresses
            .addresses
            .iter()
            .any(|net_address_with_stats| net_address_with_stats.address == net_address1));
        assert!(peer
            .addresses
            .addresses
            .iter()
            .any(|net_address_with_stats| net_address_with_stats.address == net_address2));
        assert!(peer
            .addresses
            .addresses
            .iter()
            .any(|net_address_with_stats| net_address_with_stats.address == net_address3));
        assert!(peer.is_banned());
        assert_eq!(peer.has_features(PeerFeatures::MESSAGE_PROPAGATION), true);
        assert_eq!(peer.supported_protocols, vec![protocol::IDENTITY_PROTOCOL.clone()]);
    }

    #[test]
    fn json_ser_der() {
        let expected_pk_hex = "02622ace8f7303a31cafc63f8fc48fdc16e1c8c8d234b2f0d6685282a9076031";
        let expected_nodeid_hex = "c1a7552e5d9e9b257c4008b965";
        let pk = CommsPublicKey::from_hex(expected_pk_hex).unwrap();
        let node_id = NodeId::from_key(&pk).unwrap();
        let peer = Peer::new(
            pk,
            node_id,
            "/ip4/127.0.0.1/tcp/9000".parse::<Multiaddr>().unwrap().into(),
            PeerFlags::empty(),
            PeerFeatures::empty(),
            &[],
            Default::default(),
        );

        let json = peer.to_json().unwrap();
        let json: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(json["public_key"], expected_pk_hex);
        assert_eq!(json["node_id"], expected_nodeid_hex);
    }
}

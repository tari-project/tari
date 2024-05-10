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

use std::{
    cmp,
    collections::HashMap,
    convert::TryFrom,
    fmt::Display,
    hash::{Hash, Hasher},
    time::Duration,
};

use bitflags::bitflags;
use chrono::{NaiveDateTime, Utc};
use multiaddr::Multiaddr;
use serde::{Deserialize, Serialize};
use tari_utilities::hex::serialize_to_hex;

use super::{
    node_id::{deserialize_node_id_from_hex, NodeId},
    peer_id::PeerId,
    PeerFeatures,
};
use crate::{
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    protocol::ProtocolId,
    types::CommsPublicKey,
    utils::datetime::{format_local_datetime, is_max_datetime, safe_future_datetime_from_duration},
};

bitflags! {
    /// Miscellaneous Peer flags
    #[derive(Default, Deserialize, Serialize, Eq, PartialEq, Debug, Clone, Copy)]
    pub struct PeerFlags: u8 {
        const NONE = 0x00;
        const SEED = 0x01;
    }
}

/// A Peer represents a communication peer that is identified by a Public Key and NodeId. The Peer struct maintains a
/// collection of the NetAddressesWithStats that this Peer can be reached by. The struct also maintains a set of flags
/// describing the status of the Peer.
#[derive(Debug, Clone, Deserialize, Serialize, Eq)]
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
    /// Features supported by the peer
    pub features: PeerFeatures,
    /// Protocols supported by the peer. This should not be considered a definitive list of supported protocols and is
    /// used as information for more efficient protocol negotiation.
    pub supported_protocols: Vec<ProtocolId>,
    /// Timestamp of when the peer was added to this nodes peer list
    pub added_at: NaiveDateTime,
    /// User agent advertised by the peer
    pub user_agent: String,
    /// Metadata field. This field is for use by upstream clients to record extra info about a peer.
    /// We use a hashmap here so that we can use more than one "info set"
    pub metadata: HashMap<u8, Vec<u8>>,
    /// If this peer has been deleted.
    pub deleted_at: Option<NaiveDateTime>,
}

impl Peer {
    /// Constructs a new peer.
    pub fn new(
        public_key: CommsPublicKey,
        node_id: NodeId,
        addresses: MultiaddressesWithStats,
        flags: PeerFlags,
        features: PeerFeatures,
        supported_protocols: Vec<ProtocolId>,
        user_agent: String,
    ) -> Peer {
        Peer {
            id: None,
            public_key,
            node_id,
            addresses,
            flags,
            features,
            banned_until: None,
            banned_reason: String::new(),
            added_at: Utc::now().naive_utc(),
            supported_protocols,
            user_agent,
            metadata: HashMap::new(),
            deleted_at: None,
        }
    }

    /// Returns the peers local id if this peer is persisted.
    ///
    /// This method panics if the peer does not have a PeerId, and therefore is not persisted.
    /// If the caller should be sure that the peer is persisted before calling this function.
    /// This can be checked by using `Peer::is_persisted`.
    pub fn id(&self) -> PeerId {
        self.id.expect("call to Peer::id() when peer is not persisted")
    }

    /// Merges the data with another peer. This is usually used to update a peer before it is saved to the
    /// database so that data is not overwritten
    pub fn merge(&mut self, other: &Peer) {
        self.addresses.merge(&other.addresses);
        if !other.banned_reason.is_empty() {
            self.banned_reason = other.banned_reason.clone();
        }
        self.banned_until = cmp::max(self.banned_until, other.banned_until);
        self.added_at = cmp::min(self.added_at, other.added_at);
        for protocol in &other.supported_protocols {
            if !self.supported_protocols.contains(protocol) {
                self.supported_protocols.push(protocol.clone());
            }
        }
        self.metadata = other.metadata.clone();
        self.features = other.features;
        self.flags = other.flags;
        if !other.user_agent.is_empty() {
            self.user_agent = other.user_agent.clone();
        }
    }

    pub fn is_persisted(&self) -> bool {
        self.id.is_some()
    }

    /// Returns a slice of Protocols _known to be_ supported by the peer. This should not be considered a definitive
    /// list of supported protocols
    pub fn supported_protocols(&self) -> &[ProtocolId] {
        &self.supported_protocols
    }

    pub fn last_connect_attempt(&self) -> Option<NaiveDateTime> {
        self.addresses
            .addresses()
            .iter()
            .max_by_key(|a| a.last_attempted())
            .and_then(|a| a.last_attempted())
    }

    /// The last address used to connect to the peer
    pub fn last_address_used(&self) -> Option<Multiaddr> {
        self.addresses
            .addresses()
            .iter()
            .max_by_key(|a| a.last_attempted())
            .map(|a| a.address().clone())
    }

    /// Returns true if the peer is marked as offline
    pub fn is_offline(&self) -> bool {
        self.addresses.offline_at().is_some()
    }

    pub fn offline_at(&self) -> Option<NaiveDateTime> {
        self.addresses.offline_at()
    }

    /// The length of time since a peer was marked as offline
    pub fn offline_since(&self) -> Option<Duration> {
        let offline_at = self.addresses.offline_at();
        offline_at
            .map(|offline_at| Utc::now().naive_utc() - offline_at)
            .map(|since| Duration::from_secs(u64::try_from(since.num_seconds()).unwrap_or(0)))
    }

    pub(super) fn set_id(&mut self, id: PeerId) {
        self.id = Some(id);
    }

    #[cfg(test)]
    pub(crate) fn set_id_for_test(&mut self, id: PeerId) {
        self.id = Some(id);
    }

    /// Provides that date time of the last successful interaction with the peer
    pub fn last_seen(&self) -> Option<NaiveDateTime> {
        self.addresses.last_seen()
    }

    /// Provides info about the failure status of all addresses
    pub fn all_addresses_failed(&self) -> bool {
        self.addresses.iter().all(|a| a.last_failed_reason().is_some())
    }

    /// Provides that length of time since the last successful interaction with the peer
    pub fn last_seen_since(&self) -> Option<Duration> {
        self.last_seen()
            .and_then(|dt| Utc::now().naive_utc().signed_duration_since(dt).to_std().ok())
    }

    /// Returns the ban status of the peer
    pub fn is_banned(&self) -> bool {
        self.banned_until().is_some()
    }

    /// Returns the ban status of the peer
    pub fn reason_banned(&self) -> &str {
        &self.banned_reason
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

    /// This will store metadata inside of the metadata field in the peer.
    /// It will return None if the value was empty and the old value if the value was updated
    pub fn set_metadata(&mut self, key: u8, data: Vec<u8>) -> Option<Vec<u8>> {
        self.metadata.insert(key, data)
    }

    /// This will return the value in the metadata field. It will return None if the key is not present
    pub fn get_metadata(&self, key: u8) -> Option<&Vec<u8>> {
        self.metadata.get(&key)
    }

    /// Update the peer's addresses. This call will invalidate the identity signature.
    pub fn update_addresses(&mut self, addresses: &[Multiaddr], source: &PeerAddressSource) -> &mut Self {
        self.addresses.update_addresses(addresses, source);
        self
    }

    /// Update the peer's features. This call will invalidate the identity signature if the features differ.
    pub fn set_features(&mut self, features: PeerFeatures) -> &mut Self {
        if self.features != features {
            self.features = features;
        }
        self
    }

    pub fn add_flags(&mut self, flags: PeerFlags) -> &mut Self {
        self.flags |= flags;
        self
    }

    pub fn is_seed(&self) -> bool {
        self.flags.contains(PeerFlags::SEED)
    }

    pub fn to_short_string(&self) -> String {
        format!("{}::{}", self.public_key, self.addresses)
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
            if let Some(offline_at) = self.offline_at() {
                s.push(format!("Offline since: {}", format_local_datetime(&offline_at)));
            }

            if let Some(dt) = self.banned_until() {
                if is_max_datetime(dt) {
                    s.push("Banned permanently".to_string());
                } else {
                    s.push(format!("Banned until: {}", format_local_datetime(dt)));
                }
                s.push(format!("Reason: {}", self.banned_reason))
            }
            s.join(". ")
        };

        let user_agent = match self.user_agent.as_ref() {
            "" => "<unknown>",
            ua => ua,
        };

        f.write_str(&format!(
            "{}[{}] PK={} ({}) - {}. Type: {}. User agent: {}.",
            flags_str,
            self.node_id.short_str(),
            self.public_key,
            self.addresses,
            status_str,
            match self.features {
                PeerFeatures::COMMUNICATION_NODE => "BASE_NODE".to_string(),
                PeerFeatures::COMMUNICATION_CLIENT => "WALLET".to_string(),
                f => format!("{:?}", f),
            },
            user_agent,
        ))
    }
}

impl PartialEq for Peer {
    fn eq(&self, other: &Self) -> bool {
        self.public_key == other.public_key
    }
}

impl Hash for Peer {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.public_key.hash(state)
    }
}

#[cfg(test)]
mod test {
    use serde_json::Value;
    use tari_crypto::{
        keys::PublicKey,
        ristretto::RistrettoPublicKey,
        tari_utilities::{hex::Hex, message_format::MessageFormat},
    };

    use super::*;
    use crate::{net_address::MultiaddressesWithStats, peer_manager::NodeId, types::CommsPublicKey};

    #[test]
    fn test_is_banned_and_ban_for() {
        let mut rng = rand::rngs::OsRng;
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk);
        let addresses = MultiaddressesWithStats::from_addresses_with_source(
            vec!["/ip4/123.0.0.123/tcp/8000".parse::<Multiaddr>().unwrap()],
            &PeerAddressSource::Config,
        );
        let mut peer: Peer = Peer::new(
            pk,
            node_id,
            addresses,
            PeerFlags::default(),
            PeerFeatures::empty(),
            Default::default(),
            Default::default(),
        );
        assert!(!peer.is_banned());
        peer.ban_for(Duration::from_millis(std::u64::MAX), "Very long manual ban".to_string());
        assert_eq!(peer.reason_banned(), &"Very long manual ban".to_string());
        assert!(peer.is_banned());
        peer.ban_for(Duration::from_millis(0), "".to_string());
        assert!(!peer.is_banned());
    }

    #[test]
    fn json_ser_der() {
        let expected_pk_hex = "02622ace8f7303a31cafc63f8fc48fdc16e1c8c8d234b2f0d6685282a9076031";
        let expected_nodeid_hex = "c1a7552e5d9e9b257c4008b965";
        let pk = CommsPublicKey::from_hex(expected_pk_hex).unwrap();
        let node_id = NodeId::from_key(&pk);
        let peer = Peer::new(
            pk,
            node_id,
            MultiaddressesWithStats::from_addresses_with_source(
                vec!["/ip4/127.0.0.1/tcp/9000".parse::<Multiaddr>().unwrap()],
                &PeerAddressSource::Config,
            ),
            PeerFlags::empty(),
            PeerFeatures::empty(),
            Default::default(),
            Default::default(),
        );

        let json = peer.to_json().unwrap();
        let json: Value = serde_json::from_str(&json).unwrap();

        assert_eq!(json["public_key"], expected_pk_hex);
        assert_eq!(json["node_id"], expected_nodeid_hex);
    }
}

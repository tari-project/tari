// Copyright 2019, The Tari Project
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
    convert::{TryFrom, TryInto},
    fmt,
};

use anyhow::anyhow;
use chrono::{DateTime, Utc};
use rand::Rng;
use tari_comms::{
    NodeIdentity,
    multiaddr::Multiaddr,
    peer_manager::{IdentitySignature, PeerFeatures, PeerIdentityClaim},
    types::{CommsPublicKey, CommsSecretKey, CompressedSignature},
};
use tari_utilities::{ByteArray, hex::Hex};

use crate::{proto::dht::JoinMessage, rpc::UnvalidatedPeerInfo};

/// The decode budget (see `tari_comms::decode_budget`) for a DHT envelope body and for the join and discovery messages
/// it carries. Their only lists are the envelope's parts (two for every message this node sends) and the claimed peer
/// addresses (at most `max_permitted_peer_addresses_per_claim`, 5 by default), so this is far above anything
/// legitimate. Without it, one 8 MiB messaging frame of empty parts or addresses decodes into ~4M `Vec`s.
pub const DHT_MESSAGE_MAX_DECODE_ITEMS: usize = 1_024;

#[allow(clippy::all, clippy::pedantic)]
pub mod common {
    tari_comms::outdir_include!("tari.dht.common.rs");
}

#[allow(clippy::all, clippy::pedantic)]
pub mod envelope {
    tari_comms::outdir_include!("tari.dht.envelope.rs");
}

#[allow(clippy::all, clippy::pedantic)]
pub mod dht {
    use super::common;
    tari_comms::outdir_include!("tari.dht.rs");
}

#[allow(clippy::all, clippy::pedantic)]
pub mod rpc {
    tari_comms::outdir_include!("tari.dht.rpc.rs");
}

#[allow(clippy::all, clippy::pedantic)]
pub mod message_header {
    tari_comms::outdir_include!("tari.dht.message_header.rs");
}

//---------------------------------- JoinMessage --------------------------------------------//

impl<T: AsRef<NodeIdentity>> From<T> for JoinMessage {
    fn from(identity: T) -> Self {
        let node_identity = identity.as_ref();
        Self {
            public_key: node_identity.public_key().to_vec(),
            addresses: node_identity.public_addresses().iter().map(|a| a.to_vec()).collect(),
            peer_features: node_identity.features().bits(),
            nonce: rand::rng().next_u64(),
            identity_signature: node_identity.identity_signature_read().as_ref().map(Into::into),
        }
    }
}

impl fmt::Display for dht::JoinMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "JoinMessage(PK = {}, {} Addresses, Features = {:?})",
            self.public_key.to_hex(),
            self.addresses.len(),
            PeerFeatures::from_bits(self.peer_features),
        )
    }
}

//---------------------------------- Rpc Message Conversions --------------------------------------------//

impl From<UnvalidatedPeerInfo> for rpc::PeerInfo {
    fn from(value: UnvalidatedPeerInfo) -> Self {
        Self {
            public_key: value.public_key.to_vec(),
            claims: value.claims.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<PeerIdentityClaim> for rpc::PeerIdentityClaim {
    fn from(value: PeerIdentityClaim) -> Self {
        Self {
            addresses: value.addresses.iter().map(|a| a.to_vec()).collect(),
            peer_features: value.features.bits(),
            identity_signature: Some((&value.signature).into()),
        }
    }
}

impl TryFrom<rpc::PeerInfo> for UnvalidatedPeerInfo {
    type Error = anyhow::Error;

    fn try_from(value: rpc::PeerInfo) -> Result<UnvalidatedPeerInfo, Self::Error> {
        let public_key = CommsPublicKey::from_canonical_bytes(&value.public_key)
            .map_err(|e| anyhow!("PeerInfo invalid public key: {e}"))?;
        let claims = value
            .claims
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self { public_key, claims })
    }
}

impl TryFrom<rpc::PeerIdentityClaim> for PeerIdentityClaim {
    type Error = anyhow::Error;

    fn try_from(value: rpc::PeerIdentityClaim) -> Result<PeerIdentityClaim, Self::Error> {
        let addresses = value
            .addresses
            .into_iter()
            .filter_map(|addr| Multiaddr::try_from(addr).ok())
            .collect::<Vec<_>>();

        let features = PeerFeatures::from_bits(value.peer_features).ok_or_else(|| anyhow!("Invalid peer features"))?;
        let signature = value
            .identity_signature
            .map(TryInto::try_into)
            .ok_or_else(|| anyhow::anyhow!("No signature"))??;
        Ok(PeerIdentityClaim {
            addresses,
            features,
            signature,
        })
    }
}

impl TryFrom<common::IdentitySignature> for IdentitySignature {
    type Error = anyhow::Error;

    fn try_from(value: common::IdentitySignature) -> Result<Self, Self::Error> {
        let version = u8::try_from(value.version)
            .map_err(|_| anyhow::anyhow!("Invalid peer identity signature version {}", value.version))?;
        let public_nonce = CommsPublicKey::from_canonical_bytes(&value.public_nonce)
            .map_err(|e| anyhow!("Invalid public nonce: {e}"))?;
        let signature =
            CommsSecretKey::from_canonical_bytes(&value.signature).map_err(|e| anyhow!("Invalid signature: {e}"))?;
        let updated_at = DateTime::<Utc>::from_timestamp(value.updated_at, 0)
            .ok_or_else(|| anyhow::anyhow!("updated_at overflowed"))?;

        Ok(Self::new(
            version,
            CompressedSignature::new(public_nonce, signature),
            updated_at,
        ))
    }
}

impl From<&IdentitySignature> for common::IdentitySignature {
    fn from(identity_sig: &IdentitySignature) -> Self {
        common::IdentitySignature {
            version: u32::from(identity_sig.version()),
            signature: identity_sig.signature().get_signature().to_vec(),
            public_nonce: identity_sig.signature().get_compressed_public_nonce().to_vec(),
            updated_at: identity_sig.updated_at().timestamp(),
        }
    }
}

#[cfg(test)]
mod decode_budget_test {
    use tari_comms::{message::MessageError, wrap_in_envelope_body};

    use super::{
        DHT_MESSAGE_MAX_DECODE_ITEMS,
        dht::{DiscoveryMessage, DiscoveryResponseMessage, JoinMessage},
    };

    fn is_decode_budget_error(err: &MessageError) -> bool {
        err.to_string().contains("decode budget")
    }

    /// Join and discovery messages claiming more addresses than the decode budget allows are rejected before prost
    /// decodes them; ones at the configured address limit (5 by default) and at the budget decode.
    #[test]
    fn join_and_discovery_messages_over_the_decode_budget_are_rejected() {
        let addresses = |count: usize| vec![b"/ip4/127.0.0.1/tcp/18189".to_vec(); count];

        for count in [5, DHT_MESSAGE_MAX_DECODE_ITEMS] {
            let body = wrap_in_envelope_body!(JoinMessage {
                addresses: addresses(count),
                ..Default::default()
            });
            let join = body
                .decode_part_with_max_items::<JoinMessage>(0, DHT_MESSAGE_MAX_DECODE_ITEMS)
                .unwrap()
                .unwrap();
            assert_eq!(join.addresses.len(), count);
        }

        let flood = vec![Vec::new(); DHT_MESSAGE_MAX_DECODE_ITEMS + 1];
        let join = wrap_in_envelope_body!(JoinMessage {
            addresses: flood.clone(),
            ..Default::default()
        });
        let discovery = wrap_in_envelope_body!(DiscoveryMessage {
            addresses: flood.clone(),
            ..Default::default()
        });
        let discovery_response = wrap_in_envelope_body!(DiscoveryResponseMessage {
            addresses: flood,
            ..Default::default()
        });

        // Prost alone accepts them
        assert!(<JoinMessage as prost::Message>::decode(join.parts.first().unwrap().as_slice()).is_ok());

        let err = join
            .decode_part_with_max_items::<JoinMessage>(0, DHT_MESSAGE_MAX_DECODE_ITEMS)
            .unwrap_err();
        assert!(is_decode_budget_error(&err), "unexpected error: {err}");
        let err = discovery
            .decode_part_with_max_items::<DiscoveryMessage>(0, DHT_MESSAGE_MAX_DECODE_ITEMS)
            .unwrap_err();
        assert!(is_decode_budget_error(&err), "unexpected error: {err}");
        let err = discovery_response
            .decode_part_with_max_items::<DiscoveryResponseMessage>(0, DHT_MESSAGE_MAX_DECODE_ITEMS)
            .unwrap_err();
        assert!(is_decode_budget_error(&err), "unexpected error: {err}");
    }
}

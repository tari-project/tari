//  Copyright 2022. The Tari Project
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

use std::convert::{TryFrom, TryInto};

use anyhow::anyhow;
use tari_comms::{
    multiaddr::Multiaddr,
    peer_manager::{Peer, PeerFeatures, PeerIdentityClaim},
    peer_validator::validate_address_shape,
    types::CommsPublicKey,
};
use tari_utilities::ByteArray;

use crate::proto::dht::{DiscoveryMessage, DiscoveryResponseMessage, JoinMessage};

#[derive(Debug)]
pub struct UnvalidatedPeerInfo {
    pub public_key: CommsPublicKey,
    pub claims: Vec<PeerIdentityClaim>,
}

impl UnvalidatedPeerInfo {
    pub fn from_peer_limited_claims(peer: Peer, max_claims: usize, max_addresses_per_claim: usize) -> Self {
        let claims = peer
            .addresses
            .addresses()
            .iter()
            .filter_map(|addr| {
                if addr.address().is_empty() {
                    return None;
                }

                let claim = addr.source().peer_identity_claim()?;

                if claim.addresses.len() > max_addresses_per_claim {
                    return None;
                }

                // A stored claim may hold an address that we filtered out when we accepted it, or that an older
                // version stored before addresses were checked. The claim is signed as a whole, so it cannot be
                // trimmed: it is not relayed at all, as a receiving node may drop it or count it against us.
                if claim
                    .addresses
                    .iter()
                    .any(|address| validate_address_shape(address).is_err())
                {
                    return None;
                }

                Some(claim)
            })
            .take(max_claims)
            .cloned()
            .collect::<Vec<_>>();

        Self {
            public_key: peer.public_key,
            claims,
        }
    }
}

impl TryFrom<DiscoveryMessage> for UnvalidatedPeerInfo {
    type Error = anyhow::Error;

    fn try_from(value: DiscoveryMessage) -> Result<Self, Self::Error> {
        let public_key = CommsPublicKey::from_canonical_bytes(&value.public_key)
            .map_err(|e| anyhow!("DiscoveryMessage invalid public key: {e}"))?;

        let features = PeerFeatures::from_bits(value.peer_features)
            .ok_or_else(|| anyhow!("Invalid peer features. Bits: {:#04x}", value.peer_features))?;

        let identity_signature = value
            .identity_signature
            .ok_or_else(|| anyhow!("DiscoveryMessage missing peer_identity_claim"))?
            .try_into()?;
        let identity_claim = PeerIdentityClaim {
            addresses: value
                .addresses
                .into_iter()
                .map(Multiaddr::try_from)
                .collect::<Result<_, _>>()?,
            features,
            signature: identity_signature,
        };

        Ok(Self {
            public_key,
            claims: vec![identity_claim],
        })
    }
}

impl TryFrom<DiscoveryResponseMessage> for UnvalidatedPeerInfo {
    type Error = anyhow::Error;

    fn try_from(value: DiscoveryResponseMessage) -> Result<Self, Self::Error> {
        let public_key = CommsPublicKey::from_canonical_bytes(&value.public_key)
            .map_err(|e| anyhow!("DiscoveryMessage invalid public key: {e}"))?;

        let features = PeerFeatures::from_bits(value.peer_features)
            .ok_or_else(|| anyhow!("Invalid peer features. Bits: {:#04x}", value.peer_features))?;

        let identity_signature = value
            .identity_signature
            .ok_or_else(|| anyhow!("DiscoveryMessage missing peer_identity_claim"))?
            .try_into()?;

        let identity_claim = PeerIdentityClaim {
            addresses: value
                .addresses
                .into_iter()
                .map(Multiaddr::try_from)
                .collect::<Result<_, _>>()?,
            features,
            signature: identity_signature,
        };

        Ok(Self {
            public_key,
            claims: vec![identity_claim],
        })
    }
}

impl TryFrom<JoinMessage> for UnvalidatedPeerInfo {
    type Error = anyhow::Error;

    fn try_from(value: JoinMessage) -> Result<Self, Self::Error> {
        let public_key = CommsPublicKey::from_canonical_bytes(&value.public_key)
            .map_err(|e| anyhow!("JoinMessage invalid public key: {e}"))?;

        let features = PeerFeatures::from_bits(value.peer_features)
            .ok_or_else(|| anyhow!("Invalid peer features. Bits: {:#04x}", value.peer_features))?;

        let identity_signature = value
            .identity_signature
            .ok_or_else(|| anyhow!("JoinMessage missing peer_identity_claim"))?
            .try_into()?;

        let identity_claim = PeerIdentityClaim {
            addresses: value
                .addresses
                .into_iter()
                .map(Multiaddr::try_from)
                .collect::<Result<_, _>>()?,
            features,
            signature: identity_signature,
        };

        Ok(Self {
            public_key,
            claims: vec![identity_claim],
        })
    }
}

#[cfg(test)]
mod test {
    #![allow(clippy::indexing_slicing)]
    use tari_comms::{
        net_address::{MultiaddressesWithStats, PeerAddressSource},
        peer_manager::{NodeIdentity, PeerFlags},
    };

    use super::*;

    fn claim_source(addresses: Vec<Multiaddr>) -> PeerAddressSource {
        let node_identity = NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            addresses.clone(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        PeerAddressSource::FromPeerConnection {
            peer_identity_claim: PeerIdentityClaim {
                addresses,
                features: PeerFeatures::COMMUNICATION_NODE,
                signature: node_identity.identity_signature_read().clone().unwrap(),
            },
        }
    }

    #[test]
    fn serving_skips_a_claim_with_an_address_that_fails_the_shape_check() {
        let good: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let other: Multiaddr = "/ip4/24.24.24.24/tcp/18189".parse().unwrap();
        let unusual: Multiaddr = "/dns4/a_b.example.com/tcp/1".parse().unwrap();
        let node_identity = NodeIdentity::random(&mut rand::rng(), good.clone(), PeerFeatures::COMMUNICATION_NODE);

        // Stored from a claim that also held the unusual name (which was filtered out on the way in)
        let mut addresses = MultiaddressesWithStats::from_addresses_with_source(
            vec![good.clone()],
            &claim_source(vec![good.clone(), unusual]),
        );
        let peer = Peer::new(
            node_identity.public_key().clone(),
            node_identity.node_id().clone(),
            addresses.clone(),
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            vec![],
            String::new(),
        );
        assert!(
            UnvalidatedPeerInfo::from_peer_limited_claims(peer, 5, 5)
                .claims
                .is_empty()
        );

        // A clean claim for another address is still served
        addresses.add_or_update_addresses(std::slice::from_ref(&other), &PeerAddressSource::FromDiscovery {
            peer_identity_claim: claim_source(vec![other.clone()]).peer_identity_claim().unwrap().clone(),
        });
        let peer = Peer::new(
            node_identity.public_key().clone(),
            node_identity.node_id().clone(),
            addresses,
            PeerFlags::empty(),
            PeerFeatures::COMMUNICATION_NODE,
            vec![],
            String::new(),
        );
        let served = UnvalidatedPeerInfo::from_peer_limited_claims(peer, 5, 5);
        assert_eq!(served.claims.len(), 1);
        assert_eq!(served.claims[0].addresses, vec![other]);
    }
}

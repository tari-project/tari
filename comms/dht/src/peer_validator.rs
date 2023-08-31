//  Copyright 2021, The Tari Project
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

use log::*;
use tari_comms::{
    net_address::{MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, Peer, PeerFlags},
    peer_validator,
    peer_validator::{find_most_recent_claim, PeerValidatorError},
};

use crate::{rpc::UnvalidatedPeerInfo, DhtConfig};

const _LOG_TARGET: &str = "dht::network_discovery::peer_validator";

/// Validation errors for peers shared on the network
#[derive(Debug, thiserror::Error)]
pub enum DhtPeerValidatorError {
    #[error("Peer '{peer}' is banned: {reason}")]
    Banned { peer: NodeId, reason: String },
    #[error(transparent)]
    ValidatorError(#[from] PeerValidatorError),
    #[error("Peer provided too many claims: expected max {max} but got {length}")]
    IdentityTooManyClaims { length: usize, max: usize },
}

/// Validator for Peers
pub struct PeerValidator<'a> {
    config: &'a DhtConfig,
}

impl<'a> PeerValidator<'a> {
    /// Creates a new peer validator
    pub fn new(config: &'a DhtConfig) -> Self {
        Self { config }
    }

    /// Validates the new peer against the current peer database. Returning true if a new peer was added and false if
    /// the peer already exists.
    pub fn validate_peer(
        &self,
        new_peer: UnvalidatedPeerInfo,
        existing_peer: Option<Peer>,
    ) -> Result<Peer, DhtPeerValidatorError> {
        if new_peer.claims.is_empty() {
            return Err(PeerValidatorError::PeerHasNoAddresses {
                peer: NodeId::from_public_key(&new_peer.public_key),
            }
            .into());
        }

        if new_peer.claims.len() > self.config.max_permitted_peer_claims {
            return Err(DhtPeerValidatorError::IdentityTooManyClaims {
                length: new_peer.claims.len(),
                max: self.config.max_permitted_peer_claims,
            });
        }

        if let Some(ref peer) = existing_peer {
            if peer.is_banned() {
                return Err(DhtPeerValidatorError::Banned {
                    peer: peer.node_id.clone(),
                    reason: peer.banned_reason.clone(),
                });
            }
        }

        let most_recent_claim = find_most_recent_claim(&new_peer.claims).expect("new_peer.claims is not empty");

        let node_id = NodeId::from_public_key(&new_peer.public_key);

        let mut peer = existing_peer.unwrap_or_else(|| {
            Peer::new(
                new_peer.public_key.clone(),
                node_id,
                MultiaddressesWithStats::default(),
                PeerFlags::default(),
                most_recent_claim.features,
                vec![],
                String::new(),
            )
        });

        for claim in new_peer.claims {
            peer_validator::validate_peer_identity_claim(
                &self.config.peer_validator_config,
                &new_peer.public_key,
                &claim,
            )?;
            peer.update_addresses(&claim.addresses, &PeerAddressSource::FromDiscovery {
                peer_identity_claim: claim.clone(),
            });
        }

        Ok(peer)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tari_comms::{
        multiaddr::Multiaddr,
        net_address::MultiaddressesWithStats,
        peer_manager::{IdentitySignature, PeerFeatures, PeerIdentityClaim},
        types::Signature,
    };
    use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
    use tari_test_utils::unpack_enum;
    use tari_utilities::ByteArray;

    use super::*;
    use crate::test_utils::make_node_identity;

    #[tokio::test]
    async fn it_errors_with_invalid_signature() {
        let config = DhtConfig::default_local_test();
        let node_identity = make_node_identity();
        let mut peer = node_identity.to_peer();
        peer.addresses = MultiaddressesWithStats::new(vec![]);
        let addr = Multiaddr::from_str("/ip4/23.23.23.23/tcp/80").unwrap();
        peer.addresses.add_address(&addr, &PeerAddressSource::FromDiscovery {
            peer_identity_claim: PeerIdentityClaim {
                addresses: vec![addr.clone()],
                features: PeerFeatures::COMMUNICATION_NODE,
                signature: IdentitySignature::new(
                    0,
                    Signature::new(
                        RistrettoPublicKey::from_bytes(&[0u8; 32]).unwrap(),
                        RistrettoSecretKey::from_bytes(&[0u8; 32]).unwrap(),
                    ),
                    Default::default(),
                ),
            },
        });
        let validator = PeerValidator::new(&config);
        let err = validator
            .validate_peer(UnvalidatedPeerInfo::from_peer_limited_claims(peer.clone(), 5, 5), None)
            .unwrap_err();
        unpack_enum!(DhtPeerValidatorError::ValidatorError(PeerValidatorError::InvalidPeerSignature { .. }) = err);
    }

    #[tokio::test]
    async fn it_does_not_add_an_invalid_peer() {
        let config = DhtConfig::default_local_test();
        let node_identity = make_node_identity();
        let mut peer = node_identity.to_peer();
        // Peer MUST provide at least one address
        peer.addresses = MultiaddressesWithStats::new(vec![]);
        let validator = PeerValidator::new(&config);
        let err = validator
            .validate_peer(UnvalidatedPeerInfo::from_peer_limited_claims(peer, 5, 5), None)
            .unwrap_err();
        unpack_enum!(DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerHasNoAddresses { .. }) = err);
    }
}

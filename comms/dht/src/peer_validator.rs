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
    net_address::{MultiaddressesWithStats, PeerAddressSource, is_external_address},
    peer_manager::{NodeId, Peer, PeerFlags},
    peer_validator,
    peer_validator::{PeerValidatorError, find_most_recent_claim},
};
use tari_utilities::hex::Hex;

use crate::{DhtConfig, rpc::UnvalidatedPeerInfo};

const LOG_TARGET: &str = "dht::network_discovery::peer_validator";

/// Validation errors for peers shared on the network
#[derive(Debug, thiserror::Error)]
pub enum DhtPeerValidatorError {
    #[error(transparent)]
    ValidatorError(#[from] PeerValidatorError),
    #[error("Peer provided too many claims: expected max {max} but got {length}")]
    IdentityTooManyClaims { length: usize, max: usize },
    #[error("Optional existing peer does not match new peer: existing '{existing}', new '{new}'")]
    NewAndExistingMismatch { existing: String, new: String },
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

        if let Some(existing) = &existing_peer &&
            existing.public_key != new_peer.public_key
        {
            return Err(DhtPeerValidatorError::NewAndExistingMismatch {
                existing: format!("BUG: '{}' / '{}'", existing.node_id, existing.public_key),
                new: format!(
                    "BUG: '{}' / '{}'",
                    NodeId::from_public_key(&new_peer.public_key),
                    new_peer.public_key
                ),
            });
        }

        let most_recent_claim = find_most_recent_claim(&new_peer.claims).expect("new_peer.claims is not empty");

        let node_id = NodeId::from_public_key(&new_peer.public_key);

        let mut peer = existing_peer.unwrap_or_else(|| {
            Peer::new(
                new_peer.public_key.clone(),
                node_id.clone(),
                MultiaddressesWithStats::default(),
                PeerFlags::default(),
                most_recent_claim.features,
                vec![],
                String::new(),
            )
        });

        let mut accepted_address_count = 0;
        for claim in new_peer.claims {
            if claim.addresses.len() > self.config.peer_validator_config.max_permitted_peer_addresses_per_claim {
                return Err(PeerValidatorError::PeerIdentityTooManyAddresses {
                    length: claim.addresses.len(),
                    max: self.config.peer_validator_config.max_permitted_peer_addresses_per_claim,
                }
                .into());
            }
            if !matches!(claim.is_valid(&new_peer.public_key), Ok(true)) {
                return Err(PeerValidatorError::InvalidPeerSignature { peer: node_id.clone() }.into());
            }

            let addresses = if self.config.peer_validator_config.allow_test_addresses {
                claim.addresses.clone()
            } else {
                claim
                    .addresses
                    .iter()
                    .filter(|address| is_external_address(address))
                    .cloned()
                    .collect()
            };
            peer_validator::validate_addresses(&self.config.peer_validator_config, &addresses)?;
            if addresses.is_empty() {
                continue;
            }

            accepted_address_count += addresses.len();
            peer.update_addresses(&addresses, &PeerAddressSource::FromDiscovery {
                peer_identity_claim: claim.clone(),
            });
            trace!(
                target: LOG_TARGET,
                "Peer '{}' / '{}' added with address(es) from claim: {:?}",
                node_id,
                new_peer.public_key.to_hex(),
                addresses
            );
        }

        if accepted_address_count == 0 {
            return Err(PeerValidatorError::PeerHasNoAddresses { peer: node_id }.into());
        }

        Ok(peer)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tari_comms::{
        multiaddr::Multiaddr,
        peer_manager::{IdentitySignature, PeerFeatures, PeerIdentityClaim},
        types::{CompressedSignature, Signature},
    };
    use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
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
        peer.addresses
            .add_or_update_addresses(std::slice::from_ref(&addr), &PeerAddressSource::FromDiscovery {
                peer_identity_claim: PeerIdentityClaim {
                    addresses: vec![addr.clone()],
                    features: PeerFeatures::COMMUNICATION_NODE,
                    signature: IdentitySignature::new(
                        0,
                        CompressedSignature::new_from_schnorr(Signature::new(
                            RistrettoPublicKey::from_canonical_bytes(&[0u8; 32]).unwrap(),
                            RistrettoSecretKey::from_canonical_bytes(&[0u8; 32]).unwrap(),
                        )),
                        Default::default(),
                    ),
                },
            });
        let validator = PeerValidator::new(&config);
        let err = validator
            .validate_peer(UnvalidatedPeerInfo::from_peer_limited_claims(peer.clone(), 5, 5), None)
            .unwrap_err();
        assert!(matches!(
            err,
            DhtPeerValidatorError::ValidatorError(PeerValidatorError::InvalidPeerSignature { .. })
        ));
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
        assert!(matches!(
            err,
            DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerHasNoAddresses { .. })
        ));
    }

    #[test]
    fn it_filters_internal_addresses_from_mixed_signed_claims() {
        let mut config = DhtConfig::default_local_test();
        config.peer_validator_config.allow_test_addresses = false;
        let node_identity = make_node_identity();
        let public_address = Multiaddr::from_str("/ip4/23.23.23.23/tcp/80").unwrap();
        let private_address = Multiaddr::from_str("/ip4/192.168.1.20/tcp/80").unwrap();
        let loopback_address = Multiaddr::from_str("/ip4/127.0.0.1/tcp/80").unwrap();
        let link_local_address = Multiaddr::from_str("/ip4/169.254.1.20/tcp/80").unwrap();
        node_identity.set_public_addresses(vec![
            public_address.clone(),
            private_address,
            loopback_address,
            link_local_address,
        ]);

        let peer = node_identity.to_peer();
        let validated = PeerValidator::new(&config)
            .validate_peer(UnvalidatedPeerInfo::from_peer_limited_claims(peer, 5, 5), None)
            .unwrap();
        let addresses = validated.addresses.address_iter().collect::<Vec<_>>();

        assert_eq!(addresses, vec![&public_address]);
    }

    #[test]
    fn it_rejects_claims_with_only_internal_addresses() {
        let mut config = DhtConfig::default_local_test();
        config.peer_validator_config.allow_test_addresses = false;
        let node_identity = make_node_identity();
        node_identity.set_public_addresses(vec![
            Multiaddr::from_str("/ip4/10.1.2.3/tcp/80").unwrap(),
            Multiaddr::from_str("/ip4/172.16.2.3/tcp/80").unwrap(),
            Multiaddr::from_str("/ip4/192.168.2.3/tcp/80").unwrap(),
            Multiaddr::from_str("/ip4/127.0.0.1/tcp/80").unwrap(),
        ]);

        let peer = node_identity.to_peer();
        let err = PeerValidator::new(&config)
            .validate_peer(UnvalidatedPeerInfo::from_peer_limited_claims(peer, 5, 5), None)
            .unwrap_err();

        assert!(matches!(
            err,
            DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerHasNoAddresses { .. })
        ));
    }
}

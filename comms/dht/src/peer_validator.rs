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

impl DhtPeerValidatorError {
    /// Returns whether the peer should be banned for this validation failure.
    ///
    /// A peer with no usable advertised addresses may simply be behind NAT or
    /// misconfigured. Dropping that update is sufficient; it is not evidence of
    /// hostile behavior.
    pub fn is_ban_offence(&self) -> bool {
        match self {
            Self::ValidatorError(err) => err.is_ban_offence(),
            Self::IdentityTooManyClaims { .. } => true,
            Self::NewAndExistingMismatch { .. } => false,
        }
    }
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

        let mut accepted_address_count = 0usize;
        for claim in new_peer.claims {
            let valid_addresses = peer_validator::validate_and_filter_peer_identity_claim_addresses(
                &self.config.peer_validator_config,
                &new_peer.public_key,
                &claim,
            )?;
            accepted_address_count = accepted_address_count.saturating_add(valid_addresses.len());
            peer.update_addresses(&valid_addresses, &PeerAddressSource::FromDiscovery {
                peer_identity_claim: claim.clone(),
            });
            trace!(
                target: LOG_TARGET,
                "Peer '{}' / '{}' added with address(es) from claim: {:?}",
                node_id,
                new_peer.public_key.to_hex(),
                valid_addresses
            );
        }

        if accepted_address_count == 0 {
            if peer.addresses.iter().any(|address| address.is_external()) {
                trace!(
                    target: LOG_TARGET,
                    "Peer '{}' / '{}' supplied no usable new addresses; retaining its existing addresses",
                    node_id,
                    new_peer.public_key.to_hex()
                );
                return Ok(peer);
            }
            return Err(PeerValidatorError::PeerHasNoUsableAddresses { peer: node_id }.into());
        }

        Ok(peer)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use tari_comms::{
        multiaddr::Multiaddr,
        peer_manager::{IdentitySignature, NodeIdentity, PeerFeatures, PeerIdentityClaim},
        types::{CompressedSignature, Signature},
    };
    use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
    use tari_utilities::ByteArray;

    use super::*;
    use crate::test_utils::make_node_identity;

    fn make_unvalidated_peer(addresses: Vec<Multiaddr>) -> UnvalidatedPeerInfo {
        let node_identity = NodeIdentity::random_multiple_addresses(
            &mut rand::rng(),
            addresses.clone(),
            PeerFeatures::COMMUNICATION_NODE,
        );
        let signature = node_identity
            .identity_signature_read()
            .as_ref()
            .expect("node identity must be signed")
            .clone();
        UnvalidatedPeerInfo {
            public_key: node_identity.public_key().clone(),
            claims: vec![PeerIdentityClaim {
                addresses,
                features: PeerFeatures::COMMUNICATION_NODE,
                signature,
            }],
        }
    }

    #[test]
    fn peers_without_usable_addresses_are_not_ban_offences() {
        let error = DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerHasNoAddresses {
            peer: NodeId::default(),
        });
        assert!(!error.is_ban_offence());

        let error = DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerHasNoUsableAddresses {
            peer: NodeId::default(),
        });
        assert!(!error.is_ban_offence());

        let error = DhtPeerValidatorError::IdentityTooManyClaims { length: 2, max: 1 };
        assert!(error.is_ban_offence());
    }

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

    #[tokio::test]
    async fn it_filters_local_addresses_but_keeps_the_peer() {
        let valid: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let loopback: Multiaddr = "/ip4/127.0.0.1/tcp/18189".parse().unwrap();
        let private: Multiaddr = "/ip4/192.168.1.2/tcp/18189".parse().unwrap();
        let link_local: Multiaddr = "/ip4/169.254.1.2/tcp/18189".parse().unwrap();
        let private_ipv6: Multiaddr = "/ip6/fc00::1/tcp/18189".parse().unwrap();
        let mapped_loopback: Multiaddr = "/ip6/::ffff:127.0.0.1/tcp/18189".parse().unwrap();
        let internal_dns: Multiaddr = "/dns4/node.internal/tcp/18189".parse().unwrap();
        let claimed_addresses = vec![
            valid.clone(),
            loopback.clone(),
            private.clone(),
            link_local.clone(),
            private_ipv6.clone(),
            mapped_loopback.clone(),
            internal_dns.clone(),
        ];
        let claimed_address_count = claimed_addresses.len();
        let new_peer = make_unvalidated_peer(claimed_addresses);
        let mut config = DhtConfig::default_local_test();
        config.peer_validator_config.allow_test_addresses = false;
        config.peer_validator_config.max_permitted_peer_addresses_per_claim = claimed_address_count;

        let peer = PeerValidator::new(&config)
            .validate_peer(new_peer, None)
            .expect("a peer with a valid public address must be retained");

        assert_eq!(peer.addresses.len(), 1);
        assert!(peer.addresses.contains(&valid));
        assert!(!peer.addresses.contains(&loopback));
        assert!(!peer.addresses.contains(&private));
        assert!(!peer.addresses.contains(&link_local));
        assert!(!peer.addresses.contains(&private_ipv6));
        assert!(!peer.addresses.contains(&mapped_loopback));
        assert!(!peer.addresses.contains(&internal_dns));
    }

    #[tokio::test]
    async fn it_rejects_a_peer_with_only_local_addresses() {
        let new_peer = make_unvalidated_peer(vec![
            "/ip4/127.0.0.1/tcp/18189".parse().unwrap(),
            "/ip4/10.0.0.2/tcp/18189".parse().unwrap(),
            "/ip6/fe80::1/tcp/18189".parse().unwrap(),
        ]);
        let mut config = DhtConfig::default_local_test();
        config.peer_validator_config.allow_test_addresses = false;

        let err = PeerValidator::new(&config).validate_peer(new_peer, None).unwrap_err();

        assert!(matches!(
            err,
            DhtPeerValidatorError::ValidatorError(PeerValidatorError::PeerHasNoUsableAddresses { .. })
        ));
    }

    #[tokio::test]
    async fn it_keeps_existing_addresses_after_an_all_local_update() {
        let new_peer = make_unvalidated_peer(vec![
            "/ip4/127.0.0.1/tcp/18189".parse().unwrap(),
            "/ip4/192.168.1.2/tcp/18189".parse().unwrap(),
        ]);
        let node_id = NodeId::from_public_key(&new_peer.public_key);
        let existing_address: Multiaddr = "/ip4/23.23.23.23/tcp/18189".parse().unwrap();
        let mut existing_peer = Peer::new(
            new_peer.public_key.clone(),
            node_id,
            MultiaddressesWithStats::default(),
            PeerFlags::default(),
            PeerFeatures::COMMUNICATION_NODE,
            vec![],
            String::new(),
        );
        existing_peer.addresses = MultiaddressesWithStats::from_addresses_with_source(
            vec![existing_address.clone()],
            &PeerAddressSource::Config,
        );
        let mut config = DhtConfig::default_local_test();
        config.peer_validator_config.allow_test_addresses = false;

        let peer = PeerValidator::new(&config)
            .validate_peer(new_peer, Some(existing_peer))
            .expect("an all-local update must not discard a reachable existing peer");

        assert_eq!(peer.addresses.len(), 1);
        assert!(peer.addresses.contains(&existing_address));
    }

    #[tokio::test]
    async fn it_keeps_local_addresses_when_test_addresses_are_allowed() {
        let addresses = vec![
            "/ip4/127.0.0.1/tcp/18189".parse().unwrap(),
            "/ip4/192.168.1.2/tcp/18189".parse().unwrap(),
            "/ip6/fe80::1/tcp/18189".parse().unwrap(),
            "/dns4/node.internal/tcp/18189".parse().unwrap(),
        ];
        let new_peer = make_unvalidated_peer(addresses.clone());
        let config = DhtConfig::default_local_test();

        let peer = PeerValidator::new(&config)
            .validate_peer(new_peer, None)
            .expect("test addresses must be retained when explicitly allowed");

        assert_eq!(peer.addresses.len(), addresses.len());
        for address in addresses {
            assert!(peer.addresses.contains(&address));
        }
    }
}

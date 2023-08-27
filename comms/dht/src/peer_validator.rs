//  Copyright 2021, The Taiji Project
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
use taiji_comms::{
    connection_manager::validate_address_and_source,
    net_address::{MultiaddrWithStats, MultiaddressesWithStats, PeerAddressSource},
    peer_manager::{NodeId, Peer, PeerFlags, PeerManagerError},
    types::CommsPublicKey,
    PeerManager,
};

use crate::{rpc::PeerInfo, DhtConfig};

const LOG_TARGET: &str = "dht::network_discovery::peer_validator";

/// Validation errors for peers shared on the network
#[derive(Debug, thiserror::Error)]
pub enum PeerValidatorError {
    #[error("Node ID was invalid for peer '{peer}'")]
    InvalidNodeId { peer: NodeId },
    #[error("Peer signature was invalid for peer '{peer}'")]
    InvalidPeerSignature { peer: NodeId },
    #[error("One or more peer addresses were invalid for '{peer}'")]
    InvalidPeerAddresses { peer: NodeId },
    #[error("Peer '{peer}' was banned")]
    PeerHasNoAddresses { peer: NodeId },
    #[error("Peer manager error: {0}")]
    PeerManagerError(#[from] PeerManagerError),
}

/// Validator for Peers
pub struct PeerValidator<'a> {
    peer_manager: &'a PeerManager,
    config: &'a DhtConfig,
}

impl<'a> PeerValidator<'a> {
    /// Creates a new peer validator
    pub fn new(peer_manager: &'a PeerManager, config: &'a DhtConfig) -> Self {
        Self { peer_manager, config }
    }

    /// Validates the new peer against the current peer database. Returning true if a new peer was added and false if
    /// the peer already exists.
    pub async fn validate_and_add_peer(&self, new_peer: PeerInfo) -> Result<bool, PeerValidatorError> {
        let node_id = NodeId::from_public_key(&new_peer.public_key);

        if new_peer.addresses.is_empty() {
            return Err(PeerValidatorError::PeerHasNoAddresses { peer: node_id });
        }
        let mut peer = Peer::new(
            new_peer.public_key.clone(),
            node_id.clone(),
            MultiaddressesWithStats::new(vec![]),
            PeerFlags::default(),
            new_peer.peer_features,
            new_peer.supported_protocols,
            new_peer.user_agent,
        );

        for addr in new_peer.addresses {
            let multiaddr_and_stats = MultiaddrWithStats::new(addr.address.clone(), PeerAddressSource::FromDiscovery {
                peer_identity_claim: addr.peer_identity_claim,
            });
            match validate_address_and_source(
                &new_peer.public_key,
                &multiaddr_and_stats,
                self.config.allow_test_addresses,
            ) {
                Ok(()) => {
                    peer.addresses
                        .add_address(multiaddr_and_stats.address(), multiaddr_and_stats.source());
                },
                Err(e) => {
                    warn!(
                        target: LOG_TARGET,
                        "Peer provided info on another peer that had a bad address or signature (new peer: {} \
                         address: {}): error:{}. Ignoring.",
                        new_peer.public_key,
                        addr.address,
                        e
                    );
                },
            }
        }
        validate_node_id(&peer.public_key, &peer.node_id)?;

        let exists = self.peer_manager.exists(&peer.public_key).await;

        self.peer_manager.add_peer(peer).await?;

        Ok(!exists)
    }
}

fn validate_node_id(public_key: &CommsPublicKey, node_id: &NodeId) -> Result<NodeId, PeerValidatorError> {
    let expected_node_id = NodeId::from_key(public_key);
    if expected_node_id == *node_id {
        Ok(expected_node_id)
    } else {
        Err(PeerValidatorError::InvalidNodeId { peer: node_id.clone() })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use taiji_comms::{
        multiaddr::Multiaddr,
        net_address::MultiaddressesWithStats,
        peer_manager::{IdentitySignature, PeerFeatures, PeerIdentityClaim},
        types::Signature,
    };
    use tari_crypto::ristretto::{RistrettoPublicKey, RistrettoSecretKey};
    use taiji_test_utils::unpack_enum;
    use tari_utilities::ByteArray;

    use super::*;
    use crate::test_utils::{build_peer_manager, make_node_identity};

    #[tokio::test]
    async fn it_adds_a_valid_unsigned_peer() {
        let peer_manager = build_peer_manager();
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
                unverified_data: None,
            },
        });
        let validator = PeerValidator::new(&peer_manager, &config);
        let is_new = validator.validate_and_add_peer(peer.clone().into()).await.unwrap();
        assert!(is_new);
        assert!(peer_manager.exists(&peer.public_key).await);
    }

    #[tokio::test]
    async fn it_does_not_add_an_invalid_peer() {
        let peer_manager = build_peer_manager();
        let config = DhtConfig::default_local_test();
        let node_identity = make_node_identity();
        let mut peer = node_identity.to_peer();
        // Peer MUST provide at least one address
        peer.addresses = MultiaddressesWithStats::new(vec![]);
        let validator = PeerValidator::new(&peer_manager, &config);
        let err = validator.validate_and_add_peer(peer.clone().into()).await.unwrap_err();
        unpack_enum!(PeerValidatorError::PeerHasNoAddresses { .. } = err);
        assert!(!peer_manager.exists(&peer.public_key).await);
    }
}

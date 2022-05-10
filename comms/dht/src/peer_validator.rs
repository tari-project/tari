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
    peer_manager::{NodeId, Peer, PeerManagerError},
    types::CommsPublicKey,
    validate_peer_addresses,
    PeerManager,
};

use crate::DhtConfig;

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
    pub async fn validate_and_add_peer(&self, new_peer: Peer) -> Result<bool, PeerValidatorError> {
        validate_node_id(&new_peer.public_key, &new_peer.node_id)?;

        if let Err(err) = validate_peer_addresses(new_peer.addresses.iter(), self.config.allow_test_addresses) {
            warn!(target: LOG_TARGET, "Invalid peer address: {}", err);
            return Err(PeerValidatorError::InvalidPeerAddresses { peer: new_peer.node_id });
        }

        let can_update = match new_peer.is_valid_identity_signature() {
            // Update/insert peer
            Some(true) => true,
            Some(false) => return Err(PeerValidatorError::InvalidPeerSignature { peer: new_peer.node_id }),
            // Insert new peer if it doesn't exist, do not update
            None => false,
        };

        debug!(target: LOG_TARGET, "Adding peer `{}`", new_peer.node_id);

        match self.peer_manager.find_by_node_id(&new_peer.node_id).await? {
            Some(mut current_peer) => {
                let can_update = can_update && {
                    // Update/insert peer if newer
                    // unreachable panic: can_update is true only is identity_signature is present and valid
                    let new_dt = new_peer
                        .identity_signature
                        .as_ref()
                        .map(|i| i.updated_at())
                        .expect("unreachable panic");

                    // Update if new_peer has newer timestamp than current_peer, and if the newer timestamp is after the
                    // added date
                    current_peer
                        .identity_signature
                        .as_ref()
                        .map(|i| i.updated_at() < new_dt && (
                            !current_peer.is_seed() ||
                            current_peer.added_at < new_dt.naive_utc()))
                        // If None, update to peer with valid signature
                        .unwrap_or(true)
                };

                if !can_update {
                    debug!(
                        target: LOG_TARGET,
                        "Peer `{}` already exists or is up to date and will not be updated", new_peer.node_id
                    );
                    return Ok(false);
                }

                debug!(target: LOG_TARGET, "Updating peer `{}`", new_peer.node_id);
                current_peer
                    .update_addresses(new_peer.addresses.into_vec())
                    .set_features(new_peer.features)
                    .set_offline(false);
                if let Some(sig) = new_peer.identity_signature {
                    current_peer.set_valid_identity_signature(sig);
                }
                self.peer_manager.add_peer(current_peer).await?;

                Ok(false)
            },
            None => {
                debug!(target: LOG_TARGET, "Adding peer `{}`", new_peer.node_id);
                self.peer_manager.add_peer(new_peer).await?;
                Ok(true)
            },
        }
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
    use chrono::Utc;
    use tari_comms::{net_address::MultiaddressesWithStats, peer_manager::PeerFlags};
    use tari_test_utils::unpack_enum;

    use super::*;
    use crate::test_utils::{build_peer_manager, make_node_identity};

    #[tokio::test]
    async fn it_adds_a_valid_unsigned_peer() {
        let peer_manager = build_peer_manager();
        let config = DhtConfig::default_local_test();
        let node_identity = make_node_identity();
        let mut peer = node_identity.to_peer();
        peer.identity_signature = None;
        let validator = PeerValidator::new(&peer_manager, &config);
        let is_new = validator.validate_and_add_peer(peer.clone()).await.unwrap();
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
        let err = validator.validate_and_add_peer(peer.clone()).await.unwrap_err();
        unpack_enum!(PeerValidatorError::InvalidPeerAddresses { .. } = err);
        assert!(!peer_manager.exists(&peer.public_key).await);
    }

    #[tokio::test]
    async fn it_updates_a_newer_signed_peer() {
        let peer_manager = build_peer_manager();
        let config = DhtConfig::default_local_test();
        let validator = PeerValidator::new(&peer_manager, &config);

        let node_identity = make_node_identity();
        let peer = node_identity.to_peer();
        peer_manager.add_peer(peer).await.unwrap();

        node_identity.set_public_address("/dns4/updated.com/tcp/1234".parse().unwrap());
        node_identity.sign();
        let peer = node_identity.to_peer();

        let is_new = validator.validate_and_add_peer(peer.clone()).await.unwrap();
        assert!(!is_new);
        let peer = peer_manager
            .find_by_public_key(&peer.public_key)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(peer.addresses[0].address.to_string(), "/dns4/updated.com/tcp/1234");
    }

    #[tokio::test]
    async fn it_does_not_update_a_valid_unsigned_peer() {
        let peer_manager = build_peer_manager();
        let config = DhtConfig::default_local_test();
        let validator = PeerValidator::new(&peer_manager, &config);

        let node_identity = make_node_identity();
        let prev_addr = node_identity.public_address();
        let mut peer = node_identity.to_peer();
        peer_manager.add_peer(peer.clone()).await.unwrap();

        peer.identity_signature = None;
        peer.update_addresses(vec!["/dns4/updated.com/tcp/1234".parse().unwrap()]);

        let is_new = validator.validate_and_add_peer(peer.clone()).await.unwrap();
        assert!(!is_new);
        let peer = peer_manager
            .find_by_public_key(&peer.public_key)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(peer.addresses[0].address, prev_addr);
    }

    #[tokio::test]
    async fn it_does_not_add_a_seed_peer_if_added_more_recently_than_update() {
        let peer_manager = build_peer_manager();
        let config = DhtConfig::default_local_test();
        let validator = PeerValidator::new(&peer_manager, &config);

        let node_identity = make_node_identity();
        let mut peer = node_identity.to_peer();
        peer.add_flags(PeerFlags::SEED);
        peer.added_at = (Utc::now() - chrono::Duration::minutes(10)).naive_utc();
        peer_manager.add_peer(peer).await.unwrap();

        node_identity.set_public_address("/dns4/updated.com/tcp/1234".parse().unwrap());
        node_identity.sign();

        let peer = node_identity.to_peer();
        let is_new = validator.validate_and_add_peer(peer.clone()).await.unwrap();
        assert!(!is_new);
        let peer = peer_manager
            .find_by_public_key(&peer.public_key)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(peer.addresses[0].address, node_identity.public_address());
    }
}

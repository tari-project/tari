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

use crate::DhtConfig;
use log::*;
use tari_comms::{
    peer_manager::{NodeId, Peer, PeerManagerError},
    types::CommsPublicKey,
    validate_peer_addresses,
    PeerManager,
};

const LOG_TARGET: &str = "dht::network_discovery::peer_validator";

#[derive(Debug, thiserror::Error)]
pub enum PeerValidatorError {
    #[error("Node ID was invalid")]
    InvalidNodeId,
    #[error("Peer signature was invalid")]
    InvalidPeerSignature,
    #[error("One or more peer addresses were invalid")]
    InvalidPeerAddresses,
    #[error("Peer manager error: {0}")]
    PeerManagerError(#[from] PeerManagerError),
}

pub struct PeerValidator<'a> {
    peer_manager: &'a PeerManager,
    config: &'a DhtConfig,
}

impl<'a> PeerValidator<'a> {
    pub fn new(peer_manager: &'a PeerManager, config: &'a DhtConfig) -> Self {
        Self { peer_manager, config }
    }

    /// Validates the new peer against the current peer database. Returning true if a new peer was added and false if
    /// the peer already exists.
    pub async fn validate_and_add_peer(&self, new_peer: Peer) -> Result<bool, PeerValidatorError> {
        validate_node_id(&new_peer.public_key, &new_peer.node_id)?;

        if let Err(err) = validate_peer_addresses(new_peer.addresses.iter(), self.config.allow_test_addresses) {
            warn!(target: LOG_TARGET, "Invalid peer address: {}", err);
            return Err(PeerValidatorError::InvalidPeerAddresses);
        }

        let can_update = match new_peer.is_valid_identity_signature() {
            // Update/insert peer
            Some(true) => true,
            Some(false) => return Err(PeerValidatorError::InvalidPeerSignature),
            // Insert new peer if it doesn't exist, do not update
            None => false,
        };

        debug!(target: LOG_TARGET, "Adding peer `{}`", new_peer.node_id);

        match self.peer_manager.find_by_node_id(&new_peer.node_id).await? {
            Some(mut peer) => {
                if !can_update {
                    debug!(
                        target: LOG_TARGET,
                        "Peer `{}` already exists and will not be added to the peer list", new_peer.node_id
                    );
                    return Ok(false);
                }

                peer.addresses.update_addresses(new_peer.addresses.into_vec());
                peer.identity_signature = new_peer.identity_signature;
                peer.set_offline(false);
                self.peer_manager.add_peer(peer).await?;

                Ok(false)
            },
            None => {
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
        Err(PeerValidatorError::InvalidNodeId)
    }
}

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

use crate::message::NodeDestination;
use derive_error::Error;
use std::{fmt, fmt::Formatter};
use tari_comms::{
    peer_manager::{node_id::NodeId, peer_manager::PeerManager, PeerManagerError},
    types::CommsPublicKey,
};

/// The number of neighbouring nodes that a received message will be forwarded to
pub const DHT_FORWARD_NODE_COUNT: usize = 8;

#[derive(Debug, Error)]
pub enum BroadcastStrategyError {
    PeerManagerError(PeerManagerError),
}

#[derive(Debug, Clone)]
pub struct ClosestRequest {
    pub n: usize,
    pub node_id: NodeId,
    pub excluded_peers: Vec<CommsPublicKey>,
}

#[derive(Debug, Clone)]
pub enum BroadcastStrategy {
    /// Send to a particular peer matching the given node ID
    DirectNodeId(NodeId),
    /// Send to a particular peer matching the given Public Key
    DirectPublicKey(CommsPublicKey),
    /// Send to all known Communication Node peers
    Flood,
    /// Send to all n nearest neighbour Communication Nodes
    Closest(ClosestRequest),
    /// Send to a random set of peers of size n that are Communication Nodes
    Random(usize),
}

impl fmt::Display for BroadcastStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        use BroadcastStrategy::*;
        match self {
            DirectPublicKey(pk) => write!(f, "DirectPublicKey({})", pk),
            DirectNodeId(node_id) => write!(f, "DirectNodeId({})", node_id),
            Flood => write!(f, "Flood"),
            Closest(ClosestRequest { n, .. }) => write!(f, "Closest({})", n),
            Random(n) => write!(f, "Random({})", n),
        }
    }
}

// TODO: move this logic, peer manager shouldn't be passed in to broadcast strategy
impl BroadcastStrategy {
    pub fn direct_node_id(&self) -> Option<&NodeId> {
        use BroadcastStrategy::*;
        match self {
            DirectNodeId(node_id) => Some(node_id),
            _ => None,
        }
    }

    pub fn direct_public_key(&self) -> Option<&CommsPublicKey> {
        use BroadcastStrategy::*;
        match self {
            DirectPublicKey(pk) => Some(pk),
            _ => None,
        }
    }

    /// The forward function selects the most appropriate broadcast strategy based on the received messages destination
    pub fn forward(
        // This node's node ID
        source_node_id: NodeId,
        peer_manager: &PeerManager,
        header_dest: NodeDestination,
        excluded_peers: Vec<CommsPublicKey>,
    ) -> Result<Self, BroadcastStrategyError>
    {
        Ok(match header_dest {
            NodeDestination::Undisclosed => {
                // Send to the current nodes nearest neighbours
                BroadcastStrategy::Closest(ClosestRequest {
                    n: DHT_FORWARD_NODE_COUNT,
                    node_id: source_node_id,
                    excluded_peers,
                })
            },
            NodeDestination::PublicKey(dest_public_key) => {
                if peer_manager.exists(&dest_public_key)? {
                    // Send to destination peer directly if the current node knows that peer
                    BroadcastStrategy::DirectPublicKey(dest_public_key)
                } else {
                    // Send to the current nodes nearest neighbours
                    BroadcastStrategy::Closest(ClosestRequest {
                        n: DHT_FORWARD_NODE_COUNT,
                        node_id: source_node_id,
                        excluded_peers,
                    })
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                match peer_manager.find_with_node_id(&dest_node_id) {
                    Ok(dest_peer) => {
                        // Send to destination peer directly if the current node knows that peer
                        BroadcastStrategy::DirectPublicKey(dest_peer.public_key)
                    },
                    Err(_) => {
                        // Send to peers that are closest to the destination network region
                        BroadcastStrategy::Closest(ClosestRequest {
                            n: DHT_FORWARD_NODE_COUNT,
                            node_id: dest_node_id,
                            excluded_peers,
                        })
                    },
                }
            },
        })
    }

    /// The discover function selects an appropriate broadcast strategy for the discovery of a specific node
    pub fn discover(
        source_node_id: NodeId,
        dest_node_id: Option<NodeId>,
        header_dest: NodeDestination,
        excluded_peers: Vec<CommsPublicKey>,
    ) -> Self
    {
        let network_location_node_id = match dest_node_id {
            Some(node_id) => node_id,
            None => match header_dest.clone() {
                NodeDestination::Undisclosed => source_node_id,
                NodeDestination::PublicKey(_) => source_node_id,
                NodeDestination::NodeId(node_id) => node_id,
            },
        };
        BroadcastStrategy::Closest(ClosestRequest {
            n: DHT_FORWARD_NODE_COUNT,
            node_id: network_location_node_id,
            excluded_peers,
        })
    }
}

// Copyright 2019, The Taiji Project
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

//! # Broadcast strategy
//!
//! Describes a strategy for selecting peers and active connections when sending messages.

use std::{
    fmt,
    fmt::{Display, Formatter},
};

use taiji_comms::{peer_manager::node_id::NodeId, types::CommsPublicKey};

use crate::envelope::NodeDestination;

/// Parameters for the [ClosestNodes](self::BroadcastStrategy::ClosestNodes) broadcast strategy.
#[derive(Debug, Clone)]
pub struct BroadcastClosestRequest {
    pub node_id: NodeId,
    pub excluded_peers: Vec<NodeId>,
    pub connected_only: bool,
}

impl Display for BroadcastClosestRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ClosestRequest: node_id = {}, excluded_peers = {} peer(s), connected_only = {}",
            self.node_id,
            self.excluded_peers.len(),
            self.connected_only
        )
    }
}

/// Describes a strategy for selecting peers and active connections when sending messages.
#[derive(Debug, Clone)]
pub enum BroadcastStrategy {
    /// Send to a particular peer matching the given node ID
    DirectNodeId(Box<NodeId>),
    /// Send to a particular peer matching the given Public Key
    DirectPublicKey(Box<CommsPublicKey>),
    /// Send to all connected peers. If no peers are connected, no messages are sent.
    Flood(Vec<NodeId>),
    /// Send to a random set of peers of size n that are Communication Nodes, excluding the given node IDs
    Random(usize, Vec<NodeId>),
    /// Send to all n nearest Communication Nodes according to the given BroadcastClosestRequest
    ClosestNodes(Box<BroadcastClosestRequest>),
    /// Send directly to destination if connected but otherwise send to all n nearest Communication Nodes
    DirectOrClosestNodes(Box<BroadcastClosestRequest>),
    Broadcast(Vec<NodeId>),
    SelectedPeers(Vec<NodeId>),
    /// Propagate to a set of closest neighbours and random peers
    Propagate(NodeDestination, Vec<NodeId>),
}

impl fmt::Display for BroadcastStrategy {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), fmt::Error> {
        #[allow(clippy::enum_glob_use)]
        use BroadcastStrategy::*;
        match self {
            DirectPublicKey(pk) => write!(f, "DirectPublicKey({})", pk),
            DirectNodeId(node_id) => write!(f, "DirectNodeId({})", node_id),
            Flood(excluded) => write!(f, "Flood({} excluded)", excluded.len()),
            ClosestNodes(request) => write!(f, "ClosestNodes({})", request),
            DirectOrClosestNodes(request) => write!(f, "DirectOrClosestNodes({})", request),
            Random(n, excluded) => write!(f, "Random({}, {} excluded)", n, excluded.len()),
            Broadcast(excluded) => write!(f, "Broadcast({} excluded)", excluded.len()),
            Propagate(destination, excluded) => write!(f, "Propagate({}, {} excluded)", destination, excluded.len(),),
            SelectedPeers(peers) => write!(f, "SelectedPeers({} peer(s))", peers.len()),
        }
    }
}

impl BroadcastStrategy {
    /// Returns true if this strategy will send multiple indirect messages, otherwise false
    pub fn is_multi_message(&self, chosen_peers: &[NodeId]) -> bool {
        use BroadcastStrategy::{Broadcast, ClosestNodes, DirectOrClosestNodes, Flood, Propagate, Random};

        match self {
            DirectOrClosestNodes(strategy) => {
                // Testing if there is a single chosen peer and it is the target NodeId
                chosen_peers.len() == 1 && chosen_peers.first() == Some(&strategy.node_id)
            },
            ClosestNodes(_) | Broadcast(_) | Propagate(_, _) | Flood(_) | Random(_, _) => true,
            _ => false,
        }
    }

    /// Returns true if the strategy is to send directly to the peer, otherwise false
    pub fn is_direct(&self) -> bool {
        use BroadcastStrategy::{DirectNodeId, DirectPublicKey};
        matches!(self, DirectNodeId(_) | DirectPublicKey(_))
    }

    /// Returns a reference to the NodeId used in the `DirectNodeId` strategy, otherwise None if the strategy is not
    /// `DirectNodeId`.
    pub fn direct_node_id(&self) -> Option<&NodeId> {
        use BroadcastStrategy::DirectNodeId;
        match self {
            DirectNodeId(node_id) => Some(node_id),
            _ => None,
        }
    }

    /// Returns a reference to the `CommsPublicKey` used in the `DirectPublicKey` strategy, otherwise None if the
    /// strategy is not `DirectPublicKey`.
    pub fn direct_public_key(&self) -> Option<&CommsPublicKey> {
        use BroadcastStrategy::DirectPublicKey;
        match self {
            DirectPublicKey(pk) => Some(pk),
            _ => None,
        }
    }

    /// Returns the `CommsPublicKey` used in the `DirectPublicKey` strategy, otherwise None if the strategy is not
    /// `DirectPublicKey`.
    pub fn into_direct_public_key(self) -> Option<Box<CommsPublicKey>> {
        use BroadcastStrategy::DirectPublicKey;
        match self {
            DirectPublicKey(pk) => Some(pk),
            _ => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn is_direct() {
        assert!(BroadcastStrategy::DirectPublicKey(Box::default()).is_direct());
        assert!(BroadcastStrategy::DirectNodeId(Box::default()).is_direct());
        assert!(!BroadcastStrategy::Broadcast(Default::default()).is_direct());
        assert!(!BroadcastStrategy::Propagate(Default::default(), Default::default()).is_direct(),);
        assert!(!BroadcastStrategy::Flood(Default::default()).is_direct());
        assert!(!BroadcastStrategy::ClosestNodes(Box::new(BroadcastClosestRequest {
            node_id: NodeId::default(),
            excluded_peers: Default::default(),
            connected_only: false
        }))
        .is_direct(),);
        assert!(!BroadcastStrategy::Random(0, vec![]).is_direct());
    }

    #[test]
    fn direct_public_key() {
        assert!(BroadcastStrategy::DirectPublicKey(Box::default())
            .direct_public_key()
            .is_some());
        assert!(BroadcastStrategy::DirectNodeId(Box::default())
            .direct_public_key()
            .is_none());
        assert!(BroadcastStrategy::Broadcast(Default::default(),)
            .direct_public_key()
            .is_none());
        assert!(BroadcastStrategy::Flood(Default::default())
            .direct_public_key()
            .is_none());
        assert!(BroadcastStrategy::ClosestNodes(Box::new(BroadcastClosestRequest {
            node_id: NodeId::default(),
            excluded_peers: Default::default(),
            connected_only: false
        }))
        .direct_public_key()
        .is_none(),);
        assert!(BroadcastStrategy::Random(0, vec![]).direct_public_key().is_none());
    }

    #[test]
    fn direct_node_id() {
        assert!(BroadcastStrategy::DirectPublicKey(Box::default())
            .direct_node_id()
            .is_none());
        assert!(BroadcastStrategy::DirectNodeId(Box::default())
            .direct_node_id()
            .is_some());
        assert!(BroadcastStrategy::Broadcast(Default::default(),)
            .direct_node_id()
            .is_none());
        assert!(BroadcastStrategy::Flood(Default::default()).direct_node_id().is_none());
        assert!(BroadcastStrategy::ClosestNodes(Box::new(BroadcastClosestRequest {
            node_id: NodeId::default(),
            excluded_peers: Default::default(),
            connected_only: false
        }))
        .direct_node_id()
        .is_none(),);
        assert!(BroadcastStrategy::Random(0, vec![]).direct_node_id().is_none());
    }
}

//  Copyright 2019 The Tari Project
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

use crate::{
    connection::net_address::NetAddress,
    outbound_message_service::broadcast_strategy::BroadcastStrategy,
    peer_manager::{node_id::NodeId, node_identity::PeerNodeIdentity, peer::Peer, peer_storage::PeerStorage},
};
use derive_error::Error;
use std::{hash::Hash, sync::RwLock, time::Duration};
use tari_crypto::keys::PublicKey;
use tari_storage::keyvalue_store::{DataStore, DatastoreError};
use tari_utilities::message_format::MessageFormatError;

#[derive(Debug, Error)]
pub enum PeerManagerError {
    /// The requested peer does not exist or could not be located
    PeerNotFoundError,
    /// The Thread Safety has been breached and the data access has become poisoned
    PoisonedAccess,
    /// Could not write or read from datastore
    DatastoreError(DatastoreError),
    /// A problem occurred during the serialization of the keys or data
    SerializationError(MessageFormatError),
    /// A problem occurred converting the serialized data into peers
    DeserializationError,
    /// The index doesn't relate to an existing peer
    IndexOutOfBounds,
    /// The requested operation can only be performed if the PeerManager is linked to a DataStore
    DatastoreUndefined,
    /// An empty response was received from the Datastore
    EmptyDatastoreQuery,
    /// The data update could not be performed
    DataUpdateError,
}

/// The PeerManager consist of a routing table of previously discovered peers.
/// It also provides functionality to add, find and delete peers. A subset of peers can also be requested from the
/// routing table based on the selected Broadcast strategy.
pub struct PeerManager<PubKey, DS> {
    peer_storage: RwLock<PeerStorage<PubKey, DS>>,
}

impl<PubKey, DS> PeerManager<PubKey, DS>
where
    PubKey: PublicKey + Hash,
    DS: DataStore,
{
    /// Constructs a new empty PeerManager
    pub fn new(datastore: Option<DS>) -> Result<PeerManager<PubKey, DS>, PeerManagerError> {
        Ok(match datastore {
            Some(datastore) => PeerManager {
                peer_storage: RwLock::new(PeerStorage::<PubKey, DS>::new().init_persistance_store(datastore)?),
            },
            None => PeerManager {
                peer_storage: RwLock::new(PeerStorage::<PubKey, DS>::new()),
            },
        })
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub fn add_peer(&self, peer: Peer<PubKey>) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .add_peer(peer)
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub fn delete_peer(&self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .delete_peer(node_id)
    }

    /// Find the peer with the provided NodeID
    pub fn find_with_node_id(&self, node_id: &NodeId) -> Result<Peer<PubKey>, PeerManagerError> {
        self.peer_storage
            .read()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .find_with_node_id(node_id)
    }

    /// Find the peer with the provided PublicKey
    pub fn find_with_public_key(&self, public_key: &PubKey) -> Result<Peer<PubKey>, PeerManagerError> {
        self.peer_storage
            .read()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .find_with_public_key(public_key)
    }

    /// Find the peer with the provided NetAddress
    pub fn find_with_net_address(&self, net_address: &NetAddress) -> Result<Peer<PubKey>, PeerManagerError> {
        self.peer_storage
            .read()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .find_with_net_address(net_address)
    }

    pub fn get_broadcast_identities(
        &self,
        broadcast_strategy: BroadcastStrategy,
    ) -> Result<Vec<PeerNodeIdentity<PubKey>>, PeerManagerError>
    {
        match broadcast_strategy {
            BroadcastStrategy::Direct(node_id) => {
                // Send to a particular peer matching the given node ID
                self.peer_storage
                    .read()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .direct_identity(&node_id)
            },
            BroadcastStrategy::Flood => {
                // Send to all known Communication Node peers
                self.peer_storage
                    .read()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .flood_identities()
            },
            BroadcastStrategy::Closest(n) => {
                // Send to all n nearest neighbour Communication Nodes
                self.peer_storage
                    .read()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .closest_identities(n)
            },
            BroadcastStrategy::Random(n) => {
                // Send to a random set of peers of size n that are Communication Nodes
                self.peer_storage
                    .read()
                    .map_err(|_| PeerManagerError::PoisonedAccess)?
                    .random_identities(n)
            },
        }
    }

    /// Thread safe access to peer - Changes the ban flag bit of the peer
    pub fn set_banned(&self, node_id: &NodeId, ban_flag: bool) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .set_banned(node_id, ban_flag)
    }

    /// Thread safe access to peer - Adds a new net address to the peer if it doesn't yet exist
    pub fn add_net_address(&self, node_id: &NodeId, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .add_net_address(node_id, net_address)
    }

    /// Thread safe access to peer - Finds and returns the highest priority net address until all connection attempts
    /// for each net address have been reached
    pub fn get_best_net_address(&self, node_id: &NodeId) -> Result<NetAddress, PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .get_best_net_address(node_id)
    }

    /// Thread safe access to peer - The average connection latency of the provided net address will be updated to
    /// include the current measured latency sample
    pub fn update_latency(
        &self,
        net_address: &NetAddress,
        latency_measurement: Duration,
    ) -> Result<(), PeerManagerError>
    {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .update_latency(net_address, latency_measurement)
    }

    /// Thread safe access to peer - Mark that a message was received from the specified net address
    pub fn mark_message_received(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .mark_message_received(net_address)
    }

    /// Thread safe access to peer - Mark that a rejected message was received from the specified net address
    pub fn mark_message_rejected(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .mark_message_rejected(net_address)
    }

    /// Thread safe access to peer - Mark that a successful connection was established with the specified net address
    pub fn mark_successful_connection_attempt(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .mark_successful_connection_attempt(net_address)
    }

    /// Thread safe access to peer - Mark that a connection could not be established with the specified net address
    pub fn mark_failed_connection_attempt(&self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        self.peer_storage
            .write()
            .map_err(|_| PeerManagerError::PoisonedAccess)?
            .mark_failed_connection_attempt(net_address)
    }
}

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
    peer_manager::{
        node_id::{NodeDistance, NodeId},
        node_identity::PeerNodeIdentity,
        peer::Peer,
        peer_manager::PeerManagerError,
    },
    types::CommsRng,
};
use rand::Rng;
use std::{collections::HashMap, hash::Hash, ops::Index, time::Duration};
use tari_crypto::keys::PublicKey;
use tari_storage::keyvalue_store::DataStore;
use tari_utilities::message_format::MessageFormat;

/// PeerStorage provides a mechanism to keep a datastore and a local copy of all peers in sync and allow fast searches
/// using the node_id, public key or net_address of a peer.
pub struct PeerStorage<PubKey, DS> {
    pub(crate) datastore: Option<DS>,
    pub(crate) peers: Vec<Peer<PubKey>>,
    node_id_hm: HashMap<NodeId, usize>,
    public_key_hm: HashMap<PubKey, usize>,
    net_address_hm: HashMap<NetAddress, usize>,
    rng: CommsRng,
}

impl<PubKey, DS> PeerStorage<PubKey, DS>
where
    PubKey: PublicKey + Hash,
    DS: DataStore,
{
    /// Constructs a new empty PeerStorage system
    pub fn new() -> Result<PeerStorage<PubKey, DS>, PeerManagerError> {
        Ok(PeerStorage {
            datastore: None,
            peers: Vec::new(),
            node_id_hm: HashMap::new(),
            public_key_hm: HashMap::new(),
            net_address_hm: HashMap::new(),
            rng: CommsRng::new().map_err(|_| PeerManagerError::RngError)?,
        })
    }

    /// Connects and restore the PeerStorage system from a datastore
    pub fn init_persistance_store(mut self, datastore: DS) -> Result<PeerStorage<PubKey, DS>, PeerManagerError> {
        self.datastore = Some(datastore);
        // Restore from datastore
        let mut index = 0;
        while let Ok(peer) = self.get_peer_from_datastore(index) {
            let peer_index = self.peers.len();
            self.add_peer_hashmap_links(peer_index, &peer);
            self.peers.push(peer);
            index += 1;
        }
        Ok(self)
    }

    /// Adds a peer to the routing table of the PeerManager if the peer does not already exist. When a peer already
    /// exist, the stored version will be replaced with the newly provided peer.
    pub fn add_peer(&mut self, peer: Peer<PubKey>) -> Result<(), PeerManagerError> {
        match self.public_key_hm.get(&peer.public_key) {
            Some(index) => {
                // Replace existing entry
                let peer_index = *index; // TODO Fix and remove
                self.peers[peer_index] = peer.clone();
                self.remove_from_db_and_links(peer_index)?;
                self.add_peer_hashmap_links(peer_index, &peer);
                self.add_peer_to_datastore(peer_index, &peer)
            },
            None => {
                // Add new entry
                let peer_index = self.peers.len();
                self.add_peer_hashmap_links(peer_index, &peer);
                self.add_peer_to_datastore(peer_index, &peer)?;
                self.peers.push(peer);
                Ok(())
            },
        }
    }

    /// The peer with the specified public_key will be removed from the PeerManager
    pub fn delete_peer(&mut self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_count = self.peers.len();
        let peer_index = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.remove_from_db_and_links(peer_index)?;
        // If not last element, then move last element into new available slot
        if (peer_count > 1) && (peer_index + 1 < peer_count) {
            let last_index = peer_count - 1;
            self.remove_from_db_and_links(last_index)?;
            let last_peer = self.peers[last_index].clone();
            self.add_peer_hashmap_links(peer_index, &last_peer);
            self.add_peer_to_datastore(peer_index, &last_peer)?;
            self.peers[peer_index] = last_peer;
        }
        self.peers.pop();
        Ok(())
    }

    /// Find the peer with the provided NodeID
    pub fn find_with_node_id(&self, node_id: &NodeId) -> Result<Peer<PubKey>, PeerManagerError> {
        let peer_index = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        Ok(self.peers.index(peer_index).clone())
    }

    /// Find the peer with the provided PublicKey
    pub fn find_with_public_key(&self, public_key: &PubKey) -> Result<Peer<PubKey>, PeerManagerError> {
        let peer_index = *self
            .public_key_hm
            .get(&public_key)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        Ok(self.peers.index(peer_index).clone())
    }

    /// Find the peer with the provided NetAddress
    pub fn find_with_net_address(&self, net_address: &NetAddress) -> Result<Peer<PubKey>, PeerManagerError> {
        let peer_index = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        Ok(self.peers.index(peer_index).clone())
    }

    /// Constructs a single NodeIdentity for the peer corresponding to the provided NodeId
    pub fn direct_identity(&self, node_id: &NodeId) -> Result<Vec<PeerNodeIdentity<PubKey>>, PeerManagerError> {
        let peer_index = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        if self.peers[peer_index].is_banned() {
            Err(PeerManagerError::BannedPeer)
        } else {
            Ok(vec![PeerNodeIdentity::<PubKey>::new(
                node_id.clone(),
                self.peers[peer_index].public_key.clone(),
            )])
        }
    }

    /// Compile a list of all known node identities that can be used for the flood BroadcastStrategy
    pub fn flood_identities(&self) -> Result<Vec<PeerNodeIdentity<PubKey>>, PeerManagerError> {
        // TODO: this list should only contain Communication Nodes
        let mut identities: Vec<PeerNodeIdentity<PubKey>> = Vec::new();
        for peer in &self.peers {
            if !peer.is_banned() {
                identities.push(PeerNodeIdentity::new(peer.node_id.clone(), peer.public_key.clone()));
            }
        }
        Ok(identities)
    }

    /// Compile a list of node identities that can be used for the closest BroadcastStrategy
    pub fn closest_identities(
        &self,
        node_id: NodeId,
        n: usize,
    ) -> Result<Vec<PeerNodeIdentity<PubKey>>, PeerManagerError>
    {
        let mut indices: Vec<usize> = Vec::new();
        let mut dists: Vec<NodeDistance> = Vec::new();
        for i in 0..self.peers.len() {
            if !self.peers[i].is_banned() {
                indices.push(i);
                dists.push(node_id.distance(&self.peers[i].node_id));
            }
        }
        if n > indices.len() {
            return Err(PeerManagerError::InsufficientPeers);
        }
        // Perform partial sort of elements only up to N elements
        let mut nearest_identities: Vec<PeerNodeIdentity<PubKey>> = Vec::with_capacity(n);
        for i in 0..n {
            for j in (i + 1)..indices.len() {
                if dists[i] > dists[j] {
                    dists.swap(i, j);
                    indices.swap(i, j);
                }
            }
            nearest_identities.push(PeerNodeIdentity::<PubKey>::new(
                self.peers[indices[i]].node_id.clone(),
                self.peers[indices[i]].public_key.clone(),
            ));
        }
        Ok(nearest_identities)
    }

    /// Compile a list of node identities that can be used for the random BroadcastStrategy
    pub fn random_identities(&mut self, n: usize) -> Result<Vec<PeerNodeIdentity<PubKey>>, PeerManagerError> {
        // TODO: Send to a random set of Communication Nodes
        let peer_count = self.peers.len();
        let mut indices: Vec<usize> = Vec::new();
        for i in 0..peer_count {
            if !self.peers[i].is_banned() {
                indices.push(i);
            }
        }
        if n > indices.len() {
            return Err(PeerManagerError::InsufficientPeers);
        }
        // Shuffle first n elements
        for i in 0..n {
            let j = self.rng.gen_range(0, indices.len());
            indices.swap(i, j);
        }
        // Compile list of first n shuffled elements
        let mut random_identities: Vec<PeerNodeIdentity<PubKey>> = Vec::with_capacity(n);
        for i in 0..n {
            random_identities.push(PeerNodeIdentity::<PubKey>::new(
                self.peers[indices[i]].node_id.clone(),
                self.peers[indices[i]].public_key.clone(),
            ));
        }
        Ok(random_identities)
    }

    /// Add key pairs to the search hashmaps for a newly added or moved peer
    fn add_peer_hashmap_links(&mut self, index: usize, peer: &Peer<PubKey>) {
        self.node_id_hm.insert(peer.node_id.clone(), index);
        self.public_key_hm.insert(peer.public_key.clone(), index);
        for net_address_with_stats in &peer.addresses.addresses {
            self.net_address_hm
                .insert(net_address_with_stats.net_address.clone(), index);
        }
    }

    /// Add a single peer to the datastore using the provided index as a key
    fn add_peer_to_datastore(&mut self, index: usize, peer: &Peer<PubKey>) -> Result<(), PeerManagerError> {
        if let Some(ref mut datastore) = self.datastore {
            let index_bytes = index.to_binary().map_err(|e| PeerManagerError::SerializationError(e))?;
            let peer_bytes = peer.to_binary().map_err(|e| PeerManagerError::SerializationError(e))?;
            datastore
                .put_raw(&index_bytes, peer_bytes)
                .map_err(|e| PeerManagerError::DatastoreError(e))?;
        }
        Ok(())
    }

    /// Remove the peer specified by a given index from the datastore and remove hashmap keys
    fn remove_from_db_and_links(&mut self, index: usize) -> Result<(), PeerManagerError> {
        let peer_count = self.peers.len();
        if (index > 0) && (index < peer_count) {
            // Remove entry from datastore
            if let Some(ref mut datastore) = self.datastore {
                let index_bytes = index.to_binary().map_err(|e| PeerManagerError::SerializationError(e))?;
                datastore
                    .delete_raw(&index_bytes)
                    .map_err(|e| PeerManagerError::DatastoreError(e))?;
            }
            // Remove hashmap links
            self.node_id_hm.remove(&self.peers[index].node_id);
            self.public_key_hm.remove(&self.peers[index].public_key);
            for net_address_with_stats in &self.peers[index].addresses.addresses {
                self.net_address_hm.remove(&net_address_with_stats.net_address);
            }
            Ok(())
        } else {
            Err(PeerManagerError::IndexOutOfBounds)
        }
    }

    /// Retrieve a single peer from the data store using the provided index
    pub fn get_peer_from_datastore(&self, index: usize) -> Result<Peer<PubKey>, PeerManagerError> {
        match self.datastore {
            Some(ref datastore) => {
                let index_bytes = index.to_binary().map_err(|e| PeerManagerError::SerializationError(e))?;
                match datastore
                    .get_raw(&index_bytes)
                    .map_err(|e| PeerManagerError::DatastoreError(e))?
                {
                    Some(peer_raw) => Peer::<PubKey>::from_binary(peer_raw.as_slice())
                        .map_err(|_| PeerManagerError::DeserializationError),
                    None => Err(PeerManagerError::EmptyDatastoreQuery),
                }
            },
            None => Err(PeerManagerError::DatastoreUndefined),
        }
    }

    /// Enables Thread safe access - Changes the ban flag bit of the peer
    pub fn set_banned(&mut self, node_id: &NodeId, ban_flag: bool) -> Result<(), PeerManagerError> {
        let peer_index = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        Ok(self.peers[peer_index].set_banned(ban_flag))
    }

    /// Enables Thread safe access - Adds a new net address to the peer if it doesn't yet exist
    pub fn add_net_address(&mut self, node_id: &NodeId, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_index = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peers[peer_index]
            .addresses
            .add_net_address(net_address)
            .map_err(|_| PeerManagerError::DataUpdateError)
    }

    /// Enables Thread safe access - Finds and returns the highest priority net address until all connection attempts
    /// for each net address have been reached
    pub fn get_best_net_address(&mut self, node_id: &NodeId) -> Result<NetAddress, PeerManagerError> {
        let peer_index = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peers[peer_index]
            .addresses
            .get_best_net_address()
            .map_err(|_| PeerManagerError::DataUpdateError)
    }

    /// Enables Thread safe access - The average connection latency of the provided net address will be updated to
    /// include the current measured latency sample
    pub fn update_latency(
        &mut self,
        net_address: &NetAddress,
        latency_measurement: Duration,
    ) -> Result<(), PeerManagerError>
    {
        let peer_index = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peers[peer_index]
            .addresses
            .update_latency(net_address, latency_measurement)
            .map_err(|_| PeerManagerError::DataUpdateError)
    }

    /// Enables Thread safe access - Mark that a message was received from the specified net address
    pub fn mark_message_received(&mut self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_index = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peers[peer_index]
            .addresses
            .mark_message_received(net_address)
            .map_err(|_| PeerManagerError::DataUpdateError)
    }

    /// Enables Thread safe access - Mark that a rejected message was received from the specified net address
    pub fn mark_message_rejected(&mut self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_index = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peers[peer_index]
            .addresses
            .mark_message_rejected(net_address)
            .map_err(|_| PeerManagerError::DataUpdateError)
    }

    /// Enables Thread safe access - Mark that a successful connection was established with the specified net address
    pub fn mark_successful_connection_attempt(&mut self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_index = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peers[peer_index]
            .addresses
            .mark_successful_connection_attempt(net_address)
            .map_err(|_| PeerManagerError::DataUpdateError)
    }

    /// Enables Thread safe access - Mark that a connection could not be established with the specified net address
    pub fn mark_failed_connection_attempt(&mut self, net_address: &NetAddress) -> Result<(), PeerManagerError> {
        let peer_index = *self
            .net_address_hm
            .get(&net_address)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peers[peer_index]
            .addresses
            .mark_failed_connection_attempt(net_address)
            .map_err(|_| PeerManagerError::DataUpdateError)
    }

    /// Enables Thread safe access - Finds a peer and if it exists resets all connection attempts on all net address
    /// belonging to that peer
    pub fn reset_connection_attempts(&mut self, node_id: &NodeId) -> Result<(), PeerManagerError> {
        let peer_index = *self
            .node_id_hm
            .get(&node_id)
            .ok_or(PeerManagerError::PeerNotFoundError)?;
        self.peers[peer_index].addresses.reset_connection_attempts();
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::net_address::{net_addresses::NetAddresses, NetAddress},
        peer_manager::peer::PeerFlags,
    };
    use std::fs;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::{
        keyvalue_store::DataStore,
        lmdb::{LMDBBuilder, LMDBStore},
    };

    #[test]
    fn test_add_delete_find_peer() {
        // Clear and setup DB folders
        let test_dir = "./tests/test_peer_storage";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());

        // Setup peer storage
        let mut datastore = LMDBBuilder::new().set_path(test_dir).build().unwrap();
        datastore.connect("default").unwrap();
        let mut peer_storage = PeerStorage::<RistrettoPublicKey, LMDBStore>::new()
            .unwrap()
            .init_persistance_store(datastore)
            .unwrap();

        // Create Peers
        let mut rng = rand::OsRng::new().unwrap();
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address1 = NetAddress::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
        let net_address2 = NetAddress::from("5.6.7.8:8000".parse::<NetAddress>().unwrap());
        let net_address3 = NetAddress::from("5.6.7.8:7000".parse::<NetAddress>().unwrap());
        let mut net_addresses = NetAddresses::from(net_address1.clone());
        net_addresses.add_net_address(&net_address2).unwrap();
        net_addresses.add_net_address(&net_address3).unwrap();
        let peer1: Peer<RistrettoPublicKey> =
            Peer::<RistrettoPublicKey>::new(pk, node_id, net_addresses, PeerFlags::default());

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address4 = NetAddress::from("9.10.11.12:7000".parse::<NetAddress>().unwrap());
        let net_addresses = NetAddresses::from(net_address4.clone());
        let peer2: Peer<RistrettoPublicKey> =
            Peer::<RistrettoPublicKey>::new(pk, node_id, net_addresses, PeerFlags::default());

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address5 = NetAddress::from("13.14.15.16:6000".parse::<NetAddress>().unwrap());
        let net_address6 = NetAddress::from("17.18.19.20:8000".parse::<NetAddress>().unwrap());
        let mut net_addresses = NetAddresses::from(net_address5.clone());
        net_addresses.add_net_address(&net_address6).unwrap();
        let peer3: Peer<RistrettoPublicKey> =
            Peer::<RistrettoPublicKey>::new(pk, node_id, net_addresses, PeerFlags::default());
        // Test adding and searching for peers
        assert!(peer_storage.add_peer(peer1.clone()).is_ok());
        assert!(peer_storage.add_peer(peer2.clone()).is_ok());
        assert!(peer_storage.add_peer(peer3.clone()).is_ok());

        assert_eq!(peer_storage.peers.len(), 3);

        assert_eq!(
            peer_storage.find_with_public_key(&peer1.public_key).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_public_key(&peer2.public_key).unwrap().public_key,
            peer2.public_key
        );
        assert_eq!(
            peer_storage.find_with_public_key(&peer3.public_key).unwrap().public_key,
            peer3.public_key
        );

        assert_eq!(
            peer_storage.find_with_node_id(&peer1.node_id).unwrap().node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage.find_with_node_id(&peer2.node_id).unwrap().node_id,
            peer2.node_id
        );
        assert_eq!(
            peer_storage.find_with_node_id(&peer3.node_id).unwrap().node_id,
            peer3.node_id
        );

        assert_eq!(
            peer_storage.find_with_net_address(&net_address1).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address2).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address3).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address4).unwrap().public_key,
            peer2.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address5).unwrap().public_key,
            peer3.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address6).unwrap().public_key,
            peer3.public_key
        );

        assert_eq!(peer_storage.get_peer_from_datastore(0).unwrap(), peer1);
        assert_eq!(peer_storage.get_peer_from_datastore(1).unwrap(), peer2);
        assert_eq!(peer_storage.get_peer_from_datastore(2).unwrap(), peer3);

        // Test delete of border case peer
        assert!(peer_storage.delete_peer(&peer3.node_id).is_ok());

        assert_eq!(peer_storage.peers.len(), 2);

        assert_eq!(
            peer_storage.find_with_public_key(&peer1.public_key).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_public_key(&peer2.public_key).unwrap().public_key,
            peer2.public_key
        );
        assert!(peer_storage.find_with_public_key(&peer3.public_key).is_err());

        assert_eq!(
            peer_storage.find_with_node_id(&peer1.node_id).unwrap().node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage.find_with_node_id(&peer2.node_id).unwrap().node_id,
            peer2.node_id
        );
        assert!(peer_storage.find_with_node_id(&peer3.node_id).is_err());

        assert_eq!(
            peer_storage.find_with_net_address(&net_address1).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address2).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address3).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address4).unwrap().public_key,
            peer2.public_key
        );
        assert!(peer_storage.find_with_net_address(&net_address5).is_err());
        assert!(peer_storage.find_with_net_address(&net_address6).is_err());

        assert_eq!(peer_storage.get_peer_from_datastore(0).unwrap(), peer1);
        assert_eq!(peer_storage.get_peer_from_datastore(1).unwrap(), peer2);
        assert!(peer_storage.get_peer_from_datastore(2).is_err());

        // Test of delete with moving behaviour
        assert!(peer_storage.add_peer(peer3.clone()).is_ok());
        assert!(peer_storage.delete_peer(&peer2.node_id).is_ok());

        assert_eq!(peer_storage.peers.len(), 2);

        assert_eq!(
            peer_storage.find_with_public_key(&peer1.public_key).unwrap().public_key,
            peer1.public_key
        );
        assert!(peer_storage.find_with_public_key(&peer2.public_key).is_err());
        assert_eq!(
            peer_storage.find_with_public_key(&peer3.public_key).unwrap().public_key,
            peer3.public_key
        );

        assert_eq!(
            peer_storage.find_with_node_id(&peer1.node_id).unwrap().node_id,
            peer1.node_id
        );
        assert!(peer_storage.find_with_node_id(&peer2.node_id).is_err());
        assert_eq!(
            peer_storage.find_with_node_id(&peer3.node_id).unwrap().node_id,
            peer3.node_id
        );

        assert_eq!(
            peer_storage.find_with_net_address(&net_address1).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address2).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address3).unwrap().public_key,
            peer1.public_key
        );
        assert!(peer_storage.find_with_net_address(&net_address4).is_err());
        assert_eq!(
            peer_storage.find_with_net_address(&net_address5).unwrap().public_key,
            peer3.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address6).unwrap().public_key,
            peer3.public_key
        );

        assert_eq!(peer_storage.get_peer_from_datastore(0).unwrap(), peer1);
        assert_eq!(peer_storage.get_peer_from_datastore(1).unwrap(), peer3);
        assert!(peer_storage.get_peer_from_datastore(2).is_err());

        // Clear up DB folders
        // assert!(datastore.close().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }

    #[test]
    fn test_persistence_storage() {
        // Clear and setup DB folders
        let test_dir = "./tests/test_peer_storage2";
        if std::fs::metadata(test_dir).is_ok() {
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
        assert!(fs::create_dir(test_dir).is_ok());

        // Setup peer storage
        let mut datastore = LMDBBuilder::new().set_path(test_dir).build().unwrap();
        datastore.connect("default").unwrap();
        let mut peer_storage = PeerStorage::<RistrettoPublicKey, LMDBStore>::new()
            .unwrap()
            .init_persistance_store(datastore)
            .unwrap();

        // Create Peers
        let mut rng = rand::OsRng::new().unwrap();
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address1 = NetAddress::from("1.2.3.4:8000".parse::<NetAddress>().unwrap());
        let net_address2 = NetAddress::from("5.6.7.8:8000".parse::<NetAddress>().unwrap());
        let mut net_addresses = NetAddresses::from(net_address1.clone());
        assert!(net_addresses.add_net_address(&net_address2).is_ok());
        let peer1: Peer<RistrettoPublicKey> =
            Peer::<RistrettoPublicKey>::new(pk, node_id, net_addresses, PeerFlags::default());

        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut rng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_address3 = NetAddress::from("9.10.11.12:7000".parse::<NetAddress>().unwrap());
        let net_addresses = NetAddresses::from(net_address3.clone());
        let peer2: Peer<RistrettoPublicKey> =
            Peer::<RistrettoPublicKey>::new(pk, node_id, net_addresses, PeerFlags::default());

        // Add peers to peer store
        assert!(peer_storage.add_peer(peer1.clone()).is_ok());
        assert!(peer_storage.add_peer(peer2.clone()).is_ok());

        // Reconnect to peer storage without deleting datastore
        let mut datastore = LMDBBuilder::new().set_path(test_dir).build().unwrap();
        datastore.connect("default").unwrap();
        let peer_storage = PeerStorage::<RistrettoPublicKey, LMDBStore>::new()
            .unwrap()
            .init_persistance_store(datastore)
            .unwrap();

        // Check that peer vector was restored
        assert_eq!(peer_storage.peers.len(), 2);
        // Check that the hashmap links were restored
        assert_eq!(
            peer_storage.find_with_public_key(&peer1.public_key).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_public_key(&peer2.public_key).unwrap().public_key,
            peer2.public_key
        );

        assert_eq!(
            peer_storage.find_with_node_id(&peer1.node_id).unwrap().node_id,
            peer1.node_id
        );
        assert_eq!(
            peer_storage.find_with_node_id(&peer2.node_id).unwrap().node_id,
            peer2.node_id
        );

        assert_eq!(
            peer_storage.find_with_net_address(&net_address1).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address2).unwrap().public_key,
            peer1.public_key
        );
        assert_eq!(
            peer_storage.find_with_net_address(&net_address3).unwrap().public_key,
            peer2.public_key
        );
        // Check that the datastore as the correct peers
        assert_eq!(peer_storage.get_peer_from_datastore(0).unwrap(), peer1);
        assert_eq!(peer_storage.get_peer_from_datastore(1).unwrap(), peer2);

        // Clear up DB folders
        // assert!(datastore.close().is_ok());
        let _no_val = fs::remove_dir_all(test_dir);
        if std::fs::metadata(test_dir).is_ok() {
            println!("Database file handles not released, still open in {:?}!", test_dir);
            assert!(fs::remove_dir_all(test_dir).is_ok());
        }
    }
}

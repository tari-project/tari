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

use crate::peer_manager::{peer_id::PeerId, NodeId, Peer, PeerManagerError};
use std::cmp::min;
use tari_storage::{IterationResult, KeyValueStore};
use chrono::NaiveDateTime;

type Predicate<'a, A> = Box<dyn FnMut(&A) -> bool + Send + 'a>;

/// Sort options for `PeerQuery`
#[derive(Debug, Clone)]
pub enum PeerQuerySortBy<'a> {
    /// No sorting
    None,
    /// Sort by distance from a given node id
    DistanceFrom(&'a NodeId),
    /// Sort by last connected
    LastConnected
}

impl Default for PeerQuerySortBy<'_> {
    fn default() -> Self {
        PeerQuerySortBy::None
    }
}

/// Represents a query which can be performed on the peer database
#[derive(Default)]
pub struct PeerQuery<'a> {
    select_predicate: Option<Predicate<'a, Peer>>,
    limit: Option<usize>,
    sort_by: PeerQuerySortBy<'a>,
    until_predicate: Option<Predicate<'a, [Peer]>>,
}

impl<'a> PeerQuery<'a> {
    /// Create a new `PeerQuery`
    pub fn new() -> Self {
        Default::default()
    }

    /// Set the selection predicate. This predicate should return `true` to include a `Peer`
    /// in the result set.
    pub fn select_where<F>(mut self, select_predicate: F) -> Self
    where F: FnMut(&Peer) -> bool + Send + 'a {
        self.select_predicate = Some(Box::new(select_predicate));
        self
    }

    /// Set a limit on the number of results returned
    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Sort by the given `PeerSortBy` criteria
    pub fn sort_by(mut self, sort_by: PeerQuerySortBy<'a>) -> Self {
        self.sort_by = sort_by;
        self
    }

    pub fn until<F>(mut self, until_predicate: F) -> Self
    where F: FnMut(&[Peer]) -> bool + Send + 'a {
        self.until_predicate = Some(Box::new(until_predicate));
        self
    }

    /// Returns a `PeerQueryExecutor` with this `PeerQuery`
    pub(super) fn executor<DS>(self, store: &DS) -> PeerQueryExecutor<'a, '_, DS>
    where DS: KeyValueStore<PeerId, Peer> {
        PeerQueryExecutor::new(self, store)
    }

    /// Returns true if the given limit is within the specified limit. If the limit
    /// was not specified, this always returns true
    fn within_limit(&self, limit: usize) -> bool {
        self.limit.map(|inner_limit| inner_limit > limit).unwrap_or(true)
    }

    /// Returns true if the specified select predicate returns true. If the
    /// select predicate was not specified, this always returns true.
    fn is_selected(&mut self, peer: &Peer) -> bool {
        self.select_predicate
            .as_mut()
            .map(|predicate| (predicate)(peer))
            .unwrap_or(true)
    }

    /// Returns true if the result collector should stop early, otherwise false
    fn should_stop(&mut self, peers: &[Peer]) -> bool {
        self.until_predicate
            .as_mut()
            .map(|predicate| (predicate)(peers))
            .unwrap_or(false)
    }
}

/// This struct executes the query using the given store
pub(super) struct PeerQueryExecutor<'a, 'b, DS> {
    query: PeerQuery<'a>,
    store: &'b DS,
}

impl<'a, 'b, DS> PeerQueryExecutor<'a, 'b, DS>
where DS: KeyValueStore<PeerId, Peer>
{
    pub fn new(query: PeerQuery<'a>, store: &'b DS) -> Self {
        Self { query, store }
    }

    pub fn get_results(&mut self) -> Result<Vec<Peer>, PeerManagerError> {
        match self.query.sort_by {
            PeerQuerySortBy::None => self.get_query_results(),
            PeerQuerySortBy::DistanceFrom(node_id) => self.get_distance_sorted_results(node_id),
            PeerQuerySortBy::LastConnected => self.get_last_connected_sorted_results()
        }
    }

    pub fn get_distance_sorted_results(&mut self, node_id: &NodeId) -> Result<Vec<Peer>, PeerManagerError> {
       self.get_sorted_results(|peer| peer.node_id.distance(node_id))
    }

    pub fn get_last_connected_sorted_results(&mut self) -> Result<Vec<Peer>, PeerManagerError> {
        self.get_sorted_results(|peer| peer.connection_stats.last_connected_at.unwrap_or_else(|| NaiveDateTime::from_timestamp(0,0)))
    }

    fn get_sorted_results<T, F>(&mut self, sort_key: F)  -> Result<Vec<Peer>, PeerManagerError>
        where T: Ord, F: Fn(&Peer) -> T
    {
        let mut peer_keys = Vec::new();
        let mut sort_values = Vec::new();
        self.store
            .for_each_ok(|(peer_key, peer)| {
                if self.query.is_selected(&peer) {
                    peer_keys.push(peer_key);
                    sort_values.push(sort_key(&peer));
                }

                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;

        // Use all available peers up to a maximum of N
        let max_available = self
            .query
            .limit
            .map(|limit| min(peer_keys.len(), limit))
            .unwrap_or_else(|| peer_keys.len());
        if max_available == 0 {
            return Ok(Vec::new());
        }

        // Perform partial sort of elements only up to N elements
        let mut selected_peers = Vec::with_capacity(max_available);
        for i in 0..max_available {
            for j in (i + 1)..peer_keys.len() {
                if sort_values[i] > sort_values[j] {
                    sort_values.swap(i, j);
                    peer_keys.swap(i, j);
                }
            }
            let peer = self
                .store
                .get(&peer_keys[i])
                .map_err(PeerManagerError::DatabaseError)?
                .ok_or(PeerManagerError::PeerNotFoundError)?;

            selected_peers.push(peer);

            if self.query.should_stop(&selected_peers) {
                break;
            }
        }

        Ok(selected_peers)
    }

    pub fn get_query_results(&mut self) -> Result<Vec<Peer>, PeerManagerError> {
        let mut selected_peers = match self.query.limit {
            Some(n) => Vec::with_capacity(n),
            None => Vec::new(),
        };

        self.store
            .for_each_ok(|(_, peer)| {
                if self.query.within_limit(selected_peers.len()) && !self.query.should_stop(&selected_peers) {
                    if self.query.is_selected(&peer) {
                        selected_peers.push(peer);
                    }
                } else {
                    return IterationResult::Break;
                }

                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;

        Ok(selected_peers)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        net_address::MultiaddressesWithStats,
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
            PeerFeatures,
        },
    };
    use multiaddr::Multiaddr;
    use rand::rngs::OsRng;
    use std::{iter::repeat_with, time::Duration};
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HashmapDatabase;

    fn create_test_peer(ban_flag: bool) -> Peer {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let node_id = NodeId::from_key(&pk).unwrap();
        let net_addresses = MultiaddressesWithStats::from("/ip4/1.2.3.4/tcp/8000".parse::<Multiaddr>().unwrap());
        let mut peer = Peer::new(
            pk,
            node_id,
            net_addresses,
            PeerFlags::default(),
            PeerFeatures::MESSAGE_PROPAGATION,
            Default::default(),
            Default::default(),
        );
        if ban_flag {
            peer.ban_for(Duration::from_secs(1000), "".to_string());
        }
        peer
    }

    #[test]
    fn limit_query() {
        // Create peer manager with random peers
        let mut sample_peers = Vec::new();
        // Create 20 peers were the 1st and last one is bad
        sample_peers.push(create_test_peer(true));
        let db = HashmapDatabase::new();
        let mut id_counter = 0;

        repeat_with(|| create_test_peer(false)).take(5).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        let peers = PeerQuery::new().limit(4).executor(&db).get_results().unwrap();

        assert_eq!(peers.len(), 4);
    }

    #[test]
    fn select_where_query() {
        // Create peer manager with random peers
        let mut sample_peers = Vec::new();
        // Create 20 peers were the 1st and last one is bad
        let _rng = rand::rngs::OsRng;
        sample_peers.push(create_test_peer(true));
        let db = HashmapDatabase::new();
        let mut id_counter = 0;

        repeat_with(|| create_test_peer(true)).take(2).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        repeat_with(|| create_test_peer(false)).take(5).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        let peers = PeerQuery::new()
            .select_where(|peer| !peer.is_banned())
            .executor(&db)
            .get_results()
            .unwrap();

        assert_eq!(peers.len(), 5);
        assert!(peers.iter().all(|peer| !peer.is_banned()));
    }

    #[test]
    fn select_where_limit_query() {
        // Create peer manager with random peers
        let mut sample_peers = Vec::new();
        // Create 20 peers were the 1st and last one is bad
        let _rng = rand::rngs::OsRng;
        sample_peers.push(create_test_peer(true));
        let db = HashmapDatabase::new();
        let mut id_counter = 0;

        repeat_with(|| create_test_peer(true)).take(3).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        repeat_with(|| create_test_peer(false)).take(5).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        let peers = PeerQuery::new()
            .select_where(|peer| peer.is_banned())
            .limit(2)
            .executor(&db)
            .get_results()
            .unwrap();

        assert_eq!(peers.len(), 2);
        assert!(peers.iter().all(|peer| peer.is_banned()));

        let peers = PeerQuery::new()
            .select_where(|peer| !peer.is_banned())
            .limit(100)
            .executor(&db)
            .get_results()
            .unwrap();

        assert_eq!(peers.len(), 5);
        assert!(peers.iter().all(|peer| !peer.is_banned()));
    }

    #[test]
    fn select_where_until_query() {
        // Create peer manager with random peers
        let mut sample_peers = Vec::new();
        // Create 20 peers were the 1st and last one is bad
        let _rng = rand::rngs::OsRng;
        sample_peers.push(create_test_peer(true));
        let db = HashmapDatabase::new();
        let mut id_counter = 0;

        repeat_with(|| create_test_peer(true)).take(3).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        repeat_with(|| create_test_peer(false)).take(5).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        let peers = PeerQuery::new()
            .select_where(|peer| !peer.is_banned())
            .until(|peers| peers.len() == 2)
            .executor(&db)
            .get_results()
            .unwrap();

        assert_eq!(peers.len(), 2);
        assert!(peers.iter().all(|peer| !peer.is_banned()));

        let peers = PeerQuery::new()
            .until(|peers| peers.len() == 100)
            .executor(&db)
            .get_results()
            .unwrap();

        assert_eq!(peers.len(), 8);
    }

    #[test]
    fn sort_by_query() {
        // Create peer manager with random peers
        let mut sample_peers = Vec::new();
        // Create 20 peers were the 1st and last one is bad
        let _rng = rand::rngs::OsRng;
        sample_peers.push(create_test_peer(true));
        let db = HashmapDatabase::new();
        let mut id_counter = 0;

        repeat_with(|| create_test_peer(true)).take(3).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        repeat_with(|| create_test_peer(false)).take(5).for_each(|peer| {
            db.insert(id_counter, peer).unwrap();
            id_counter += 1;
        });

        let node_id = NodeId::default();

        let peers = PeerQuery::new()
            .sort_by(PeerQuerySortBy::DistanceFrom(&node_id))
            .limit(2)
            .executor(&db)
            .get_results()
            .unwrap();

        assert_eq!(peers.len(), 2);

        db.for_each_ok(|(_, current_peer)| {
            // Exclude selected peers
            if !peers.contains(&current_peer) {
                // Every selected peer'a distance from node_id is less than every other peer'a distance from node_id
                for selected_peer in &peers {
                    assert!(selected_peer.node_id.distance(&node_id) <= current_peer.node_id.distance(&node_id));
                }
            }
            IterationResult::Continue
        })
        .unwrap();
    }
}

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

use std::cmp::{min, Ordering};

use tari_storage::{IterationResult, KeyValueStore};

use crate::peer_manager::{peer_id::PeerId, NodeId, Peer, PeerManagerError};

type Predicate<'a, A> = Box<dyn FnMut(&A) -> bool + Send + 'a>;

/// Sort options for `PeerQuery`
#[derive(Default, Debug, Clone)]
pub enum PeerQuerySortBy<'a> {
    /// No sorting
    #[default]
    None,
    /// Sort by distance from a given node id
    DistanceFrom(&'a NodeId),
    /// Sort by last connected
    LastConnected,
    /// Sort by distance from a given node followed by last connected
    DistanceFromLastConnected(&'a NodeId),
}

/// Represents a query which can be performed on the peer database
#[derive(Default)]
pub struct PeerQuery<'a> {
    select_predicate: Option<Predicate<'a, Peer>>,
    limit: Option<usize>,
    sort_by: PeerQuerySortBy<'a>,
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
    #[allow(clippy::wrong_self_convention)]
    fn is_selected(&mut self, peer: &Peer) -> bool {
        self.select_predicate
            .as_mut()
            .map(|predicate| (predicate)(peer))
            .unwrap_or(true)
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
            PeerQuerySortBy::None => self.get_unsorted_results(),
            PeerQuerySortBy::DistanceFrom(node_id) => self.get_distance_sorted_results(node_id),
            PeerQuerySortBy::LastConnected => self.get_last_connected_sorted_results(),
            PeerQuerySortBy::DistanceFromLastConnected(node_id) => {
                self.get_distance_then_last_connected_results(node_id)
            },
        }
    }

    pub fn get_last_connected_sorted_results(&mut self) -> Result<Vec<Peer>, PeerManagerError> {
        self.get_sorted_results(last_seen_compare_desc)
    }

    pub fn get_distance_sorted_results(&mut self, node_id: &NodeId) -> Result<Vec<Peer>, PeerManagerError> {
        self.get_sorted_results(|a, b| {
            let a = a.node_id.distance(node_id);
            let b = b.node_id.distance(node_id);
            // Sort ascending
            a.cmp(&b)
        })
    }

    fn get_distance_then_last_connected_results(&mut self, node_id: &NodeId) -> Result<Vec<Peer>, PeerManagerError> {
        let mut peers = self.get_distance_sorted_results(node_id)?;
        peers.sort_by(last_seen_compare_desc);
        Ok(peers)
    }

    fn get_sorted_results<F>(&mut self, compare: F) -> Result<Vec<Peer>, PeerManagerError>
    where F: FnMut(&Peer, &Peer) -> Ordering {
        let mut selected_peers = Vec::new();
        self.store
            .for_each_ok(|(_, peer)| {
                if self.query.is_selected(&peer) {
                    selected_peers.push(peer);
                }

                IterationResult::Continue
            })
            .map_err(PeerManagerError::DatabaseError)?;

        // Use all available peers up to a maximum of N
        let max_available = self
            .query
            .limit
            .map(|limit| min(selected_peers.len(), limit))
            .unwrap_or_else(|| selected_peers.len());
        if max_available == 0 {
            return Ok(Vec::new());
        }

        selected_peers.sort_by(compare);
        selected_peers.truncate(max_available);

        Ok(selected_peers)
    }

    pub fn get_unsorted_results(&mut self) -> Result<Vec<Peer>, PeerManagerError> {
        let mut selected_peers = self.query.limit.map(Vec::with_capacity).unwrap_or_default();

        self.store
            .for_each_ok(|(_, peer)| {
                if self.query.within_limit(selected_peers.len()) {
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

fn last_seen_compare_desc(a: &Peer, b: &Peer) -> Ordering {
    match (a.last_seen(), b.last_seen()) {
        // Sort descending
        (Some(a), Some(b)) => b.cmp(&a),
        // Nones go to the end
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod test {
    use std::{iter::repeat_with, time::Duration};

    use multiaddr::Multiaddr;
    use rand::rngs::OsRng;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
    use tari_storage::HashmapDatabase;

    use super::*;
    use crate::{
        net_address::{MultiaddressesWithStats, PeerAddressSource},
        peer_manager::{
            node_id::NodeId,
            peer::{Peer, PeerFlags},
            PeerFeatures,
        },
    };

    fn create_test_peer(ban_flag: bool) -> Peer {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut OsRng);
        let node_id = NodeId::from_key(&pk);
        let net_addresses = MultiaddressesWithStats::from_addresses_with_source(
            vec!["/ip4/1.2.3.4/tcp/8000".parse::<Multiaddr>().unwrap()],
            &PeerAddressSource::Config,
        );
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
        // Create some good peers
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
        // Create some good and bad peers
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
        // Create some good and bad peers
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
    fn sort_by_query() {
        // Create some good and bad peers
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

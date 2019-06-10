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

use std::collections::HashMap;

use crate::{connection::PeerConnection, peer_manager::node_id::NodeId};

use std::sync::{Arc, RwLock};

lazy_static! {
    static ref PORT_ALLOCATIONS: RwLock<Vec<u16>> = RwLock::new(vec![]);
}

pub trait Repository<I, T> {
    fn get(&self, id: &I) -> Option<Arc<T>>;
    fn has(&self, id: &I) -> bool;
    fn size(&self) -> usize;
    fn insert(&mut self, id: I, value: Arc<T>);
    fn remove(&mut self, id: &I) -> Option<Arc<T>>;
}

#[derive(Default)]
pub(super) struct ConnectionRepository {
    entries: HashMap<NodeId, Arc<PeerConnection>>,
}

impl Repository<NodeId, PeerConnection> for ConnectionRepository {
    fn get(&self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        self.entries.get(node_id).map(|entry| entry.clone())
    }

    fn has(&self, node_id: &NodeId) -> bool {
        self.entries.contains_key(node_id)
    }

    fn size(&self) -> usize {
        self.entries.values().count()
    }

    fn insert(&mut self, node_id: NodeId, entry: Arc<PeerConnection>) {
        self.entries.insert(node_id, entry);
    }

    fn remove(&mut self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        self.entries.remove(node_id)
    }
}

impl ConnectionRepository {
    pub fn count_where<P>(&self, predicate: P) -> usize
    where P: FnMut(&&Arc<PeerConnection>) -> bool {
        self.entries.values().filter(predicate).count()
    }

    pub fn for_each(&self, mut f: impl FnMut(&Arc<PeerConnection>)) {
        for entry in self.entries.values() {
            f(entry);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::OsRng;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    fn make_node_id() -> NodeId {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut OsRng::new().unwrap());
        NodeId::from_key(&pk).unwrap()
    }

    fn make_repo_with_connections(n: usize) -> (ConnectionRepository, Vec<NodeId>) {
        let mut repo = ConnectionRepository::default();
        let mut node_ids = vec![];
        for _i in 0..n {
            let node_id = make_node_id();
            let conn = Arc::new(PeerConnection::new());
            repo.insert(node_id.clone(), conn.clone());
            node_ids.push(node_id);
        }
        (repo, node_ids)
    }

    #[test]
    fn insert_get_remove() {
        let (mut repo, node_ids) = make_repo_with_connections(2);
        let unknown_node_id = make_node_id();

        // Retrieve
        assert!(repo.has(&node_ids[0]));
        assert!(repo.has(&node_ids[1]));
        assert!(!repo.has(&unknown_node_id));

        assert!(repo.get(&node_ids[0]).is_some());
        assert!(repo.get(&node_ids[1]).is_some());
        assert!(repo.get(&unknown_node_id).is_none());

        // Remove
        assert!(repo.remove(&node_ids[0]).is_some());
        assert!(repo.remove(&unknown_node_id).is_none());

        assert!(!repo.has(&node_ids[0]));
        assert!(repo.has(&node_ids[1]));
        assert!(!repo.has(&unknown_node_id));
    }

    #[test]
    fn for_each() {
        let (repo, _) = make_repo_with_connections(3);

        let mut count = 0;
        repo.for_each(|_| {
            count += 1;
        });

        assert_eq!(3, count);
    }

    #[test]
    fn count_where() {
        let (repo, _) = make_repo_with_connections(4);

        let mut count = 0;
        let total = repo.count_where(|_| {
            count += 1;
            count % 2 == 0
        });

        assert_eq!(2, total);
    }
}

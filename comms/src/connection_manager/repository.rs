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

use crate::{
    connection::{Direction, NetAddress, PeerConnection},
    peer_manager::node_id::NodeId,
};

use std::sync::{Arc, RwLock};

lazy_static! {
    static ref PORT_ALLOCATIONS: RwLock<Vec<u16>> = RwLock::new(vec![]);
}

#[derive(Clone)]
pub(super) struct PeerConnectionEntry {
    pub(super) connection: Arc<PeerConnection>,
    pub(super) address: NetAddress,
    pub(super) direction: Direction,
}

pub trait Repository<I, T> {
    fn get(&self, id: &I) -> Option<Arc<T>>;
    fn has(&self, id: &I) -> bool;
    fn size(&self) -> usize;
    fn insert(&mut self, id: I, value: T);
    fn remove(&mut self, id: &I) -> Option<Arc<T>>;
}

#[derive(Default)]
pub(super) struct ConnectionRepository {
    entries: HashMap<NodeId, Arc<PeerConnectionEntry>>,
}

impl Repository<NodeId, PeerConnectionEntry> for ConnectionRepository {
    fn get(&self, node_id: &NodeId) -> Option<Arc<PeerConnectionEntry>> {
        self.entries.get(node_id).map(|entry| entry.clone())
    }

    fn has(&self, node_id: &NodeId) -> bool {
        self.entries.contains_key(node_id)
    }

    fn size(&self) -> usize {
        self.entries.values().count()
    }

    fn insert(&mut self, node_id: NodeId, entry: PeerConnectionEntry) {
        if entry.direction == Direction::Inbound {
            if let Some(port) = entry.address.maybe_port() {
                self.allocate_local_port(port);
            }
        }

        self.entries.insert(node_id, Arc::new(entry));
    }

    fn remove(&mut self, node_id: &NodeId) -> Option<Arc<PeerConnectionEntry>> {
        match self.entries.get(node_id) {
            Some(entry) => {
                match entry.direction {
                    Direction::Inbound => {
                        if let Some(port) = entry.address.maybe_port() {
                            self.release_port(port);
                        }
                    },
                    _ => {},
                }
                self.entries.remove(node_id)
            },
            None => None,
        }
    }
}

impl ConnectionRepository {
    pub fn count_where<P>(&self, predicate: P) -> usize
    where P: FnMut(&&Arc<PeerConnectionEntry>) -> bool {
        self.entries.values().filter(predicate).count()
    }

    pub fn for_each(&self, f: impl Fn(&Arc<PeerConnectionEntry>)) {
        for entry in self.entries.values() {
            f(entry);
        }
    }

    pub fn allocate_local_port(&mut self, port: u16) {
        let mut lock = acquire_write_lock!(PORT_ALLOCATIONS);
        if !lock.contains(&port) {
            lock.push(port);
        }
    }

    fn release_port(&mut self, port: u16) {
        let mut lock = acquire_write_lock!(PORT_ALLOCATIONS);
        lock.iter().position(|p| *p == port).map(|idx| lock.remove(idx));
    }

    pub fn is_port_allocated(&self, port: u16) -> bool {
        let lock = acquire_read_lock!(PORT_ALLOCATIONS);
        lock.contains(&port)
    }

    pub fn port_allocation_count(&self) -> usize {
        let lock = acquire_read_lock!(PORT_ALLOCATIONS);
        lock.len()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use rand::OsRng;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    fn make_peer_connection_entry(address: NetAddress, direction: Direction) -> PeerConnectionEntry {
        PeerConnectionEntry {
            connection: Arc::new(PeerConnection::new()),
            address,
            direction,
        }
    }

    fn make_node_id() -> NodeId {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut OsRng::new().unwrap());
        NodeId::from_key(&pk).unwrap()
    }

    #[test]
    fn insert_get_remove() {
        let node_id1 = make_node_id();
        let node_id2 = make_node_id();
        let node_id3 = make_node_id();

        let conn1 = make_peer_connection_entry("127.0.0.1:9000".parse().unwrap(), Direction::Inbound);

        let mut repo = ConnectionRepository::default();

        // Create
        repo.insert(node_id1.clone(), conn1.clone());
        repo.insert(node_id2.clone(), conn1.clone());

        // Retrieve
        assert!(repo.has(&node_id1));
        assert!(repo.has(&node_id2));
        assert!(!repo.has(&node_id3));

        assert!(repo.get(&node_id1).is_some());
        assert!(repo.get(&node_id2).is_some());
        assert!(repo.get(&node_id3).is_none());

        // Remove
        assert!(repo.remove(&node_id1).is_some());
        assert!(repo.remove(&node_id3).is_none());

        assert!(!repo.has(&node_id1));
        assert!(repo.has(&node_id2));
        assert!(!repo.has(&node_id3));
    }

    #[test]
    fn port_allocated() {
        let node_id = make_node_id();
        let test_port = 24321;
        let conn = make_peer_connection_entry(format!("127.0.0.1:{}", test_port).parse().unwrap(), Direction::Inbound);
        let mut repo = ConnectionRepository::default();
        {
            let pa_lock = PORT_ALLOCATIONS.read().unwrap();
            if pa_lock.contains(&test_port) {
                panic!("{} port used for testing is already in use", test_port);
            }

            assert!(!repo.is_port_allocated(test_port));
            assert_eq!(pa_lock.len(), repo.port_allocation_count());
        }
        {
            // Create
            repo.insert(node_id, conn.clone());
            let pa_lock = PORT_ALLOCATIONS.read().unwrap();
            assert!(repo.is_port_allocated(test_port));
            assert_eq!(pa_lock.len(), repo.port_allocation_count());
        }
    }

    // TODO: Test atomicity of the repository
    //    #[test]
    //    fn port_allocated_threaded() {
    //        let node_ids = repeat_with(make_node_id).take(3).collect::<Vec<NodeId>>();
    //
    //        let repo = Arc::new(ConnectionRepository::new());
    //
    //        let mut handles = vec![];
    //        for i in 0..3 {
    //            let mut thread_repo = repo.clone();
    //            handles.push(thread::spawn(move || {
    //                let conn = make_peer_connection_entry(format!("127.0.0.1:{}", 9000 + i).parse().unwrap(),
    //                                                      Direction::Inbound);
    //                thread_repo.insert(node_ids[i].clone(), conn);
    //
    //                if i > 0 {
    //                    thread_repo.remove(&node_ids[i - 1]);
    //                    if thread_repo.is_port_allocated(9000 + (i - 1) as u16) {
    //                        return Err("failed");
    //                    }
    //                }
    //
    //                Ok(())
    //            }));
    //        }
    //
    //        assert!(handles.iter().map(|h| h.join()).all(|r| r.is_ok()));
    //    }
}

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

use log::*;

use super::{
    error::ConnectionManagerError,
    repository::{ConnectionRepository, PeerConnectionEntry, Repository},
    types::PeerConnectionJoinHandle,
    Result,
};

use crate::{
    connection::{PeerConnection, PeerConnectionState},
    peer_manager::node_id::NodeId,
};

use crate::connection::ConnectionError;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

const LOG_TARGET: &'static str = "comms::connection_manager::connections";

/// Stores, and establishes the live peer connections
pub(super) struct LivePeerConnections {
    repository: RwLock<ConnectionRepository>,
    connection_thread_handles: RwLock<HashMap<NodeId, PeerConnectionJoinHandle>>,
}

impl LivePeerConnections {
    /// Create a new live peer connection
    pub fn new() -> Self {
        Self {
            repository: RwLock::new(ConnectionRepository::default()),
            connection_thread_handles: RwLock::new(HashMap::new()),
        }
    }

    /// Get a connection byy node id
    pub fn get_connection(&self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        self.atomic_read(|lock| lock.get(node_id).map(|entry| entry.connection.clone()))
    }

    /// Get a connection by node id only if it is active
    pub fn get_active_connection(&self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        self.get_connection_if(node_id, |connection| connection.is_active())
    }

    /// Get number of active connections
    pub fn get_active_connection_count(&self) -> usize {
        self.atomic_read(|repo| repo.count_where(|entry| entry.connection.is_active()))
    }

    /// Get the state for a connection
    pub fn get_connection_state(&self, node_id: &NodeId) -> Option<PeerConnectionState> {
        self.get_connection(node_id).and_then(|conn| Some(conn.get_state()))
    }

    /// Add a connection to live peer connections
    pub fn add_connection(&self, node_id: NodeId, entry: Arc<PeerConnectionEntry>, handle: PeerConnectionJoinHandle) {
        acquire_write_lock!(self.connection_thread_handles).insert(node_id.clone(), handle);

        self.atomic_write(|mut repo| {
            repo.insert(node_id, entry.into());
        })
    }

    /// If the connection exists, it is removed, shut down and returned. Otherwise
    /// `ConnectionManagerError::PeerConnectionNotFound` is returned
    pub fn drop_connection(&self, node_id: &NodeId) -> Result<(Arc<PeerConnection>, Option<PeerConnectionJoinHandle>)> {
        let conn = self.atomic_write(|mut repo| {
            repo.remove(node_id)
                .ok_or(ConnectionManagerError::PeerConnectionNotFound)
                .map(|entry| entry.connection.clone())
        })?;

        let handle = acquire_write_lock!(self.connection_thread_handles).remove(node_id);

        debug!(target: LOG_TARGET, "Dropping connection for NodeID={}", node_id);

        if conn.is_active() {
            conn.shutdown().map_err(ConnectionManagerError::ConnectionError)?;
        }
        Ok((conn, handle))
    }

    /// Send a shutdown signal to all peer connections, returning their worker thread handles
    pub fn shutdown_all(self) -> HashMap<NodeId, PeerConnectionJoinHandle> {
        info!(target: LOG_TARGET, "Shutting down all peer connections");
        self.atomic_read(|repo| {
            repo.for_each(|entry| {
                let _ = entry.connection.shutdown();
            });
        });

        acquire_lock!(self.connection_thread_handles, into_inner)
    }

    /// Send a shutdown signal to all peer connections, and wait for all of them to
    /// shut down, returning the result of the shutdown. Warning: If a PeerConnection worker
    /// is deadlocked, this method may never return.
    pub fn shutdown_joined(self) -> Vec<std::result::Result<(), ConnectionError>> {
        let handles = self.shutdown_all();

        let mut results = vec![];
        for (_, handle) in handles.into_iter() {
            match handle.join() {
                Ok(result) => results.push(result),
                Err(err) => {
                    error!(target: LOG_TARGET, "Failed to join: {:?}", err);
                },
            }
        }

        results
    }

    fn atomic_write<F, T>(&self, f: F) -> T
    where F: FnOnce(RwLockWriteGuard<ConnectionRepository>) -> T {
        let lock = acquire_write_lock!(self.repository);
        f(lock)
    }

    fn atomic_read<F, T>(&self, f: F) -> T
    where F: FnOnce(RwLockReadGuard<ConnectionRepository>) -> T {
        let lock = acquire_read_lock!(self.repository);
        f(lock)
    }

    fn get_connection_if<P>(&self, node_id: &NodeId, predicate: P) -> Option<Arc<PeerConnection>>
    where P: FnOnce(&Arc<PeerConnection>) -> bool {
        self.get_connection(node_id).filter(predicate)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use rand::OsRng;
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    use std::thread;

    fn make_join_handle() -> PeerConnectionJoinHandle {
        thread::spawn(move || Ok(()))
    }

    fn make_peer_connection_entry() -> Arc<PeerConnectionEntry> {
        Arc::new(PeerConnectionEntry {
            connection: Arc::new(PeerConnection::new()),
            address: "127.0.0.1:0".parse().unwrap(),
        })
    }

    fn make_node_id() -> NodeId {
        let (_sk, pk) = RistrettoPublicKey::random_keypair(&mut OsRng::new().unwrap());
        NodeId::from_key(&pk).unwrap()
    }

    #[test]
    fn new() {
        let connections = LivePeerConnections::new();
        assert_eq!(0, connections.get_active_connection_count());
    }

    #[test]
    fn crud() {
        let connections = LivePeerConnections::new();

        let node_id = make_node_id();
        let entry = make_peer_connection_entry();
        let join_handle = make_join_handle();

        connections.add_connection(node_id.clone(), entry, join_handle);
        assert!(connections.get_active_connection(&node_id).is_none());
        assert_eq!(
            1,
            acquire_read_lock!(connections.connection_thread_handles)
                .values()
                .count()
        );
        connections.get_connection(&node_id).unwrap();
        connections.get_connection_state(&node_id).unwrap();
        connections.drop_connection(&node_id).unwrap();
        assert_eq!(
            0,
            acquire_read_lock!(connections.connection_thread_handles)
                .values()
                .count()
        );

        assert_eq!(0, connections.get_active_connection_count());
    }

    #[test]
    fn drop_connection_fail() {
        let connections = LivePeerConnections::new();
        let node_id = make_node_id();
        match connections.drop_connection(&node_id) {
            Err(ConnectionManagerError::PeerConnectionNotFound) => {},
            Err(err) => panic!("Unexpected error: {:?}", err),
            Ok(_) => panic!("Unexpected Ok result"),
        }
    }

    #[test]
    fn shutdown() {
        let connections = LivePeerConnections::new();

        for _i in 0..3 {
            let node_id = make_node_id();
            let entry = make_peer_connection_entry();
            let join_handle = make_join_handle();

            connections.add_connection(node_id, entry, join_handle);
        }

        let results = connections.shutdown_joined();
        assert_eq!(3, results.len());
        assert!(results.iter().all(|r| r.is_ok()));
    }
}

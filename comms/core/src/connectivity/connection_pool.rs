//  Copyright 2020, The Tari Project
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

use std::{collections::HashMap, fmt, time::Duration};

use nom::lib::std::collections::hash_map::Entry;

use crate::{Minimized, PeerConnection, peer_manager::NodeId};

/// Status type for connections
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    NotConnected,
    Connecting,
    Connected,
    Retrying,
    Failed,
    Disconnected(Minimized),
}

impl ConnectionStatus {
    pub fn is_connected(self) -> bool {
        matches!(self, ConnectionStatus::Connected)
    }
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Connection state. This struct holds the PeerConnection handle if currently or previously connected.
#[derive(Debug, Clone)]
pub struct PeerConnectionState {
    node_id: NodeId,
    connection: Option<PeerConnection>,
    status: ConnectionStatus,
}

impl PeerConnectionState {
    pub fn connection(&self) -> Option<&PeerConnection> {
        self.connection.as_ref()
    }

    /// Return true if the underlying connection exists and is connected, otherwise false
    pub fn is_connected(&self) -> bool {
        self.status.is_connected() && self.connection().is_some_and(|c| c.is_connected())
    }

    pub fn connection_mut(&mut self) -> Option<&mut PeerConnection> {
        self.connection.as_mut()
    }

    pub fn into_connection(self) -> Option<PeerConnection> {
        self.connection
    }

    pub fn status(&self) -> ConnectionStatus {
        self.status
    }

    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    fn connected(conn: PeerConnection) -> Self {
        Self {
            node_id: conn.peer_node_id().clone(),
            connection: Some(conn),
            status: ConnectionStatus::Connected,
        }
    }

    fn set_connection(&mut self, conn: PeerConnection) {
        self.connection = Some(conn);
    }
}

impl fmt::Display for PeerConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}, Status = {}",
            self.connection()
                .map(ToString::to_string)
                .unwrap_or_else(|| self.node_id.to_string()),
            self.status()
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConnectionPool {
    connections: HashMap<NodeId, PeerConnectionState>,
}

impl ConnectionPool {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn contains(&mut self, node_id: &NodeId) -> bool {
        self.connections.contains_key(node_id)
    }

    pub fn insert_connection(&mut self, conn: PeerConnection) -> ConnectionStatus {
        match self.connections.entry(conn.peer_node_id().clone()) {
            Entry::Occupied(mut entry) => {
                let entry_mut = entry.get_mut();
                entry_mut.status = if conn.is_connected() {
                    ConnectionStatus::Connected
                } else {
                    ConnectionStatus::Disconnected(Minimized::No)
                };
                entry_mut.set_connection(conn);
                entry_mut.status
            },
            Entry::Vacant(entry) => entry.insert(PeerConnectionState::connected(conn)).status,
        }
    }

    #[inline]
    pub fn get(&self, node_id: &NodeId) -> Option<&PeerConnectionState> {
        self.connections.get(node_id)
    }

    #[inline]
    pub fn get_mut(&mut self, node_id: &NodeId) -> Option<&mut PeerConnectionState> {
        self.connections.get_mut(node_id)
    }

    pub fn all(&self) -> Vec<&PeerConnectionState> {
        self.connections.values().collect()
    }

    pub fn get_connection(&self, node_id: &NodeId) -> Option<&PeerConnection> {
        self.get(node_id).and_then(|c| c.connection())
    }

    pub fn get_connection_mut(&mut self, node_id: &NodeId) -> Option<&mut PeerConnection> {
        self.get_mut(node_id).and_then(|c| c.connection_mut())
    }

    pub fn get_connection_status(&self, node_id: &NodeId) -> ConnectionStatus {
        self.get(node_id)
            .map(|c| c.status())
            .unwrap_or(ConnectionStatus::NotConnected)
    }

    /// Returns the connections, inbound and outbound, that may be reaped: connections older than `min_age` that look
    /// idle, and write-stalled connections whatever their age and use. A stalled connection's substream and handle
    /// counts say nothing about whether it is useful, and the data yamux holds for it is not freed until it closes.
    /// Write-stalled connections come first.
    pub fn get_inactive_outbound_connections_mut(&mut self, min_age: Duration) -> Vec<&mut PeerConnection> {
        // The stall state changes while this runs, so it is read once per connection
        let mut stalled = Vec::new();
        let mut idle = Vec::new();
        for conn in self.connections.values_mut().filter_map(|c| c.connection_mut()) {
            let is_idle = conn.age() > min_age && conn.handle_count() <= 1 && conn.substream_count() < 3;
            if conn.is_write_stalled() {
                stalled.push(conn);
            } else if is_idle {
                idle.push(conn);
            } else {
                // Busy and not stalled: not reapable
            }
        }
        stalled.extend(idle);
        stalled
    }

    pub(in crate::connectivity) fn filter_drain<P>(&mut self, mut predicate: P) -> Vec<PeerConnectionState>
    where P: FnMut(&PeerConnectionState) -> bool {
        let (keep, remove) = self
            .connections
            .drain()
            .partition::<Vec<_>, _>(|(_, c)| !(predicate)(c));
        self.connections = keep.into_iter().collect::<HashMap<_, _>>();
        remove.into_iter().map(|(_, s)| s).collect()
    }

    pub(in crate::connectivity) fn filter_connection_states<P>(&self, mut predicate: P) -> Vec<&PeerConnection>
    where P: FnMut(&PeerConnectionState) -> bool {
        self.connections
            .values()
            .filter(|c| (predicate)(c))
            .filter_map(|c| c.connection())
            .collect()
    }

    pub fn set_status(&mut self, node_id: &NodeId, status: ConnectionStatus) -> ConnectionStatus {
        match self.connections.get_mut(node_id) {
            Some(state) => {
                let old_status = state.status();
                state.status = status;
                old_status
            },
            None => ConnectionStatus::NotConnected,
        }
    }

    pub fn remove(&mut self, node_id: &NodeId) -> Option<PeerConnection> {
        self.connections.remove(node_id).and_then(|c| c.into_connection())
    }

    pub fn count_connected_nodes(&self) -> usize {
        self.count_filtered(|c| {
            c.status() == ConnectionStatus::Connected &&
                c.connection()
                    .filter(|c| c.is_connected() && c.peer_features().is_node())
                    .is_some()
        })
    }

    pub fn count_connected_clients(&self) -> usize {
        self.count_filtered(|c| {
            c.status() == ConnectionStatus::Connected &&
                c.connection()
                    .filter(|c| c.is_connected() && c.peer_features().is_client())
                    .is_some()
        })
    }

    pub fn count_connected(&self) -> usize {
        self.count_filtered(|c| c.is_connected())
    }

    pub fn count_failed(&self) -> usize {
        self.count_filtered(|c| c.status() == ConnectionStatus::Failed)
    }

    pub fn count_disconnected(&self) -> usize {
        self.count_filtered(|c| matches!(c.status(), ConnectionStatus::Disconnected(_)))
    }

    pub fn count_entries(&self) -> usize {
        self.connections.len()
    }

    /// Get all connected node IDs for filtering in peer discovery
    pub fn get_connected_node_ids(&self) -> Vec<NodeId> {
        self.connections
            .iter()
            .filter_map(|(node_id, conn_state)| {
                if conn_state.status() == ConnectionStatus::Connected &&
                    conn_state
                        .connection()
                        .filter(|c| c.is_connected() && c.peer_features().is_node())
                        .is_some()
                {
                    Some(node_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    pub(in crate::connectivity) fn count_filtered<P>(&self, mut predicate: P) -> usize
    where P: FnMut(&PeerConnectionState) -> bool {
        self.connections.values().filter(|c| (predicate)(c)).count()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        peer_manager::NodeId,
        test_utils::{mocks::create_dummy_peer_connection, node_id},
    };

    #[test]
    fn a_write_stalled_connection_is_reapable_even_if_it_looks_busy() {
        let mut pool = ConnectionPool::new();
        let (busy, _busy_requests) = create_dummy_peer_connection(node_id::random());
        let (stalled, _stalled_requests) = create_dummy_peer_connection(node_id::random());
        // Both are young and have extra handles, so neither looks idle
        let _busy_handle = busy.clone();
        let _stalled_handle = stalled.clone();
        let stalled_node_id = stalled.peer_node_id().clone();
        pool.insert_connection(busy);
        pool.insert_connection(stalled.clone());

        let min_age = Duration::from_secs(60 * 60);
        assert!(pool.get_inactive_outbound_connections_mut(min_age).is_empty());

        // A short wait is normal backpressure
        stalled
            .write_stall()
            .set_stalled_since(tokio::time::Instant::now() - Duration::from_secs(1));
        assert!(pool.get_inactive_outbound_connections_mut(min_age).is_empty());

        stalled
            .write_stall()
            .set_stalled_since(tokio::time::Instant::now() - Duration::from_secs(31));
        let reapable = pool
            .get_inactive_outbound_connections_mut(min_age)
            .into_iter()
            .map(|conn| conn.peer_node_id().clone())
            .collect::<Vec<NodeId>>();
        assert_eq!(reapable, vec![stalled_node_id]);
    }

    #[test]
    fn write_stalled_connections_are_reaped_first() {
        let mut pool = ConnectionPool::new();
        let mut stalled_node_ids = Vec::new();
        let mut handles = Vec::new();
        for i in 0..6 {
            let (conn, requests) = create_dummy_peer_connection(node_id::random());
            if i % 2 == 1 {
                conn.write_stall()
                    .set_stalled_since(tokio::time::Instant::now() - Duration::from_secs(60));
                stalled_node_ids.push(conn.peer_node_id().clone());
            }
            handles.push(requests);
            pool.insert_connection(conn);
        }

        // Every connection is older than zero and idle, so all are reapable, stalled ones first
        std::thread::sleep(Duration::from_millis(1));
        let reapable = pool.get_inactive_outbound_connections_mut(Duration::ZERO);
        assert_eq!(reapable.len(), 6);
        for conn in reapable.iter().take(stalled_node_ids.len()) {
            assert!(stalled_node_ids.contains(conn.peer_node_id()));
        }
    }
}

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

use std::{cmp, collections::HashMap, fmt, time::Duration};

use nom::lib::std::collections::hash_map::Entry;

use crate::{Minimized, PeerConnection, connection_manager::ConnectionDirection, peer_manager::NodeId};

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

    /// Returns the connections the reaper should close: the inactive outbound connections above
    /// `outbound_threshold` outbound connections, and the inactive inbound connections above `inbound_soft_limit`
    /// inbound connections. Each direction is counted and reaped on its own, so a flood of inbound connections can
    /// never push out the outbound connections this node chose.
    ///
    /// A connection is inactive if it is older than `min_age` and looks idle, or if it is write stalled whatever its
    /// age and use (a stalled connection's substream and handle counts say nothing about whether it is useful, and
    /// the data yamux holds for it is not freed until it closes). Write-stalled connections go first, then the
    /// oldest idle ones. Connections pinned by a strong handle are never returned.
    pub fn get_reapable_connections_mut(
        &mut self,
        outbound_threshold: usize,
        inbound_soft_limit: usize,
        min_age: Duration,
    ) -> Vec<&mut PeerConnection> {
        let excess_outbound = self.count_connected_outbound().saturating_sub(outbound_threshold);
        let excess_inbound = self.count_connected_inbound().saturating_sub(inbound_soft_limit);
        if excess_outbound == 0 && excess_inbound == 0 {
            return Vec::new();
        }

        let mut outbound = Vec::new();
        let mut inbound = Vec::new();
        for conn in self.connections.values_mut().filter_map(|c| c.connection_mut()) {
            if !conn.is_connected() || conn.is_strongly_held() {
                continue;
            }
            match conn.direction() {
                ConnectionDirection::Outbound => outbound.push(conn),
                ConnectionDirection::Inbound => inbound.push(conn),
            }
        }

        let mut reapable = Self::inactive_first(outbound, min_age);
        reapable.truncate(excess_outbound);
        let mut inbound = Self::inactive_first(inbound, min_age);
        inbound.truncate(excess_inbound);
        reapable.extend(inbound);
        reapable
    }

    // Keeps the inactive connections: write-stalled ones first, then idle ones oldest first.
    fn inactive_first(connections: Vec<&mut PeerConnection>, min_age: Duration) -> Vec<&mut PeerConnection> {
        // The stall state changes while this runs, so it is read once per connection
        let mut stalled = Vec::new();
        let mut idle = Vec::new();
        for conn in connections {
            let is_idle = conn.age() > min_age && conn.handle_count() <= 1 && conn.substream_count() < 3;
            if conn.is_write_stalled() {
                stalled.push(conn);
            } else if is_idle {
                idle.push(conn);
            } else {
                // Busy and not stalled: not reapable
            }
        }
        idle.sort_by_key(|conn| cmp::Reverse(conn.age()));
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

    pub fn count_connected_inbound(&self) -> usize {
        self.count_filtered(|c| {
            c.is_connected() &&
                c.connection()
                    .is_some_and(|c| c.direction() == ConnectionDirection::Inbound)
        })
    }

    pub fn count_connected_outbound(&self) -> usize {
        self.count_filtered(|c| {
            c.is_connected() &&
                c.connection()
                    .is_some_and(|c| c.direction() == ConnectionDirection::Outbound)
        })
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
    #![allow(clippy::indexing_slicing)]
    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        RefKind,
        connection_manager::PeerConnectionRequest,
        peer_manager::NodeId,
        test_utils::{mocks::create_dummy_peer_connection_with_direction, node_id},
    };

    fn add_connection(
        pool: &mut ConnectionPool,
        direction: ConnectionDirection,
    ) -> (PeerConnection, mpsc::Receiver<PeerConnectionRequest>) {
        let (conn, requests) = create_dummy_peer_connection_with_direction(node_id::random(), direction);
        pool.insert_connection(conn.clone());
        (conn, requests)
    }

    fn reapable_node_ids(
        pool: &mut ConnectionPool,
        outbound_threshold: usize,
        inbound_soft_limit: usize,
        min_age: Duration,
    ) -> Vec<NodeId> {
        pool.get_reapable_connections_mut(outbound_threshold, inbound_soft_limit, min_age)
            .into_iter()
            .map(|conn| conn.peer_node_id().clone())
            .collect()
    }

    #[test]
    fn a_write_stalled_connection_is_reapable_even_if_it_looks_busy() {
        let mut pool = ConnectionPool::new();
        // Both are young and the test holds an extra handle, so neither looks idle
        let (_busy, _busy_requests) = add_connection(&mut pool, ConnectionDirection::Outbound);
        let (stalled, _stalled_requests) = add_connection(&mut pool, ConnectionDirection::Outbound);
        let stalled_node_id = stalled.peer_node_id().clone();

        let min_age = Duration::from_secs(60 * 60);
        assert!(reapable_node_ids(&mut pool, 0, 0, min_age).is_empty());

        // A short wait is normal backpressure
        stalled
            .write_stall()
            .set_stalled_since(tokio::time::Instant::now() - Duration::from_secs(1));
        assert!(reapable_node_ids(&mut pool, 0, 0, min_age).is_empty());

        stalled
            .write_stall()
            .set_stalled_since(tokio::time::Instant::now() - Duration::from_secs(31));
        assert_eq!(reapable_node_ids(&mut pool, 0, 0, min_age), vec![stalled_node_id]);
    }

    #[test]
    fn write_stalled_connections_are_reaped_first() {
        let mut pool = ConnectionPool::new();
        let mut stalled_node_ids = Vec::new();
        let mut requests = Vec::new();
        for i in 0..6 {
            let (conn, conn_requests) =
                create_dummy_peer_connection_with_direction(node_id::random(), ConnectionDirection::Outbound);
            if i % 2 == 1 {
                conn.write_stall()
                    .set_stalled_since(tokio::time::Instant::now() - Duration::from_secs(60));
                stalled_node_ids.push(conn.peer_node_id().clone());
            }
            requests.push(conn_requests);
            pool.insert_connection(conn);
        }

        // Every connection is older than zero and idle, so all are reapable, stalled ones first
        std::thread::sleep(Duration::from_millis(1));
        let reapable = reapable_node_ids(&mut pool, 0, 0, Duration::ZERO);
        assert_eq!(reapable.len(), 6);
        for node_id in reapable.iter().take(stalled_node_ids.len()) {
            assert!(stalled_node_ids.contains(node_id));
        }
    }

    #[test]
    fn sybil_inbound_connections_do_not_evict_idle_outbound() {
        let mut pool = ConnectionPool::new();
        let mut requests = Vec::new();
        let (outbound, outbound_requests) =
            create_dummy_peer_connection_with_direction(node_id::random(), ConnectionDirection::Outbound);
        pool.insert_connection(outbound);
        requests.push(outbound_requests);
        for _ in 0..20 {
            let (conn, conn_requests) =
                create_dummy_peer_connection_with_direction(node_id::random(), ConnectionDirection::Inbound);
            pool.insert_connection(conn);
            requests.push(conn_requests);
        }
        std::thread::sleep(Duration::from_millis(1));
        assert_eq!(pool.count_connected_inbound(), 20);
        assert_eq!(pool.count_connected_outbound(), 1);

        // 21 idle connections against an outbound threshold of 12 (a base node's DHT pool size): the inbound
        // connections are not counted against it, and are under the inbound soft limit
        assert!(reapable_node_ids(&mut pool, 12, 400, Duration::ZERO).is_empty());
        // Even when the inbound connections are over their own limit, only inbound connections are reaped
        let reapable = reapable_node_ids(&mut pool, 12, 5, Duration::ZERO);
        assert_eq!(reapable.len(), 15);
        for node_id in &reapable {
            let conn = pool.get_connection(node_id).unwrap();
            assert_eq!(conn.direction(), ConnectionDirection::Inbound);
        }
    }

    #[test]
    fn inbound_soft_limit_reaps_oldest_idle_inbound() {
        let mut pool = ConnectionPool::new();
        let mut node_ids = Vec::new();
        let mut requests = Vec::new();
        for _ in 0..3 {
            let (conn, conn_requests) =
                create_dummy_peer_connection_with_direction(node_id::random(), ConnectionDirection::Inbound);
            node_ids.push(conn.peer_node_id().clone());
            pool.insert_connection(conn);
            requests.push(conn_requests);
            // Distinct ages
            std::thread::sleep(Duration::from_millis(5));
        }

        assert_eq!(reapable_node_ids(&mut pool, 50, 2, Duration::ZERO), vec![
            node_ids[0].clone()
        ]);
        assert!(reapable_node_ids(&mut pool, 50, 3, Duration::ZERO).is_empty());

        // A connection pinned by a strong handle is never reaped, the next oldest goes instead
        let _strong_handle = pool.get_connection(&node_ids[0]).unwrap().clone_with(RefKind::Strong);
        assert_eq!(reapable_node_ids(&mut pool, 50, 2, Duration::ZERO), vec![
            node_ids[1].clone()
        ]);
    }
}

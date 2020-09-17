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

use crate::{peer_manager::NodeId, PeerConnection};
use nom::lib::std::collections::hash_map::Entry;
use std::{collections::HashMap, fmt, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionStatus {
    NotConnected,
    Connecting,
    Connected,
    Retrying,
    Failed,
    Disconnected,
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone)]
pub struct PeerConnectionState {
    node_id: NodeId,
    connection: Option<PeerConnection>,
    status: ConnectionStatus,
}

impl PeerConnectionState {
    #[inline]
    pub fn connection(&self) -> Option<&PeerConnection> {
        self.connection.as_ref()
    }

    #[inline]
    pub fn connection_mut(&mut self) -> Option<&mut PeerConnection> {
        self.connection.as_mut()
    }

    #[inline]
    pub fn into_connection(self) -> Option<PeerConnection> {
        self.connection
    }

    #[inline]
    pub fn status(&self) -> ConnectionStatus {
        self.status
    }

    #[inline]
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    fn not_connected(node_id: NodeId) -> Self {
        Self {
            node_id,
            connection: None,
            status: ConnectionStatus::NotConnected,
        }
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

    pub fn insert(&mut self, node_id: NodeId) -> ConnectionStatus {
        match self.connections.entry(node_id) {
            Entry::Occupied(entry) => entry.get().status(),
            Entry::Vacant(entry) => {
                let node_id = entry.key().clone();
                entry.insert(PeerConnectionState::not_connected(node_id)).status()
            },
        }
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
                    ConnectionStatus::Disconnected
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

    pub fn all(&self) -> Vec<&PeerConnectionState> {
        self.connections.values().collect()
    }

    pub fn get_connection(&self, node_id: &NodeId) -> Option<&PeerConnection> {
        self.get(node_id).and_then(|c| c.connection())
    }

    pub fn get_connection_status(&self, node_id: &NodeId) -> ConnectionStatus {
        self.get(node_id)
            .map(|c| c.status())
            .unwrap_or(ConnectionStatus::NotConnected)
    }

    pub fn get_inactive_connections_mut(&mut self, min_age: Duration) -> Vec<&mut PeerConnection> {
        self.filter_connections_mut(|conn| conn.age() > min_age && conn.substream_count() == 0)
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
            .filter(|c| (predicate)(*c))
            .filter_map(|c| c.connection())
            .collect()
    }

    fn filter_connections_mut<P>(&mut self, mut predicate: P) -> Vec<&mut PeerConnection>
    where P: FnMut(&PeerConnection) -> bool {
        self.connections
            .values_mut()
            .filter_map(|c| c.connection_mut())
            .filter(|c| (predicate)(*c))
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
        self.connections
            .values()
            .filter(|c| {
                c.status() == ConnectionStatus::Connected &&
                    c.connection()
                        .filter(|c| c.is_connected() && c.peer_features().is_node())
                        .is_some()
            })
            .count()
    }

    pub fn count_connected_clients(&self) -> usize {
        self.connections
            .values()
            .filter(|c| {
                c.status() == ConnectionStatus::Connected &&
                    c.connection()
                        .filter(|c| c.is_connected() && c.peer_features().is_client())
                        .is_some()
            })
            .count()
    }

    pub fn count_failed(&self) -> usize {
        self.count_status(ConnectionStatus::Failed)
    }

    pub fn count_disconnected(&self) -> usize {
        self.count_status(ConnectionStatus::Disconnected)
    }

    pub fn count_entries(&self) -> usize {
        self.connections.len()
    }

    fn count_status(&self, status: ConnectionStatus) -> usize {
        self.connections.values().filter(|c| c.status() == status).count()
    }
}

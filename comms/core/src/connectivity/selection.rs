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

use std::{collections::HashMap, fmt, fmt::Display, time::Duration};

use rand::seq::IndexedRandom;

use super::{connection_pool::ConnectionPool, connection_stats::PeerConnectionStats};
use crate::{PeerConnection, connectivity::connection_pool::ConnectionStatus, peer_manager::NodeId};

/// Selection query for PeerConnections.
#[derive(Debug, Clone)]
pub struct ConnectivitySelection {
    selection_mode: SelectionMode,
    excluded_peers: Vec<NodeId>,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone)]
enum SelectionMode {
    AllNodes,
    RandomNodes(usize),
    HealthyNodes(usize),
}

impl ConnectivitySelection {
    /// Returns a query that will return all connections for peers with `PeerFeatures::COMMUNICATION_NODES` excluding
    /// the given [NodeId]s.
    ///
    /// [NodeId](crate::peer_manager::NodeId)
    pub fn all_nodes(exclude: Vec<NodeId>) -> Self {
        Self {
            selection_mode: SelectionMode::AllNodes,
            excluded_peers: exclude,
        }
    }

    /// Returns a query that will return `n` connections for peers with `PeerFeatures::COMMUNICATION_NODES` excluding
    /// the given [NodeId]s.
    ///
    /// [NodeId](crate::peer_manager::NodeId)
    pub fn random_nodes(n: usize, exclude: Vec<NodeId>) -> Self {
        Self {
            selection_mode: SelectionMode::RandomNodes(n),
            excluded_peers: exclude,
        }
    }

    /// Select `n` peer connections ordered by health score, excluding the given `exclude` [NodeId]s.
    ///
    /// [NodeId](crate::peer_manager::NodeId)
    pub fn healthy_nodes(n: usize, exclude: Vec<NodeId>) -> Self {
        Self {
            selection_mode: SelectionMode::HealthyNodes(n),
            excluded_peers: exclude,
        }
    }

    /// Select peers from the pool according to the ConnectivitySelection
    pub fn select<'a>(&self, pool: &'a ConnectionPool) -> Vec<&'a PeerConnection> {
        use SelectionMode::{AllNodes, HealthyNodes, RandomNodes};
        match &self.selection_mode {
            AllNodes => select_connected_nodes(pool, &self.excluded_peers),
            RandomNodes(n) => select_random_nodes(pool, *n, &self.excluded_peers),
            HealthyNodes(n) => {
                // For basic health selection without health stats, fall back to random selection
                select_random_nodes(pool, *n, &self.excluded_peers)
            },
        }
    }

    /// Select peers from the pool with health-based prioritization
    pub fn select_with_health<'a>(
        &self,
        pool: &'a ConnectionPool,
        health_stats: &HashMap<NodeId, PeerConnectionStats>,
        health_window: Duration,
    ) -> Vec<&'a PeerConnection> {
        use SelectionMode::{AllNodes, HealthyNodes, RandomNodes};
        match &self.selection_mode {
            AllNodes => select_connected_nodes(pool, &self.excluded_peers),
            RandomNodes(n) => select_random_nodes(pool, *n, &self.excluded_peers),
            HealthyNodes(n) => {
                let mut connections = select_healthy_nodes(pool, health_stats, health_window, &self.excluded_peers);
                connections.truncate(*n);
                connections.to_vec()
            },
        }
    }
}

fn select_connected_nodes<'a>(pool: &'a ConnectionPool, exclude: &[NodeId]) -> Vec<&'a PeerConnection> {
    pool.filter_connection_states(|state| {
        if state.status() != ConnectionStatus::Connected {
            return false;
        }
        let conn = state
            .connection()
            .expect("Connection does not exist in PeerConnectionState with status=Connected");
        conn.is_connected() && conn.peer_features().is_node() && !exclude.contains(conn.peer_node_id())
    })
}

fn select_random_nodes<'a>(pool: &'a ConnectionPool, n: usize, exclude: &[NodeId]) -> Vec<&'a PeerConnection> {
    let nodes = select_connected_nodes(pool, exclude);
    nodes.sample(&mut rand::rng(), n).copied().collect()
}

fn select_healthy_nodes<'a>(
    pool: &'a ConnectionPool,
    health_stats: &HashMap<NodeId, PeerConnectionStats>,
    health_window: Duration,
    exclude: &[NodeId],
) -> Vec<&'a PeerConnection> {
    let mut nodes = select_connected_nodes(pool, exclude);

    // Sort by health score (descending)
    nodes.sort_by(|a, b| {
        let health_a = health_stats
            .get(a.peer_node_id())
            .map(|s| s.health_score(health_window))
            .unwrap_or(0.5); // Neutral score for unknown peers

        let health_b = health_stats
            .get(b.peer_node_id())
            .map(|s| s.health_score(health_window))
            .unwrap_or(0.5);

        health_b.partial_cmp(&health_a).unwrap_or(std::cmp::Ordering::Equal)
    });

    nodes
}

impl Display for ConnectivitySelection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "ConnectivitySelection(mode = {}, excluded {} peer(s))",
            self.selection_mode,
            self.excluded_peers.len()
        )
    }
}

impl Display for SelectionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use SelectionMode::{AllNodes, HealthyNodes, RandomNodes};
        match self {
            AllNodes => write!(f, "AllNodes"),
            RandomNodes(n) => write!(f, "RandomNodes({n})"),
            HealthyNodes(n) => write!(f, "HealthyNodes({n})"),
        }
    }
}

#[cfg(test)]
mod test {
    use std::iter::repeat_with;

    use tokio::sync::mpsc;

    use super::*;
    use crate::{
        connection_manager::PeerConnectionRequest,
        test_utils::{mocks::create_dummy_peer_connection, node_id},
    };

    fn create_pool_with_connections(n: usize) -> (ConnectionPool, Vec<mpsc::Receiver<PeerConnectionRequest>>) {
        let mut pool = ConnectionPool::new();
        let mut receivers = Vec::with_capacity(n);
        repeat_with(|| create_dummy_peer_connection(node_id::random()))
            .take(n)
            .for_each(|(conn, rx)| {
                receivers.push(rx);
                assert!(conn.is_connected());
                pool.insert_connection(conn);
            });
        (pool, receivers)
    }

    #[test]
    fn select_random() {
        let (pool, _receivers) = create_pool_with_connections(10);
        let conns = select_random_nodes(&pool, 500, &[]);
        assert_eq!(conns.len(), 10);

        let first_node = conns.first().unwrap().peer_node_id().clone();
        let conns = select_random_nodes(&pool, 10, std::slice::from_ref(&first_node));
        assert_eq!(conns.len(), 9);
        assert!(conns.iter().all(|c| c.peer_node_id() != &first_node));
    }
}

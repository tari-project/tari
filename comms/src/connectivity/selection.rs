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

use super::connection_pool::ConnectionPool;
use crate::{connectivity::connection_pool::ConnectionStatus, peer_manager::NodeId, PeerConnection};
use rand::{rngs::OsRng, seq::SliceRandom};

#[derive(Debug, Clone)]
pub enum ConnectivitySelection {
    RandomNodes(usize, Vec<NodeId>),
    ClosestTo(Box<NodeId>, usize, Vec<NodeId>),
}

pub fn select_connected_nodes<'a>(pool: &'a ConnectionPool, exclude: &[NodeId]) -> Vec<&'a PeerConnection> {
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

pub fn select_closest<'a>(pool: &'a ConnectionPool, node_id: &NodeId, exclude: &[NodeId]) -> Vec<&'a PeerConnection> {
    let mut nodes = select_connected_nodes(pool, exclude);

    nodes.sort_by(|a, b| {
        let dist_a = a.peer_node_id().distance(node_id);
        let dist_b = b.peer_node_id().distance(node_id);
        dist_a.cmp(&dist_b)
    });

    nodes
}

pub fn select_random_nodes<'a>(pool: &'a ConnectionPool, n: usize, exclude: &[NodeId]) -> Vec<&'a PeerConnection> {
    let nodes = select_connected_nodes(pool, exclude);
    nodes.choose_multiple(&mut OsRng, n).map(|c| *c).collect()
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection_manager::PeerConnectionRequest,
        peer_manager::node_id::NodeDistance,
        test_utils::{mocks::create_dummy_peer_connection, node_id, node_identity::build_node_identity},
    };
    use futures::channel::mpsc;
    use std::iter::repeat_with;

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
        let conns = select_random_nodes(&pool, 10, &[first_node.clone()]);
        assert_eq!(conns.len(), 9);
        assert!(conns.iter().all(|c| c.peer_node_id() != &first_node));
    }

    #[test]
    fn select_closest_ordering() {
        let (pool, _receivers) = create_pool_with_connections(10);
        let subject_node_identity = build_node_identity(Default::default());
        let conns = select_closest(&pool, subject_node_identity.node_id(), &[]);
        assert_eq!(conns.len(), 10);

        let mut last_dist = NodeDistance::zero();
        for (i, conn) in conns.into_iter().enumerate() {
            let dist = conn.peer_node_id().distance(subject_node_identity.node_id());
            assert!(dist > last_dist, "Ordering was incorrect on connection {}", i);
            last_dist = dist;
        }
    }

    #[test]
    fn select_closest_empty() {
        let pool = ConnectionPool::new();
        let node_identity = build_node_identity(Default::default());
        let conns = select_closest(&pool, node_identity.node_id(), &[]);
        assert!(conns.is_empty());
    }
}

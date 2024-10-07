//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use libp2p::{core::ConnectedPoint, swarm::ConnectionId, PeerId};

#[derive(Debug, Clone)]
pub struct Connection {
    pub connection_id: ConnectionId,
    pub peer_id: PeerId,
    pub created_at: Instant,
    pub endpoint: ConnectedPoint,
    pub num_established: u32,
    pub num_concurrent_dial_errors: usize,
    pub established_in: Duration,
    pub ping_latency: Option<Duration>,
    pub user_agent: Option<Arc<String>>,
}

impl Connection {
    pub fn peer_id(&self) -> &PeerId {
        &self.peer_id
    }

    pub fn age(&self) -> Duration {
        self.created_at.elapsed()
    }
}

//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use libp2p::{core::ConnectedPoint, swarm::ConnectionId, PeerId};

use crate::{identity::PublicKey, multiaddr::Multiaddr, ConnectionDirection};

#[derive(Debug, Clone)]
pub struct Connection {
    pub connection_id: ConnectionId,
    pub peer_id: PeerId,
    pub public_key: Option<PublicKey>,
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

    pub fn address(&self) -> &Multiaddr {
        self.endpoint.get_remote_address()
    }

    pub fn direction(&self) -> ConnectionDirection {
        if self.endpoint.is_dialer() {
            ConnectionDirection::Outbound
        } else {
            ConnectionDirection::Inbound
        }
    }

    pub fn is_wallet_user_agent(&self) -> bool {
        self.user_agent.as_ref().map_or(false, |x| x.contains("wallet"))
    }
}

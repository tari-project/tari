//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::fmt::{Display, Formatter};

use libp2p::{identity, PeerId, StreamProtocol};

#[derive(Debug, Clone)]
pub enum NetworkEvent {
    IdentifiedPeer {
        peer_id: PeerId,
        public_key: identity::PublicKey,
        agent_version: String,
        supported_protocols: Vec<StreamProtocol>,
    },
    PeerConnected {
        peer_id: PeerId,
        direction: ConnectionDirection,
    },
    PeerDisconnected {
        peer_id: PeerId,
    },
    PeerBanned {
        peer_id: PeerId,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ConnectionDirection {
    Inbound,
    Outbound,
}

impl ConnectionDirection {
    pub fn is_inbound(&self) -> bool {
        matches!(self, ConnectionDirection::Inbound)
    }

    pub fn is_outbound(&self) -> bool {
        matches!(self, ConnectionDirection::Outbound)
    }
}

impl Display for ConnectionDirection {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionDirection::Inbound => write!(f, "Inbound"),
            ConnectionDirection::Outbound => write!(f, "Outbound"),
        }
    }
}

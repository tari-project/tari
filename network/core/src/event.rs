//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p::{identity, PeerId, StreamProtocol};

#[derive(Debug, Clone)]
pub enum NetworkingEvent {
    NewIdentifiedPeer {
        peer_id: PeerId,
        public_key: identity::PublicKey,
        supported_protocols: Vec<StreamProtocol>,
    },
    PeerConnected {
        peer_id: PeerId,
        direction: ConnectionDirection,
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

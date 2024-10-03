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
}

//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p::{PeerId, StreamProtocol};

/// Event emitted when a new inbound substream is requested by a remote node.
#[derive(Debug, Clone)]
pub enum ProtocolEvent<TSubstream> {
    NewInboundSubstream { peer_id: PeerId, substream: TSubstream },
}

/// Notification of a new protocol
#[derive(Debug, Clone)]
pub struct ProtocolNotification<TSubstream> {
    pub event: ProtocolEvent<TSubstream>,
    pub protocol: StreamProtocol,
}

impl<TSubstream> ProtocolNotification<TSubstream> {
    pub fn new(protocol: StreamProtocol, event: ProtocolEvent<TSubstream>) -> Self {
        Self { event, protocol }
    }
}

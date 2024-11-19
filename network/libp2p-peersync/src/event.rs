//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p::PeerId;

use crate::error::Error;

#[derive(Debug)]
pub enum Event {
    InboundFailure {
        peer_id: PeerId,
        error: Error,
    },
    OutboundFailure {
        peer_id: PeerId,
        error: Error,
    },
    PeerBatchReceived {
        from_peer: PeerId,
        new_peers: usize,
    },
    InboundStreamInterrupted {
        peer_id: PeerId,
    },
    OutboundStreamInterrupted {
        peer_id: PeerId,
    },
    InboundRequestCompleted {
        peer_id: PeerId,
        peers_sent: usize,
        requested: usize,
    },
    LocalPeerRecordUpdated,
    Error(Error),
}

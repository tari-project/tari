//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p::{PeerId, Stream, StreamProtocol};

use crate::{error::Error, stream::StreamId, ProtocolNotification};

#[derive(Debug)]
pub enum Event {
    SubstreamOpen {
        peer_id: PeerId,
        stream_id: StreamId,
        stream: Stream,
        protocol: StreamProtocol,
    },
    InboundSubstreamOpen {
        notification: ProtocolNotification<Stream>,
    },
    InboundFailure {
        peer_id: PeerId,
        stream_id: StreamId,
        error: Error,
    },
    OutboundFailure {
        peer_id: PeerId,
        protocol: StreamProtocol,
        stream_id: StreamId,
        error: Error,
    },
    Error(Error),
}

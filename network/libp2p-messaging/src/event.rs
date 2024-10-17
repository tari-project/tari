//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p::PeerId;

use crate::{error::Error, stream::StreamId, MessageId};

#[derive(Debug)]
pub enum Event<TMsg> {
    ReceivedMessage {
        peer_id: PeerId,
        message: TMsg,
        length: usize,
    },
    MessageSent {
        message_id: MessageId,
        stream_id: StreamId,
    },
    InboundFailure {
        peer_id: PeerId,
        stream_id: StreamId,
        error: Error,
    },
    OutboundFailure {
        peer_id: PeerId,
        stream_id: StreamId,
        error: Error,
    },
    OutboundStreamOpened {
        peer_id: PeerId,
        stream_id: StreamId,
    },
    InboundStreamOpened {
        peer_id: PeerId,
    },
    InboundStreamClosed {
        peer_id: PeerId,
    },
    StreamClosed {
        peer_id: PeerId,
        stream_id: StreamId,
    },
    Error(Error),
}

//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{fmt::Debug, io};

use futures_bounded::Timeout;
use libp2p::{multiaddr, PeerId};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Codec error: {0}")]
    CodecError(#[from] io::Error),
    #[error("Connection closed")]
    ConnectionClosed,
    #[error("Unexpected end of inbound stream")]
    InboundStreamEnded,
    #[error("Timeout: {0}")]
    Timeout(Timeout),
    #[error("Dial failure")]
    DialFailure,
    #[error("Dial upgrade error")]
    DialUpgradeError,
    #[error("Protocol not supported")]
    ProtocolNotSupported,
    #[error("Want list too large: want_list_len={want_list_len}, max_len={max_len}")]
    WantListTooLarge { want_list_len: usize, max_len: usize },
    #[error("Store error: {0}")]
    StoreError(String),
    #[error("Invalid message from peer `{peer_id}`: {details}")]
    InvalidMessage { peer_id: PeerId, details: String },
    #[error("Failed to decode multiaddr: {0}")]
    DecodeMultiaddr(#[from] multiaddr::Error),

    #[error("Invalid signed peer receord from peer `{peer_id}`: {details}")]
    InvalidSignedPeer { peer_id: PeerId, details: String },
    #[error("Exceeded maximum number of pending tasks")]
    ExceededMaxNumberOfPendingTasks,
    #[error("Max failed attempts reached")]
    MaxFailedAttemptsReached,
    #[error("Local peer not signed")]
    LocalPeerNotSigned,
}

impl From<quick_protobuf_codec::Error> for Error {
    fn from(err: quick_protobuf_codec::Error) -> Self {
        Self::CodecError(err.into())
    }
}

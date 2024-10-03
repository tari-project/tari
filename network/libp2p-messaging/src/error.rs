//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::{
    fmt::{Debug, Display, Formatter},
    io,
};

use futures_bounded::Timeout;

#[derive(Debug)]
pub enum Error {
    CodecError(io::Error),
    ConnectionClosed,
    Timeout(Timeout),
    DialFailure,
    DialUpgradeError,
    ProtocolNotSupported,
    ChannelClosed,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CodecError(err) => write!(f, "Codec error: {}", err),
            Self::ConnectionClosed => write!(f, "Connection closed"),
            Self::Timeout(err) => write!(f, "Timeout: {}", err),
            Self::DialFailure => write!(f, "Dial failure"),
            Self::DialUpgradeError => write!(f, "Dial upgrade error"),
            Self::ProtocolNotSupported => write!(f, "Protocol not supported"),
            Self::ChannelClosed => write!(f, "Channel closed"),
        }
    }
}

impl std::error::Error for Error {}

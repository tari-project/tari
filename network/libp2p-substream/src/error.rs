//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use std::fmt::{Debug, Display, Formatter};

#[derive(Debug, Clone)]
pub enum Error {
    ConnectionClosed,
    DialFailure { details: String },
    NoAddressesForPeer,
    DialUpgradeError,
    ProtocolNotSupported,
    ProtocolNegotiationTimeout,
    ChannelClosed,
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConnectionClosed => write!(f, "Connection closed"),
            Self::DialFailure { details } => write!(f, "Dial failure: {details}"),
            Self::NoAddressesForPeer => write!(f, "No addresses for peer"),
            Self::DialUpgradeError => write!(f, "Dial upgrade error"),
            Self::ProtocolNotSupported => write!(f, "Protocol not supported"),
            Self::ProtocolNegotiationTimeout => write!(f, "Protocol negotiation timeout"),
            Self::ChannelClosed => write!(f, "Channel closed"),
        }
    }
}

impl std::error::Error for Error {}

//   Copyright 2023 The Tari Project
//   SPDX-License-Identifier: BSD-3-Clause

use libp2p::{noise, swarm::InvalidProtocol};

#[derive(Debug, thiserror::Error)]
pub enum TariSwarmError {
    #[error("Noise error: {0}")]
    Noise(#[from] noise::Error),
    #[error(transparent)]
    InvalidProtocol(#[from] InvalidProtocol),
    #[error("Behaviour error: {0}")]
    BehaviourError(String),
    #[error("'{given}' is not a valid protocol version string")]
    ProtocolVersionParseFailed { given: String },
    #[error("Invalid version string: {given}")]
    InvalidVersionString { given: String },
}

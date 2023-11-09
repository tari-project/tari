// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::time::Duration;

use tari_comms_dht::outbound::DhtOutboundError;
use thiserror::Error;

use crate::{
    base_node::{comms_interface::CommsInterfaceError, service::initializer::ExtractBlockError},
    common::BanReason,
};

#[derive(Debug, Error)]
pub enum BaseNodeServiceError {
    #[error("Comms interface error: `{0}`")]
    CommsInterfaceError(#[from] CommsInterfaceError),
    #[error("DHT outbound error: `{0}`")]
    DhtOutboundError(#[from] DhtOutboundError),
    #[error("Invalid request error: `{0}`")]
    InvalidRequest(String),
    #[error("Invalid response error: `{0}`")]
    InvalidResponse(String),
    #[error("Invalid block error: `{0}`")]
    InvalidBlockMessage(#[from] ExtractBlockError),
}

impl BaseNodeServiceError {
    pub fn get_ban_reason(&self, short_ban: Duration, long_ban: Duration) -> Option<BanReason> {
        match self {
            BaseNodeServiceError::CommsInterfaceError(comms) => match comms {
                err @ CommsInterfaceError::UnexpectedApiResponse | err @ CommsInterfaceError::RequestTimedOut => {
                    Some(BanReason {
                        reason: err.to_string(),
                        ban_duration: short_ban,
                    })
                },
                err @ CommsInterfaceError::InvalidPeerResponse(_) |
                err @ CommsInterfaceError::InvalidBlockHeader(_) |
                err @ CommsInterfaceError::TransactionError(_) |
                err @ CommsInterfaceError::InvalidFullBlock { .. } |
                err @ CommsInterfaceError::InvalidRequest { .. } => Some(BanReason {
                    reason: err.to_string(),
                    ban_duration: long_ban,
                }),
                CommsInterfaceError::MempoolError(e) => e.get_ban_reason(short_ban, long_ban),
                CommsInterfaceError::TransportChannelError(e) => Some(BanReason {
                    reason: e.to_string(),
                    ban_duration: short_ban,
                }),
                CommsInterfaceError::ChainStorageError(e) => e.get_ban_reason(short_ban, long_ban),
                CommsInterfaceError::MergeMineError(e) => e.get_ban_reason(short_ban, long_ban),
                CommsInterfaceError::NoBootstrapNodesConfigured |
                CommsInterfaceError::OutboundMessageError(_) |
                CommsInterfaceError::BroadcastFailed |
                CommsInterfaceError::InternalChannelError(_) |
                CommsInterfaceError::DifficultyAdjustmentManagerError(_) |
                CommsInterfaceError::InternalError(_) |
                CommsInterfaceError::ApiError(_) |
                CommsInterfaceError::BlockError(_) |
                CommsInterfaceError::DifficultyError(_) => None,
            },
            BaseNodeServiceError::DhtOutboundError(_) => None,
            err @ BaseNodeServiceError::InvalidRequest(_) |
            err @ BaseNodeServiceError::InvalidResponse(_) |
            err @ BaseNodeServiceError::InvalidBlockMessage(_) => Some(BanReason {
                reason: err.to_string(),
                ban_duration: long_ban,
            }),
        }
    }
}

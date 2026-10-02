// Copyright 2019. The Tari Project
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

use tari_common_types::types::FixedHash;
use tari_comms_dht::outbound::DhtOutboundError;
use tari_node_components::blocks::{BlockError, BlockHeaderValidationError};
use tari_service_framework::reply_channel::TransportChannelError;
use tari_transaction_components::{
    BanPeriod,
    BanReason,
    consensus::ConsensusManagerError,
    tari_proof_of_work::DifficultyError,
    transaction_components::TransactionError,
};
use thiserror::Error;

use crate::{
    chain_storage::ChainStorageError,
    mempool::MempoolError,
    proof_of_work::{cuckaroo_pow::CuckarooVerificationError, monero_rx::MergeMineError},
};

#[derive(Debug, Error)]
pub enum CommsInterfaceError {
    #[error("Received an unexpected response from a remote peer")]
    UnexpectedApiResponse,
    #[error("Request timed out")]
    RequestTimedOut,
    #[error("No bootstrap nodes have been configured")]
    NoBootstrapNodesConfigured,
    #[error("Transport channel error: {0}")]
    TransportChannelError(#[from] TransportChannelError),
    #[error("Chain storage error: {0}")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Failed to send outbound message: {0}")]
    OutboundMessageError(#[from] DhtOutboundError),
    #[error("Mempool error: {0}")]
    MempoolError(#[from] MempoolError),
    #[error("Failed to broadcast message")]
    BroadcastFailed,
    #[error("Internal channel error: {0}")]
    InternalChannelError(String),
    #[error("Difficulty adjustment error: {0}")]
    DifficultyAdjustmentManagerError(#[from] ConsensusManagerError),
    #[error("Invalid peer response: {0}")]
    InvalidPeerResponse(String),
    #[error("Invalid Block Header: {0}")]
    InvalidBlockHeader(#[from] BlockHeaderValidationError),
    #[error("Internal error:{0}")]
    InternalError(String),
    #[error("API responded with an error: {0}")]
    ApiError(String),
    #[error("Block error: {0}")]
    BlockError(#[from] BlockError),
    #[error("Invalid request for {request}: {details}")]
    InvalidRequest { request: &'static str, details: String },
    #[error("Peer sent invalid full block {hash}: {details}")]
    InvalidFullBlock { hash: FixedHash, details: String },
    #[error("Block {hash} is already known to be bad: {reason}")]
    KnownBadBlock { hash: String, reason: String },
    #[error("Block {hash} does not build on our tip and spends outputs we do not have: {details}")]
    UnknownSpentOutputs {
        hash: FixedHash,
        details: String,
        /// Whether the block's parent is a held orphan, whose chain was searched for the outputs
        held_orphan_parent: bool,
    },
    #[error("Invalid merge mined block: {0}")]
    MergeMineError(#[from] MergeMineError),
    #[error("Invalid difficulty: {0}")]
    DifficultyError(#[from] DifficultyError),
    #[error("Transaction error: {0}")]
    TransactionError(#[from] TransactionError),
    #[error("Cuckaroo verification error: {0}")]
    CuckarooVerificationError(#[from] CuckarooVerificationError),
}

impl CommsInterfaceError {
    pub fn get_ban_reason(&self) -> Option<BanReason> {
        match self {
            err @ CommsInterfaceError::UnexpectedApiResponse |
            err @ CommsInterfaceError::RequestTimedOut |
            err @ CommsInterfaceError::TransportChannelError(_) |
            // The block builds on a chain we hold, which we searched, and it spends an output that is not on it
            err @ CommsInterfaceError::UnknownSpentOutputs {
                held_orphan_parent: true,
                ..
            } => Some(BanReason {
                reason: err.to_string(),
                ban_duration: BanPeriod::Short,
            }),
            err @ CommsInterfaceError::InvalidPeerResponse(_) |
            err @ CommsInterfaceError::InvalidBlockHeader(_) |
            err @ CommsInterfaceError::TransactionError(_) |
            err @ CommsInterfaceError::CuckarooVerificationError(_) |
            err @ CommsInterfaceError::InvalidFullBlock { .. } |
            err @ CommsInterfaceError::InvalidRequest { .. } => Some(BanReason {
                reason: err.to_string(),
                ban_duration: BanPeriod::Long,
            }),
            CommsInterfaceError::MempoolError(e) => e.get_ban_reason(),
            CommsInterfaceError::ChainStorageError(e) => e.get_ban_reason(),
            CommsInterfaceError::MergeMineError(e) => e.get_ban_reason(),
            CommsInterfaceError::NoBootstrapNodesConfigured |
            CommsInterfaceError::OutboundMessageError(_) |
            CommsInterfaceError::BroadcastFailed |
            CommsInterfaceError::InternalChannelError(_) |
            CommsInterfaceError::DifficultyAdjustmentManagerError(_) |
            CommsInterfaceError::InternalError(_) |
            CommsInterfaceError::ApiError(_) |
            CommsInterfaceError::BlockError(_) |
            // A relayed hash we hold as bad: the peer did not send us invalid data, and may not have been able to tell
            CommsInterfaceError::KnownBadBlock { .. } |
            // A block on a chain we do not hold: honest peers relay such blocks across forks, so it says nothing about
            // the peer
            CommsInterfaceError::UnknownSpentOutputs {
                held_orphan_parent: false,
                ..
            } |
            // CommsInterfaceError::Other(_) |
            CommsInterfaceError::DifficultyError(_) => None,
        }
    }
}

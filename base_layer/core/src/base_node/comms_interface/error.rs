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

use crate::{
    blocks::BlockHeaderValidationError,
    chain_storage::ChainStorageError,
    consensus::ConsensusManagerError,
    mempool::MempoolError,
};
use tari_comms_dht::outbound::DhtOutboundError;
use tari_service_framework::reply_channel::TransportChannelError;
use thiserror::Error;

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
}

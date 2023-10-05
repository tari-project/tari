//  Copyright 2022, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::{num::TryFromIntError, time::Duration};

use tari_common_types::types::FixedHashSizeError;
use tari_comms::{
    connectivity::ConnectivityError,
    peer_manager::NodeId,
    protocol::rpc::{RpcError, RpcStatus},
};
use tari_crypto::errors::RangeProofError;
use tari_mmr::{error::MerkleMountainRangeError, sparse_merkle_tree::SMTError};
use thiserror::Error;
use tokio::task;

use crate::{
    chain_storage::ChainStorageError,
    common::BanReason,
    transactions::transaction_components::TransactionError,
    validation::ValidationError,
};

#[derive(Debug, Error)]
pub enum HorizonSyncError {
    #[error("Peer sent an invalid response: {0}")]
    IncorrectResponse(String),
    #[error("Chain storage error: {0}")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Final state validation failed: {0}")]
    FinalStateValidationFailed(ValidationError),
    #[error("Join error: {0}")]
    JoinError(#[from] task::JoinError),
    #[error("A range proof verification has produced an error: {0}")]
    RangeProofError(String),
    #[error("An invalid transaction has been encountered: {0}")]
    TransactionError(#[from] TransactionError),
    #[error(
        "Merkle root did not match for {mr_tree} at height {at_height}. Expected {actual_hex} to equal {expected_hex}"
    )]
    InvalidMrRoot {
        mr_tree: String,
        at_height: u64,
        expected_hex: String,
        actual_hex: String,
    },
    #[error("Invalid MMR position {mmr_position} at height {at_height}")]
    InvalidMmrPosition { at_height: u64, mmr_position: u64 },
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),
    #[error("RPC status: {0}")]
    RpcStatus(#[from] RpcStatus),
    #[error("Could not convert data:{0}")]
    ConversionError(String),
    #[error("MerkleMountainRangeError: {0}")]
    MerkleMountainRangeError(#[from] MerkleMountainRangeError),
    #[error("Connectivity error: {0}")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("Validation error: {0}")]
    ValidationError(#[from] ValidationError),
    #[error("No sync peers")]
    NoSyncPeers,
    #[error("Sync failed for all peers")]
    FailedSyncAllPeers,
    #[error("Peer {peer} exceeded maximum permitted sync latency. latency: {latency:.2?}s, max: {max_latency:.2?}s")]
    MaxLatencyExceeded {
        peer: NodeId,
        latency: Duration,
        max_latency: Duration,
    },
    #[error("All sync peers exceeded max allowed latency")]
    AllSyncPeersExceedLatency,
    #[error("FixedHash size error: {0}")]
    FixedHashSizeError(#[from] FixedHashSizeError),
    #[error("No more sync peers available: {0}")]
    NoMoreSyncPeers(String),
    #[error("Could not find peer info")]
    PeerNotFound,
    #[error("Sparse Merkle Tree error: {0}")]
    SMTError(#[from] SMTError),
}

impl From<TryFromIntError> for HorizonSyncError {
    fn from(err: TryFromIntError) -> Self {
        HorizonSyncError::ConversionError(err.to_string())
    }
}

impl From<RangeProofError> for HorizonSyncError {
    fn from(e: RangeProofError) -> Self {
        HorizonSyncError::RangeProofError(e.to_string())
    }
}

impl HorizonSyncError {
    pub fn get_ban_reason(&self, short_ban: Duration, long_ban: Duration) -> Option<BanReason> {
        match self {
            // no ban
            HorizonSyncError::ChainStorageError(_) |
            HorizonSyncError::NoSyncPeers |
            HorizonSyncError::FailedSyncAllPeers |
            HorizonSyncError::AllSyncPeersExceedLatency |
            HorizonSyncError::ConnectivityError(_) |
            HorizonSyncError::RpcError(_) |
            HorizonSyncError::RpcStatus(_) |
            HorizonSyncError::NoMoreSyncPeers(_) |
            HorizonSyncError::PeerNotFound |
            HorizonSyncError::JoinError(_) => None,

            // short ban
            err @ HorizonSyncError::MaxLatencyExceeded { .. } => Some(BanReason {
                reason: format!("{}", err),
                ban_duration: short_ban,
            }),

            // long ban
            err @ HorizonSyncError::IncorrectResponse(_) |
            err @ HorizonSyncError::FinalStateValidationFailed(_) |
            err @ HorizonSyncError::RangeProofError(_) |
            err @ HorizonSyncError::InvalidMrRoot { .. } |
            err @ HorizonSyncError::InvalidMmrPosition { .. } |
            err @ HorizonSyncError::ConversionError(_) |
            err @ HorizonSyncError::MerkleMountainRangeError(_) |
            err @ HorizonSyncError::FixedHashSizeError(_) |
            err @ HorizonSyncError::TransactionError(_) => Some(BanReason {
                reason: format!("{}", err),
                ban_duration: long_ban,
            }),

            HorizonSyncError::ValidationError(err) => ValidationError::get_ban_reason(err, Some(long_ban)),
        }
    }
}

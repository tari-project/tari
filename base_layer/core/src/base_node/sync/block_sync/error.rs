//  Copyright 2020, The Tari Project
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

use std::time::Duration;

use tari_common_types::types::FixedHashSizeError;
use tari_network::{identity::PeerId, NetworkError};
use tari_rpc_framework::{RpcError, RpcStatus, RpcStatusCode};

use crate::{
    chain_storage::ChainStorageError,
    common::{BanPeriod, BanReason},
    validation::ValidationError,
};

#[derive(Debug, thiserror::Error)]
pub enum BlockSyncError {
    #[error("Async validation task failed: {0}")]
    AsyncTaskFailed(#[from] tokio::task::JoinError),
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),
    #[error("RPC request failed: {0}")]
    RpcRequestError(#[from] RpcStatus),
    #[error("Chain storage error: {0}")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Peer sent a block that did not form a chain. Expected hash = {expected}, got = {got}")]
    BlockWithoutParent { expected: String, got: String },
    #[error("Network Error: {0}")]
    NetworkError(#[from] NetworkError),
    #[error("No more sync peers available: {0}")]
    NoMoreSyncPeers(String),
    #[error("Block validation failed: {0}")]
    ValidationError(#[from] ValidationError),
    #[error("Failed to construct valid chain block")]
    FailedToConstructChainBlock,
    #[error("Peer sent unknown hash")]
    UnknownHeaderHash(String),
    #[error("Peer sent block with invalid block body")]
    InvalidBlockBody(String),
    #[error("Peer {peer} exceeded maximum permitted sync latency. latency: {latency:.2?}, max: {max_latency:.2?}")]
    MaxLatencyExceeded {
        peer: PeerId,
        latency: Duration,
        max_latency: Duration,
    },
    #[error("All sync peers exceeded max allowed latency")]
    AllSyncPeersExceedLatency,
    #[error("FixedHash size error: {0}")]
    FixedHashSizeError(#[from] FixedHashSizeError),
    #[error("This sync round failed")]
    SyncRoundFailed,
    #[error("Could not find peer info")]
    PeerNotFound,
    #[error("Peer did not supply all the blocks they claimed they had: {0}")]
    PeerDidNotSupplyAllClaimedBlocks(String),
}

impl BlockSyncError {
    pub fn to_short_str(&self) -> &'static str {
        match self {
            BlockSyncError::RpcError(_) => "RpcError",
            BlockSyncError::RpcRequestError(status) if status.as_status_code() == RpcStatusCode::Timeout => {
                "RpcTimeout"
            },
            BlockSyncError::RpcRequestError(_) => "RpcRequestError",
            BlockSyncError::AsyncTaskFailed(_) => "AsyncTaskFailed",
            BlockSyncError::ChainStorageError(_) => "ChainStorageError",
            BlockSyncError::BlockWithoutParent { .. } => "PeerSentBlockThatDidNotFormAChain",
            BlockSyncError::NetworkError(_) => "NetworkError",
            BlockSyncError::NoMoreSyncPeers(_) => "NoMoreSyncPeers",
            BlockSyncError::ValidationError(_) => "ValidationError",
            BlockSyncError::FailedToConstructChainBlock => "FailedToConstructChainBlock",
            BlockSyncError::UnknownHeaderHash(_) => "UnknownHeaderHash",
            BlockSyncError::InvalidBlockBody(_) => "InvalidBlockBody",
            BlockSyncError::MaxLatencyExceeded { .. } => "MaxLatencyExceeded",
            BlockSyncError::AllSyncPeersExceedLatency => "AllSyncPeersExceedLatency",
            BlockSyncError::FixedHashSizeError(_) => "FixedHashSizeError",
            BlockSyncError::SyncRoundFailed => "SyncRoundFailed",
            BlockSyncError::PeerNotFound => "PeerNotFound",
            BlockSyncError::PeerDidNotSupplyAllClaimedBlocks(_) => "PeerDidNotSupplyAllClaimedBlocks",
        }
    }
}

impl BlockSyncError {
    pub fn get_ban_reason(&self) -> Option<BanReason> {
        match self {
            // no ban
            BlockSyncError::AsyncTaskFailed(_) |
            BlockSyncError::ConnectivityError(_) |
            BlockSyncError::NoMoreSyncPeers(_) |
            BlockSyncError::AllSyncPeersExceedLatency |
            BlockSyncError::FailedToConstructChainBlock |
            BlockSyncError::PeerNotFound |
            BlockSyncError::SyncRoundFailed => None,
            BlockSyncError::ChainStorageError(e) => e.get_ban_reason(),
            // short ban
            err @ BlockSyncError::MaxLatencyExceeded { .. } |
            err @ BlockSyncError::PeerDidNotSupplyAllClaimedBlocks(_) |
            err @ BlockSyncError::RpcError(_) |
            err @ BlockSyncError::RpcRequestError(_) => Some(BanReason {
                reason: format!("{}", err),
                ban_duration: BanPeriod::Short,
            }),

            // long ban
            err @ BlockSyncError::BlockWithoutParent { .. } |
            err @ BlockSyncError::UnknownHeaderHash(_) |
            err @ BlockSyncError::InvalidBlockBody(_) |
            err @ BlockSyncError::FixedHashSizeError(_) => Some(BanReason {
                reason: format!("{}", err),
                ban_duration: BanPeriod::Long,
            }),

            BlockSyncError::ValidationError(err) => ValidationError::get_ban_reason(err),
        }
    }
}

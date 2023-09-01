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
use crate::common::BanReason;
use tari_comms::{
    connectivity::ConnectivityError,
    peer_manager::NodeId,
    protocol::rpc::{RpcError, RpcStatus},
};

use crate::{blocks::BlockError, chain_storage::ChainStorageError, validation::ValidationError};

#[derive(Debug, thiserror::Error)]
pub enum BlockHeaderSyncError {
    #[error("No more sync peers available: {0}")]
    NoMoreSyncPeers(String),
    #[error("RPC error: {0}")]
    RpcError(#[from] RpcError),
    #[error("RPC request failed: {0}")]
    RpcRequestError(#[from] RpcStatus),
    #[error("Peer sent invalid header: {0}")]
    ReceivedInvalidHeader(String),
    #[error("Chain storage error: {0}")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Validation failed: {0}")]
    ValidationFailed(#[from] ValidationError),
    #[error("Sync failed for all peers")]
    SyncFailedAllPeers,
    #[error("Peer sent a found hash index that was out of range (Expected less than {0}, Found: {1})")]
    FoundHashIndexOutOfRange(u64, u64),
    #[error("Failed to ban peer: {0}")]
    FailedToBan(ConnectivityError),
    #[error("Connectivity Error: {0}")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("Node is still not in sync. Sync will be retried with another peer if possible.")]
    NotInSync,
    #[error("Unable to locate start hash `{0}`")]
    StartHashNotFound(String),
    #[error("Expected header height {expected} got {actual}")]
    InvalidBlockHeight { expected: u64, actual: u64 },
    #[error("Unable to find chain split from peer `{0}`")]
    ChainSplitNotFound(NodeId),
    #[error("Invalid protocol response: {0}")]
    InvalidProtocolResponse(String),
    #[error("Header at height {height} did not form a chain. Expected {actual} to equal the previous hash {expected}")]
    ChainLinkBroken {
        height: u64,
        actual: String,
        expected: String,
    },
    #[error("Block error: {0}")]
    BlockError(#[from] BlockError),
    #[error(
        "Peer claimed a stronger chain than they were able to provide. Claimed {claimed}, Actual: {actual:?}, local: \
         {local}"
    )]
    PeerSentInaccurateChainMetadata {
        claimed: u128,
        actual: Option<u128>,
        local: u128,
    },
    #[error("This peer sent too many headers ({0}) in response to a chain split request")]
    PeerSentTooManyHeaders(usize),
    #[error("Peer {peer} exceeded maximum permitted sync latency. latency: {latency:.2?}s, max: {max_latency:.2?}s")]
    MaxLatencyExceeded {
        peer: NodeId,
        latency: Duration,
        max_latency: Duration,
    },
    #[error("All sync peers exceeded max allowed latency")]
    AllSyncPeersExceedLatency,
}

impl BlockHeaderSyncError {
    pub fn get_ban_reason(&self, short_ban: Duration, long_ban: Duration) -> Option<BanReason> {
        match self {
            // no ban
            BlockHeaderSyncError::NoMoreSyncPeers(_) |
            BlockHeaderSyncError::RpcError(_) |
            BlockHeaderSyncError::RpcRequestError(_) |
            BlockHeaderSyncError::SyncFailedAllPeers |
            BlockHeaderSyncError::FailedToBan(_) |
            BlockHeaderSyncError::AllSyncPeersExceedLatency | 
            BlockHeaderSyncError::ConnectivityError(_) |
            BlockHeaderSyncError::NotInSync |
            BlockHeaderSyncError::ChainStorageError(_) => None,

            // short ban
            err @ BlockHeaderSyncError::MaxLatencyExceeded { .. } => Some(BanReason {
                reason: format!("{}", err),
                ban_duration: short_ban,
            }),

            // long ban
            err @ BlockHeaderSyncError::ReceivedInvalidHeader(_) |       
            err @ BlockHeaderSyncError::ValidationFailed(_) |
            err @ BlockHeaderSyncError::FoundHashIndexOutOfRange(_, _) |
            err @ BlockHeaderSyncError::StartHashNotFound(_) |
            err @ BlockHeaderSyncError::InvalidBlockHeight { .. } |
            err @ BlockHeaderSyncError::ChainSplitNotFound(_) |
            err @ BlockHeaderSyncError::InvalidProtocolResponse(_) |
            err @ BlockHeaderSyncError::ChainLinkBroken { .. } |
            err @ BlockHeaderSyncError::BlockError(_) |
            err @ BlockHeaderSyncError::PeerSentInaccurateChainMetadata { .. } |
            err @ BlockHeaderSyncError::PeerSentTooManyHeaders(_) => Some(BanReason {
                reason: format!("{}", err),
                ban_duration: long_ban,
            }),
        }
    }
}

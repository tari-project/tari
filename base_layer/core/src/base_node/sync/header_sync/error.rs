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

use tari_comms::{
    connectivity::ConnectivityError,
    peer_manager::NodeId,
    protocol::rpc::{RpcError, RpcStatus},
};

use crate::{blocks::BlockError, chain_storage::ChainStorageError, validation::ValidationError};

#[derive(Debug, thiserror::Error)]
pub enum BlockHeaderSyncError {
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
    #[error(
        "Peer could not provide a stronger chain than the local chain. Claimed was {claimed} but validated was \
         {actual} (local: {local})"
    )]
    WeakerChain { claimed: u128, actual: u128, local: u128 },
    #[error("Node is still not in sync. Sync will be retried with another peer if possible.")]
    NotInSync,
    #[error("Unable to locate start hash `{0}`")]
    StartHashNotFound(String),
    #[error("Expected header height {expected} got {actual}")]
    InvalidBlockHeight { expected: u64, actual: u64 },
    #[error("Unable to find chain split from peer `{0}`")]
    ChainSplitNotFound(NodeId),
    #[error("Node could not find any other node with which to sync. Silence.")]
    NetworkSilence,
    #[error("Invalid protocol response: {0}")]
    InvalidProtocolResponse(String),
    #[error("Headers did not form a chain. Expected {actual} to equal the previous hash {expected}")]
    ChainLinkBroken { actual: String, expected: String },
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
    #[error("Peer {peer} exceeded maximum permitted sync latency. latency: {latency:.2?}s, max: {max_latency:.2?}s")]
    MaxLatencyExceeded {
        peer: NodeId,
        latency: Duration,
        max_latency: Duration,
    },
    #[error("All sync peers exceeded max allowed latency")]
    AllSyncPeersExceedLatency,
}

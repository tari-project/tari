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

use tari_comms::{
    connectivity::ConnectivityError,
    peer_manager::NodeId,
    protocol::rpc::{RpcError, RpcStatus},
};
use tari_mmr::error::MerkleMountainRangeError;
use thiserror::Error;
use tokio::task;

use crate::{
    base_node::comms_interface::CommsInterfaceError,
    chain_storage::{ChainStorageError, MmrTree},
    transactions::transaction_components::TransactionError,
    validation::ValidationError,
};

#[derive(Debug, Error)]
pub enum HorizonSyncError {
    #[error("Peer sent an invalid response: {0}")]
    IncorrectResponse(String),
    #[error("Chain storage error: {0}")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Comms interface error: {0}")]
    CommsInterfaceError(#[from] CommsInterfaceError),
    #[error("Final state validation failed: {0}")]
    FinalStateValidationFailed(ValidationError),
    #[error("Join error: {0}")]
    JoinError(#[from] task::JoinError),
    #[error("Invalid kernel signature: {0}")]
    InvalidKernelSignature(TransactionError),
    #[error("MMR did not match for {mmr_tree} at height {at_height}. Expected {actual_hex} to equal {expected_hex}")]
    InvalidMmrRoot {
        mmr_tree: MmrTree,
        at_height: u64,
        expected_hex: String,
        actual_hex: String,
    },
    #[error("Invalid range proof for output:{0} : {1}")]
    InvalidRangeProof(String, String),
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
}

impl From<TryFromIntError> for HorizonSyncError {
    fn from(err: TryFromIntError) -> Self {
        HorizonSyncError::ConversionError(err.to_string())
    }
}

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

use primitive_types::U512;
use tari_comms::{
    connectivity::ConnectivityError,
    peer_manager::NodeId,
    protocol::rpc::{RpcError, RpcStatus},
};
use tari_node_components::blocks::BlockError;
use tari_transaction_components::{BanPeriod, BanReason};

use crate::{chain_storage::ChainStorageError, validation::ValidationError};

#[derive(Debug, thiserror::Error)]
pub enum BlockHeaderSyncError {
    #[error("No more sync peers available: {0}")]
    NoMoreSyncPeers(String),
    #[error("Could not find peer info")]
    PeerNotFound,
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
    #[error("The sync peer could not take an RPC session ({0}). Sync will be retried with another peer if possible.")]
    SyncPeerUnavailable(RpcError),
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
        claimed: U512,
        actual: Option<U512>,
        local: U512,
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
    #[error("Unable to get TargetDifficulties: ({0})")]
    TargetDifficultiesError(String),
    #[error(
        "Refused a deep reorg (GHSA-3qmx-q9pv-f3m4): the peer's chain splits from ours at height {split_height}, \
         below the advisory's activation height {activation_height}, while our tip is already at {local_tip_height}"
    )]
    ReorgBelowGhsaActivation {
        split_height: u64,
        activation_height: u64,
        local_tip_height: u64,
    },
}

impl BlockHeaderSyncError {
    /// The error for a failure to connect an RPC client to a sync peer. A peer that could not take a session (see
    /// `RpcError::is_handshake_unavailable`) is skipped without a ban; any other connect error is an `RpcError`.
    pub(crate) fn from_connect_error(err: RpcError) -> Self {
        if err.is_handshake_unavailable() {
            Self::SyncPeerUnavailable(err)
        } else {
            Self::RpcError(err)
        }
    }

    pub fn get_ban_reason(&self) -> Option<BanReason> {
        match self {
            // no ban
            BlockHeaderSyncError::NoMoreSyncPeers(_) |
            BlockHeaderSyncError::SyncFailedAllPeers |
            BlockHeaderSyncError::FailedToBan(_) |
            BlockHeaderSyncError::AllSyncPeersExceedLatency |
            BlockHeaderSyncError::ConnectivityError(_) |
            BlockHeaderSyncError::NotInSync |
            BlockHeaderSyncError::SyncPeerUnavailable(_) |
            BlockHeaderSyncError::TargetDifficultiesError(_) |
            BlockHeaderSyncError::PeerNotFound => None,
            BlockHeaderSyncError::ChainStorageError(e) => e.get_ban_reason(),

            // short ban
            err @ BlockHeaderSyncError::MaxLatencyExceeded { .. } |
            err @ BlockHeaderSyncError::RpcError { .. } |
            err @ BlockHeaderSyncError::RpcRequestError { .. } |
            err @ BlockHeaderSyncError::PeerSentInaccurateChainMetadata { .. } => Some(BanReason {
                reason: format!("{err}"),
                ban_duration: BanPeriod::Short,
            }),

            // long ban
            err @ BlockHeaderSyncError::ReceivedInvalidHeader(_) |
            err @ BlockHeaderSyncError::FoundHashIndexOutOfRange(_, _) |
            err @ BlockHeaderSyncError::StartHashNotFound(_) |
            err @ BlockHeaderSyncError::InvalidBlockHeight { .. } |
            err @ BlockHeaderSyncError::ChainSplitNotFound(_) |
            err @ BlockHeaderSyncError::InvalidProtocolResponse(_) |
            err @ BlockHeaderSyncError::ChainLinkBroken { .. } |
            err @ BlockHeaderSyncError::BlockError(_) |
            err @ BlockHeaderSyncError::PeerSentTooManyHeaders(_) |
            err @ BlockHeaderSyncError::ReorgBelowGhsaActivation { .. } => Some(BanReason {
                reason: format!("{err}"),
                ban_duration: BanPeriod::Long,
            }),

            BlockHeaderSyncError::ValidationFailed(err) => ValidationError::get_ban_reason(err),
        }
    }
}

#[cfg(test)]
mod connect_error_test {
    use tari_comms::protocol::rpc::{HandshakeRejectReason, RpcError, RpcHandshakeError};

    use super::*;

    /// A sync peer that cannot take a session while connecting is skipped without a ban; one that misbehaves while
    /// connecting, and any RPC error after connecting, is still banned
    #[test]
    fn only_a_busy_or_unreachable_sync_peer_escapes_the_ban() {
        let unavailable = || {
            vec![
                RpcHandshakeError::Rejected(HandshakeRejectReason::NoServerSessionsAvailable("busy")),
                RpcHandshakeError::Rejected(HandshakeRejectReason::NoClientSessionsAvailable("busy")),
                RpcHandshakeError::ServerClosedRequest,
                RpcHandshakeError::Io(std::io::ErrorKind::ConnectionReset.into()),
            ]
        };
        for err in unavailable() {
            let err = BlockHeaderSyncError::from_connect_error(RpcError::HandshakeError(err));
            assert!(err.get_ban_reason().is_none(), "{err} was banned");
        }

        let misbehaving = vec![
            RpcError::ReplyTimeout,
            RpcError::HandshakeError(RpcHandshakeError::DecodeError(prost::DecodeError::new("bad"))),
            RpcError::HandshakeError(RpcHandshakeError::FrameTooLarge { max: 1024 }),
            RpcError::HandshakeError(RpcHandshakeError::Rejected(HandshakeRejectReason::UnsupportedVersion)),
        ];
        for err in misbehaving {
            let err = BlockHeaderSyncError::from_connect_error(err);
            assert!(err.get_ban_reason().is_some(), "{err} was not banned");
        }

        // After connecting, the same errors are still banned
        for err in unavailable() {
            let err = BlockHeaderSyncError::from(RpcError::HandshakeError(err));
            assert!(err.get_ban_reason().is_some(), "{err} was not banned after connecting");
        }
        assert!(
            BlockHeaderSyncError::from(RpcError::ServerClosedRequest)
                .get_ban_reason()
                .is_some()
        );
    }
}

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
    base_node::comms_interface::CommsInterfaceError,
    chain_storage::ChainStorageError,
    proof_of_work::PowError,
};
use log::*;
use std::time::Duration;
use tari_comms::{connectivity::ConnectivityError, peer_manager::PeerManagerError};

// If more than one sync peer discovered with the correct chain, enable or disable the selection of a random sync peer
// to query headers and blocks.
const RANDOM_SYNC_PEER_WITH_CHAIN: bool = true;
// The default length of time to ban a misbehaving/malfunctioning sync peer (24 hours)
const DEFAULT_PEER_BAN_DURATION: Duration = Duration::from_secs(24 * 60 * 60);
// The length of time for a short term ban of a misbehaving/malfunctioning sync peer
const SHORT_TERM_PEER_BAN_DURATION: Duration = Duration::from_secs(30 * 60);

// TODO: Deprecate
#[derive(Debug, thiserror::Error)]
pub enum BaseNodeRequestError {
    // #[error("Maximum request attempts reached error")]
    // MaxRequestAttemptsReached,
    // #[error("No sync peers error")]
    // NoSyncPeers,
    #[error("Chain storage error: `{0}`")]
    ChainStorageError(#[from] ChainStorageError),
    #[error("Peer manager error: `{0}`")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Connectivity error: `{0}`")]
    ConnectivityError(#[from] ConnectivityError),
    #[error("Comms interface error: `{0}`")]
    CommsInterfaceError(#[from] CommsInterfaceError),
    #[error("PowError: `{0}`")]
    PowError(#[from] PowError),
}

/// Configuration for the Sync Peer Selection and Banning.
#[derive(Clone, Copy)]
pub struct SyncPeerConfig {
    pub random_sync_peer_with_chain: bool,
    pub peer_ban_duration: Duration,
    pub short_term_peer_ban_duration: Duration,
}

impl Default for SyncPeerConfig {
    fn default() -> Self {
        Self {
            random_sync_peer_with_chain: RANDOM_SYNC_PEER_WITH_CHAIN,
            peer_ban_duration: DEFAULT_PEER_BAN_DURATION,
            short_term_peer_ban_duration: SHORT_TERM_PEER_BAN_DURATION,
        }
    }
}

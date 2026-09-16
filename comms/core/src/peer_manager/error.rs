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
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

// use std::sync::PoisonError;

use std::fmt;

use multiaddr::Multiaddr;
use tari_common_sqlite::error::StorageError;
use tari_utilities::hex::{Hex, HexError};
use thiserror::Error;
use tokio::task::JoinError;

use crate::peer_manager::NodeId;

/// Error type for [PeerManager](super::PeerManager).
#[derive(Debug, Error, Clone)]
pub enum PeerManagerError {
    #[error("The requested peer does not exist: {0}")]
    PeerNotFound(DisplayVec),
    #[error("DB Data inconsistency: {0}")]
    DataInconsistency(String),
    #[error("The peer has been banned")]
    BannedPeer,
    #[error("A problem has been encountered with the sql database: {0}")]
    StorageError(String),
    #[error("An error occurred while migrating the database: {0}")]
    MigrationError(String),
    #[error("Identity signature is invalid")]
    InvalidIdentitySignature,
    #[error("Identity signature missing")]
    MissingIdentitySignature,
    #[error("Invalid peer address: {0}")]
    MultiaddrError(String),
    #[error("Unable to parse any of the network addresses offered by the connecting peer")]
    PeerIdentityNoValidAddresses,
    #[error("Invalid peer feature bits '{bits:#x}'")]
    InvalidPeerFeatures { bits: u32 },
    #[error("Address {address} not found for peer {node_id}")]
    AddressNotFoundError { address: Multiaddr, node_id: NodeId },
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("Invalid version string")]
    InvalidVersionString,
    #[error("Peer version {peer_version} to older than the minimum required version {min_version}")]
    PeerVersionTooOld { min_version: String, peer_version: String },
    #[error("Hex conversion error: `{0}`")]
    HexError(String),
    #[error("Tokio task join error: `{0}`")]
    JoinError(String),
    #[error("Process error: `{0}`")]
    ProcessError(String),
}
impl From<JoinError> for PeerManagerError {
    fn from(err: JoinError) -> Self {
        PeerManagerError::JoinError(err.to_string())
    }
}

impl From<StorageError> for PeerManagerError {
    fn from(err: StorageError) -> Self {
        PeerManagerError::StorageError(err.to_string())
    }
}

impl From<HexError> for PeerManagerError {
    fn from(value: HexError) -> Self {
        PeerManagerError::HexError(value.to_string())
    }
}

impl From<std::io::Error> for PeerManagerError {
    fn from(value: std::io::Error) -> Self {
        PeerManagerError::StorageError(value.to_string())
    }
}

/// Display helper struct for a vector of strings.
#[derive(Debug, Error, Clone)]
pub struct DisplayVec(Vec<NodeId>);

impl fmt::Display for DisplayVec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[{}]",
            self.0.iter().map(|v| v.to_hex()).collect::<Vec<_>>().join(", ")
        )
    }
}

impl PeerManagerError {
    pub fn peer_not_found(peer: &NodeId) -> Self {
        PeerManagerError::PeerNotFound(DisplayVec(vec![peer.clone()]))
    }

    pub fn peers_not_found<T>(peers: T) -> Self
    where T: AsRef<[NodeId]> {
        PeerManagerError::PeerNotFound(DisplayVec(peers.as_ref().to_vec()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peer_not_found() {
        let peers = [
            NodeId::from_hex("abb1556d806c2ff042f433ca0a").unwrap(),
            NodeId::from_hex("ba9ab662a6d974c5a607562326").unwrap(),
            NodeId::from_hex("97676095b1901327bdc36e8cb6").unwrap(),
        ];

        let error = PeerManagerError::peers_not_found(&peers);
        assert_eq!(
            error.to_string(),
            "The requested peer does not exist: [abb1556d806c2ff042f433ca0a, ba9ab662a6d974c5a607562326, \
             97676095b1901327bdc36e8cb6]"
        );

        let error = PeerManagerError::peer_not_found(&peers[0]);
        assert_eq!(
            error.to_string(),
            "The requested peer does not exist: [abb1556d806c2ff042f433ca0a]"
        );
    }
}

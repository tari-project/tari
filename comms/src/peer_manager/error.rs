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

use std::sync::PoisonError;
use tari_storage::KeyValStoreError;
use thiserror::Error;

#[derive(Debug, Error, Clone)]
pub enum PeerManagerError {
    #[error("The requested peer does not exist")]
    PeerNotFoundError,
    #[error("The peer has been banned")]
    BannedPeer,
    #[error("A problem has been encountered with the database: {0}")]
    DatabaseError(#[from] KeyValStoreError),
    #[error("An error occurred while migrating the database: {0}")]
    MigrationError(String),
}

impl PeerManagerError {
    /// Returns true if this error indicates that the peer is not found, otherwise false
    pub fn is_peer_not_found(&self) -> bool {
        matches!(self, PeerManagerError::PeerNotFoundError)
    }
}

impl<T> From<PoisonError<T>> for PeerManagerError {
    fn from(_: PoisonError<T>) -> Self {
        PeerManagerError::DatabaseError(KeyValStoreError::PoisonedAccess)
    }
}

// Copyright 2019, The Tari Project
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
    connection_manager::{error::ConnectionManagerError, peer_connection::PeerConnection},
    peer_manager::Peer,
};
use futures::channel::oneshot;
use tari_shutdown::ShutdownSignal;

/// The state of the dial request
pub struct DialState {
    /// Number of dial attempts
    attempts: usize,
    /// This peer being dialed
    pub peer: Box<Peer>,
    /// Cancel signal
    cancel_signal: ShutdownSignal,
    /// Reply channel for a connection result
    pub reply_tx: oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
}

impl DialState {
    /// Create a new DialState for the given NodeId
    pub fn new(
        peer: Box<Peer>,
        reply_tx: oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
        cancel_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            peer,
            attempts: 0,
            reply_tx,
            cancel_signal,
        }
    }

    /// Take ownership of the cancel receiver if this DialState has ownership of one
    pub fn get_cancel_signal(&self) -> ShutdownSignal {
        self.cancel_signal.clone()
    }

    /// Increment the number of attempts
    pub fn inc_attempts(&mut self) -> &mut Self {
        self.attempts += 1;
        self
    }

    pub fn num_attempts(&self) -> usize {
        self.attempts
    }
}

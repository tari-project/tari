// Copyright 2020, The Tari Project
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
    connection_manager::ConnectionManagerError,
    peer_manager::PeerManagerError,
    protocol::ProtocolExtensionError,
    tor::HiddenServiceControllerError,
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CommsBuilderError {
    #[error("Peer manager error: {0}")]
    PeerManagerError(#[from] PeerManagerError),
    #[error("Connection manager error: {0}")]
    ConnectionManagerError(#[from] ConnectionManagerError),
    #[error("Node identity not set. Call `with_node_identity(node_identity)` on [CommsBuilder]")]
    NodeIdentityNotSet,
    #[error("The PeerStorage was not provided to the CommsBuilder. Use `with_peer_storage` to set it.")]
    PeerStorageNotProvided,
    #[error("Unable to receive a ConnectionManagerEvent within timeout")]
    ConnectionManagerEventStreamTimeout,
    #[error("ConnectionManagerEvent stream unexpectedly closed")]
    ConnectionManagerEventStreamClosed,
    #[error("Receiving on ConnectionManagerEvent stream lagged unexpectedly")]
    ConnectionManagerEventStreamLagged,
    #[error("Comms protocol extension failed to install: {0}")]
    CommsProtocolExtensionError(#[from] ProtocolExtensionError),
    #[error("Failed to initialize tor hidden service: {0}")]
    HiddenServiceControllerError(#[from] HiddenServiceControllerError),
}

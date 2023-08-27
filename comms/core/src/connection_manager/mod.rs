// Copyright 2019, The Taiji Project
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
//! # ConnectionManager
//!
//! This component is responsible for orchestrating PeerConnections, specifically:
//! - dialing peers,
//! - listening for peer connections on the configured transport,
//! - performing connection upgrades (noise protocol, identity and multiplexing),
//! - and, notifying the connectivity manager of changes in connection state (new connections, disconnects, etc)

mod dial_state;
mod dialer;
mod listener;
mod metrics;

mod common;
pub use common::{validate_address_and_source, validate_addresses, validate_addresses_and_source};

mod direction;
pub use direction::ConnectionDirection;

mod requester;
pub use requester::{ConnectionManagerRequest, ConnectionManagerRequester};

mod manager;
pub(crate) use manager::ConnectionManager;
pub use manager::{ConnectionManagerConfig, ConnectionManagerEvent, ListenerInfo};

mod error;
pub use error::{ConnectionManagerError, PeerConnectionError};

mod peer_connection;
pub use peer_connection::{ConnectionId, NegotiatedSubstream, PeerConnection, PeerConnectionRequest};

mod liveness;
pub(crate) use liveness::LivenessCheck;
pub use liveness::LivenessStatus;

mod wire_mode;

#[cfg(test)]
mod tests;

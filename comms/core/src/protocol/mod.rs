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

mod error;
pub use error::ProtocolError;

mod extensions;
pub use extensions::{ProtocolExtension, ProtocolExtensionContext, ProtocolExtensionError, ProtocolExtensions};

mod identity;
pub use identity::{identity_exchange, IdentityProtocolError};

mod negotiation;
pub use negotiation::ProtocolNegotiation;

mod network_info;
pub use network_info::NodeNetworkInfo;

mod protocols;
pub use protocols::{ProtocolEvent, ProtocolNotification, ProtocolNotificationRx, ProtocolNotificationTx, Protocols};

#[cfg(feature = "rpc")]
pub mod rpc;

pub mod messaging;

/// Represents a protocol id string (e.g. /tari/transactions/1.0.0).
/// This is atomically reference counted, so clones are shallow and cheap
pub type ProtocolId = bytes::Bytes;

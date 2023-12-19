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

// Much of this module is inspired or (more or less) verbatim from the Libra codebase.
// Copyright (c) The Libra Core Contributors
// SPDX-License-Identifier: Apache-2.0

//! # Transports
//!
//! Provides an abstraction for [Transport](self::Transport)s and several implemenations:
//! - [TCP](self::TcpTransport) - communication over TCP and IP4/IP6 and DNS
//! - [SOCKS](self::SocksTransport) - communication over a SOCKS5 proxy.
//! - [Memory](self::MemoryTransport) - in-process communication (mpsc channel), typically for testing.

use multiaddr::Multiaddr;
use tokio_stream::Stream;

mod dns;

pub mod predicate;

mod memory;
pub use memory::MemoryTransport;

mod socks;
pub use socks::{SocksConfig, SocksTransport};

mod tcp;
pub use tcp::TcpTransport;

mod tcp_with_tor;
mod hidden_service_transport;
pub use hidden_service_transport::HiddenServiceTransport;

pub use tcp_with_tor::TcpWithTorTransport;

/// Defines an abstraction for implementations that can dial and listen for connections over a provided address.
#[crate::async_trait]
pub trait Transport {
    /// The output of the transport after a connection is established
    type Output;
    /// Transport error type
    type Error: std::error::Error + Send + Sync;
    /// A stream which emits a (socket, source ip) tuple whenever a successful inbound connection is made
    type Listener: Stream<Item = Result<(Self::Output, Multiaddr), Self::Error>> + Send + Unpin;

    /// Listen for connections on the given multiaddr
    async fn listen(&self, addr: &Multiaddr) -> Result<(Self::Listener, Multiaddr), Self::Error>;

    /// Connect (dial) to the given multiaddr
    async fn dial(&self, addr: &Multiaddr) -> Result<Self::Output, Self::Error>;
}

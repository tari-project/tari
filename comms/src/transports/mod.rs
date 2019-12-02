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

use futures::{Future, Stream};
use multiaddr::Multiaddr;

mod tcp;

pub use tcp::TcpSocket;

pub trait Transport {
    /// The output of the transport after a connection is established
    type Output;
    /// Transport error type
    type Error: std::error::Error;
    /// A stream which emits `Self::Output` whenever a successful inbound connection is made
    type Inbound: Stream<Item = Result<Self::Output, Self::Error>> + Send;

    /// The future returned from the `listen` method.
    type ListenFuture: Future<Output = Result<(Self::Inbound, Multiaddr), Self::Error>>;
    /// The future returned from the `dial` method.
    type DialFuture: Future<Output = Result<Self::Output, Self::Error>>;

    /// Listen for connections on the given multiaddr
    fn listen(&self, addr: Multiaddr) -> Self::ListenFuture;

    /// Connect (dial) to the given multiaddr
    fn dial(&self, addr: Multiaddr) -> Self::DialFuture;
}

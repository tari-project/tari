//  Copyright 2019 The Tari Project
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

use crate::connection::ConnectionError;

/// The types of socket available
pub enum SocketType {
    Request,
    Reply,
    Router,
    Dealer,
    Pub,
    Sub,
    Push,
    Pull,
    Pair,
}

/// Result type used by `comms::connection` module
pub type Result<T> = std::result::Result<T, ConnectionError>;

/// Represents the linger behavior of a connection. This can, depending on the chosen behavior,
/// allow a connection to finish sending messages before disconnecting.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Linger {
    /// Linger until all messages have been sent
    Indefinitely,
    /// Don't linger, close the connection immediately
    Never,
    /// Linger for the specified time (in milliseconds) before disconnecting.
    Timeout(u32),
}

/// Direction of the connection
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Direction {
    /// Connection listens for incoming connections
    Inbound,
    /// Connection establishes an outbound connection
    Outbound,
}

/// Used to select the method to use when establishing the connection.
pub enum SocketEstablishment {
    /// Select bind or connect based on connection [Direction](./enum.Direction.html)
    Auto,
    /// Always bind on the socket
    Bind,
    /// Always connect on the socket
    Connect,
}

impl Default for SocketEstablishment {
    fn default() -> Self {
        SocketEstablishment::Auto
    }
}

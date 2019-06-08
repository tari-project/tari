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

/// # peer_connection
///
/// A peer connection is a bi-directional connection to a given [NetAddress]. The [Direction] of
/// a [PeerConnection] relates to which side initiate the connection. i.e for Inbound, this node
/// initiated the connection and waits for the peer to connect. For Outbound, this node is connecting
/// out to a listening socket. Frames can be sent and received over a single [Connection]. All received
/// messages are forwarded to a consumer connection.
///
/// The [PeerConnection] object starts a [Worker] which is responsible for establishing the required
/// connections. Two [Connections] are needed: a connection to/from the given [NetAddress] and a
/// connection to the consumer. A [ConnectionMonitor] is started which receives socket events from the
/// peer connection.
///
/// A [PeerConnection] consists of these modules:
///
/// 1. `connection` - responsible for starting and sending control messages to the PeerConnection
///                   worker thread.
/// 2. `context` - Builder for a `PeerConnectionContext` which is owned by a PeerConnection worker
///                thread. This provides all the information required to create the underlying
///                connections to the peer and consumer.
/// 3. `control` - Contains the control messages which can be sent from the [PeerConnection] to
///                the [Worker], as well as a thin wrapper around [std::sync::mpsc::Sender].
/// 4. `error` - Contains [PeerConnectionError]
/// 5. `worker` - Where all the work is done. Contains the code responsible for establishing
///               connections (peer and consumer), receiving messages to forward to the
///               consumer connection, updating the peer connection state from socket events
///               and receiving control messages and acting on them.
///
/// ## PeerConnectionState
///
/// +--------------------+
/// |                    |
/// |      Initial       |
/// |                    |
/// +--------------------+
///          |
///          |                            +------------------+
/// +--------------------+                 |                  |
/// |                    |       +---------|     Shutdown     |
/// |     Connecting     |       |         |                  |
/// |                    |-      |         +------------------+
/// +---------|----------+ \ +-----+       +------------------+
///           |              |     |       |                  |
///           |              |     |-------+  Disconnected    |
///  Accepted / Connected    |     |       |                  |
///           |             /+-----+       +------------------+
///           |            /     |         +------------------+
///           |           /      |         |                  |
/// +---------------------       |         |     Failed       |
/// |                    |       +----------                  |
/// |     Connected      |                 +------------------+
/// |                    |
/// +--------------------+
///
/// [PeerConnection](./connection/struct.PeerConnection.html]
/// [Direction](../types/enum.Direction.html]
/// [NetAddress](../net_address/enum.NetAddress.html]
/// [Connection](../connection/struct.Connection.html]
/// [Worker](./worker/struct.Worker.html]
/// [ConnectionMonitor](../monitor/struct.ConnectionMonitor.html]
/// [PeerConnectionError](./error/struct.PeerConnectionError.html]
mod connection;
mod context;
mod control;
mod error;
mod worker;

pub use self::{
    connection::{ConnectionId, PeerConnection, PeerConnectionSimpleState},
    context::{PeerConnectionContext, PeerConnectionContextBuilder},
    error::PeerConnectionError,
};

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

//! # Connection
//!
//! Related modules that abstract [0MQ] connections and other 0MQ related features,
//! address types and connections to peers.
//!
//! This module consists of:
//!
//! - [NetAddress]
//!
//! Represents an IP, Onion or I2P address.
//!
//! - Zero MQ module
//!
//! Thin wrappers of [0MQ]. Namely,
//! - [ZmqContext]
//! - [CurveEncryption]
//! - [ZmqEndpoint] trait
//! - [InprocAddress]
//!
//! - [Connection]
//!
//! Provides a connection builder and thin wrapper around a [0MQ] Router
//! (Inbound [Direction](./connection/enum.Direction.html) or Dealer (Outbound [Direction]) socket.
//!
//! - [DealerProxy]
//!
//! Async wrapper around [zmq_proxy_steerable] for use with [Connection].
//!
//! - [ConnectionMonitor]
//!
//! Receives socket events from [0MQ] connections. Wrapper around [zmq_socket_monitor].
//!
//! - [PeerConnection]
//!
//! Represents a connection to a peer. See [PeerConnection].
//!
//! [0MQ]: http://zeromq.org/
//! [NetAddress]: (./net_address/enum.NetAddress.html)
//! [Connection]: (./connection/struct.Connection.html)
//! [DealerProxy]: (./dealer_proxy/struct.DealerProxy.html)
//! [PeerConnection]: (./peer_connection/index.html)
//! [ZmqContext]: (./zmq/context/struct.ZmqContext.html)
//! [CurveEncryption]: (./zmq/curve_keypair/enum.CurveEncryption.html)
//! [ZmqEndpoint]: (./zmq/endpoint/trait.ZmqEndpoint.html)
//! [InprocAddress]: (./zmq/inproc_address/struct.InprocAddress.html)
//! [Direction]: (./connection/enum.Direction.html)
//! [zmq_proxy_steerable]: http://api.zeromq.org/4-1:zmq-proxy-steerable
//! [ConnectionMonitor]: ./monitor/struct.ConnectionMonitor.html
//! [zmq_socket_monitor]: http://api.zeromq.org/4-1:zmq-socket-monitor

#[macro_use]
mod macros;

pub mod connection;
pub mod dealer_proxy;
pub mod error;
pub mod monitor;
pub mod net_address;
pub mod peer_connection;
pub mod types;
pub mod zmq;

pub use self::{
    connection::{Connection, EstablishedConnection},
    dealer_proxy::{DealerProxy, DealerProxyError},
    error::ConnectionError,
    net_address::{NetAddress, NetAddressError, NetAddressesWithStats},
    peer_connection::{
        PeerConnection,
        PeerConnectionContextBuilder,
        PeerConnectionError,
        PeerConnectionSimpleState as PeerConnectionState,
    },
    types::{Direction, SocketEstablishment},
    zmq::{curve_keypair, CurveEncryption, CurvePublicKey, CurveSecretKey, InprocAddress, ZmqContext},
};

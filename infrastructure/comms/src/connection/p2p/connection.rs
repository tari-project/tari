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

use crate::connection::{
    message::FrameSet,
    p2p::{Direction, Linger},
    types::SocketType,
    zmq::{Context, CurveEncryption, InprocAddress, ZmqEndpoint},
    ConnectionError,
    Result,
};
use std::iter::IntoIterator;

/// Represents a low-level connection which can be bound an address
/// supported by [`ZeroMQ`] the `ZMQ_ROUTER` socket.
///
/// ```edition2018
/// # use tari_comms::connection::{
/// #   zmq::{Context, InprocAddress, curve_keypair, CurveEncryption},
/// #   p2p::{Linger, Direction, connection::Connection},
/// # };
///
///  let ctx  = Context::new();
///
///  let (secret_key, _public_key) = curve_keypair::generate().unwrap();
///
///  let addr = "inproc://docs-comms-inbound-connection".parse::<InprocAddress>().unwrap();
///
///  let conn = Connection::new(&ctx, Direction::Inbound)
///         .set_curve_encryption(CurveEncryption::Server {secret_key})
///         .set_linger(Linger::Never)
///         .set_max_message_size(Some(123))
///         .set_receive_hwm(1)
///         .set_send_hwm(2)
///         .establish(&addr)
///         .unwrap();
///
///   // Receive timeout is 1 so timeout error is returned
///   let result = conn.receive(1);
///   assert!(result.is_err());
/// ```
/// [`ZeroMQ`]: http://zeromq.org/
pub struct Connection<'a> {
    pub(crate) context: &'a Context,
    pub(crate) direction: Direction,
    pub(crate) recv_hwm: Option<i32>,
    pub(crate) send_hwm: Option<i32>,
    pub(crate) identity: Option<String>,
    pub(crate) linger: Linger,
    pub(crate) curve_encryption: CurveEncryption,
    pub(crate) max_message_size: Option<i64>,
    pub(crate) socks_proxy_addr: Option<String>,
    pub(crate) monitor_addr: Option<InprocAddress>,
}

impl<'a> Connection<'a> {
    /// Create a new InboundConnection
    pub fn new(context: &'a Context, direction: Direction) -> Self {
        Self {
            context,
            direction,
            recv_hwm: None,
            send_hwm: None,
            identity: None,
            linger: Linger::Never,
            curve_encryption: CurveEncryption::None,
            max_message_size: None,
            socks_proxy_addr: None,
            monitor_addr: None,
        }
    }

    /// Set receive high water mark
    pub fn set_receive_hwm(mut self, hwm: i32) -> Self {
        self.recv_hwm = Some(hwm);
        self
    }

    /// Set send high water mark
    pub fn set_send_hwm(mut self, hwm: i32) -> Self {
        self.send_hwm = Some(hwm);
        self
    }

    /// Set the connection identity
    pub fn set_identity(mut self, identity: &str) -> Self {
        self.identity = Some(identity.to_owned());
        self
    }

    /// Set the period for how long the underling socket connection should
    /// continue to send messages after this connection is dropped.
    pub fn set_linger(mut self, linger: Linger) -> Self {
        self.linger = linger;
        self
    }

    /// The maximum size in bytes of the inbound message. If a message is
    /// received which is larger, the connection will disconnect.
    /// Set to None for no limit
    pub fn set_max_message_size(mut self, msg_size: Option<i64>) -> Self {
        self.max_message_size = Some(msg_size.unwrap_or(-1));
        self
    }

    /// Set the InprocAddress to enable monitoring on the underlying socket.
    /// All socket events are sent to sent to this address
    pub fn set_monitor_addr(mut self, addr: InprocAddress) -> Self {
        self.monitor_addr = Some(addr);
        self
    }

    /// Set the ip:port of a SOCKS proxy to use for this connection
    pub fn set_socks_proxy_addr(mut self, addr: Option<String>) -> Self {
        self.socks_proxy_addr = addr;
        self
    }

    /// Set Curve25519 encryption for this connection.
    pub fn set_curve_encryption(mut self, encryption: CurveEncryption) -> Self {
        self.curve_encryption = encryption;
        self
    }

    /// Create the socket, configure it and bind/connect it to the given address
    pub fn establish<T: ZmqEndpoint>(self, addr: &T) -> Result<EstablishedConnection> {
        let socket = match self.direction {
            Direction::Inbound => self.context.socket(SocketType::Router).unwrap(),
            Direction::Outbound => self.context.socket(SocketType::Dealer).unwrap(),
        };

        let config_error_mapper = |e| ConnectionError::SocketError(format!("Unable to configure socket: {}", e));

        if self.direction == Direction::Inbound {
            socket.set_router_mandatory(true).map_err(config_error_mapper)?;
        }

        if let Some(v) = self.recv_hwm {
            socket.set_rcvhwm(v).map_err(config_error_mapper)?;
        }

        if let Some(v) = self.send_hwm {
            socket.set_sndhwm(v).map_err(config_error_mapper)?;
        }

        if let Some(ident) = self.identity {
            socket.set_identity(ident.as_bytes()).map_err(config_error_mapper)?;
        }

        if let Some(v) = self.max_message_size {
            socket.set_maxmsgsize(v).map_err(config_error_mapper)?;
        }

        match self.linger {
            Linger::Indefinitely => {
                socket.set_linger(-1).map_err(config_error_mapper)?;
            },

            Linger::Never => {
                socket.set_linger(0).map_err(config_error_mapper)?;
            },

            Linger::Timeout(t) => {
                socket.set_linger(t as i32).map_err(config_error_mapper)?;
            },
        }

        match self.curve_encryption {
            CurveEncryption::None => {},
            CurveEncryption::Server { secret_key } => {
                socket.set_curve_server(true).map_err(config_error_mapper)?;
                socket
                    .set_curve_secretkey(&secret_key.into_inner())
                    .map_err(config_error_mapper)?;
            },
            CurveEncryption::Client {
                secret_key,
                public_key,
                server_public_key,
            } => {
                socket
                    .set_curve_serverkey(&server_public_key.into_inner())
                    .map_err(config_error_mapper)?;
                socket
                    .set_curve_secretkey(&secret_key.into_inner())
                    .map_err(config_error_mapper)?;
                socket
                    .set_curve_publickey(&public_key.into_inner())
                    .map_err(config_error_mapper)?;
            },
        }

        if let Some(ref v) = self.socks_proxy_addr {
            socket.set_socks_proxy(Some(v)).map_err(config_error_mapper)?;
        }

        if let Some(ref v) = self.monitor_addr {
            socket
                .monitor(v.to_zmq_endpoint().as_str(), zmq::SocketEvent::ALL as i32)
                .map_err(|e| ConnectionError::SocketError(format!("Unable to set monitor address: {}", e)))?;
        }

        match self.direction {
            Direction::Inbound => socket.bind(&addr.to_zmq_endpoint()),
            Direction::Outbound => socket.connect(&addr.to_zmq_endpoint()),
        }
        .map_err(|e| ConnectionError::SocketError(format!("Failed to establish socket: {}", e)))?;

        Ok(EstablishedConnection { socket })
    }
}

/// Represents an established connection.
pub struct EstablishedConnection {
    socket: zmq::Socket,
}

impl EstablishedConnection {
    /// Receive a multipart message or return a `ConnectionError::Timeout` if the specified timeout has expired.
    /// This method may be repeatably called, probably in a loop in a separate thread, to receive multiple multipart
    /// messages.
    pub fn receive(&self, timeout_ms: u32) -> Result<FrameSet> {
        match self.socket.poll(zmq::POLLIN, timeout_ms as i64) {
            Ok(rc) => {
                match rc {
                    // Internal error when polling connection
                    -1 => Err(ConnectionError::SocketError("Failed to poll socket".to_string())),
                    // Nothing to receive
                    0 => Err(ConnectionError::Timeout),
                    // Ready to receive
                    _ => self.receive_multipart(),
                }
            },

            Err(e) => Err(ConnectionError::SocketError(format!("Failed to poll: {}", e))),
        }
    }

    /// Sends multipart message frames. This function is non-blocking.
    pub fn send<I, T>(&self, frames: I) -> Result<()>
    where
        I: IntoIterator<Item = T>,
        T: AsRef<[u8]>,
    {
        self.send_with_flags(frames, zmq::DONTWAIT)
    }

    /// Sends multipart message frames. This will block until the message is queued
    /// for sending.
    pub fn send_sync<I, T>(&self, frames: I) -> Result<()>
    where
        I: IntoIterator<Item = T>,
        T: AsRef<[u8]>,
    {
        self.send_with_flags(frames, 0)
    }

    fn send_with_flags<I, T>(&self, frames: I, flags: i32) -> Result<()>
    where
        I: IntoIterator<Item = T>,
        T: AsRef<[u8]>,
    {
        let mut last_frame: Option<T> = None;
        for frame in frames.into_iter() {
            if let Some(f) = last_frame.take() {
                self.send_frame(f, flags | zmq::SNDMORE)?;
            }
            last_frame = Some(frame);
        }
        if let Some(f) = last_frame {
            self.send_frame(f, flags)?;
        }

        Ok(())
    }

    fn send_frame<T>(&self, frame: T, flags: i32) -> Result<()>
    where T: AsRef<[u8]> {
        self.socket
            .send(frame.as_ref(), flags)
            .map_err(|e| ConnectionError::SocketError(format!("Error sending: {} ({})", e, e.to_raw())))
    }

    fn receive_multipart(&self) -> Result<FrameSet> {
        self.socket
            .recv_multipart(0)
            .map_err(|e| ConnectionError::SocketError(format!("Error receiving: {} ({})", e, e.to_raw())))
    }

    #[cfg(test)]
    pub fn get_socket(&self) -> &zmq::Socket {
        &self.socket
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::zmq::{curve_keypair, InprocAddress};

    #[test]
    fn sets_socketopts() {
        let ctx = Context::new();

        let addr = InprocAddress::random();
        let monitor_addr = InprocAddress::random();

        let conn = Connection::new(&ctx, Direction::Inbound)
            .set_identity("identity")
            .set_linger(Linger::Timeout(200))
            .set_max_message_size(Some(123))
            .set_receive_hwm(1)
            .set_send_hwm(2)
            .set_socks_proxy_addr(Some("127.0.0.1:9988".to_string()))
            .set_monitor_addr(monitor_addr)
            .establish(&addr)
            .unwrap();

        let sock = conn.get_socket();
        assert!(!sock.is_curve_server().unwrap());
        assert_eq!(200, sock.get_linger().unwrap());
        assert_eq!(123, sock.get_maxmsgsize().unwrap());
        assert_eq!("identity".as_bytes(), sock.get_identity().unwrap().as_slice());
        assert_eq!(1, sock.get_rcvhwm().unwrap());
        assert_eq!(2, sock.get_sndhwm().unwrap());
        assert_eq!(Ok("127.0.0.1:9988".to_string()), sock.get_socks_proxy().unwrap());
    }

    #[test]
    fn set_server_encryption() {
        let ctx = Context::new();

        let addr = InprocAddress::random();

        let (sk, _) = curve_keypair::generate().unwrap();
        let expected_sk = sk.clone();

        let conn = Connection::new(&ctx, Direction::Inbound)
            .set_curve_encryption(CurveEncryption::Server { secret_key: sk })
            .establish(&addr)
            .unwrap();

        let sock = conn.get_socket();
        assert!(sock.is_curve_server().unwrap());
        assert_eq!(expected_sk.into_inner(), sock.get_curve_secretkey().unwrap().as_slice());
    }

    #[test]
    fn set_client_encryption() {
        let ctx = Context::new();

        let addr = InprocAddress::random();

        let (sk, pk) = curve_keypair::generate().unwrap();
        let (_, spk) = curve_keypair::generate().unwrap();
        let expected_sk = sk.clone();
        let expected_pk = pk.clone();
        let expected_spk = spk.clone();

        let conn = Connection::new(&ctx, Direction::Inbound)
            .set_curve_encryption(CurveEncryption::Client {
                secret_key: sk,
                public_key: pk,
                server_public_key: spk,
            })
            .establish(&addr)
            .unwrap();

        let sock = conn.get_socket();
        assert!(!sock.is_curve_server().unwrap());
        assert_eq!(expected_sk.into_inner(), sock.get_curve_secretkey().unwrap().as_slice());
        assert_eq!(expected_pk.into_inner(), sock.get_curve_publickey().unwrap().as_slice());
        assert_eq!(
            expected_spk.into_inner(),
            sock.get_curve_serverkey().unwrap().as_slice()
        );
    }
}

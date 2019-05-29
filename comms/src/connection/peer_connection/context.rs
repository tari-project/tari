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

use std::convert::{TryFrom, TryInto};

use super::{ConnectionId, PeerConnectionError};

use crate::connection::{
    net_address::ip::SocketAddress,
    zmq::{Context, CurveEncryption, InprocAddress},
    ConnectionError,
    Direction,
    NetAddress,
    Result,
};

/// The default maximum message size which will be used if no maximum message size is set.
const DEFAULT_MAX_MSG_SIZE: u64 = 500 * 1024; // 500 kb
/// The default maximum number of retries before failing the connection.
const DEFAULT_MAX_RETRY_ATTEMPTS: u16 = 10;

/// Context for connecting to a Peer. This is handed to a PeerConnection and is used to establish the connection.
///
/// # Fields
///
/// `context` - the underlying connection context
/// `peer_address` - the address to listen (Direction::Inbound) or connect(Direction::Outbound)
/// `consumer_address` - the address to forward all received messages
/// `direction` - the connection direction (Inbound or Outbound)
/// `curve_encryption` - the [CurveEncryption] for the connection
/// `max_msg_size` - the maximum size of a incoming message
/// `socks_address` - optional address for a SOCKS proxy
///
/// [CurveEncryption]: ./../zmq/curve_keypair/struct.CurveEncryption.html
pub struct PeerConnectionContext {
    pub(crate) context: Context,
    pub(crate) peer_address: NetAddress,
    pub(crate) consumer_address: InprocAddress,
    pub(crate) direction: Direction,
    pub(crate) id: ConnectionId,
    pub(crate) curve_encryption: CurveEncryption,
    pub(crate) max_msg_size: u64,
    pub(crate) max_retry_attempts: u16,
    pub(crate) socks_address: Option<SocketAddress>,
}

impl<'a> TryFrom<PeerConnectionContextBuilder<'a>> for PeerConnectionContext {
    type Error = ConnectionError;

    /// Convert a PeerConnectionContextBuilder to a PeerConnectionContext
    fn try_from(builder: PeerConnectionContextBuilder<'a>) -> Result<Self> {
        builder.check_curve_encryption()?;

        let consumer_address = unwrap_prop(builder.consumer_address, "consumer_address")?;
        let context = unwrap_prop(builder.context, "context")?.clone();
        let curve_encryption = builder.curve_encryption;
        let direction = unwrap_prop(builder.direction, "direction")?;
        let id = unwrap_prop(builder.id, "id")?;
        let max_msg_size = builder.max_msg_size.unwrap_or(DEFAULT_MAX_MSG_SIZE);
        let max_retry_attempts = builder.max_retry_attempts.unwrap_or(DEFAULT_MAX_RETRY_ATTEMPTS);
        let peer_address = unwrap_prop(builder.address, "peer_address")?;
        let socks_address = builder.socks_address;

        Ok(PeerConnectionContext {
            consumer_address,
            context,
            curve_encryption,
            direction,
            id,
            max_msg_size,
            max_retry_attempts,
            peer_address,
            socks_address,
        })
    }
}

/// Local utility function to unwrap a builder property, or return a PeerConnectionError::InitializationError
#[inline(always)]
fn unwrap_prop<T>(prop: Option<T>, prop_name: &str) -> Result<T> {
    match prop {
        Some(t) => Ok(t),
        None => Err(ConnectionError::PeerError(PeerConnectionError::InitializationError(
            format!("Missing required connection property '{}'", prop_name),
        ))),
    }
}

/// Used to build a context for a PeerConnection. Fields
/// are the same as a [PeerConnectionContext].
///
/// # Example
///
/// ```edition2018
/// # use tari_comms::connection::{
/// #     Context,
/// #     InprocAddress,
/// #     Direction,
/// #     PeerConnectionContextBuilder,
/// #     PeerConnection,
/// # };
///
/// let ctx = Context::new();
///
/// let peer_context = PeerConnectionContextBuilder::new()
///    .set_id("123")
///    .set_context(&ctx)
///    .set_direction(Direction::Outbound)
///    .set_consumer_address(InprocAddress::random())
///    .set_address("127.0.0.1:8080".parse().unwrap())
///    .build()
///    .unwrap();
/// ```
///
/// [PeerConnectionContext]: ./struct.PeerConnectionContext.html
#[derive(Default)]
pub struct PeerConnectionContextBuilder<'a> {
    pub(super) address: Option<NetAddress>,
    pub(super) consumer_address: Option<InprocAddress>,
    pub(super) context: Option<&'a Context>,
    pub(super) curve_encryption: CurveEncryption,
    pub(super) direction: Option<Direction>,
    pub(super) id: Option<ConnectionId>,
    pub(super) max_msg_size: Option<u64>,
    pub(super) max_retry_attempts: Option<u16>,
    pub(super) socks_address: Option<SocketAddress>,
}

impl<'a> PeerConnectionContextBuilder<'a> {
    setter!(set_address, address, NetAddress);

    setter!(set_consumer_address, consumer_address, InprocAddress);

    setter!(set_context, context, &'a Context);

    setter!(set_direction, direction, Direction);

    setter!(set_max_retry_attempts, max_retry_attempts, u16);

    setter!(set_max_msg_size, max_msg_size, u64);

    setter!(set_socks_proxy, socks_address, SocketAddress);

    /// Return a new PeerConnectionContextBuilder
    pub fn new() -> Self {
        Default::default()
    }

    /// Set CurveEncryption. Defaults to the default of CurveEncryption.
    pub fn set_curve_encryption(mut self, enc: CurveEncryption) -> Self {
        self.curve_encryption = enc;
        self
    }

    pub fn set_id<T>(mut self, id: T) -> Self
    where T: AsRef<[u8]> {
        self.id = Some(id.as_ref().to_vec());
        self
    }

    /// Build the PeerConnectionContext.
    ///
    /// Will return an Err if any of the required fields are not set or if
    /// curve encryption is not set correctly for the connection direction.
    /// i.e CurveEncryption::Server must be set with Direction::Inbound and
    ///  CurveEncryption::Client must be set with Direction::Outbound.
    ///  CurveEncryption::None will succeed in either direction.
    pub fn build(self) -> Result<PeerConnectionContext> {
        self.try_into()
    }

    fn check_curve_encryption(&self) -> Result<()> {
        match self.direction {
            Some(ref direction) => match direction {
                Direction::Outbound => match self.curve_encryption {
                    CurveEncryption::None { .. } => Ok(()),
                    CurveEncryption::Client { .. } => Ok(()),
                    CurveEncryption::Server { .. } => Err(PeerConnectionError::InitializationError(
                        "'Client' curve encryption required for outbound connection".to_string(),
                    )
                    .into()),
                },
                Direction::Inbound => match self.curve_encryption {
                    CurveEncryption::None { .. } => Ok(()),
                    CurveEncryption::Client { .. } => Err(PeerConnectionError::InitializationError(
                        "'Server' curve encryption required for inbound connection".to_string(),
                    )
                    .into()),
                    CurveEncryption::Server { .. } => Ok(()),
                },
            },

            None => Err(
                PeerConnectionError::InitializationError("Direction not set for peer connection".to_string()).into(),
            ),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::{
        peer_connection::PeerConnectionError,
        zmq::{curve_keypair, Context, CurveEncryption, InprocAddress},
        ConnectionError,
        Direction,
        NetAddress,
        Result,
    };

    fn assert_initialization_error<T>(result: Result<T>, expected: &str) {
        if let Err(error) = result {
            match error {
                ConnectionError::PeerError(err) => match err {
                    PeerConnectionError::InitializationError(s) => {
                        assert_eq!(expected, s);
                    },
                    _ => panic!("Unexpected PeerConnectionError {:?}", err),
                },
                _ => panic!("Unexpected ConnectionError {:?}", error),
            }
        } else {
            panic!("Unexpected success when building invalid PeerConnectionContext");
        }
    }

    #[test]
    fn valid_build() {
        let ctx = Context::new();

        let recv_addr = InprocAddress::random();
        let peer_addr = "127.0.0.1:80".parse::<NetAddress>().unwrap();
        let conn_id = "123".as_bytes();
        let socks_addr = "127.0.0.1:8080".parse::<SocketAddress>().unwrap();

        let peer_ctx = PeerConnectionContextBuilder::new()
            .set_id(conn_id.clone())
            .set_direction(Direction::Inbound)
            .set_context(&ctx)
            .set_socks_proxy(socks_addr.clone())
            .set_consumer_address(recv_addr.clone())
            .set_address(peer_addr.clone())
            .build()
            .unwrap();

        assert_eq!(conn_id.to_vec(), peer_ctx.id);
        assert_eq!(recv_addr, peer_ctx.consumer_address);
        assert_eq!(Direction::Inbound, peer_ctx.direction);
        assert_eq!(peer_addr, peer_ctx.peer_address);
        assert_eq!(Some(socks_addr), peer_ctx.socks_address);
    }

    #[test]
    fn invalid_build() {
        let (sk, pk) = curve_keypair::generate().unwrap();
        let ctx = Context::new();

        let result = PeerConnectionContextBuilder::new()
            .set_id("123")
            .set_direction(Direction::Outbound)
            .set_consumer_address(InprocAddress::random())
            .set_address("127.0.0.1:80".parse().unwrap())
            .build();

        assert_initialization_error(result, "Missing required connection property 'context'");

        let result = PeerConnectionContextBuilder::new()
            .set_id("123")
            .set_direction(Direction::Inbound)
            .set_context(&ctx)
            .set_consumer_address(InprocAddress::random())
            .set_curve_encryption(CurveEncryption::Client {
                secret_key: sk.clone(),
                public_key: pk.clone(),
                server_public_key: pk.clone(),
            })
            .set_address("127.0.0.1:80".parse().unwrap())
            .build();

        assert_initialization_error(result, "'Server' curve encryption required for inbound connection");

        let result = PeerConnectionContextBuilder::new()
            .set_id("123")
            .set_direction(Direction::Outbound)
            .set_context(&ctx)
            .set_consumer_address(InprocAddress::random())
            .set_curve_encryption(CurveEncryption::Server { secret_key: sk.clone() })
            .set_address("127.0.0.1:80".parse().unwrap())
            .build();

        assert_initialization_error(result, "'Client' curve encryption required for outbound connection");
    }
}

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

use super::PeerConnectionError;
use crate::{
    connection::{
        types::{ConnectionDirection, Linger},
        zmq::{CurveEncryption, ZmqContext, ZmqIdentity},
    },
    message::FrameSet,
};
use futures::channel::mpsc::Sender;
use multiaddr::Multiaddr;
use std::{
    convert::{TryFrom, TryInto},
    net::SocketAddr,
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
/// `message_sink_channel` - the channel to forward all received messages to
/// `direction` - the connection direction (Inbound or Outbound)
/// `curve_encryption` - the [CurveEncryption] for the connection
/// `max_msg_size` - the maximum size of a incoming message
/// `socks_address` - optional address for a SOCKS proxy
///
/// [CurveEncryption]: ./../zmq/CurveEncryption/struct.CurveEncryption.html
pub struct PeerConnectionContext {
    pub(crate) context: ZmqContext,
    pub(crate) connection_identity: Option<ZmqIdentity>,
    pub(crate) peer_identity: Option<ZmqIdentity>,
    pub(crate) peer_address: Multiaddr,
    pub(crate) message_sink_channel: Sender<FrameSet>,
    pub(crate) direction: ConnectionDirection,
    pub(crate) curve_encryption: CurveEncryption,
    pub(crate) max_msg_size: u64,
    pub(crate) max_retry_attempts: u16,
    pub(crate) socks_address: Option<SocketAddr>,
    pub(crate) linger: Linger,
    pub(crate) shutdown_on_send_failure: bool,
}

impl PeerConnectionContext {
    pub fn direction(&self) -> ConnectionDirection {
        self.direction
    }
}

impl<'a> TryFrom<PeerConnectionContextBuilder<'a>> for PeerConnectionContext {
    type Error = PeerConnectionError;

    /// Convert a PeerConnectionContextBuilder to a PeerConnectionContext
    fn try_from(builder: PeerConnectionContextBuilder<'a>) -> Result<Self, PeerConnectionError> {
        builder.check_curve_encryption()?;

        let message_sink_channel = unwrap_prop(builder.message_sink_channel, "message_sink_channel")?;
        let context = unwrap_prop(builder.context, "context")?.clone();
        let curve_encryption = builder.curve_encryption;
        let direction = unwrap_prop(builder.direction, "direction")?;
        let (connection_identity, peer_identity) = match direction {
            ConnectionDirection::Inbound => (None, None),
            ConnectionDirection::Outbound => (
                Some(
                    builder
                        .connection_identity
                        .ok_or(PeerConnectionError::ConnectionIdentityNotSet)?,
                ),
                Some(builder.peer_identity.ok_or(PeerConnectionError::PeerIdentityNotSet)?),
            ),
        };
        let shutdown_on_send_failure = builder.shutdown_on_send_failure;
        let max_msg_size = builder.max_msg_size.unwrap_or(DEFAULT_MAX_MSG_SIZE);
        let max_retry_attempts = builder.max_retry_attempts.unwrap_or(DEFAULT_MAX_RETRY_ATTEMPTS);
        let peer_address = unwrap_prop(builder.address, "peer_address")?;
        let socks_address = builder.socks_address;
        let linger = builder.linger.or(Some(Linger::Timeout(100))).unwrap();

        Ok(PeerConnectionContext {
            peer_identity,
            connection_identity,
            message_sink_channel,
            context,
            curve_encryption,
            direction,
            max_msg_size,
            max_retry_attempts,
            peer_address,
            socks_address,
            linger,
            shutdown_on_send_failure,
        })
    }
}

/// Local utility function to unwrap a builder property, or return a PeerConnectionError::InitializationError
#[inline(always)]
fn unwrap_prop<T>(prop: Option<T>, prop_name: &str) -> Result<T, PeerConnectionError> {
    match prop {
        Some(t) => Ok(t),
        None => Err(PeerConnectionError::InitializationError(format!(
            "Missing required connection property '{}'",
            prop_name
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
/// #     ZmqContext,
/// #     InprocAddress,
/// #     ConnectionDirection,
/// #     PeerConnectionContextBuilder,
/// #     PeerConnection,
/// # };
/// # use futures::channel::mpsc::channel;
///
/// let ctx = ZmqContext::new();
/// let (tx, _rx) =  channel(100);
///
/// let peer_context = PeerConnectionContextBuilder::new()
///     // This is sent as the first frame in the consumer channel on outbound connections
///    .set_peer_identity(b"some-peer-identiifier".to_vec())
///     // This is how we identify to the remote ZMQ_ROUTER socket
///    .set_connection_identity(b"123".to_vec())
///    .set_context(&ctx)
///    .set_direction(ConnectionDirection::Outbound)
///    .set_message_sink_channel(tx)
///    .set_address("/ip4/127.0.0.1/tcp/8080".parse().unwrap())
///    .finish()
///    .unwrap();
/// ```
///
/// [PeerConnectionContext]: ./struct.PeerConnectionContext.html
#[derive(Default)]
pub struct PeerConnectionContextBuilder<'c> {
    pub(super) address: Option<Multiaddr>,
    pub(super) message_sink_channel: Option<Sender<FrameSet>>,
    pub(super) context: Option<&'c ZmqContext>,
    pub(super) curve_encryption: CurveEncryption,
    pub(super) direction: Option<ConnectionDirection>,
    pub(super) peer_identity: Option<ZmqIdentity>,
    pub(super) connection_identity: Option<ZmqIdentity>,
    pub(super) max_msg_size: Option<u64>,
    pub(super) max_retry_attempts: Option<u16>,
    pub(super) socks_address: Option<SocketAddr>,
    pub(super) linger: Option<Linger>,
    pub(crate) shutdown_on_send_failure: bool,
}

impl<'c> PeerConnectionContextBuilder<'c> {
    /// Set the peer address
    setter!(set_address, address, Option<Multiaddr>);

    /// Set the channel where incoming peer messages are forwarded
    setter!(set_message_sink_channel, message_sink_channel, Option<Sender<FrameSet>>);

    /// Set the zmq context
    setter!(set_context, context, Option<&'c ZmqContext>);

    /// Set the connection direction
    setter!(set_direction, direction, Option<ConnectionDirection>);

    /// Set the maximum connection retry attempts
    setter!(set_max_retry_attempts, max_retry_attempts, Option<u16>);

    /// Set the maximum message size in bytes
    setter!(set_max_msg_size, max_msg_size, Option<u64>);

    /// Set the socks proxy address
    setter!(set_socks_proxy, socks_address, Option<SocketAddr>);

    /// Set the [Linger] for this connection
    setter!(set_linger, linger, Option<Linger>);

    /// Set the remote identity to use for this connection (the zmq identity frame)
    ///
    /// On an outbound connection, this sets the first frame to be sent on the connection to the peer.
    /// This is given by the peer's control service when requesting to connect.
    setter!(set_connection_identity, connection_identity, Option<ZmqIdentity>);

    /// Set the identity of the peer which is used to identify the peer (i.e. NodeId).
    ///
    /// On an outbound connection, this sets the first frame to be sent on the mpsc channel
    /// to identify the peer.
    setter!(set_peer_identity, peer_identity, Option<Vec<u8>>);

    /// Shutdown peer connection if failure occurs when sending a message
    setter!(set_shutdown_on_send_failure, shutdown_on_send_failure, bool);

    /// Return a new PeerConnectionContextBuilder
    pub fn new() -> Self {
        Default::default()
    }

    /// Set CurveEncryption. Defaults to the default of CurveEncryption.
    pub fn set_curve_encryption(mut self, enc: CurveEncryption) -> Self {
        self.curve_encryption = enc;
        self
    }

    /// Build the PeerConnectionContext.
    ///
    /// Will return an Err if any of the required fields are not set or if
    /// curve encryption is not set correctly for the connection direction.
    /// i.e CurveEncryption::Server must be set with Direction::Inbound and
    ///  CurveEncryption::Client must be set with Direction::Outbound.
    ///  CurveEncryption::None will succeed in either direction.
    pub fn finish(self) -> Result<PeerConnectionContext, PeerConnectionError> {
        self.try_into()
    }

    fn check_curve_encryption(&self) -> Result<(), PeerConnectionError> {
        match self.direction {
            Some(ref direction) => match direction {
                ConnectionDirection::Outbound => match self.curve_encryption {
                    CurveEncryption::None { .. } => Ok(()),
                    CurveEncryption::Client { .. } => Ok(()),
                    CurveEncryption::Server { .. } => Err(PeerConnectionError::InitializationError(
                        "'Client' curve encryption required for outbound connection".to_string(),
                    )
                    .into()),
                },
                ConnectionDirection::Inbound => match self.curve_encryption {
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
        types::ConnectionDirection,
        zmq::{CurveEncryption, ZmqContext},
    };
    use futures::channel::mpsc::channel;

    fn assert_initialization_error<T>(result: Result<T, PeerConnectionError>, expected: &str) {
        if let Err(err) = result {
            match err {
                PeerConnectionError::InitializationError(s) => {
                    assert_eq!(expected, s);
                },
                _ => panic!("Unexpected PeerConnectionError {:?}", err),
            }
        } else {
            panic!("Unexpected success when building invalid PeerConnectionContext");
        }
    }

    #[test]
    fn valid_build() {
        let ctx = ZmqContext::new();

        let peer_addr = "/ip4/127.0.0.1/tcp/80".parse::<Multiaddr>().unwrap();
        let socks_addr = "127.0.0.1:8080".parse::<SocketAddr>().unwrap();

        let (tx, _rx) = channel(10);

        let peer_ctx = PeerConnectionContextBuilder::new()
            .set_direction(ConnectionDirection::Inbound)
            .set_context(&ctx)
            .set_socks_proxy(socks_addr.clone())
            .set_message_sink_channel(tx)
            .set_address(peer_addr.clone())
            .finish()
            .unwrap();

        assert_eq!(ConnectionDirection::Inbound, peer_ctx.direction);
        assert_eq!(peer_addr, peer_ctx.peer_address);
        assert_eq!(Some(socks_addr), peer_ctx.socks_address);
    }

    #[test]
    fn invalid_build() {
        let (sk, pk) = CurveEncryption::generate_keypair().unwrap();
        let ctx = ZmqContext::new();
        let (tx, _rx) = channel(10);
        let result = PeerConnectionContextBuilder::new()
            .set_peer_identity(b"123".to_vec())
            .set_connection_identity(b"123".to_vec())
            .set_direction(ConnectionDirection::Outbound)
            .set_message_sink_channel(tx)
            .set_address("/ip4/127.0.0.1/tcp/80".parse().unwrap())
            .finish();

        assert_initialization_error(result, "Missing required connection property 'context'");

        let (tx, _rx) = channel(10);

        let result = PeerConnectionContextBuilder::new()
            .set_direction(ConnectionDirection::Inbound)
            .set_context(&ctx)
            .set_message_sink_channel(tx)
            .set_curve_encryption(CurveEncryption::Client {
                secret_key: sk.clone(),
                public_key: pk.clone(),
                server_public_key: pk.clone(),
            })
            .set_address("/ip4/127.0.0.1/tcp/80".parse().unwrap())
            .finish();

        assert_initialization_error(result, "'Server' curve encryption required for inbound connection");
        let (tx, _rx) = channel(10);
        let result = PeerConnectionContextBuilder::new()
            .set_peer_identity(b"123".to_vec())
            .set_connection_identity(b"123".to_vec())
            .set_direction(ConnectionDirection::Outbound)
            .set_context(&ctx)
            .set_message_sink_channel(tx)
            .set_curve_encryption(CurveEncryption::Server { secret_key: sk.clone() })
            .set_address("/ip4/127.0.0.1/tcp/80".parse().unwrap())
            .finish();

        assert_initialization_error(result, "'Client' curve encryption required for outbound connection");
    }
}

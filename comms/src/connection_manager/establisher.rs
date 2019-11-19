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

use super::{error::ConnectionManagerError, Result};
use crate::{
    connection::{
        curve_keypair::{CurvePublicKey, CurveSecretKey},
        net_address::ip::SocketAddress,
        peer_connection::PeerConnectionJoinHandle,
        types::{Direction, Linger},
        zmq::ZmqIdentity,
        Connection,
        CurveEncryption,
        EstablishedConnection,
        NetAddress,
        PeerConnection,
        PeerConnectionContextBuilder,
        ZmqContext,
    },
    control_service::ControlServiceClient,
    message::FrameSet,
    peer_manager::{NodeIdentity, Peer, PeerManager},
    types::DEFAULT_LISTENER_ADDRESS,
};
use futures::channel::mpsc::Sender;
use log::*;
use std::{sync::Arc, time::Duration};

const LOG_TARGET: &str = "comms::connection_manager::establisher";

/// Configuration for peer connections which are produced by the ConnectionEstablisher
/// These are the common properties which are shared across all peer connections
#[derive(Clone)]
pub struct PeerConnectionConfig {
    /// Maximum number of peer connections. Newer connections will be rejected until there are
    /// less than `max_connections` active connections.
    pub max_connections: usize,
    /// Maximum size of inbound messages - messages larger than this will be dropped
    pub max_message_size: u64,
    /// The number of connection attempts to make to one address before giving up
    pub max_connect_retries: u16,
    /// The address of the SOCKS proxy to use for this connection
    pub socks_proxy_address: Option<SocketAddress>,
    /// The address to bind to for the listening connection (Default: DEFAULT_LISTENER_ADDRESS)
    pub listening_address: NetAddress,
    /// The length of time to wait for the requested peer connection to be established before timing out.
    /// Depending on the network, this should be long enough to allow a single back-and-forth
    /// communication between peers.
    pub peer_connection_establish_timeout: Duration,
}

impl Default for PeerConnectionConfig {
    fn default() -> Self {
        Self {
            max_connections: 100,
            max_message_size: 1024 * 1024,
            max_connect_retries: 5,
            socks_proxy_address: None,
            peer_connection_establish_timeout: Duration::from_secs(20),
            listening_address: DEFAULT_LISTENER_ADDRESS.parse().unwrap(),
        }
    }
}

/// ## ConnectionEstablisher
///
/// This component is responsible for creating encrypted connections to peers and updating
/// the peer stats for failed/successful connection attempts. This component does not hold any
/// connections, but returns them so that the caller may use them as needed. This component does
/// not complete the peer connection protocol, it simply creates connections with some reliability.
/// # Arguments
/// `context`: The ZMQ context for created connections
/// `config`: Parameters used when intantiating new PeerConnections
/// `node_identity`: This node's identity
/// `peer_manager`: This node's Peer Manager
/// `message_sink_channel`: The Sender side of the MPSC channel to which Peer Connections will send received messages
pub struct ConnectionEstablisher {
    context: ZmqContext,
    config: PeerConnectionConfig,
    node_identity: Arc<NodeIdentity>,
    peer_manager: Arc<PeerManager>,
    message_sink_channel: Sender<FrameSet>,
}

impl ConnectionEstablisher {
    /// Create a new ConnectionEstablisher.
    pub fn new(
        context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        config: PeerConnectionConfig,
        peer_manager: Arc<PeerManager>,
        message_sink_channel: Sender<FrameSet>,
    ) -> Self
    {
        Self {
            context,
            node_identity,
            config,
            peer_manager,
            message_sink_channel,
        }
    }

    /// Returns the peer connection config
    pub fn get_config(&self) -> &PeerConnectionConfig {
        &self.config
    }

    /// Attempt to establish a control service connection to one of the peer's addresses
    ///
    /// ### Arguments
    /// - `peer`: `&Peer` - The peer to connect to
    pub fn connect_control_service_client(&self, peer: &Peer) -> Result<ControlServiceClient> {
        let config = &self.config;

        self.peer_manager
            .reset_connection_attempts(&peer.node_id)
            .map_err(ConnectionManagerError::PeerManagerError)?;

        info!(
            target: LOG_TARGET,
            "Starting {} attempt(s) to connect to control port for NodeId={}",
            peer.addresses.len(),
            peer.node_id
        );

        let maybe_client = self.attempt_control_port_connection_for_peer(&peer, || {
            let address = self
                .peer_manager
                .get_best_net_address(&peer.node_id)
                .map_err(ConnectionManagerError::PeerManagerError)?;
            debug!(target: LOG_TARGET, "Attempting to connect to {}", address);

            let conn = Connection::new(&self.context, Direction::Outbound)
                .set_name(format!("outbound-control-port-conn-{}", peer.node_id).as_str())
                .set_linger(Linger::Never)
                .set_backlog(1)
                .set_send_hwm(5)
                .set_socks_proxy_addr(config.socks_proxy_address.clone())
                .set_max_message_size(Some(config.max_message_size))
                .establish(&address)
                .map_err(ConnectionManagerError::ConnectionError)?;

            Ok((conn, address))
        })?;

        // Reset the connection attempts for this peer
        // TODO(sdbondi): This is a good reason why peer manager shouldn't be managing connection
        //                attempts. Peer manager should be simplified a little to return an iterator
        //                sorted from 'best' to 'worst' net address without having to modify shared
        //                state.
        self.peer_manager
            .reset_connection_attempts(&peer.node_id)
            .map_err(ConnectionManagerError::PeerManagerError)?;

        match maybe_client {
            Some(client) => Ok(client),
            None => Err(ConnectionManagerError::ControlServiceFailedConnectionAllAddresses),
        }
    }

    fn attempt_control_port_connection_for_peer(
        &self,
        peer: &Peer,
        connection_factory: impl Fn() -> Result<(EstablishedConnection, NetAddress)>,
    ) -> Result<Option<ControlServiceClient>>
    {
        let num_attempts = peer.addresses.len();
        let mut current_attempts = 1;
        loop {
            let (conn, address) = connection_factory()?;
            let client = ControlServiceClient::new(Arc::clone(&self.node_identity), peer.public_key.clone(), conn);

            debug!(
                target: LOG_TARGET,
                "Sending ping to remote control port (NodeId={})", peer.node_id
            );
            match client.ping_pong(self.config.peer_connection_establish_timeout) {
                Ok(Some(_)) => {
                    self.peer_manager
                        .mark_successful_connection_attempt(&address)
                        .map_err(ConnectionManagerError::PeerManagerError)?;

                    debug!(
                        target: LOG_TARGET,
                        "Received PONG reply. Connection succeeded (NodeId={})", peer.node_id
                    );

                    break Ok(Some(client));
                },
                Ok(None) => {
                    debug!(
                        target: LOG_TARGET,
                        "Did not receive PONG within timeout from peer {}", peer.node_id
                    );
                },
                Err(err) => {
                    debug!(
                        target: LOG_TARGET,
                        "Control service ping pong check failed because '{:?}'", err
                    );
                },
            }

            self.peer_manager
                .mark_failed_connection_attempt(&address)
                .map_err(ConnectionManagerError::PeerManagerError)?;

            if current_attempts >= num_attempts {
                break Ok(None);
            }

            current_attempts += 1;
        }
    }

    /// Create a new outbound PeerConnection
    ///
    /// ### Arguments
    /// `conn_id`: [ConnectionId] - The id to use for the connection
    /// `address`: [NetAddress] - The [NetAddress] to connect to
    /// `curve_public_key`: [&NetAddress] - The Curve25519 public key of the destination connection
    ///
    /// Returns an Arc<[PeerConnection]> in `Connected` state and the [std::thread::JoinHandle] of the
    /// [PeerConnection] worker thread or an error.
    pub fn establish_outbound_peer_connection(
        &self,
        address: NetAddress,
        curve_public_key: CurvePublicKey,
        connection_identity: ZmqIdentity,
        peer_identity: ZmqIdentity,
    ) -> Result<(Arc<PeerConnection>, PeerConnectionJoinHandle)>
    {
        debug!(target: LOG_TARGET, "Establishing outbound connection to {}", address);
        let (secret_key, public_key) = CurveEncryption::generate_keypair()?;

        let context = self
            .new_context_builder()
            .set_direction(Direction::Outbound)
            .set_connection_identity(connection_identity)
            .set_peer_identity(peer_identity)
            .set_address(address)
            .set_shutdown_on_send_failure(true)
            .set_curve_encryption(CurveEncryption::Client {
                secret_key,
                public_key,
                server_public_key: curve_public_key,
            })
            .set_message_sink_channel(self.message_sink_channel.clone())
            .finish()?;

        let (connection, worker_handle) = PeerConnection::connect(context)?;
        connection
            .wait_connected_or_failure(self.config.peer_connection_establish_timeout)
            .or_else(|err| {
                debug!(target: LOG_TARGET, "Outbound connection failed: {:?}", err);
                Err(err)
            })?;

        let connection = Arc::new(connection);

        Ok((connection, worker_handle))
    }

    /// Establish a new inbound peer connection listening on `PeerConnectionConfig::listening_address`.
    ///
    /// ### Arguments
    /// `curve_secret_key`: [&CurveSecretKey] - The zmq Curve25519 secret key for the connection
    ///
    /// Returns an Arc<[PeerConnection]> in `Listening` state and the [std::thread::JoinHandle] of the
    /// [PeerConnection] worker thread or an error.
    pub fn establish_peer_listening_connection(
        &self,
        curve_secret_key: CurveSecretKey,
    ) -> Result<(Arc<PeerConnection>, PeerConnectionJoinHandle)>
    {
        let address = self.config.listening_address.clone();
        debug!(target: LOG_TARGET, "Establishing inbound connection to {}", address);

        let context = self
            .new_context_builder()
            .set_direction(Direction::Inbound)
            .set_address(address)
            .set_shutdown_on_send_failure(false)
            .set_curve_encryption(CurveEncryption::Server {
                secret_key: curve_secret_key,
            })
            .set_message_sink_channel(self.message_sink_channel.clone())
            .finish()?;

        let (connection, worker_handle) = PeerConnection::listen(context)?;
        connection
            .wait_listening_or_failure(self.config.peer_connection_establish_timeout)
            .or_else(|err| {
                debug!(target: LOG_TARGET, "Unable to establish inbound connection: {:?}", err);
                Err(err)
            })?;

        debug!(
            target: LOG_TARGET,
            "Inbound connection established on (NetAddress={:?}, SocketAddress={:?})",
            connection.get_address(),
            connection.get_connected_address()
        );

        let connection = Arc::new(connection);

        Ok((connection, worker_handle))
    }

    fn new_context_builder(&self) -> PeerConnectionContextBuilder {
        let config = &self.config;

        let mut builder = PeerConnectionContextBuilder::new()
            .set_context(&self.context)
            .set_max_msg_size(config.max_message_size)
            .set_max_retry_attempts(config.max_connect_retries);

        if let Some(ref addr) = config.socks_proxy_address {
            builder = builder.set_socks_proxy(addr.clone());
        }

        builder
    }
}

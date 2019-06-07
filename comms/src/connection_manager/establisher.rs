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

use log::*;

use super::{error::ConnectionManagerError, repository::PeerConnectionEntry, types::PeerConnectionJoinHandle, Result};

use crate::{
    connection::{
        curve_keypair::{CurvePublicKey, CurveSecretKey},
        net_address::ip::SocketAddress,
        types::Direction,
        Connection,
        Context,
        CurveEncryption,
        InprocAddress,
        NetAddress,
        PeerConnection,
        PeerConnectionContextBuilder,
    },
    peer_manager::{Peer, PeerManager},
    types::{CommsDataStore, CommsPublicKey},
};

use std::{net::IpAddr, sync::Arc, time::Duration};

const LOG_TARGET: &'static str = "comms::connection_manager::establisher";

/// Configuration for peer connections which are produced by the ConnectionEstablisher
/// These are the common properties which are shared across all peer connections
pub struct PeerConnectionConfig {
    /// The peer connection context
    pub context: Context,
    /// Maximum size of inbound messages - messages larger than this will be dropped
    pub max_message_size: u64,
    /// The number of connection attempts to make to one address before giving up
    pub max_connect_retries: u16,
    /// The address of the SOCKS proxy to use for this connection
    pub socks_proxy_address: Option<SocketAddress>,
    /// The address to forward all the messages received from this peer connection
    pub consumer_address: InprocAddress,
    /// The host to bind to when creating inbound connections
    pub host: IpAddr,
    /// The length of time to wait for the connection to be established to a peer's control services.
    pub control_service_establish_timeout: Duration,
    /// The length of time to wait for the requested peer connection to be established before timing out.
    /// This should be more than twice as long as control_service_establish_timeout, as communication
    /// this must be long enough for a single back-and-forth between the peers.
    pub peer_connection_establish_timeout: Duration,
}

/// ## ConnectionEstablisher
///
/// This component is responsible for creating encrypted connections to peers and updating
/// the peer stats for failed/successful connection attempts. This component does not hold any
/// connections, but returns them so that the caller may use them as needed. This component does
/// not complete the peer connection protocol, it simply creates connections with some reliability.
pub(crate) struct ConnectionEstablisher {
    config: PeerConnectionConfig,
    peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
}

impl ConnectionEstablisher {
    /// Create a new ConnectionEstablisher.
    pub fn new(config: PeerConnectionConfig, peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>) -> Self {
        Self { config, peer_manager }
    }

    /// Returns the peer connection config
    pub fn get_config(&self) -> &PeerConnectionConfig {
        &self.config
    }

    /// Attempt to establish a control service connection to one of the peer's addresses
    pub fn establish_control_service_connection(&self, peer: &Peer<CommsPublicKey>) -> Result<EstablishedConnection> {
        let config = &self.config;

        let mut attempt = ConnectionAttempts::new(
            &config.context,
            self.peer_manager.clone(),
            |attempt_count, monitor_addr| {
                let address = self
                    .peer_manager
                    .get_best_net_address(&peer.node_id)
                    .map_err(ConnectionManagerError::PeerManagerError)?;

                let conn = Connection::new(&config.context, Direction::Outbound)
                    .set_monitor_addr(monitor_addr)
                    .set_socks_proxy_addr(config.socks_proxy_address.clone())
                    .set_max_message_size(Some(config.max_message_size))
                    .set_receive_hwm(0)
                    .establish(&address)
                    .map_err(ConnectionManagerError::ConnectionError)?;

                debug!(
                    target: LOG_TARGET,
                    "Connection attempt #{} to NodeId={}", attempt_count, peer.node_id
                );

                Ok((conn, address))
            },
        );

        info!(
            target: LOG_TARGET,
            "Attempting to connect to control port for NodeId={}", peer.node_id
        );
        attempt.try_connect(peer.addresses.len())
    }

    /// Create a new outbound PeerConnection for a peer.
    pub fn establish_outbound_peer_connection(
        &self,
        peer: &Peer<CommsPublicKey>,
        address: &NetAddress,
        server_public_key: CurvePublicKey,
    ) -> Result<(PeerConnectionEntry, PeerConnectionJoinHandle)>
    {
        let (secret_key, public_key) = CurveEncryption::generate_keypair()?;

        let context = self
            .new_context_builder()
            .set_id(peer.node_id.clone())
            .set_direction(Direction::Outbound)
            .set_address(address.clone())
            .set_curve_encryption(CurveEncryption::Client {
                secret_key,
                public_key,
                server_public_key,
            })
            .build()?;

        let mut connection = PeerConnection::new();
        let worker_handle = connection.start(context)?;
        connection
            .wait_connected_or_failure(&self.config.control_service_establish_timeout)
            .or_else(|err| {
                error!(
                    target: LOG_TARGET,
                    "Outbound connection to NodeId={} failed: {:?}", peer.node_id, err
                );
                Err(ConnectionManagerError::ConnectionError(err))
            })?;

        let connection = Arc::new(connection);

        Ok((
            PeerConnectionEntry {
                connection,
                address: address.clone(),
            },
            worker_handle,
        ))
    }

    /// Establish a new connection for a peer. A connection may be Inbound
    pub fn establish_inbound_peer_connection(
        &self,
        peer: &Peer<CommsPublicKey>,
        secret_key: CurveSecretKey,
    ) -> Result<(PeerConnectionEntry, PeerConnectionJoinHandle)>
    {
        // Providing port 0 tells the OS to allocate a port for us
        let mut address = NetAddress::IP((self.config.host, 0).into());
        let context = self
            .new_context_builder()
            .set_id(peer.node_id.clone())
            .set_direction(Direction::Inbound)
            .set_address(address.clone())
            .set_curve_encryption(CurveEncryption::Server { secret_key })
            .build()?;

        let mut connection = PeerConnection::new();
        let worker_handle = connection.start(context)?;
        connection
            .wait_listening_or_failure(&self.config.control_service_establish_timeout)
            .or_else(|err| {
                error!(
                    target: LOG_TARGET,
                    "Unable to establish inbound connection for NodeId={}: {:?}", peer.node_id, err
                );
                Err(ConnectionManagerError::ConnectionError(err))
            })?;

        let connection = Arc::new(connection);
        if let Some(addr) = connection.get_connected_address().map(|a| a.into()) {
            address = addr;
        }

        Ok((PeerConnectionEntry { connection, address }, worker_handle))
    }

    fn new_context_builder(&self) -> PeerConnectionContextBuilder {
        let config = &self.config;

        let mut builder = PeerConnectionContextBuilder::new()
            .set_context(&config.context)
            .set_max_msg_size(config.max_message_size)
            .set_consumer_address(config.consumer_address.clone())
            .set_max_retry_attempts(config.max_connect_retries);

        if let Some(ref addr) = config.socks_proxy_address {
            builder = builder.set_socks_proxy(addr.clone());
        }

        builder
    }
}

//---------------------------------- Connection Attempts --------------------------------------------//

use crate::connection::{
    connection::EstablishedConnection,
    monitor::{ConnectionMonitor, SocketEventType},
};

/// Helper struct which enables multiple attempts at connecting. This also updates the peers connection
/// attempts statistics
struct ConnectionAttempts<'c, F> {
    context: &'c Context,
    num_attempts: usize,
    attempt_fn: F,
    peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
}

impl<'c, F> ConnectionAttempts<'c, F>
where F: Fn(usize, InprocAddress) -> Result<(EstablishedConnection, NetAddress)>
{
    pub fn new(
        context: &'c Context,
        peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
        attempt_fn: F,
    ) -> Self
    {
        Self {
            context,
            num_attempts: 0,
            attempt_fn,
            peer_manager,
        }
    }

    pub fn try_connect(&mut self, num_attempts: usize) -> Result<EstablishedConnection> {
        let mut attempt_count = 0usize;
        loop {
            let monitor_addr = InprocAddress::random();
            let monitor = ConnectionMonitor::connect(self.context, &monitor_addr)
                .map_err(ConnectionManagerError::ConnectionError)?;

            attempt_count += 1;
            let (conn, address) = (self.attempt_fn)(attempt_count, monitor_addr)?;

            if self.is_connected(monitor)? {
                debug!(
                    target: LOG_TARGET,
                    "Successful connection on control port: {:?}",
                    conn.get_connected_address()
                );
                self.peer_manager
                    .mark_successful_connection_attempt(&address)
                    .map_err(ConnectionManagerError::PeerManagerError)?;
                break Ok(conn);
            } else {
                self.peer_manager
                    .mark_failed_connection_attempt(&address)
                    .map_err(ConnectionManagerError::PeerManagerError)?;
                self.num_attempts += 1;
                if self.num_attempts > num_attempts {
                    break Err(ConnectionManagerError::MaxConnnectionAttemptsExceeded);
                }
            }
        }
    }

    fn is_connected(&self, monitor: ConnectionMonitor) -> Result<bool> {
        loop {
            if let Some(event) = connection_try!(monitor.read(100)) {
                use SocketEventType::*;
                debug!(target: LOG_TARGET, "Socket Event: {:?}", event);
                match event.event_type {
                    Connected | Listening => break Ok(true),
                    Disconnected |
                    Closed |
                    BindFailed |
                    HandshakeFailedAuth |
                    HandshakeFailedNoDetail |
                    HandshakeFailedProtocol |
                    MonitorStopped => break Ok(false),
                    _ => {},
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_support::{
        factories::{self, Factory},
        helpers::ConnectionMessageCounter,
    };

    fn make_peer_connection_config(context: &Context, consumer_address: InprocAddress) -> PeerConnectionConfig {
        PeerConnectionConfig {
            context: context.clone(),
            control_service_establish_timeout: Duration::from_millis(2000),
            peer_connection_establish_timeout: Duration::from_secs(20),
            max_message_size: 1024,
            host: "127.0.0.1".parse().unwrap(),
            max_connect_retries: 3,
            consumer_address,
            socks_proxy_address: None,
        }
    }

    #[test]
    fn establish_control_service_connection_fail() {
        let context = Context::new();
        let peers = factories::peer::create_many(2).build().unwrap();
        let peer_manager = Arc::new(
            factories::peer_manager::create()
                .with_peers(peers.clone())
                .build()
                .unwrap(),
        );
        let config = make_peer_connection_config(&context, InprocAddress::random());

        let example_peer = &peers[0];

        let establisher = ConnectionEstablisher::new(config, peer_manager);
        let result = establisher.establish_control_service_connection(example_peer);

        match result {
            Ok(_) => panic!("Unexpected success result"),
            Err(ConnectionManagerError::MaxConnnectionAttemptsExceeded) => {},
            Err(err) => panic!("Unexpected error type: {:?}", err),
        }
    }

    #[test]
    fn establish_control_service_connection_succeed() {
        let context = Context::new();
        let address: NetAddress = "127.0.0.1:0".parse().unwrap();

        // Setup a connection to act as an endpoint for a peers control service
        let dummy_conn = Connection::new(&context, Direction::Inbound)
            .establish(&address)
            .unwrap();

        let address: NetAddress = dummy_conn.get_connected_address().clone().unwrap().into();

        let example_peer = factories::peer::create()
            .with_net_addresses(vec![address])
            .build()
            .unwrap();

        let peer_manager = Arc::new(
            factories::peer_manager::create()
                .with_peers(vec![example_peer.clone()])
                .build()
                .unwrap(),
        );

        let config = make_peer_connection_config(&context, InprocAddress::random());
        let establisher = ConnectionEstablisher::new(config, peer_manager);
        establisher.establish_control_service_connection(&example_peer).unwrap();
    }

    #[test]
    fn establish_peer_connection_outbound() {
        let context = Context::new();
        let consumer_address = InprocAddress::random();

        // Setup a message counter to count the number of messages sent to the consumer address
        let msg_counter = ConnectionMessageCounter::new(&context);
        msg_counter.start(consumer_address.clone());

        // Setup a peer connection
        let (other_peer_conn, _, peer_curve_pk) = factories::peer_connection::create()
            .with_peer_connection_context_factory(
                factories::peer_connection_context::create()
                    .with_consumer_address(consumer_address.clone())
                    .with_context(&context)
                    .with_direction(Direction::Inbound),
            )
            .build()
            .unwrap();

        other_peer_conn
            .wait_listening_or_failure(&Duration::from_millis(200))
            .unwrap();

        let address: NetAddress = other_peer_conn.get_connected_address().unwrap().into();

        let example_peer = factories::peer::create()
            .with_net_addresses(vec![address.clone()])
            .build()
            .unwrap();

        let peer_manager = Arc::new(
            factories::peer_manager::create()
                .with_peers(vec![example_peer.clone()])
                .build()
                .unwrap(),
        );

        let config = make_peer_connection_config(&context, InprocAddress::random());
        let establisher = ConnectionEstablisher::new(config, peer_manager);
        let (entry, peer_conn_handle) = establisher
            .establish_outbound_peer_connection(&example_peer, &address, peer_curve_pk)
            .unwrap();

        entry.connection.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
        entry.connection.send(vec!["TARI".as_bytes().to_vec()]).unwrap();

        entry.connection.shutdown().unwrap();
        entry
            .connection
            .wait_disconnected(&Duration::from_millis(1000))
            .unwrap();

        assert_eq!(msg_counter.count(), 2);

        peer_conn_handle.join().unwrap().unwrap();
    }

    #[test]
    fn establish_peer_connection_inbound() {
        let context = Context::new();
        let consumer_address = InprocAddress::random();

        let (secret_key, public_key) = CurveEncryption::generate_keypair().unwrap();

        let example_peer = factories::peer::create().build().unwrap();

        let peer_manager = Arc::new(
            factories::peer_manager::create()
                .with_peers(vec![example_peer.clone()])
                .build()
                .unwrap(),
        );

        // Setup a message counter to count the number of messages sent to the consumer address
        let msg_counter = ConnectionMessageCounter::new(&context);
        msg_counter.start(consumer_address.clone());

        // Create a connection establisher
        let config = make_peer_connection_config(&context, consumer_address.clone());
        let establisher = ConnectionEstablisher::new(config, peer_manager);
        let (entry, peer_conn_handle) = establisher
            .establish_inbound_peer_connection(&example_peer, secret_key)
            .unwrap();

        entry
            .connection
            .wait_listening_or_failure(&Duration::from_millis(2000))
            .unwrap();
        let address: NetAddress = entry.connection.get_connected_address().unwrap().into();

        // Setup a peer connection which will connect to our established inbound peer connection
        let (other_peer_conn, _, _) = factories::peer_connection::create()
            .with_peer_connection_context_factory(
                factories::peer_connection_context::create()
                    .with_context(&context)
                    .with_address(address)
                    .with_server_public_key(public_key.clone())
                    .with_direction(Direction::Outbound),
            )
            .build()
            .unwrap();

        other_peer_conn
            .wait_connected_or_failure(&Duration::from_millis(2000))
            .unwrap();
        // Start sending messages
        other_peer_conn.send(vec!["HELLO".as_bytes().to_vec()]).unwrap();
        other_peer_conn.send(vec!["TARI".as_bytes().to_vec()]).unwrap();
        let _ = other_peer_conn.shutdown();
        other_peer_conn.wait_disconnected(&Duration::from_millis(1000)).unwrap();

        assert_eq!(msg_counter.count(), 2);

        peer_conn_handle.join().unwrap().unwrap();
    }
}

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

use super::{
    connections::LivePeerConnections,
    establisher::ConnectionEstablisher,
    protocol::PeerConnectionProtocol,
    ConnectionManagerError,
    EstablishLockResult,
    PeerConnectionConfig,
    Result,
};
use crate::{
    connection::{
        peer_connection::PeerConnectionJoinHandle,
        CurveEncryption,
        CurvePublicKey,
        Direction,
        PeerConnection,
        PeerConnectionError,
        PeerConnectionState,
        ZmqContext,
    },
    connection_manager::{dialer::Dialer, Connectivity},
    control_service::messages::RejectReason,
    message::FrameSet,
    peer_manager::{NodeId, NodeIdentity, Peer, PeerManager},
};
use futures::{channel::mpsc::Sender, Future};
use log::*;
use std::{
    collections::HashMap,
    result,
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};
use tari_utilities::{thread_join::thread_join::ThreadJoinWithTimeout, ByteArray};
use tokio_executor::blocking;

const LOG_TARGET: &str = "comms::connection_manager::manager";

struct ListenerConnection {
    connection: Arc<PeerConnection>,
    curve_public_key: CurvePublicKey,
    join_handle: Option<PeerConnectionJoinHandle>,
}

impl ListenerConnection {
    pub fn new(
        connection: Arc<PeerConnection>,
        curve_public_key: CurvePublicKey,
        join_handle: PeerConnectionJoinHandle,
    ) -> Self
    {
        Self {
            connection,
            curve_public_key,
            join_handle: Some(join_handle),
        }
    }

    fn get_connection(&self) -> Arc<PeerConnection> {
        Arc::clone(&self.connection)
    }

    fn curve_public_key(&self) -> &CurvePublicKey {
        &self.curve_public_key
    }
}

pub struct ConnectionManager {
    node_identity: Arc<NodeIdentity>,
    connections: LivePeerConnections,
    establisher: ConnectionEstablisher,
    peer_manager: Arc<PeerManager>,

    listener_connection: RwLock<Option<ListenerConnection>>,

    establish_locks: Mutex<HashMap<NodeId, Arc<Mutex<()>>>>,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new(
        zmq_context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        config: PeerConnectionConfig,
        message_sink_channel: Sender<FrameSet>,
    ) -> Self
    {
        Self {
            connections: LivePeerConnections::with_max_connections(config.max_connections),
            establisher: ConnectionEstablisher::new(
                zmq_context,
                Arc::clone(&node_identity),
                config,
                Arc::clone(&peer_manager),
                message_sink_channel,
            ),
            node_identity,
            peer_manager,
            listener_connection: RwLock::new(None),
            establish_locks: Mutex::new(HashMap::new()),
        }
    }

    pub fn run_listener(&self) -> Result<()> {
        debug!(target: LOG_TARGET, "Starting inbound listener");
        if self.is_listener_started() {
            debug!(target: LOG_TARGET, "Listener connection already started!");
            return Ok(());
        }

        let (secret_key, public_key) = CurveEncryption::generate_keypair()?;
        let (conn, join_handle) = self.establisher.establish_peer_listening_connection(secret_key)?;

        debug!(
            target: LOG_TARGET,
            "Listener started on {}",
            conn.get_connected_address()
                .map(|addr| addr.to_string())
                .or_else(|| conn.get_address().map(|addr| addr.to_string()))
                .unwrap_or("<unknown>".to_string())
        );
        let mut listener_connection = acquire_write_lock!(self.listener_connection);
        *listener_connection = Some(ListenerConnection::new(conn, public_key, join_handle));

        Ok(())
    }

    pub fn is_listener_started(&self) -> bool {
        acquire_read_lock!(self.listener_connection).is_some()
    }

    fn get_listener_connection_and_public_key(&self) -> Option<(Arc<PeerConnection>, CurvePublicKey)> {
        let lock = acquire_read_lock!(self.listener_connection);
        lock.as_ref()
            .map(|listener| (listener.get_connection(), listener.curve_public_key().clone()))
    }

    fn shutdown_listener(&self) -> std::result::Result<(), PeerConnectionError> {
        let mut lock = acquire_write_lock!(self.listener_connection);
        match lock.as_mut() {
            Some(listener) => {
                listener.get_connection().shutdown()?;
                let join_handle = listener
                    .join_handle
                    .take()
                    .expect("listener connection did not have a join handle");
                join_handle
                    .timeout_join(Duration::from_millis(5000))
                    .map_err(Into::into)
            },
            None => Ok(()),
        }
    }

    /// Attempt to establish a connection to a given NodeId. If the connection exists
    /// the existing connection is returned.
    pub fn establish_connection_to_node_id(&self, node_id: &NodeId) -> Result<Arc<PeerConnection>> {
        match self.peer_manager.find_by_node_id(node_id) {
            Ok(peer) => self.with_establish_lock(node_id, || self.attempt_connection_to_peer(&peer)),
            Err(err) => Err(ConnectionManagerError::PeerManagerError(err)),
        }
    }

    fn attempt_connection_to_peer(&self, peer: &Peer) -> Result<Arc<PeerConnection>> {
        match self.connections.get_connection(&peer.node_id) {
            Some(conn) => {
                match conn.direction() {
                    Direction::Inbound => {
                        // If the peer is connected to the single listener connection, check their connected status
                        match conn.test_connection(peer.node_id.to_vec()) {
                            Ok(_) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Inbound connection is connected to NodeId '{}'. Reusing connection", peer.node_id
                                );

                                Ok(conn)
                            },
                            Err(err) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Inbound connection not connected to NodeId '{}' because '{}'", peer.node_id, err
                                );

                                let _ = self.connections.disconnect_peer(&peer.node_id);
                                self.initiate_peer_connection(peer)
                            },
                        }
                    },
                    Direction::Outbound => {
                        let state = conn.get_state();

                        match state {
                            PeerConnectionState::Initial |
                            PeerConnectionState::Disconnected |
                            PeerConnectionState::Shutdown => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Peer connection state is '{}'. Attempting to reestablish connection to peer.",
                                    state
                                );
                                // Ignore not found error when dropping
                                let _ = self.disconnect_peer(&peer.node_id);
                                self.initiate_peer_connection(peer)
                            },
                            PeerConnectionState::Failed(err) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Peer connection for NodeId={} in failed state. Error({:?}) Attempting to \
                                     reestablish.",
                                    peer.node_id,
                                    err
                                );
                                // Ignore not found error when dropping
                                self.disconnect_peer(&peer.node_id)?;
                                self.initiate_peer_connection(peer)
                            },
                            // Already have an active connection, just return it
                            PeerConnectionState::Listening(Some(address)) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Waiting for NodeId={} to connect at {}...", peer.node_id, address
                                );
                                Ok(conn)
                            },
                            PeerConnectionState::Listening(None) => {
                                debug!(
                                    target: LOG_TARGET,
                                    "Listening on non-tcp socket for NodeId={}...", peer.node_id
                                );
                                Ok(conn)
                            },
                            PeerConnectionState::Connecting => {
                                debug!(target: LOG_TARGET, "Still connecting to {}...", peer.node_id);
                                Ok(conn)
                            },
                            PeerConnectionState::Connected(Some(address)) => {
                                debug!(target: LOG_TARGET, "Connection already established to {}.", address);
                                Ok(conn)
                            },
                            PeerConnectionState::Connected(None) => {
                                debug!(target: LOG_TARGET, "Connection already established to non-TCP socket");
                                Ok(conn)
                            },
                        }
                    },
                }
            },
            None => {
                debug!(
                    target: LOG_TARGET,
                    "Peer connection does not exist for NodeId={}", peer.node_id
                );

                self.initiate_peer_connection(peer)
            },
        }
    }

    /// Establish an inbound connection for the given peer and pass it (and it's `CurvePublicKey`) to a callback.
    /// That callback will determine whether the connection should be added to the live connection list. This
    /// enables you to for instance, implement a connection protocol which decides if the connection manager
    /// ultimately accepts the peer connection.
    ///
    /// ## Arguments
    ///
    /// - `peer`: &Peer - Create an inbound connection for this peer
    /// - `with_connection`: This callback is called with the new connection. If `Ok(Some(connection))` is returned, the
    ///   connection is added to the live connection list, otherwise it is discarded
    pub(crate) fn with_listener_connection<E>(
        &self,
        peer: &Peer,
        with_connection: impl FnOnce(Arc<PeerConnection>, CurvePublicKey) -> result::Result<Option<Arc<PeerConnection>>, E>,
    ) -> Result<()>
    where
        E: Into<ConnectionManagerError>,
    {
        // If we have reached the maximum connections, we won't allow new connections to be requested
        if self.connections.has_reached_max_active_connections() {
            return Err(ConnectionManagerError::MaxConnectionsReached);
        }

        if !self.is_listener_started() {
            return Err(ConnectionManagerError::ListenerNotStarted);
        }

        let (listener_connection, curve_public_key) =
            self.get_listener_connection_and_public_key().expect("already checked");

        match with_connection(listener_connection, curve_public_key).map_err(Into::into)? {
            Some(conn) => {
                self.connections.add_connection(peer.node_id.clone(), conn, None)?;
            },
            None => {},
        }

        Ok(())
    }

    /// Sends shutdown signals to all PeerConnections
    pub fn shutdown(&self) -> Vec<std::result::Result<(), PeerConnectionError>> {
        let result = self.shutdown_listener();
        let mut results = self.connections.shutdown_joined();
        results.push(result);
        results
    }

    /// Try to acquire an establish lock for the node ID. If a lock exists for the Node ID,
    /// then return `EstablishLockResult::Collision` is returned.
    pub fn try_acquire_establish_lock<T>(&self, node_id: &NodeId, func: impl FnOnce() -> T) -> EstablishLockResult<T> {
        if acquire_lock!(self.establish_locks).contains_key(node_id) {
            EstablishLockResult::Collision
        } else {
            self.with_establish_lock(node_id, || {
                let res = func();
                EstablishLockResult::Ok(res)
            })
        }
    }

    /// Lock a critical section for the given node id during connection establishment
    pub fn with_establish_lock<T>(&self, node_id: &NodeId, func: impl FnOnce() -> T) -> T {
        // Return the lock for the given node id. If no lock exists create a new one and return it.
        let nid_lock = {
            let mut establish_locks = acquire_lock!(self.establish_locks);
            match establish_locks.get(node_id) {
                Some(lock) => lock.clone(),
                None => {
                    let new_lock = Arc::new(Mutex::new(()));
                    establish_locks.insert(node_id.clone(), new_lock.clone());
                    new_lock
                },
            }
        };

        // Lock the lock for the NodeId
        let _nid_lock_guard = acquire_lock!(nid_lock);
        let ret = func();
        // Remove establish lock once done to release memory. This is safe because the function has already
        // established the connection, so any subsequent calls will return the existing connection.
        {
            let mut establish_locks = acquire_lock!(self.establish_locks);
            establish_locks.remove(node_id);
        }
        ret
    }

    /// Disconnects a given peer's [PeerConnection] and return it if one exists,
    /// otherwise None is returned.
    ///
    /// [PeerConnection]: ../../connection/peer_connection/index.html
    pub fn disconnect_peer(&self, node_id: &NodeId) -> Result<Option<Arc<PeerConnection>>> {
        match self.connections.disconnect_peer(&node_id) {
            Ok((conn, Some(handle))) => {
                handle
                    .timeout_join(Duration::from_millis(3000))
                    .map_err(ConnectionManagerError::PeerConnectionThreadError)?;
                Ok(Some(conn))
            },
            Ok((conn, None)) => Ok(Some(conn)),
            Err(ConnectionManagerError::PeerConnectionNotFound) => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Get the peer manager
    pub(crate) fn peer_manager(&self) -> &PeerManager {
        &self.peer_manager
    }

    /// Return a connection for a peer if one exists, otherwise None is returned
    pub(crate) fn get_connection(&self, peer: &Peer) -> Option<Arc<PeerConnection>> {
        self.get_connection_by_node_id(&peer.node_id)
    }

    /// Return a connection for a node id if one exists, otherwise None is returned
    pub(crate) fn get_connection_by_node_id(&self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        self.connections.get_connection(&node_id)
    }

    /// Return the number of _active_ peer connections currently managed by this instance
    pub fn get_active_connection_count(&self) -> usize {
        self.connections.get_active_connection_count()
    }

    fn initiate_peer_connection(&self, peer: &Peer) -> Result<Arc<PeerConnection>> {
        debug!(
            target: LOG_TARGET,
            "Initiating connection to peer. NodeId={}, connection_stats={:?}", peer.node_id, peer.connection_stats
        );

        let protocol = PeerConnectionProtocol::new(&self.node_identity, &self.establisher);
        self.peer_manager
            .reset_connection_attempts(&peer.node_id)
            .map_err(ConnectionManagerError::PeerManagerError)?;

        protocol
            .negotiate_peer_connection(peer)
            .and_then(|(new_conn, join_handle)| {
                let config = self.establisher.get_config();
                debug!(
                    target: LOG_TARGET,
                    "[{:?}] Waiting {}s for peer connection acceptance from remote peer ",
                    new_conn.get_address(),
                    config.peer_connection_establish_timeout.as_secs(),
                );

                // Wait for peer connection to transition to connected state before continuing
                new_conn
                    .wait_connected_or_failure(config.peer_connection_establish_timeout)
                    .or_else(|err| {
                        info!(
                            target: LOG_TARGET,
                            "Peer did not accept the connection within {:?} [NodeId={}] : {:?}",
                            config.peer_connection_establish_timeout,
                            peer.node_id,
                            err,
                        );
                        Err(err)
                    })?;
                debug!(
                    target: LOG_TARGET,
                    "[{:?}] Connection established. Adding to active peer connections.",
                    new_conn.get_address(),
                );

                self.connections
                    .add_connection(peer.node_id.clone(), Arc::clone(&new_conn), Some(join_handle))?;

                Ok(new_conn)
            })
            .or_else(|err| match err {
                ConnectionManagerError::ConnectionRejected(reason) => self.handle_connection_rejection(peer, reason),
                _ => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to establish peer connection to NodeId={}", peer.node_id
                    );
                    warn!(
                        target: LOG_TARGET,
                        "Connection error for NodeId={}: {:?}", peer.node_id, err
                    );
                    Err(err)
                },
            })
    }

    /// The peer is telling us that we already have a connection. This can occur if the connection has been made
    /// by the remote peer while attempting to connect to it. Let's look for a connection and return it if we have one.
    fn handle_connection_rejection(&self, peer: &Peer, reason: RejectReason) -> Result<Arc<PeerConnection>> {
        match reason {
            RejectReason::ExistingConnection => self
                .connections
                .get_active_connection(&peer.node_id)
                .ok_or(ConnectionManagerError::PeerConnectionNotFound),
            _ => Err(ConnectionManagerError::ConnectionRejected(reason)),
        }
    }
}

#[derive(Clone)]
pub struct ConnectionManagerDialer {
    inner: Arc<ConnectionManager>,
}

impl ConnectionManagerDialer {
    pub fn new(connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            inner: connection_manager,
        }
    }
}

impl From<Arc<ConnectionManager>> for ConnectionManagerDialer {
    fn from(connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            inner: connection_manager,
        }
    }
}

impl Dialer<NodeId> for ConnectionManagerDialer {
    type Error = ConnectionManagerError;
    type Output = Arc<PeerConnection>;

    type Future = impl Future<Output = Result<Self::Output>>;

    fn dial(&self, node_id: &NodeId) -> Self::Future {
        let inner = Arc::clone(&self.inner);
        let node_id = node_id.clone();
        blocking::run(move || {
            // TODO: This is synchronous until we can make connection manager fully async
            inner.establish_connection_to_node_id(&node_id)
        })
    }
}

impl Connectivity for ConnectionManagerDialer {
    fn get_connection(&self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        self.inner.get_connection_by_node_id(node_id)
    }

    fn get_active_connection_count(&self) -> usize {
        self.inner.get_active_connection_count()
    }

    fn disconnect_peer(&self, node_id: &NodeId) -> Result<Option<Arc<PeerConnection>>> {
        self.inner.disconnect_peer(node_id)
    }

    fn set_last_connection_failed(&self, node_id: &NodeId) -> Result<()> {
        self.inner.peer_manager().set_failed_connection_state(&node_id)?;
        debug!(target: LOG_TARGET, "Set failed connection state for node {}", node_id);
        Ok(())
    }

    fn set_last_connection_succeeded(&self, node_id: &NodeId) -> Result<()> {
        self.inner.peer_manager().set_success_connection_state(&node_id)?;
        debug!(target: LOG_TARGET, "Set success connection state for node {}", node_id);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::{NetAddress, ZmqContext},
        peer_manager::{PeerFeatures, PeerFlags},
        types::CommsPublicKey,
    };
    use futures::channel::mpsc::channel;
    use rand::rngs::OsRng;
    use std::{thread, time::Duration};
    use tari_crypto::keys::PublicKey;
    use tari_storage::HMapDatabase;

    fn setup() -> (ZmqContext, Arc<NodeIdentity>, Arc<PeerManager>, Sender<FrameSet>) {
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None, PeerFeatures::empty()));
        let peer_manager = Arc::new(PeerManager::new(HMapDatabase::new()).unwrap());
        let (tx, _rx) = channel(10);
        (context, node_identity, peer_manager, tx)
    }

    fn create_peer(address: NetAddress) -> Peer {
        let (_, pk) = CommsPublicKey::random_keypair(&mut OsRng::new().unwrap());
        let node_id = NodeId::from_key(&pk).unwrap();
        Peer::new(pk, node_id, address.into(), PeerFlags::empty(), PeerFeatures::empty())
    }

    #[test]
    fn get_active_connection_count() {
        let (context, node_identity, peer_manager, sender) = setup();
        let manager = ConnectionManager::new(
            context,
            node_identity,
            peer_manager,
            PeerConnectionConfig {
                peer_connection_establish_timeout: Duration::from_secs(5),
                max_message_size: 1024,
                listening_address: "127.0.0.1:0".parse().unwrap(),
                max_connect_retries: 3,
                max_connections: 10,

                socks_proxy_address: None,
            },
            sender,
        );

        assert_eq!(manager.get_active_connection_count(), 0);
    }

    #[test]
    fn disconnect_peer() {
        let (context, node_identity, peer_manager, sender) = setup();
        let manager = ConnectionManager::new(
            context,
            node_identity,
            peer_manager,
            PeerConnectionConfig {
                peer_connection_establish_timeout: Duration::from_secs(5),
                max_message_size: 1024,
                listening_address: "127.0.0.1:0".parse().unwrap(),
                max_connect_retries: 3,
                max_connections: 10,
                socks_proxy_address: None,
            },
            sender,
        );

        assert_eq!(manager.get_active_connection_count(), 0);

        let address = "127.0.0.1:43456".parse::<NetAddress>().unwrap();
        let peer = create_peer(address.clone());

        assert!(manager.disconnect_peer(&peer.node_id).unwrap().is_none());

        let (peer_conn, rx) = PeerConnection::new_with_connecting_state_for_test("127.0.0.1:0".parse().unwrap());
        let peer_conn = Arc::new(peer_conn);
        let join_handle = thread::spawn(|| Ok(()));
        manager
            .connections
            .add_connection(peer.node_id.clone(), peer_conn, Some(join_handle))
            .unwrap();

        match manager.disconnect_peer(&peer.node_id).unwrap() {
            Some(_) => {},
            None => panic!("disconnect_peer did not return active peer connection"),
        }

        drop(rx);
    }
}

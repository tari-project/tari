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
    error::ConnectionManagerError,
    repository::{ConnectionRepository, PeerConnectionEntry, Repository},
    Result,
};

use crate::{
    connection::{
        self,
        curve_keypair::{CurvePublicKey, CurveSecretKey},
        net_address::ip::SocketAddress,
        Context,
        CurveEncryption,
        Direction,
        NetAddress,
        NetAddresses,
        PeerConnection,
        PeerConnectionContextBuilder,
        PeerConnectionState,
    },
    peer_manager::node_id::NodeId,
};

use crate::connection::InprocAddress;
use log::*;
use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
    thread::JoinHandle,
    time::Duration,
};

const LOG_TARGET: &'static str = "comms::connection_manager::connections";

pub struct PeerConnectionConfig {
    pub max_message_size: u64,
    pub max_connect_retries: u16,
    pub socks_proxy_address: Option<SocketAddress>,
    pub consumer_address: InprocAddress,
    pub host: IpAddr,
    pub establish_timeout: Duration,
}

/// Indicates the type of connection to establish.
pub enum ConnectionDirection {
    /// Listen for a connection from a peer and possibly accept their connection
    Inbound {
        node_id: NodeId,
        secret_key: CurveSecretKey,
    },
    /// Connect to a peer and wait for them to possibly accept
    Outbound {
        node_id: NodeId,
        net_addresses: NetAddresses,
        server_public_key: CurvePublicKey,
    },
}

/// Stores, and establishes the live peer connections
pub struct LivePeerConnections {
    context: Context,
    repository: RwLock<ConnectionRepository>,
    pub(crate) config: PeerConnectionConfig,
    connection_thread_handles: RwLock<HashMap<NodeId, JoinHandle<connection::Result<()>>>>,
}

impl LivePeerConnections {
    /// Create a new live peer connection
    pub fn new(context: Context, config: PeerConnectionConfig) -> Self {
        Self {
            context,
            config,
            repository: RwLock::new(ConnectionRepository::default()),
            connection_thread_handles: RwLock::new(HashMap::new()),
        }
    }

    /// Borrow the connection context
    pub fn borrow_context(&self) -> &Context {
        &self.context
    }

    /// Borrow the PeerConnectionConfig
    pub fn borrow_config(&self) -> &PeerConnectionConfig {
        &self.config
    }

    /// Get a connection byy node id
    pub fn get_connection(&self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        self.atomic_read(|lock| lock.get(node_id).map(|entry| entry.connection.clone()))
    }

    /// Get a connection by node id only if it is active
    pub fn get_active_connection(&self, node_id: &NodeId) -> Option<Arc<PeerConnection>> {
        self.get_connection_if(node_id, |connection| connection.is_active())
    }

    /// Get number of active connections
    pub fn get_active_connection_count(&self) -> usize {
        self.atomic_read(|repo| repo.count_where(|entry| entry.connection.is_active()))
    }

    /// Get the state for a connection
    pub fn get_connection_state(&self, node_id: &NodeId) -> Option<PeerConnectionState> {
        self.get_connection(node_id).and_then(|conn| conn.get_state().ok())
    }

    /// Establish a new peer connection
    pub fn establish_connection(&self, direction: ConnectionDirection) -> Result<NetAddress> {
        match direction {
            ConnectionDirection::Inbound { node_id, secret_key } => {
                self.establish_inbound_connection(node_id, secret_key)
            },
            ConnectionDirection::Outbound {
                node_id,
                net_addresses,
                server_public_key,
            } => self.establish_outbound_connection(node_id, net_addresses, server_public_key),
        }
    }

    fn establish_outbound_connection(
        &self,
        node_id: NodeId,
        mut addresses: NetAddresses,
        server_public_key: CurvePublicKey,
    ) -> Result<NetAddress>
    {
        debug!("Establishing outbound connection to {}", node_id);
        let mut repo = acquire_write_lock!(self.repository);
        let (secret_key, public_key) = CurveEncryption::generate_keypair()?;

        let address = addresses
            .get_best_net_address()
            .map_err(ConnectionManagerError::NetAddressError)?;

        let context = self
            .new_context_builder()
            .set_id(node_id.clone())
            .set_direction(Direction::Outbound)
            .set_address(address.clone())
            .set_curve_encryption(CurveEncryption::Client {
                secret_key,
                public_key,
                server_public_key,
            })
            .build()?;

        let connection = PeerConnection::new();
        let worker_handle = connection.start(context)?;
        acquire_write_lock!(self.connection_thread_handles).insert(node_id.clone(), worker_handle);

        let connection = Arc::new(connection);
        debug!("Outbound connection to {} established.", node_id);
        repo.insert(node_id, PeerConnectionEntry {
            connection,
            address: address.clone(),
            direction: Direction::Outbound,
        });
        Ok(address)
    }

    fn establish_inbound_connection(&self, node_id: NodeId, secret_key: CurveSecretKey) -> Result<NetAddress> {
        debug!("Establishing inbound connection from {}", node_id);
        let mut repo = acquire_write_lock!(self.repository);

        // Providing port 0 tells the OS to allocate a port for us
        let address = NetAddress::IP((self.config.host, 0).into());
        let context = self
            .new_context_builder()
            .set_id(node_id.clone())
            .set_direction(Direction::Inbound)
            .set_address(address.clone())
            .set_curve_encryption(CurveEncryption::Server { secret_key })
            .build()?;

        let connection = PeerConnection::new();
        let worker_handle = connection.start(context)?;
        acquire_write_lock!(self.connection_thread_handles).insert(node_id.clone(), worker_handle);

        let connection = Arc::new(connection);
        let connected_address = connection
            .get_connected_address()
            .and_then(|a| Some(a.to_string()))
            .unwrap_or("non-TCP socket".to_string());
        debug!("Connection to {} on {}", node_id, connected_address);
        repo.insert(node_id, PeerConnectionEntry {
            connection,
            address: address.clone(),
            direction: Direction::Inbound,
        });

        Ok(address)
    }

    pub fn drop_connection(&self, node_id: &NodeId) -> Result<Arc<PeerConnection>> {
        self.atomic_write(|mut repo| {
            repo.remove(node_id)
                .ok_or(ConnectionManagerError::PeerConnectionNotFound)
                .map(|entry| entry.connection.clone())
        })
    }

    pub fn shutdown_wait(self) -> Result<()> {
        self.atomic_read(|repo| {
            repo.for_each(|entry| {
                // TODO: if shutdown message fails (possible?) log and don't join the corresponding handle (or something
                // simpler)
                let _ = entry.connection.shutdown();
            });
        });
        // Wait for all peer connection threads to exit
        let thread_handles = acquire_lock!(self.connection_thread_handles, into_inner);

        for (_, handle) in thread_handles.into_iter() {
            if let Ok(result) = handle.join() {
                result.map_err(ConnectionManagerError::ConnectionError)?
            }
        }

        Ok(())
    }

    fn atomic_write<F, T>(&self, f: F) -> T
    where F: FnOnce(RwLockWriteGuard<ConnectionRepository>) -> T {
        let lock = acquire_write_lock!(self.repository);
        f(lock)
    }

    fn atomic_read<F, T>(&self, f: F) -> T
    where F: FnOnce(RwLockReadGuard<ConnectionRepository>) -> T {
        let lock = acquire_read_lock!(self.repository);
        f(lock)
    }

    fn get_connection_if<P>(&self, node_id: &NodeId, predicate: P) -> Option<Arc<PeerConnection>>
    where P: FnOnce(&Arc<PeerConnection>) -> bool {
        self.get_connection(node_id).filter(predicate)
    }

    fn new_context_builder(&self) -> PeerConnectionContextBuilder {
        let config = &self.config;

        let mut builder = PeerConnectionContextBuilder::new()
            .set_context(&self.context)
            .set_max_msg_size(config.max_message_size)
            .set_consumer_address(config.consumer_address.clone())
            .set_max_retry_attempts(config.max_connect_retries);

        if let Some(ref addr) = config.socks_proxy_address {
            builder = builder.set_socks_proxy(addr.clone());
        }

        builder
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::connection::{CurveEncryption, InprocAddress};
    use std::{thread, time::Duration};
    use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};

    fn pause() {
        thread::sleep(Duration::from_millis(5));
    }

    fn make_connection_manager_config(consumer_address: InprocAddress) -> PeerConnectionConfig {
        PeerConnectionConfig {
            host: "127.0.0.1".parse().unwrap(),
            socks_proxy_address: None,
            consumer_address,
            max_connect_retries: 5,
            max_message_size: 512 * 1024,
            establish_timeout: Duration::from_millis(2000),
        }
    }

    fn make_live_connections(context: &Context) -> (LivePeerConnections, CurveSecretKey, CurvePublicKey) {
        let consumer_address = InprocAddress::random();
        let (secret_key, public_key) = CurveEncryption::generate_keypair().unwrap();
        let live_connections =
            LivePeerConnections::new(context.clone(), make_connection_manager_config(consumer_address));
        (live_connections, secret_key, public_key)
    }

    fn make_node_id() -> NodeId {
        let (_secret_key, public_key) = RistrettoPublicKey::random_keypair(&mut rand::OsRng::new().unwrap());
        NodeId::from_key(&public_key).unwrap()
    }

    #[test]
    fn get_active_connection_count() {
        let context = Context::new();
        let (connections, secret_key, _) = make_live_connections(&context);
        assert_eq!(0, connections.get_active_connection_count());

        let node_id = make_node_id();
        connections
            .establish_connection(ConnectionDirection::Inbound {
                node_id: node_id.clone(),
                secret_key: secret_key.clone(),
            })
            .unwrap();
        connections
            .get_connection(&node_id)
            .unwrap()
            .wait_connected_or_failure(Duration::from_millis(100))
            .unwrap();
        let node_id2 = make_node_id();
        connections
            .establish_connection(ConnectionDirection::Inbound {
                node_id: node_id2.clone(),
                secret_key: secret_key.clone(),
            })
            .unwrap();
        connections
            .get_connection(&node_id2)
            .unwrap()
            .wait_connected_or_failure(Duration::from_millis(100))
            .unwrap();

        assert_eq!(2, connections.get_active_connection_count());

        let conn = connections.get_connection(&node_id2).unwrap();
        conn.shutdown().unwrap();
        conn.wait_disconnected(Duration::from_millis(2000)).unwrap();

        assert_eq!(1, connections.get_active_connection_count());
    }

    #[test]
    fn get_connection_state() {
        let context = Context::new();
        let node_id = make_node_id();
        let (establisher, secret_key, _) = make_live_connections(&context);

        establisher
            .establish_connection(ConnectionDirection::Inbound {
                node_id: node_id.clone(),
                secret_key,
            })
            .unwrap();

        match establisher.get_connection_state(&node_id).unwrap() {
            PeerConnectionState::Connecting | PeerConnectionState::Connected(_) => {},
            _ => panic!("Invalid state"),
        }
    }
}

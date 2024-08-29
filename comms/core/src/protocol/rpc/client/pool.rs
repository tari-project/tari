//  Copyright 2021, The Tari Project
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

use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
};

use log::*;
use tokio::sync::Mutex;

use crate::{
    peer_manager::NodeId,
    protocol::rpc::{
        error::HandshakeRejectReason,
        NamedProtocolService,
        RpcClient,
        RpcClientBuilder,
        RpcError,
        RpcHandshakeError,
    },
    PeerConnection,
};

const LOG_TARGET: &str = "comms::protocol::rpc::client_pool";

#[derive(Clone)]
pub struct RpcClientPool<T> {
    pool: Arc<Mutex<LazyPool<T>>>,
}

impl<T> RpcClientPool<T>
where T: RpcPoolClient + From<RpcClient> + NamedProtocolService + Clone
{
    /// Create a new RpcClientPool. Panics if passed a pool_size of 0.
    pub(crate) fn new(peer_connection: PeerConnection, pool_size: usize, client_config: RpcClientBuilder<T>) -> Self {
        let pool = LazyPool::new(peer_connection, pool_size, client_config);
        Self {
            pool: Arc::new(Mutex::new(pool)),
        }
    }

    pub async fn get(&self) -> Result<RpcClientLease<T>, RpcClientPoolError> {
        let mut pool = self.pool.lock().await;
        pool.get_least_used_or_connect().await
    }

    pub async fn is_connected(&self) -> bool {
        let pool = self.pool.lock().await;
        pool.is_connected()
    }
}

#[derive(Clone)]
pub(crate) struct LazyPool<T> {
    connection: PeerConnection,
    clients: Vec<RpcClientLease<T>>,
    client_config: RpcClientBuilder<T>,
}

impl<T> LazyPool<T>
where T: RpcPoolClient + From<RpcClient> + NamedProtocolService + Clone
{
    pub fn new(connection: PeerConnection, capacity: usize, client_config: RpcClientBuilder<T>) -> Self {
        assert!(capacity > 0, "Pool capacity of 0 is invalid");
        Self {
            connection,
            clients: Vec::with_capacity(capacity),
            client_config,
        }
    }

    pub async fn get_least_used_or_connect(&mut self) -> Result<RpcClientLease<T>, RpcClientPoolError> {
        loop {
            self.check_peer_connection()?;

            let client = match self.get_next_lease() {
                Some(c) => c,
                None => match self.add_new_client_session().await {
                    Ok(c) => c,
                    // This is an edge case where the remote node does not have any further sessions available. This is
                    // gracefully handled by returning one of the existing used sessions.
                    Err(RpcClientPoolError::NoMoreRemoteServerRpcSessions(val)) => self
                        .get_least_used()
                        .ok_or(RpcClientPoolError::NoMoreRemoteServerRpcSessions(val))?,
                    Err(RpcClientPoolError::NoMoreRemoteClientRpcSessions(val)) => self
                        .get_least_used()
                        .ok_or(RpcClientPoolError::NoMoreRemoteClientRpcSessions(val))?,
                    Err(err) => {
                        return Err(err);
                    },
                },
            };

            if !client.is_connected() {
                self.prune();
                continue;
            }

            // Clone is what actually takes the lease out (increments the Arc)
            return Ok(client.clone());
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connection.is_connected()
    }

    #[allow(dead_code)]
    pub(super) fn refresh_num_active_connections(&mut self) -> usize {
        self.prune();
        self.clients.len()
    }

    fn check_peer_connection(&self) -> Result<(), RpcClientPoolError> {
        if self.connection.is_connected() {
            Ok(())
        } else {
            Err(RpcClientPoolError::PeerConnectionDropped {
                peer: self.connection.peer_node_id().clone(),
            })
        }
    }

    /// Return the next client that is not in use. If all clients are in use and there are still more slots open in the
    /// pool, None is returned. Otherwise, we return a client with the least uses.
    /// If the pool is full, this function is _guaranteed_ to return Some(&T), however it is up to the caller to
    /// check that the session is still connected.
    fn get_next_lease(&self) -> Option<&RpcClientLease<T>> {
        let client = self.get_least_used()?;
        // If the pool is full, we choose the client with the smallest lease_count (i.e. the one that is being used
        // the least or not at all).
        if self.is_full() {
            return Some(client);
        }

        // Otherwise, if the least used connection is still in use and since there is capacity for more connections,
        // return None. This indicates that the best option is to create a new connection.
        if client.lease_count() > 0 {
            return None;
        }

        Some(client)
    }

    fn get_least_used(&self) -> Option<&RpcClientLease<T>> {
        let mut min_count = usize::MAX;
        let mut selected_client = None;
        for client in &self.clients {
            let lease_count = client.lease_count();
            if lease_count == 0 {
                return Some(client);
            }

            if min_count > lease_count {
                selected_client = Some(client);
                min_count = lease_count;
            }
        }

        selected_client
    }

    pub fn is_full(&self) -> bool {
        self.clients.len() == self.clients.capacity()
    }

    async fn add_new_client_session(&mut self) -> Result<&RpcClientLease<T>, RpcClientPoolError> {
        debug_assert!(!self.is_full(), "add_new_client called when pool is full");
        let client = self
            .connection
            .connect_rpc_using_builder(self.client_config.clone())
            .await?;
        let client = RpcClientLease::new(client);
        self.clients.push(client);
        Ok(self.clients.last().unwrap())
    }

    fn prune(&mut self) {
        let initial_len = self.clients.len();
        let cap = self.clients.capacity();
        self.clients = self.clients.drain(..).fold(Vec::with_capacity(cap), |mut vec, c| {
            if c.is_connected() {
                vec.push(c);
            }
            vec
        });
        assert_eq!(self.clients.capacity(), cap);
        debug!(
            target: LOG_TARGET,
            "Pruned {} client(s) (total connections: {})",
            initial_len - self.clients.len(),
            self.clients.len()
        )
    }
}

/// A lease of a client RPC session. This is a thin wrapper that provides an atomic reference counted lease around an
/// RPC client session. This wrapper dereferences into the client it holds, meaning that it can be used in the same way
/// as the inner client itself.
#[derive(Debug, Clone)]
pub struct RpcClientLease<T> {
    inner: T,
    rc: Arc<()>,
}

impl<T> RpcClientLease<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner,
            rc: Arc::new(()),
        }
    }

    /// Returns the number of active leases for this client
    pub(super) fn lease_count(&self) -> usize {
        Arc::strong_count(&self.rc) - 1
    }
}

impl<T> Deref for RpcClientLease<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for RpcClientLease<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<T: RpcPoolClient> RpcPoolClient for RpcClientLease<T> {
    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RpcClientPoolError {
    #[error("Peer connection to peer '{peer}' dropped")]
    PeerConnectionDropped { peer: NodeId },
    #[error("No peer RPC server sessions are available")]
    NoMoreRemoteServerRpcSessions(String),
    #[error("No peer RPC client sessions are available")]
    NoMoreRemoteClientRpcSessions(String),
    #[error("Failed to create client connection: {0}")]
    FailedToConnect(RpcError),
}

impl From<RpcError> for RpcClientPoolError {
    fn from(err: RpcError) -> Self {
        match err {
            RpcError::HandshakeError(RpcHandshakeError::Rejected(
                HandshakeRejectReason::NoServerSessionsAvailable(val),
            )) => RpcClientPoolError::NoMoreRemoteServerRpcSessions(val.to_string()),
            RpcError::HandshakeError(RpcHandshakeError::Rejected(
                HandshakeRejectReason::NoClientSessionsAvailable(val),
            )) => RpcClientPoolError::NoMoreRemoteClientRpcSessions(val.to_string()),
            err => RpcClientPoolError::FailedToConnect(err),
        }
    }
}

pub trait RpcPoolClient {
    fn is_connected(&self) -> bool;
}

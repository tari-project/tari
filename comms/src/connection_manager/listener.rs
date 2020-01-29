// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use super::error::ConnectionManagerError;
use crate::{
    bounded_executor::BoundedExecutor,
    connection::ConnectionDirection,
    connection_manager::{
        common,
        next::{ConnectionManagerConfig, ConnectionManagerEvent},
        peer_connection::{self, PeerConnection},
    },
    multiaddr::Multiaddr,
    multiplexing::yamux::Yamux,
    noise::NoiseConfig,
    peer_manager::{AsyncPeerManager, NodeIdentity},
    protocol::ProtocolId,
    transports::Transport,
};
use futures::{channel::mpsc, AsyncRead, AsyncWrite, SinkExt, StreamExt};
use log::*;
use std::sync::Arc;
use tari_shutdown::ShutdownSignal;
use tokio::runtime;

const LOG_TARGET: &str = "comms::connection_manager::listener";

pub struct PeerListener<TTransport> {
    listen_address: Multiaddr,
    executor: runtime::Handle,
    bounded_executor: BoundedExecutor,
    conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
    shutdown_signal: Option<ShutdownSignal>,
    transport: TTransport,
    noise_config: NoiseConfig,
    peer_manager: AsyncPeerManager,
    node_identity: Arc<NodeIdentity>,
    listening_address: Option<Multiaddr>,
    our_supported_protocols: Vec<ProtocolId>,
}

impl<TTransport> PeerListener<TTransport>
where
    TTransport: Transport,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    pub fn new(
        executor: runtime::Handle,
        config: ConnectionManagerConfig,
        transport: TTransport,
        noise_config: NoiseConfig,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        peer_manager: AsyncPeerManager,
        node_identity: Arc<NodeIdentity>,
        supported_protocols: Vec<ProtocolId>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            executor: executor.clone(),
            listen_address: config.listener_address,
            transport,
            noise_config,
            conn_man_notifier,
            peer_manager,
            node_identity,
            shutdown_signal: Some(shutdown_signal),
            listening_address: None,
            our_supported_protocols: supported_protocols,
            bounded_executor: BoundedExecutor::new(executor, config.max_simultaneous_inbound_connects),
        }
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("PeerListener initialized without a ShutdownSignal");

        match self.listen().await {
            Ok((inbound, address)) => {
                let inbound = inbound.fuse();
                futures::pin_mut!(inbound);

                info!(target: LOG_TARGET, "Listening for peer connection on '{}'", address);
                self.listening_address = Some(address.clone());

                self.send_event(ConnectionManagerEvent::Listening(address)).await;

                loop {
                    futures::select! {
                        inbound_result = inbound.select_next_some() => {
                            if let Some((inbound_future, peer_addr)) = log_if_error!(target: LOG_TARGET, inbound_result, "Inbound connection failed because '{error}'",) {
                                if let Some(socket) = log_if_error!(target: LOG_TARGET, inbound_future.await,  "Inbound connection failed because '{error}'",) {
                                    self.spawn_listen_task(socket, peer_addr).await;
                                }
                            }
                        },
                        _ = shutdown_signal => {
                            info!(target: LOG_TARGET, "PeerListener is shutting down because the shutdown signal was triggered");
                            break;
                        },
                    }
                }
            },
            Err(err) => {
                error!(target: LOG_TARGET, "PeerListener was unable to start because '{}'", err);
                self.send_event(ConnectionManagerEvent::ListenFailed(err)).await;
            },
        }
    }

    async fn spawn_listen_task(&self, socket: TTransport::Output, peer_addr: Multiaddr) {
        let executor = self.executor.clone();
        let node_identity = self.node_identity.clone();
        let peer_manager = self.peer_manager.clone();
        let mut conn_man_notifier = self.conn_man_notifier.clone();
        let noise_config = self.noise_config.clone();
        let our_supported_protocols = self.our_supported_protocols.clone();

        // This will block (asynchronously) if we have reached the maximum simultaneous connections, creating
        // back-pressure on nodes connecting to this node
        self.bounded_executor
            .spawn(async move {
                let result = Self::perform_socket_upgrade_procedure(
                    executor,
                    node_identity,
                    peer_manager,
                    noise_config,
                    conn_man_notifier.clone(),
                    socket,
                    peer_addr,
                    our_supported_protocols,
                )
                .await;

                match result {
                    Ok(peer_conn) => {
                        log_if_error!(
                            target: LOG_TARGET,
                            conn_man_notifier
                                .send(ConnectionManagerEvent::PeerConnected(Box::new(peer_conn)))
                                .await,
                            "Failed to publish event because '{error}'",
                        );
                    },
                    Err(err) => {
                        log_if_error!(
                            target: LOG_TARGET,
                            conn_man_notifier
                                .send(ConnectionManagerEvent::PeerInboundConnectFailed(err))
                                .await,
                            "Failed to publish event because '{error}'",
                        );
                    },
                }
            })
            .await;
    }

    async fn send_event(&mut self, event: ConnectionManagerEvent) {
        log_if_error_fmt!(
            target: LOG_TARGET,
            self.conn_man_notifier.send(event).await,
            "Failed to send connection manager event in listener",
        );
    }

    async fn perform_socket_upgrade_procedure(
        executor: runtime::Handle,
        node_identity: Arc<NodeIdentity>,
        peer_manager: AsyncPeerManager,
        noise_config: NoiseConfig,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        socket: TTransport::Output,
        peer_addr: Multiaddr,
        our_supported_protocols: Vec<ProtocolId>,
    ) -> Result<PeerConnection, ConnectionManagerError>
    {
        static CONNECTION_DIRECTION: ConnectionDirection = ConnectionDirection::Inbound;
        debug!(
            target: LOG_TARGET,
            "Starting noise protocol upgrade for peer at address '{}'", peer_addr
        );

        let noise_socket = noise_config
            .upgrade_socket(socket, CONNECTION_DIRECTION)
            .await
            .map_err(|err| ConnectionManagerError::NoiseError(err.to_string()))?;

        let authenticated_public_key = noise_socket
            .get_remote_public_key()
            .ok_or(ConnectionManagerError::InvalidStaticPublicKey)?;

        let mut muxer = Yamux::upgrade_connection(executor.clone(), noise_socket, CONNECTION_DIRECTION)
            .await
            .map_err(|err| ConnectionManagerError::YamuxUpgradeFailure(err.to_string()))?;

        trace!(
            target: LOG_TARGET,
            "Starting peer identity exchange for peer with public key '{}'",
            authenticated_public_key
        );
        let peer_identity = common::perform_identity_exchange(&mut muxer, node_identity, CONNECTION_DIRECTION).await?;

        let peer_node_id =
            common::validate_and_add_peer_from_peer_identity(&peer_manager, authenticated_public_key, peer_identity)
                .await?;

        peer_connection::create(
            executor,
            muxer,
            peer_addr,
            peer_node_id,
            CONNECTION_DIRECTION,
            conn_man_notifier,
            our_supported_protocols,
        )
    }

    async fn listen(&self) -> Result<(TTransport::Listener, Multiaddr), ConnectionManagerError> {
        self.transport
            .listen(self.listen_address.clone())
            .await
            .map_err(|err| ConnectionManagerError::TransportError(err.to_string()))
    }
}

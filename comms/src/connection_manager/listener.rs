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

use super::{
    common,
    error::ConnectionManagerError,
    peer_connection::{self, PeerConnection},
    types::ConnectionDirection,
    ConnectionManagerConfig,
    ConnectionManagerEvent,
};
use crate::{
    bounded_executor::BoundedExecutor,
    connection_manager::{liveness::LivenessSession, wire_mode::WireMode},
    multiaddr::Multiaddr,
    multiplexing::Yamux,
    noise::NoiseConfig,
    peer_manager::{NodeIdentity, PeerFeatures},
    protocol::ProtocolId,
    runtime,
    transports::Transport,
    utils::multiaddr::multiaddr_to_socketaddr,
    PeerManager,
};
use futures::{channel::mpsc, future, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, SinkExt, StreamExt};
use log::*;
use std::{
    convert::TryInto,
    mem,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tari_crypto::tari_utilities::hex::Hex;
use tari_shutdown::ShutdownSignal;
use tokio::time;

const LOG_TARGET: &str = "comms::connection_manager::listener";

pub struct PeerListener<TTransport> {
    config: ConnectionManagerConfig,
    bounded_executor: BoundedExecutor,
    conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
    shutdown_signal: ShutdownSignal,
    transport: TTransport,
    noise_config: NoiseConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    listening_address: Option<Multiaddr>,
    our_supported_protocols: Vec<ProtocolId>,
    liveness_session_count: Arc<AtomicUsize>,
}

impl<TTransport> PeerListener<TTransport>
where
    TTransport: Transport,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        config: ConnectionManagerConfig,
        transport: TTransport,
        noise_config: NoiseConfig,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        shutdown_signal: ShutdownSignal,
    ) -> Self
    {
        Self {
            transport,
            noise_config,
            conn_man_notifier,
            peer_manager,
            node_identity,
            shutdown_signal,
            listening_address: None,
            our_supported_protocols: Vec::new(),
            bounded_executor: BoundedExecutor::from_current(config.max_simultaneous_inbound_connects),
            liveness_session_count: Arc::new(AtomicUsize::new(config.liveness_max_sessions)),
            config,
        }
    }

    /// Set the supported protocols of this node to send to peers during the peer identity exchange
    pub fn set_supported_protocols(&mut self, our_supported_protocols: Vec<ProtocolId>) -> &mut Self {
        self.our_supported_protocols = our_supported_protocols;
        self
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self.shutdown_signal.clone();

        match self.listen().await {
            Ok((inbound, address)) => {
                let inbound = inbound.fuse();
                futures::pin_mut!(inbound);

                info!(target: LOG_TARGET, "Listening for peer connections on '{}'", address);
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
                warn!(target: LOG_TARGET, "PeerListener was unable to start because '{}'", err);
                self.send_event(ConnectionManagerEvent::ListenFailed(err)).await;
            },
        }
    }

    async fn read_wire_format(socket: &mut TTransport::Output, time_to_first_byte: Duration) -> Option<WireMode> {
        let mut buf = [0u8; 1];
        match time::timeout(time_to_first_byte, socket.read_exact(&mut buf))
            .await
            .ok()?
        {
            Ok(_) => match buf[0].try_into().ok() {
                Some(wf) => Some(wf),
                None => {
                    warn!(target: LOG_TARGET, "Invalid wire format byte '{}'", buf[0]);
                    None
                },
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "Failed to read first byte: {}", err);
                None
            },
        }
    }

    fn is_address_in_liveness_cidr_range(addr: &Multiaddr, allowlist: &[cidr::AnyIpCidr]) -> bool {
        match multiaddr_to_socketaddr(addr) {
            Ok(socket_addr) => allowlist.iter().any(|cidr| cidr.contains(&socket_addr.ip())),
            Err(_) => {
                warn!(
                    target: LOG_TARGET,
                    "Peer address '{}' is invalid for liveness checks. It must be an TCP/IP address.", addr
                );
                false
            },
        }
    }

    async fn spawn_liveness_session(
        socket: TTransport::Output,
        permit: Arc<AtomicUsize>,
        shutdown_signal: ShutdownSignal,
    )
    {
        permit.fetch_sub(1, Ordering::SeqCst);
        let liveness = LivenessSession::new(socket);
        debug!(target: LOG_TARGET, "Started liveness session");
        runtime::current().spawn(async move {
            future::select(liveness.run(), shutdown_signal).await;
            permit.fetch_add(1, Ordering::SeqCst);
        });
    }

    async fn spawn_listen_task(&self, mut socket: TTransport::Output, peer_addr: Multiaddr) {
        let node_identity = self.node_identity.clone();
        let peer_manager = self.peer_manager.clone();
        let mut conn_man_notifier = self.conn_man_notifier.clone();
        let noise_config = self.noise_config.clone();
        let config = self.config.clone();
        let our_supported_protocols = self.our_supported_protocols.clone();
        let allow_test_addresses = self.config.allow_test_addresses;
        let liveness_session_count = self.liveness_session_count.clone();
        let user_agent = self.config.user_agent.clone();
        let shutdown_signal = self.shutdown_signal.clone();

        let inbound_fut = async move {
            match Self::read_wire_format(&mut socket, config.time_to_first_byte).await {
                Some(WireMode::Comms) => {
                    let this_node_id_str = node_identity.node_id().short_str();
                    let result = Self::perform_socket_upgrade_procedure(
                        node_identity,
                        peer_manager,
                        noise_config,
                        conn_man_notifier.clone(),
                        socket,
                        peer_addr,
                        our_supported_protocols,
                        user_agent,
                        allow_test_addresses,
                    )
                    .await;

                    match result {
                        Ok(peer_conn) => {
                            log_if_error!(
                                target: LOG_TARGET,
                                conn_man_notifier
                                    .send(ConnectionManagerEvent::PeerConnected(peer_conn))
                                    .await,
                                "Failed to publish event because '{error}'",
                            );
                        },
                        Err(err) => {
                            debug!(
                                target: LOG_TARGET,
                                "[ThisNode={}] Peer connection upgrade failed for peer because '{:?}'",
                                this_node_id_str,
                                err
                            );
                            log_if_error!(
                                target: LOG_TARGET,
                                conn_man_notifier
                                    .send(ConnectionManagerEvent::PeerInboundConnectFailed(err))
                                    .await,
                                "Failed to publish event because '{error}'",
                            );
                        },
                    }
                },
                Some(WireMode::Liveness) => {
                    if liveness_session_count.load(Ordering::SeqCst) > 0 &&
                        Self::is_address_in_liveness_cidr_range(&peer_addr, &config.liveness_cidr_allowlist)
                    {
                        debug!(
                            target: LOG_TARGET,
                            "Connection at address '{}' requested liveness session", peer_addr
                        );
                        Self::spawn_liveness_session(socket, liveness_session_count, shutdown_signal).await;
                    } else {
                        debug!(
                            target: LOG_TARGET,
                            "No liveness sessions available or permitted for peer address '{}'", peer_addr
                        );

                        let _ = socket.close().await;
                    }
                },
                None => {
                    warn!(
                        target: LOG_TARGET,
                        "Peer at address '{}' failed to send valid wire format", peer_addr
                    );
                },
            }
        };

        // This will block (asynchronously) if we have reached the maximum simultaneous connections, creating
        // back-pressure on nodes connecting to this node
        self.bounded_executor.spawn(inbound_fut).await;
    }

    async fn send_event(&mut self, event: ConnectionManagerEvent) {
        log_if_error_fmt!(
            target: LOG_TARGET,
            self.conn_man_notifier.send(event).await,
            "Failed to send connection manager event in listener",
        );
    }

    #[allow(clippy::too_many_arguments)]
    async fn perform_socket_upgrade_procedure(
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        noise_config: NoiseConfig,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        socket: TTransport::Output,
        peer_addr: Multiaddr,
        our_supported_protocols: Vec<ProtocolId>,
        user_agent: String,
        allow_test_addresses: bool,
    ) -> Result<PeerConnection, ConnectionManagerError>
    {
        static CONNECTION_DIRECTION: ConnectionDirection = ConnectionDirection::Inbound;
        debug!(
            target: LOG_TARGET,
            "Starting noise protocol upgrade for peer at address '{}'", peer_addr
        );

        let noise_socket = time::timeout(
            Duration::from_secs(30),
            noise_config.upgrade_socket(socket, CONNECTION_DIRECTION),
        )
        .await
        .map_err(|_| ConnectionManagerError::NoiseProtocolTimeout)??;

        let authenticated_public_key = noise_socket
            .get_remote_public_key()
            .ok_or_else(|| ConnectionManagerError::InvalidStaticPublicKey)?;

        // Check if we know the peer and if it is banned
        let known_peer = common::find_unbanned_peer(&peer_manager, &authenticated_public_key).await?;

        let mut muxer = Yamux::upgrade_connection(noise_socket, CONNECTION_DIRECTION)
            .await
            .map_err(|err| ConnectionManagerError::YamuxUpgradeFailure(err.to_string()))?;

        trace!(
            target: LOG_TARGET,
            "Starting peer identity exchange for peer with public key '{}'",
            authenticated_public_key
        );

        let peer_identity = common::perform_identity_exchange(
            &mut muxer,
            &node_identity,
            CONNECTION_DIRECTION,
            &our_supported_protocols,
            user_agent,
        )
        .await?;

        let features = PeerFeatures::from_bits_truncate(peer_identity.features);
        debug!(
            target: LOG_TARGET,
            "Peer identity exchange succeeded on Inbound connection for peer '{}' (Features = {:?})",
            peer_identity.node_id.to_hex(),
            features
        );
        trace!(target: LOG_TARGET, "{:?}", peer_identity);

        let (peer_node_id, their_supported_protocols) = common::validate_and_add_peer_from_peer_identity(
            &peer_manager,
            known_peer,
            authenticated_public_key,
            peer_identity,
            None,
            allow_test_addresses,
        )
        .await?;

        debug!(
            target: LOG_TARGET,
            "[ThisNode={}] Peer '{}' added to peer list.",
            node_identity.node_id().short_str(),
            peer_node_id.short_str()
        );

        peer_connection::create(
            muxer,
            peer_addr,
            peer_node_id,
            features,
            CONNECTION_DIRECTION,
            conn_man_notifier,
            our_supported_protocols,
            their_supported_protocols,
        )
    }

    async fn listen(&mut self) -> Result<(TTransport::Listener, Multiaddr), ConnectionManagerError> {
        let listener_address = mem::replace(&mut self.config.listener_address, Multiaddr::empty());
        debug!(target: LOG_TARGET, "Attempting to listen on {}", listener_address);
        self.transport
            .listen(listener_address)
            .map_err(|err| ConnectionManagerError::TransportError(err.to_string()))?
            .await
            .map_err(|err| ConnectionManagerError::TransportError(err.to_string()))
    }
}

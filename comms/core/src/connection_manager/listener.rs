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

use std::{
    convert::TryInto,
    future::Future,
    io::{Error, ErrorKind},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use futures::{future, FutureExt};
use log::*;
use tari_shutdown::{oneshot_trigger, oneshot_trigger::OneshotTrigger, ShutdownSignal};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::mpsc,
    time,
};
use tokio_stream::StreamExt;
use tracing::{span, Instrument, Level};

use super::{
    common,
    direction::ConnectionDirection,
    error::ConnectionManagerError,
    peer_connection::{self, PeerConnection},
    ConnectionManagerConfig,
    ConnectionManagerEvent,
};
#[cfg(feature = "metrics")]
use crate::connection_manager::metrics;
use crate::{
    bounded_executor::BoundedExecutor,
    connection_manager::{
        liveness::LivenessSession,
        wire_mode::{WireMode, LIVENESS_WIRE_MODE},
    },
    multiaddr::Multiaddr,
    multiplexing::Yamux,
    noise::NoiseConfig,
    peer_manager::NodeIdentity,
    protocol::ProtocolId,
    transports::Transport,
    utils::multiaddr::multiaddr_to_socketaddr,
    PeerManager,
};

const LOG_TARGET: &str = "comms::connection_manager::listener";

/// Listens on the given transport for peer connections and notifies when a new inbound peer connection is established.
pub struct PeerListener<TTransport> {
    config: ConnectionManagerConfig,
    bind_address: Multiaddr,
    bounded_executor: BoundedExecutor,
    conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
    shutdown_signal: ShutdownSignal,
    transport: TTransport,
    noise_config: NoiseConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    our_supported_protocols: Arc<Vec<ProtocolId>>,
    liveness_session_count: Arc<AtomicUsize>,
    on_listening: OneshotTrigger<Result<Multiaddr, ConnectionManagerError>>,
}

impl<TTransport> PeerListener<TTransport>
where
    TTransport: Transport + Clone + Send + Sync + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    pub fn new(
        config: ConnectionManagerConfig,
        bind_address: Multiaddr,
        transport: TTransport,
        noise_config: NoiseConfig,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        shutdown_signal: ShutdownSignal,
    ) -> Self {
        Self {
            transport,
            bind_address,
            noise_config,
            conn_man_notifier,
            peer_manager,
            node_identity,
            shutdown_signal,
            our_supported_protocols: Arc::new(Vec::new()),
            bounded_executor: BoundedExecutor::new(config.max_simultaneous_inbound_connects),
            liveness_session_count: Arc::new(AtomicUsize::new(config.liveness_max_sessions)),
            config,
            on_listening: oneshot_trigger::channel(),
        }
    }

    /// Returns a future that resolves once the listener has either succeeded (`Ok(bind_addr)`) or failed (`Err(...)`)
    /// in binding the listener socket
    // This returns an impl Future and is not async because we want to exclude &self from the future so that it has a
    // 'static lifetime as well as to flatten the oneshot result for ergonomics
    pub fn on_listening(&self) -> impl Future<Output = Result<Multiaddr, ConnectionManagerError>> + 'static {
        let signal = self.on_listening.to_signal();
        signal.map(|r| r.ok_or(ConnectionManagerError::ListenerOneshotCancelled)?)
    }

    /// Set the supported protocols of this node to send to peers during the peer identity exchange
    pub fn set_supported_protocols(&mut self, our_supported_protocols: Vec<ProtocolId>) -> &mut Self {
        self.our_supported_protocols = Arc::new(our_supported_protocols);
        self
    }

    pub async fn listen(self) -> Result<Multiaddr, ConnectionManagerError> {
        let on_listening = self.on_listening();
        tokio::spawn(self.run());
        on_listening.await
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self.shutdown_signal.clone();

        match self.bind().await {
            Ok((mut inbound, address)) => {
                info!(target: LOG_TARGET, "Listening for peer connections on '{}'", address);

                self.on_listening.broadcast(Ok(address));

                loop {
                    tokio::select! {
                        biased;

                        _ = &mut shutdown_signal => {
                            info!(target: LOG_TARGET, "PeerListener is shutting down because the shutdown signal was triggered");
                            break;
                        },
                        Some(inbound_result) = inbound.next() => {
                            if let Some((socket, peer_addr)) = log_if_error!(target: LOG_TARGET, inbound_result, "Inbound connection failed because '{error}'",) {
                                self.spawn_listen_task(socket, peer_addr).await;
                            }
                        },
                    }
                }
            },
            Err(err) => {
                warn!(target: LOG_TARGET, "PeerListener was unable to start because '{}'", err);
                self.on_listening.broadcast(Err(err));
            },
        }
    }

    async fn read_wire_format(
        socket: &mut TTransport::Output,
        time_to_first_byte: Duration,
    ) -> Result<WireMode, Error> {
        let mut buf = [0u8; 1];
        match time::timeout(time_to_first_byte, socket.read_exact(&mut buf)).await {
            Ok(result) => match result {
                Ok(_) => match buf[0].try_into().ok() {
                    Some(wf) => Ok(wf),
                    None => {
                        warn!(target: LOG_TARGET, "Invalid wire format byte '{}'", buf[0]);
                        Err(ErrorKind::InvalidData.into())
                    },
                },
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Failed to read wire format byte due to error: {}", err
                    );
                    Err(err)
                },
            },
            Err(elapsed) => {
                warn!(
                    target: LOG_TARGET,
                    "Failed to read wire format byte within timeout of {:#?}. {}", time_to_first_byte, elapsed
                );
                Err(elapsed.into())
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
    ) {
        permit.fetch_sub(1, Ordering::SeqCst);
        let liveness = LivenessSession::new(socket);
        debug!(target: LOG_TARGET, "Started liveness session");
        tokio::spawn(async move {
            future::select(liveness.run(), shutdown_signal).await;
            permit.fetch_add(1, Ordering::SeqCst);
        });
    }

    async fn spawn_listen_task(&self, mut socket: TTransport::Output, peer_addr: Multiaddr) {
        let node_identity = self.node_identity.clone();
        let peer_manager = self.peer_manager.clone();
        let conn_man_notifier = self.conn_man_notifier.clone();
        let noise_config = self.noise_config.clone();
        let config = self.config.clone();
        let our_supported_protocols = self.our_supported_protocols.clone();
        let liveness_session_count = self.liveness_session_count.clone();
        let shutdown_signal = self.shutdown_signal.clone();

        let span = span!(Level::TRACE, "connection_mann::listener::inbound_task",);
        let inbound_fut = async move {
            #[cfg(feature = "metrics")]
            metrics::pending_connections(None, ConnectionDirection::Inbound).inc();
            match Self::read_wire_format(&mut socket, config.time_to_first_byte).await {
                Ok(WireMode::Comms(byte)) if byte == config.network_info.network_byte => {
                    let this_node_id_str = node_identity.node_id().short_str();
                    let result = Self::perform_socket_upgrade_procedure(
                        &node_identity,
                        &peer_manager,
                        noise_config.clone(),
                        conn_man_notifier.clone(),
                        socket,
                        peer_addr,
                        our_supported_protocols,
                        &config,
                    )
                    .await;

                    match result {
                        Ok(peer_conn) => {
                            log_if_error!(
                                target: LOG_TARGET,
                                conn_man_notifier
                                    .send(ConnectionManagerEvent::PeerConnected(peer_conn.into()))
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
                Ok(WireMode::Comms(byte)) => {
                    warn!(
                        target: LOG_TARGET,
                        "Peer at address '{}' sent invalid wire format byte. Expected {:x?} got: {:x?} ",
                        peer_addr,
                        config.network_info.network_byte,
                        byte,
                    );
                    let _result = socket.shutdown().await;
                },
                Ok(WireMode::Liveness) => {
                    if config.liveness_self_check_interval.is_some() ||
                        (liveness_session_count.load(Ordering::SeqCst) > 0 &&
                            Self::is_address_in_liveness_cidr_range(&peer_addr, &config.liveness_cidr_allowlist))
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

                        let _result = socket.shutdown().await;
                    }
                },
                Err(err) => {
                    warn!(
                        target: LOG_TARGET,
                        "Peer at address '{}' failed to send its wire format. Expected network byte {:x?} or liveness \
                         byte {:x?} not received. Error: {}",
                        peer_addr,
                        config.network_info.network_byte,
                        LIVENESS_WIRE_MODE,
                        err
                    );
                },
            }

            #[cfg(feature = "metrics")]
            metrics::pending_connections(None, ConnectionDirection::Inbound).dec();
        }
        .instrument(span);

        // This will block (asynchronously) if we have reached the maximum simultaneous connections, creating
        // back-pressure on nodes connecting to this node
        self.bounded_executor.spawn(inbound_fut).await;
    }

    async fn perform_socket_upgrade_procedure(
        node_identity: &NodeIdentity,
        peer_manager: &PeerManager,
        noise_config: NoiseConfig,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        socket: TTransport::Output,
        peer_addr: Multiaddr,
        our_supported_protocols: Arc<Vec<ProtocolId>>,
        config: &ConnectionManagerConfig,
    ) -> Result<PeerConnection, ConnectionManagerError> {
        const CONNECTION_DIRECTION: ConnectionDirection = ConnectionDirection::Inbound;
        debug!(
            target: LOG_TARGET,
            "Starting noise protocol upgrade for peer at address '{}'", peer_addr
        );

        let timer = Instant::now();
        let mut noise_socket = noise_config.upgrade_socket(socket, CONNECTION_DIRECTION).await?;

        let authenticated_public_key = noise_socket
            .get_remote_public_key()
            .ok_or(ConnectionManagerError::InvalidStaticPublicKey)?;

        debug!(
            target: LOG_TARGET,
            "Noise socket upgrade completed in {:.2?} with public key '{}'",
            timer.elapsed(),
            authenticated_public_key
        );

        // Check if we know the peer and if it is banned
        let known_peer = common::find_unbanned_peer(peer_manager, &authenticated_public_key).await?;

        debug!(
            target: LOG_TARGET,
            "Starting peer identity exchange for peer with public key '{}'", authenticated_public_key
        );

        let peer_identity_result = common::perform_identity_exchange(
            &mut noise_socket,
            node_identity,
            &*our_supported_protocols,
            config.network_info.clone(),
        )
        .await;

        let peer_identity =
            common::ban_on_offence(peer_manager, &authenticated_public_key, peer_identity_result).await?;

        let valid_peer_identity_result = common::validate_peer_identity_message(
            &config.peer_validation_config,
            &authenticated_public_key,
            peer_identity,
        );

        let valid_peer_identity =
            common::ban_on_offence(peer_manager, &authenticated_public_key, valid_peer_identity_result).await?;

        let peer = common::create_or_update_peer_from_validated_peer_identity(
            known_peer,
            authenticated_public_key,
            &valid_peer_identity,
        );

        let muxer = Yamux::upgrade_connection(noise_socket, CONNECTION_DIRECTION)
            .map_err(|err| ConnectionManagerError::YamuxUpgradeFailure(err.to_string()))?;

        let conn = peer_connection::create(
            muxer,
            peer_addr,
            peer.node_id.clone(),
            peer.features,
            CONNECTION_DIRECTION,
            conn_man_notifier,
            our_supported_protocols,
            valid_peer_identity.metadata.supported_protocols,
        );

        peer_manager.add_peer(peer).await?;

        Ok(conn)
    }

    async fn bind(&mut self) -> Result<(TTransport::Listener, Multiaddr), ConnectionManagerError> {
        let bind_address = self.bind_address.clone();
        debug!(target: LOG_TARGET, "Attempting to listen on {}", bind_address);
        self.transport
            .listen(&bind_address)
            .await
            .map_err(|err| ConnectionManagerError::ListenerError {
                address: bind_address.to_string(),
                details: err.to_string(),
            })
    }
}

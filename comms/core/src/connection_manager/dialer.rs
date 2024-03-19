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

use std::{collections::HashMap, sync::Arc, time::Instant};

use futures::{
    future,
    future::{BoxFuture, Either, FusedFuture},
    pin_mut,
    stream::FuturesUnordered,
    FutureExt,
};
use log::*;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tari_utilities::hex::Hex;
use tokio::{
    io::{AsyncRead, AsyncWrite, AsyncWriteExt},
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time,
};
use tokio_stream::StreamExt;
use tracing::{self, span, Instrument, Level};

use super::{direction::ConnectionDirection, error::ConnectionManagerError, peer_connection::PeerConnection};
#[cfg(feature = "metrics")]
use crate::connection_manager::metrics;
use crate::{
    backoff::Backoff,
    connection_manager::{
        common,
        common::ValidatedPeerIdentityExchange,
        dial_state::DialState,
        manager::{ConnectionManagerConfig, ConnectionManagerEvent},
        peer_connection,
    },
    multiaddr::Multiaddr,
    multiplexing::Yamux,
    net_address::PeerAddressSource,
    noise::{NoiseConfig, NoiseSocket},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerManager},
    protocol::ProtocolId,
    transports::Transport,
    types::CommsPublicKey,
};

const LOG_TARGET: &str = "comms::connection_manager::dialer";

type DialResult<TSocket> = Result<(NoiseSocket<TSocket>, Multiaddr), ConnectionManagerError>;
type DialFuturesUnordered = FuturesUnordered<
    BoxFuture<
        'static,
        (
            DialState,
            Result<(PeerConnection, ValidatedPeerIdentityExchange), ConnectionManagerError>,
        ),
    >,
>;

#[derive(Debug)]
pub(crate) enum DialerRequest {
    Dial(
        Box<Peer>,
        Option<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>,
    ),
    CancelPendingDial(NodeId),
    NotifyNewInboundConnection(Box<PeerConnection>),
}

/// Responsible for dialing peers on the given transport.
pub struct Dialer<TTransport, TBackoff> {
    config: ConnectionManagerConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    transport: TTransport,
    noise_config: NoiseConfig,
    backoff: Arc<TBackoff>,
    request_rx: mpsc::Receiver<DialerRequest>,
    cancel_signals: HashMap<NodeId, Shutdown>,
    conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
    shutdown: Option<ShutdownSignal>,
    pending_dial_requests: HashMap<NodeId, Vec<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>>,
    our_supported_protocols: Arc<Vec<ProtocolId>>,
}

impl<TTransport, TBackoff> Dialer<TTransport, TBackoff>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TBackoff: Backoff + Send + Sync + 'static,
{
    pub(crate) fn new(
        config: ConnectionManagerConfig,
        node_identity: Arc<NodeIdentity>,
        peer_manager: Arc<PeerManager>,
        transport: TTransport,
        noise_config: NoiseConfig,
        backoff: TBackoff,
        request_rx: mpsc::Receiver<DialerRequest>,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        shutdown: ShutdownSignal,
    ) -> Self {
        Self {
            config,
            node_identity,
            peer_manager,
            transport,
            noise_config,
            backoff: Arc::new(backoff),
            request_rx,
            cancel_signals: Default::default(),
            conn_man_notifier,
            shutdown: Some(shutdown),
            pending_dial_requests: Default::default(),
            our_supported_protocols: Arc::new(Vec::new()),
        }
    }

    /// Set the supported protocols of this node to send to peers during the peer identity exchange
    pub fn set_supported_protocols(&mut self, our_supported_protocols: Vec<ProtocolId>) -> &mut Self {
        self.our_supported_protocols = Arc::new(our_supported_protocols);
        self
    }

    pub fn spawn(self) -> JoinHandle<()> {
        tokio::spawn(self.run())
    }

    pub async fn run(mut self) {
        let mut pending_dials = FuturesUnordered::new();
        let mut shutdown = self
            .shutdown
            .take()
            .expect("Establisher initialized without a shutdown");
        debug!(target: LOG_TARGET, "Connection dialer started");
        loop {
            tokio::select! {
                // Biased ordering is used because we already have the futures polled here in a fair order, and so wish to
                // forgo the minor cost of the random ordering
                biased;

                _ = &mut shutdown => {
                    info!(target: LOG_TARGET, "Connection dialer shutting down because the shutdown signal was received");
                    self.cancel_all_dials();
                    break;
                }
                Some((dial_state, dial_result)) = pending_dials.next() => {
                    self.handle_dial_result(dial_state, dial_result).await;
                }
                Some(request) = self.request_rx.recv() => self.handle_request(&mut pending_dials, request),
            }
        }
    }

    fn handle_request(&mut self, pending_dials: &mut DialFuturesUnordered, request: DialerRequest) {
        use DialerRequest::{CancelPendingDial, Dial, NotifyNewInboundConnection};
        debug!(target: LOG_TARGET, "Connection dialer got request: {:?}", request);
        match request {
            Dial(peer, reply_tx) => {
                self.handle_dial_peer_request(pending_dials, peer, reply_tx);
            },
            CancelPendingDial(peer_id) => {
                self.cancel_dial(&peer_id);
            },

            NotifyNewInboundConnection(conn) => {
                if conn.is_connected() {
                    self.resolve_pending_dials(*conn);
                }
            },
        }
    }

    fn cancel_dial(&mut self, peer_id: &NodeId) {
        if let Some(mut s) = self.cancel_signals.remove(peer_id) {
            s.trigger();
        }
    }

    fn resolve_pending_dials(&mut self, conn: PeerConnection) {
        let peer = conn.peer_node_id().clone();
        self.reply_to_pending_requests(&peer, Ok(conn));
        self.cancel_dial(&peer);
    }

    fn is_pending_dial(&self, node_id: &NodeId) -> bool {
        self.cancel_signals.contains_key(node_id)
    }

    fn cancel_all_dials(&mut self) {
        debug!(
            target: LOG_TARGET,
            "Cancelling {} pending dial(s)",
            self.cancel_signals.len()
        );
        self.cancel_signals.drain().for_each(|(_, mut signal)| {
            signal.trigger();
        })
    }

    async fn handle_dial_result(
        &mut self,
        mut dial_state: DialState,
        dial_result: Result<(PeerConnection, ValidatedPeerIdentityExchange), ConnectionManagerError>,
    ) {
        let node_id = dial_state.peer().node_id.clone();
        #[cfg(feature = "metrics")]
        metrics::pending_connections(Some(&node_id), ConnectionDirection::Outbound).inc();

        match dial_result {
            Ok((conn, peer_identity)) => {
                // try save the peer back to the peer manager
                let peer = dial_state.peer_mut();
                peer.update_addresses(&peer_identity.claim.addresses, &PeerAddressSource::FromPeerConnection {
                    peer_identity_claim: peer_identity.claim.clone(),
                });
                peer.supported_protocols = peer_identity.metadata.supported_protocols;
                peer.user_agent = peer_identity.metadata.user_agent;

                debug!(target: LOG_TARGET, "Successfully dialed peer '{}'", node_id);
                self.notify_connection_manager(ConnectionManagerEvent::PeerConnected(conn.clone().into()))
                    .await;

                if dial_state.send_reply(Ok(conn.clone())).is_err() {
                    warn!(
                        target: LOG_TARGET,
                        "Reply oneshot was closed before dial response for peer '{}' was sent", node_id
                    );
                }

                self.reply_to_pending_requests(&node_id, Ok(conn));
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to dial peer '{}' because '{:?}'", node_id, err
                );
                self.notify_connection_manager(ConnectionManagerEvent::PeerConnectFailed(node_id.clone(), err.clone()))
                    .await;

                if dial_state.send_reply(Err(err.clone())).is_err() {
                    warn!(
                        target: LOG_TARGET,
                        "Reply oneshot was closed before dial response for peer '{}' was sent", node_id
                    );
                }
                self.reply_to_pending_requests(&node_id, Err(err));
            },
        }

        let _ = self
            .peer_manager
            .add_peer(dial_state.peer().clone())
            .await
            .map_err(|e| {
                error!(target: LOG_TARGET, "Could not update peer data: {}", e);
                let _ = dial_state
                    .send_reply(Err(ConnectionManagerError::PeerManagerError(e)))
                    .map_err(|e| error!(target: LOG_TARGET, "Could not send reply to dial request: {:?}", e));
            });

        #[cfg(feature = "metrics")]
        metrics::pending_connections(Some(&node_id), ConnectionDirection::Outbound).dec();

        self.cancel_dial(&node_id);
    }

    pub async fn notify_connection_manager(&mut self, event: ConnectionManagerEvent) {
        log_if_error!(
            target: LOG_TARGET,
            self.conn_man_notifier.send(event).await,
            "Failed to publish event because '{error}'",
        );
    }

    fn reply_to_pending_requests(
        &mut self,
        peer_node_id: &NodeId,
        result: Result<PeerConnection, ConnectionManagerError>,
    ) {
        self.pending_dial_requests
            .remove(peer_node_id)
            .and_then(|reply_oneshots| {
                reply_oneshots.into_iter().for_each(|tx| {
                    log_if_error_fmt!(
                        target: LOG_TARGET,
                        tx.send(result.clone()),
                        "Failed to send dial result for peer '{}'",
                        peer_node_id.short_str()
                    );
                });
                Option::<()>::None
            });
    }

    fn handle_dial_peer_request(
        &mut self,
        pending_dials: &mut DialFuturesUnordered,
        peer: Box<Peer>,
        reply_tx: Option<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>,
    ) {
        if self.is_pending_dial(&peer.node_id) {
            debug!(
                target: LOG_TARGET,
                "Dial to peer '{}' already pending - adding to wait queue", peer.node_id
            );
            if let Some(reply_tx) = reply_tx {
                let entry = self.pending_dial_requests.entry(peer.node_id).or_default();
                entry.push(reply_tx);
            }
            return;
        }

        let transport = self.transport.clone();
        let dial_cancel = Shutdown::new();
        let cancel_signal = dial_cancel.to_signal();
        self.cancel_signals.insert(peer.node_id.clone(), dial_cancel);

        let backoff = Arc::clone(&self.backoff);

        let dial_state = DialState::new(peer, reply_tx, cancel_signal);
        let node_identity = Arc::clone(&self.node_identity);
        let conn_man_notifier = self.conn_man_notifier.clone();
        let supported_protocols = self.our_supported_protocols.clone();
        let noise_config = self.noise_config.clone();
        let config = self.config.clone();
        let peer_manager = self.peer_manager.clone();

        let span = span!(Level::TRACE, "handle_dial_peer_request_inner1");
        let dial_fut = async move {
            let (dial_state, dial_result) =
                Self::dial_peer_with_retry(dial_state, noise_config, transport, backoff, &config).await;

            let cancel_signal = dial_state.get_cancel_signal();

            match dial_result {
                Ok((socket, addr)) => {
                    let authenticated_public_key =
                        match Self::check_authenticated_public_key(&socket, &dial_state.peer().public_key) {
                            Ok(pk) => pk,
                            Err(err) => {
                                let mut dial_state = dial_state;
                                dial_state
                                    .peer_mut()
                                    .addresses
                                    .mark_failed_connection_attempt(&addr, err.to_string());
                                return (dial_state, Err(err));
                            },
                        };

                    let result = Self::perform_socket_upgrade_procedure(
                        &peer_manager,
                        &node_identity,
                        socket,
                        addr.clone(),
                        authenticated_public_key,
                        conn_man_notifier,
                        supported_protocols,
                        &config,
                        cancel_signal,
                    )
                    .await;

                    if let Err(err) = &result {
                        let mut dial_state = dial_state;
                        dial_state
                            .peer_mut()
                            .addresses
                            .mark_failed_connection_attempt(&addr, err.to_string());
                        (dial_state, result)
                    } else {
                        (dial_state, result)
                    }
                },
                Err(err) => (dial_state, Err(err)),
            }
        }
        .instrument(span);

        pending_dials.push(dial_fut.boxed());
    }

    fn check_authenticated_public_key(
        socket: &NoiseSocket<TTransport::Output>,
        expected_public_key: &CommsPublicKey,
    ) -> Result<CommsPublicKey, ConnectionManagerError> {
        let authenticated_public_key = socket
            .get_remote_public_key()
            .ok_or(ConnectionManagerError::InvalidStaticPublicKey)?;

        if &authenticated_public_key != expected_public_key {
            return Err(ConnectionManagerError::DialedPublicKeyMismatch {
                authenticated_pk: authenticated_public_key.to_hex(),
                expected_pk: expected_public_key.to_hex(),
            });
        }

        Ok(authenticated_public_key)
    }

    async fn perform_socket_upgrade_procedure(
        peer_manager: &PeerManager,
        node_identity: &NodeIdentity,
        mut socket: NoiseSocket<TTransport::Output>,
        dialed_addr: Multiaddr,
        authenticated_public_key: CommsPublicKey,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        our_supported_protocols: Arc<Vec<ProtocolId>>,
        config: &ConnectionManagerConfig,
        cancel_signal: ShutdownSignal,
    ) -> Result<(PeerConnection, ValidatedPeerIdentityExchange), ConnectionManagerError> {
        static CONNECTION_DIRECTION: ConnectionDirection = ConnectionDirection::Outbound;
        debug!(
            target: LOG_TARGET,
            "Starting peer identity exchange for peer with public key '{}'", authenticated_public_key
        );

        let peer_identity_result = common::perform_identity_exchange(
            &mut socket,
            node_identity,
            &*our_supported_protocols,
            config.network_info.clone(),
        )
        .await;

        let peer_identity =
            common::ban_on_offence(peer_manager, &authenticated_public_key, peer_identity_result).await?;

        if cancel_signal.is_terminated() {
            return Err(ConnectionManagerError::DialCancelled);
        }

        let peer_identity_result = common::validate_peer_identity_message(
            &config.peer_validation_config,
            &authenticated_public_key,
            peer_identity,
        );
        let peer_identity =
            common::ban_on_offence(peer_manager, &authenticated_public_key, peer_identity_result).await?;

        if cancel_signal.is_terminated() {
            return Err(ConnectionManagerError::DialCancelled);
        }

        let muxer = Yamux::upgrade_connection(socket, CONNECTION_DIRECTION)
            .map_err(|err| ConnectionManagerError::YamuxUpgradeFailure(err.to_string()))?;

        if cancel_signal.is_terminated() {
            muxer.get_yamux_control().close().await?;
            return Err(ConnectionManagerError::DialCancelled);
        }

        let peer_connection = peer_connection::create(
            muxer,
            dialed_addr,
            NodeId::from_public_key(&authenticated_public_key),
            peer_identity.claim.features,
            CONNECTION_DIRECTION,
            conn_man_notifier,
            our_supported_protocols,
            peer_identity.metadata.supported_protocols.clone(),
        );

        Ok((peer_connection, peer_identity))
    }

    async fn dial_peer_with_retry(
        dial_state: DialState,
        noise_config: NoiseConfig,
        transport: TTransport,
        backoff: Arc<TBackoff>,
        config: &ConnectionManagerConfig,
    ) -> (DialState, DialResult<TTransport::Output>) {
        // Container for dial
        let mut dial_state = Some(dial_state);
        let mut transport = Some(transport);

        loop {
            let mut current_state = dial_state.take().expect("dial_state must own current dial state");
            current_state.inc_attempts();
            let current_transport = transport.take().expect("transport must own current dial state");
            let backoff_duration = backoff.calculate_backoff(current_state.num_attempts());
            debug!(
                target: LOG_TARGET,
                "[Attempt {}] Will attempt connection to peer '{}' in {} second(s)",
                current_state.num_attempts(),
                current_state.peer().node_id.short_str(),
                backoff_duration.as_secs()
            );
            let delay = time::sleep(backoff_duration).fuse();
            let cancel_signal = current_state.get_cancel_signal();
            tokio::select! {
                _ = delay => {
                    debug!(target: LOG_TARGET, "[Attempt {}] Connecting to peer '{}'", current_state.num_attempts(), current_state.peer().node_id.short_str());
                    match Self::dial_peer(current_state, &noise_config, &current_transport, config.network_info.network_byte).await {
                        (state, Ok((socket, addr))) => {
                            debug!(target: LOG_TARGET, "Dial succeeded for peer '{}' after {} attempt(s)", state.peer().node_id.short_str(), state.num_attempts());
                            break (state, Ok((socket, addr)));
                        },
                        // Inflight dial was cancelled
                        (state, Err(ConnectionManagerError::DialCancelled)) => break (state, Err(ConnectionManagerError::DialCancelled)),
                        (state, Err(err)) => {
                            debug!(target: LOG_TARGET, "Failed to dial peer {} | Attempt {} | Error: {}", state.peer().node_id.short_str(), state.num_attempts(), err);
                            if state.num_attempts() >= config.max_dial_attempts {
                                break (state, Err(ConnectionManagerError::ConnectFailedMaximumAttemptsReached));
                            }

                            // Put the dial state and transport back for the retry
                            dial_state = Some(state);
                            transport = Some(current_transport);
                        }
                    }
                },
                // Delayed dial was cancelled
                _ = cancel_signal => {
                    warn!(target: LOG_TARGET, "[Attempt {}] Connection attempt cancelled for peer '{}'", current_state.num_attempts(), current_state.peer().node_id.short_str());
                    break (current_state, Err(ConnectionManagerError::DialCancelled));
                }
            }
        }
    }

    /// Attempts to dial a peer sequentially on all addresses.
    /// Returns ownership of the given `DialState` and a success or failure result for the dial,
    /// or None if the dial was cancelled inflight
    async fn dial_peer(
        mut dial_state: DialState,
        noise_config: &NoiseConfig,
        transport: &TTransport,
        network_byte: u8,
    ) -> (
        DialState,
        Result<(NoiseSocket<TTransport::Output>, Multiaddr), ConnectionManagerError>,
    ) {
        let addresses = dial_state.peer().addresses.clone().into_vec();
        let cancel_signal = dial_state.get_cancel_signal();
        for address in addresses {
            debug!(
                target: LOG_TARGET,
                "Attempting address '{}' for peer '{}'",
                address,
                dial_state.peer().node_id.short_str()
            );

            let moved_address = address.clone();
            let node_id = dial_state.peer().node_id.clone();
            let dial_fut = async move {
                let mut timer = Instant::now();
                let mut socket =
                    transport
                        .dial(&moved_address)
                        .await
                        .map_err(|err| ConnectionManagerError::TransportError {
                            address: moved_address.to_string(),
                            details: err.to_string(),
                        })?;
                debug!(
                    target: LOG_TARGET,
                    "Socket established on '{}'. Performing noise upgrade protocol", moved_address
                );
                let initial_dial_time = timer.elapsed();

                debug!(
                    "Dialed peer: {} on address: {} on tcp after: {}",
                    node_id.short_str(),
                    moved_address,
                    timer.elapsed().as_millis()
                );
                timer = Instant::now();

                socket
                    .write(&[network_byte])
                    .await
                    .map_err(|_| ConnectionManagerError::WireFormatSendFailed)?;

                let noise_socket = noise_config
                    .upgrade_socket(socket, ConnectionDirection::Outbound)
                    .await?;

                let noise_upgrade_time = timer.elapsed();
                debug!(
                    "Dial - upgraded noise: {} on address: {} on tcp after: {}",
                    node_id.short_str(),
                    moved_address,
                    timer.elapsed().as_millis()
                );

                Result::<_, ConnectionManagerError>::Ok((initial_dial_time, noise_upgrade_time, noise_socket))
            };

            pin_mut!(dial_fut);
            let either = future::select(dial_fut, cancel_signal.clone()).await;
            match either {
                Either::Left((Ok((initial_dial_time, noise_upgrade_time, noise_socket)), _)) => {
                    dial_state.peer_mut().addresses.mark_last_seen_now(&address);
                    dial_state.peer_mut().addresses.update_address_stats(&address, |addr| {
                        // Initial dial time can be much slower due to tor discovery.
                        addr.update_initial_dial_time(initial_dial_time);
                        addr.update_latency(noise_upgrade_time);
                    });
                    return (dial_state, Ok((noise_socket, address.clone())));
                },
                Either::Left((Err(err), _)) => {
                    debug!(
                        target: LOG_TARGET,
                        "(Attempt {}) Dial failed on address '{}' for peer '{}' because '{}'",
                        dial_state.num_attempts(),
                        address,
                        dial_state.peer().node_id.short_str(),
                        err,
                    );

                    dial_state
                        .peer_mut()
                        .addresses
                        .mark_failed_connection_attempt(&address, err.to_string());
                    // Try the next address
                    continue;
                },
                // Canceled
                Either::Right(_) => {
                    debug!(
                        target: LOG_TARGET,
                        "Dial for peer '{}' cancelled",
                        dial_state.peer().node_id.short_str()
                    );
                    return (dial_state, Err(ConnectionManagerError::DialCancelled));
                },
            }
        }

        (dial_state, Err(ConnectionManagerError::DialConnectFailedAllAddresses))
    }
}

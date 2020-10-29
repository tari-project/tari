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

use super::{error::ConnectionManagerError, peer_connection::PeerConnection, types::ConnectionDirection};
use crate::{
    backoff::Backoff,
    connection_manager::{
        common,
        dial_state::DialState,
        manager::{ConnectionManagerConfig, ConnectionManagerEvent},
        peer_connection,
        wire_mode::WireMode,
    },
    multiaddr::Multiaddr,
    multiplexing::Yamux,
    noise::{NoiseConfig, NoiseSocket},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerFeatures, PeerManager},
    protocol::ProtocolId,
    transports::Transport,
    types::CommsPublicKey,
};
use futures::{
    channel::{mpsc, oneshot},
    future,
    future::{BoxFuture, Either, FusedFuture},
    pin_mut,
    stream::{Fuse, FuturesUnordered},
    AsyncRead,
    AsyncWrite,
    AsyncWriteExt,
    FutureExt,
    SinkExt,
    StreamExt,
};
use log::*;
use std::{collections::HashMap, sync::Arc, time::Duration};
use tari_crypto::tari_utilities::hex::Hex;
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::time;

const LOG_TARGET: &str = "comms::connection_manager::dialer";

type DialResult<TSocket> = Result<(NoiseSocket<TSocket>, Multiaddr), ConnectionManagerError>;
type DialFuturesUnordered =
    FuturesUnordered<BoxFuture<'static, (DialState, Result<PeerConnection, ConnectionManagerError>)>>;

#[derive(Debug)]
pub(crate) enum DialerRequest {
    Dial(
        Box<Peer>,
        oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
    ),
    CancelPendingDial(NodeId),
}

pub struct Dialer<TTransport, TBackoff> {
    config: ConnectionManagerConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    transport: TTransport,
    noise_config: NoiseConfig,
    backoff: Arc<TBackoff>,
    request_rx: Fuse<mpsc::Receiver<DialerRequest>>,
    cancel_signals: HashMap<NodeId, Shutdown>,
    conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
    shutdown: Option<ShutdownSignal>,
    pending_dial_requests: HashMap<NodeId, Vec<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>>,
    our_supported_protocols: Vec<ProtocolId>,
}

impl<TTransport, TBackoff> Dialer<TTransport, TBackoff>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TBackoff: Backoff + Send + Sync + 'static,
{
    #[allow(clippy::too_many_arguments)]
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
    ) -> Self
    {
        Self {
            config,
            node_identity,
            peer_manager,
            transport,
            noise_config,
            backoff: Arc::new(backoff),
            request_rx: request_rx.fuse(),
            cancel_signals: Default::default(),
            conn_man_notifier,
            shutdown: Some(shutdown),
            pending_dial_requests: Default::default(),
            our_supported_protocols: Vec::new(),
        }
    }

    /// Set the supported protocols of this node to send to peers during the peer identity exchange
    pub fn set_supported_protocols(&mut self, our_supported_protocols: Vec<ProtocolId>) -> &mut Self {
        self.our_supported_protocols = our_supported_protocols;
        self
    }

    pub async fn run(mut self) {
        let mut pending_dials = FuturesUnordered::new();
        let mut shutdown = self
            .shutdown
            .take()
            .expect("Establisher initialized without a shutdown");
        debug!(target: LOG_TARGET, "Connection dialer started");
        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => self.handle_request(&mut pending_dials, request),
                (dial_state, dial_result) = pending_dials.select_next_some() => {
                    self.handle_dial_result(dial_state, dial_result).await;
                }
                _ = shutdown => {
                    info!(target: LOG_TARGET, "Connection dialer shutting down because the shutdown signal was received");
                    self.cancel_all_dials();
                    break;
                }
            }
        }
    }

    fn handle_request(&mut self, pending_dials: &mut DialFuturesUnordered, request: DialerRequest) {
        use DialerRequest::*;
        trace!(target: LOG_TARGET, "Connection dialer got request: {:?}", request);
        match request {
            Dial(peer, reply_tx) => {
                self.handle_dial_peer_request(pending_dials, peer, reply_tx);
            },
            CancelPendingDial(peer_id) => {
                if let Some(mut s) = self.cancel_signals.remove(&peer_id) {
                    let _ = s.trigger();
                }
            },
        }
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
            log_if_error_fmt!(
                level: warn,
                target: LOG_TARGET,
                signal.trigger(),
                "Shutdown trigger failed",
            );
        })
    }

    async fn handle_dial_result(
        &mut self,
        dial_state: DialState,
        dial_result: Result<PeerConnection, ConnectionManagerError>,
    )
    {
        let DialState { peer, reply_tx, .. } = dial_state;

        let node_id = peer.node_id.clone();
        let peer_id_short_str = peer.node_id.short_str();

        let removed = self.cancel_signals.remove(&node_id);
        drop(removed);

        match &dial_result {
            Ok(conn) => {
                debug!(target: LOG_TARGET, "Successfully dialed peer '{}'", peer_id_short_str);
                self.notify_connection_manager(ConnectionManagerEvent::PeerConnected(conn.clone()))
                    .await
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to dial peer '{}' because '{:?}'", peer_id_short_str, err
                );
                self.notify_connection_manager(ConnectionManagerEvent::PeerConnectFailed(
                    Box::new(node_id.clone()),
                    err.clone(),
                ))
                .await
            },
        }

        if self.pending_dial_requests.contains_key(&node_id) {
            self.reply_to_pending_requests(&node_id, dial_result.clone());
        }

        let _ = reply_tx.send(dial_result);
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
    )
    {
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
        reply_tx: oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
    )
    {
        if self.is_pending_dial(&peer.node_id) {
            let entry = self.pending_dial_requests.entry(peer.node_id).or_insert_with(Vec::new);
            entry.push(reply_tx);
            return;
        }

        let transport = self.transport.clone();
        let dial_cancel = Shutdown::new();
        let cancel_signal = dial_cancel.to_signal();
        self.cancel_signals.insert(peer.node_id.clone(), dial_cancel);

        let backoff = Arc::clone(&self.backoff);
        let max_attempts = self.config.max_dial_attempts;

        let dial_state = DialState::new(peer, reply_tx, cancel_signal);
        let node_identity = Arc::clone(&self.node_identity);
        let peer_manager = self.peer_manager.clone();
        let conn_man_notifier = self.conn_man_notifier.clone();
        let supported_protocols = self.our_supported_protocols.clone();
        let user_agent = self.config.user_agent.clone();
        let noise_config = self.noise_config.clone();
        let allow_test_addresses = self.config.allow_test_addresses;

        let dial_fut = async move {
            let (dial_state, dial_result) =
                Self::dial_peer_with_retry(dial_state, noise_config, transport, backoff, max_attempts).await;

            let cancel_signal = dial_state.get_cancel_signal();

            match dial_result {
                Ok((socket, addr)) => {
                    let authenticated_public_key =
                        match Self::check_authenticated_public_key(&socket, &dial_state.peer.public_key) {
                            Ok(pk) => pk,
                            Err(err) => {
                                return (dial_state, Err(err));
                            },
                        };

                    let result = Self::perform_socket_upgrade_procedure(
                        peer_manager,
                        node_identity,
                        socket,
                        addr,
                        authenticated_public_key,
                        conn_man_notifier,
                        supported_protocols,
                        user_agent,
                        allow_test_addresses,
                        cancel_signal,
                    )
                    .await;

                    (dial_state, result)
                },
                Err(err) => (dial_state, Err(err)),
            }
        };

        pending_dials.push(dial_fut.boxed());
    }

    fn check_authenticated_public_key(
        socket: &NoiseSocket<TTransport::Output>,
        expected_public_key: &CommsPublicKey,
    ) -> Result<CommsPublicKey, ConnectionManagerError>
    {
        let authenticated_public_key = socket
            .get_remote_public_key()
            .ok_or_else(|| ConnectionManagerError::InvalidStaticPublicKey)?;

        if &authenticated_public_key != expected_public_key {
            return Err(ConnectionManagerError::DialedPublicKeyMismatch);
        }

        Ok(authenticated_public_key)
    }

    #[allow(clippy::too_many_arguments)]
    async fn perform_socket_upgrade_procedure(
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        socket: NoiseSocket<TTransport::Output>,
        dialed_addr: Multiaddr,
        authenticated_public_key: CommsPublicKey,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        our_supported_protocols: Vec<ProtocolId>,
        user_agent: String,
        allow_test_addresses: bool,
        cancel_signal: ShutdownSignal,
    ) -> Result<PeerConnection, ConnectionManagerError>
    {
        static CONNECTION_DIRECTION: ConnectionDirection = ConnectionDirection::Outbound;

        let mut muxer = Yamux::upgrade_connection(socket, CONNECTION_DIRECTION)
            .await
            .map_err(|err| ConnectionManagerError::YamuxUpgradeFailure(err.to_string()))?;

        debug!(
            target: LOG_TARGET,
            "Starting peer identity exchange for peer with public key '{}'", authenticated_public_key
        );
        if cancel_signal.is_terminated() {
            return Err(ConnectionManagerError::DialCancelled);
        }

        let peer_identity = common::perform_identity_exchange(
            &mut muxer,
            &node_identity,
            CONNECTION_DIRECTION,
            &our_supported_protocols,
            user_agent,
        )
        .await?;
        if cancel_signal.is_terminated() {
            muxer.get_yamux_control().close().await?;
            return Err(ConnectionManagerError::DialCancelled);
        }

        let features = PeerFeatures::from_bits_truncate(peer_identity.features);
        trace!(
            target: LOG_TARGET,
            "Peer identity exchange succeeded on Outbound connection for peer '{}' (Features = {:?})",
            peer_identity.node_id.to_hex(),
            features
        );
        trace!(target: LOG_TARGET, "{:?}", peer_identity);

        // Check if we know the peer and if it is banned
        let known_peer = common::find_unbanned_peer(&peer_manager, &authenticated_public_key).await?;

        let peer_node_id = common::validate_and_add_peer_from_peer_identity(
            &peer_manager,
            known_peer,
            authenticated_public_key,
            peer_identity,
            Some(&dialed_addr),
            allow_test_addresses,
        )
        .await?;

        if cancel_signal.is_terminated() {
            muxer.get_yamux_control().close().await?;
            return Err(ConnectionManagerError::DialCancelled);
        }

        debug!(
            target: LOG_TARGET,
            "[ThisNode={}] Peer '{}' added to peer list.",
            node_identity.node_id().short_str(),
            peer_node_id.short_str()
        );

        peer_connection::create(
            muxer,
            dialed_addr,
            peer_node_id,
            features,
            CONNECTION_DIRECTION,
            conn_man_notifier,
            our_supported_protocols,
        )
    }

    async fn dial_peer_with_retry(
        dial_state: DialState,
        noise_config: NoiseConfig,
        transport: TTransport,
        backoff: Arc<TBackoff>,
        max_attempts: usize,
    ) -> (DialState, DialResult<TTransport::Output>)
    {
        // Container for dial state
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
                current_state.peer.node_id.short_str(),
                backoff_duration.as_secs()
            );
            let mut delay = time::delay_for(backoff_duration).fuse();
            let mut cancel_signal = current_state.get_cancel_signal();
            futures::select! {
                _ = delay => {
                    debug!(target: LOG_TARGET, "[Attempt {}] Connecting to peer '{}'", current_state.num_attempts(), current_state.peer.node_id.short_str());
                    match Self::dial_peer(current_state, &noise_config, &current_transport).await {
                        (state, Ok((socket, addr))) => {
                            debug!(target: LOG_TARGET, "Dial succeeded for peer '{}' after {} attempt(s)", state.peer.node_id.short_str(), state.num_attempts());
                            break (state, Ok((socket, addr)));
                        },
                        // Inflight dial was cancelled
                        (state, Err(ConnectionManagerError::DialCancelled)) => break (state, Err(ConnectionManagerError::DialCancelled)),
                        (mut state, Err(err)) => {
                            if state.num_attempts() >= max_attempts {
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
                    debug!(target: LOG_TARGET, "[Attempt {}] Connection attempt cancelled for peer '{}'", current_state.num_attempts(), current_state.peer.node_id.short_str());
                    break (current_state, Err(ConnectionManagerError::DialCancelled));
                }
            }
        }
    }

    /// Attempts to dial a peer sequentially on all addresses.
    /// Returns ownership of the given `DialState` and a success or failure result for the dial,
    /// or None if the dial was cancelled inflight
    async fn dial_peer(
        dial_state: DialState,
        noise_config: &NoiseConfig,
        transport: &TTransport,
    ) -> (
        DialState,
        Result<(NoiseSocket<TTransport::Output>, Multiaddr), ConnectionManagerError>,
    )
    {
        let mut addr_iter = dial_state.peer.addresses.address_iter();
        let cancel_signal = dial_state.get_cancel_signal();
        loop {
            let result = match addr_iter.next() {
                Some(address) => {
                    debug!(
                        target: LOG_TARGET,
                        "Attempting address '{}' for peer '{}'",
                        address,
                        dial_state.peer.node_id.short_str()
                    );

                    let dial_fut = async move {
                        let mut socket = transport
                            .dial(address.clone())
                            .map_err(|err| ConnectionManagerError::TransportError(err.to_string()))?
                            .await
                            .map_err(|err| ConnectionManagerError::TransportError(err.to_string()))?;
                        debug!(
                            target: LOG_TARGET,
                            "Socket established on '{}'. Performing noise upgrade protocol", address
                        );

                        socket
                            .write(&[WireMode::Comms as u8])
                            .await
                            .map_err(|_| ConnectionManagerError::WireFormatSendFailed)?;

                        let noise_socket = time::timeout(
                            Duration::from_secs(30),
                            noise_config.upgrade_socket(socket, ConnectionDirection::Outbound),
                        )
                        .await
                        .map_err(|_| ConnectionManagerError::NoiseProtocolTimeout)??;
                        Result::<_, ConnectionManagerError>::Ok(noise_socket)
                    };

                    pin_mut!(dial_fut);
                    let either = future::select(dial_fut, cancel_signal.clone()).await;
                    match either {
                        Either::Left((Ok(noise_socket), _)) => Ok((noise_socket, address.clone())),
                        Either::Left((Err(err), _)) => {
                            debug!(
                                target: LOG_TARGET,
                                "(Attempt {}) Dial failed on address '{}' for peer '{}' because '{}'",
                                dial_state.num_attempts(),
                                address,
                                dial_state.peer.node_id.short_str(),
                                err,
                            );
                            // Try the next address
                            continue;
                        },
                        Either::Right((cancel_result, _)) => {
                            debug!(
                                target: LOG_TARGET,
                                "Dial for peer '{}' cancelled",
                                dial_state.peer.node_id.short_str()
                            );
                            log_if_error!(
                                level: warn,
                                target: LOG_TARGET,
                                cancel_result,
                                "Cancel channel error during dial: {}",
                            );
                            Err(ConnectionManagerError::DialCancelled)
                        },
                    }
                },
                // No more addresses to try - returning failure
                None => Err(ConnectionManagerError::DialConnectFailedAllAddresses),
            };

            drop(addr_iter);

            break (dial_state, result);
        }
    }
}

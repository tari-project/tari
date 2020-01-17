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

use super::{error::ConnectionManagerError, peer_connection::PeerConnection};
use crate::{
    backoff::Backoff,
    connection::ConnectionDirection,
    connection_manager::{
        dial_state::DialState,
        manager::{ConnectionManagerConfig, ConnectionManagerEvent},
        peer_connection::create_peer_connection,
        utils::short_str,
    },
    multiaddr::Multiaddr,
    noise::{NoiseConfig, NoiseSocket},
    peer_manager::{Peer, PeerId},
    transports::Transport,
};
use futures::{
    channel::{mpsc, oneshot},
    future,
    future::{BoxFuture, Either},
    pin_mut,
    stream::{Fuse, FuturesUnordered},
    AsyncRead,
    AsyncWrite,
    FutureExt,
    SinkExt,
    StreamExt,
};
use log::*;
use std::{collections::HashMap, sync::Arc};
use tari_shutdown::{Shutdown, ShutdownSignal};
use tokio::{runtime, time};

const LOG_TARGET: &str = "comms::connection_manager::establisher";

type DialResult<TSock> = Result<(TSock, Multiaddr), ConnectionManagerError>;

#[derive(Debug)]
pub enum DialerRequest {
    Dial(
        Box<Peer>,
        oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
    ),
    CancelPending(PeerId),
}

pub struct Dialer<TTransport, TBackoff> {
    executor: runtime::Handle,
    config: ConnectionManagerConfig,
    transport: TTransport,
    noise_config: NoiseConfig,
    backoff: Arc<TBackoff>,
    request_rx: Fuse<mpsc::Receiver<DialerRequest>>,
    cancel_signals: HashMap<PeerId, Shutdown>,
    conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
    shutdown: Option<ShutdownSignal>,
    pending_dial_requests: HashMap<PeerId, Vec<oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>>>,
}

impl<TTransport, TBackoff> Dialer<TTransport, TBackoff>
where
    TTransport: Transport + Unpin + Send + Sync + Clone + 'static,
    TTransport::Output: AsyncRead + AsyncWrite + Send + Sync + Unpin + 'static,
    TBackoff: Backoff + Send + Sync + 'static,
{
    pub fn new(
        executor: runtime::Handle,
        config: ConnectionManagerConfig,
        transport: TTransport,
        noise_config: NoiseConfig,
        backoff: Arc<TBackoff>,
        request_rx: mpsc::Receiver<DialerRequest>,
        conn_man_notifier: mpsc::Sender<ConnectionManagerEvent>,
        shutdown: ShutdownSignal,
    ) -> Self
    {
        Self {
            executor,
            config,
            transport,
            noise_config,
            backoff,
            request_rx: request_rx.fuse(),
            cancel_signals: Default::default(),
            conn_man_notifier,
            shutdown: Some(shutdown),
            pending_dial_requests: Default::default(),
        }
    }

    pub async fn run(mut self) {
        let mut pending_dials = FuturesUnordered::new();
        let mut shutdown = self
            .shutdown
            .take()
            .expect("Establisher initialized without a shutdown");
        debug!(target: LOG_TARGET, "Connection establisher started");
        loop {
            futures::select! {
                request = self.request_rx.select_next_some() => self.handle_request(&mut pending_dials, request),
                dial = pending_dials.select_next_some() => {
                    if let Some((dial_state, dial_result)) = dial {
                        self.handle_dial_result(dial_state, dial_result).await
                    }
                }
                _ = shutdown => {
                    info!(target: LOG_TARGET, "Connection establisher shutting down because the shutdown signal was received");
                    self.cancel_all_dials();
                    break;
                }
            }
        }
    }

    fn handle_request(
        &mut self,
        pending_dials: &mut FuturesUnordered<
            BoxFuture<'static, Option<(DialState, DialResult<NoiseSocket<TTransport::Output>>)>>,
        >,
        request: DialerRequest,
    )
    {
        use DialerRequest::*;
        match request {
            Dial(peer, reply_tx) => {
                if !peer.is_persisted() {
                    log_if_error_fmt!(
                        target: LOG_TARGET,
                        reply_tx.send(Err(ConnectionManagerError::PeerNotPersisted)),
                        "Failed to send dial result for peer '{}'",
                        peer.node_id.short_str()
                    );
                    return;
                }
                self.handle_dial_peer_request(pending_dials, *peer, reply_tx);
            },
            CancelPending(peer_id) => {
                debug!(target: LOG_TARGET, "Cancelling dial for peer id '{}'", peer_id);
                if let Some(mut signal) = self.cancel_signals.remove(&peer_id) {
                    log_if_error_fmt!(
                        target: LOG_TARGET,
                        signal.trigger(),
                        "Failed to cancel dial for peer Id('{}')",
                        peer_id
                    );
                }
            },
        }
    }

    fn is_pending_dial(&self, peer_id: &PeerId) -> bool {
        self.cancel_signals.contains_key(peer_id)
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
        dial_result: DialResult<NoiseSocket<TTransport::Output>>,
    )
    {
        let DialState { peer, reply_tx, .. } = dial_state;
        let peer_id = peer.id();

        let removed = self.cancel_signals.remove(&peer_id);
        debug_assert!(removed.is_some());
        drop(removed);

        let reply = match dial_result {
            Ok((socket, peer_addr)) => self.upgrade_to_peer_connection(&peer, socket, peer_addr).await,
            Err(err) => Err(err),
        };

        if self.pending_dial_requests.contains_key(&peer_id) {
            self.reply_to_pending_requests(peer_id, reply.clone());
        }

        log_if_error_fmt!(
            target: LOG_TARGET,
            reply_tx.send(reply),
            "Failed to send dial result reply for peer '{}'",
            short_str(&peer.public_key)
        );
    }

    async fn upgrade_to_peer_connection(
        &mut self,
        peer: &Peer,
        socket: NoiseSocket<TTransport::Output>,
        peer_addr: Multiaddr,
    ) -> Result<PeerConnection, ConnectionManagerError>
    {
        debug!(
            target: LOG_TARGET,
            "Connection successfully established to peer '{}' on address '{}'",
            peer.node_id.short_str(),
            peer_addr
        );
        let peer_public_key = socket
            .get_remote_public_key()
            .ok_or(ConnectionManagerError::InvalidStaticPublicKey)?;

        if peer_public_key != peer.public_key {
            return Err(ConnectionManagerError::DialedPublicKeyMismatch);
        }
        let peer_conn_result = create_peer_connection(
            self.executor.clone(),
            socket,
            peer_addr,
            peer_public_key,
            ConnectionDirection::Outbound,
            self.conn_man_notifier.clone(),
        )
        .await;

        match peer_conn_result {
            Ok(peer_conn) => {
                self.notify_connection_manager(ConnectionManagerEvent::PeerConnected(Box::new(peer_conn.clone())))
                    .await;

                Ok(peer_conn)
            },
            Err(err) => {
                let err_str = err.to_string();
                self.notify_connection_manager(ConnectionManagerEvent::PeerConnectFailed(
                    Box::new(peer.public_key.clone()),
                    err,
                ))
                .await;
                Err(ConnectionManagerError::YamuxUpgradeFailure(err_str))
            },
        }
    }

    pub async fn notify_connection_manager(&mut self, event: ConnectionManagerEvent) {
        log_if_error!(
            target: LOG_TARGET,
            self.conn_man_notifier.send(event).await,
            "Failed to publish event because '{error}'",
        );
    }

    fn reply_to_pending_requests(&mut self, peer_id: PeerId, result: Result<PeerConnection, ConnectionManagerError>) {
        self.pending_dial_requests.remove(&peer_id).and_then(|reply_oneshots| {
            reply_oneshots.into_iter().for_each(|tx| {
                log_if_error_fmt!(
                    target: LOG_TARGET,
                    tx.send(result.clone()),
                    "Failed to send dial result for peer '{}'",
                    peer_id
                );
            });
            Option::<()>::None
        });
    }

    fn handle_dial_peer_request(
        &mut self,
        pending_dials: &mut FuturesUnordered<
            BoxFuture<'static, Option<(DialState, DialResult<NoiseSocket<TTransport::Output>>)>>,
        >,
        peer: Peer,
        reply_tx: oneshot::Sender<Result<PeerConnection, ConnectionManagerError>>,
    )
    {
        if self.is_pending_dial(&peer.id()) {
            let entry = self.pending_dial_requests.entry(peer.id()).or_insert(Vec::new());
            entry.push(reply_tx);
            return;
        }

        let transport = self.transport.clone();
        let dial_cancel = Shutdown::new();
        let cancel_signal = dial_cancel.to_signal();
        self.cancel_signals.insert(peer.id(), dial_cancel);

        let backoff = Arc::clone(&self.backoff);
        let max_attempts = self.config.max_dial_attempts;

        let dial_state = DialState::new(peer, reply_tx, cancel_signal);
        pending_dials.push(
            Self::dial_peer_with_retry(dial_state, self.noise_config.clone(), transport, backoff, max_attempts).boxed(),
        );
    }

    async fn dial_peer_with_retry(
        dial_state: DialState,
        noise_config: NoiseConfig,
        transport: TTransport,
        backoff: Arc<TBackoff>,
        max_attempts: usize,
    ) -> Option<(DialState, DialResult<NoiseSocket<TTransport::Output>>)>
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
                        Some((state, Ok((socket, addr)))) => {
                            break Some((state, Ok((socket, addr))));
                        },
                        Some((mut state, Err(err))) => {
                            if state.num_attempts() > max_attempts {
                                break Some((state, Err(ConnectionManagerError::ConnectFailedMaximumAttemptsReached)));
                            }

                            // Put the dial state and transport back for the retry
                            dial_state = Some(state);
                            transport = Some(current_transport);
                        }
                        // Inflight dial was cancelled
                        None => break None,
                    }
                },
                // Delayed dial was cancelled
                _ = cancel_signal => {
                    debug!(target: LOG_TARGET, "[Attempt {}] Connecting to peer '{}'...", current_state.num_attempts(), current_state.peer.node_id.short_str());
                    break None
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
    ) -> Option<(DialState, DialResult<NoiseSocket<TTransport::Output>>)>
    {
        let mut addr_iter = dial_state.peer.addresses.address_iter();
        let cancel_signal = dial_state.get_cancel_signal();
        loop {
            let result = match addr_iter.next() {
                Some(address) => {
                    let dial_fut = async move {
                        let socket = transport
                            .dial(address.clone())
                            .await
                            .map_err(|err| ConnectionManagerError::TransportError(err.to_string()))?;
                        debug!(
                            target: LOG_TARGET,
                            "Socket established on '{}'. Performing noise upgrade protocol", address
                        );
                        let noise_socket = noise_config
                            .upgrade_socket(socket, ConnectionDirection::Outbound)
                            .await
                            .map_err(|err| ConnectionManagerError::NoiseError(err.to_string()))?;
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
                            break None;
                        },
                    }
                },
                // No more addresses to try - returning failure
                None => Err(ConnectionManagerError::DialConnectFailedAllAddresses),
            };

            drop(addr_iter);

            break Some((dial_state, result));
        }
    }
}

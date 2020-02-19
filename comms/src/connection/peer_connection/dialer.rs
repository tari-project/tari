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
    connection::{PeerConnectionSimpleState, PeerConnectionState, PeerConnectionStats},
    control::ControlMessage,
    types::{ConnectionInfo, PeerConnectionProtocolMessage},
    PeerConnectionContext,
    PeerConnectionError,
};
use crate::{
    connection::{
        connection::{Connection, EstablishedConnection},
        monitor::{ConnectionMonitor, SocketEvent, SocketEventType},
        peer_connection::types::PeerConnectionJoinHandle,
        zmq::ZmqIdentity,
        ConnectionDirection,
        InprocAddress,
    },
    message::FrameSet,
    utils::multiaddr::{multiaddr_to_socketaddr, socketaddr_to_multiaddr},
};
use log::*;
use std::{
    sync::{mpsc, mpsc::RecvTimeoutError, Arc, Condvar, Mutex, MutexGuard, RwLock},
    thread,
    time::Duration,
};
use tari_crypto::tari_utilities::hex::{to_hex, Hex};

const LOG_TARGET: &str = "comms::connection::peer_connection::worker";

/// Send HWM for outbound peer connections
const PEER_CONNECTION_OUTBOUND_SEND_HWM: i32 = 100;
/// Receive HWM for outbound peer connections
const PEER_CONNECTION_OUTBOUND_RECV_HWM: i32 = 10;

/// Set the allocated stack size for each PeerConnectionDialer thread
const THREAD_STACK_SIZE: usize = 64 * 1024; // 64kb

/// Worker which:
/// - Establishes a connection to peer
/// - Establishes a connection to the message consumer
/// - Receives and handles ControlMessages
/// - Forwards frames to consumer
/// - Handles SocketEvents and updates shared connection state
pub(super) struct PeerConnectionDialer {
    context: PeerConnectionContext,
    sender: mpsc::SyncSender<ControlMessage>,
    receiver: mpsc::Receiver<ControlMessage>,
    monitor_addr: InprocAddress,
    connection_state: Arc<Mutex<PeerConnectionState>>,
    connection_stats: Arc<RwLock<PeerConnectionStats>>,
    retry_count: u16,
    state_var: Arc<Condvar>,
    connection_identity: ZmqIdentity,
    peer_identity: ZmqIdentity,
}

impl PeerConnectionDialer {
    /// Create a new Worker from the given context
    pub fn new(
        mut context: PeerConnectionContext,
        connection_state: Arc<Mutex<PeerConnectionState>>,
        connection_stats: Arc<RwLock<PeerConnectionStats>>,
        state_var: Arc<Condvar>,
    ) -> Self
    {
        let (sender, receiver) = mpsc::sync_channel(10);
        Self {
            sender,
            receiver,
            monitor_addr: InprocAddress::random(),
            connection_state,
            connection_stats,
            retry_count: 0,
            state_var,
            connection_identity: context
                .connection_identity
                .take()
                .expect("already checked by PeerConnectionContextBuilder"),
            peer_identity: context
                .peer_identity
                .take()
                .expect("already checked by PeerConnectionContextBuilder"),
            context,
        }
    }

    /// Spawn a worker thread
    pub fn spawn(mut self) -> Result<PeerConnectionJoinHandle, PeerConnectionError> {
        {
            // Set connecting state
            let mut state_lock = acquire_lock!(self.connection_state);
            *state_lock = PeerConnectionState::Connecting(Arc::new(self.sender.clone().into()));
        }

        let short_id = {
            let start = match self.peer_identity.len().checked_sub(8) {
                Some(s) => s,
                None => self.peer_identity.len(),
            };
            self.peer_identity[start..].to_vec().to_hex()
        };

        let handle = thread::Builder::new()
            .name(format!("peer-conn-{}-thread", &short_id))
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || -> Result<(), PeerConnectionError> {
                let result = self.run();

                // Main loop exited, let's set the shared connection state.
                self.handle_run_result(result)?;

                Ok(())
            })
            .map_err(|_| PeerConnectionError::ThreadInitializationError)?;

        Ok(handle)
    }

    /// Handle the result for the worker loop and update connection state if necessary
    fn handle_run_result(&mut self, result: Result<(), PeerConnectionError>) -> Result<(), PeerConnectionError> {
        let mut lock = acquire_lock!(self.connection_state);
        match result {
            Ok(_) => {
                info!(
                    target: LOG_TARGET,
                    "[{}] Peer connection shut down cleanly", self.context.peer_address
                );
                // The loop exited cleanly.
                match *lock {
                    // The connection is still in a connected state, transition to Shutdown
                    PeerConnectionState::Connected(_) | PeerConnectionState::Connecting(_) => {
                        self.set_state(&mut lock, PeerConnectionState::Shutdown);
                    },
                    // Connection is in some other state, the loop exited without error
                    // so we won't change the state to preserve failed or disconnected states.
                    _ => {},
                }
            },
            Err(err) => {
                error!(
                    target: LOG_TARGET,
                    "[{}] Peer connection exited with an error: {:?}", self.context.peer_address, err
                );
                // Loop failed, update the connection state to reflect that
                self.set_state(&mut lock, PeerConnectionState::Failed(err));
            },
        }

        Ok(())
    }

    /// The main loop for the worker. This is where the work is done.
    /// The required connections are set up and messages processed.
    fn run(&mut self) -> Result<(), PeerConnectionError> {
        let monitor = self.connect_monitor()?;
        let peer_conn = self.establish_peer_connection()?;
        let addr = peer_conn.get_connected_address();

        if let Some(addr) = addr {
            debug!(target: LOG_TARGET, "Starting peer connection worker thread on {}", addr);
            self.context.peer_address = socketaddr_to_multiaddr(&addr);
        }

        loop {
            if let Some(msg) = self.receive_control_msg()? {
                use ControlMessage::*;
                match msg {
                    Shutdown => {
                        debug!(target: LOG_TARGET, "[{:?}] Shutdown message received", addr);
                        // Ensure that the peer connection is dropped as soon as possible.
                        // This somehow seemed to improve connection reliability.
                        drop(peer_conn);
                        break Ok(());
                    },
                    SendMsg(peer_identity, frames, reply_rx) => {
                        debug!(
                            target: LOG_TARGET,
                            "[{:?}] SendMsg control message received ({} frames) for identity '{}'",
                            addr,
                            frames.len(),
                            peer_identity
                                .as_ref()
                                .map(Hex::to_hex)
                                .unwrap_or_else(|| "<unspecified>".to_string()),
                        );

                        match self.send_payload(&peer_conn, PeerConnectionProtocolMessage::Message, frames) {
                            Ok(_) => {
                                let _ = reply_rx.send(Ok(()));
                                acquire_write_lock!(self.connection_stats).incr_message_sent();
                            },
                            Err(err) => {
                                let _ = reply_rx.send(Err(err.clone()));
                                if self.context.shutdown_on_send_failure {
                                    warn!(
                                        target: LOG_TARGET,
                                        "Error sending message: {}. Connection will shut down", err
                                    );
                                    break Err(err);
                                } else {
                                    trace!(
                                        target: LOG_TARGET,
                                        "Error when sending message. Connection will remain active"
                                    );
                                }
                            },
                        }
                    },
                    SetLinger(linger) => {
                        debug!(
                            target: LOG_TARGET,
                            "[{:?}] Setting linger to {:?} on peer connection", addr, linger
                        );
                        // Log and ignore errors here since this is unlikely to happen or cause any issues
                        match peer_conn.set_linger(linger) {
                            Ok(_) => {},
                            Err(err) => error!(target: LOG_TARGET, "Error setting linger on connection: {:?}", err),
                        }
                    },
                    AllowIdentity(_, _) => {
                        warn!(target: LOG_TARGET, "AllowIdentity called on outbound connection");
                    },
                    DenyIdentity(_) => {
                        warn!(target: LOG_TARGET, "DenyIdentity called on outbound connection");
                    },
                    TestConnection(identity, reply_tx) => {
                        debug!(
                            target: LOG_TARGET,
                            "Executing TestConnection for identity '{}'",
                            self.peer_identity.to_hex()
                        );
                        log_if_error_fmt!(
                            target: LOG_TARGET,
                            reply_tx.send(
                                self.send_payload(&peer_conn, PeerConnectionProtocolMessage::Ping, vec![])
                                    .map_err(|err| PeerConnectionError::ConnectionTestFailed(err.to_string())),
                            ),
                            "Error result back for TestConnection query for identity '{}'",
                            to_hex(&identity)
                        );
                        debug!(target: LOG_TARGET, "TestConnection complete");
                    },
                }
            }

            if let Ok(event) = monitor.read(1) {
                self.handle_socket_event(event)?;
            }

            self.handle_frames(&peer_conn)?;
        }
    }

    fn send_payload(
        &self,
        conn: &EstablishedConnection,
        message_type: PeerConnectionProtocolMessage,
        frames: FrameSet,
    ) -> Result<(), PeerConnectionError>
    {
        let payload = self.create_outbound_payload(message_type, frames);
        conn.send(payload).map_err(PeerConnectionError::SendFailure)
    }

    fn set_state<'a>(&self, guard: &mut MutexGuard<'a, PeerConnectionState>, new_state: PeerConnectionState) {
        **guard = new_state;
        self.state_var.notify_all();
    }

    /// Handles socket events from the ConnectionMonitor. Updating connection
    /// state as necessary.
    fn handle_socket_event(&mut self, event: SocketEvent) -> Result<(), PeerConnectionError> {
        use SocketEventType::*;

        trace!(target: LOG_TARGET, "{:?}", event);
        match event.event_type {
            Disconnected => {
                let mut lock = acquire_lock!(self.connection_state);
                self.set_state(&mut lock, PeerConnectionState::Disconnected);
            },

            Connected | HandshakeSucceeded => {
                self.retry_count = 0;
                self.transition_connected()?;
            },
            HandshakeFailedNoDetail | HandshakeFailedProtocol | HandshakeFailedAuth => {
                self.set_state(
                    &mut acquire_lock!(self.connection_state),
                    PeerConnectionState::Failed(PeerConnectionError::ConnectFailed),
                );
            },

            ConnectDelayed => {
                trace!(target: LOG_TARGET, "Still connecting...");
            },
            ConnectRetried => {
                let mut lock = acquire_lock!(self.connection_state);
                if let PeerConnectionState::Connecting(_) = *lock {
                    self.retry_count += 1;
                }
                if self.retry_count >= self.context.max_retry_attempts {
                    self.set_state(
                        &mut lock,
                        PeerConnectionState::Failed(PeerConnectionError::ExceededMaxConnectRetryCount),
                    )
                }
            },
            evt => {
                error!(
                    target: LOG_TARGET,
                    "Received unexpected socket '{}' event on outbound connection", evt
                );
            },
        }

        Ok(())
    }

    fn transition_connected(&self) -> Result<(), PeerConnectionError> {
        let mut lock = acquire_lock!(self.connection_state);

        match &*lock {
            PeerConnectionState::Connecting(thread_ctl) => {
                let info = ConnectionInfo {
                    control_messenger: thread_ctl.clone(),
                    connected_address: multiaddr_to_socketaddr(&self.context.peer_address).ok().map(Into::into),
                };
                info!(target: LOG_TARGET, "[{}] Connected", self.context.peer_address);
                self.set_state(&mut lock, PeerConnectionState::Connected(Arc::new(info)));
            },
            PeerConnectionState::Connected(_) => {
                debug!(
                    target: LOG_TARGET,
                    "[{}] Connected event when already connected", self.context.peer_address
                );
            },
            s => {
                return Err(PeerConnectionError::StateError(format!(
                    "Unable to transition to connected state from state '{}'",
                    PeerConnectionSimpleState::from(s)
                )));
            },
        }

        debug!(
            target: LOG_TARGET,
            "[{}] Peer connection state is '{}'",
            self.context.peer_address,
            PeerConnectionSimpleState::from(&*lock)
        );

        Ok(())
    }

    /// Connects the connection monitor to this worker's peer Connection.
    fn connect_monitor(&self) -> Result<ConnectionMonitor, PeerConnectionError> {
        let context = &self.context;
        ConnectionMonitor::connect(&context.context, &self.monitor_addr).map_err(Into::into)
    }

    /// Handles PeerMessageType messages Forwards frames from the source to the sink
    fn handle_frames(&mut self, conn: &EstablishedConnection) -> Result<(), PeerConnectionError> {
        if let Some(frames) = connection_try!(conn.receive(10)) {
            trace!(target: LOG_TARGET, "Received {} frame(s)", frames.len());
            // Attempt to extract the parts of a peer message.
            // If we can't extract the correct frames, we ignore the message
            if let Some((message_type, frames)) = self.extract_frame_parts(frames) {
                match message_type {
                    PeerConnectionProtocolMessage::Identify => {
                        warn!(
                            target: LOG_TARGET,
                            "Ignoring IDENTIFY message sent to outbound peer connection '{}'",
                            self.peer_identity.to_hex()
                        );
                    },
                    PeerConnectionProtocolMessage::Message => {
                        acquire_write_lock!(self.connection_stats).incr_message_recv();

                        let payload = self.construct_sink_payload(frames);
                        self.send_to_sink(payload)?
                    },
                    PeerConnectionProtocolMessage::Ping => {
                        debug!(
                            target: LOG_TARGET,
                            "Received ping on outbound connection for address '{}'", self.context.peer_address
                        );
                    },
                    PeerConnectionProtocolMessage::Deny => {
                        warn!(
                            target: LOG_TARGET,
                            "Peer at address '{}' has denied the outbound connection", self.context.peer_address,
                        );
                        self.set_state(
                            &mut acquire_lock!(self.connection_state),
                            PeerConnectionState::Failed(PeerConnectionError::ConnectionDenied),
                        );
                    },
                    PeerConnectionProtocolMessage::Invalid => {
                        debug!(
                            target: LOG_TARGET,
                            "Peer sent invalid message type. Discarding the message",
                        );
                    },
                }
            }
        }
        Ok(())
    }

    fn extract_frame_parts(&self, mut frames: FrameSet) -> Option<(PeerConnectionProtocolMessage, FrameSet)> {
        if frames.is_empty() {
            return None;
        }
        let mut msg_type_frame = frames.remove(0);
        if msg_type_frame.is_empty() {
            return None;
        }
        let message_type_u8 = msg_type_frame.remove(0);

        Some((message_type_u8.into(), frames))
    }

    fn construct_sink_payload(&self, frames: FrameSet) -> FrameSet {
        let mut payload = Vec::with_capacity(1 + frames.len());
        payload.push(self.peer_identity.clone());
        payload.extend_from_slice(&frames);
        payload
    }

    fn send_to_sink(&mut self, mut payload: FrameSet) -> Result<(), PeerConnectionError> {
        let mut attempts = 0;
        loop {
            match self.context.message_sink_channel.try_send(payload) {
                Ok(_) => break Ok(()),
                Err(e) => {
                    if e.is_full() {
                        warn!(
                            target: LOG_TARGET,
                            "Message Sink MPSC channel is full. Payload will be sent in 1 second"
                        );
                        if attempts > 10 {
                            error!(
                                target: LOG_TARGET,
                                "Message Sink MPSC channel is full and has not cleared after 10 seconds! Discarding \
                                 pending message."
                            );
                            break Err(PeerConnectionError::ChannelBacklogError);
                        }
                        attempts += 1;
                        // Create back pressure on peer by not reading from the socket
                        thread::sleep(Duration::from_secs(1));
                        payload = e.into_inner();
                    } else {
                        break Err(PeerConnectionError::ChannelDisconnectedError);
                    }
                },
            }
        }
    }

    /// Creates the payload to be sent to the underlying connection
    fn create_outbound_payload(&self, message_type: PeerConnectionProtocolMessage, frames: FrameSet) -> FrameSet {
        let mut payload = Vec::with_capacity(1 + frames.len());
        payload.push(vec![message_type as u8]);
        payload.extend(frames);
        debug!(target: LOG_TARGET, "Created payload ({} frame(s) total)", payload.len());
        payload
    }

    /// Receive a `ControlMessage` on the control message channel
    #[inline]
    fn receive_control_msg(&self) -> Result<Option<ControlMessage>, PeerConnectionError> {
        match self.receiver.recv_timeout(Duration::from_millis(5)) {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => match e {
                RecvTimeoutError::Disconnected => Err(PeerConnectionError::ControlChannelDisconnected),
                RecvTimeoutError::Timeout => Ok(None),
            },
        }
    }

    /// Establish the connection to the peer address
    fn establish_peer_connection(&self) -> Result<EstablishedConnection, PeerConnectionError> {
        let context = &self.context;
        Connection::new(&context.context, ConnectionDirection::Outbound)
            .set_name(
                format!(
                    "peer-conn-{}",
                    context
                        .connection_identity
                        .as_ref()
                        .map(Hex::to_hex)
                        .unwrap_or_else(|| "<no-ident>".to_string())
                )
                .as_str(),
            )
            .set_identity(&self.connection_identity)
            .set_linger(context.linger.clone())
            .set_heartbeat_interval(Duration::from_millis(1000))
            .set_heartbeat_timeout(Duration::from_millis(5000))
            .set_monitor_addr(self.monitor_addr.clone())
            .set_curve_encryption(context.curve_encryption.clone())
            .set_socks_proxy_addr(context.socks_address.clone())
            .set_max_message_size(Some(context.max_msg_size))
            .set_receive_hwm(PEER_CONNECTION_OUTBOUND_RECV_HWM)
            .set_send_hwm(PEER_CONNECTION_OUTBOUND_SEND_HWM)
            .establish(&context.peer_address)
            .map_err(Into::into)
    }
}

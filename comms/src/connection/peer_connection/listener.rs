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
        types::Direction,
        zmq::ZmqIdentity,
        InprocAddress,
    },
    message::{Frame, FrameSet},
    utils::multiaddr::multiaddr_to_socketaddr,
};
use log::*;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender},
        Arc,
        Condvar,
        Mutex,
        MutexGuard,
        RwLock,
    },
    thread,
    time::Duration,
};
use tari_utilities::hex::Hex;

const LOG_TARGET: &str = "comms::connection::peer_connection::worker";

/// Send HWM for inbound peer connections
const PEER_CONNECTION_INBOUND_SEND_HWM: i32 = 100;
/// Receive HWM for inbound peer connections
const PEER_CONNECTION_INBOUND_RECV_HWM: i32 = 100;

/// Set the allocated stack size for each PeerConnectionListener thread
const THREAD_STACK_SIZE: usize = 64 * 1024; // 64kb

/// Worker which:
/// - Establishes a connection to peer
/// - Establishes a connection to the message consumer
/// - Receives and handles ControlMessages
/// - Forwards frames to consumer
/// - Handles SocketEvents and updates shared connection state
pub(super) struct PeerConnectionListener {
    context: PeerConnectionContext,
    sender: SyncSender<ControlMessage>,
    receiver: Receiver<ControlMessage>,
    identity_whitelist: HashMap<ZmqIdentity, ZmqIdentity>,
    monitor_addr: InprocAddress,
    connection_state: Arc<Mutex<PeerConnectionState>>,
    connection_stats: Arc<RwLock<PeerConnectionStats>>,
    retry_count: u16,
    state_var: Arc<Condvar>,
}

impl PeerConnectionListener {
    /// Create a new Worker from the given context
    pub fn new(
        context: PeerConnectionContext,
        connection_state: Arc<Mutex<PeerConnectionState>>,
        connection_stats: Arc<RwLock<PeerConnectionStats>>,
        state_var: Arc<Condvar>,
    ) -> Self
    {
        let (sender, receiver) = sync_channel(10);
        Self {
            context,
            sender,
            receiver,
            monitor_addr: InprocAddress::random(),
            identity_whitelist: HashMap::new(),
            connection_state,
            connection_stats,
            retry_count: 0,
            state_var,
        }
    }

    /// Spawn a worker thread
    pub fn spawn(mut self) -> Result<PeerConnectionJoinHandle, PeerConnectionError> {
        {
            // Set connecting state
            let mut state_lock = acquire_lock!(self.connection_state);
            *state_lock = PeerConnectionState::Connecting(Arc::new(self.sender.clone().into()));
        }

        let handle = thread::Builder::new()
            .name(format!("inbound-listener-{}", self.context.peer_address))
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
            self.context.peer_address = addr.clone().into();
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
                        match peer_identity {
                            Some(identity) => {
                                match self.handle_sendmsg(&peer_conn, identity, frames) {
                                    Ok(_) => {
                                        let _ = reply_rx.send(Ok(()));
                                    },
                                    Err(err) => {
                                        let _ = reply_rx.send(Err(err.clone()));
                                        if self.context.shutdown_on_send_failure {
                                            warn!(
                                                target: LOG_TARGET,
                                                "Error sending message: {}. Connection will shut down", err
                                            );
                                            // An error returned here will exit the run loop
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
                            None => {
                                let _ = reply_rx.send(Err(PeerConnectionError::InvalidOperation(
                                    "SendMsg called without a peer identity. This is invalid for an inbound connection"
                                        .to_string(),
                                )));
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
                    AllowIdentity(connection_identity, peer_identity) => {
                        self.identity_whitelist.insert(connection_identity, peer_identity);
                    },
                    DenyIdentity(local_identity) => {
                        self.identity_whitelist.remove(&local_identity);
                    },
                    TestConnection(peer_identity, reply_tx) => {
                        debug!(
                            target: LOG_TARGET,
                            "Executing TestConnection for identity '{}'",
                            peer_identity.to_hex()
                        );
                        log_if_error!(
                            target: LOG_TARGET,
                            "Error result back for TestConnection query",
                            reply_tx.send(
                                self.send_payload(
                                    &peer_conn,
                                    PeerConnectionProtocolMessage::Ping,
                                    &peer_identity,
                                    vec![]
                                )
                                .map_err(|err| PeerConnectionError::ConnectionTestFailed(err.to_string())),
                            ),
                            no_fmt
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

    fn handle_sendmsg(
        &self,
        peer_conn: &EstablishedConnection,
        peer_identity: ZmqIdentity,
        frames: FrameSet,
    ) -> Result<(), PeerConnectionError>
    {
        debug!(
            target: LOG_TARGET,
            "SendMsg control message received ({} frames) for identity '{}'",
            frames.len(),
            peer_identity.to_hex()
        );

        self.send_payload(
            &peer_conn,
            PeerConnectionProtocolMessage::Message,
            &peer_identity,
            frames,
        )
        .and_then(|_| {
            acquire_write_lock!(self.connection_stats).incr_message_sent();
            Ok(())
        })
    }

    fn send_payload(
        &self,
        conn: &EstablishedConnection,
        message_type: PeerConnectionProtocolMessage,
        peer_identity: &ZmqIdentity,
        frames: FrameSet,
    ) -> Result<(), PeerConnectionError>
    {
        let conn_identity = self
            .get_whitelisted_connection_identity(&peer_identity)
            .ok_or(PeerConnectionError::AccessDenied)?;
        let payload = self.create_inbound_payload(message_type, conn_identity, frames);
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
                debug!(target: LOG_TARGET, "Peer disconnected from listener");
            },
            Listening => {
                self.retry_count = 0;
                let mut lock = acquire_lock!(self.connection_state);
                match *lock {
                    PeerConnectionState::Connecting(ref thread_ctl) => {
                        let info = ConnectionInfo {
                            control_messenger: thread_ctl.clone(),
                            connected_address: self.bound_addr(),
                        };
                        info!(
                            target: LOG_TARGET,
                            "[{}] Listening on Inbound connection", self.context.peer_address
                        );

                        self.set_state(&mut lock, PeerConnectionState::Listening(Arc::new(info)));
                    },
                    PeerConnectionState::Connected(_) => {
                        warn!(
                            target: LOG_TARGET,
                            "[{}] Listening event when connected", self.context.peer_address
                        );
                    },
                    ref s => {
                        return Err(PeerConnectionError::StateError(format!(
                            "Unable to transition to connected state from state '{}'",
                            PeerConnectionSimpleState::from(s)
                        ))
                        .into());
                    },
                }
            },
            Connected => {
                self.retry_count = 0;
                self.transition_connected()?;
            },
            BindFailed => {
                self.set_state(
                    &mut acquire_lock!(self.connection_state),
                    PeerConnectionState::Failed(PeerConnectionError::ConnectFailed),
                );
            },
            HandshakeFailedNoDetail | HandshakeFailedProtocol | HandshakeFailedAuth => {
                // Don't set Fail state on inbound connections if these occur
                debug!(
                    target: LOG_TARGET,
                    "CurveZMQ handshake failed for address '{}' (error_code={})", event.address, event.event_value
                );
            },
            ConnectRetried => {
                debug!(target: LOG_TARGET, "Connection retrying...",);
            },
            _ => {},
        }

        Ok(())
    }

    fn transition_connected(&self) -> Result<(), PeerConnectionError> {
        let mut lock = acquire_lock!(self.connection_state);

        match &*lock {
            PeerConnectionState::Connecting(thread_ctl) => {
                let info = ConnectionInfo {
                    control_messenger: thread_ctl.clone(),
                    connected_address: self.bound_addr(),
                };
                info!(target: LOG_TARGET, "[{}] Connected", self.context.peer_address);
                self.set_state(&mut lock, PeerConnectionState::Listening(Arc::new(info)));
            },
            PeerConnectionState::Listening(_) => {
                info!(
                    target: LOG_TARGET,
                    "Inbound connection already listening on {}", self.context.peer_address
                );
            },
            PeerConnectionState::Connected(_) => {
                warn!(
                    target: LOG_TARGET,
                    "[{}] Connected event when listening", self.context.peer_address
                );
            },
            s => {
                return Err(PeerConnectionError::StateError(format!(
                    "Unable to transition to connected state from state '{}'",
                    PeerConnectionSimpleState::from(s)
                ))
                .into());
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

    fn bound_addr(&self) -> Option<SocketAddr> {
        multiaddr_to_socketaddr(&self.context.peer_address).ok()
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
            if let Some((connection_identity, message_type, frames)) = self.extract_frame_parts(frames) {
                let peer_identity = match self.get_whitelisted_peer_identity(&connection_identity) {
                    Some(peer_identity) => peer_identity,
                    None => {
                        warn!(
                            target: LOG_TARGET,
                            "Connection identity '{}' not whitelisted",
                            connection_identity.to_hex(),
                        );

                        let _ =
                            self.send_payload(&conn, PeerConnectionProtocolMessage::Deny, &connection_identity, vec![]);

                        return Ok(());
                    },
                };

                match message_type {
                    PeerConnectionProtocolMessage::Identify => {
                        debug!(
                            target: LOG_TARGET,
                            "Received IDENTIFY message from identity {}",
                            peer_identity.to_hex()
                        );
                    },
                    PeerConnectionProtocolMessage::Message => {
                        acquire_write_lock!(self.connection_stats).incr_message_recv();

                        let payload = self.construct_sink_payload(peer_identity, frames);
                        log_if_error!(
                            target: LOG_TARGET,
                            "Failed to send to sink because '{}'",
                            self.send_to_sink(payload)
                        );
                    },
                    PeerConnectionProtocolMessage::Ping => {
                        debug!(
                            target: LOG_TARGET,
                            "Received ping on inbound connection for address '{}'", self.context.peer_address
                        );
                    },
                    PeerConnectionProtocolMessage::Deny => {
                        // This should never happen
                        debug!(
                            target: LOG_TARGET,
                            "Peer at address '{}' has denied our inbound connection.", self.context.peer_address,
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

    fn get_whitelisted_peer_identity(&self, conn_identity: &ZmqIdentity) -> Option<ZmqIdentity> {
        self.identity_whitelist.get(conn_identity).map(Clone::clone)
    }

    fn get_whitelisted_connection_identity(&self, peer_identity: &ZmqIdentity) -> Option<ZmqIdentity> {
        self.identity_whitelist
            .iter()
            .find(|(_, v)| *v == peer_identity)
            .map(|(k, _)| k.clone())
    }

    fn extract_frame_parts(&self, mut frames: FrameSet) -> Option<(Frame, PeerConnectionProtocolMessage, FrameSet)> {
        if frames.len() < 2 {
            return None;
        }
        let identity = frames.remove(0);
        let mut msg_type_frame = frames.remove(0);
        if msg_type_frame.len() == 0 {
            return None;
        }
        let message_type_u8 = msg_type_frame.remove(0);

        Some((identity, message_type_u8.into(), frames))
    }

    fn construct_sink_payload(&self, peer_identity: ZmqIdentity, frames: FrameSet) -> FrameSet {
        let mut payload = Vec::with_capacity(1 + frames.len());
        payload.push(peer_identity);
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
    fn create_inbound_payload(
        &self,
        message_type: PeerConnectionProtocolMessage,
        conn_identity: ZmqIdentity,
        frames: FrameSet,
    ) -> FrameSet
    {
        // Add identity frame to the front of the payload for ROUTER socket
        let mut payload = Vec::with_capacity(2 + frames.len());
        payload.push(conn_identity);
        payload.push(vec![message_type as u8]);
        payload.extend(frames);
        debug!(target: LOG_TARGET, "Created payload ({} frame(s) total)", payload.len());
        payload
    }

    /// Receive a `ControlMessage` on the control message channel
    fn receive_control_msg(&self) -> Result<Option<ControlMessage>, PeerConnectionError> {
        match self.receiver.recv_timeout(Duration::from_millis(5)) {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => match e {
                RecvTimeoutError::Disconnected => Err(PeerConnectionError::ControlChannelDisconnected.into()),
                RecvTimeoutError::Timeout => Ok(None),
            },
        }
    }

    /// Establish the connection to the peer address
    fn establish_peer_connection(&self) -> Result<EstablishedConnection, PeerConnectionError> {
        let context = &self.context;
        Connection::new(&context.context, Direction::Inbound)
            .set_name(format!("peer-conn-inbound-{}", self.context.peer_address).as_str())
            .set_linger(context.linger.clone())
            .set_heartbeat_interval(Duration::from_millis(1000))
            .set_heartbeat_timeout(Duration::from_millis(5000))
            .set_monitor_addr(self.monitor_addr.clone())
            .set_curve_encryption(context.curve_encryption.clone())
            .set_socks_proxy_addr(context.socks_address.clone())
            .set_max_message_size(Some(context.max_msg_size))
            .set_receive_hwm(PEER_CONNECTION_INBOUND_RECV_HWM)
            .set_send_hwm(PEER_CONNECTION_INBOUND_SEND_HWM)
            .establish(&context.peer_address)
            .map_err(Into::into)
    }
}

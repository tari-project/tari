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

use crate::{
    connection::{
        connection::{Connection, EstablishedConnection},
        monitor::{ConnectionMonitor, SocketEvent, SocketEventType},
        peer_connection::{
            connection::{ConnectionInfo, PeerConnectionSimpleState, PeerConnectionState, PeerConnectionStats},
            control::ControlMessage,
            PeerConnectionContext,
            PeerConnectionError,
        },
        types::{Direction, Result},
        ConnectionError,
        InprocAddress,
        NetAddress,
    },
    message::{Frame, FrameSet},
};
use log::*;
use std::{
    sync::{
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender},
        Arc,
        RwLock,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const LOG_TARGET: &'static str = "comms::connection::peer_connection::worker";

/// Send HWM for peer connections
const PEER_CONNECTION_SEND_HWM: i32 = 10;
/// Receive HWM for peer connections
const PEER_CONNECTION_RECV_HWM: i32 = 10;

/// Set the allocated stack size for each PeerConnectionWorker thread
const THREAD_STACK_SIZE: usize = 64 * 1024; // 64kb

/// Worker which:
/// - Establishes a connection to peer
/// - Establishes a connection to the message consumer
/// - Receives and handles ControlMessages
/// - Forwards frames to consumer
/// - Handles SocketEvents and updates shared connection state
pub(super) struct PeerConnectionWorker {
    context: PeerConnectionContext,
    sender: SyncSender<ControlMessage>,
    receiver: Receiver<ControlMessage>,
    identity: Option<Frame>,
    paused: bool,
    monitor_addr: InprocAddress,
    connection_state: Arc<RwLock<PeerConnectionState>>,
    connection_stats: Arc<RwLock<PeerConnectionStats>>,
    retry_count: u16,
}

impl PeerConnectionWorker {
    /// Create a new Worker from the given context
    pub fn new(
        context: PeerConnectionContext,
        connection_state: Arc<RwLock<PeerConnectionState>>,
        connection_stats: Arc<RwLock<PeerConnectionStats>>,
    ) -> Self
    {
        let (sender, receiver) = sync_channel(5);
        Self {
            context,
            sender,
            receiver,
            identity: None,
            paused: false,
            monitor_addr: InprocAddress::random(),
            connection_state,
            connection_stats,
            retry_count: 0,
        }
    }

    /// Spawn a worker thread
    pub fn spawn(mut self) -> Result<JoinHandle<Result<()>>> {
        {
            // Set connecting state
            let mut state_lock = acquire_write_lock!(self.connection_state);
            *state_lock = PeerConnectionState::Connecting(Arc::new(self.sender.clone().into()));
        }

        let handle = thread::Builder::new()
            .name(format!("peer-conn-{}-thread", &self.context.id.to_short_id()))
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || -> Result<()> {
                let result = self.main_loop();

                // Main loop exited, let's set the shared connection state.
                self.handle_loop_result(result)?;

                Ok(())
            })
            .map_err(|_| PeerConnectionError::ThreadInitializationError)?;

        Ok(handle)
    }

    /// Handle the result for the worker loop and update connection state if necessary
    fn handle_loop_result(&mut self, result: Result<()>) -> Result<()> {
        let mut lock = acquire_write_lock!(self.connection_state);
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
                        *lock = PeerConnectionState::Shutdown;
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
                *lock = match err {
                    ConnectionError::PeerError(err) => PeerConnectionState::Failed(err),
                    e => PeerConnectionState::Failed(PeerConnectionError::UnexpectedConnectionError(format!("{}", e))),
                };
            },
        }

        Ok(())
    }

    /// The main loop for the worker. This is where the work is done.
    /// The required connections are set up and messages processed.
    fn main_loop(&mut self) -> Result<()> {
        let monitor = self.connect_monitor()?;
        let peer_conn = self.establish_peer_connection()?;
        let consumer = self.establish_consumer_connection()?;
        let addr = peer_conn.get_connected_address();

        if let Some(a) = addr {
            debug!(target: LOG_TARGET, "Starting peer connection worker thread on {}", a);
            self.context.peer_address = a.clone().into();
        }

        loop {
            if let Some(msg) = self.receive_control_msg()? {
                match msg {
                    ControlMessage::Shutdown => {
                        debug!(target: LOG_TARGET, "[{:?}] Shutdown message received", addr);
                        // Ensure that the peer connection is dropped as soon as possible.
                        // This somehow seemed to improve connection reliability.
                        drop(peer_conn);
                        break Ok(());
                    },
                    ControlMessage::SendMsg(frames) => {
                        debug!(
                            target: LOG_TARGET,
                            "[{:?}] SendMsg message received ({} frames)",
                            addr,
                            frames.len()
                        );
                        let payload = self.create_payload(frames)?;
                        peer_conn.send(payload)?;
                        acquire_write_lock!(self.connection_stats).incr_message_sent();
                    },
                    ControlMessage::Pause => {
                        debug!(target: LOG_TARGET, "[{:?}] Pause message received", addr);
                        self.paused = true;
                    },
                    ControlMessage::Resume => {
                        debug!(target: LOG_TARGET, "[{:?}] Resume message received", addr);
                        self.paused = false;
                    },
                    ControlMessage::SetLinger(linger) => {
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
                }
            }

            if !self.paused {
                self.forward_frames(&peer_conn, &consumer)?;
            }

            if let Ok(event) = monitor.read(1) {
                self.handle_socket_event(event)?;
            }
        }
    }

    /// Handles socket events from the ConnectionMonitor. Updating connection
    /// state as necessary.
    fn handle_socket_event(&mut self, event: SocketEvent) -> Result<()> {
        use SocketEventType::*;

        debug!(target: LOG_TARGET, "{:?}", event);
        match event.event_type {
            Disconnected => {
                let mut lock = acquire_write_lock!(self.connection_state);
                *lock = PeerConnectionState::Disconnected;
            },
            Listening => {
                self.retry_count = 0;
                let mut lock = acquire_write_lock!(self.connection_state);
                match *lock {
                    PeerConnectionState::Connecting(ref thread_ctl) => {
                        let info = ConnectionInfo {
                            control_messenger: thread_ctl.clone(),
                            connected_address: match self.context.peer_address {
                                NetAddress::IP(ref socket_addr) => Some(socket_addr.clone()),
                                _ => None,
                            },
                        };
                        info!(
                            target: LOG_TARGET,
                            "[{}] Listening on Inbound connection", self.context.peer_address
                        );
                        *lock = PeerConnectionState::Listening(Arc::new(info));
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
            Accepted | Connected => {
                self.retry_count = 0;
                self.transition_connected()?;
            },
            BindFailed | AcceptFailed | HandshakeFailedNoDetail | HandshakeFailedProtocol | HandshakeFailedAuth => {
                let mut lock = acquire_write_lock!(self.connection_state);
                *lock = PeerConnectionState::Failed(PeerConnectionError::ConnectFailed);
            },
            ConnectRetried => {
                let mut lock = acquire_write_lock!(self.connection_state);
                match *lock {
                    PeerConnectionState::Connecting(_) => {
                        self.retry_count += 1;
                        if self.retry_count >= self.context.max_retry_attempts {
                            *lock = PeerConnectionState::Failed(PeerConnectionError::ExceededMaxConnectRetryCount);
                        }
                    },
                    _ => {},
                }
            },
            _ => {},
        }

        Ok(())
    }

    fn transition_connected(&self) -> Result<()> {
        let mut lock = acquire_write_lock!(self.connection_state);

        match *lock {
            PeerConnectionState::Connecting(ref thread_ctl) => {
                let info = ConnectionInfo {
                    control_messenger: thread_ctl.clone(),
                    connected_address: match self.context.peer_address {
                        NetAddress::IP(ref socket_addr) => Some(socket_addr.clone()),
                        _ => None,
                    },
                };
                info!(target: LOG_TARGET, "[{}] Connected", self.context.peer_address);
                match self.context.direction {
                    Direction::Inbound => {
                        if self.identity.is_some() {
                            *lock = PeerConnectionState::Connected(Arc::new(info));
                        }
                    },
                    Direction::Outbound => {
                        *lock = PeerConnectionState::Connected(Arc::new(info));
                    },
                }
            },
            PeerConnectionState::Listening(ref info) => match self.context.direction {
                Direction::Inbound => {
                    info!(
                        target: LOG_TARGET,
                        "Inbound connection listening on {}", self.context.peer_address
                    );
                    if self.identity.is_some() {
                        *lock = PeerConnectionState::Connected(info.clone());
                    }
                },
                Direction::Outbound => {
                    return Err(PeerConnectionError::StateError(format!(
                        "Should not happen: outbound connection was in listening state",
                    ))
                    .into());
                },
            },
            PeerConnectionState::Connected(_) => {
                warn!(
                    target: LOG_TARGET,
                    "[{}] Connected event when already connected", self.context.peer_address
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

        debug!(
            target: LOG_TARGET,
            "[{}] Peer connection state is '{}'",
            self.context.peer_address,
            PeerConnectionSimpleState::from(&*lock)
        );

        Ok(())
    }

    /// Connects the connection monitor to this worker's peer Connection.
    fn connect_monitor(&self) -> Result<ConnectionMonitor> {
        let context = &self.context;
        ConnectionMonitor::connect(&context.context, &self.monitor_addr)
    }

    /// Forwards frames from the source to the sink
    fn forward_frames(&mut self, frontend: &EstablishedConnection, backend: &EstablishedConnection) -> Result<()> {
        let context = &self.context;
        if let Some(frames) = connection_try!(frontend.receive(10)) {
            acquire_write_lock!(self.connection_stats).incr_message_recv();

            match context.direction {
                // For a ZMQ_ROUTER, the first frame is the identity
                Direction::Inbound => match self.identity {
                    Some(ref ident) => {
                        if frames[0] != *ident {
                            return Err(PeerConnectionError::UnexpectedIdentity.into());
                        }
                    },
                    None => {
                        self.identity = Some(frames[0].clone());
                        debug!(
                            target: LOG_TARGET,
                            "Set peer connection identity to {:x?}", self.identity
                        );
                        self.transition_connected()?;
                    },
                },
                Direction::Outbound => {},
            }

            let payload = self.construct_consumer_payload(frames);
            backend.send(&payload)?;
        }
        Ok(())
    }

    fn construct_consumer_payload(&self, frames: FrameSet) -> FrameSet {
        let mut payload = vec![];
        payload.push(self.context.id.clone().into_inner());
        match self.context.direction {
            Direction::Inbound => {
                payload.extend_from_slice(&frames[1..]);
            },
            Direction::Outbound => {
                payload.extend_from_slice(&frames);
            },
        }
        payload
    }

    /// Creates the payload to be sent to the underlying connection
    #[inline]
    fn create_payload(&self, frames: FrameSet) -> Result<FrameSet> {
        let context = &self.context;

        match context.direction {
            // Add identity frame to the front of the payload for ROUTER socket
            Direction::Inbound => match self.identity {
                Some(ref ident) => {
                    let mut payload = vec![ident.clone()];
                    payload.extend(frames);
                    Ok(payload)
                },
                None => return Err(PeerConnectionError::IdentityNotEstablished.into()),
            },
            Direction::Outbound => Ok(frames),
        }
    }

    /// Receive a `ControlMessage` on the control message channel
    fn receive_control_msg(&self) -> Result<Option<ControlMessage>> {
        match self.receiver.recv_timeout(Duration::from_millis(5)) {
            Ok(msg) => Ok(Some(msg)),
            Err(e) => match e {
                RecvTimeoutError::Disconnected => Err(PeerConnectionError::ControlPortDisconnected.into()),
                RecvTimeoutError::Timeout => Ok(None),
            },
        }
    }

    /// Establish the connection to the peer address
    fn establish_peer_connection(&self) -> Result<EstablishedConnection> {
        let context = &self.context;
        Connection::new(&context.context, context.direction.clone())
            .set_linger(context.linger.clone())
            .set_heartbeat_interval(Duration::from_millis(1000))
            .set_heartbeat_timeout(Duration::from_millis(5000))
            .set_monitor_addr(self.monitor_addr.clone())
            .set_curve_encryption(context.curve_encryption.clone())
            .set_receive_hwm(PEER_CONNECTION_RECV_HWM)
            .set_send_hwm(PEER_CONNECTION_SEND_HWM)
            .set_socks_proxy_addr(context.socks_address.clone())
            .set_max_message_size(Some(context.max_msg_size))
            .establish(&context.peer_address)
    }

    /// Establish the connection to the consumer
    fn establish_consumer_connection(&self) -> Result<EstablishedConnection> {
        let context = &self.context;
        Connection::new(&context.context, Direction::Outbound).establish(&context.message_sink_address)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::connection::{
        peer_connection::{control::ThreadControlMessenger, ConnectionId},
        types::Linger,
        CurveEncryption,
        ZmqContext,
    };

    fn make_thread_ctl() -> (Arc<ThreadControlMessenger>, Receiver<ControlMessage>) {
        let (tx, rx) = sync_channel(1);
        (Arc::new(tx.into()), rx)
    }

    fn transition_connected_setup(
        direction: Direction,
        initial_state: PeerConnectionSimpleState,
        identity: Option<Frame>,
    ) -> PeerConnectionWorker
    {
        let context = ZmqContext::new();
        let peer_address = "127.0.0.1:9000".parse().unwrap();

        let (thread_ctl, receiver) = make_thread_ctl();
        let info = Arc::new(ConnectionInfo {
            connected_address: None,
            control_messenger: Arc::clone(&thread_ctl),
        });
        let connection_state = match initial_state {
            PeerConnectionSimpleState::Initial => PeerConnectionState::Initial,
            PeerConnectionSimpleState::Connecting => PeerConnectionState::Connecting(Arc::clone(&thread_ctl)),
            PeerConnectionSimpleState::Connected(_) => PeerConnectionState::Connected(info),
            PeerConnectionSimpleState::Disconnected => PeerConnectionState::Disconnected,
            PeerConnectionSimpleState::Shutdown => PeerConnectionState::Shutdown,
            PeerConnectionSimpleState::Listening(_) => PeerConnectionState::Listening(info),
            PeerConnectionSimpleState::Failed(err) => PeerConnectionState::Failed(err),
        };

        let context = PeerConnectionContext {
            context,
            message_sink_address: InprocAddress::random(),
            peer_address,
            direction,
            linger: Linger::Indefinitely,
            id: ConnectionId::default(),
            curve_encryption: CurveEncryption::default(),
            socks_address: None,
            max_msg_size: 1024 * 1024,
            max_retry_attempts: 1,
        };
        PeerConnectionWorker {
            context,
            identity,
            receiver,
            sender: thread_ctl.get_sender().clone(),
            connection_state: Arc::new(RwLock::new(connection_state)),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            monitor_addr: InprocAddress::random(),
            retry_count: 1,
            paused: false,
        }
    }

    #[test]
    fn transition_connected() {
        // Transition outbound to connected
        let subject = transition_connected_setup(Direction::Outbound, PeerConnectionSimpleState::Connecting, None);
        subject.transition_connected().unwrap();
        {
            let lock = subject.connection_state.read().unwrap();
            match (&*lock).into() {
                PeerConnectionSimpleState::Connected(_) => {},
                s => panic!("Unexpected state '{:?}'", s),
            }
        }

        // Transition connecting inbound without identity
        let subject = transition_connected_setup(Direction::Inbound, PeerConnectionSimpleState::Connecting, None);
        subject.transition_connected().unwrap();
        {
            let lock = subject.connection_state.read().unwrap();
            match (&*lock).into() {
                PeerConnectionSimpleState::Connecting => {},
                s => panic!("Unexpected state '{:?}'", s),
            }
        }

        // Transition connecting inbound with identity
        let subject = transition_connected_setup(
            Direction::Inbound,
            PeerConnectionSimpleState::Connecting,
            Some(Vec::new()),
        );
        subject.transition_connected().unwrap();
        {
            let lock = subject.connection_state.read().unwrap();
            match (&*lock).into() {
                PeerConnectionSimpleState::Connected(_) => {},
                s => panic!("Unexpected state '{:?}'", s),
            }
        }

        // Transition listening inbound without identity
        let subject = transition_connected_setup(Direction::Inbound, PeerConnectionSimpleState::Listening(None), None);
        subject.transition_connected().unwrap();
        {
            let lock = subject.connection_state.read().unwrap();
            match (&*lock).into() {
                PeerConnectionSimpleState::Listening(None) => {},
                s => panic!("Unexpected state '{:?}'", s),
            }
        }

        // Transition listening inbound with identity
        let subject = transition_connected_setup(
            Direction::Inbound,
            PeerConnectionSimpleState::Listening(None),
            Some(Vec::new()),
        );
        subject.transition_connected().unwrap();
        {
            let lock = subject.connection_state.read().unwrap();
            match (&*lock).into() {
                PeerConnectionSimpleState::Connected(_) => {},
                s => panic!("Unexpected state '{:?}'", s),
            }
        }

        // Transition listening outbound with identity
        let subject = transition_connected_setup(
            Direction::Outbound,
            PeerConnectionSimpleState::Listening(None),
            Some(Vec::new()),
        );
        match subject.transition_connected().unwrap_err() {
            ConnectionError::PeerError(PeerConnectionError::StateError(_)) => {},
            err => panic!("Unexpected error: {:?}", err),
        }

        // Transition connected to connected
        let subject = transition_connected_setup(Direction::Inbound, PeerConnectionSimpleState::Connected(None), None);
        subject.transition_connected().unwrap();
        {
            let lock = subject.connection_state.read().unwrap();
            match (&*lock).into() {
                PeerConnectionSimpleState::Connected(_) => {},
                s => panic!("Unexpected state '{:?}'", s),
            }
        }
        // Transition from other states
        let subject = transition_connected_setup(Direction::Inbound, PeerConnectionSimpleState::Initial, None);
        match subject.transition_connected().unwrap_err() {
            ConnectionError::PeerError(PeerConnectionError::StateError(_)) => {},
            err => panic!("Unexpected error: {:?}", err),
        }
    }
}

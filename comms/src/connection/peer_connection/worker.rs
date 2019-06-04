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

use log::*;
use std::{
    sync::{
        mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender},
        Arc,
        RwLock,
    },
    thread,
    time::Duration,
};

use crate::{
    connection::{
        connection::{Connection, EstablishedConnection},
        monitor::{ConnectionMonitor, SocketEvent, SocketEventType},
        peer_connection::{
            connection::{PeerConnectionSimpleState, PeerConnectionState},
            control::ControlMessage,
            PeerConnectionContext,
            PeerConnectionError,
        },
        types::Direction,
        ConnectionError,
        InprocAddress,
        Result,
    },
    message::{Frame, FrameSet},
};
use std::thread::JoinHandle;

/// Send HWM for peer connections
const PEER_CONNECTION_SEND_HWM: i32 = 10;
/// Receive HWM for peer connections
const PEER_CONNECTION_RECV_HWM: i32 = 10;

macro_rules! acquire_write_lock {
    ($lock: expr) => {
        $lock.write().map_err(|e| -> ConnectionError {
            PeerConnectionError::StateError(format!("Unable to acquire write lock on PeerConnection state: {}", e))
                .into()
        });
    };
}

/// Worker which:
/// - Establishes a connection to peer
/// - Establishes a connection to the message consumer
/// - Receives and handles ControlMessages
/// - Forwards frames to consumer
/// - Handles SocketEvents and updates shared connection state
pub(super) struct Worker {
    context: PeerConnectionContext,
    sender: SyncSender<ControlMessage>,
    receiver: Receiver<ControlMessage>,
    identity: Option<Frame>,
    paused: bool,
    monitor_addr: InprocAddress,
    connection_state: Arc<RwLock<PeerConnectionState>>,
    retry_count: u16,
}

impl Worker {
    /// Create a new Worker from the given context
    pub fn new(context: PeerConnectionContext, connection_state: Arc<RwLock<PeerConnectionState>>) -> Self {
        let (sender, receiver) = sync_channel(5);
        Self {
            context,
            sender,
            receiver,
            identity: None,
            paused: false,
            monitor_addr: InprocAddress::random(),
            connection_state,
            retry_count: 0,
        }
    }

    /// Spawn a worker thread
    pub fn spawn(mut self) -> (JoinHandle<Result<()>>, SyncSender<ControlMessage>) {
        let sender = self.sender.clone();

        let handle = thread::spawn(move || -> Result<()> {
            let result = self.main_loop();

            // Main loop exited, let's set the shared connection state.
            self.handle_loop_result(result)?;

            Ok(())
        });

        (handle, sender)
    }

    /// Handle the result for the worker loop and update connection state if necessary
    fn handle_loop_result(&mut self, result: Result<()>) -> Result<()> {
        let mut lock = acquire_write_lock!(self.connection_state)?;
        match result {
            Ok(_) => {
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

        loop {
            if let Some(msg) = self.receive_control_msg()? {
                match msg {
                    ControlMessage::Shutdown => break Ok(()),
                    ControlMessage::SendMsg(frames) => {
                        let payload = self.create_payload(frames)?;
                        peer_conn.send(payload)?;
                    },
                    ControlMessage::Pause => {
                        self.paused = true;
                    },
                    ControlMessage::Resume => {
                        self.paused = false;
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

        debug!(target: "comms::peer_connection::worker", "{:?}", event);
        match event.event_type {
            Disconnected => {
                let mut lock = acquire_write_lock!(self.connection_state)?;
                *lock = PeerConnectionState::Disconnected;
            },
            Listening | Accepted | Connected => {
                self.retry_count = 0;
                let mut lock = acquire_write_lock!(self.connection_state)?;
                match *lock {
                    PeerConnectionState::Connecting(ref thread_ctl) => {
                        *lock = PeerConnectionState::Connected(thread_ctl.clone());
                    },
                    PeerConnectionState::Connected(_) => {},
                    ref s => {
                        return Err(PeerConnectionError::StateError(format!(
                            "Unable to transition to connected state from state '{}'",
                            PeerConnectionSimpleState::from(s)
                        ))
                        .into());
                    },
                }
            },
            BindFailed | AcceptFailed | HandshakeFailedNoDetail | HandshakeFailedProtocol | HandshakeFailedAuth => {
                let mut lock = acquire_write_lock!(self.connection_state)?;
                *lock = PeerConnectionState::Failed(PeerConnectionError::ConnectFailed);
            },
            ConnectRetried => {
                let mut lock = acquire_write_lock!(self.connection_state)?;
                match *lock {
                    PeerConnectionState::Connecting(_) => {
                        self.retry_count += 1;
                        if self.retry_count >= self.context.max_retry_attempts {
                            *lock = PeerConnectionState::Failed(PeerConnectionError::ConnectFailed);
                        }
                    },
                    _ => {},
                }
            },
            _ => {},
        }

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
            match context.direction {
                // For a ROUTER backend, the first frame is the identity
                Direction::Inbound => match self.identity {
                    Some(ref ident) => {
                        if frames[0] != *ident {
                            return Err(PeerConnectionError::UnexpectedIdentity.into());
                        }
                    },
                    None => {
                        self.identity = Some(frames[0].clone());
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
        payload.push(self.context.id.clone());
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
        Connection::new(&context.context, Direction::Outbound).establish(&context.consumer_address)
    }
}

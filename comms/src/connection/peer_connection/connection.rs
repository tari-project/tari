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
    control::{ControlMessage, ThreadControlMessenger},
    dialer::PeerConnectionDialer,
    listener::PeerConnectionListener,
    oneshot,
    types::ConnectionInfo,
    PeerConnectionContext,
    PeerConnectionError,
    PeerConnectionJoinHandle,
};
use crate::{
    condvar_shim,
    connection::{types::Linger, zmq::ZmqIdentity, ConnectionDirection, ConnectionError},
    message::{Frame, FrameSet},
    utils::multiaddr::multiaddr_to_socketaddr,
};
use chrono::{NaiveDateTime, Utc};
use multiaddr::Multiaddr;
use std::{
    fmt,
    net::SocketAddr,
    sync::{Arc, Condvar, Mutex, MutexGuard, RwLock},
    time::Duration,
};

const CONTROL_MESSAGE_REPLY_TIMEOUT: Duration = Duration::from_secs(1);

macro_rules! is_state {
    ($name: ident, $($e: pat)|*) => {
	pub fn $name(&self) -> bool {
        use PeerConnectionState::*;
        match self {
            $($e)|* => true,
            _ => false,
        }
	}
    };
}

macro_rules! is_state_unlock {
    ($name: ident) => {
	pub fn $name(&self) -> bool {
	    acquire_lock!(self.state).$name()
	}
    };
}

/// The state of the PeerConnection
pub(super) enum PeerConnectionState {
    /// The connection object has been created but is not connected
    Initial,
    /// The connection thread is running, but the connection has not been accepted
    Connecting(Arc<ThreadControlMessenger>),
    /// The inbound connection is listening for connections
    Listening(Arc<ConnectionInfo>),
    /// The connection thread is running, and has been accepted.
    Connected(Arc<ConnectionInfo>),
    /// The connection has been shut down (node disconnected)
    Shutdown,
    /// The remote peer has disconnected
    Disconnected,
    /// Peer connection runner failed
    Failed(PeerConnectionError),
}

impl Default for PeerConnectionState {
    fn default() -> Self {
        PeerConnectionState::Initial
    }
}

impl PeerConnectionState {
    /// Returns true if the PeerConnection is in an `Initial` state, otherwise false
    is_state!(is_initial, Initial);

    /// Returns true if the PeerConnection is in a `Connected` state for outbound connections, otherwise false
    is_state!(is_connected, Connected(_));

    /// Returns true if the PeerConnection is in a `Shutdown` state, otherwise false
    is_state!(is_shutdown, Shutdown);

    /// Returns true if the PeerConnection is in a `Listening` state, otherwise false
    is_state!(is_listening, Listening(_));

    /// Returns true if the PeerConnection is in a `Disconnected`/`Shutdown`/`Failed` state, otherwise false
    is_state!(is_disconnected, Disconnected | Shutdown | Failed(_));

    /// Returns true if the PeerConnection is in a `Failed` state, otherwise false
    is_state!(is_failed, Failed(_));

    /// Returns true if the PeerConnection is in a `Connecting`, `Listening` or `Connected` state, otherwise false
    is_state!(is_active, Connecting(_) | Connected(_) | Listening(_));

    /// If the connection is in a `Failed` state, the failure error is returned, otherwise `None`
    pub fn failure(&self) -> Option<&PeerConnectionError> {
        match self {
            PeerConnectionState::Failed(err) => Some(err),
            _ => None,
        }
    }
}

/// Basic stats for peer connections. PeerConnectionStats are updated by the [PeerConnectionWorker]
/// and read by the [PeerConnection].
///
/// [PeerConnectionWorker](../worker/struct.PeerConnectionWorker.html)
/// [PeerConnection](./struct.PeerConnection.html)
#[derive(Clone, Debug)]
pub struct PeerConnectionStats {
    last_activity: NaiveDateTime,
    messages_sent: usize,
    messages_recv: usize,
}

impl PeerConnectionStats {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn incr_message_recv(&mut self) {
        self.messages_recv += 1;
        self.last_activity = Utc::now().naive_utc();
    }

    pub fn incr_message_sent(&mut self) {
        self.messages_sent += 1;
        self.last_activity = Utc::now().naive_utc();
    }

    pub fn messages_sent(&self) -> usize {
        self.messages_sent
    }

    pub fn messages_recv(&self) -> usize {
        self.messages_recv
    }

    pub fn last_activity(&self) -> &NaiveDateTime {
        &self.last_activity
    }
}

impl Default for PeerConnectionStats {
    fn default() -> Self {
        Self {
            last_activity: Utc::now().naive_local(),
            messages_sent: 0,
            messages_recv: 0,
        }
    }
}

/// Represents an asynchonous bi-directional connection to a Peer.
/// A PeerConnectionContext must be given to start the underlying thread
/// This may be easily shared and cloned across threads
///
/// # Fields
///
/// `state` - current state of the thread
///
/// # Example
///
/// ```edition2018
/// 
/// # use tari_comms::connection::*;
/// # use std::time::Duration;
/// # use futures::channel::mpsc::channel;
/// let ctx = ZmqContext::new();
/// let addr = "/ip4/127.0.0.1/tcp/8080".parse().unwrap();
/// let (message_sink_tx, _message_sink_rx) = channel(10);
/// let peer_context = PeerConnectionContextBuilder::new()
///    .set_peer_identity(b"peer-identifier-bytes".to_vec())
///    .set_connection_identity(b"zmq-identity-to-use-for-conn".to_vec())
///    .set_context(&ctx)
///    .set_direction(ConnectionDirection::Outbound)
///    .set_message_sink_channel(message_sink_tx)
///    .set_address(addr)
///    .finish()
///    .unwrap();
///
/// // Start the peer connection worker thread
/// let (conn, _thread_handle) = PeerConnection::connect(peer_context).unwrap();
///
/// // Wait for connection
/// // This will never connect because there is nothing
/// // listening on the other end
/// match conn.wait_connected_or_failure(Duration::from_millis(100)) {
///   Ok(()) => {
///     assert!(conn.is_connected());
///     println!("Connection established");
///   }
///   Err(err) => {
///     assert!(!conn.is_connected());
///     println!("Failed to connect after 100ms (may still be trying if err is Timeout). Error: {:?}", err);
///   }
/// }
/// ```
#[derive(Clone)]
pub struct PeerConnection {
    state: Arc<Mutex<PeerConnectionState>>,
    connection_stats: Arc<RwLock<PeerConnectionStats>>,
    direction: ConnectionDirection,
    peer_address: Multiaddr,
    state_var: Arc<Condvar>,
}

impl PeerConnection {
    /// Returns true if the PeerConnection is in an `Initial` state, otherwise false
    is_state_unlock!(is_initial);

    /// Returns true if the PeerConnection is in a `Connected` state for outbound connections, otherwise false
    is_state_unlock!(is_connected);

    /// Returns true if the PeerConnection is in a `Shutdown` state, otherwise false
    is_state_unlock!(is_shutdown);

    /// Returns true if the PeerConnection is in a `Listening` state, otherwise false
    is_state_unlock!(is_listening);

    /// Returns true if the PeerConnection is in a `Disconnected`/`Shutdown`/`Failed` state, otherwise false
    is_state_unlock!(is_disconnected);

    /// Returns true if the PeerConnection is in a `Failed` state, otherwise false
    is_state_unlock!(is_failed);

    /// Returns true if the PeerConnection is in a `Connecting`, `Listening` or `Connected` state, otherwise false
    is_state_unlock!(is_active);

    /// Start a connecting (dialer) worker thread for the PeerConnection and begin connecting to
    /// the address in the `context`
    ///
    /// # Arguments
    ///
    /// `context` - The PeerConnectionContext which is owned by the underlying thread
    pub fn connect(context: PeerConnectionContext) -> Result<(Self, PeerConnectionJoinHandle), PeerConnectionError> {
        let conn = Self {
            direction: ConnectionDirection::Outbound,
            peer_address: context.peer_address.clone(),
            state_var: Arc::new(Condvar::new()),
            state: Arc::new(Mutex::new(PeerConnectionState::Initial)),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
        };

        let worker = PeerConnectionDialer::new(
            context,
            Arc::clone(&conn.state),
            Arc::clone(&conn.connection_stats),
            Arc::clone(&conn.state_var),
        );

        let handle = worker.spawn()?;
        Ok((conn, handle))
    }

    /// Start the worker thread for the PeerConnection and transition the
    /// state to PeerConnectionState::Connected. The PeerConnection now
    /// has a ThreadMessenger which is used to send ControlMessages to the
    /// underlying thread.
    ///
    /// # Arguments
    ///
    /// `context` - The PeerConnectionContext which is owned by the underlying thread
    pub fn listen(context: PeerConnectionContext) -> Result<(Self, PeerConnectionJoinHandle), PeerConnectionError> {
        let conn = Self {
            direction: ConnectionDirection::Inbound,
            peer_address: context.peer_address.clone(),
            state_var: Arc::new(Condvar::new()),
            state: Arc::new(Mutex::new(PeerConnectionState::Initial)),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
        };

        let worker = PeerConnectionListener::new(
            context,
            Arc::clone(&conn.state),
            Arc::clone(&conn.connection_stats),
            Arc::clone(&conn.state_var),
        );

        let handle = worker.spawn()?;
        Ok((conn, handle))
    }

    /// Tell the underlying thread to shut down. The connection will not immediately
    /// be in a `Shutdown` state. [wait_shutdown] can be used to wait for the
    /// connection to shut down. If the connection is not active, this method does nothing.
    pub fn shutdown(&self) -> Result<(), PeerConnectionError> {
        match self.send_control_message(ControlMessage::Shutdown) {
            // StateError only returns from send_control_message
            // if the connection worker is not active
            Ok(_) | Err(PeerConnectionError::StateError(_)) => Ok(()),
            e => e,
        }
    }

    /// Send frames to the connected Peer. An Err will be returned if the
    /// connection is not in a Connected state.
    ///
    /// # Arguments
    ///
    /// `identity` - The identity to send to.
    /// `frames` - The frames to send
    pub fn send_to_identity(&self, peer_identity: ZmqIdentity, frames: FrameSet) -> Result<(), PeerConnectionError> {
        self.send_control_message_sendmsg(Some(peer_identity), frames)
    }

    /// Send frames to the connected Peer. An Err will be returned if the
    /// connection is not in a Connected state.
    ///
    /// # Arguments
    ///
    /// `frames` - The frames to send
    pub fn send(&self, frames: FrameSet) -> Result<(), PeerConnectionError> {
        self.send_control_message_sendmsg(None, frames)
    }

    /// Send frames to the connected Peer. An Err will be returned if the
    /// connection is not in a Connected state.
    fn send_control_message_sendmsg(
        &self,
        peer_identity: Option<ZmqIdentity>,
        frames: FrameSet,
    ) -> Result<(), PeerConnectionError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_control_message(ControlMessage::SendMsg(peer_identity, frames, reply_tx))?;
        reply_rx
            .recv_timeout(CONTROL_MESSAGE_REPLY_TIMEOUT)
            .map_err(|_| PeerConnectionError::ControlMessageReplyFailed)?
            .ok_or_else(|| {
                PeerConnectionError::OperationTimeout(
                    "Peer connection failed to send reply to send message request".to_string(),
                )
            })?
    }

    /// Return a PeerSender that is used to send multiple messages to a particular peer
    pub fn get_peer_sender(&self, identity: ZmqIdentity) -> Result<PeerSender, PeerConnectionError> {
        let messenger = self.get_control_messenger()?;
        Ok(PeerSender::new(messenger, identity))
    }

    /// Set the linger for the connection
    ///
    /// # Arguments
    ///
    /// `linger` - The Linger to set
    pub fn set_linger(&self, linger: Linger) -> Result<(), PeerConnectionError> {
        self.send_control_message(ControlMessage::SetLinger(linger))
    }

    /// Allow an identity to send to this connection. Applies to inbound connections only
    pub fn allow_identity(
        &self,
        connection_identity: ZmqIdentity,
        peer_identity: Frame,
    ) -> Result<(), PeerConnectionError>
    {
        self.send_control_message(ControlMessage::AllowIdentity(connection_identity, peer_identity))
    }

    /// Deny an identity from sending to this connection. Applies to inbound connections only
    pub fn deny_identity(&self, peer_identity: ZmqIdentity) -> Result<(), PeerConnectionError> {
        self.send_control_message(ControlMessage::DenyIdentity(peer_identity))
    }

    pub fn test_connection(&self, peer_identity: ZmqIdentity) -> Result<(), PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.send_control_message(ControlMessage::TestConnection(peer_identity, reply_tx))?;
        reply_rx
            .recv_timeout(CONTROL_MESSAGE_REPLY_TIMEOUT)
            .map_err(|_| {
                PeerConnectionError::InvalidOperation(
                    "Peer connection dropped the reply sender before responding.".to_string(),
                )
            })?
            .ok_or_else(|| {
                PeerConnectionError::OperationTimeout(
                    "Peer connection worker failed to response within 10s.".to_string(),
                )
            })?
    }

    /// Return the actual address this connection is bound to. If the connection state is not Connected,
    /// this function returns None
    pub fn get_address(&self) -> Option<SocketAddr> {
        let lock = acquire_lock!(self.state);
        match &*lock {
            PeerConnectionState::Listening(info) | PeerConnectionState::Connected(info) => info
                .connected_address
                .as_ref()
                .map_or(Some(multiaddr_to_socketaddr(&self.peer_address).ok()?), |addr| {
                    Some(*addr)
                }),
            _ => None,
        }
    }

    /// Returns a snapshot of latest connection stats from this peer connection
    pub fn get_connection_stats(&self) -> PeerConnectionStats {
        acquire_read_lock!(self.connection_stats).clone()
    }

    /// Returns the last time this connection sent or received a message
    pub fn last_activity(&self) -> NaiveDateTime {
        *acquire_read_lock!(self.connection_stats).last_activity()
    }

    /// Send control message to the ThreadControlMessenger.
    /// Will return an error if the connection is not in an active state.
    ///
    /// # Arguments
    ///
    /// `msg` - The ControlMessage to send
    fn send_control_message(&self, msg: ControlMessage) -> Result<(), PeerConnectionError> {
        use PeerConnectionState::*;
        let lock = acquire_lock!(self.state);
        match &*lock {
            Connecting(ref thread_ctl) => thread_ctl.send(msg),
            Listening(ref info) => info.control_messenger.send(msg),
            Connected(ref info) => info.control_messenger.send(msg),
            state => Err(PeerConnectionError::StateError(format!(
                "Attempt to retrieve thread messenger on peer connection with state '{}'",
                PeerConnectionSimpleState::from(state)
            ))),
        }
    }

    fn get_control_messenger(&self) -> Result<Arc<ThreadControlMessenger>, PeerConnectionError> {
        use PeerConnectionState::*;
        let lock = acquire_lock!(self.state);
        match &*lock {
            Connecting(ref thread_ctl) => Ok(Arc::clone(thread_ctl)),
            Listening(ref info) => Ok(Arc::clone(&info.control_messenger)),
            Connected(ref info) => Ok(Arc::clone(&info.control_messenger)),
            state => Err(PeerConnectionError::StateError(format!(
                "Attempt to retrieve thread messenger on peer connection with state '{}'",
                PeerConnectionSimpleState::from(state)
            ))),
        }
    }

    /// Blocks the current thread until the connection is in a `Connected` state (returning `Ok`),
    /// the timeout has been reached (returning `Err(ConnectionError::Timeout)`), or the connection
    /// is in a `Failed` state (returning the error which caused the failure)
    pub fn wait_listening_or_failure(&self, until: Duration) -> Result<(), PeerConnectionError> {
        if self.direction().is_outbound() {
            return Err(PeerConnectionError::InvalidOperation(
                "Call to wait_listening_or_failure on Outbound connection".to_string(),
            ));
        }

        let guard = self.wait_until(until, |state| !state.is_active() || state.is_listening())?;
        if guard.is_listening() {
            Ok(())
        } else {
            match guard.failure() {
                Some(err) => Err(PeerConnectionError::OperationFailed(format!(
                    "Connection failed to enter 'Listening' state within {}ms because '{}'",
                    until.as_millis(),
                    err
                ))),
                None => Err(PeerConnectionError::OperationTimeout(format!(
                    "Connection failed to enter 'Listening' state within {}ms",
                    until.as_millis()
                ))),
            }
        }
    }

    /// Blocks the current thread until the connection is in a `Connected` state (returning `Ok`),
    /// the timeout has been reached (returning `Err(ConnectionError::Timeout)`), or the connection
    /// is in a `Failed` state (returning the error which caused the failure)
    pub fn wait_connected_or_failure(&self, until: Duration) -> Result<(), PeerConnectionError> {
        let guard = self.wait_until(until, |guard| !guard.is_active() || guard.is_connected())?;
        if guard.is_connected() {
            Ok(())
        } else {
            match guard.failure() {
                Some(err) => Err(PeerConnectionError::OperationFailed(format!(
                    "Connection failed to enter 'Connected' state within {}ms because '{}'",
                    until.as_millis(),
                    err
                ))),
                None => Err(PeerConnectionError::OperationTimeout(format!(
                    "Connection failed to enter 'Connected' state within {}ms",
                    until.as_millis()
                ))),
            }
        }
    }

    /// Blocks the current thread until the connection is in a `Shutdown` or `Disconnected` state (Ok) or
    /// the timeout is reached (Err).
    pub fn wait_disconnected(&self, until: Duration) -> Result<(), PeerConnectionError> {
        let _ = self.wait_until(until, |lock| lock.is_disconnected())?;
        Ok(())
    }

    /// Returns the connection state without the ThreadControlMessenger
    /// which should never be leaked.
    pub fn get_state(&self) -> PeerConnectionSimpleState {
        let lock = acquire_lock!(self.state);
        PeerConnectionSimpleState::from(&*lock)
    }

    /// Gets the direction for this peer connection
    pub fn direction(&self) -> ConnectionDirection {
        self.direction
    }

    /// Waits until the condition returns true or the timeout (`until`) is reached.
    /// If the timeout was reached, an `Err(ConnectionError::Timeout)` is returned, otherwise `Ok(())`
    fn wait_until(
        &self,
        until: Duration,
        predicate: impl Fn(&mut PeerConnectionState) -> bool,
    ) -> Result<MutexGuard<PeerConnectionState>, PeerConnectionError>
    {
        let guard = acquire_lock!(self.state);
        let (guard, is_timeout) = recover_lock!(condvar_shim::wait_timeout_until(
            &self.state_var,
            guard,
            until,
            predicate
        ));
        if is_timeout {
            Err(ConnectionError::Timeout.into())
        } else {
            Ok(guard)
        }
    }

    #[cfg(test)]
    pub fn new_with_connecting_state_for_test(
        peer_address: Multiaddr,
    ) -> (Self, std::sync::mpsc::Receiver<ControlMessage>) {
        use std::sync::mpsc::sync_channel;
        let (tx, rx) = sync_channel(1);
        (
            Self {
                state: Arc::new(Mutex::new(PeerConnectionState::Connecting(Arc::new(tx.into())))),
                direction: ConnectionDirection::Outbound,
                state_var: Arc::new(Condvar::new()),
                connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
                peer_address,
            },
            rx,
        )
    }
}

pub struct PeerSender {
    messenger: Arc<ThreadControlMessenger>,
    identity: ZmqIdentity,
}

impl PeerSender {
    fn new(messenger: Arc<ThreadControlMessenger>, identity: ZmqIdentity) -> Self {
        Self { messenger, identity }
    }

    /// Send frames to the connected Peer. An Err will be returned if the
    /// connection is not in a Connected state.
    ///
    /// # Arguments
    ///
    /// `frames` - The frames to send
    pub fn send(&self, frames: FrameSet) -> Result<(), PeerConnectionError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.messenger
            .send(ControlMessage::SendMsg(Some(self.identity.clone()), frames, reply_tx))?;
        reply_rx
            .recv_timeout(CONTROL_MESSAGE_REPLY_TIMEOUT)
            .map_err(|_| PeerConnectionError::ControlMessageReplyFailed)?
            .ok_or_else(|| {
                PeerConnectionError::OperationTimeout(
                    "Peer connection failed to send reply to send message request".to_string(),
                )
            })?
    }
}

/// Represents the states that a peer connection can be in without
/// exposing ThreadControlMessenger which should not be leaked.
#[derive(Debug)]
pub enum PeerConnectionSimpleState {
    /// The connection object has been created but is not connected
    Initial,
    /// The connection thread is running, but the connection has not been accepted
    Connecting,
    /// The connection is listening, and has been not been accepted.
    Listening(Option<SocketAddr>),
    /// The connection is connected, and has been accepted.
    Connected(Option<SocketAddr>),
    /// The connection has been shut down (node disconnected)
    Shutdown,
    /// The remote peer has disconnected
    Disconnected,
    /// Peer connection failed
    Failed(String),
}

impl From<&PeerConnectionState> for PeerConnectionSimpleState {
    fn from(state: &PeerConnectionState) -> Self {
        match state {
            PeerConnectionState::Initial => PeerConnectionSimpleState::Initial,
            PeerConnectionState::Connecting(_) => PeerConnectionSimpleState::Connecting,
            PeerConnectionState::Listening(info) => PeerConnectionSimpleState::Listening(info.connected_address),
            PeerConnectionState::Connected(info) => PeerConnectionSimpleState::Connected(info.connected_address),
            PeerConnectionState::Shutdown => PeerConnectionSimpleState::Shutdown,
            PeerConnectionState::Disconnected => PeerConnectionSimpleState::Disconnected,
            PeerConnectionState::Failed(e) => PeerConnectionSimpleState::Failed(format!("{}", e)),
        }
    }
}

impl fmt::Display for PeerConnectionSimpleState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use PeerConnectionSimpleState::*;
        match *self {
            Initial => write!(f, "Initial"),
            Connecting => write!(f, "Connecting"),
            Listening(Some(ref addr)) => write!(f, "Listening on {}", addr),
            Listening(None) => write!(f, "Listening on non TCP socket"),
            Connected(Some(ref addr)) => write!(f, "Connected to {}", addr),
            Connected(None) => write!(f, "Connected to non TCP socket"),
            Shutdown => write!(f, "Shutdown"),
            Disconnected => write!(f, "Disconnected"),
            Failed(ref err) => write!(f, "Failed({})", err),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{
        sync::{
            mpsc::{sync_channel, Receiver},
            Arc,
        },
        thread,
    };

    fn create_thread_ctl() -> (Arc<ThreadControlMessenger>, Receiver<ControlMessage>) {
        let (tx, rx) = sync_channel::<ControlMessage>(1);
        (Arc::new(tx.into()), rx)
    }

    #[test]
    fn state_display() {
        let addr = "127.0.0.1:8000".parse().ok();
        assert_eq!("Initial", format!("{}", PeerConnectionSimpleState::Initial));
        assert_eq!("Connecting", format!("{}", PeerConnectionSimpleState::Connecting));
        assert_eq!(
            "Connected to non TCP socket",
            format!("{}", PeerConnectionSimpleState::Connected(None))
        );
        assert_eq!(
            "Connected to 127.0.0.1:8000",
            format!("{}", PeerConnectionSimpleState::Connected(addr))
        );
        assert_eq!("Shutdown", format!("{}", PeerConnectionSimpleState::Shutdown));
        assert_eq!(
            format!("Failed({})", PeerConnectionError::ConnectFailed),
            format!(
                "{}",
                PeerConnectionSimpleState::Failed(PeerConnectionError::ConnectFailed.to_string())
            )
        );
    }

    #[test]
    fn state_connected() {
        let (thread_ctl, _) = create_thread_ctl();

        let info = ConnectionInfo {
            control_messenger: thread_ctl,
            connected_address: Some("127.0.0.1:1000".parse().unwrap()),
        };
        let conn = PeerConnection {
            state: Arc::new(Mutex::new(PeerConnectionState::Connected(Arc::new(info)))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: ConnectionDirection::Outbound,
            peer_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            state_var: Default::default(),
        };

        assert!(conn.is_connected());
        assert!(!conn.is_listening());
        assert!(!conn.is_disconnected());
        assert!(conn.is_active());
        assert!(!conn.is_shutdown());
        assert!(!conn.is_failed());
    }

    #[test]
    fn state_listening() {
        let (thread_ctl, _) = create_thread_ctl();

        let info = ConnectionInfo {
            control_messenger: thread_ctl,
            connected_address: Some("127.0.0.1:1000".parse().unwrap()),
        };
        let conn = PeerConnection {
            state: Arc::new(Mutex::new(PeerConnectionState::Listening(Arc::new(info)))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: ConnectionDirection::Outbound,
            peer_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            state_var: Default::default(),
        };

        assert!(!conn.is_connected());
        assert!(conn.is_listening());
        assert!(!conn.is_disconnected());
        assert!(conn.is_active());
        assert!(!conn.is_shutdown());
        assert!(!conn.is_failed());
    }

    #[test]
    fn state_connecting() {
        let (thread_ctl, _) = create_thread_ctl();

        let conn = PeerConnection {
            state: Arc::new(Mutex::new(PeerConnectionState::Connecting(thread_ctl))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: ConnectionDirection::Outbound,
            peer_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            state_var: Default::default(),
        };

        assert!(!conn.is_connected());
        assert!(!conn.is_listening());
        assert!(!conn.is_disconnected());
        assert!(conn.is_active());
        assert!(!conn.is_shutdown());
        assert!(!conn.is_failed());
    }

    #[test]
    fn state_active() {
        let (thread_ctl, _) = create_thread_ctl();

        let conn = PeerConnection {
            state: Arc::new(Mutex::new(PeerConnectionState::Connecting(thread_ctl))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: ConnectionDirection::Outbound,
            peer_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            state_var: Default::default(),
        };

        assert!(!conn.is_connected());
        assert!(!conn.is_listening());
        assert!(!conn.is_disconnected());
        assert!(conn.is_active());
        assert!(!conn.is_shutdown());
        assert!(!conn.is_failed());
    }

    #[test]
    fn state_failed() {
        let conn = PeerConnection {
            state: Arc::new(Mutex::new(PeerConnectionState::Failed(
                PeerConnectionError::ConnectFailed,
            ))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: ConnectionDirection::Outbound,
            peer_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            state_var: Default::default(),
        };

        assert!(!conn.is_connected());
        assert!(!conn.is_listening());
        assert!(conn.is_disconnected());
        assert!(!conn.is_active());
        assert!(!conn.is_shutdown());
        assert!(conn.is_failed());
    }

    #[test]
    fn state_disconnected() {
        let conn = PeerConnection {
            state: Arc::new(Mutex::new(PeerConnectionState::Disconnected)),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: ConnectionDirection::Outbound,
            peer_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            state_var: Default::default(),
        };

        assert!(!conn.is_connected());
        assert!(!conn.is_listening());
        assert!(conn.is_disconnected());
        assert!(!conn.is_active());
        assert!(!conn.is_shutdown());
        assert!(!conn.is_failed());
    }

    #[test]
    fn state_shutdown() {
        let conn = PeerConnection {
            state: Arc::new(Mutex::new(PeerConnectionState::Shutdown)),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: ConnectionDirection::Outbound,
            peer_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            state_var: Default::default(),
        };

        assert!(!conn.is_connected());
        assert!(!conn.is_listening());
        assert!(conn.is_disconnected());
        assert!(!conn.is_active());
        assert!(conn.is_shutdown());
        assert!(!conn.is_failed());
    }

    fn create_connected_peer_connection() -> (PeerConnection, Receiver<ControlMessage>) {
        let (thread_ctl, rx) = create_thread_ctl();
        let info = ConnectionInfo {
            control_messenger: thread_ctl,
            connected_address: Some("127.0.0.1:1000".parse().unwrap()),
        };
        let conn = PeerConnection {
            state: Arc::new(Mutex::new(PeerConnectionState::Connected(Arc::new(info)))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: ConnectionDirection::Outbound,
            peer_address: "/ip4/127.0.0.1/tcp/0".parse().unwrap(),
            state_var: Default::default(),
        };
        (conn, rx)
    }

    #[test]
    fn send() {
        let (conn, rx) = create_connected_peer_connection();

        let sample_frames = vec![vec![123u8]];
        let sample_frames_inner = sample_frames.clone();
        thread::spawn(move || {
            conn.send(sample_frames_inner).unwrap();
        });
        let msg = rx.recv_timeout(Duration::from_millis(10)).unwrap();
        match msg {
            ControlMessage::SendMsg(identity, frames, reply_tx) => {
                assert_eq!(sample_frames, frames);
                assert!(identity.is_none());
                reply_tx.send(Ok(())).unwrap();
            },
            m => panic!("Unexpected control message '{}'", m),
        }
    }

    #[test]
    fn send_with_identity() {
        let (conn, rx) = create_connected_peer_connection();

        let sample_frames = vec![vec![123u8]];
        let sample_frames_inner = sample_frames.clone();
        thread::spawn(move || {
            conn.send_to_identity(b"123".to_vec(), sample_frames_inner).unwrap();
        });
        let msg = rx.recv_timeout(Duration::from_millis(10)).unwrap();
        match msg {
            ControlMessage::SendMsg(identity, frames, reply_tx) => {
                assert_eq!(sample_frames, frames);
                assert_eq!(identity, Some(b"123".to_vec()));
                reply_tx.send(Ok(())).unwrap();
            },
            m => panic!("Unexpected control message '{}'", m),
        }
    }

    #[test]
    fn shutdown() {
        let (conn, rx) = create_connected_peer_connection();

        conn.shutdown().unwrap();
        let msg = rx.recv_timeout(Duration::from_millis(10)).unwrap();
        match msg {
            ControlMessage::Shutdown => {},
            _ => panic!("received unexpected message"),
        }
    }

    #[test]
    fn connection_stats() {
        let (conn, _) = create_connected_peer_connection();

        let stats = conn.get_connection_stats();
        assert_eq!(stats.messages_recv, 0);
        assert_eq!(stats.messages_sent, 0);
    }
}

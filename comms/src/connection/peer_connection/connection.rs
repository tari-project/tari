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
    worker::PeerConnectionWorker,
    PeerConnectionContext,
    PeerConnectionError,
};
use crate::{
    connection::{
        net_address::ip::SocketAddress,
        types::{Linger, Result},
        ConnectionError,
        Direction,
        NetAddress,
    },
    message::FrameSet,
};
use chrono::{NaiveDateTime, Utc};
use std::{
    fmt,
    sync::{Arc, RwLock},
    thread::{self, JoinHandle},
    time::Duration,
};
use tari_utilities::hex::to_hex;

/// Represents the ID of a PeerConnection. This is sent as the first frame
/// to the message sink on the peer connection.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ConnectionId(Vec<u8>);

impl ConnectionId {
    pub fn new(id: Vec<u8>) -> Self {
        Self(id)
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    /// Returns a shortened (length of 8 or less) connection ID
    /// This would typically be used for display purposes when the connection ID is a
    /// sufficiently large random value and you don't want to have large strings displayed.
    pub fn to_short_id(&self) -> Self {
        let start = match self.0.len().checked_sub(8) {
            Some(s) => s,
            None => self.0.len(),
        };
        Self(self.0[start..].to_vec())
    }
}

impl Default for ConnectionId {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl PartialEq<ConnectionId> for Vec<u8> {
    fn eq(&self, other: &ConnectionId) -> bool {
        self == &other.0
    }
}

impl From<Vec<u8>> for ConnectionId {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl From<&[u8]> for ConnectionId {
    fn from(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }
}

impl From<&str> for ConnectionId {
    fn from(bytes: &str) -> Self {
        Self(bytes.as_bytes().to_vec())
    }
}

impl AsRef<[u8]> for ConnectionId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl fmt::Display for ConnectionId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", to_hex(self.as_bytes()))
    }
}

pub struct ConnectionInfo {
    pub(super) control_messenger: Arc<ThreadControlMessenger>,
    pub(super) connected_address: Option<SocketAddress>,
}

/// The state of the PeerConnection
#[derive(Clone)]
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

macro_rules! is_state {
    ($name: ident, $($e: pat)|*) => {
	pub fn $name(&self) -> bool {
        use PeerConnectionState::*;
	    let lock = acquire_read_lock!(self.state);
        match *lock {
            $($e)|* => true,
            _ => false,
        }
	}
    };
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
///
/// let ctx = ZmqContext::new();
/// let addr: NetAddress = "127.0.0.1:8080".parse().unwrap();
///
/// let peer_context = PeerConnectionContextBuilder::new()
///    .set_id("123")
///    .set_context(&ctx)
///    .set_direction(Direction::Outbound)
///    .set_message_sink_address(InprocAddress::random())
///    .set_address(addr.clone())
///    .build()
///    .unwrap();
///
/// let mut conn = PeerConnection::new();
///
/// assert!(!conn.is_connected());
/// // Start the peer connection worker thread
/// conn.start(peer_context).unwrap();
/// // Wait for connection
/// // This will never connect because there is nothing
/// // listening on the other end
/// match conn.wait_connected_or_failure(&Duration::from_millis(100)) {
///   Ok(()) => {
///     assert!(conn.is_connected());
///     println!("Able to establish connection on {}", addr);
///   }
///   Err(err) => {
///     assert!(!conn.is_connected());
///     println!("Failed to connect to {} after 100ms (may still be trying if err is Timeout). Error: {:?}", addr, err);
///   }
/// }
/// ```
#[derive(Default, Clone)]
pub struct PeerConnection {
    state: Arc<RwLock<PeerConnectionState>>,
    connection_stats: Arc<RwLock<PeerConnectionStats>>,
    direction: Option<Direction>,
    peer_address: Option<NetAddress>,
}

impl PeerConnection {
    /// Returns true if the PeerConnection is in a connected state, otherwise false
    is_state!(is_connected, Connected(_));

    /// Returns true if the PeerConnection is in a shutdown state, otherwise false
    is_state!(is_shutdown, Shutdown);

    /// Returns true if the PeerConnection is in a listening state, otherwise false
    is_state!(is_listening, Listening(_));

    /// Returns true if the PeerConnection is in a `Disconnected`/`Shutdown`/`Failed` state, otherwise false
    is_state!(is_disconnected, Disconnected | Shutdown | Failed(_));

    /// Returns true if the PeerConnection is in a failed state, otherwise false
    is_state!(is_failed, Failed(_));

    /// Returns true if the PeerConnection is in a connecting, listening or connected state, otherwise false
    is_state!(is_active, Connecting(_) | Connected(_) | Listening(_));

    /// Create a new PeerConnection
    pub fn new() -> Self {
        Default::default()
    }

    /// Start the worker thread for the PeerConnection and transition the
    /// state to PeerConnectionState::Connected. The PeerConnection now
    /// has a ThreadMessenger which is used to send ControlMessages to the
    /// underlying thread.
    ///
    /// # Arguments
    ///
    /// `context` - The PeerConnectionContext which is owned by the underlying thread
    pub fn start(&mut self, context: PeerConnectionContext) -> Result<JoinHandle<Result<()>>> {
        self.direction = Some(context.direction.clone());
        self.peer_address = Some(context.peer_address.clone());

        let worker = PeerConnectionWorker::new(context, self.state.clone(), self.connection_stats.clone());
        let handle = worker.spawn()?;
        Ok(handle)
    }

    /// Tell the underlying thread to shut down. The connection will not immediately
    /// be in a `Shutdown` state. [wait_shutdown] can be used to wait for the
    /// connection to shut down.
    pub fn shutdown(&self) -> Result<()> {
        self.send_control_message(ControlMessage::Shutdown)
    }

    /// Send frames to the connected Peer. An Err will be returned if the
    /// connection is not in a Connected state.
    ///
    /// # Arguments
    ///
    /// `frames` - The frames to send
    pub fn send(&self, frames: FrameSet) -> Result<()> {
        self.send_control_message(ControlMessage::SendMsg(frames))
    }

    /// Set the linger for the connection
    ///
    /// # Arguments
    ///
    /// `linger` - The Linger to set
    pub fn set_linger(&self, linger: Linger) -> Result<()> {
        self.send_control_message(ControlMessage::SetLinger(linger))
    }

    /// Temporarily suspend messages from being processed and forwarded to the consumer.
    /// Pending messages will be buffered until reaching the receive HWM. Once resumed,
    /// buffered messages will be released to the consumer.
    /// An Err will be returned if the connection is not in a Connected state.
    pub fn pause(&self) -> Result<()> {
        self.send_control_message(ControlMessage::Pause)
    }

    /// Unpause the connection and resume message processing from the peer.
    /// An Err will be returned if the connection is not in a Connected state.
    pub fn resume(&self) -> Result<()> {
        self.send_control_message(ControlMessage::Resume)
    }

    /// Return the actual address this connection is bound to. If the connection is not over a TCP socket, or the
    /// connection state is not Connected, this function returns None
    pub fn get_connected_address(&self) -> Option<SocketAddress> {
        let lock = acquire_read_lock!(self.state);
        match &*lock {
            PeerConnectionState::Listening(info) | PeerConnectionState::Connected(info) => {
                info.connected_address.clone()
            },
            _ => None,
        }
    }

    /// Return the actual address this connection is bound to. If the connection state is not Connected,
    /// this function returns None
    pub fn get_address(&self) -> Option<NetAddress> {
        let lock = acquire_read_lock!(self.state);
        match &*lock {
            PeerConnectionState::Listening(info) | PeerConnectionState::Connected(info) => info
                .connected_address
                .clone()
                .map_or(self.peer_address.clone(), |addr| Some(addr.into())),
            _ => None,
        }
    }

    /// Returns a snapshot of latest connection stats from this peer connection
    pub fn connection_stats(&self) -> PeerConnectionStats {
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
    fn send_control_message(&self, msg: ControlMessage) -> Result<()> {
        use PeerConnectionState::*;
        let lock = acquire_read_lock!(self.state);
        match &*lock {
            Connecting(ref thread_ctl) => thread_ctl.send(msg),
            Listening(ref info) => info.control_messenger.send(msg),
            Connected(ref info) => info.control_messenger.send(msg),
            state => Err(PeerConnectionError::StateError(format!(
                "Attempt to retrieve thread messenger on peer connection with state '{}'",
                PeerConnectionSimpleState::from(state)
            ))
            .into()),
        }
    }

    /// Blocks the current thread until the connection is in a `Connected` state (returning `Ok`),
    /// the timeout has been reached (returning `Err(ConnectionError::Timeout)`), or the connection
    /// is in a `Failed` state (returning the error which caused the failure)
    pub fn wait_listening_or_failure(&self, until: &Duration) -> Result<()> {
        match self.get_direction() {
            Some(direction) => {
                if *direction == Direction::Outbound {
                    return Err(ConnectionError::InvalidOperation(
                        "Call to wait_listening_or_failure on Outbound connection".to_string(),
                    ));
                }
            },
            None => {
                return Err(ConnectionError::InvalidOperation(
                    "Call to wait_listening_or_failure before peer connection has started".to_string(),
                ));
            },
        }
        self.wait_until(until, || !self.is_active() || self.is_listening())?;
        if self.is_listening() {
            Ok(())
        } else {
            match self.failure() {
                Some(err) => Err(err),
                None => Err(ConnectionError::Timeout),
            }
        }
    }

    /// Blocks the current thread until the connection is in a `Connected` state (returning `Ok`),
    /// the timeout has been reached (returning `Err(ConnectionError::Timeout)`), or the connection
    /// is in a `Failed` state (returning the error which caused the failure)
    pub fn wait_connected_or_failure(&self, until: &Duration) -> Result<()> {
        self.wait_until(until, || !self.is_active() || self.is_connected())?;
        if self.is_connected() {
            Ok(())
        } else {
            match self.failure() {
                Some(err) => Err(err),
                None => Err(ConnectionError::Timeout),
            }
        }
    }

    /// Blocks the current thread until the connection is in a `Shutdown` or `Disconnected` state (Ok) or
    /// the timeout is reached (Err).
    pub fn wait_disconnected(&self, until: &Duration) -> Result<()> {
        self.wait_until(until, || self.is_disconnected())
    }

    /// If the connection is in a `Failed` state, the failure error is returned, otherwise `None`
    pub fn failure(&self) -> Option<ConnectionError> {
        let lock = acquire_read_lock!(self.state);
        match &*lock {
            PeerConnectionState::Failed(err) => Some(err.clone().into()),
            _ => None,
        }
    }

    /// Returns the connection state without the ThreadControlMessenger
    /// which should never be leaked.
    pub fn get_state(&self) -> PeerConnectionSimpleState {
        let lock = acquire_read_lock!(self.state);
        PeerConnectionSimpleState::from(&*lock)
    }

    /// Gets the direction for this peer connection
    pub fn get_direction(&self) -> &Option<Direction> {
        &self.direction
    }

    /// Waits until the condition returns true or the timeout (`until`) is reached.
    /// If the timeout was reached, an `Err(ConnectionError::Timeout)` is returned, otherwise `Ok(())`
    fn wait_until(&self, until: &Duration, condition: impl Fn() -> bool) -> Result<()> {
        let mut count = 0;
        let timeout_ms = until.as_millis();
        while !condition() && count < timeout_ms {
            thread::sleep(Duration::from_millis(1));
            count += 1;
        }

        if count < timeout_ms {
            Ok(())
        } else {
            Err(ConnectionError::Timeout)
        }
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
    Listening(Option<SocketAddress>),
    /// The connection is connected, and has been accepted.
    Connected(Option<SocketAddress>),
    /// The connection has been shut down (node disconnected)
    Shutdown,
    /// The remote peer has disconnected
    Disconnected,
    /// Peer connection failed
    Failed(PeerConnectionError),
}

impl From<&PeerConnectionState> for PeerConnectionSimpleState {
    fn from(state: &PeerConnectionState) -> Self {
        match state {
            PeerConnectionState::Initial => PeerConnectionSimpleState::Initial,
            PeerConnectionState::Connecting(_) => PeerConnectionSimpleState::Connecting,
            PeerConnectionState::Listening(info) => {
                PeerConnectionSimpleState::Listening(info.connected_address.clone())
            },
            PeerConnectionState::Connected(info) => {
                PeerConnectionSimpleState::Connected(info.connected_address.clone())
            },
            PeerConnectionState::Shutdown => PeerConnectionSimpleState::Shutdown,
            PeerConnectionState::Disconnected => PeerConnectionSimpleState::Disconnected,
            PeerConnectionState::Failed(e) => PeerConnectionSimpleState::Failed(e.clone()),
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
            Failed(ref event) => write!(f, "Failed({})", event),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::{
        mpsc::{sync_channel, Receiver},
        Arc,
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
                PeerConnectionSimpleState::Failed(PeerConnectionError::ConnectFailed)
            )
        );
    }

    #[test]
    fn new() {
        let conn = PeerConnection::new();
        assert!(!conn.is_connected());
        assert!(!conn.is_listening());
        assert!(!conn.is_disconnected());
        assert!(!conn.is_active());
        assert!(!conn.is_shutdown());
        assert!(!conn.is_failed());
    }

    #[test]
    fn state_connected() {
        let (thread_ctl, _) = create_thread_ctl();

        let info = ConnectionInfo {
            control_messenger: thread_ctl,
            connected_address: Some("127.0.0.1:1000".parse().unwrap()),
        };
        let conn = PeerConnection {
            state: Arc::new(RwLock::new(PeerConnectionState::Connected(Arc::new(info)))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: None,
            peer_address: None,
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
            state: Arc::new(RwLock::new(PeerConnectionState::Listening(Arc::new(info)))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: None,
            peer_address: None,
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
            state: Arc::new(RwLock::new(PeerConnectionState::Connecting(thread_ctl))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: None,
            peer_address: None,
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
            state: Arc::new(RwLock::new(PeerConnectionState::Connecting(thread_ctl))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: None,
            peer_address: None,
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
            state: Arc::new(RwLock::new(PeerConnectionState::Failed(
                PeerConnectionError::ConnectFailed,
            ))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: None,
            peer_address: None,
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
            state: Arc::new(RwLock::new(PeerConnectionState::Disconnected)),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: None,
            peer_address: None,
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
            state: Arc::new(RwLock::new(PeerConnectionState::Shutdown)),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: None,
            peer_address: None,
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
            state: Arc::new(RwLock::new(PeerConnectionState::Connected(Arc::new(info)))),
            connection_stats: Arc::new(RwLock::new(PeerConnectionStats::new())),
            direction: None,
            peer_address: None,
        };
        (conn, rx)
    }

    #[test]
    fn send() {
        let (conn, rx) = create_connected_peer_connection();

        let sample_frames = vec![vec![123u8]];
        conn.send(sample_frames.clone()).unwrap();
        let msg = rx.recv_timeout(Duration::from_millis(10)).unwrap();
        match msg {
            ControlMessage::SendMsg(frames) => {
                assert_eq!(sample_frames, frames);
            },
            m => panic!("Unexpected control message '{}'", m),
        }
    }

    #[test]
    fn pause() {
        let (conn, rx) = create_connected_peer_connection();

        conn.pause().unwrap();
        let msg = rx.recv_timeout(Duration::from_millis(10)).unwrap();
        assert_eq!(ControlMessage::Pause, msg);
    }

    #[test]
    fn resume() {
        let (conn, rx) = create_connected_peer_connection();

        conn.resume().unwrap();
        let msg = rx.recv_timeout(Duration::from_millis(10)).unwrap();
        assert_eq!(ControlMessage::Resume, msg);
    }

    #[test]
    fn shutdown() {
        let (conn, rx) = create_connected_peer_connection();

        conn.shutdown().unwrap();
        let msg = rx.recv_timeout(Duration::from_millis(10)).unwrap();
        assert_eq!(ControlMessage::Shutdown, msg);
    }

    #[test]
    fn connection_stats() {
        let (conn, _) = create_connected_peer_connection();

        let stats = conn.connection_stats();
        assert_eq!(stats.messages_recv, 0);
        assert_eq!(stats.messages_sent, 0);
    }
}

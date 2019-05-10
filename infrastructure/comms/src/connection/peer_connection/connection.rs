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

use std::fmt;

use crate::connection::{message::FrameSet, Result};

use super::{
    control::{ControlMessage, ThreadControlMessenger},
    runner,
    PeerConnectionContext,
    PeerConnectionError,
};

/// The state of the PeerConnection
enum PeerConnectionState {
    /// The connection object has been created but is not connected
    Initial,
    /// The connection thread is running, contains the thead ControlMessage sender
    Connected(ThreadControlMessenger),
    /// The connection has been shut down
    Shutdown,
}

impl Default for PeerConnectionState {
    fn default() -> Self {
        PeerConnectionState::Initial
    }
}

impl fmt::Display for PeerConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use PeerConnectionState::*;
        match *self {
            Initial => write!(f, "Initial"),
            Connected(_) => write!(f, "Connected"),
            Shutdown => write!(f, "Shutdown"),
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
/// # use tari_comms::connection::{
/// #     Context,
/// #     InprocAddress,
/// #     Direction,
/// #     PeerConnectionContextBuilder,
/// #     PeerConnection,
/// # };
///
/// let ctx = Context::new();
///
/// let peer_context = PeerConnectionContextBuilder::new()
///    .set_context(&ctx)
///    .set_direction(Direction::Outbound)
///    .set_consumer_address(InprocAddress::random())
///    .set_address("127.0.0.1:8080".parse().unwrap())
///    .build()
///    .unwrap();
///
/// let mut conn = PeerConnection::new();
///
/// assert!(!conn.is_connected());
/// // peer_context is consumed by the underlying thread, which is why it's not part of the PeerConnection
/// conn.start(peer_context).unwrap();
/// assert!(conn.is_connected());
/// ```
#[derive(Default)]
pub struct PeerConnection {
    state: PeerConnectionState,
}

impl PeerConnection {
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
    pub fn start(&mut self, context: PeerConnectionContext) -> Result<()> {
        self.state = PeerConnectionState::Connected(runner::start_thread(context).into());
        Ok(())
    }

    /// Transition the PeerConnection into a Shutdown state. The underlying thread
    /// will be terminated and the connection to the Peer closed impolitely.
    pub fn shutdown(&mut self) {
        self.state = PeerConnectionState::Shutdown;
    }

    /// Send frames to the connected Peer. An Err will be returned if the
    /// connection is not in a Connected state.
    ///
    /// # Arguments
    ///
    /// `frames` - The frames to send
    pub fn send(&self, frames: FrameSet) -> Result<()> {
        let thread_messenger = self.get_thread_messenger()?;
        thread_messenger.send(ControlMessage::SendMsg(frames))
    }

    /// Returns true if the PeerConnection is in a connected state, otherwise false
    pub fn is_connected(&self) -> bool {
        match self.state {
            PeerConnectionState::Connected(_) => true,
            _ => false,
        }
    }

    /// Returns the ThreadMessenger. Will return an Err if the PeerConnection is not in
    /// a connected state.
    fn get_thread_messenger(&self) -> Result<&ThreadControlMessenger> {
        match self.state {
            PeerConnectionState::Connected(ref state) => Ok(state),
            _ => Err(PeerConnectionError::StateError(format!(
                "Attempt to retrieve thread messenger on peer connection with state '{}'",
                self.state
            ))
            .into()),
        }
    }
}

impl Drop for PeerConnection {
    /// Transition the PeerConnection to a Shutdown state on Drop
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn state_display() {
        assert_eq!("Initial", format!("{}", PeerConnectionState::Initial));
        let (tx, _) = sync_channel::<ControlMessage>(1);
        let messenger = tx.into();
        assert_eq!("Connected", format!("{}", PeerConnectionState::Connected(messenger)));
        assert_eq!("Shutdown", format!("{}", PeerConnectionState::Shutdown));
    }

    #[test]
    fn new() {
        let conn = PeerConnection::new();
        assert!(!conn.is_connected());
    }
}

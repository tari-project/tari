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

use std::{convert::From, fmt, sync::mpsc::SyncSender};

use crate::connection::{FrameSet, Result};

use super::PeerConnectionError;

/// Control messages which are sent by PeerConnection to the underlying thread.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum ControlMessage {
    /// Shut the thread down
    Shutdown,
    /// Send the given frames to the peer
    SendMsg(FrameSet),
    /// Temporarily pause receiving messages from this connection
    Pause,
    /// Resume receiving messages from peer
    Resume,
}

impl fmt::Display for ControlMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", *self)
    }
}

/// Send and join handles to the worker thread for a PeerConnection
/// This can be converted from a SyncSender
#[derive(Clone)]
pub(super) struct ThreadControlMessenger(SyncSender<ControlMessage>);

impl ThreadControlMessenger {
    /// Send a [ControlMessage] to the listening thread.
    ///
    /// # Arguments
    /// `msg` - The [ControlMessage] to send
    ///
    /// [ControlMessage]: ./enum.ControlMessage.html
    pub fn send(&self, msg: ControlMessage) -> Result<()> {
        self.0.send(msg).map_err(|e| {
            PeerConnectionError::ControlSendError(format!("Failed to send control message: {:?}", e)).into()
        })
    }
}

impl From<SyncSender<ControlMessage>> for ThreadControlMessenger {
    /// Convert a SyncSender<ControlMessage> to a ThreadControlMessenger
    fn from(sender: SyncSender<ControlMessage>) -> Self {
        Self(sender)
    }
}

impl Drop for ThreadControlMessenger {
    /// Send a ControlMessage::Shutdown on drop.
    fn drop(&mut self) {
        // We assume here that the thread responds to the shutdown request.
        let _ = self.0.try_send(ControlMessage::Shutdown);
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{sync::mpsc::sync_channel, thread, time::Duration};

    #[test]
    fn send_control_message() {
        let (tx, rx) = sync_channel::<ControlMessage>(1);

        let handle = thread::spawn(move || {
            let msg = rx
                .recv_timeout(Duration::from_millis(100))
                .map_err(|e| format!("{:?}", e))?;
            match msg {
                ControlMessage::Shutdown => Ok(()),
                x => Err(format!("Received unexpected message {}", x)),
            }
        });

        let messenger: ThreadControlMessenger = tx.into();
        messenger.send(ControlMessage::Shutdown).unwrap();

        handle
            .join()
            .unwrap()
            .map_err(|e| format!("Test thread errored: {:?}", e))
            .unwrap();
    }
}

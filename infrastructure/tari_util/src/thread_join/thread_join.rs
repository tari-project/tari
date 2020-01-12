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

use crate::thread_join::ThreadError;
use std::{
    sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender},
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Debug)]
pub enum StatusMessage {
    /// Successfully joined the thread
    Ok,
    /// An error occurred attempting to join the thread
    Error,
}

/// Spawn a single thread that will attempt to join the specified thread
fn spawn_join_thread<T>(thread_handle: JoinHandle<T>, status_sync_sender: SyncSender<StatusMessage>)
where T: 'static {
    thread::spawn(move || match thread_handle.join() {
        Ok(_) => status_sync_sender.send(StatusMessage::Ok).unwrap(),
        Err(_) => status_sync_sender.send(StatusMessage::Error).unwrap(),
    });
}

/// Perform a thread join with a timeout on the JoinHandle, it has a configurable timeout
fn timeout_join<T>(thread_handle: JoinHandle<T>, timeout_in_ms: Duration) -> Result<(), ThreadError>
where T: 'static {
    let (status_sync_sender, status_receiver) = sync_channel(5);
    spawn_join_thread(thread_handle, status_sync_sender);

    // Check for status messages
    match status_receiver.recv_timeout(timeout_in_ms) {
        Ok(status_msg) => match status_msg {
            StatusMessage::Ok => Ok(()),
            StatusMessage::Error => Err(ThreadError::JoinError),
        },
        Err(RecvTimeoutError::Timeout) => Err(ThreadError::TimeoutReached),
        Err(RecvTimeoutError::Disconnected) => Err(ThreadError::ChannelDisconnected),
    }
}

pub trait ThreadJoinWithTimeout<T> {
    /// Attempt to join the current thread with a configurable timeout
    fn timeout_join(self, timeout_in_ms: Duration) -> Result<(), ThreadError>;
}

/// Extend JoinHandle to have member functions that enable join with a timeout
impl<T> ThreadJoinWithTimeout<T> for JoinHandle<T>
where T: 'static
{
    fn timeout_join(self, timeout: Duration) -> Result<(), ThreadError> {
        timeout_join(self, timeout)
    }
}

#[cfg(test)]
mod test {
    use crate::thread_join::ThreadJoinWithTimeout;
    use std::{thread, time::Duration};

    #[test]
    fn test_normal_thread_join() {
        // Create a blocking thread
        let thread_handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
        });

        let join_timeout_in_ms = Duration::from_millis(100);
        assert!(thread_handle.timeout_join(join_timeout_in_ms).is_ok());
    }

    #[test]
    fn test_thread_join_with_timeout() {
        // Create a blocking thread
        let thread_handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
        });

        let join_timeout_in_ms = Duration::from_millis(25);
        assert!(thread_handle.timeout_join(join_timeout_in_ms).is_err());
    }
}

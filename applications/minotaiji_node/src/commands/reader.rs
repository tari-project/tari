//  Copyright 2022, The Taiji Project
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

use rustyline::{error::ReadlineError, Editor};
use tokio::{
    sync::mpsc,
    task::{self, JoinHandle},
};

use super::parser::Parser;

/// A reader that uses `rustyline` in a separate thread
/// to read input and send it to an async channel.
pub struct CommandReader {
    #[allow(dead_code)]
    task: JoinHandle<()>,
    sender: mpsc::Sender<()>,
    receiver: mpsc::Receiver<Result<String, ReadlineError>>,
}

impl CommandReader {
    /// Creates a reader instance and spawns a blocking tokio thread.
    ///
    /// The thread terminates when an instance of the reader is dropped
    /// (when inner receiver dropped and the thread can't write a value
    /// to a channel).
    pub fn new(mut rustyline: Editor<Parser>) -> Self {
        let (tx_next, mut rx_next) = mpsc::channel(1);
        let (tx_event, rx_event) = mpsc::channel(1);
        let task = task::spawn_blocking(move || loop {
            if rx_next.blocking_recv().is_none() {
                break;
            }
            let event = rustyline.readline(">> ");
            if tx_event.blocking_send(event).is_err() {
                break;
            }
        });
        Self {
            task,
            sender: tx_next,
            receiver: rx_event,
        }
    }

    /// Reads the next command from a terminal.
    pub async fn next_command(&mut self) -> Option<Result<String, ReadlineError>> {
        self.sender.send(()).await.ok()?;
        self.receiver.recv().await
    }
}

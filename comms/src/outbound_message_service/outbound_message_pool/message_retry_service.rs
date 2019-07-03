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

use super::error::OutboundMessagePoolError;
use crate::{
    connection::{Connection, Direction, EstablishedConnection, InprocAddress, ZmqContext},
    message::FrameSet,
    outbound_message_service::{outbound_message_pool::OutboundMessagePoolConfig, OutboundMessage},
};
use log::*;
use std::{
    collections::BinaryHeap,
    sync::mpsc::{Receiver, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::Duration,
};
use tari_utilities::message_format::MessageFormat;

const LOG_TARGET: &'static str = "comms::outbound_message_service::outbound_message_pool::message_retry_pool";
const THREAD_STACK_SIZE: usize = 512_000;

/// # MessageRetryService
///
/// Call the `MessageRetryService::start` function to start the message retry service. Once successfully started
/// it listens for messages on the given inbound address. It expects the message to be a serialized [OutboundMessage].
///
/// On receipt of the message, it attempts to deserialize the message, marks it as failed and adds it to it's internal
/// queue. The queue (BinaryHeap) is checked approximately every second for messages which should be retried.
/// The order in which the messages are retried is determined by [OutboundMessage]'s `PartialOrd` implementation.
///
/// [OutboundMessage]: ../outbound_message/struct.OutboundMessage.html
pub struct MessageRetryService {
    queue: BinaryHeap<OutboundMessage>,
    config: OutboundMessagePoolConfig,
    shutdown_signal_rx: Receiver<()>,
}

impl MessageRetryService {
    /// Start the message retry service.
    ///
    /// This method will panic if the OS-level thread is unable to start.
    pub fn start(
        context: ZmqContext,
        config: OutboundMessagePoolConfig,
        inbound_address: InprocAddress,
        outbound_address: InprocAddress,
        shutdown_signal_rx: Receiver<()>,
    ) -> JoinHandle<Result<(), OutboundMessagePoolError>>
    {
        thread::Builder::new()
            .name("msg-retry-service".to_string())
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || {
                let mut message_retry_queue = Self::new(config, shutdown_signal_rx);
                let inbound_conn = Connection::new(&context, Direction::Inbound)
                    .set_name("omp-retry-service-inbound")
                    .establish(&inbound_address)
                    .map_err(OutboundMessagePoolError::ConnectionError)?;

                let outbound_conn = Connection::new(&context, Direction::Outbound)
                    .set_name("omp-retry-service-outbound")
                    .establish(&outbound_address)
                    .map_err(OutboundMessagePoolError::ConnectionError)?;

                loop {
                    match message_retry_queue.run(&inbound_conn, &outbound_conn) {
                        Ok(_) => break,
                        Err(err) => {
                            error!(target: LOG_TARGET, "Outbound message retry pool errored: {:?}", err);
                            warn!(
                                target: LOG_TARGET,
                                "Restarting outbound message retry queue after failure."
                            );
                            thread::sleep(Duration::from_millis(1000));
                        },
                    }
                }

                info!(target: LOG_TARGET, "Message Retry Queue has cleanly shut down");

                Ok(())
            })
            .or_else(|err| {
                error!(
                    target: LOG_TARGET,
                    "Unable to start thread for MessageRetryService: {:?}", err
                );
                Err(err)
            })
            .unwrap()
    }

    fn new(config: OutboundMessagePoolConfig, shutdown_signal_rx: Receiver<()>) -> Self {
        Self {
            queue: BinaryHeap::new(),
            config,
            shutdown_signal_rx,
        }
    }

    fn run(
        &mut self,
        inbound_conn: &EstablishedConnection,
        outbound_conn: &EstablishedConnection,
    ) -> Result<(), OutboundMessagePoolError>
    {
        loop {
            if let Some(frames) = connection_try!(inbound_conn.receive(1000)) {
                let mut msg = Self::deserialize_outbound_message(frames)?;
                msg.mark_failed_attempt();

                if msg.num_attempts() > self.config.max_retries {
                    // Discard message
                    warn!(
                        target: LOG_TARGET,
                        "Discarding message for NodeId {}. Max retry attempts ({}) exceeded.",
                        msg.destination_node_id(),
                        self.config.max_retries
                    );
                    continue;
                }

                // Add it to the queue for later retry
                self.queue.push(msg);
            }

            match self.shutdown_signal_rx.recv_timeout(Duration::from_millis(1)) {
                Ok(_) => break,
                Err(RecvTimeoutError::Timeout) => {},
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(OutboundMessagePoolError::ControlMessageSenderDisconnected)
                },
            }

            if let Some(msg) = self.queue.peek() {
                if !msg.is_scheduled() {
                    continue;
                }

                let msg = self.queue.pop().unwrap();

                let frame = msg.to_binary().map_err(OutboundMessagePoolError::MessageFormatError)?;
                outbound_conn
                    .send(&[frame])
                    .map_err(OutboundMessagePoolError::ConnectionError)?;
            }
        }

        Ok(())
    }

    fn deserialize_outbound_message(mut frames: FrameSet) -> Result<OutboundMessage, OutboundMessagePoolError> {
        match frames.drain(1..).next() {
            Some(frame) => OutboundMessage::from_binary(&frame).map_err(OutboundMessagePoolError::MessageFormatError),
            None => Err(OutboundMessagePoolError::InvalidFrameFormat(format!(
                "Message retry pool worker received a frame set with invalid length"
            ))),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::peer_manager::NodeId;
    use std::sync::mpsc::sync_channel;

    #[test]
    fn new() {
        let config = OutboundMessagePoolConfig::default();
        let (_, rx) = sync_channel(0);
        let subject = MessageRetryService::new(config.clone(), rx);
        assert!(subject.queue.is_empty());
    }

    #[test]
    fn deserialize_outbound_message() {
        let msg = OutboundMessage::new(NodeId::new(), vec![vec![]]);
        let msg_frame = msg.to_binary().unwrap();
        let frames = vec![vec![1, 2, 3, 4], msg_frame];
        let result_msg = MessageRetryService::deserialize_outbound_message(frames).unwrap();
        assert_eq!(result_msg, msg);
    }
}

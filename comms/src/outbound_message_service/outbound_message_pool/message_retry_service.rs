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

use super::error::RetryServiceError;
use crate::{
    connection::{Connection, Direction, EstablishedConnection, InprocAddress, ZmqContext},
    outbound_message_service::{outbound_message_pool::OutboundMessagePoolConfig, OutboundMessage},
    peer_manager::NodeId,
};
use log::*;
use std::{
    collections::BinaryHeap,
    sync::mpsc::{Receiver, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::Duration,
};
use tari_utilities::message_format::MessageFormat;

const LOG_TARGET: &str = "comms::outbound_message_service::outbound_message_pool::message_retry_pool";
const THREAD_STACK_SIZE: usize = 256 * 1024; // 256kb

pub enum RetryServiceMessage {
    Shutdown,
    FailedAttempt(OutboundMessage),
    /// Indicates that all messages for a given node ID should be cleared from the
    /// queue and sent to the message_sink.
    Flush(NodeId),
}

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
    message_source: Receiver<RetryServiceMessage>,
}

impl MessageRetryService {
    /// Start the message retry service.
    ///
    /// This method will panic if the OS-level thread is unable to start.
    pub fn start(
        context: ZmqContext,
        config: OutboundMessagePoolConfig,
        message_source: Receiver<RetryServiceMessage>,
        outbound_address: InprocAddress,
    ) -> JoinHandle<Result<(), RetryServiceError>>
    {
        let message_retry_service = Self::new(config, message_source);
        Self::spawn(context, outbound_address, message_retry_service)
    }

    fn spawn(
        context: ZmqContext,
        outbound_address: InprocAddress,
        mut message_retry_service: Self,
    ) -> JoinHandle<Result<(), RetryServiceError>>
    {
        thread::Builder::new()
            .name("msg-retry-service".to_string())
            .stack_size(THREAD_STACK_SIZE)
            .spawn(move || {
                let outbound_conn = Connection::new(&context, Direction::Outbound)
                    .set_name("omp-retry-service-outbound")
                    .establish(&outbound_address)
                    .map_err(RetryServiceError::ConnectionError)?;

                loop {
                    match message_retry_service.run(&outbound_conn) {
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

    fn new(config: OutboundMessagePoolConfig, message_source: Receiver<RetryServiceMessage>) -> Self {
        Self {
            queue: BinaryHeap::new(),
            config,
            message_source,
        }
    }

    fn run(&mut self, outbound_conn: &EstablishedConnection) -> Result<(), RetryServiceError> {
        loop {
            trace!(
                target: LOG_TARGET,
                "MessageRetryService loop (queue_size: {})",
                self.queue.len()
            );

            if let Some(msg) = self.receive_control_msg(Duration::from_millis(1000))? {
                match msg {
                    RetryServiceMessage::Shutdown => {
                        info!(target: LOG_TARGET, "MessageRetryService shutdown signal received");
                        break;
                    },
                    RetryServiceMessage::FailedAttempt(mut outbound_msg) => {
                        outbound_msg.mark_failed_attempt();
                        if outbound_msg.num_attempts() > self.config.max_retries {
                            // Discard message
                            warn!(
                                target: LOG_TARGET,
                                "Discarding message for NodeId {}. Max retry attempts ({}) exceeded.",
                                outbound_msg.destination_node_id(),
                                self.config.max_retries
                            );
                            continue;
                        }

                        // Add it to the queue for later retry
                        debug!(
                            target: LOG_TARGET,
                            "Message failed to send. Message will be retried in {}s",
                            outbound_msg.scheduled_duration().num_seconds()
                        );
                        self.queue.push(outbound_msg);
                    },
                    RetryServiceMessage::Flush(node_id) => {
                        debug!(
                            target: LOG_TARGET,
                            "Immediately retrying messages from NodeId={}", node_id
                        );
                        let flushed_msgs = self.flush_messages_for_node_id(&node_id)?;
                        debug!(
                            target: LOG_TARGET,
                            "Flushing {} messages for NodeId={}",
                            flushed_msgs.len(),
                            node_id
                        );
                        for msg in flushed_msgs.into_iter() {
                            self.send_msg(outbound_conn, msg)?;
                        }
                    },
                };
            }

            if let Some(msg) = self.queue.peek() {
                if !msg.is_scheduled() {
                    continue;
                }

                let msg = self.queue.pop().unwrap();

                debug!(
                    target: LOG_TARGET,
                    "Message for NodeId {} scheduled for another attempt ({} of {})",
                    msg.destination_node_id(),
                    msg.num_attempts(),
                    self.config.max_retries
                );

                self.send_msg(outbound_conn, msg)?;
            }
        }

        Ok(())
    }

    fn receive_control_msg(&self, timeout: Duration) -> Result<Option<RetryServiceMessage>, RetryServiceError> {
        match self.message_source.recv_timeout(timeout) {
            Ok(msg) => Ok(Some(msg)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(RetryServiceError::ControlMessageSenderDisconnected),
        }
    }

    /// Flush all messages for a particular peer from the queue and add to the sending queue
    fn flush_messages_for_node_id(
        &mut self,
        node_id: &NodeId,
    ) -> Result<BinaryHeap<OutboundMessage>, RetryServiceError>
    {
        // TODO(sdbondi): Use drain_filter when it's available (https://github.com/rust-lang/rust/issues/43244)
        let queue = self.queue.drain().collect::<BinaryHeap<OutboundMessage>>();
        let (flushed_msgs, queue) = queue.into_iter().partition(|msg| msg.destination_node_id() == node_id);
        self.queue = queue;

        Ok(flushed_msgs)
    }

    /// Serialize and send a message on a given connection
    fn send_msg(&self, conn: &EstablishedConnection, msg: OutboundMessage) -> Result<(), RetryServiceError> {
        let frame = msg.to_binary().map_err(RetryServiceError::MessageFormatError)?;
        conn.send(&[frame]).map_err(RetryServiceError::ConnectionError)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::peer_manager::NodeId;
    use std::{iter::repeat_with, sync::mpsc::sync_channel};
    use tari_utilities::{byte_array::ByteArray, thread_join::ThreadJoinWithTimeout};

    #[test]
    fn new() {
        let config = OutboundMessagePoolConfig::default();
        let (_, rx) = sync_channel(0);
        let subject = MessageRetryService::new(config.clone(), rx);
        assert!(subject.queue.is_empty());
    }

    #[test]
    fn flush_messages_for_node_id() {
        let node_id1 = NodeId::new();
        let node_id2 = NodeId::from_bytes(
            [
                144, 28, 106, 112, 220, 197, 216, 119, 9, 217, 42, 77, 159, 211, 53, 207, 0, 157, 5, 55, 235, 247, 160,
                195, 240, 48, 146, 168, 119, 15, 241, 54,
            ]
            .as_bytes(),
        )
        .unwrap();
        let (_shutdown_signal_tx, shutdown_signal_rx) = sync_channel(1);

        let mut service = MessageRetryService::new(OutboundMessagePoolConfig::default(), shutdown_signal_rx);
        let dummy_msg = OutboundMessage::new(node_id1.clone(), vec![vec![]]);
        let dummy_msg2 = OutboundMessage::new(node_id2.clone(), vec!["EXPECTED".as_bytes().to_vec()]);
        let mut messages = repeat_with(|| dummy_msg.clone())
            .take(5)
            .collect::<Vec<OutboundMessage>>();
        messages.extend(
            repeat_with(|| dummy_msg2.clone())
                .take(2)
                .collect::<Vec<OutboundMessage>>(),
        );

        service.queue = messages.into();

        let msgs = service.flush_messages_for_node_id(&node_id2).unwrap();
        for msg in msgs {
            assert_eq!(msg.message_frames()[0], "EXPECTED".as_bytes());
        }

        assert_eq!(service.queue.len(), 5);
    }

    #[test]
    fn shutdown_msg() {
        let context = ZmqContext::new();
        let config = OutboundMessagePoolConfig::default();
        let (tx, rx) = sync_channel(0);
        let handle = MessageRetryService::start(context, config.clone(), rx, InprocAddress::random());
        tx.send(RetryServiceMessage::Shutdown).unwrap();
        handle.timeout_join(Duration::from_millis(3000)).unwrap();
    }

    #[test]
    fn flush_msg() {
        let node_id = NodeId::new();
        let context = ZmqContext::new();
        let config = OutboundMessagePoolConfig::default();
        let (tx, rx) = sync_channel(0);
        let mut service = MessageRetryService::new(config.clone(), rx);
        service.queue = repeat_with(|| OutboundMessage::new(node_id.clone(), vec![vec![]]))
            .take(3)
            .collect::<Vec<OutboundMessage>>()
            .into();

        let omp_addr = InprocAddress::random();
        let omp_conn = Connection::new(&context, Direction::Inbound)
            .establish(&omp_addr)
            .unwrap();
        let handle = MessageRetryService::spawn(context, omp_addr, service);
        tx.send(RetryServiceMessage::Flush(node_id.clone())).unwrap();

        for _ in 0..2 {
            omp_conn.receive(3000).unwrap();
        }
        tx.send(RetryServiceMessage::Shutdown).unwrap();

        handle.timeout_join(Duration::from_millis(3000)).unwrap();
    }

    #[test]
    fn failed_attempt_msg() {
        let node_id = NodeId::new();
        let context = ZmqContext::new();
        let config = OutboundMessagePoolConfig::default();
        let (tx, rx) = sync_channel(0);
        let service = MessageRetryService::new(config.clone(), rx);
        assert_eq!(service.queue.len(), 0);

        let omp_addr = InprocAddress::random();
        let omp_conn = Connection::new(&context, Direction::Inbound)
            .establish(&omp_addr)
            .unwrap();
        let handle = MessageRetryService::spawn(context, omp_addr, service);
        // Send a message which is scheduled to send immediately
        tx.send(RetryServiceMessage::FailedAttempt(OutboundMessage::new(
            node_id.clone(),
            vec!["EXPECTED".as_bytes().to_vec()],
        )))
        .unwrap();

        let msg = omp_conn
            .receive(3000)
            .map(|frames| OutboundMessage::from_binary(&frames[1]))
            .unwrap()
            .unwrap();

        assert_eq!(msg.message_frames()[0], "EXPECTED".as_bytes().to_vec());

        tx.send(RetryServiceMessage::Shutdown).unwrap();
        handle.timeout_join(Duration::from_millis(3000)).unwrap();
    }
}

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
    peer_manager::NodeId,
};
use log::*;
use std::{
    collections::BinaryHeap,
    sync::mpsc::{Receiver, RecvTimeoutError},
    thread::{self, JoinHandle},
    time::Duration,
};
use tari_utilities::{byte_array::ByteArray, message_format::MessageFormat};

const LOG_TARGET: &str = "comms::outbound_message_service::outbound_message_pool::message_retry_pool";
const THREAD_STACK_SIZE: usize = 256 * 1024; // 256kb

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
    /// Control message which indicates that all messages for a node ID should be
    /// immediately queued for sending.
    pub(super) const CTL_FLUSH_NODE_MSGS: &'static str = "FLUSH_NODE_MSGS";

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
            trace!(
                target: LOG_TARGET,
                "MessageRetryService loop (queue_size: {})",
                self.queue.len()
            );
            if let Some(frames) = connection_try!(inbound_conn.receive(1000)) {
                if let Some(node_id) = Self::maybe_parse_node_flush_msg(&frames) {
                    debug!(
                        target: LOG_TARGET,
                        "Immediately retrying messages from NodeId={}", node_id
                    );
                    let num_flushed = self.flush_messages_for_node_id(&outbound_conn, &node_id)?;
                    debug!(
                        target: LOG_TARGET,
                        "Flushed {} messages from NodeId={}", num_flushed, node_id
                    );
                    continue;
                }

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
                debug!(
                    target: LOG_TARGET,
                    "Message failed to send. Message will be retried in {}s",
                    msg.scheduled_duration().num_seconds()
                );
                self.queue.push(msg);
            }

            match self.shutdown_signal_rx.recv_timeout(Duration::from_millis(1)) {
                Ok(_) => {
                    info!(target: LOG_TARGET, "SHUTDOWN SIGNAL RECEIVED");
                    break;
                },
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

                debug!(
                    target: LOG_TARGET,
                    "Message for NodeId {} scheduled for another attempt ({} of {})",
                    msg.destination_node_id(),
                    msg.num_attempts(),
                    self.config.max_retries
                );

                self.send_msg(&outbound_conn, msg)?;
            }
        }

        Ok(())
    }

    /// Flush all messages for a particular peer from the queue and add to the sending queue
    fn flush_messages_for_node_id(
        &mut self,
        outbound_conn: &EstablishedConnection,
        node_id: &NodeId,
    ) -> Result<usize, OutboundMessagePoolError>
    {
        // TODO(sdbondi): Use drain_filter when it's available (https://github.com/rust-lang/rust/issues/43244)
        let queue = self.queue.drain().collect::<BinaryHeap<OutboundMessage>>();
        let (msgs_to_send, queue) = queue.into_iter().partition(|msg| msg.destination_node_id() == node_id);
        self.queue = queue;

        let len = msgs_to_send.len();

        for msg in msgs_to_send {
            self.send_msg(&outbound_conn, msg)?;
        }

        Ok(len)
    }

    /// If the frameset is a CTL_FLUSH_NODE_MSGS, return the NodeId to flush, otherwise None
    fn maybe_parse_node_flush_msg(frames: &FrameSet) -> Option<NodeId> {
        match frames.len() {
            3 if frames[1] == Self::CTL_FLUSH_NODE_MSGS.as_bytes() => NodeId::from_bytes(&frames[2]).ok(),
            _ => None,
        }
    }

    /// Serialize and send a message on a given connection
    fn send_msg(&self, conn: &EstablishedConnection, msg: OutboundMessage) -> Result<(), OutboundMessagePoolError> {
        let frame = msg.to_binary().map_err(OutboundMessagePoolError::MessageFormatError)?;
        conn.send(&[frame]).map_err(OutboundMessagePoolError::ConnectionError)
    }

    /// Expect the given frameset to contain an outbound message
    fn deserialize_outbound_message(mut frames: FrameSet) -> Result<OutboundMessage, OutboundMessagePoolError> {
        match frames.drain(1..).next() {
            Some(frame) => OutboundMessage::from_binary(&frame).map_err(OutboundMessagePoolError::MessageFormatError),
            None => Err(OutboundMessagePoolError::InvalidFrameFormat(
                "Message retry pool worker received a frame set with invalid length".to_string(),
            )),
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

    #[test]
    fn maybe_parse_node_flush_msg() {
        let node_id = NodeId::new();
        let maybe_node_id = MessageRetryService::maybe_parse_node_flush_msg(&vec![
            vec![],
            MessageRetryService::CTL_FLUSH_NODE_MSGS.as_bytes().to_vec(),
            node_id.as_bytes().to_vec(),
        ]);

        assert!(maybe_node_id.is_some());
        assert_eq!(maybe_node_id.unwrap(), node_id);
    }

    #[test]
    fn maybe_parse_node_flush_msg_fail() {
        let node_id = NodeId::new();

        // Not enough frames
        let maybe_node_id = MessageRetryService::maybe_parse_node_flush_msg(&vec![
            MessageRetryService::CTL_FLUSH_NODE_MSGS.as_bytes().to_vec(),
            node_id.as_bytes().to_vec(),
        ]);

        assert!(maybe_node_id.is_none());

        // Too many frames
        let maybe_node_id = MessageRetryService::maybe_parse_node_flush_msg(&vec![
            vec![],
            MessageRetryService::CTL_FLUSH_NODE_MSGS.as_bytes().to_vec(),
            node_id.as_bytes().to_vec(),
            vec![],
        ]);

        assert!(maybe_node_id.is_none());

        // Bad flush message identifier
        let maybe_node_id = MessageRetryService::maybe_parse_node_flush_msg(&vec![
            vec![],
            "BAD".as_bytes().to_vec(),
            node_id.as_bytes().to_vec(),
        ]);

        assert!(maybe_node_id.is_none());

        // Bad node id
        let maybe_node_id = MessageRetryService::maybe_parse_node_flush_msg(&vec![
            vec![],
            MessageRetryService::CTL_FLUSH_NODE_MSGS.as_bytes().to_vec(),
            node_id.as_bytes().to_vec().drain(1..).collect::<Vec<u8>>(),
        ]);

        assert!(maybe_node_id.is_none());
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
        let dummy_frames = vec![vec![]];
        let dummy_frames2 = vec!["EXPECTED".as_bytes().to_vec()];
        service
            .queue
            .push(OutboundMessage::new(node_id1.clone(), dummy_frames.clone()));
        service
            .queue
            .push(OutboundMessage::new(node_id1.clone(), dummy_frames.clone()));
        service
            .queue
            .push(OutboundMessage::new(node_id2.clone(), dummy_frames2.clone()));
        service
            .queue
            .push(OutboundMessage::new(node_id1.clone(), dummy_frames.clone()));
        service
            .queue
            .push(OutboundMessage::new(node_id2.clone(), dummy_frames2.clone()));
        service
            .queue
            .push(OutboundMessage::new(node_id2.clone(), dummy_frames2.clone()));
        service
            .queue
            .push(OutboundMessage::new(node_id1.clone(), dummy_frames.clone()));
        service
            .queue
            .push(OutboundMessage::new(node_id1.clone(), dummy_frames.clone()));

        let context = ZmqContext::new();
        let address = InprocAddress::random();
        let out_conn = Connection::new(&context, Direction::Outbound)
            .establish(&address)
            .unwrap();

        let in_conn = Connection::new(&context, Direction::Inbound)
            .establish(&address)
            .unwrap();

        service.flush_messages_for_node_id(&out_conn, &node_id2).unwrap();

        let mut msg_count = 0;

        for _ in 0..3 {
            let frames = in_conn.receive(2000).unwrap();
            let msg = OutboundMessage::from_binary(&frames[1]).unwrap();
            assert_eq!(msg.message_frames()[0], "EXPECTED".as_bytes());
            msg_count += 1;
        }

        assert_eq!(msg_count, 3);
        assert_eq!(service.queue.len(), 5);
    }
}

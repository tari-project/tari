// Copyright 2019 The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE

use super::{outbound_message_pool::OutboundMessagePoolConfig, OutboundError, OutboundMessage};
use crate::{
    connection::{Connection, ConnectionError, Direction, InprocAddress, SocketEstablishment, ZmqContext},
    connection_manager::ConnectionManager,
    message::{FrameSet, MessageEnvelope},
    peer_manager::PeerManager,
    types::{CommsDataStore, CommsPublicKey},
};
use chrono::Utc;
use log::*;
#[cfg(test)]
use std::sync::mpsc::SyncSender;
use std::{convert::TryFrom, sync::Arc, thread};
use tari_utilities::message_format::MessageFormat;

const LOG_TARGET: &'static str = "comms::outbound_message_service::pool::worker";

#[derive(Debug)]
pub enum WorkerError {
    /// Problem with inbound connection
    InboundConnectionError(ConnectionError),
    /// Failed to connect to message queue
    MessageQueueConnectionError(ConnectionError),
}

/// This is an instance of a single Worker thread for the Outbound Message Pool
pub struct MessagePoolWorker {
    config: OutboundMessagePoolConfig,
    context: ZmqContext,
    inbound_address: InprocAddress,
    message_requeue_address: InprocAddress,
    peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    connection_manager: Arc<ConnectionManager>,
    #[cfg(test)]
    test_sync_sender: Option<SyncSender<String>>,
}

impl MessagePoolWorker {
    pub fn new(
        config: OutboundMessagePoolConfig,
        context: ZmqContext,
        inbound_address: InprocAddress,
        message_requeue_address: InprocAddress,
        peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
        connection_manager: Arc<ConnectionManager>,
    ) -> MessagePoolWorker
    {
        MessagePoolWorker {
            config,
            context,
            inbound_address,
            message_requeue_address,
            peer_manager,
            connection_manager,
            #[cfg(test)]
            test_sync_sender: None,
        }
    }

    /// Start MessagePoolWorker which will connect to the inbound message dealer, accept messages from the queue,
    /// attempt to send them and if it cannot send then requeue the message
    fn start_worker(&mut self) -> Result<(), WorkerError> {
        #[cfg(test)]
        let tx = self.test_sync_sender.clone().unwrap();

        // Connection to the message dealer proxy
        let inbound_connection = Connection::new(&self.context, Direction::Inbound)
            .set_socket_establishment(SocketEstablishment::Connect)
            .establish(&self.inbound_address)
            .map_err(|e| WorkerError::InboundConnectionError(e))?;
        loop {
            match inbound_connection.receive(self.config.worker_timeout_in_ms) {
                Ok(mut frame_set) => {
                    // This strips off the two ZeroMQ Identity frames introduced by the transmission to the proxy and
                    // from the proxy to this worker
                    let data: FrameSet = frame_set.drain(2..).collect();
                    if let Ok(mut msg) = OutboundMessage::<MessageEnvelope>::try_from(data) {
                        #[cfg(test)]
                        tx.send(String::from("Message Received".to_string())).unwrap();

                        // Check if the retry time wait period has elapsed
                        if msg.last_retry_timestamp().is_none() ||
                            msg.last_retry_timestamp().unwrap() + self.config.retry_wait_time <= Utc::now()
                        {
                            // Attempt transmission
                            match self.attempt_message_transmission(&mut msg) {
                                Ok(()) => {
                                    debug!(target: LOG_TARGET, "Outbound message successfully sent");
                                },
                                Err(e) => {
                                    warn!(
                                        target: LOG_TARGET,
                                        "Failed to transmit outbound message - Error: {:?}", e
                                    );
                                    match self.queue_message_retry(msg) {
                                        Ok(()) => {
                                            debug!(target: LOG_TARGET, "Message retry successfully requeued");
                                        },
                                        Err(e) => error!(
                                            target: LOG_TARGET,
                                            "Error retrying message transmission - Error {:?}", e
                                        ),
                                    }
                                },
                            }
                        } else {
                            // Requeue a message whose Retry Wait Time has not elapsed without marking a transmission
                            // attempt
                            match self.requeue_message(&msg) {
                                Ok(_) => (),
                                Err(e) => {
                                    error!(
                                        target: LOG_TARGET,
                                        "Error requeuing an Outbound Message - Error: {:?}", e
                                    );
                                },
                            };
                        }
                    }
                },
                Err(ConnectionError::Timeout) => (),
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Error receiving messages from outbound message queue - Error: {}", e
                    );
                },
            };
        }
    }

    /// Start the MessagePoolWorker thread
    pub fn start(mut self) {
        thread::spawn(move || loop {
            match self.start_worker() {
                Ok(_) => (),
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Error starting Outbound Message Pool worker: {:?}", e
                    );
                },
            }
        });
    }

    /// Attempt to send a message to the NodeId specified in the message. If the the attempt is not successful then mark
    /// the failed connection attempt and requeue the message for another attempt
    fn attempt_message_transmission(
        &mut self,
        msg: &mut OutboundMessage<MessageEnvelope>,
    ) -> Result<(), OutboundError>
    {
        let peer = self.peer_manager.find_with_node_id(&msg.destination_node_id)?;
        let peer_connection = self.connection_manager.establish_connection_to_peer(&peer)?;
        peer_connection.send(msg.message_envelope.clone().into_frame_set())?;
        Ok(())
    }

    /// Check if a message transmission is able to be retried, if so then mark the transmission attempt and requeue it.
    fn queue_message_retry(&self, mut msg: OutboundMessage<MessageEnvelope>) -> Result<(), OutboundError> {
        if msg.number_of_retries() < self.config.max_num_of_retries {
            msg.mark_transmission_attempt();
            self.peer_manager.reset_connection_attempts(&msg.destination_node_id)?;
            self.requeue_message(&msg)?;
        };
        Ok(())
    }

    /// Send a message back to the Outbound Message Pool message queue.
    fn requeue_message(&self, msg: &OutboundMessage<MessageEnvelope>) -> Result<(), OutboundError> {
        let outbound_message_buffer = vec![msg.to_binary().map_err(|e| OutboundError::MessageFormatError(e))?];

        let outbound_connection = Connection::new(&self.context, Direction::Outbound)
            .set_socket_establishment(SocketEstablishment::Connect)
            .establish(&self.message_requeue_address)
            .map_err(|e| OutboundError::ConnectionError(e))?;
        outbound_connection
            .send(&outbound_message_buffer)
            .map_err(|e| OutboundError::ConnectionError(e))?;
        Ok(())
    }

    #[cfg(test)]
    pub fn set_test_channel(&mut self, tx: SyncSender<String>) {
        self.test_sync_sender = Some(tx);
    }
}

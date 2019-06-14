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

use crate::{
    connection::{
        zmq::{Context, InprocAddress},
        Connection,
        ConnectionError,
        Direction,
        SocketEstablishment,
    },
    message::{FrameSet, MessageEnvelope},
    outbound_message_service::{OutboundError, OutboundMessage},
    peer_manager::PeerManager,
};

use crate::connection::net_address::NetAddressError;
use log::*;
#[cfg(test)]
use std::sync::mpsc::SyncSender;
use std::{convert::TryFrom, hash::Hash, sync::Arc, thread};
use tari_crypto::keys::PublicKey;

use tari_storage::keyvalue_store::DataStore;

const LOG_TARGET: &'static str = "comms::outbound_message_service::pool::worker";

#[derive(Debug)]
pub enum WorkerError {
    /// Problem with inbound connection
    InboundConnectionError(ConnectionError),
    /// Failed to connect to message queue
    MessageQueueConnectionError(ConnectionError),
}

pub struct MessagePoolWorker<P, DS> {
    context: Context,
    inbound_address: InprocAddress,
    message_queue_address: InprocAddress,
    peer_manager: Arc<PeerManager<P, DS>>,
    #[cfg(test)]
    test_sync_sender: Option<SyncSender<String>>,
}

impl<P, DS> MessagePoolWorker<P, DS>
where
    P: PublicKey + Hash + Send + Sync + 'static,
    DS: DataStore + Send + Sync + 'static,
{
    pub fn new(
        context: Context,
        inbound_address: InprocAddress,
        message_queue_address: InprocAddress,
        peer_manager: Arc<PeerManager<P, DS>>,
    ) -> MessagePoolWorker<P, DS>
    {
        MessagePoolWorker {
            context,
            inbound_address,
            message_queue_address,
            peer_manager,
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
            match inbound_connection.receive(100) {
                Ok(mut frame_set) => {
                    // This strips off the two ZeroMQ Identity frames introduced by the transmission to the proxy and
                    // from the proxy to this worker
                    let data: FrameSet = frame_set.drain(2..).collect();
                    if let Ok(mut msg) = OutboundMessage::<MessageEnvelope>::try_from(data) {
                        match self.attempt_message_transmission(&mut msg) {
                            Ok(()) => {
                                #[cfg(test)]
                                tx.send(String::from(format!("Attempt {:?}", msg.number_of_retries())))
                                    .unwrap();
                            },
                            Err(e) => match e {
                                OutboundError::NetAddressError(e) => match e {
                                    NetAddressError::ConnectionAttemptsExceeded => {
                                        warn!(
                                            target: LOG_TARGET,
                                            "Number of Connection Attempts Exceeded - Error: {}", e
                                        );
                                        #[cfg(test)]
                                        tx.send(String::from("Connection Attempts Exceeded")).unwrap()
                                    },
                                    _ => (),
                                },
                                _ => (),
                            },
                        }
                    }
                },
                Err(ConnectionError::Timeout) => (),
                Err(e) => {
                    error!(
                        target: LOG_TARGET,
                        "Failed to receive messages from outbound message queue - Error: {}", e
                    );
                },
            };
        }
    }

    /// Start the MessagePoolWorker thread
    pub fn start(mut self) {
        thread::spawn(move || {
            loop {
                match self.start_worker() {
                    Ok(_) => (),
                    Err(_e) => {
                        // TODO Write WorkerError as a Log Error
                    },
                }
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
        // Should attempt to connect and send, just mark as failed attempt
        let mut peer = self.peer_manager.find_with_node_id(&msg.destination_node_id)?;
        let net_address = peer.addresses.get_best_net_address()?;
        self.peer_manager.mark_failed_connection_attempt(&net_address)?;

        // TODO Actually try send

        msg.mark_transmission_attempt();
        self.requeue_message(msg)?;

        Ok(())
    }

    /// Send a message back to the Outbound Message Pool message queue.
    fn requeue_message(&self, msg: &OutboundMessage<MessageEnvelope>) -> Result<(), OutboundError> {
        let outbound_message_buffer = vec![msg
            .to_frame()
            .map_err(|e| OutboundError::MessageSerializationError(e))?];

        let outbound_connection = Connection::new(&self.context, Direction::Outbound)
            .set_socket_establishment(SocketEstablishment::Connect)
            .establish(&self.message_queue_address)
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

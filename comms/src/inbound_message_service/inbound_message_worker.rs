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

use super::error::InboundError;
use crate::{
    connection::{
        peer_connection::ControlMessage,
        zmq::{InprocAddress, ZmqContext},
        Connection,
        ConnectionError,
        Direction,
        SocketEstablishment,
    },
    dispatcher::DispatchableKey,
    inbound_message_service::{
        inbound_message_broker::InboundMessageBroker,
        inbound_message_service::InboundMessageServiceConfig,
    },
    message::{FrameSet, MessageContext, MessageData},
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::{peer_manager::PeerManager, NodeId, NodeIdentity, Peer},
    types::{CommsDataStore, CommsPublicKey, MessageDispatcher},
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    convert::TryFrom,
    sync::{
        mpsc::{sync_channel, Receiver, SyncSender},
        Arc,
    },
    thread,
};

const LOG_TARGET: &'static str = "comms::inbound_message_service::worker";

/// The InboundMessageWorker retrieve messages from the inbound message queue, creates a MessageContext for the message
/// that is then dispatch using the dispatcher.
pub struct InboundMessageWorker<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    config: InboundMessageServiceConfig,
    context: ZmqContext,
    node_identity: Arc<NodeIdentity>,
    message_queue_address: InprocAddress,
    message_dispatcher: Arc<MessageDispatcher<MessageContext<MType>>>,
    inbound_message_broker: Arc<InboundMessageBroker<MType>>,
    outbound_message_service: Arc<OutboundMessageService>,
    peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    control_receiver: Option<Receiver<ControlMessage>>,
    is_running: bool,
}

impl<MType> InboundMessageWorker<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    /// Setup a new InboundMessageWorker that will read incoming messages and dispatch them using the message_dispatcher
    /// and inbound_message_broker
    pub fn new(
        config: InboundMessageServiceConfig,
        context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        message_queue_address: InprocAddress,
        message_dispatcher: Arc<MessageDispatcher<MessageContext<MType>>>,
        inbound_message_broker: Arc<InboundMessageBroker<MType>>,
        outbound_message_service: Arc<OutboundMessageService>,
        peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    ) -> Self
    {
        InboundMessageWorker {
            config,
            context,
            node_identity,
            message_queue_address,
            message_dispatcher,
            inbound_message_broker,
            outbound_message_service,
            peer_manager,
            control_receiver: None,
            is_running: false,
        }
    }

    fn lookup_peer(&self, node_id: &NodeId) -> Option<Peer<CommsPublicKey>> {
        self.peer_manager.find_with_node_id(node_id).ok()
    }

    // Main loop of worker thread
    fn start_worker(&mut self) -> Result<(), InboundError> {
        let inbound_connection = Connection::new(&self.context, Direction::Inbound)
            .set_socket_establishment(SocketEstablishment::Bind)
            .establish(&self.message_queue_address)
            .map_err(|e| InboundError::InboundConnectionError(e))?;

        // Retrieve, process and dispatch messages
        loop {
            // Check for control messages
            self.process_control_messages();

            if self.is_running {
                match inbound_connection.receive(self.config.worker_timeout_in_ms.as_millis() as u32) {
                    Ok(mut frame_set) => {
                        // This strips off the two ZeroMQ Identity frames introduced by the transmission to this worker
                        debug!(target: LOG_TARGET, "Received {} frames", frame_set.len());
                        let frame_set: FrameSet = frame_set.drain(1..).collect();

                        match MessageData::try_from(frame_set) {
                            Ok(message_data) => {
                                let peer = match self.lookup_peer(&message_data.source_node_id) {
                                    Some(peer) => peer,
                                    None => {
                                        warn!(
                                            target: LOG_TARGET,
                                            "Received unknown node id from peer connection. Discarding message from \
                                             NodeId={:?}",
                                            message_data.source_node_id
                                        );
                                        continue;
                                    },
                                };

                                let message_context = MessageContext::new(
                                    self.node_identity.clone(),
                                    peer,
                                    message_data.message_envelope,
                                    self.outbound_message_service.clone(),
                                    self.peer_manager.clone(),
                                    self.inbound_message_broker.clone(),
                                );
                                self.message_dispatcher.dispatch(message_context).unwrap_or_else(|e| {
                                    warn!(
                                        target: LOG_TARGET,
                                        "Could not dispatch message to handler - Error: {:?}", e
                                    );
                                });
                            },
                            Err(e) => {
                                // if unable to deserialize the MessageHeader then MUST discard the
                                // message
                                warn!(
                                    target: LOG_TARGET,
                                    "Message discarded as it could not be deserialised - Error: {:?}", e
                                );
                            },
                        }
                    },
                    Err(ConnectionError::Timeout) => (),
                    Err(e) => {
                        error!(
                            target: LOG_TARGET,
                            "Error receiving messages from message queue - Error: {}", e
                        );
                    },
                };
            } else {
                break;
            }
        }
        Ok(())
    }

    /// Start the InboundMessageWorker thread, setup the control channel and start retrieving and dispatching incoming
    /// messages to handlers
    pub fn start(mut self) -> (thread::JoinHandle<()>, SyncSender<ControlMessage>) {
        self.is_running = true;
        let (control_sync_sender, control_receiver) = sync_channel(5);
        self.control_receiver = Some(control_receiver);

        let thread_handle = thread::spawn(move || match self.start_worker() {
            Ok(_) => (),
            Err(e) => {
                error!(target: LOG_TARGET, "Error starting inbound message worker: {:?}", e);
            },
        });
        (thread_handle, control_sync_sender)
    }

    /// Check for control messages to manage worker thread
    fn process_control_messages(&mut self) {
        match &self.control_receiver {
            Some(control_receiver) => {
                if let Some(control_msg) = control_receiver.recv_timeout(self.config.control_timeout_in_ms).ok() {
                    debug!(target: LOG_TARGET, "Received control message: {:?}", control_msg);
                    match control_msg {
                        ControlMessage::Shutdown => {
                            info!(target: LOG_TARGET, "Shutting down worker");
                            self.is_running = false;
                        },
                        _ => {},
                    }
                }
            },
            None => warn!(target: LOG_TARGET, "Control receive not available for worker"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        connection::{Connection, Direction, NetAddress},
        inbound_message_service::comms_msg_handlers::construct_comms_msg_dispatcher,
        message::{
            DomainMessageContext,
            FrameSet,
            Message,
            MessageEnvelope,
            MessageFlags,
            MessageHeader,
            NodeDestination,
        },
        peer_manager::{peer_manager::PeerManager, NodeIdentity, PeerFlags},
        types::{CommsDataStore, CommsPublicKey},
    };
    use serde::{Deserialize, Serialize};
    use std::{
        sync::Arc,
        time::{self, Duration},
    };
    use tari_utilities::message_format::MessageFormat;

    fn init() {
        let _ = simple_logger::init();
    }

    fn pause() {
        thread::sleep(Duration::from_millis(5));
    }

    #[test]
    fn test_dispatch_to_multiple_service_handlers() {
        init();
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));

        // Create Handler Services
        let handler1_inproc_address = InprocAddress::random();
        let handler1_queue_connection = Connection::new(&context, Direction::Inbound)
            .establish(&handler1_inproc_address)
            .unwrap();

        let handler2_inproc_address = InprocAddress::random();
        let handler2_queue_connection = Connection::new(&context, Direction::Inbound)
            .establish(&handler2_inproc_address)
            .unwrap();

        #[derive(Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
        pub enum DomainBrokerType {
            Type1,
            Type2,
        }

        // Create Worker
        let message_queue_address = InprocAddress::random();
        let message_dispatcher = Arc::new(construct_comms_msg_dispatcher::<DomainBrokerType>());
        let inbound_message_broker = Arc::new(
            InboundMessageBroker::new(context.clone())
                .route(DomainBrokerType::Type1, handler1_inproc_address)
                .route(DomainBrokerType::Type2, handler2_inproc_address)
                .start()
                .unwrap(),
        );
        let peer_manager = Arc::new(PeerManager::<CommsPublicKey, CommsDataStore>::new(None).unwrap());
        // Add peer to peer manager
        let peer = Peer::new(
            node_identity.identity.public_key.clone(),
            node_identity.identity.node_id.clone(),
            "127.0.0.1:9000".parse::<NetAddress>().unwrap().into(),
            PeerFlags::empty(),
        );
        peer_manager.add_peer(peer).unwrap();
        let outbound_message_service = Arc::new(
            OutboundMessageService::new(
                context.clone(),
                node_identity.clone(),
                InprocAddress::random(),
                peer_manager.clone(),
            )
            .unwrap(),
        );
        let ims_config = InboundMessageServiceConfig::default();
        let worker = InboundMessageWorker::new(
            ims_config,
            context.clone(),
            node_identity.clone(),
            message_queue_address.clone(),
            message_dispatcher,
            inbound_message_broker,
            outbound_message_service,
            peer_manager,
        );
        let (thread_handle, control_sync_sender) = worker.start();
        // Give worker sufficient time to spinup thread and create a socket
        std::thread::sleep(time::Duration::from_millis(100));

        // Create a client that will send messages to the message queue
        let client_connection = Connection::new(&context, Direction::Outbound)
            .establish(&message_queue_address)
            .unwrap();

        // Construct test message 1
        let message_header = MessageHeader {
            message_type: DomainBrokerType::Type1,
        };
        let message_body = "Test Message Body1".as_bytes().to_vec();
        let message_envelope_body1 = Message::from_message_format(message_header, message_body).unwrap();
        let dest_public_key = node_identity.identity.public_key.clone(); // Send to self
        let message_envelope = MessageEnvelope::construct(
            &node_identity,
            dest_public_key.clone(),
            NodeDestination::Unknown,
            message_envelope_body1.to_binary().unwrap(),
            MessageFlags::NONE,
        )
        .unwrap();
        let message_data1 = MessageData::new(
            NodeId::from_key(&node_identity.identity.public_key).unwrap(),
            message_envelope,
        );
        let mut message1_frame_set = Vec::new();
        message1_frame_set.extend(message_data1.clone().try_into_frame_set().unwrap());

        // Construct test message 2
        let message_header = MessageHeader {
            message_type: DomainBrokerType::Type2,
        };
        let message_body = "Test Message Body2".as_bytes().to_vec();
        let message_envelope_body2 = Message::from_message_format(message_header, message_body).unwrap();
        let message_envelope = MessageEnvelope::construct(
            &node_identity,
            dest_public_key.clone(),
            NodeDestination::Unknown,
            message_envelope_body2.to_binary().unwrap(),
            MessageFlags::NONE,
        )
        .unwrap();
        let message_data2 = MessageData::new(
            NodeId::from_key(&node_identity.identity.public_key).unwrap(),
            message_envelope,
        );
        let mut message2_frame_set = Vec::new();
        message2_frame_set.extend(message_data2.clone().try_into_frame_set().unwrap());

        // Submit Messages to the Worker
        pause();
        client_connection.send(message1_frame_set.clone()).unwrap();
        client_connection.send(message2_frame_set).unwrap();

        // Retrieve messages at handler services
        pause();
        let received_message_data_bytes: FrameSet =
            handler1_queue_connection.receive(100).unwrap().drain(1..).collect();
        let received_domain_message_context =
            DomainMessageContext::from_binary(&received_message_data_bytes[0]).unwrap();
        assert_eq!(received_domain_message_context.message, message_envelope_body1);
        let received_message_data_bytes: FrameSet =
            handler2_queue_connection.receive(100).unwrap().drain(1..).collect();
        let received_domain_message_context =
            DomainMessageContext::from_binary(&received_message_data_bytes[0]).unwrap();
        assert_eq!(received_domain_message_context.message, message_envelope_body2);

        // Test worker clean shutdown
        control_sync_sender.send(ControlMessage::Shutdown).unwrap();
        std::thread::sleep(time::Duration::from_millis(200));
        thread_handle.join().unwrap();
        assert!(client_connection.send(message1_frame_set).is_err());
    }
}

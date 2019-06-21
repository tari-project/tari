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

use super::{error::InboundMessageServiceError, msg_processing_worker::*};
use crate::{
    connection::{
        zmq::{InprocAddress, ZmqContext},
        ConnectionError,
        DealerProxy,
        DealerProxyError,
    },
    dispatcher::DispatchableKey,
    inbound_message_service::inbound_message_broker::InboundMessageBroker,
    message::MessageContext,
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::{peer_manager::PeerManager, NodeIdentity},
    types::{CommsDataStore, CommsPublicKey, MessageDispatcher},
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
#[cfg(test)]
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::{
    sync::Arc,
    thread::{self, JoinHandle},
};

const LOG_TARGET: &'static str = "comms::inbound_message_service";

/// The maximum number of processing worker threads that will be created by the InboundMessageService
const MAX_INBOUND_MSG_PROCESSING_WORKERS: u8 = 4; // TODO read this from config

pub struct InboundMessageService<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    context: ZmqContext,
    node_identity: Arc<NodeIdentity<CommsPublicKey>>,
    inbound_address: InprocAddress,
    dealer_address: InprocAddress,
    message_dispatcher: Arc<MessageDispatcher<MessageContext<MType>>>,
    inbound_message_broker: Arc<InboundMessageBroker<MType>>,
    outbound_message_service: Arc<OutboundMessageService>,
    peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    #[cfg(test)]
    test_sync_sender: Vec<SyncSender<String>>, /* These channels will be to test the pool workers threaded
                                                * operation */
}

impl<MType> InboundMessageService<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    /// Creates a new InboundMessageService that will fairly deal message received on the inbound address to worker
    /// threads
    pub fn new(
        context: ZmqContext,
        node_identity: Arc<NodeIdentity<CommsPublicKey>>,
        inbound_address: InprocAddress,
        message_dispatcher: Arc<MessageDispatcher<MessageContext<MType>>>,
        inbound_message_broker: Arc<InboundMessageBroker<MType>>,
        outbound_message_service: Arc<OutboundMessageService>,
        peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    ) -> Result<Self, ConnectionError>
    {
        Ok(InboundMessageService {
            context,
            node_identity,
            inbound_address,
            dealer_address: InprocAddress::random(),
            message_dispatcher,
            inbound_message_broker,
            outbound_message_service,
            peer_manager,
            #[cfg(test)]
            test_sync_sender: Vec::new(),
        })
    }

    fn start_dealer(&self) -> Result<(), DealerProxyError> {
        DealerProxy::new(self.inbound_address.clone(), self.dealer_address.clone()).proxy(&self.context)
    }

    /// Starts the MsgProcessingWorker threads and the InboundMessageService with Dealer in its own thread
    pub fn start(self) -> JoinHandle<Result<(), InboundMessageServiceError>> {
        thread::spawn(move || {
            // Start workers
            debug!(target: LOG_TARGET, "Starting inbound message service workers");
            #[allow(unused_variables)]
            for i in 0..MAX_INBOUND_MSG_PROCESSING_WORKERS {
                #[allow(unused_mut)] // Allow for testing
                let mut worker = MsgProcessingWorker::new(
                    self.context.clone(),
                    self.node_identity.clone(),
                    self.dealer_address.clone(),
                    self.message_dispatcher.clone(),
                    self.inbound_message_broker.clone(),
                    self.outbound_message_service.clone(),
                    self.peer_manager.clone(),
                );

                #[cfg(test)]
                worker.set_test_channel(self.test_sync_sender[i as usize].clone());

                worker.start();
            }
            // Start dealer
            loop {
                self.start_dealer()
                    .map_err(InboundMessageServiceError::DealerProxyError)?;
            }
            #[allow(unreachable_code)]
            Ok(())
        })
    }

    /// Create a channel pairs for use during testing the workers, the sync sender will be passed into the worker's
    /// threads and the receivers returned to the test function.
    #[cfg(test)]
    fn create_test_channels(&mut self) -> Vec<Receiver<String>> {
        let mut receivers = Vec::new();
        for _ in 0..MAX_INBOUND_MSG_PROCESSING_WORKERS {
            let (tx, rx) = sync_channel::<String>(0);
            self.test_sync_sender.push(tx);
            receivers.push(rx);
        }
        receivers
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::{
            zmq::{InprocAddress, ZmqContext},
            Connection,
            Direction,
            NetAddress,
        },
        inbound_message_service::comms_msg_handlers::*,
        message::{
            DomainMessageContext,
            FrameSet,
            Message,
            MessageData,
            MessageEnvelope,
            MessageFlags,
            MessageHeader,
            NodeDestination,
        },
        peer_manager::{peer_manager::PeerManager, NodeId, NodeIdentity, Peer, PeerFlags},
        types::{CommsDataStore, CommsPublicKey},
    };
    use serde::{Deserialize, Serialize};
    use std::{sync::Arc, thread, time::Duration};
    use tari_utilities::message_format::MessageFormat;

    fn init() {
        let _ = simple_logger::init();
    }

    fn pause() {
        thread::sleep(Duration::from_millis(5));
    }

    #[test]
    fn test_fair_dealing() {
        init();
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));

        // Create a client that will write message to the inbound message pool
        let inbound_msg_queue_address = InprocAddress::random();
        let conn_client = Connection::new(&context, Direction::Outbound)
            .establish(&inbound_msg_queue_address)
            .unwrap();

        // Create Handler Service
        let handler_inproc_address = InprocAddress::random();
        let handler_queue_connection = Connection::new(&context, Direction::Inbound)
            .establish(&handler_inproc_address)
            .unwrap();

        #[derive(Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
        pub enum DomainBrokerType {
            Type1,
        }

        // Create MessageDispatcher, InboundMessageBroker, PeerManager, OutboundMessageService and InboundMessageService
        let message_dispatcher = Arc::new(construct_comms_msg_dispatcher::<DomainBrokerType>());
        let inbound_message_broker = Arc::new(
            InboundMessageBroker::new(context.clone())
                .route(DomainBrokerType::Type1, handler_inproc_address)
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
        let mut inbound_message_service = InboundMessageService::new(
            context,
            node_identity.clone(),
            inbound_msg_queue_address,
            message_dispatcher,
            inbound_message_broker,
            outbound_message_service,
            peer_manager,
        )
        .unwrap();
        // Instantiate the channels that will be used in the tests.
        let receivers = inbound_message_service.create_test_channels();
        inbound_message_service.start();

        // Construct a test message
        let message_header = MessageHeader {
            message_type: DomainBrokerType::Type1,
        };
        let message_body = "Test Message Body1".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
        let dest_public_key = node_identity.identity.public_key.clone(); // Send to self
        let message_envelope = MessageEnvelope::construct(
            &node_identity,
            dest_public_key.clone(),
            NodeDestination::Unknown,
            message_envelope_body.to_binary().unwrap(),
            MessageFlags::NONE,
        )
        .unwrap();
        let message_data = MessageData::new(
            NodeId::from_key(&node_identity.identity.public_key).unwrap(),
            message_envelope,
        );
        let message_data_buffer = message_data.clone().try_into_frame_set().unwrap();

        // Submit Messages to the InboundMessageService
        pause();
        for _ in 0..MAX_INBOUND_MSG_PROCESSING_WORKERS {
            conn_client.send(&message_data_buffer).unwrap();
            pause();
        }

        // Check that all messages reached handler service queue
        for _ in 0..MAX_INBOUND_MSG_PROCESSING_WORKERS {
            let received_message_data_bytes: FrameSet =
                handler_queue_connection.receive(2000).unwrap().drain(1..).collect();
            let received_domain_message_context =
                DomainMessageContext::from_binary(&received_message_data_bytes[0]).unwrap();
            assert_eq!(received_domain_message_context.message, message_envelope_body);
        }

        // Check that each worker thread received work
        // This array marks which workers responded. If fairly dealt each index should be set to 1
        let mut worker_responses = [0; MAX_INBOUND_MSG_PROCESSING_WORKERS as usize];
        // Keep track of how many channels have responded
        let mut resp_count = 0;
        loop {
            // Poll all the channels
            for i in 0..MAX_INBOUND_MSG_PROCESSING_WORKERS as usize {
                if let Ok(_recv) = receivers[i].try_recv() {
                    // If this worker responded multiple times then the message were not fairly dealt so bork the count
                    if worker_responses[i] > 0 {
                        worker_responses[i] = MAX_INBOUND_MSG_PROCESSING_WORKERS + 1;
                    } else {
                        worker_responses[i] = 1;
                    }
                    resp_count += 1;
                }
            }
            // Check to see if all the workers have responded.
            if resp_count >= MAX_INBOUND_MSG_PROCESSING_WORKERS {
                break;
            }
        }
        // Confirm that the messages were fairly dealt
        assert_eq!(
            worker_responses.iter().fold(0, |acc, x| acc + x),
            MAX_INBOUND_MSG_PROCESSING_WORKERS
        );
    }
}

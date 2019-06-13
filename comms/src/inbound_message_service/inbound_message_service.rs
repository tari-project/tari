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
    dispatcher::{DispatchableKey, DomainMessageDispatcher},
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
const MAX_INBOUND_MSG_PROCESSING_WORKERS: u8 = 8; // TODO read this from config

pub struct InboundMessageService<MType>
where
    //    PK: PublicKey,
    //    PK::K: Serialize + DeserializeOwned,
    MType: Serialize + DeserializeOwned,
    MType: DispatchableKey,
{
    context: ZmqContext,
    node_identity: Arc<NodeIdentity<CommsPublicKey>>,
    inbound_address: InprocAddress,
    dealer_address: InprocAddress,
    message_dispatcher: Arc<MessageDispatcher<MessageContext<MType>>>,
    domain_dispatcher: Arc<DomainMessageDispatcher<MType>>,
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
        domain_dispatcher: Arc<DomainMessageDispatcher<MType>>,
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
            domain_dispatcher,
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
                    self.domain_dispatcher.clone(),
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
            connection::EstablishedConnection,
            types::SocketType,
            zmq::{InprocAddress, ZmqContext, ZmqEndpoint},
        },
        dispatcher::{domain::DomainDispatchResolver, DispatchError, HandlerError},
        inbound_message_service::comms_msg_handlers::*,
        message::{
            DomainMessageContext,
            Message,
            MessageContext,
            MessageData,
            MessageEnvelope,
            MessageFlags,
            MessageHeader,
            NodeDestination,
        },
        peer_manager::{peer_manager::PeerManager, NodeIdentity},
        types::{CommsDataStore, CommsPublicKey, MessageDispatcher},
    };
    use serde::{Deserialize, Serialize};
    use std::{convert::TryInto, sync::Arc, thread, time::Duration};
    use tari_crypto::ristretto::RistrettoPublicKey;
    use tari_utilities::message_format::MessageFormat;

    fn init() {
        let _ = simple_logger::init();
    }

    fn pause() {
        thread::sleep(Duration::from_millis(5));
    }

    #[test]
    fn test_new_and_start() {
        init();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
        // Create a client that will write message into message pool
        let context = ZmqContext::new();
        let msg_pool_address = InprocAddress::random();
        let client_socket = context.socket(SocketType::Request).unwrap();
        client_socket.connect(&msg_pool_address.to_zmq_endpoint()).unwrap();
        let conn_client: EstablishedConnection = client_socket.try_into().unwrap();

        // Create a common variable that the workers can change
        static mut HANDLER_RESPONSES: u64 = 0;

        #[derive(Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
        pub enum DomainDispatchType {
            Default,
        }

        fn domain_test_fn(_message_context: DomainMessageContext) -> Result<(), HandlerError> {
            Ok(())
        }

        let domain_dispatcher = Arc::new(
            DomainMessageDispatcher::<DomainDispatchType>::new(DomainDispatchResolver::new()).catch_all(domain_test_fn),
        );

        // Create a testing dispatcher for MessageContext
        fn test_fn(_message_context: MessageContext<DomainDispatchType>) -> Result<(), DispatchError> {
            unsafe {
                HANDLER_RESPONSES += 1;
            }
            Ok(())
        }
        let message_dispatcher = Arc::new(MessageDispatcher::new(InboundMessageServiceResolver {}).catch_all(test_fn));

        // Setup and start InboundMessagePool
        let peer_manager = Arc::new(PeerManager::<CommsPublicKey, CommsDataStore>::new(None).unwrap());
        let outbound_message_service = Arc::new(
            OutboundMessageService::new(
                context.clone(),
                node_identity.clone(),
                InprocAddress::random(),
                peer_manager.clone(),
            )
            .unwrap(),
        );
        let mut inbound_msg_service = InboundMessageService::new(
            context,
            node_identity.clone(),
            msg_pool_address,
            message_dispatcher,
            domain_dispatcher,
            outbound_message_service,
            peer_manager,
        )
        .unwrap();
        // Instantiate the channels that will be used in the tests.
        let receivers = inbound_msg_service.create_test_channels();

        inbound_msg_service.start();
        // Create a new Message Context
        let message_header = MessageHeader {
            message_type: DomainDispatchType::Default,
        };
        let message_body = "Test Message Body".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
        let connection_id: Vec<u8> = vec![0, 1, 2, 3, 4];
        let dest_node_public_key = node_identity.identity.public_key.clone();
        let dest = NodeDestination::Unknown;
        let message_envelope = MessageEnvelope::construct(
            &node_identity,
            dest_node_public_key.clone(),
            dest,
            message_envelope_body.to_binary().unwrap(),
            MessageFlags::NONE,
        )
        .unwrap();
        let message_context = MessageData::<RistrettoPublicKey>::new(connection_id, None, message_envelope);
        let message_context_buffer = message_context.into_frame_set();

        pause();
        for i in 0..8 {
            conn_client.send(&message_context_buffer).unwrap();
            conn_client.receive(2000).unwrap();
            pause();
            unsafe {
                assert_eq!(HANDLER_RESPONSES, i + 1);
            }
        }

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

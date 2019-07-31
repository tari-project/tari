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

use super::{error::InboundError, inbound_message_worker::*};
use crate::{
    connection::{
        peer_connection::ControlMessage,
        zmq::{InprocAddress, ZmqContext},
    },
    dispatcher::DispatchableKey,
    inbound_message_service::inbound_message_broker::InboundMessageBroker,
    message::MessageContext,
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::{peer_manager::PeerManager, NodeIdentity},
    types::MessageDispatcher,
};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    sync::{mpsc::SyncSender, Arc},
    thread::JoinHandle,
    time::Duration,
};
use tari_utilities::thread_join::ThreadJoinWithTimeout;

const LOG_TARGET: &str = "comms::inbound_message_service";

/// Set the maximum waiting time for InboundMessageWorker thread to join
const THREAD_JOIN_TIMEOUT_IN_MS: Duration = Duration::from_millis(100);

#[derive(Clone, Copy)]
pub struct InboundMessageServiceConfig {
    /// Timeout used for receiving messages from the message queue
    pub worker_timeout_in_ms: Duration,
    /// Timeout used for listening for control messages
    pub control_timeout_in_ms: Duration,
}

impl Default for InboundMessageServiceConfig {
    fn default() -> Self {
        InboundMessageServiceConfig {
            worker_timeout_in_ms: Duration::from_millis(100),
            control_timeout_in_ms: Duration::from_millis(5),
        }
    }
}

/// The InboundMessageService manages the inbound message queue. The messages received from different peers are written
/// to, and accumulate in, the inbound message queue. The InboundMessageWorker will then retrieve messages from the
/// queue and dispatch them using the dispatcher, that will check signatures and decrypt the message before being sent
/// to the InboundMessageBroker. The InboundMessageBroker will then send it to the correct handler services.
pub struct InboundMessageService<MType>
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
    peer_manager: Arc<PeerManager>,
    worker_thread_handle: Option<JoinHandle<()>>,
    worker_control_sender: Option<SyncSender<ControlMessage>>,
}

impl<MType> InboundMessageService<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    /// Creates a new InboundMessageService that will receive message on the message_queue_address that it will then
    /// dispatch
    pub fn new(
        config: InboundMessageServiceConfig,
        context: ZmqContext,
        node_identity: Arc<NodeIdentity>,
        message_queue_address: InprocAddress,
        message_dispatcher: Arc<MessageDispatcher<MessageContext<MType>>>,
        inbound_message_broker: Arc<InboundMessageBroker<MType>>,
        outbound_message_service: Arc<OutboundMessageService>,
        peer_manager: Arc<PeerManager>,
    ) -> Self
    {
        InboundMessageService {
            config,
            context: context.clone(),
            node_identity,
            message_queue_address,
            message_dispatcher,
            inbound_message_broker,
            outbound_message_service,
            peer_manager,
            worker_thread_handle: None,
            worker_control_sender: None,
        }
    }

    /// Spawn an InboundMessageWorker for the InboundMessageService
    pub fn start(&mut self) -> Result<(), InboundError> {
        info!(target: LOG_TARGET, "Starting inbound message service");
        let worker = InboundMessageWorker::new(
            self.config,
            self.context.clone(),
            self.node_identity.clone(),
            self.message_queue_address.clone(),
            self.message_dispatcher.clone(),
            self.inbound_message_broker.clone(),
            self.outbound_message_service.clone(),
            self.peer_manager.clone(),
        );
        let (worker_thread_handle, worker_sync_sender) = worker.start()?;
        self.worker_thread_handle = Some(worker_thread_handle);
        self.worker_control_sender = Some(worker_sync_sender);
        Ok(())
    }

    /// Tell the underlying worker thread to shut down
    pub fn shutdown(self) -> Result<(), InboundError> {
        self.worker_control_sender
            .ok_or(InboundError::ControlSenderUndefined)?
            .send(ControlMessage::Shutdown)
            .map_err(|e| InboundError::ControlSendError(format!("Failed to send control message: {:?}", e)))?;
        self.worker_thread_handle
            .ok_or(InboundError::ThreadHandleUndefined)?
            .timeout_join(THREAD_JOIN_TIMEOUT_IN_MS)
            .map_err(InboundError::ThreadJoinError)?;
        Ok(())
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
        peer_manager::{peer_manager::PeerManager, NodeIdentity, Peer, PeerFlags},
    };
    use crossbeam_channel as channel;
    use serde::{Deserialize, Serialize};
    use std::{sync::Arc, thread, time::Duration};
    use tari_storage::key_val_store::HMapDatabase;
    use tari_utilities::message_format::MessageFormat;

    fn pause() {
        thread::sleep(Duration::from_millis(5));
    }

    fn create_message_data_buffer(node_identity: Arc<NodeIdentity>, message_envelope_body: Message) -> Vec<Vec<u8>> {
        let dest_public_key = node_identity.identity.public_key.clone(); // Send to self
        let message_envelope = MessageEnvelope::construct(
            &node_identity,
            dest_public_key.clone(),
            NodeDestination::Unknown,
            message_envelope_body.to_binary().unwrap(),
            MessageFlags::NONE,
        )
        .unwrap();
        let message_data = MessageData::new(node_identity.identity.node_id.clone(), message_envelope);
        message_data.clone().try_into_frame_set().unwrap()
    }

    #[test]
    fn test_message_queue() {
        let context = ZmqContext::new();
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));

        // Create a client that will write message to the inbound message pool
        let message_queue_address = InprocAddress::random();
        let client_connection = Connection::new(&context, Direction::Outbound)
            .establish(&message_queue_address)
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

        // Create MessageDispatcher, InboundMessageBroker, PeerManager, OutboundMessageService and
        let message_dispatcher = Arc::new(construct_comms_msg_dispatcher::<DomainBrokerType>());
        let inbound_message_broker = Arc::new(
            InboundMessageBroker::new(context.clone())
                .route(DomainBrokerType::Type1, handler_inproc_address)
                .start()
                .unwrap(),
        );

        let (message_sender, _) = channel::unbounded();
        let peer_manager = Arc::new(PeerManager::new(HMapDatabase::new()).unwrap());
        // Add peer to peer manager
        let peer = Peer::new(
            node_identity.identity.public_key.clone(),
            node_identity.identity.node_id.clone(),
            "127.0.0.1:9000".parse::<NetAddress>().unwrap().into(),
            PeerFlags::empty(),
        );
        peer_manager.add_peer(peer).unwrap();
        let outbound_message_service =
            Arc::new(OutboundMessageService::new(node_identity.clone(), message_sender, peer_manager.clone()).unwrap());
        let ims_config = InboundMessageServiceConfig::default();
        let mut inbound_message_service = InboundMessageService::new(
            ims_config,
            context,
            node_identity.clone(),
            message_queue_address,
            message_dispatcher,
            inbound_message_broker,
            outbound_message_service,
            peer_manager,
        );
        inbound_message_service.start().unwrap();

        // Submit Messages to the InboundMessageService
        pause();
        let test_message_count = 3;
        let mut message_envelope_body_list = Vec::new();
        for i in 0..test_message_count {
            // Construct a test message
            let message_header = MessageHeader::new(DomainBrokerType::Type1).unwrap();
            // Messages with the same message body will be discarded by the DuplicateMsgCache
            let message_body = format!("Test Message Body {}", i).as_bytes().to_vec();
            let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
            message_envelope_body_list.push(message_envelope_body.clone());
            let message_data_buffer = create_message_data_buffer(node_identity.clone(), message_envelope_body);

            client_connection.send(&message_data_buffer).unwrap();
        }

        // Check that all messages reached handler service queue
        pause();
        for _ in 0..test_message_count {
            let received_message_data_bytes: FrameSet =
                handler_queue_connection.receive(2000).unwrap().drain(1..).collect();
            let received_domain_message_context =
                DomainMessageContext::from_binary(&received_message_data_bytes[0]).unwrap();
            assert!(message_envelope_body_list.contains(&received_domain_message_context.message));
        }

        // Test shutdown control
        inbound_message_service.shutdown().unwrap();
        std::thread::sleep(Duration::from_millis(200));

        let message_header = MessageHeader::new(DomainBrokerType::Type1).unwrap();
        let message_body = "Test Message Body".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
        let message_data_buffer = create_message_data_buffer(node_identity.clone(), message_envelope_body);
        assert!(client_connection.send(&message_data_buffer).is_err());
    }
}

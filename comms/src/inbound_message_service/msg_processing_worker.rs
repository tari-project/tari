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

use super::error::InboundMessageServiceError;
use crate::{
    connection::{
        types::SocketType,
        zmq::{InprocAddress, ZmqContext, ZmqEndpoint},
    },
    dispatcher::{DispatchableKey, DomainMessageDispatcher},
    message::{MessageContext, MessageData},
    outbound_message_service::outbound_message_service::OutboundMessageService,
    peer_manager::{peer_manager::PeerManager, NodeIdentity},
    types::{CommsDataStore, CommsPublicKey, MessageDispatcher},
};
use serde::{de::DeserializeOwned, Serialize};
#[cfg(test)]
use std::sync::mpsc::SyncSender;
use std::{
    convert::TryFrom,
    sync::Arc,
    thread::{self, JoinHandle},
};

pub struct MsgProcessingWorker<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    context: ZmqContext,
    node_identity: Arc<NodeIdentity<CommsPublicKey>>,
    inbound_address: InprocAddress,
    message_dispatcher: Arc<MessageDispatcher<MessageContext<MType>>>,
    domain_dispatcher: Arc<DomainMessageDispatcher<MType>>,
    outbound_message_service: Arc<OutboundMessageService>,
    peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    #[cfg(test)]
    test_sync_sender: Option<SyncSender<String>>,
}

impl<MType> MsgProcessingWorker<MType>
where
    MType: DispatchableKey,
    MType: Serialize + DeserializeOwned,
{
    /// Setup a new MsgProcessingWorker that will read incoming messages and dispatch them using the message_dispatcher
    pub fn new(
        context: ZmqContext,
        node_identity: Arc<NodeIdentity<CommsPublicKey>>,
        inbound_address: InprocAddress,
        message_dispatcher: Arc<MessageDispatcher<MessageContext<MType>>>,
        domain_dispatcher: Arc<DomainMessageDispatcher<MType>>,
        outbound_message_service: Arc<OutboundMessageService>,
        peer_manager: Arc<PeerManager<CommsPublicKey, CommsDataStore>>,
    ) -> Self
    {
        MsgProcessingWorker {
            context,
            node_identity,
            inbound_address,
            message_dispatcher,
            domain_dispatcher,
            outbound_message_service,
            peer_manager,
            #[cfg(test)]
            test_sync_sender: None,
        }
    }

    fn start_worker(&self) -> Result<(), InboundMessageServiceError> {
        // Establish connection inbound socket
        let inbound_socket = self
            .context
            .socket(SocketType::Reply)
            .map_err(|e| InboundMessageServiceError::InboundSocketError(e))?;
        inbound_socket
            .connect(&self.inbound_address.to_zmq_endpoint())
            .map_err(|e| InboundMessageServiceError::InboundConnectionError(e))?;
        // Retrieve, process and dispatch messages
        loop {
            #[cfg(test)]
            let sync_sender = self.test_sync_sender.clone();

            let frames = match inbound_socket.recv_multipart(0) {
                Ok(frames) => frames,
                Err(_e) => {
                    inbound_socket.send("FAILED".as_bytes(), 0).unwrap_or_else(|_e| {
                        (/*TODO Log Error: failed to receive message*/)
                    });
                    break; // Attempt to reconnect to socket
                },
            };

            match MessageData::try_from(frames) {
                Ok(message_data) => {
                    let message_context = MessageContext::new(
                        self.node_identity.clone(),
                        message_data,
                        self.outbound_message_service.clone(),
                        self.peer_manager.clone(),
                        self.domain_dispatcher.clone(),
                    );

                    // TODO: Provide worker with the current nodes identity by adding it onto MessageContext, I
                    // think it should rather be added as another frame before the message reaches the dealer
                    self.message_dispatcher.dispatch(message_context).unwrap_or_else(|_e| {
                        (/*TODO Log Warning: could not dispatch message to handler*/)
                    });
                    inbound_socket.send("OK".as_bytes(), 0).unwrap_or_else(|_e| {
                        (/*TODO Log Warning: could not return status message*/)
                    });

                    #[cfg(test)]
                    {
                        if let Some(tx) = sync_sender {
                            tx.send("Message dispatched".to_string()).unwrap();
                        }
                    }
                },
                Err(_e) => {
                    // if unable to deserialize the MessageHeader then MUST discard the
                    // message
                    inbound_socket.send("DISCARD_MSG".as_bytes(), 0).unwrap_or_else(|_e| {
                        (/*TODO Log Warning: message could not be deserialised*/)
                    })
                },
            }
        }
        Ok(())
    }

    /// Start the MsgProcessingWorker thread, connect to reply socket, retrieve and dispatch incoming messages to
    /// handlers
    pub fn start(self) -> JoinHandle<Result<(), InboundMessageServiceError>> {
        thread::spawn(move || loop {
            self.start_worker()?;
        })
    }

    #[cfg(test)]
    pub fn set_test_channel(&mut self, tx: SyncSender<String>) {
        self.test_sync_sender = Some(tx);
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        connection::connection::EstablishedConnection,
        dispatcher::{domain::DomainDispatchResolver, DispatchError, HandlerError},
        inbound_message_service::comms_msg_handlers::{CommsDispatchType, InboundMessageServiceResolver},
        message::{DomainMessageContext, Message, MessageEnvelope, MessageFlags, MessageHeader, NodeDestination},
        peer_manager::{peer_manager::PeerManager, NodeIdentity},
        types::{CommsDataStore, CommsPublicKey},
    };
    use serde::{Deserialize, Serialize};
    use std::{convert::TryInto, sync::Arc, time};
    use tari_crypto::ristretto::RistrettoPublicKey;
    use tari_utilities::message_format::MessageFormat;

    #[test]
    fn test_new_and_start() {
        let node_identity = Arc::new(NodeIdentity::random_for_test(None));
        // Create a common variable that the worker can change and the test can read to determine that the message was
        // correctly dispatched
        static mut CALLED_FN_TYPE: CommsDispatchType = CommsDispatchType::Discard;

        #[derive(Debug, Hash, Eq, PartialEq, Serialize, Deserialize)]
        pub enum DomainDispatchType {
            Default,
        }

        fn domain_test_fn(_message_context: DomainMessageContext) -> Result<(), HandlerError> {
            Ok(())
        }

        let domain_dispatcher =
            Arc::new(DomainMessageDispatcher::new(DomainDispatchResolver::new()).catch_all(domain_test_fn));

        fn test_fn_handle(_msg_context: MessageContext<DomainDispatchType>) -> Result<(), DispatchError> {
            unsafe {
                CALLED_FN_TYPE = CommsDispatchType::Handle;
            }
            Ok(())
        }

        let message_dispatcher = Arc::new(
            MessageDispatcher::new(InboundMessageServiceResolver {}).route(CommsDispatchType::Handle, test_fn_handle),
        );

        // Create the message worker
        let context = ZmqContext::new();
        let inbound_address = InprocAddress::random();
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
        let worker = MsgProcessingWorker::new(
            context.clone(),
            node_identity.clone(),
            inbound_address.clone(),
            message_dispatcher,
            domain_dispatcher,
            outbound_message_service,
            peer_manager,
        );
        worker.start();

        // Give worker sufficient time to spinup thread ad create socket
        std::thread::sleep(time::Duration::from_millis(100));

        // Create a dealer that will send the worker messages
        let client_socket = context.socket(SocketType::Request).unwrap();
        assert!(client_socket.bind(&inbound_address.to_zmq_endpoint()).is_ok());
        let conn_outbound: EstablishedConnection = client_socket.try_into().unwrap();

        // Create a new Message Context
        let message_header = MessageHeader {
            message_type: DomainDispatchType::Default,
        };
        let message_body = "Test Message Body".as_bytes().to_vec();
        let message_envelope_body = Message::from_message_format(message_header, message_body).unwrap();
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

        let message_buffer = MessageData::<RistrettoPublicKey> {
            message_envelope,
            connection_id: vec![1u8],
            source_node_identity: None,
        }
        .into_frame_set();
        conn_outbound.send(message_buffer).unwrap();
        assert!(conn_outbound.receive(2000).is_ok());
        unsafe {
            assert_eq!(CALLED_FN_TYPE, CommsDispatchType::Handle);
        }
    }
}

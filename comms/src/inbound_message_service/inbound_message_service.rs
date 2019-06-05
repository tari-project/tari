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

use crate::{
    connection::{
        zmq::{Context, InprocAddress},
        ConnectionError,
        DealerProxy,
        DealerProxyError,
    },
    inbound_message_service::{
        message_context::MessageContext,
        message_dispatcher::MessageDispatcher,
        msg_processing_worker::*,
    },
};
use std::{marker::Send, thread};
use tari_crypto::keys::PublicKey;

#[cfg(test)]
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

/// The maximum number of processing worker threads that will be created by the InboundMessageService
const MAX_MSG_PROCESSING_WORKERS: u8 = 8;

pub struct InboundMessageService<PubKey> {
    context: Context,
    inbound_address: InprocAddress,
    dealer_address: InprocAddress,
    node_identity: PubKey,
    message_dispatcher: MessageDispatcher<MessageContext<PubKey>>,
    #[cfg(test)]
    test_sync_sender: Vec<SyncSender<String>>, /* These channels will be to test the pool workers threaded
                                                * operation */
}

impl<PubKey: PublicKey + Send + 'static> InboundMessageService<PubKey> {
    /// Creates a new InboundMessageService that will fairly deal message received on the inbound address to worker
    /// threads
    pub fn new(
        context: Context,
        inbound_address: InprocAddress,
        node_identity: PubKey,
        message_dispatcher: MessageDispatcher<MessageContext<PubKey>>,
    ) -> Result<InboundMessageService<PubKey>, ConnectionError>
    {
        Ok(InboundMessageService {
            context,
            inbound_address,
            dealer_address: InprocAddress::random(),
            node_identity,
            message_dispatcher,
            #[cfg(test)]
            test_sync_sender: Vec::new(),
        })
    }

    fn start_dealer(&self) -> Result<(), DealerProxyError> {
        DealerProxy::new(self.inbound_address.clone(), self.dealer_address.clone()).proxy(&self.context)
    }

    /// Starts the MsgProcessingWorker threads and the InboundMessageService with Dealer in its own thread
    pub fn start(self) {
        thread::spawn(move || {
            // Start workers
            #[allow(unused_variables)]
            for i in 0..MAX_MSG_PROCESSING_WORKERS {
                #[allow(unused_mut)] // Allow for testing
                let mut worker = MsgProcessingWorker::new(
                    self.context.clone(),
                    self.dealer_address.clone(),
                    self.node_identity.clone(),
                    self.message_dispatcher.clone(),
                );

                #[cfg(test)]
                worker.set_test_channel(self.test_sync_sender[i as usize].clone());

                worker.start();
            }
            // Start dealer
            loop {
                match self.start_dealer() {
                    Ok(_) => (),
                    Err(_e) => (/*TODO Write DealerError as a Log Error*/),
                }
            }
        });
    }

    /// Create a channel pairs for use during testing the workers, the sync sender will be passed into the worker's
    /// threads and the receivers returned to the test function.
    #[cfg(test)]
    fn create_test_channels(&mut self) -> Vec<Receiver<String>> {
        let mut receivers = Vec::new();
        for _ in 0..MAX_MSG_PROCESSING_WORKERS {
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
            zmq::{Context, InprocAddress, ZmqEndpoint},
            SocketType,
        },
        dispatcher::DispatchError,
        inbound_message_service::comms_msg_handlers::*,
        message::{MessageEnvelope, MessageEnvelopeHeader, MessageFlags, NodeDestination},
    };

    use std::{convert::TryInto, time};
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };
    use tari_utilities::message_format::MessageFormat;

    #[test]
    fn test_new_and_start() {
        // Create a client that will write message into message pool
        let context = Context::new();
        let msg_pool_address = InprocAddress::random();
        let client_socket = context.socket(SocketType::Request).unwrap();
        client_socket.connect(&msg_pool_address.to_zmq_endpoint()).unwrap();
        let conn_client: EstablishedConnection = client_socket.try_into().unwrap();

        // Create a common variable that the workers can change
        static mut HANDLER_RESPONSES: u64 = 0;

        // Create a testing dispatcher for MessageContext
        fn test_fn(_message_context: MessageContext<RistrettoPublicKey>) -> Result<(), DispatchError> {
            unsafe {
                HANDLER_RESPONSES += 1;
            }
            Ok(())
        }
        let message_dispatcher = MessageDispatcher::new(InboundMessageServiceResolver {}).catch_all(test_fn);

        // Setup and start InboundMessagePool
        let mut rng = rand::OsRng::new().unwrap();
        let mut inbound_msg_service = InboundMessageService::new(
            context,
            msg_pool_address,
            RistrettoPublicKey::from_secret_key(&RistrettoSecretKey::random(&mut rng)),
            message_dispatcher,
        )
        .unwrap();
        // Instantiate the channels that will be used in the tests.
        let receivers = inbound_msg_service.create_test_channels();

        inbound_msg_service.start();
        // Create a new Message Context
        let connection_id: Vec<u8> = vec![0, 1, 2, 3, 4];
        let _source: Vec<u8> = vec![5, 6, 7, 8, 9];
        let version: Vec<u8> = vec![10];
        let dest: NodeDestination<RistrettoPublicKey> = NodeDestination::Unknown;
        let message_envelope_header: MessageEnvelopeHeader<RistrettoPublicKey> = MessageEnvelopeHeader {
            version: 0,
            source: RistrettoPublicKey::from_secret_key(&RistrettoSecretKey::random(&mut rng)),
            dest,
            signature: vec![0],
            flags: MessageFlags::ENCRYPTED,
        };

        let message_envelope_body: Vec<u8> = "handle".as_bytes().to_vec();

        let message_envelope = MessageEnvelope::new(
            version,
            message_envelope_header.to_binary().unwrap(),
            message_envelope_body,
        );

        let message_context = MessageContext::<RistrettoPublicKey>::new(connection_id, None, message_envelope);

        let message_context_buffer = message_context.into_frame_set();

        for i in 0..8 {
            conn_client.send(&message_context_buffer).unwrap();
            conn_client.receive(2000).unwrap();
            thread::sleep(time::Duration::from_millis(1));
            unsafe {
                assert_eq!(HANDLER_RESPONSES, i + 1);
            }
        }

        // This array marks which worked responded. If fairly dealt each index should be set to 1
        let mut worker_responses = [0; MAX_MSG_PROCESSING_WORKERS as usize];
        // Keep track of how many channels have responded
        let mut resp_count = 0;
        loop {
            // Poll all the channels
            for i in 0..MAX_MSG_PROCESSING_WORKERS as usize {
                if let Ok(_recv) = receivers[i].try_recv() {
                    // If this worker responded multiple times then the message were not fairly dealt so bork the count
                    if worker_responses[i] > 0 {
                        worker_responses[i] = MAX_MSG_PROCESSING_WORKERS + 1;
                    } else {
                        worker_responses[i] = 1;
                    }
                    resp_count += 1;
                }
            }
            // Check to see if all the workers have responded.
            if resp_count >= MAX_MSG_PROCESSING_WORKERS {
                break;
            }
        }
        // Confirm that the messages were fairly dealt
        assert_eq!(
            worker_responses.iter().fold(0, |acc, x| acc + x),
            MAX_MSG_PROCESSING_WORKERS
        );
    }
}

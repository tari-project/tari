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

/// The maximum number of processing worker threads that will be created by the InboundMessageService
const MAX_MSG_PROCESSING_WORKERS: u8 = 8;

pub struct InboundMessageService<PubKey> {
    context: Context,
    inbound_address: InprocAddress,
    dealer_address: InprocAddress,
    node_identity: PubKey,
    message_dispatcher: MessageDispatcher<MessageContext<PubKey>>,
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
        })
    }

    fn start_dealer(&self) -> Result<(), DealerProxyError> {
        DealerProxy::new(self.inbound_address.clone(), self.dealer_address.clone()).proxy(&self.context)
    }

    /// Starts the MsgProcessingWorker threads and the InboundMessageService with Dealer in its own thread
    pub fn start(self) {
        thread::spawn(move || {
            // Start workers
            for _ in 0..MAX_MSG_PROCESSING_WORKERS {
                MsgProcessingWorker::new(
                    self.context.clone(),
                    self.dealer_address.clone(),
                    self.node_identity.clone(),
                    self.message_dispatcher.clone(),
                )
                .start();
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
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        connection::{
            connection::EstablishedConnection,
            message::{IdentityFlags, MessageEnvelopeHeader, NodeDestination},
            zmq::{Context, InprocAddress, ZmqEndpoint},
            SocketType,
        },
        inbound_message_service::{comms_msg_handlers::*, message_dispatcher::DispatchError},
    };
    use std::thread::ThreadId;
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };

    #[test]
    fn test_new_and_start() {
        // TODO: This is very unsafe but currently the only way to get the ThreadID into an integer. Remove and fix when issue is resolved -> https://github.com/rust-lang/rust/issues/52780
        fn thread_id_to_u64(thread_id: ThreadId) -> u64 {
            unsafe { std::mem::transmute::<ThreadId, u64>(thread_id) }
        }

        // Create a client that will write message into message pool
        let context = Context::new();
        let msg_pool_address = InprocAddress::random();
        let client_socket = context.socket(SocketType::Request).unwrap();
        client_socket.connect(&msg_pool_address.to_zmq_endpoint()).unwrap();
        let conn_client = EstablishedConnection { socket: client_socket };

        // Create a common variable that the workers can change
        static mut WORKER_ID: u64 = 0;

        // Create a testing dispatcher for MessageContext
        fn test_fn(_message_context: MessageContext<RistrettoPublicKey>) -> Result<(), DispatchError> {
            unsafe {
                WORKER_ID = thread_id_to_u64(thread::current().id());
            }
            Ok(())
        }
        let message_dispatcher = MessageDispatcher::<MessageContext<RistrettoPublicKey>>::new()
            .route(CommsDispatchType::Handle as u32, test_fn)
            .route(CommsDispatchType::Forward as u32, test_fn)
            .route(CommsDispatchType::Discard as u32, test_fn);

        // Setup and start InboundMessagePool
        let mut rng = rand::OsRng::new().unwrap();
        let _inbound_msg_service = InboundMessageService::new(
            context,
            msg_pool_address,
            RistrettoPublicKey::from_secret_key(&RistrettoSecretKey::random(&mut rng)),
            message_dispatcher,
        )
        .unwrap()
        .start();
        // Create a new Message Context
        let connection_id: Vec<u8> = vec![0, 1, 2, 3, 4];
        let source: Vec<u8> = vec![5, 6, 7, 8, 9];
        let version: Vec<u8> = vec![10];
        let dest: NodeDestination<RistrettoPublicKey> = NodeDestination::Unknown;
        let message_envelope_header: MessageEnvelopeHeader<RistrettoPublicKey> = MessageEnvelopeHeader {
            version: 0,
            source: RistrettoPublicKey::from_secret_key(&RistrettoSecretKey::random(&mut rng)),
            dest,
            signature: vec![0],
            flags: IdentityFlags::ENCRYPTED,
        };
        let message_envelope_body: Vec<u8> = vec![11, 12, 13, 14, 15];
        let message_context = MessageContext::<RistrettoPublicKey>::new(
            connection_id,
            source,
            version,
            None,
            message_envelope_header,
            message_envelope_body,
        );
        let message_context_buffer = message_context.to_frame_set().unwrap();

        // Check that the dealer distributed messages to different threads
        assert!(conn_client.send(&message_context_buffer).is_ok());
        assert!(conn_client.receive(2000).is_ok());
        let prev_worker_id = unsafe { WORKER_ID };

        assert!(conn_client.send(&message_context_buffer).is_ok());
        assert!(conn_client.receive(2000).is_ok());
        assert_ne!(prev_worker_id, unsafe { WORKER_ID });
    }
}

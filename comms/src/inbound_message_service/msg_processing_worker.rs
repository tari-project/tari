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
        types::SocketType,
        zmq::{Context, InprocAddress, ZmqEndpoint, ZmqError},
    },
    inbound_message_service::{message_context::MessageContext, message_dispatcher::MessageDispatcher},
};
use std::{convert::TryFrom, marker::Send, thread};
use tari_crypto::keys::PublicKey;

/// As DealerError is handled in a thread it needs to be written to the error log
#[derive(Debug)]
pub enum WorkerError {
    /// Problem with inbound socket
    InboundSocketError(ZmqError),
    /// Failed to connect to inbound socket
    InboundConnectionError(zmq::Error),
}

pub struct MsgProcessingWorker<PubKey> {
    context: Context,
    inbound_address: InprocAddress,
    #[allow(dead_code)]
    node_identity: PubKey,
    message_dispatcher: MessageDispatcher<MessageContext<PubKey>>,
}

impl<PubKey: PublicKey + Send + 'static> MsgProcessingWorker<PubKey> {
    /// Setup a new MsgProcessingWorker that will read incoming messages and dispatch them using the message_dispatcher
    pub fn new(
        context: Context,
        inbound_address: InprocAddress,
        node_identity: PubKey,
        message_dispatcher: MessageDispatcher<MessageContext<PubKey>>,
    ) -> MsgProcessingWorker<PubKey>
    {
        MsgProcessingWorker {
            context,
            inbound_address,
            node_identity,
            message_dispatcher,
        }
    }

    fn start_worker(&self) -> Result<(), WorkerError> {
        // Establish connection inbound socket
        let inbound_socket = self
            .context
            .socket(SocketType::Reply)
            .map_err(|e| WorkerError::InboundSocketError(e))?;
        inbound_socket
            .connect(&self.inbound_address.to_zmq_endpoint())
            .map_err(|e| WorkerError::InboundConnectionError(e))?;
        // Retrieve, process and dispatch messages
        loop {
            let frames = match inbound_socket.recv_multipart(0) {
                Ok(frames) => frames,
                Err(_e) => {
                    inbound_socket.send("FAILED".as_bytes(), 0).unwrap_or_else(|_e| {
                        (/*TODO Log Error: failed to receive message*/)
                    });
                    break; // Attempt to reconnect to socket
                },
            };

            match MessageContext::try_from(frames) {
                Ok(message_context) => {
                    // TODO: Provide worker with the current nodes identity by adding it onto MessageContext, I
                    // think it should rather be added as another frame before the message reaches the dealer
                    self.message_dispatcher.dispatch(message_context).unwrap_or_else(|_e| {
                        (/*TODO Log Warning: could not dispatch message to handler*/)
                    });
                    inbound_socket.send("OK".as_bytes(), 0).unwrap_or_else(|_e| {
                        (/*TODO Log Warning: could not return status message*/)
                    });
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
    pub fn start(self) {
        thread::spawn(move || {
            loop {
                match self.start_worker() {
                    Ok(_) => (),
                    Err(_e) => (/*TODO Write WorkerError as a Log Error*/),
                }
            }
        });
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use crate::{
        connection::connection::EstablishedConnection,
        dispatcher::DispatchError,
        inbound_message_service::comms_msg_handlers::{CommsDispatchType, InboundMessageServiceResolver},
        message::MessageEnvelope,
    };
    use std::time;
    use tari_crypto::{
        keys::{PublicKey, SecretKey},
        ristretto::{RistrettoPublicKey, RistrettoSecretKey},
    };

    #[test]
    fn test_new_and_start() {
        // Create a common variable that the worker can change and the test can read to determine that the message was
        // correctly dispatched
        static mut CALLED_FN_TYPE: CommsDispatchType = CommsDispatchType::Discard;

        // Setup a test message dispatcher
        //        fn test_fn_forward(_msg_data: MessageContext<RistrettoPublicKey>) -> Result<(), DispatchError> {
        //            unsafe {
        //                CALLED_FN_TYPE = CommsDispatchType::Forward;
        //            }
        //            Ok(())
        //        }

        fn test_fn_handle(_msg_data: MessageContext<RistrettoPublicKey>) -> Result<(), DispatchError> {
            unsafe {
                CALLED_FN_TYPE = CommsDispatchType::Handle;
            }
            Ok(())
        }

        let message_dispatcher =
            MessageDispatcher::new(InboundMessageServiceResolver {}).route(CommsDispatchType::Handle, test_fn_handle);

        // Create the message worker
        let context = Context::new();
        let inbound_address = InprocAddress::random();
        let mut rng = rand::OsRng::new().unwrap();
        let node_identity = RistrettoPublicKey::from_secret_key(&RistrettoSecretKey::random(&mut rng));
        let worker = MsgProcessingWorker::new(
            context.clone(),
            inbound_address.clone(),
            node_identity,
            message_dispatcher,
        );
        worker.start();

        // Give worker sufficient time to spinup thread ad create socket
        std::thread::sleep(time::Duration::from_millis(100));

        // Create a dealer that will send the worker messages
        let client_socket = context.socket(SocketType::Request).unwrap();
        assert!(client_socket.bind(&inbound_address.to_zmq_endpoint()).is_ok());
        let conn_outbound: EstablishedConnection = client_socket.into();

        let message_buffer = MessageContext::<RistrettoPublicKey> {
            message_envelope: MessageEnvelope::new(vec![1u8], vec![1u8], "handle".as_bytes().to_vec()),
            connection_id: vec![1u8],
            node_identity: None,
        }
        .into_frame_set();
        conn_outbound.send(message_buffer).unwrap();
        assert!(conn_outbound.receive(2000).is_ok());
        unsafe {
            assert_eq!(CALLED_FN_TYPE, CommsDispatchType::Handle);
        }
    }
}

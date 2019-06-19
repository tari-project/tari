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
        connection::EstablishedConnection,
        zmq::{InprocAddress, ZmqContext},
        Connection,
        ConnectionError,
        Direction,
        SocketEstablishment,
    },
    dispatcher::DispatchableKey,
    message::FrameSet,
};
use derive_error::Error;
use std::{collections::HashMap, sync::Mutex};

#[derive(Debug, Error)]
pub enum BrokerError {
    /// The route was not defined for the specific message type
    RouteNotDefined,
    /// Problem communicating with the registered manager pool
    ConnectionError(ConnectionError),
    /// The Thread Safety has been breached and data access has become poisoned
    PoisonedAccess,
}

/// The InboundMessageBroker stores a set of registered routes that maps a message_type to a destination handler
/// service. It maintains connections with each registered handler service and can dispatch messages with a set message
/// type to the correct handler service.
pub struct InboundMessageBroker<MType> {
    context: ZmqContext,
    inproc_addresses: Vec<InprocAddress>,
    type_to_index_hm: HashMap<MType, usize>,
    connections: Vec<Mutex<EstablishedConnection>>,
}

impl<MType> InboundMessageBroker<MType>
where MType: DispatchableKey
{
    /// Create a new InboundMessageBroker with an empty routing table
    pub fn new(context: ZmqContext) -> InboundMessageBroker<MType> {
        InboundMessageBroker {
            context,
            inproc_addresses: Vec::new(),
            type_to_index_hm: HashMap::new(),
            connections: Vec::new(),
        }
    }

    /// Add a new route to a handler service that maps a message type to the destination inproc address of the handler
    /// service
    pub fn route(mut self, message_type: MType, inproc_address: InprocAddress) -> Self {
        let index = match self.inproc_addresses.iter().position(|r| *r == inproc_address) {
            Some(index) => {
                // Reuse inproc for new route
                index
            },
            None => {
                // Use unique inproc for new route
                self.inproc_addresses.push(inproc_address);
                self.inproc_addresses.len() - 1
            },
        };
        self.type_to_index_hm.insert(message_type, index);
        self
    }

    /// Start the InboundMessageBroker by establishing each connection with each registered peer
    pub fn start(mut self) -> Result<Self, BrokerError> {
        for inproc_address in &self.inproc_addresses {
            self.connections.push(Mutex::new(
                Connection::new(&self.context, Direction::Outbound)
                    .set_socket_establishment(SocketEstablishment::Connect)
                    .establish(inproc_address)
                    .map_err(|e| BrokerError::ConnectionError(e))?,
            ));
        }
        Ok(self)
    }

    /// Dispatch the provided message to the handler service registered with the specified message_type
    pub fn dispatch(&self, message_type: MType, msg: &FrameSet) -> Result<(), BrokerError> {
        let index = *self
            .type_to_index_hm
            .get(&message_type)
            .ok_or(BrokerError::RouteNotDefined)?;
        self.connections[index]
            .lock()
            .map_err(|_| BrokerError::PoisonedAccess)?
            .send(msg)
            .map_err(|e| BrokerError::ConnectionError(e))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tari_utilities::message_format::MessageFormat;

    #[test]
    fn test_new_and_dispatch() {
        let context = ZmqContext::new();
        // Create handler services
        let handler1_inproc_address = InprocAddress::random();
        let handler1_queue_connection = Connection::new(&context, Direction::Inbound)
            .establish(&handler1_inproc_address)
            .unwrap();

        let handler2_inproc_address = InprocAddress::random();
        let handler2_queue_connection = Connection::new(&context, Direction::Inbound)
            .establish(&handler2_inproc_address)
            .unwrap();

        // Setup InboundMessageBroker
        let message_type1 = 1;
        let message_type2 = 2;
        let message_type3 = 3;
        let message_type4 = 4;
        let inbound_message_broker = InboundMessageBroker::new(context)
            .route(message_type1, handler1_inproc_address)
            .route(message_type2, handler2_inproc_address.clone())
            .route(message_type3, handler2_inproc_address)
            .start()
            .unwrap();

        // Let the broker dispatch the messages to the correct managers
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct TestMessage {
            body: String,
        }
        let test_msg1 = TestMessage {
            body: "test message1".to_string(),
        };
        let test_msg2 = TestMessage {
            body: "test message2".to_string(),
        };
        let test_msg3 = TestMessage {
            body: "test message3".to_string(),
        };
        let test_msg4 = TestMessage {
            body: "test message4".to_string(),
        };
        inbound_message_broker
            .dispatch(message_type1, &vec![test_msg1.to_binary().unwrap()])
            .unwrap();
        inbound_message_broker
            .dispatch(message_type2, &vec![test_msg2.to_binary().unwrap()])
            .unwrap();
        inbound_message_broker
            .dispatch(message_type3, &vec![test_msg3.to_binary().unwrap()])
            .unwrap();
        assert!(inbound_message_broker
            .dispatch(message_type4, &vec![test_msg4.to_binary().unwrap()])
            .is_err());

        // Test that the managers received their messages
        let received_msg_bytes: FrameSet = handler1_queue_connection.receive(100).unwrap().drain(1..).collect();
        let received_msg = TestMessage::from_binary(&received_msg_bytes[0]).unwrap();
        assert_eq!(received_msg, test_msg1);
        let received_msg_bytes: FrameSet = handler2_queue_connection.receive(100).unwrap().drain(1..).collect();
        let received_msg = TestMessage::from_binary(&received_msg_bytes[0]).unwrap();
        assert_eq!(received_msg, test_msg2);
        let received_msg_bytes: FrameSet = handler2_queue_connection.receive(100).unwrap().drain(1..).collect();
        let received_msg = TestMessage::from_binary(&received_msg_bytes[0]).unwrap();
        assert_eq!(received_msg, test_msg3);

        // Ensure queues are empty
        assert!(handler1_queue_connection.receive(10).is_err());
        assert!(handler2_queue_connection.receive(10).is_err());
    }
}

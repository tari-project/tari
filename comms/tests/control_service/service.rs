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

use crate::support::utils;
use serde::{Deserialize, Serialize};

use std::{convert::TryInto, sync::RwLock, thread, time::Duration};

use tari_comms::{
    connection::{curve_keypair, Connection, Context, Direction, Linger},
    control_service::{
        messages::EstablishConnection,
        ControlService,
        ControlServiceConfig,
        ControlServiceError,
        ControlServiceMessageContext,
    },
    dispatcher::{DispatchError, DispatchResolver, Dispatcher},
    message::{Message, MessageEnvelope, MessageError, MessageFlags, MessageHeader, NodeDestination},
    types::MessageEnvelopeHeader,
};
use tari_crypto::{keys::PublicKey, ristretto::RistrettoPublicKey};
use tari_utilities::message_format::MessageFormat;

#[derive(Eq, PartialEq, Hash, Serialize, Deserialize)]
enum MessageType {
    EstablishConnection = 1,
    InvalidMessage = 2,
}

lazy_static! {
    static ref HANDLER_CALL_COUNT: RwLock<u8> = RwLock::new(0);
}

fn test_handler(context: ControlServiceMessageContext) -> Result<(), ControlServiceError> {
    let msg: EstablishConnection = context
        .message
        .to_message()
        .map_err(|err| DispatchError::HandlerError(format!("Failed to parse message: {}", err)))?;

    assert!(msg.address.is_ip());

    let mut lock = HANDLER_CALL_COUNT.write().unwrap();
    *lock += 1;

    Ok(())
}

struct CustomTestResolver;

impl DispatchResolver<MessageType, ControlServiceMessageContext> for CustomTestResolver {
    fn resolve(&self, context: &ControlServiceMessageContext) -> Result<MessageType, DispatchError> {
        let header: MessageHeader<MessageType> = context
            .message
            .to_header()
            .map_err(|err| DispatchError::HandlerError(format!("Failed to parse header: {}", err)))?;

        Ok(header.message_type)
    }
}

fn construct_envelope<T: MessageFormat>(
    pk: RistrettoPublicKey,
    message_type: MessageType,
    msg: T,
) -> Result<MessageEnvelope, MessageError>
{
    let msg_header = MessageHeader { message_type };
    let msg: Message = (msg_header, msg).try_into()?;
    let envelope_header = MessageEnvelopeHeader {
        source: pk,
        dest: NodeDestination::Unknown,
        flags: MessageFlags::empty(),
        signature: vec![0],
        version: 0,
    };

    Ok(MessageEnvelope::new(
        vec![0],
        envelope_header.to_binary()?,
        msg.to_binary()?,
    ))
}

fn poll_handler_call_count_change(ms: u64) -> Option<u8> {
    let initial = {
        let lock = HANDLER_CALL_COUNT.read().unwrap();
        *lock
    };
    for _i in 0..ms {
        {
            let lock = HANDLER_CALL_COUNT.read().unwrap();
            if *lock != initial {
                return Some(*lock);
            }
        }
        thread::sleep(Duration::from_millis(1))
    }

    None
}

#[test]
fn recv_message() {
    // tari_common::logging::initialize_logger::initialize_logger();
    let context = Context::new();
    let control_service_address = utils::find_available_tcp_net_address("127.0.0.1").unwrap();

    let dispatcher = Dispatcher::new(CustomTestResolver {}).route(MessageType::EstablishConnection, test_handler);

    let service = ControlService::new(&context)
        .configure(ControlServiceConfig {
            socks_proxy_address: None,
            listener_address: control_service_address.clone(),
        })
        .serve(dispatcher)
        .unwrap();

    // A "remote" node sends an EstablishConnection message to this node's control port
    let requesting_node_address = utils::find_available_tcp_net_address("127.0.0.1").unwrap();
    let (_secret_key, public_key) = RistrettoPublicKey::random_keypair(&mut rand::OsRng::new().unwrap());
    let (_sk, server_pk) = curve_keypair::generate().unwrap();
    let msg = EstablishConnection {
        address: requesting_node_address,
        public_key: public_key.clone(),
        server_key: server_pk,
    };

    let envelope = construct_envelope(public_key, MessageType::EstablishConnection, msg).unwrap();

    let remote_conn = Connection::new(&context, Direction::Outbound)
        .set_linger(Linger::Indefinitely)
        .establish(&control_service_address)
        .unwrap();

    remote_conn.send_sync(envelope.clone().into_frame_set()).unwrap();

    let call_count = poll_handler_call_count_change(500).unwrap();
    assert_eq!(1, call_count);

    remote_conn.send_sync(envelope.into_frame_set()).unwrap();
    let call_count = poll_handler_call_count_change(500).unwrap();
    assert_eq!(2, call_count);

    service.shutdown().unwrap();
}

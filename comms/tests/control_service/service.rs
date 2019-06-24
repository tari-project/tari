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

use crate::support::factories::{self, TestFactory};
use std::{
    sync::{mpsc::channel, Arc, RwLock},
    thread,
    time::Duration,
};
use tari_comms::{
    connection::{
        types::{Direction, Linger},
        Connection,
        CurveEncryption,
        NetAddress,
        ZmqContext,
    },
    control_service::{
        handlers::ControlServiceResolver,
        ControlService,
        ControlServiceConfig,
        ControlServiceError,
        ControlServiceMessageContext,
        ControlServiceMessageType,
    },
    dispatcher::{DispatchError, Dispatcher},
    message::{
        p2p::EstablishConnection,
        Message,
        MessageEnvelope,
        MessageEnvelopeHeader,
        MessageError,
        MessageFlags,
        MessageHeader,
        NodeDestination,
    },
    peer_manager::{NodeId, NodeIdentity},
    types::{CommsCipher, CommsPublicKey, CommsSecretKey},
};
use tari_crypto::keys::DiffieHellmanSharedSecret;
use tari_utilities::{byte_array::ByteArray, ciphers::cipher::Cipher, message_format::MessageFormat};

lazy_static! {
    static ref HANDLER_CALL_COUNT: RwLock<u8> = RwLock::new(0);
}

fn test_handler(context: ControlServiceMessageContext<u8>) -> Result<(), ControlServiceError> {
    let msg: EstablishConnection<CommsPublicKey> = context
        .message
        .to_message()
        .map_err(|err| DispatchError::HandlerError(format!("Failed to parse message: {}", err)))?;

    assert!(msg.address.is_ip());

    let mut lock = HANDLER_CALL_COUNT.write().unwrap();
    *lock += 1;

    Ok(())
}

fn encrypt_message(secret_key: &CommsSecretKey, public_key: &CommsPublicKey, msg: Vec<u8>) -> Vec<u8> {
    let shared_secret = CommsPublicKey::shared_secret(secret_key, public_key);
    CommsCipher::seal_with_integral_nonce(&msg, &shared_secret.to_vec()).unwrap()
}

fn construct_envelope<T: MessageFormat>(
    node_identity: &Arc<NodeIdentity<CommsPublicKey>>,
    message_type: ControlServiceMessageType,
    msg: T,
) -> Result<MessageEnvelope, MessageError>
{
    let msg_header = MessageHeader { message_type };
    let msg = Message::from_message_format(msg_header, msg)?;
    let envelope_header = MessageEnvelopeHeader {
        source: node_identity.identity.public_key.clone(),
        dest: NodeDestination::NodeId(node_identity.identity.node_id.clone()),
        flags: MessageFlags::ENCRYPTED,
        signature: vec![0],
        version: 0,
    };

    let encrypted_body = encrypt_message(
        &node_identity.secret_key,
        &node_identity.identity.public_key,
        msg.to_binary()?,
    );

    Ok(MessageEnvelope::new(
        vec![0],
        envelope_header.to_binary()?,
        encrypted_body,
    ))
}

fn poll_handler_call_count_change(initial: u8, ms: u64) -> Option<u8> {
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
    let context = ZmqContext::new();
    let connection_manager = Arc::new(
        factories::connection_manager::create()
            .with_context(context.clone())
            .build()
            .unwrap(),
    );
    let control_service_address = factories::net_address::create().build().unwrap();
    let node_identity = Arc::new(
        factories::node_identity::create::<CommsPublicKey>()
            .with_control_service_address(control_service_address.clone())
            .build()
            .unwrap(),
    );

    let dispatcher = Dispatcher::new(ControlServiceResolver::new())
        .route(ControlServiceMessageType::EstablishConnection, test_handler);

    let service = ControlService::new(context.clone(), node_identity.clone(), ControlServiceConfig {
        socks_proxy_address: None,
        listener_address: control_service_address.clone(),
        accept_message_type: 123,
        requested_outbound_connection_timeout: Duration::from_millis(2000),
    })
    .with_custom_dispatcher(dispatcher)
    .serve(connection_manager)
    .unwrap();

    // A "remote" node sends an EstablishConnection message to this node's control port
    let requesting_node_address = factories::net_address::create().build().unwrap();
    //    let (secret_key, public_key) = RistrettoPublicKey::random_keypair(&mut rand::OsRng::new().unwrap());
    let (_sk, server_pk) = CurveEncryption::generate_keypair().unwrap();
    let msg = EstablishConnection {
        address: requesting_node_address,
        node_id: NodeId::from_key(&node_identity.identity.public_key).unwrap(),
        public_key: node_identity.identity.public_key.clone(),
        server_key: server_pk,
        control_service_address: control_service_address.clone(),
    };

    let envelope = construct_envelope(&node_identity, ControlServiceMessageType::EstablishConnection, msg).unwrap();

    let remote_conn = Connection::new(&context, Direction::Outbound)
        .set_linger(Linger::Indefinitely)
        .establish(&control_service_address)
        .unwrap();

    let initial = {
        let lock = HANDLER_CALL_COUNT.read().unwrap();
        *lock
    };

    remote_conn.send_sync(envelope.clone().into_frame_set()).unwrap();

    let call_count = poll_handler_call_count_change(initial, 2000).expect("Timeout before handler was called");
    assert_eq!(1, call_count);

    let initial = {
        let lock = HANDLER_CALL_COUNT.read().unwrap();
        *lock
    };

    remote_conn.send_sync(envelope.into_frame_set()).unwrap();
    let call_count = poll_handler_call_count_change(initial, 500).expect("Timeout before handler was called");
    assert_eq!(2, call_count);

    service.shutdown().unwrap();
}

#[test]
fn serve_and_shutdown() {
    let node_identity = Arc::new(factories::node_identity::create::<CommsPublicKey>().build().unwrap());
    let (tx, rx) = channel();
    let context = ZmqContext::new();
    let connection_manager = Arc::new(
        factories::connection_manager::create()
            .with_context(context.clone())
            .build()
            .unwrap(),
    );

    let listener_address: NetAddress = "127.0.0.1:0".parse().unwrap(); // factories::net_address::create().use_os_port().build().unwrap();
    thread::spawn(move || {
        let service = ControlService::new(context, node_identity, ControlServiceConfig {
            listener_address,
            socks_proxy_address: None,
            accept_message_type: 123,
            requested_outbound_connection_timeout: Duration::from_millis(2000),
        })
        .serve(connection_manager)
        .unwrap();

        service.shutdown().unwrap();
        tx.send(()).unwrap();
    });

    // Test that the control service loop ends within 1000ms
    rx.recv_timeout(Duration::from_millis(1000)).unwrap();
}

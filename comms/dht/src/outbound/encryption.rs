// Copyright 2019, The Tari Project
//
// Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
// following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
// disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
// following disclaimer in the documentation and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
// products derived from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
// INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
// WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
// USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use crate::{
    crypt,
    outbound::message::{DhtOutboundMessage, OutboundEncryption},
    proto::envelope::OriginMac,
};
use futures::{task::Context, Future};
use log::*;
use rand::rngs::OsRng;
use std::{sync::Arc, task::Poll};
use tari_comms::{
    message::MessageExt,
    peer_manager::NodeIdentity,
    pipeline::PipelineError,
    types::CommsPublicKey,
    utils::signature,
};
use tari_crypto::{
    keys::PublicKey,
    tari_utilities::{message_format::MessageFormat, ByteArray},
};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::middleware::encryption";

/// This layer is responsible for attempting to decrypt inbound messages.
pub struct EncryptionLayer {
    node_identity: Arc<NodeIdentity>,
}

impl EncryptionLayer {
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        Self { node_identity }
    }
}

impl<S> Layer<S> for EncryptionLayer {
    type Service = EncryptionService<S>;

    fn layer(&self, service: S) -> Self::Service {
        EncryptionService::new(service, Arc::clone(&self.node_identity))
    }
}

/// Responsible for decrypting InboundMessages and passing a DecryptedInboundMessage to the given service
#[derive(Clone)]
pub struct EncryptionService<S> {
    node_identity: Arc<NodeIdentity>,
    inner: S,
}

impl<S> EncryptionService<S> {
    pub fn new(service: S, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            inner: service,
            node_identity,
        }
    }
}

impl<S> Service<DhtOutboundMessage> for EncryptionService<S>
where S: Service<DhtOutboundMessage, Response = (), Error = PipelineError> + Clone
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, mut message: DhtOutboundMessage) -> Self::Future {
        let next_service = self.inner.clone();
        let node_identity = Arc::clone(&self.node_identity);
        async move {
            trace!(target: LOG_TARGET, "DHT Message flags: {:?}", message.dht_flags);
            match &message.encryption {
                OutboundEncryption::EncryptFor(public_key) => {
                    debug!(target: LOG_TARGET, "Encrypting message for {}", public_key);
                    // Generate ephemeral public/private key pair and ECDH shared secret
                    let (e_sk, e_pk) = CommsPublicKey::random_keypair(&mut OsRng);
                    let shared_ephemeral_secret = crypt::generate_ecdh_secret(&e_sk, &**public_key);
                    // Encrypt the message with the body
                    let encrypted_body =
                        crypt::encrypt(&shared_ephemeral_secret, &message.body).map_err(PipelineError::from_debug)?;

                    // Sign the encrypted message
                    let origin_mac = create_origin_mac(&node_identity, &encrypted_body)?;
                    // Encrypt and set the origin field
                    let encrypted_origin_mac =
                        crypt::encrypt(&shared_ephemeral_secret, &origin_mac).map_err(PipelineError::from_debug)?;
                    message
                        .with_origin_mac(encrypted_origin_mac)
                        .with_ephemeral_public_key(e_pk)
                        .set_body(encrypted_body.into());
                },
                OutboundEncryption::None => {
                    debug!(target: LOG_TARGET, "Encryption not requested for message");

                    if message.include_origin && message.custom_header.is_none() {
                        let origin_mac = create_origin_mac(&node_identity, &message.body)?;
                        message.with_origin_mac(origin_mac);
                    }
                },
            };

            next_service.oneshot(message).await
        }
    }
}

fn create_origin_mac(node_identity: &NodeIdentity, body: &[u8]) -> Result<Vec<u8>, PipelineError> {
    let signature =
        signature::sign(&mut OsRng, node_identity.secret_key().clone(), body).map_err(PipelineError::from_debug)?;

    let mac = OriginMac {
        public_key: node_identity.public_key().to_vec(),
        signature: signature.to_binary().map_err(PipelineError::from_debug)?,
    };
    Ok(mac.to_encoded_bytes())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{create_outbound_message, make_node_identity, service_spy},
    };
    use futures::executor::block_on;
    use tari_comms::{peer_manager::NodeId, types::CommsPublicKey};
    use tari_test_utils::panic_context;

    #[test]
    fn no_encryption() {
        let spy = service_spy();
        let node_identity = make_node_identity();
        let mut encryption = EncryptionLayer::new(Arc::clone(&node_identity)).layer(spy.to_service::<PipelineError>());

        panic_context!(cx);
        assert!(encryption.poll_ready(&mut cx).is_ready());

        let body = b"A";
        let msg = create_outbound_message(body);
        block_on(encryption.call(msg)).unwrap();

        let msg = spy.pop_request().unwrap();
        assert_eq!(msg.body.to_vec(), body);
        assert_eq!(msg.destination_peer.node_id, NodeId::default());
        assert!(msg.ephemeral_public_key.is_none())
    }

    #[test]
    fn encryption() {
        let spy = service_spy();
        let node_identity = make_node_identity();
        let mut encryption = EncryptionLayer::new(Arc::clone(&node_identity)).layer(spy.to_service::<PipelineError>());

        panic_context!(cx);
        assert!(encryption.poll_ready(&mut cx).is_ready());

        let body = b"A";
        let mut msg = create_outbound_message(body);
        msg.dht_flags = DhtMessageFlags::ENCRYPTED;
        msg.encryption = OutboundEncryption::EncryptFor(Box::new(CommsPublicKey::default()));
        block_on(encryption.call(msg)).unwrap();

        let msg = spy.pop_request().unwrap();
        assert_ne!(msg.body.to_vec(), body);
        assert_eq!(msg.destination_peer.node_id, NodeId::default());
        assert!(msg.ephemeral_public_key.is_some())
    }
}

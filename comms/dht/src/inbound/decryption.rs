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
    envelope::{DhtMessageFlags, DhtMessageHeader},
    inbound::message::{DecryptedDhtMessage, DhtInboundMessage},
    proto::envelope::OriginMac,
};
use derive_error::Error;
use futures::{task::Context, Future};
use log::*;
use prost::Message;
use std::{sync::Arc, task::Poll};
use tari_comms::{
    message::EnvelopeBody,
    peer_manager::NodeIdentity,
    pipeline::PipelineError,
    types::CommsPublicKey,
    utils::signature,
};
use tari_utilities::ByteArray;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::middleware::decryption";

#[derive(Error, Debug)]
enum DecryptionError {
    /// Failed to validate origin MAC signature
    OriginMacInvalidSignature,
    /// Origin MAC contained an invalid public key
    OriginMacInvalidPublicKey,
    /// Origin MAC not provided for encrypted message
    OriginMacNotProvided,
    /// Failed to decrypt origin MAC
    OriginMacDecryptedFailed,
    /// Failed to decode clear-text origin MAC
    OriginMacClearTextDecodeFailed,
    /// Failed to decrypt message body
    MessageBodyDecryptionFailed,
}

/// This layer is responsible for attempting to decrypt inbound messages.
pub struct DecryptionLayer {
    node_identity: Arc<NodeIdentity>,
}

impl DecryptionLayer {
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        Self { node_identity }
    }
}

impl<S> Layer<S> for DecryptionLayer {
    type Service = DecryptionService<S>;

    fn layer(&self, service: S) -> Self::Service {
        DecryptionService::new(service, Arc::clone(&self.node_identity))
    }
}

/// Responsible for decrypting InboundMessages and passing a DecryptedInboundMessage to the given service
#[derive(Clone)]
pub struct DecryptionService<S> {
    node_identity: Arc<NodeIdentity>,
    inner: S,
}

impl<S> DecryptionService<S> {
    pub fn new(service: S, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            inner: service,
            node_identity,
        }
    }
}

impl<S> Service<DhtInboundMessage> for DecryptionService<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtInboundMessage) -> Self::Future {
        Self::handle_message(self.inner.clone(), Arc::clone(&self.node_identity), msg)
    }
}

impl<S> DecryptionService<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    async fn handle_message(
        next_service: S,
        node_identity: Arc<NodeIdentity>,
        message: DhtInboundMessage,
    ) -> Result<(), PipelineError>
    {
        let dht_header = &message.dht_header;

        if !dht_header.flags.contains(DhtMessageFlags::ENCRYPTED) {
            return Self::success_not_encrypted(next_service, message).await;
        }
        trace!(
            target: LOG_TARGET,
            "Decrypting message {} (Trace: {})",
            message.tag,
            message.dht_header.message_tag
        );

        let e_pk = dht_header
            .ephemeral_public_key
            .as_ref()
            // TODO: #banheuristic - encrypted message sent without ephemeral public key
            .ok_or("Ephemeral public key not provided for encrypted message")?;

        let shared_secret = crypt::generate_ecdh_secret(node_identity.secret_key(), e_pk);

        // Decrypt and verify the origin
        let authenticated_origin = match Self::attempt_decrypt_origin_mac(&shared_secret, dht_header) {
            Ok((public_key, signature)) => {
                // If this fails, discard the message because we decrypted and deserialized the message with our shared
                // ECDH secret but the message could not be authenticated
                Self::authenticate_origin_mac(&public_key, &signature, &message.body)
                    .map_err(PipelineError::from_debug)?;
                public_key
            },
            Err(err) => {
                trace!(
                    target: LOG_TARGET,
                    "Unable to decrypt message origin: {}, {} (Trace: {})",
                    err,
                    message.tag,
                    message.dht_header.message_tag
                );
                return Self::decryption_failed(next_service, &node_identity, message).await;
            },
        };

        trace!(
            target: LOG_TARGET,
            "Attempting to decrypt message body from origin public key '{}', {} (Trace: {})",
            authenticated_origin,
            message.tag,
            message.dht_header.message_tag
        );
        match Self::attempt_decrypt_message_body(&shared_secret, &message.body) {
            Ok(message_body) => {
                debug!(
                    target: LOG_TARGET,
                    "Message successfully decrypted, {} (Trace: {})", message.tag, message.dht_header.message_tag
                );
                let msg = DecryptedDhtMessage::succeeded(message_body, Some(authenticated_origin), message);
                next_service.oneshot(msg).await
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Unable to decrypt message: {}, {} (Trace: {})", err, message.tag, message.dht_header.message_tag
                );
                Self::decryption_failed(next_service, &node_identity, message).await
            },
        }
    }

    fn attempt_decrypt_origin_mac(
        shared_secret: &CommsPublicKey,
        dht_header: &DhtMessageHeader,
    ) -> Result<(CommsPublicKey, Vec<u8>), DecryptionError>
    {
        let encrypted_origin_mac = Some(&dht_header.origin_mac)
            .filter(|b| !b.is_empty())
            // TODO: #banheuristic - this should not have been sent/propagated
            .ok_or_else(|| DecryptionError::OriginMacNotProvided)?;
        let decrypted_bytes = crypt::decrypt(shared_secret, encrypted_origin_mac)
            .map_err(|_| DecryptionError::OriginMacDecryptedFailed)?;
        let origin_mac =
            OriginMac::decode(decrypted_bytes.as_slice()).map_err(|_| DecryptionError::OriginMacDecryptedFailed)?;
        // Check the public key here, because it is possible (rare but possible) for an failed decrypted message to pass
        // protobuf decoding of the relatively simple OriginMac struct but with invalid data
        let public_key = CommsPublicKey::from_bytes(&origin_mac.public_key)
            .map_err(|_| DecryptionError::OriginMacInvalidPublicKey)?;
        Ok((public_key, origin_mac.signature))
    }

    fn authenticate_origin_mac(
        public_key: &CommsPublicKey,
        signature: &[u8],
        body: &[u8],
    ) -> Result<(), DecryptionError>
    {
        if signature::verify(public_key, signature, body).unwrap_or(false) {
            Ok(())
        } else {
            Err(DecryptionError::OriginMacInvalidSignature)
        }
    }

    fn attempt_decrypt_message_body(
        shared_secret: &CommsPublicKey,
        message_body: &[u8],
    ) -> Result<EnvelopeBody, DecryptionError>
    {
        let decrypted =
            crypt::decrypt(shared_secret, message_body).map_err(|_| DecryptionError::MessageBodyDecryptionFailed)?;
        // Deserialization into an EnvelopeBody is done here to determine if the
        // decryption produced valid bytes or not.
        EnvelopeBody::decode(decrypted.as_slice())
            .and_then(|body| {
                // Check if we received a body length of zero
                //
                // In addition to a peer sending a zero-length EnvelopeBody, decoding can erroneously succeed
                // if the decrypted bytes happen to be valid protobuf encoding. This is very possible and
                // the decrypt_inbound_fail test below _will_ sporadically fail without the following check.
                // This is because proto3 will set fields to their default value if they don't exist in a valid
                // encoding.
                //
                // For the parts of EnvelopeBody to be erroneously populated with bytes, all of these
                // conditions would have to be true:
                // 1. field type == 2 (length-delimited)
                // 2. field number == 1
                // 3. the subsequent byte(s) would have to be varint-encoded length which does not overflow
                // 4. the rest of the bytes would have to be valid protobuf encoding
                //
                // The chance of this happening is extremely negligible.
                if body.is_empty() {
                    return Err(prost::DecodeError::new("EnvelopeBody has no parts"));
                }
                Ok(body)
            })
            .map_err(|_| DecryptionError::MessageBodyDecryptionFailed)
    }

    async fn success_not_encrypted(next_service: S, message: DhtInboundMessage) -> Result<(), PipelineError> {
        let authenticated_pk = if message.dht_header.origin_mac.is_empty() {
            None
        } else {
            let origin_mac = OriginMac::decode(message.dht_header.origin_mac.as_slice())
                .map_err(|_| PipelineError::from_debug(DecryptionError::OriginMacClearTextDecodeFailed))?;
            let public_key = CommsPublicKey::from_bytes(&origin_mac.public_key)
                .map_err(|_| PipelineError::from_debug(DecryptionError::OriginMacInvalidPublicKey))?;
            Self::authenticate_origin_mac(&public_key, &origin_mac.signature, &message.body)
                .map_err(PipelineError::from_debug)?;
            Some(public_key)
        };

        match EnvelopeBody::decode(message.body.as_slice()) {
            Ok(deserialized) => {
                trace!(
                    target: LOG_TARGET,
                    "Message {} is not encrypted. Passing onto next service (Trace: {})",
                    message.tag,
                    message.dht_header.message_tag
                );
                let msg = DecryptedDhtMessage::succeeded(deserialized, authenticated_pk, message);
                next_service.oneshot(msg).await
            },
            Err(err) => {
                // Message was not encrypted but failed to deserialize - immediately discard
                // TODO: Bad node behaviour?
                debug!(
                    target: LOG_TARGET,
                    "Unable to deserialize message {}: {}. Message will be discarded. (Trace: {})",
                    message.tag,
                    err,
                    message.dht_header.message_tag
                );
                Ok(())
            },
        }
    }

    async fn decryption_failed(
        next_service: S,
        node_identity: &NodeIdentity,
        message: DhtInboundMessage,
    ) -> Result<(), PipelineError>
    {
        if message.dht_header.destination == node_identity.node_id() ||
            message.dht_header.destination == node_identity.public_key()
        {
            // TODO: #banheuristic - the origin of this message sent this node a message we could not decrypt
            warn!(
                target: LOG_TARGET,
                "Received message from peer '{}' that is destined for this node that could not be decrypted. \
                 Discarding message {} (Trace: {})",
                message.source_peer.node_id,
                message.tag,
                message.dht_header.message_tag
            );
            return Err(
                "Message rejected because this node could not decrypt a message that was addressed to it".into(),
            );
        }
        let msg = DecryptedDhtMessage::failed(message);
        next_service.oneshot(msg).await
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{make_dht_inbound_message, make_node_identity, service_fn},
    };
    use futures::{executor::block_on, future};
    use std::sync::Mutex;
    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tari_test_utils::counter_context;

    #[test]
    fn poll_ready() {
        let inner = service_fn(|_: DecryptedDhtMessage| future::ready(Result::<(), PipelineError>::Ok(())));
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(inner, node_identity);

        counter_context!(cx, counter);

        assert!(service.poll_ready(&mut cx).is_ready());

        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn decrypt_inbound_success() {
        let result = Mutex::new(None);
        let inner = service_fn(|msg: DecryptedDhtMessage| {
            *result.lock().unwrap() = Some(msg);
            future::ready(Result::<(), PipelineError>::Ok(()))
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(inner, Arc::clone(&node_identity));

        let plain_text_msg = wrap_in_envelope_body!(b"Secret plans".to_vec());
        let inbound_msg = make_dht_inbound_message(
            &node_identity,
            plain_text_msg.to_encoded_bytes(),
            DhtMessageFlags::ENCRYPTED,
            true,
        );

        block_on(service.call(inbound_msg)).unwrap();
        let decrypted = result.lock().unwrap().take().unwrap();
        assert_eq!(decrypted.decryption_succeeded(), true);
        assert_eq!(decrypted.decryption_result.unwrap(), plain_text_msg);
    }

    #[test]
    fn decrypt_inbound_fail() {
        let result = Mutex::new(None);
        let inner = service_fn(|msg: DecryptedDhtMessage| {
            *result.lock().unwrap() = Some(msg);
            future::ready(Result::<(), PipelineError>::Ok(()))
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(inner, Arc::clone(&node_identity));

        let some_secret = "Super secret message".as_bytes().to_vec();
        let some_other_node_identity = make_node_identity();
        let inbound_msg =
            make_dht_inbound_message(&some_other_node_identity, some_secret, DhtMessageFlags::ENCRYPTED, true);

        block_on(service.call(inbound_msg.clone())).unwrap();
        let decrypted = result.lock().unwrap().take().unwrap();

        assert_eq!(decrypted.decryption_succeeded(), false);
        assert_eq!(decrypted.decryption_result.unwrap_err(), inbound_msg.body);
    }

    #[test]
    fn decrypt_inbound_fail_destination() {
        let result = Mutex::new(None);
        let inner = service_fn(|msg: DecryptedDhtMessage| {
            *result.lock().unwrap() = Some(msg);
            future::ready(Result::<(), PipelineError>::Ok(()))
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(inner, Arc::clone(&node_identity));

        let nonsense = "Cannot Decrypt this".as_bytes().to_vec();
        let mut inbound_msg =
            make_dht_inbound_message(&node_identity, nonsense.clone(), DhtMessageFlags::ENCRYPTED, true);
        inbound_msg.dht_header.destination = node_identity.public_key().clone().into();

        let err = block_on(service.call(inbound_msg)).unwrap_err();
        assert!(err.to_string().starts_with("Message rejected"),);
        assert!(result.lock().unwrap().is_none());
    }
}

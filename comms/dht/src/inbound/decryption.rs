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

use std::{convert::TryInto, sync::Arc, task::Poll, time::Duration};

use futures::{future::BoxFuture, task::Context};
use log::*;
use prost::Message;
use tari_comms::{
    connectivity::ConnectivityRequester,
    message::EnvelopeBody,
    peer_manager::NodeIdentity,
    pipeline::PipelineError,
    types::CommsDHKE,
    BytesMut,
};
use thiserror::Error;
use tower::{layer::Layer, Service, ServiceExt};

use crate::{
    crypt,
    envelope::DhtMessageHeader,
    error::DhtEncryptError,
    inbound::message::{DecryptedDhtMessage, DhtInboundMessage, ValidatedDhtInboundMessage},
    message_signature::{MessageSignature, MessageSignatureError, ProtoMessageSignature},
    DhtConfig,
};

const LOG_TARGET: &str = "comms::middleware::decryption";

#[derive(Error, Debug)]
enum DecryptionError {
    #[error("Failed to validate ENCRYPTED message signature")]
    MessageSignatureInvalidEncryptedSignature,
    #[error("Failed to validate CLEARTEXT message signature")]
    MessageSignatureInvalidClearTextSignature,
    #[error("Message signature not provided for encrypted message")]
    MessageSignatureNotProvidedForEncryptedMessage,
    #[error("Failed to decrypt message signature")]
    MessageSignatureDecryptedFailed,
    #[error("Failed to deserialize message signature")]
    MessageSignatureDeserializedFailed,
    #[error("Failed to decode clear-text message signature")]
    MessageSignatureClearTextDecodeFailed,
    #[error("Message signature error for cleartext message: {0}")]
    MessageSignatureErrorClearText(MessageSignatureError),
    #[error("Message signature error for encrypted message: {0}")]
    MessageSignatureErrorEncrypted(MessageSignatureError),
    #[error("Ephemeral public key not provided for encrypted message")]
    EphemeralKeyNotProvidedForEncryptedMessage,
    #[error("Message rejected because this node could not decrypt a message that was addressed to it")]
    MessageRejectDecryptionFailed,
    #[error("Failed to decode envelope body")]
    EnvelopeBodyDecodeFailed,
    #[error("Encrypted message without a destination is invalid")]
    EncryptedMessageNoDestination,
    #[error("Decryption failed: {0}")]
    DecryptionFailedMalformedCipher(#[from] DhtEncryptError),
}

/// This layer is responsible for attempting to decrypt inbound messages.
pub struct DecryptionLayer {
    node_identity: Arc<NodeIdentity>,
    connectivity: ConnectivityRequester,
    config: Arc<DhtConfig>,
}

impl DecryptionLayer {
    pub fn new(config: Arc<DhtConfig>, node_identity: Arc<NodeIdentity>, connectivity: ConnectivityRequester) -> Self {
        Self {
            node_identity,
            connectivity,
            config,
        }
    }
}

impl<S> Layer<S> for DecryptionLayer {
    type Service = DecryptionService<S>;

    fn layer(&self, service: S) -> Self::Service {
        DecryptionService::new(
            self.config.clone(),
            self.node_identity.clone(),
            self.connectivity.clone(),
            service,
        )
    }
}

/// Responsible for decrypting InboundMessages and passing a DecryptedInboundMessage to the given service
#[derive(Clone)]
pub struct DecryptionService<S> {
    config: Arc<DhtConfig>,
    node_identity: Arc<NodeIdentity>,
    connectivity: ConnectivityRequester,
    inner: S,
}

impl<S> DecryptionService<S> {
    pub fn new(
        config: Arc<DhtConfig>,
        node_identity: Arc<NodeIdentity>,
        connectivity: ConnectivityRequester,
        service: S,
    ) -> Self {
        Self {
            node_identity,
            connectivity,
            config,
            inner: service,
        }
    }
}

impl<S> Service<DhtInboundMessage> for DecryptionService<S>
where
    S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + Send + 'static,
    S::Future: Send,
{
    type Error = PipelineError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtInboundMessage) -> Self::Future {
        Box::pin(Self::handle_message(
            self.inner.clone(),
            Arc::clone(&self.node_identity),
            self.connectivity.clone(),
            self.config.ban_duration,
            msg,
        ))
    }
}

impl<S> DecryptionService<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    async fn handle_message(
        next_service: S,
        node_identity: Arc<NodeIdentity>,
        mut connectivity: ConnectivityRequester,
        ban_duration: Duration,
        message: DhtInboundMessage,
    ) -> Result<(), PipelineError> {
        use DecryptionError::*;
        let source = message.source_peer.clone();
        let trace_id = message.dht_header.message_tag;
        let tag = message.tag;
        match Self::validate_and_decrypt_message(node_identity, message).await {
            Ok(msg) => {
                trace!(target: LOG_TARGET, "Passing onto next service (Trace: {})", msg.tag);
                next_service.oneshot(msg).await
            },
            // The peer received an invalid message signature however we cannot ban the source peer because they have no
            // way to validate this
            Err(err @ MessageSignatureInvalidEncryptedSignature) | Err(err @ MessageSignatureErrorEncrypted(_)) => {
                warn!(
                    target: LOG_TARGET,
                    "SECURITY: {} ({}, peer={}, trace={}). Message discarded", err, tag, source.node_id, trace_id
                );
                Err(err.into())
            },

            // These are verifiable error cases that can be checked by every node
            Err(err @ MessageSignatureNotProvidedForEncryptedMessage) |
            Err(err @ EphemeralKeyNotProvidedForEncryptedMessage) |
            Err(err @ MessageSignatureClearTextDecodeFailed) |
            Err(err @ MessageSignatureInvalidClearTextSignature) |
            Err(err @ EncryptedMessageNoDestination) |
            Err(err @ MessageSignatureErrorClearText(_)) => {
                warn!(
                    target: LOG_TARGET,
                    "SECURITY: {} ({}, peer={}, trace={}). Message discarded", err, tag, source.node_id, trace_id
                );
                // This message should not have been propagated, or has been manipulated in some way. Ban the source of
                // this message.
                connectivity
                    .ban_peer_until(source.node_id.clone(), ban_duration, err.to_string())
                    .await?;
                Err(err.into())
            },
            Err(EnvelopeBodyDecodeFailed) => {
                debug!(
                    target: LOG_TARGET,
                    "Failed to decode message body ({}, peer={}, trace={}). Message discarded",
                    tag,
                    source.node_id,
                    trace_id
                );
                Ok(())
            },
            Err(err) => Err(err.into()),
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn validate_and_decrypt_message(
        node_identity: Arc<NodeIdentity>,
        message: DhtInboundMessage,
    ) -> Result<DecryptedDhtMessage, DecryptionError> {
        let validated_msg = Self::initial_validation(message)?;

        if !validated_msg.header().flags.is_encrypted() {
            return Self::success_not_encrypted(validated_msg).await;
        }

        trace!(
            target: LOG_TARGET,
            "Decrypting message {} (Trace: {})",
            validated_msg.message().tag,
            validated_msg.message().dht_header.message_tag
        );

        let dht_header = validated_msg.header();

        let e_pk = dht_header
            .ephemeral_public_key
            .as_ref()
            // No ephemeral key with ENCRYPTED flag set
            .ok_or( DecryptionError::EphemeralKeyNotProvidedForEncryptedMessage)?;

        if !validated_msg.message().dht_header.destination.is_unknown() &&
            validated_msg
                .message()
                .dht_header
                .destination
                .public_key()
                .map(|pk| pk != node_identity.public_key())
                .unwrap_or(false)
        {
            debug!(
                target: LOG_TARGET,
                "Encrypted message (source={}, {}) not destined for this peer. Passing to next service (Trace: {})",
                validated_msg.message().source_peer.node_id,
                validated_msg.message().dht_header.message_tag,
                validated_msg.message().tag
            );
            return Ok(DecryptedDhtMessage::failed(validated_msg.into_message()));
        }

        let shared_secret = CommsDHKE::new(node_identity.secret_key(), e_pk);
        let message = validated_msg.message();

        // Decrypt and verify the origin
        let authenticated_origin = match Self::attempt_decrypt_message_signature(&shared_secret, dht_header) {
            Ok(message_signature) => {
                // If this fails, discard the message because we decrypted and deserialized the message with our shared
                // ECDH secret but the message could not be authenticated
                let binding_message_representation =
                    crypt::create_message_domain_separated_hash(&message.dht_header, &message.body);

                if !message_signature.verify(&binding_message_representation) {
                    return Err(DecryptionError::MessageSignatureInvalidEncryptedSignature);
                }
                message_signature.into_signer_public_key()
            },
            Err(err) => {
                trace!(
                    target: LOG_TARGET,
                    "Unable to decrypt message origin: {}, {} (Trace: {})",
                    err,
                    message.tag,
                    message.dht_header.message_tag
                );
                if message.dht_header.destination.equals_node_identity(&node_identity) {
                    warn!(
                        target: LOG_TARGET,
                        "Received message from peer '{}' that is destined for this node that could not be decrypted. \
                         Discarding message {} (Trace: {})",
                        message.source_peer.node_id,
                        message.tag,
                        message.dht_header.message_tag
                    );
                    return Err(DecryptionError::MessageSignatureDecryptedFailed);
                }
                return Ok(DecryptedDhtMessage::failed(validated_msg.into_message()));
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
                Ok(DecryptedDhtMessage::succeeded(
                    message_body,
                    Some(authenticated_origin),
                    validated_msg.into_message(),
                ))
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Unable to decrypt message: {}, {} (Trace: {})", err, message.tag, message.dht_header.message_tag
                );

                if message.dht_header.destination.equals_node_identity(&node_identity) {
                    warn!(
                        target: LOG_TARGET,
                        "Received message from peer '{}' that is destined for this node that could not be decrypted. \
                         Discarding message {} (Trace: {})",
                        message.source_peer.node_id,
                        message.tag,
                        message.dht_header.message_tag
                    );
                    return Err(DecryptionError::MessageRejectDecryptionFailed);
                }

                Ok(DecryptedDhtMessage::failed(validated_msg.into_message()))
            },
        }
    }

    /// Performs message validation that should be performed by all nodes. If an error is encountered, the message is
    /// invalid and should never have been sent.
    fn initial_validation(message: DhtInboundMessage) -> Result<ValidatedDhtInboundMessage, DecryptionError> {
        if message.dht_header.flags.is_encrypted() {
            // Check if there is no destination specified and discard
            if message.dht_header.destination.is_unknown() {
                return Err(DecryptionError::EncryptedMessageNoDestination);
            }

            // No e_pk is invalid for encrypted messages
            if message.dht_header.ephemeral_public_key.is_none() {
                return Err(DecryptionError::EphemeralKeyNotProvidedForEncryptedMessage);
            }

            Ok(ValidatedDhtInboundMessage::new(message, None))
        } else if message.dht_header.message_signature.is_empty() {
            Ok(ValidatedDhtInboundMessage::new(message, None))
        } else {
            let message_signature: MessageSignature =
                ProtoMessageSignature::decode(message.dht_header.message_signature.as_slice())
                    .map_err(|_| DecryptionError::MessageSignatureClearTextDecodeFailed)?
                    .try_into()
                    .map_err(DecryptionError::MessageSignatureErrorClearText)?;

            let binding_message_representation =
                crypt::create_message_domain_separated_hash(&message.dht_header, &message.body);

            if !message_signature.verify(&binding_message_representation) {
                return Err(DecryptionError::MessageSignatureInvalidClearTextSignature);
            }
            Ok(ValidatedDhtInboundMessage::new(
                message,
                Some(message_signature.into_signer_public_key()),
            ))
        }
    }

    fn attempt_decrypt_message_signature(
        shared_secret: &CommsDHKE,
        dht_header: &DhtMessageHeader,
    ) -> Result<MessageSignature, DecryptionError> {
        let encrypted_message_signature = Some(&dht_header.message_signature)
            .filter(|b| !b.is_empty())
            // This should not have been sent/propagated
            .ok_or( DecryptionError::MessageSignatureNotProvidedForEncryptedMessage)?;

        // obtain key signature for authenticated decrypt signature
        let key_signature = crypt::generate_key_signature_for_authenticated_encryption(shared_secret);
        let decrypted_bytes = crypt::decrypt_with_chacha20_poly1305(&key_signature, encrypted_message_signature)
            .map_err(|_| DecryptionError::MessageSignatureDecryptedFailed)?;
        let message_signature = ProtoMessageSignature::decode(decrypted_bytes.as_slice())
            .map_err(|_| DecryptionError::MessageSignatureDeserializedFailed)?;

        let message_signature = message_signature
            .try_into()
            .map_err(DecryptionError::MessageSignatureErrorEncrypted)?;
        Ok(message_signature)
    }

    fn attempt_decrypt_message_body(
        shared_secret: &CommsDHKE,
        message_body: &[u8],
    ) -> Result<EnvelopeBody, DecryptionError> {
        let key_message = crypt::generate_key_message(shared_secret);
        let mut decrypted = BytesMut::from(message_body);
        crypt::decrypt(&key_message, &mut decrypted).map_err(DecryptionError::DecryptionFailedMalformedCipher)?;
        // Deserialization into an EnvelopeBody is done here to determine if the
        // decryption produced valid bytes or not.
        EnvelopeBody::decode(decrypted.freeze())
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
            .map_err(|_| DecryptionError::EnvelopeBodyDecodeFailed)
    }

    async fn success_not_encrypted(
        validated: ValidatedDhtInboundMessage,
    ) -> Result<DecryptedDhtMessage, DecryptionError> {
        let authenticated_pk = validated.authenticated_origin().cloned();
        let msg = validated.message();
        match EnvelopeBody::decode(msg.body.as_slice()) {
            Ok(deserialized) => {
                trace!(
                    target: LOG_TARGET,
                    "Message {} is not encrypted. Passing onto next service (Trace: {})",
                    msg.tag,
                    msg.dht_header.message_tag
                );
                Ok(DecryptedDhtMessage::succeeded(
                    deserialized,
                    authenticated_pk,
                    validated.into_message(),
                ))
            },
            Err(err) => {
                // Message was not encrypted but failed to deserialize - immediately discard
                // TODO: Bad node behaviour?
                debug!(
                    target: LOG_TARGET,
                    "Unable to deserialize message {}: {}. Message will be discarded. (Trace: {})",
                    msg.tag,
                    err,
                    msg.dht_header.message_tag
                );
                Err(DecryptionError::EnvelopeBodyDecodeFailed)
            },
        }
    }
}

#[cfg(test)]
mod test {
    use std::sync::Mutex;

    use futures::{executor::block_on, future};
    use tari_comms::{
        message::MessageTag,
        runtime,
        test_utils::mocks::create_connectivity_mock,
        wrap_in_envelope_body,
        BytesMut,
    };
    use tari_test_utils::{counter_context, unpack_enum};
    use tokio::time::sleep;
    use tower::service_fn;

    use super::*;
    use crate::{
        envelope::{DhtEnvelope, DhtMessageFlags},
        test_utils::{
            make_dht_header,
            make_dht_inbound_message,
            make_dht_inbound_message_raw,
            make_keypair,
            make_node_identity,
            make_valid_message_signature,
        },
    };

    #[test]
    fn poll_ready() {
        let service = service_fn(|_: DecryptedDhtMessage| future::ready(Result::<(), PipelineError>::Ok(())));
        let node_identity = make_node_identity();
        let (connectivity, _) = create_connectivity_mock();
        let mut service = DecryptionService::new(Default::default(), node_identity, connectivity, service);

        counter_context!(cx, counter);

        assert!(service.poll_ready(&mut cx).is_ready());

        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn decrypt_inbound_success() {
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        let node_identity = make_node_identity();
        let (connectivity, _) = create_connectivity_mock();
        let mut service = DecryptionService::new(Default::default(), node_identity.clone(), connectivity, service);

        let plain_text_msg = wrap_in_envelope_body!(b"Secret plans".to_vec());
        let inbound_msg =
            make_dht_inbound_message(&node_identity, &plain_text_msg, DhtMessageFlags::ENCRYPTED, true, true).unwrap();

        block_on(service.call(inbound_msg)).unwrap();
        let decrypted = result.lock().unwrap().take().unwrap();
        assert!(decrypted.decryption_succeeded());
        assert_eq!(decrypted.decryption_result.unwrap(), plain_text_msg);
    }

    #[test]
    fn decrypt_inbound_fail() {
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        let node_identity = make_node_identity();
        let (connectivity, _) = create_connectivity_mock();
        let mut service = DecryptionService::new(Default::default(), node_identity, connectivity, service);

        let some_secret = b"Super secret message".to_vec();
        let some_other_node_identity = make_node_identity();
        let inbound_msg = make_dht_inbound_message(
            &some_other_node_identity,
            &some_secret,
            DhtMessageFlags::ENCRYPTED,
            true,
            true,
        )
        .unwrap();

        block_on(service.call(inbound_msg.clone())).unwrap();
        let decrypted = result.lock().unwrap().take().unwrap();

        assert!(!decrypted.decryption_succeeded());
        assert_eq!(decrypted.decryption_result.unwrap_err(), inbound_msg.body);
    }

    #[runtime::test]
    async fn decrypt_inbound_fail_destination() {
        let (connectivity, mock) = create_connectivity_mock();
        mock.spawn();
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(Default::default(), node_identity.clone(), connectivity, service);

        let nonsense = b"Cannot Decrypt this".to_vec();
        let inbound_msg =
            make_dht_inbound_message_raw(&node_identity, nonsense, DhtMessageFlags::ENCRYPTED, true, true).unwrap();

        let err = service.call(inbound_msg).await.unwrap_err();
        let err = err.downcast::<DecryptionError>().unwrap();
        unpack_enum!(DecryptionError::MessageRejectDecryptionFailed = err);
        assert!(result.lock().unwrap().is_none());
    }

    #[runtime::test]
    async fn decrypt_inbound_fail_no_destination() {
        let (connectivity, mock) = create_connectivity_mock();
        mock.spawn();
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(Default::default(), node_identity.clone(), connectivity, service);

        let plain_text_msg = b"Secret message to nowhere".to_vec();
        let inbound_msg =
            make_dht_inbound_message(&node_identity, &plain_text_msg, DhtMessageFlags::ENCRYPTED, true, false).unwrap();

        let err = service.call(inbound_msg).await.unwrap_err();
        let err = err.downcast::<DecryptionError>().unwrap();
        unpack_enum!(DecryptionError::EncryptedMessageNoDestination = err);
        assert!(result.lock().unwrap().is_none());
    }

    #[runtime::test]
    async fn decrypt_inbound_fail_invalid_signature_encrypted() {
        let (connectivity, mock) = create_connectivity_mock();
        let mock_state = mock.spawn();
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(Default::default(), node_identity.clone(), connectivity, service);

        let plain_text_msg = BytesMut::from(b"Secret message".as_slice());
        let (e_secret_key, e_public_key) = make_keypair();
        let shared_secret = CommsDHKE::new(&e_secret_key, node_identity.public_key());
        let key_message = crypt::generate_key_message(&shared_secret);
        let msg_tag = MessageTag::new();

        let mut message = plain_text_msg.clone();
        crypt::encrypt(&key_message, &mut message).unwrap();
        let message = message.freeze();
        let header = make_dht_header(
            &node_identity,
            &e_public_key,
            &e_secret_key,
            &message,
            DhtMessageFlags::ENCRYPTED,
            true,
            msg_tag,
            true,
        )
        .unwrap();
        let envelope = DhtEnvelope::new(header.into(), message.into());
        let msg_tag = MessageTag::new();
        let mut inbound_msg = DhtInboundMessage::new(
            msg_tag,
            envelope.header.unwrap().try_into().unwrap(),
            Arc::new(node_identity.to_peer()),
            envelope.body,
        );

        // Sign invalid data. Other peers cannot validate this while propagating, but this should not cause them to be
        // banned.
        let signature = make_valid_message_signature(&node_identity, b"sign invalid data");
        let key_signature = crypt::generate_key_signature_for_authenticated_encryption(&shared_secret);

        inbound_msg.dht_header.message_signature =
            crypt::encrypt_with_chacha20_poly1305(&key_signature, &signature).unwrap();

        let err = service.call(inbound_msg).await.unwrap_err();
        let err = err.downcast::<DecryptionError>().unwrap();
        unpack_enum!(DecryptionError::MessageSignatureInvalidEncryptedSignature = err);
        assert!(result.lock().unwrap().is_none());

        // Proving a negative i.e. ban is not called, we have no choice but to sleep to wait for any potential calls to
        // be registered. This should ensure that if this bug re-occurs that this test is flaky.
        sleep(Duration::from_secs(1)).await;
        assert_eq!(mock_state.count_calls_containing("BanPeer").await, 0);
    }

    #[runtime::test]
    async fn decrypt_inbound_fail_invalid_signature_cleartext() {
        let (connectivity, mock) = create_connectivity_mock();
        let mock_state = mock.spawn();
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(Default::default(), node_identity.clone(), connectivity, service);

        let plain_text_msg = BytesMut::from(b"Public message".as_slice());
        let (e_secret_key, e_public_key) = make_keypair();
        let shared_secret = CommsDHKE::new(&e_secret_key, node_identity.public_key());
        let key_message = crypt::generate_key_message(&shared_secret);
        let msg_tag = MessageTag::new();

        let mut message = plain_text_msg.clone();
        crypt::encrypt(&key_message, &mut message).unwrap();
        let message = message.freeze();
        let header = make_dht_header(
            &node_identity,
            &e_public_key,
            &e_secret_key,
            &message,
            DhtMessageFlags::NONE,
            true,
            msg_tag,
            true,
        )
        .unwrap();
        let envelope = DhtEnvelope::new(header.into(), message.into());
        let msg_tag = MessageTag::new();
        let mut inbound_msg = DhtInboundMessage::new(
            msg_tag,
            envelope.header.unwrap().try_into().unwrap(),
            Arc::new(node_identity.to_peer()),
            envelope.body,
        );

        inbound_msg.dht_header.ephemeral_public_key = Some(e_public_key.clone());
        inbound_msg.dht_header.message_signature = make_valid_message_signature(&node_identity, b"sign invalid data");

        let err = service.call(inbound_msg).await.unwrap_err();
        let err = err.downcast::<DecryptionError>().unwrap();
        unpack_enum!(DecryptionError::MessageSignatureInvalidClearTextSignature = err);
        assert!(result.lock().unwrap().is_none());

        mock_state.await_call_count(1).await;
        assert_eq!(mock_state.count_calls_containing("BanPeer").await, 1);
    }
}

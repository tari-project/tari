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
use tari_utilities::ByteArray;
use thiserror::Error;
use tower::{layer::Layer, Service, ServiceExt};

use crate::{
    crypt,
    inbound::message::{DecryptedDhtMessage, DhtInboundMessage, ValidatedDhtInboundMessage},
    message_signature::{MessageSignature, ProtoMessageSignature},
    DhtConfig,
};

const LOG_TARGET: &str = "comms::middleware::decryption";

#[derive(Error, Debug, PartialEq)]
enum DecryptionError {
    #[error("Failed to validate message signature")]
    InvalidSignature,
    #[error("Bad encrypted message semantics")]
    BadEncryptedMessageSemantics,
    #[error("Message rejected because this node could not decrypt a message that was addressed to it")]
    MessageRejectDecryptionFailed,
    #[error("Failed to decode envelope body")]
    EnvelopeBodyDecodeFailed,
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
            // These are verifiable error cases that can be checked by every node
            Err(err @ BadEncryptedMessageSemantics) | Err(err @ InvalidSignature) => {
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
        // Perform initial checks and check the message signature if needed
        let validated_msg = Self::initial_validation(message)?;

        // The message is unencrypted and valid
        if !validated_msg.header().flags.is_encrypted() {
            return Self::success_not_encrypted(validated_msg).await;
        }

        trace!(
            target: LOG_TARGET,
            "Decrypting message {} (Trace: {})",
            validated_msg.message().tag,
            validated_msg.message().dht_header.message_tag
        );

        // The message is encrypted, so see if it is for us
        // If not, pass it along
        if validated_msg
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

        // The message is encrypted and for us, so complete the ephemeral key exchange
        let header = validated_msg.header();
        let ephemeral_public_key = header
            .ephemeral_public_key
            .as_ref()
            .ok_or(DecryptionError::BadEncryptedMessageSemantics)?;
        let shared_ephemeral_secret = CommsDHKE::new(node_identity.secret_key(), ephemeral_public_key);
        let message = validated_msg.message();

        // Unmask the sender public key using an offset mask derived from the ECDH exchange
        let mask = crypt::generate_key_mask(&shared_ephemeral_secret)
            .map_err(|_| DecryptionError::MessageRejectDecryptionFailed)?;
        let mask_inverse = mask.invert().ok_or(DecryptionError::MessageRejectDecryptionFailed)?;
        let sender_masked_public_key = validated_msg
            .authenticated_origin()
            .ok_or(DecryptionError::MessageRejectDecryptionFailed)?;
        let sender_public_key = mask_inverse * sender_masked_public_key;

        trace!(
            target: LOG_TARGET,
            "Attempting to decrypt message body from origin public key '{}', {} (Trace: {})",
            sender_public_key,
            message.tag,
            message.dht_header.message_tag
        );

        // Decrypt and verify the message
        match Self::attempt_decrypt_message_body(
            &shared_ephemeral_secret,
            &message.body,
            sender_masked_public_key.as_bytes(),
        ) {
            Ok(message_body) => {
                debug!(
                    target: LOG_TARGET,
                    "Message successfully decrypted, {} (Trace: {})", message.tag, message.dht_header.message_tag
                );
                Ok(DecryptedDhtMessage::succeeded(
                    message_body,
                    Some(sender_public_key),
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
    /// invalid and should never have been propagated.
    ///
    /// These failure modes are detectable by any node, so it is generally safe to ban an offending peer.
    fn initial_validation(message: DhtInboundMessage) -> Result<ValidatedDhtInboundMessage, DecryptionError> {
        // If an unencrypted message has no signature, it passes this validation automatically
        if !message.dht_header.flags.is_encrypted() && message.dht_header.message_signature.is_empty() {
            return Ok(ValidatedDhtInboundMessage::new(message, None));
        }

        // If the message is encrypted:
        // - it must be nonempty
        // - it needs a destination
        // - it needs an ephemeral public key
        // - it needs a signature
        if message.dht_header.flags.is_encrypted() {
            // Must be nonempty
            if message.body.is_empty() {
                return Err(DecryptionError::BadEncryptedMessageSemantics);
            }

            // Must have a destination
            if message.dht_header.destination.is_unknown() {
                return Err(DecryptionError::BadEncryptedMessageSemantics);
            }

            // Must have an ephemeral public key
            if message.dht_header.ephemeral_public_key.is_none() {
                return Err(DecryptionError::BadEncryptedMessageSemantics);
            }

            // Must have a signature
            if message.dht_header.message_signature.is_empty() {
                return Err(DecryptionError::BadEncryptedMessageSemantics);
            }
        }

        // If a signature is present, it must be valid
        let message_signature: MessageSignature =
            ProtoMessageSignature::decode(message.dht_header.message_signature.as_slice())
                .map_err(|_| DecryptionError::InvalidSignature)?
                .try_into()
                .map_err(|_| DecryptionError::InvalidSignature)?;

        let binding_hash = crypt::create_message_domain_separated_hash(&message.dht_header, &message.body);

        if !message_signature.verify(&binding_hash) {
            return Err(DecryptionError::InvalidSignature);
        }

        // The message is valid at this point
        Ok(ValidatedDhtInboundMessage::new(
            message,
            Some(message_signature.into_signer_public_key()),
        ))
    }

    fn attempt_decrypt_message_body(
        shared_secret: &CommsDHKE,
        message_body: &[u8],
        authenticated_data: &[u8],
    ) -> Result<EnvelopeBody, DecryptionError> {
        let key_message = crypt::generate_key_message(shared_secret);
        let mut decrypted = BytesMut::from(message_body);
        crypt::decrypt_message(&key_message, &mut decrypted, authenticated_data)
            .map_err(|_| DecryptionError::MessageRejectDecryptionFailed)?;
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
    use tari_comms::{message::MessageExt, test_utils::mocks::create_connectivity_mock, wrap_in_envelope_body};
    use tari_test_utils::counter_context;
    use tokio::time::sleep;
    use tower::service_fn;

    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{make_dht_inbound_message, make_dht_inbound_message_raw, make_node_identity},
    };

    /// Receive a message, assert a specific error is raised, and test for peer ban status
    async fn expect_error(
        node_identity: Arc<NodeIdentity>,
        message: DhtInboundMessage,
        error: DecryptionError,
        ban: bool,
    ) {
        // Set up messaging
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
        let mut service = DecryptionService::new(Default::default(), node_identity, connectivity, service);

        // Receive the message and check for the expected error
        let err = service.call(message).await.unwrap_err();
        let err = err.downcast::<DecryptionError>().unwrap();
        assert_eq!(error, err);
        assert!(result.lock().unwrap().is_none());

        // Assert the expected ban status
        if ban {
            mock_state.await_call_count(1).await;
            assert_eq!(mock_state.count_calls_containing("BanPeer").await, 1);
        } else {
            // Waiting like this isn't a guarantee that the peer won't be banned
            sleep(Duration::from_secs(1)).await;
            assert_eq!(mock_state.count_calls_containing("BanPeer").await, 0);
        }
    }

    /// Receive a message successfully, decrypt if possible, check for peer banning, and return the message
    async fn expect_no_error(
        node_identity: Arc<NodeIdentity>,
        message: DhtInboundMessage,
        decryption_succeeded: bool,
    ) -> DecryptedDhtMessage {
        // Set up messaging
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
        let mut service = DecryptionService::new(Default::default(), node_identity, connectivity, service);

        // Receive the message and assert there were no errors
        block_on(service.call(message)).unwrap();
        assert!(result.lock().unwrap().is_some());
        let decrypted = result.lock().unwrap().take().unwrap();

        // See if decryption succeeded or failed as expected
        // We check both functions just in case!
        assert_eq!(decrypted.decryption_succeeded(), decryption_succeeded);
        assert_eq!(decrypted.decryption_failed(), !decryption_succeeded);

        // Don't ban the peer
        // Waiting like this isn't a guarantee that the peer won't be banned later
        sleep(Duration::from_secs(1)).await;
        assert_eq!(mock_state.count_calls_containing("BanPeer").await, 0);

        // Return the decrypted message for further handling; decryption may have failed
        decrypted
    }

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

    #[tokio::test]
    /// We can decrypt valid encrypted messages destined for us
    async fn decrypt_inbound_success() {
        let node_identity = make_node_identity();

        // Encrypt a message for us
        let plain_text = wrap_in_envelope_body!(b"Secret plans".to_vec());
        let message =
            make_dht_inbound_message(&node_identity, &plain_text, DhtMessageFlags::ENCRYPTED, true, true).unwrap();

        // Check that decryption succeeds and yields the original message
        let decrypted = expect_no_error(node_identity, message, true).await;
        assert_eq!(decrypted.decryption_result.unwrap(), plain_text);
    }

    #[tokio::test]
    /// An encrypted message is not destined for us
    async fn decrypt_inbound_not_for_us() {
        let node_identity = make_node_identity();
        let some_other_node_identity = make_node_identity();

        // Encrypt a message for someone else
        let plain_text = wrap_in_envelope_body!(b"Secret plans".to_vec());
        let message = make_dht_inbound_message(
            &some_other_node_identity,
            &plain_text,
            DhtMessageFlags::ENCRYPTED,
            true,
            true,
        )
        .unwrap();

        // Check that the message is received, but that decryption fails
        let decrypted = expect_no_error(node_identity, message.clone(), false).await;

        // The error should contain the message body
        assert_eq!(decrypted.decryption_result.unwrap_err(), message.body);
    }

    #[tokio::test]
    /// An encrypted message is empty
    async fn empty_message() {
        let node_identity = make_node_identity();
        let other_identity = make_node_identity();

        // Encrypt an empty message
        for identity in [&node_identity, &other_identity] {
            let mut message =
                make_dht_inbound_message(identity, &Vec::new(), DhtMessageFlags::ENCRYPTED, true, true).unwrap();
            message.body = Vec::new(); // due to padding, we need to manually reset this

            // Ban the peer
            expect_error(
                node_identity.clone(),
                message,
                DecryptionError::BadEncryptedMessageSemantics,
                true,
            )
            .await;
        }
    }

    #[tokio::test]
    /// An encrypted message is destined for us but can't be decrypted
    async fn decrypt_inbound_fail_for_us() {
        let node_identity = make_node_identity();

        // Encrypt an invalid message destined for us
        let nonsense = b"Cannot Decrypt this".to_vec();
        let message =
            make_dht_inbound_message_raw(&node_identity, nonsense, DhtMessageFlags::ENCRYPTED, true, true).unwrap();

        // Don't ban the peer
        expect_error(
            node_identity,
            message,
            DecryptionError::MessageRejectDecryptionFailed,
            false,
        )
        .await;
    }

    #[tokio::test]
    /// An encrypted message has no destination
    async fn decrypt_inbound_fail_no_destination() {
        let node_identity = make_node_identity();

        // Encrypt a message with no destination
        let plain_text_msg = b"Secret message to nowhere".to_vec();
        let message =
            make_dht_inbound_message(&node_identity, &plain_text_msg, DhtMessageFlags::ENCRYPTED, true, false).unwrap();

        // Ban the peer
        expect_error(
            node_identity,
            message,
            DecryptionError::BadEncryptedMessageSemantics,
            true,
        )
        .await;
    }

    #[tokio::test]
    /// A message has an invalid signature
    async fn decrypt_inbound_fail_invalid_signature() {
        let node_identity = make_node_identity();
        let other_identity = make_node_identity();
        let plain_text_msg = b"a message".to_vec();

        // Handle the cases where we are and aren't the recipient
        for identity in [&node_identity, &other_identity] {
            // Handle the cases where the message is and isn't encrypted
            for flag in [DhtMessageFlags::NONE, DhtMessageFlags::ENCRYPTED] {
                let mut message = make_dht_inbound_message(identity, &plain_text_msg, flag, true, true).unwrap();

                // Manipulate the signature so it's invalid
                let malleated_index = message.dht_header.message_signature.len() - 1;
                message.dht_header.message_signature[malleated_index] =
                    !message.dht_header.message_signature[malleated_index];

                // Ban the peer
                expect_error(node_identity.clone(), message, DecryptionError::InvalidSignature, true).await;
            }
        }
    }

    #[tokio::test]
    /// An encrypted message has no signature
    async fn decrypt_inbound_fail_missing_signature_encrypted() {
        let node_identity = make_node_identity();
        let other_identity = make_node_identity();
        let plain_text_msg = b"a secret message".to_vec();

        // Handle the cases where we are and aren't the recipient
        for identity in [&node_identity, &other_identity] {
            let mut message =
                make_dht_inbound_message(identity, &plain_text_msg, DhtMessageFlags::ENCRYPTED, true, true).unwrap();

            // Remove the signature
            message.dht_header.message_signature = Vec::new();

            // Ban the peer
            expect_error(
                node_identity.clone(),
                message,
                DecryptionError::BadEncryptedMessageSemantics,
                true,
            )
            .await;
        }
    }

    #[tokio::test]
    /// An encrypted message has no ephemeral key
    async fn decrypt_inbound_fail_missing_ephemeral_encrypted() {
        let node_identity = make_node_identity();
        let other_identity = make_node_identity();
        let plain_text_msg = b"a secret message".to_vec();

        // Handle the cases where we are and aren't the recipient
        for identity in [&node_identity, &other_identity] {
            let mut message =
                make_dht_inbound_message(identity, &plain_text_msg, DhtMessageFlags::ENCRYPTED, true, true).unwrap();

            // Remove the ephemeral key
            message.dht_header.ephemeral_public_key = None;

            // Ban the peer
            expect_error(
                node_identity.clone(),
                message,
                DecryptionError::BadEncryptedMessageSemantics,
                true,
            )
            .await;
        }
    }

    #[tokio::test]
    /// A message has a signature that can't be decoded (wire format)
    async fn decrypt_inbound_fail_signature_decode_wire() {
        let node_identity = make_node_identity();
        let other_identity = make_node_identity();
        let plain_text_msg = b"a message".to_vec();

        // Handle the cases where we are and aren't the recipient
        for identity in [&node_identity, &other_identity] {
            // Handle the cases where the message is and isn't encrypted
            for flag in [DhtMessageFlags::NONE, DhtMessageFlags::ENCRYPTED] {
                let mut message = make_dht_inbound_message(identity, &plain_text_msg, flag, true, true).unwrap();

                // Render the signature not decodable
                message.dht_header.message_signature = vec![1u8; 32];

                // Ban the peer
                expect_error(node_identity.clone(), message, DecryptionError::InvalidSignature, true).await;
            }
        }
    }

    #[tokio::test]
    /// A message has a signature that can't be decoded (signature structure)
    async fn decrypt_inbound_fail_signature_decode_structure() {
        let node_identity = make_node_identity();
        let other_identity = make_node_identity();
        let plain_text_msg = b"a message".to_vec();

        // Handle the cases where we are and aren't the recipient
        for identity in [&node_identity, &other_identity] {
            // Handle the cases where the message is and isn't encrypted
            for flag in [DhtMessageFlags::NONE, DhtMessageFlags::ENCRYPTED] {
                let mut message = make_dht_inbound_message(identity, &plain_text_msg, flag, true, true).unwrap();

                // Render a signature field not decodable
                let mut signature =
                    MessageSignature::new_signed(node_identity.secret_key().clone(), &plain_text_msg).to_proto();
                signature.signer_public_key = vec![1u8; 8]; // invalid public key encoding
                message.dht_header.message_signature = signature.to_encoded_bytes();

                // Ban the peer
                expect_error(node_identity.clone(), message, DecryptionError::InvalidSignature, true).await;
            }
        }
    }
}

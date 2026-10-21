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

use std::{convert::TryInto, sync::Arc, task::Poll};

use futures::{future::BoxFuture, task::Context};
use log::*;
use prost::Message;
use tari_comms::{
    BytesMut,
    decode_budget::decode_with_max_items,
    message::EnvelopeBody,
    peer_manager::NodeIdentity,
    pipeline::PipelineError,
    types::CommsDHKE,
};
use tari_crypto::compressed_key::CompressedKey;
use tari_utilities::ByteArray;
use thiserror::Error;
use tower::{Service, ServiceExt, layer::Layer};

use crate::{
    crypt,
    inbound::message::{DecryptedDhtMessage, DhtInboundMessage, ValidatedDhtInboundMessage},
    message_signature::{MessageSignature, ProtoMessageSignature},
    proto::{DHT_MESSAGE_MAX_DECODE_ITEMS, is_canonical},
};

const LOG_TARGET: &str = "comms::middleware::decryption";

/// The most parts an envelope body may carry. DHT handlers only ever read the first part, and Tari sends at most two,
/// so this leaves plenty of room while stopping a peer from packing an 8 MiB frame with millions of empty parts.
const MAX_ENVELOPE_PARTS: usize = 16;

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
    #[error("Bad clear-text message semantics")]
    BadClearTextMessageSemantics,
}

impl DecryptionError {
    /// The `reason` label of the rejected-message metric
    #[cfg(feature = "metrics")]
    fn metric_reason(&self) -> &'static str {
        match self {
            DecryptionError::InvalidSignature => "invalid_signature",
            DecryptionError::BadEncryptedMessageSemantics => "bad_encrypted_semantics",
            DecryptionError::MessageRejectDecryptionFailed => "decryption_failed",
            DecryptionError::EnvelopeBodyDecodeFailed => "body_decode_failed",
            DecryptionError::BadClearTextMessageSemantics => "bad_clear_text_semantics",
        }
    }
}

#[cfg(feature = "metrics")]
fn inbound_messages_rejected() -> tari_metrics::IntCounterVec {
    static METER: std::sync::LazyLock<tari_metrics::IntCounterVec> = std::sync::LazyLock::new(|| {
        tari_metrics::register_int_counter_vec(
            "comms::dht::inbound_messages_rejected",
            "Inbound messages dropped by the decryption layer, by reason",
            &["reason"],
        )
        .unwrap()
    });

    METER.clone()
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
        DecryptionService::new(self.node_identity.clone(), service)
    }
}

/// Responsible for decrypting InboundMessages and passing a DecryptedInboundMessage to the given service
#[derive(Clone)]
pub struct DecryptionService<S> {
    node_identity: Arc<NodeIdentity>,
    inner: S,
}

impl<S> DecryptionService<S> {
    pub fn new(node_identity: Arc<NodeIdentity>, service: S) -> Self {
        Self {
            node_identity,
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
            msg,
        ))
    }
}

/// **Attribution rule.** (1) Never ban the direct source for a validation failure on a message whose signer is not
/// that source: a relay cannot vouch for bytes it did not produce. (2) Never ban a signer because of a message
/// delivered by someone else. Ban only the party that is provably culpable from the message itself, and only when
/// honest software could never have produced that message.
///
/// The decryption layer cannot tell whether the direct source produced a message or relayed it, so it never bans:
/// every rejection here is a drop. Flood protection is the only rate control for invalid messages. The DHT handler
/// (`dht_handler/task.rs`) and the discovery service apply the same rule to bans through `DhtRequester::ban_peer`.
impl<S> DecryptionService<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    async fn handle_message(
        next_service: S,
        node_identity: Arc<NodeIdentity>,
        message: DhtInboundMessage,
    ) -> Result<(), PipelineError> {
        let source = message.source_peer.clone();
        let trace_id = message.dht_header.message_tag;
        let tag = message.tag;
        match Self::validate_and_decrypt_message(node_identity, message).await {
            Ok(msg) => {
                trace!(target: LOG_TARGET, "Passing onto next service (Trace: {})", msg.tag);
                next_service.oneshot(msg).await
            },
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "SECURITY: {} ({}, peer={}, trace={}). Message discarded", err, tag, source.node_id, trace_id
                );
                #[cfg(feature = "metrics")]
                inbound_messages_rejected()
                    .with_label_values(&[err.metric_reason()])
                    .inc();
                Ok(())
            },
        }
    }

    #[allow(clippy::too_many_lines)]
    // Ristretto point/scalar arithmetic, not integer arithmetic: these operators cannot overflow.
    #[allow(clippy::arithmetic_side_effects)]
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
            return Ok(validated_msg.into_failed());
        }

        // The message is encrypted and for us, so complete the ephemeral key exchange
        let header = validated_msg.header();
        let ephemeral_public_key = header
            .ephemeral_public_key
            .as_ref()
            .ok_or(DecryptionError::BadEncryptedMessageSemantics)?
            .to_public_key()
            .map_err(|_| DecryptionError::BadEncryptedMessageSemantics)?;
        let shared_ephemeral_secret = CommsDHKE::new(node_identity.secret_key(), &ephemeral_public_key);
        let message = validated_msg.message();

        // Unmask the sender public key using an offset mask derived from the ECDH exchange
        let mask = crypt::generate_key_mask(&shared_ephemeral_secret)
            .map_err(|_| DecryptionError::MessageRejectDecryptionFailed)?;
        let mask_inverse = mask.invert().ok_or(DecryptionError::MessageRejectDecryptionFailed)?;
        let sender_masked_public_key = validated_msg
            .authenticated_origin()
            .ok_or(DecryptionError::MessageRejectDecryptionFailed)?
            .to_public_key()
            .map_err(|_| DecryptionError::InvalidSignature)?;
        let sender_public_key = mask_inverse * &sender_masked_public_key;

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
                Ok(validated_msg.into_succeeded(message_body, Some(CompressedKey::new_from_pk(sender_public_key))))
            },
            // The body passed the AEAD check, so the message is provably for us, but it did not decode (or was over
            // the decode budget). Drop it: treating it as "not for us" would forward it on.
            Err(DecryptionError::EnvelopeBodyDecodeFailed) => Err(DecryptionError::EnvelopeBodyDecodeFailed),
            Err(err) => {
                debug!(
                    target: LOG_TARGET,
                    "Unable to decrypt message: {}, {} (Trace: {})", err, message.tag, message.dht_header.message_tag
                );

                if message.dht_header.destination.equals_node_identity(&node_identity) {
                    debug!(
                        target: LOG_TARGET,
                        "Received message from peer '{}' that is destined for this node that could not be decrypted. \
                         Discarding message {} (Trace: {})",
                        message.source_peer.node_id,
                        message.tag,
                        message.dht_header.message_tag
                    );
                    return Err(DecryptionError::MessageRejectDecryptionFailed);
                }

                Ok(validated_msg.into_failed())
            },
        }
    }

    /// Performs message validation that should be performed by all nodes. If an error is encountered, the message is
    /// invalid and should never have been propagated.
    ///
    /// These failure modes are detectable by any node, but not attributable to the direct source (see the attribution
    /// rule above), so the message is only dropped.
    fn initial_validation(message: DhtInboundMessage) -> Result<ValidatedDhtInboundMessage, DecryptionError> {
        if !message.is_semantically_valid() {
            if message.dht_header.flags.is_encrypted() {
                return Err(DecryptionError::BadEncryptedMessageSemantics);
            } else {
                return Err(DecryptionError::BadClearTextMessageSemantics);
            }
        }

        // If a signature is not present, the message is valid at this point
        if message.dht_header.message_signature.is_empty() {
            return Ok(ValidatedDhtInboundMessage::new(message, None));
        }

        // If a signature is present, it must be valid and canonically encoded: other encodings of the same signature
        // would let anyone change the bytes relays carry
        let proto_signature = ProtoMessageSignature::decode(message.dht_header.message_signature.as_slice())
            .map_err(|_| DecryptionError::InvalidSignature)?;
        if !is_canonical(&message.dht_header.message_signature, &proto_signature) {
            return Err(DecryptionError::InvalidSignature);
        }
        let message_signature: MessageSignature = proto_signature
            .try_into()
            .map_err(|_| DecryptionError::InvalidSignature)?;

        let binding_hash = crypt::create_message_domain_separated_hash(&message.dht_header, &message.body);

        message_signature
            .verify(&binding_hash)
            .map_err(|_| DecryptionError::InvalidSignature)?;

        // The message is valid at this point
        Ok(ValidatedDhtInboundMessage::new(message, Some(message_signature)))
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
        decode_envelope_body(&decrypted)
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
        match decode_envelope_body(&msg.body) {
            Ok(deserialized) => {
                trace!(
                    target: LOG_TARGET,
                    "Message {} is not encrypted. Passing onto next service (Trace: {})",
                    msg.tag,
                    msg.dht_header.message_tag
                );
                Ok(validated.into_succeeded(deserialized, authenticated_pk))
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

/// Decodes an envelope body, rejecting one with more than [MAX_ENVELOPE_PARTS] parts like any other undecodable body.
pub(crate) fn decode_envelope_body(bytes: &[u8]) -> Result<EnvelopeBody, prost::DecodeError> {
    let body = decode_with_max_items::<EnvelopeBody>(bytes, DHT_MESSAGE_MAX_DECODE_ITEMS)?;
    if body.parts.len() > MAX_ENVELOPE_PARTS {
        return Err(prost::DecodeError::new(format!(
            "EnvelopeBody has {} parts, at most {} allowed",
            body.parts.len(),
            MAX_ENVELOPE_PARTS
        )));
    }
    Ok(body)
}

#[cfg(test)]
mod test {

    #![allow(clippy::indexing_slicing)]
    use std::sync::Mutex;

    use futures::{executor::block_on, future};
    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tari_test_utils::counter_context;
    use tower::{service_fn, util::ServiceFn};

    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{make_dht_inbound_message, make_dht_inbound_message_raw, make_node_identity},
    };

    type SinkFn = ServiceFn<fn(DecryptedDhtMessage) -> future::Ready<Result<(), PipelineError>>>;

    /// Assert that validation rejects the message with `error`, and that the service drops it: it is not passed on,
    /// and the pipeline sees no error. The service has no way to ban the source.
    async fn expect_error(node_identity: Arc<NodeIdentity>, message: DhtInboundMessage, error: DecryptionError) {
        let err = DecryptionService::<SinkFn>::validate_and_decrypt_message(node_identity.clone(), message.clone())
            .await
            .unwrap_err();
        assert_eq!(error, err);
        assert!(!passed_on(node_identity, message).await);
    }

    /// Receive a message successfully, decrypt if possible, and return the message
    async fn expect_no_error(
        node_identity: Arc<NodeIdentity>,
        message: DhtInboundMessage,
        decryption_succeeded: bool,
    ) -> DecryptedDhtMessage {
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        let mut service = DecryptionService::new(node_identity, service);

        // Receive the message and assert there were no errors
        block_on(service.call(message)).unwrap();
        assert!(result.lock().unwrap().is_some());
        let decrypted = result.lock().unwrap().take().unwrap();

        // See if decryption succeeded or failed as expected
        // We check both functions just in case!
        assert_eq!(decrypted.decryption_succeeded(), decryption_succeeded);
        assert_eq!(decrypted.decryption_failed(), !decryption_succeeded);

        // Return the decrypted message for further handling; decryption may have failed
        decrypted
    }

    #[test]
    fn poll_ready() {
        let service = service_fn(|_: DecryptedDhtMessage| future::ready(Result::<(), PipelineError>::Ok(())));
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(node_identity, service);

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
        // Only a Join may be re-sent under its original header, so no other message keeps its received bytes
        assert!(decrypted.raw_body.is_empty());
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

        // The undecrypted body is kept as received
        assert_eq!(decrypted.fail(), Some(&message.body));
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

            expect_error(
                node_identity.clone(),
                message,
                DecryptionError::BadEncryptedMessageSemantics,
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

        expect_error(node_identity, message, DecryptionError::MessageRejectDecryptionFailed).await;
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
        expect_error(node_identity, message, DecryptionError::BadEncryptedMessageSemantics).await;
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

                expect_error(node_identity.clone(), message, DecryptionError::InvalidSignature).await;
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

            expect_error(
                node_identity.clone(),
                message,
                DecryptionError::BadEncryptedMessageSemantics,
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

            expect_error(
                node_identity.clone(),
                message,
                DecryptionError::BadEncryptedMessageSemantics,
            )
            .await;
        }
    }

    #[tokio::test]
    /// An encrypted message for us, validly signed, whose ephemeral key is 32 bytes that are not a curve point, is
    /// dropped. The relay that delivered it could not have checked the point, so it is not blamed.
    async fn decrypt_inbound_fail_ephemeral_key_not_a_point() {
        let node_identity = make_node_identity();
        let signer = make_node_identity();
        let mut message = make_dht_inbound_message(
            &node_identity,
            &b"a secret message".to_vec(),
            DhtMessageFlags::ENCRYPTED,
            true,
            true,
        )
        .unwrap();

        let not_a_point = CompressedKey::from_canonical_bytes(&[0xffu8; 32]).unwrap();
        assert!(not_a_point.to_public_key().is_err());
        message.dht_header.ephemeral_public_key = Some(not_a_point);
        let binding_hash = crypt::create_message_domain_separated_hash(&message.dht_header, &message.body);
        message.dht_header.message_signature =
            crate::test_utils::make_valid_message_signature(signer.secret_key(), &binding_hash);

        expect_error(node_identity, message, DecryptionError::BadEncryptedMessageSemantics).await;
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

                expect_error(node_identity.clone(), message, DecryptionError::InvalidSignature).await;
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

                expect_error(node_identity.clone(), message, DecryptionError::InvalidSignature).await;
            }
        }
    }

    #[tokio::test]
    /// A valid signature whose bytes are not its canonical encoding (an appended unknown field, or a duplicated field)
    /// decodes to the same signature but is rejected; the canonical encoding is accepted
    async fn a_non_canonical_message_signature_is_rejected() {
        let node_identity = make_node_identity();
        let message = make_dht_inbound_message(
            &node_identity,
            &b"a secret message".to_vec(),
            DhtMessageFlags::ENCRYPTED,
            true,
            true,
        )
        .unwrap();
        let canonical = message.dht_header.message_signature.clone();

        let mut unknown_field = canonical.clone();
        // Unknown field 15 (varint) = 1
        unknown_field.extend_from_slice(&[0x78, 0x01]);
        // A first `signer_public_key` that the real one, later in the bytes, overrides
        let mut duplicated_field = vec![0x0a, 0x01, 0x00];
        duplicated_field.extend_from_slice(&canonical);

        for signature in [unknown_field, duplicated_field] {
            let decoded = ProtoMessageSignature::decode(signature.as_slice()).unwrap();
            assert_eq!(decoded.encode_to_vec(), canonical);
            let mut message = message.clone();
            message.dht_header.message_signature = signature;
            expect_error(node_identity.clone(), message, DecryptionError::InvalidSignature).await;
        }

        expect_no_error(node_identity, message, true).await;
    }

    #[tokio::test]
    /// An envelope body with more parts than the decode budget allows is rejected before prost decodes it, with the
    /// handling any other undecodable body gets: discarded, without a ban. (The encrypted case is
    /// `a_decryptable_body_that_fails_to_decode_is_dropped_not_forwarded`.)
    async fn an_envelope_body_over_the_decode_budget_is_rejected() {
        let node_identity = make_node_identity();
        let flood = EnvelopeBody {
            parts: vec![Vec::new(); DHT_MESSAGE_MAX_DECODE_ITEMS + 1],
        };
        // Prost alone accepts it
        assert_eq!(
            EnvelopeBody::decode(flood.to_encoded_bytes().as_slice()).unwrap(),
            flood
        );

        // Clear text (the test maker only signs encrypted messages correctly; this one needs no signature)
        let message = make_dht_inbound_message(&node_identity, &flood, DhtMessageFlags::NONE, false, true).unwrap();
        assert!(
            !passed_on(node_identity.clone(), message).await,
            "an over-budget body was passed on"
        );

        // At the parts cap (which is below the decode budget), it decodes
        let at_cap = EnvelopeBody {
            parts: vec![Vec::new(); MAX_ENVELOPE_PARTS],
        };
        let message =
            make_dht_inbound_message(&node_identity, &at_cap, DhtMessageFlags::ENCRYPTED, true, true).unwrap();
        let decrypted = expect_no_error(node_identity, message, true).await;
        assert_eq!(decrypted.decryption_result.unwrap(), at_cap);
    }

    #[tokio::test]
    /// An envelope body with more than MAX_ENVELOPE_PARTS parts (but inside the decode budget) is discarded without a
    /// ban, whether or not it is encrypted, and one at the cap is passed on
    async fn envelope_with_too_many_parts_is_rejected() {
        let node_identity = make_node_identity();
        let too_many = EnvelopeBody {
            parts: vec![b"part".to_vec(); MAX_ENVELOPE_PARTS + 1],
        };
        const { assert!(MAX_ENVELOPE_PARTS < DHT_MESSAGE_MAX_DECODE_ITEMS) };

        for flags in [DhtMessageFlags::NONE, DhtMessageFlags::ENCRYPTED] {
            // The test maker only signs encrypted messages correctly; a clear text message needs no signature
            let is_encrypted = flags.is_encrypted();
            let message = make_dht_inbound_message(&node_identity, &too_many, flags, is_encrypted, true).unwrap();
            assert_eq!(passed_on(node_identity.clone(), message).await, false, "{flags:?}");

            let at_cap = EnvelopeBody {
                parts: vec![b"part".to_vec(); MAX_ENVELOPE_PARTS],
            };
            let message = make_dht_inbound_message(&node_identity, &at_cap, flags, is_encrypted, true).unwrap();
            assert_eq!(passed_on(node_identity.clone(), message).await, true, "{flags:?}");
        }
    }

    /// Calls the decryption service with `message` and returns whether the message was passed on (to the forwarder
    /// and the rest of the pipeline)
    async fn passed_on(node_identity: Arc<NodeIdentity>, message: DhtInboundMessage) -> bool {
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        let mut service = DecryptionService::new(node_identity, service);
        service.call(message).await.unwrap();
        result.lock().unwrap().is_some()
    }

    #[tokio::test]
    /// An encrypted body that decrypts (so passes the AEAD check, and is provably for us) but does not decode, or is
    /// over the decode budget, is dropped without a ban. It used to be treated as "not for us" and passed on as
    /// undecrypted, which forwards it.
    async fn a_decryptable_body_that_fails_to_decode_is_dropped_not_forwarded() {
        let node_identity = make_node_identity();
        let over_budget = EnvelopeBody {
            parts: vec![Vec::new(); DHT_MESSAGE_MAX_DECODE_ITEMS + 1],
        };
        // Encodes as field 1 with a varint, which prost rejects for `EnvelopeBody.parts` (field 1, bytes)
        let undecodable = 1u32;

        for (name, message) in [
            (
                "over budget",
                make_dht_inbound_message(&node_identity, &over_budget, DhtMessageFlags::ENCRYPTED, true, true),
            ),
            (
                "undecodable",
                make_dht_inbound_message(&node_identity, &undecodable, DhtMessageFlags::ENCRYPTED, true, true),
            ),
        ] {
            assert_eq!(
                passed_on(node_identity.clone(), message.unwrap()).await,
                false,
                "{name}"
            );
        }
    }
}

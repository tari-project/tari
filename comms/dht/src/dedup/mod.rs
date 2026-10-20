// Copyright 2020, The Tari Project
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

//! # Dedup Cache
//!
//! Keeps track of messages seen before by this node and discards duplicates.

mod dedup_cache;

use std::task::Poll;

use blake2::Blake2b;
pub use dedup_cache::DedupCacheDatabase;
use digest::{Digest, consts::U32};
use futures::{future::BoxFuture, task::Context};
use log::*;
use prost::Message;
use tari_comms::pipeline::PipelineError;
use tari_crypto::{
    hash_domain,
    hashing::{DomainSeparatedHasher, LengthExtensionAttackResistant},
};
use tari_utilities::hex::Hex;
use tower::{Service, ServiceExt, layer::Layer};

use crate::{
    actor::DhtRequester,
    inbound::{DecryptedDhtMessage, DhtInboundMessage},
    message_signature::{MessageSignature, ProtoMessageSignature},
};

const LOG_TARGET: &str = "comms::dht::dedup";
const DEDUP_MESSAGE_HASH_LABEL: &str = "dedup.meesage_hash";

hash_domain!(CommsDhtDedupDomain, "com.tari.comms.dht", 1);

fn comms_dht_dedup_message_hash<D: Digest + LengthExtensionAttackResistant>(
    label: &'static str,
) -> DomainSeparatedHasher<D, CommsDhtDedupDomain> {
    DomainSeparatedHasher::<D, CommsDhtDedupDomain>::new_with_label(label)
}

/// The signature bytes that go into the dedup key. Re-encoding the decoded signature means a different encoding of the
/// same signature (unknown or repeated fields, another field order) does not give the message a new key.
pub fn canonical_signature_bytes(signature: &MessageSignature) -> Vec<u8> {
    signature.to_proto().encode_to_vec()
}

/// Returns the dedup key of a message whose signature has not been decoded yet, or `None` if it has a signature that
/// does not decode.
pub fn hash_inbound_message(msg: &DhtInboundMessage) -> Option<[u8; 32]> {
    let signature = &msg.dht_header.message_signature;
    if signature.is_empty() {
        return Some(create_message_hash(&[], &msg.body));
    }
    let signature = MessageSignature::try_from(ProtoMessageSignature::decode(signature.as_slice()).ok()?).ok()?;
    Some(create_message_hash(&canonical_signature_bytes(&signature), &msg.body))
}

/// Hashes the canonical signature bytes (empty for an unsigned message) and the body into a dedup key.
pub fn create_message_hash(sig_canonical: &[u8], body: &[u8]) -> [u8; 32] {
    let result = comms_dht_dedup_message_hash::<Blake2b<U32>>(DEDUP_MESSAGE_HASH_LABEL)
        .chain(sig_canonical)
        .chain(body)
        .finalize();

    let mut out = [0u8; 32];
    out.copy_from_slice(result.as_ref());
    out
}

/// # DHT Deduplication middleware
///
/// Takes in a `DecryptedDhtMessage` and checks the message signature cache for duplicates.
/// If a duplicate message is detected, it is discarded.
#[derive(Clone)]
pub struct DedupMiddleware<S> {
    next_service: S,
    dht_requester: DhtRequester,
    allowed_message_occurrences: usize,
}

impl<S> DedupMiddleware<S> {
    pub fn new(service: S, dht_requester: DhtRequester, allowed_message_occurrences: usize) -> Self {
        Self {
            next_service: service,
            dht_requester,
            allowed_message_occurrences,
        }
    }
}

impl<S> Service<DecryptedDhtMessage> for DedupMiddleware<S>
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

    fn call(&mut self, mut message: DecryptedDhtMessage) -> Self::Future {
        let next_service = self.next_service.clone();
        let mut dht_requester = self.dht_requester.clone();
        let allowed_message_occurrences = self.allowed_message_occurrences;
        Box::pin(async move {
            if message.dedup_hash.is_empty() {
                debug!(
                    target: LOG_TARGET,
                    "Message {} has no dedup key because its signature does not decode. Message discarded.",
                    message.tag
                );
                return Ok(());
            }
            trace!(
                target: LOG_TARGET,
                "Inserting message hash {} for message {} (Trace: {})",
                message.dedup_hash.to_hex(),
                message.tag,
                message.dht_header.message_tag
            );

            message.dedup_hit_count = dht_requester
                .add_message_to_dedup_cache(message.dedup_hash.clone(), message.source_peer.public_key.clone())
                .await?;

            if message.dedup_hit_count as usize > allowed_message_occurrences {
                trace!(
                    target: LOG_TARGET,
                    "Received duplicate message {} (hit_count = {}) from peer '{}' (Trace: {}). Message discarded.",
                    message.tag,
                    message.dedup_hit_count,
                    message.source_peer.node_id.short_str(),
                    message.dht_header.message_tag,
                );
                return Ok(());
            }

            trace!(
                target: LOG_TARGET,
                "Passing message {} (hit_count = {}) onto next service (Trace: {})",
                message.tag,
                message.dedup_hit_count,
                message.dht_header.message_tag
            );
            next_service.oneshot(message).await
        })
    }
}

pub struct DedupLayer {
    dht_requester: DhtRequester,
    allowed_message_occurrences: usize,
}

impl DedupLayer {
    pub fn new(dht_requester: DhtRequester, allowed_message_occurrences: usize) -> Self {
        Self {
            dht_requester,
            allowed_message_occurrences,
        }
    }
}

impl<S> Layer<S> for DedupLayer {
    type Service = DedupMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        DedupMiddleware::new(service, self.dht_requester.clone(), self.allowed_message_occurrences)
    }
}

#[cfg(test)]
mod test {
    use tari_comms::wrap_in_envelope_body;
    use tari_test_utils::panic_context;
    use tokio::runtime::Runtime;

    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{create_dht_actor_mock, make_dht_inbound_message, make_node_identity, service_spy},
    };

    #[test]
    fn process_message() {
        let rt = Runtime::new().unwrap();
        let spy = service_spy();

        let (dht_requester, mock) = create_dht_actor_mock();
        let mock_state = mock.get_shared_state();
        mock_state.set_number_of_message_hits(1);
        rt.spawn(mock.run());

        let mut dedup = DedupLayer::new(dht_requester, 3).layer(spy.to_service::<PipelineError>());

        panic_context!(cx);

        assert!(dedup.poll_ready(&mut cx).is_ready());
        let node_identity = make_node_identity();
        let inbound_message =
            make_dht_inbound_message(&node_identity, &vec![], DhtMessageFlags::empty(), false, false).unwrap();
        let decrypted_msg = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(vec![]), None, inbound_message);

        rt.block_on(dedup.call(decrypted_msg.clone())).unwrap();
        assert_eq!(spy.call_count(), 1);

        mock_state.set_number_of_message_hits(4);
        rt.block_on(dedup.call(decrypted_msg)).unwrap();
        assert_eq!(spy.call_count(), 1);
        // Drop dedup so that the DhtMock will stop running
        drop(dedup);
    }

    #[test]
    fn deterministic_hash() {
        const TEST_MSG: &[u8] = b"test123";
        const EXPECTED_HASH: &str = "64f1db1039353cf2ff7e09889d14ffd9885766c3dc494a59823a499edfe6997c";

        let node_identity = make_node_identity();
        let dht_message = make_dht_inbound_message(
            &node_identity,
            &TEST_MSG.to_vec(),
            DhtMessageFlags::empty(),
            false,
            false,
        )
        .unwrap();
        let decrypted1 = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(vec![]), None, dht_message);

        let node_identity = make_node_identity();
        let dht_message = make_dht_inbound_message(
            &node_identity,
            &TEST_MSG.to_vec(),
            DhtMessageFlags::empty(),
            false,
            false,
        )
        .unwrap();
        let decrypted2 = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(vec![]), None, dht_message);

        assert_eq!(decrypted1.dedup_hash, decrypted2.dedup_hash);
        let subjects = &[decrypted1.dedup_hash, decrypted2.dedup_hash];
        assert!(subjects.iter().all(|h| h.to_hex() == EXPECTED_HASH));
    }

    fn signed_message() -> DhtInboundMessage {
        make_dht_inbound_message(
            &make_node_identity(),
            &b"signed".to_vec(),
            DhtMessageFlags::empty(),
            true,
            false,
        )
        .unwrap()
    }

    fn with_signature(msg: &DhtInboundMessage, message_signature: Vec<u8>) -> DhtInboundMessage {
        let mut msg = msg.clone();
        msg.dht_header.message_signature = message_signature;
        msg
    }

    /// Encodes one length-delimited protobuf field. Every field used here is shorter than 128 bytes.
    fn encode_field(field_number: u8, bytes: &[u8]) -> Vec<u8> {
        let mut out = vec![(field_number << 3) | 2, u8::try_from(bytes.len()).unwrap()];
        out.extend_from_slice(bytes);
        out
    }

    #[test]
    fn signed_key_is_independent_of_signature_encoding() {
        let msg = signed_message();
        let canonical = msg.dht_header.message_signature.clone();
        let proto = ProtoMessageSignature::decode(canonical.as_slice()).unwrap();
        let expected = hash_inbound_message(&msg).unwrap();

        // An honest node's signature bytes are already canonical, so its key is unchanged
        assert_eq!(expected, create_message_hash(&canonical, &msg.body));

        // Unknown field appended
        let mut appended = canonical.clone();
        appended.extend_from_slice(&[0x78, 0x01]);
        // Field 1 duplicated; prost keeps the last value
        let mut duplicated = encode_field(1, &[0u8; 32]);
        duplicated.extend_from_slice(&canonical);
        // Fields in reverse order
        let mut reordered = encode_field(3, &proto.signature);
        reordered.extend(encode_field(2, &proto.public_nonce));
        reordered.extend(encode_field(1, &proto.signer_public_key));

        for variant in [appended, duplicated, reordered] {
            assert_ne!(variant, canonical);
            assert_eq!(hash_inbound_message(&with_signature(&msg, variant)).unwrap(), expected);
        }
    }

    #[test]
    fn signed_key_depends_on_every_signature_field() {
        let msg = signed_message();
        let proto = ProtoMessageSignature::decode(msg.dht_header.message_signature.as_slice()).unwrap();
        let expected = hash_inbound_message(&msg).unwrap();
        let other = ProtoMessageSignature::decode(signed_message().dht_header.message_signature.as_slice()).unwrap();

        let mut different_signer = proto.clone();
        different_signer.signer_public_key = other.signer_public_key;
        let mut different_nonce = proto.clone();
        different_nonce.public_nonce = other.public_nonce;
        let mut different_scalar = proto;
        different_scalar.signature = other.signature;

        for variant in [different_signer, different_nonce, different_scalar] {
            let key = hash_inbound_message(&with_signature(&msg, variant.encode_to_vec())).unwrap();
            assert_ne!(key, expected);
        }

        // Stripping the signature gives an unsigned message, which has its own key
        let unsigned = hash_inbound_message(&with_signature(&msg, Vec::new())).unwrap();
        assert_ne!(unsigned, expected);
    }

    #[test]
    fn undecodable_signature_gets_no_key_and_is_discarded() {
        let rt = Runtime::new().unwrap();
        let spy = service_spy();
        let (dht_requester, mock) = create_dht_actor_mock();
        // A hit count of 1 would pass the message on, so the spy is only skipped if dedup never asks the actor
        mock.get_shared_state().set_number_of_message_hits(1);
        rt.spawn(mock.run());
        let mut dedup = DedupLayer::new(dht_requester, 3).layer(spy.to_service::<PipelineError>());

        let msg = with_signature(&signed_message(), vec![0xff]);
        assert!(hash_inbound_message(&msg).is_none());
        let decrypted = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(vec![]), None, msg);
        assert!(decrypted.dedup_hash.is_empty());

        rt.block_on(dedup.call(decrypted)).unwrap();
        assert_eq!(spy.call_count(), 0);
        drop(dedup);
    }
}

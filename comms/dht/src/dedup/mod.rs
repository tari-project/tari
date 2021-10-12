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

mod dedup_cache;

pub use dedup_cache::DedupCacheDatabase;

use crate::{actor::DhtRequester, inbound::DecryptedDhtMessage};
use futures::{future::BoxFuture, task::Context};
use log::*;
use std::task::Poll;
use tari_comms::pipeline::PipelineError;
use tari_utilities::hex::Hex;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::dedup";

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
            trace!(
                target: LOG_TARGET,
                "Inserting message hash {} for message {} (Trace: {})",
                message.hash.to_hex(),
                message.tag,
                message.dht_header.message_tag
            );

            // TODO check if a reference is enough
            message.dedup_hit_count = dht_requester
                .add_message_to_dedup_cache(message.hash.clone(), message.source_peer.public_key.clone())
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
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        inbound::{DecryptionLayer, DhtInboundMessage},
        test_utils::{create_dht_actor_mock, make_dht_inbound_message, make_node_identity, service_spy},
    };
    use futures::future;
    use std::sync::{Arc, Mutex};
    use tari_comms::test_utils::mocks::create_connectivity_mock;
    use tari_test_utils::panic_context;
    use tokio::runtime::Runtime;
    use tower::service_fn;

    #[test]
    fn process_message() {
        let rt = Runtime::new().unwrap();
        let spy = service_spy();

        let (dht_requester, mock) = create_dht_actor_mock(1);
        let mock_state = mock.get_shared_state();
        mock_state.set_number_of_message_hits(1);
        rt.spawn(mock.run());

        let mut dedup = DedupLayer::new(dht_requester, 3).layer(spy.to_service::<PipelineError>());

        panic_context!(cx);

        assert!(dedup.poll_ready(&mut cx).is_ready());
        let node_identity = make_node_identity();
        let msg = make_dht_inbound_message(&node_identity, Vec::new(), DhtMessageFlags::empty(), false, false);

        // TODO clean up tests, maybe use new maker instead
        let (connectivity, _) = create_connectivity_mock();
        let service_decrypt = DecryptionLayer::new(Default::default(), node_identity.clone(), connectivity);
        let decrypted_msg = decrypt_dht_message(&rt, &service_decrypt, msg);

        rt.block_on(dedup.call(decrypted_msg.clone())).unwrap();
        assert_eq!(spy.call_count(), 1);

        mock_state.set_number_of_message_hits(4);
        rt.block_on(dedup.call(decrypted_msg)).unwrap();
        assert_eq!(spy.call_count(), 1);
        // Drop dedup so that the DhtMock will stop running
        drop(dedup);
        drop(service_decrypt);
    }

    #[test]
    fn deterministic_hash() {
        const TEST_MSG: &[u8] = b"test123";
        const EXPECTED_HASH: &str = "90cccd774db0ac8c6ea2deff0e26fc52768a827c91c737a2e050668d8c39c224";
        let node_identity = make_node_identity();
        let msg = make_dht_inbound_message(
            &node_identity,
            TEST_MSG.to_vec(),
            DhtMessageFlags::empty(),
            false,
            false,
        );

        let rt = Runtime::new().unwrap();
        let (connectivity, _) = create_connectivity_mock();
        let service_decrypt = DecryptionLayer::new(Default::default(), node_identity.clone(), connectivity);

        // TODO test fails as, presumably, this is not a valid message test123
        // TODO make a valid message, fix expected hash
        let decrypted1 = decrypt_dht_message(&rt, &service_decrypt, msg);
        assert!(decrypted1.decryption_succeeded());

        let node_identity = make_node_identity();
        let msg = make_dht_inbound_message(
            &node_identity,
            TEST_MSG.to_vec(),
            DhtMessageFlags::empty(),
            false,
            false,
        );

        let decrypted2 = decrypt_dht_message(&rt, &service_decrypt, msg);
        assert!(decrypted2.decryption_succeeded());

        assert_eq!(decrypted1.hash, decrypted2.hash);
        let subjects = &[decrypted1.hash, decrypted2.hash];
        assert!(subjects.iter().all(|h| h.to_hex() == EXPECTED_HASH));
    }

    fn decrypt_dht_message(
        rt: &Runtime,
        service_decrypt: &DecryptionLayer,
        msg: DhtInboundMessage,
    ) -> DecryptedDhtMessage {
        let result = Arc::new(Mutex::new(None));
        let service = service_fn({
            let result = result.clone();
            move |msg: DecryptedDhtMessage| {
                *result.lock().unwrap() = Some(msg);
                future::ready(Result::<(), PipelineError>::Ok(()))
            }
        });
        rt.block_on(service_decrypt.layer(service).call(msg)).unwrap();

        let res = result.lock().unwrap().take().unwrap();
        res
    }
}

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

use crate::{actor::DhtRequester, inbound::DhtInboundMessage};
use digest::Input;
use futures::{task::Context, Future};
use log::*;
use std::task::Poll;
use tari_comms::{pipeline::PipelineError, types::Challenge};
use tari_utilities::hex::Hex;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::dedup";

fn hash_inbound_message(message: &DhtInboundMessage) -> Vec<u8> {
    Challenge::new().chain(&message.body).result().to_vec()
}

/// # DHT Deduplication middleware
///
/// Takes in a `DhtInboundMessage` and checks the message signature cache for duplicates.
/// If a duplicate message is detected, it is discarded.
#[derive(Clone)]
pub struct DedupMiddleware<S> {
    next_service: S,
    dht_requester: DhtRequester,
}

impl<S> DedupMiddleware<S> {
    pub fn new(service: S, dht_requester: DhtRequester) -> Self {
        Self {
            next_service: service,
            dht_requester,
        }
    }
}

impl<S> Service<DhtInboundMessage> for DedupMiddleware<S>
where S: Service<DhtInboundMessage, Response = (), Error = PipelineError> + Clone
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: DhtInboundMessage) -> Self::Future {
        let next_service = self.next_service.clone();
        let mut dht_requester = self.dht_requester.clone();
        async move {
            let hash = hash_inbound_message(&message);
            trace!(
                target: LOG_TARGET,
                "Inserting message hash {} for message {} (Trace: {})",
                hash.to_hex(),
                message.tag,
                message.dht_header.message_tag
            );
            if dht_requester.insert_message_hash(hash).await? {
                trace!(
                    target: LOG_TARGET,
                    "Received duplicate message {} from peer '{}' (Trace: {}). Message discarded.",
                    message.tag,
                    message.source_peer.node_id.short_str(),
                    message.dht_header.message_tag,
                );
                return Ok(());
            }

            trace!(
                target: LOG_TARGET,
                "Passing message {} onto next service (Trace: {})",
                message.tag,
                message.dht_header.message_tag
            );
            next_service.oneshot(message).await
        }
    }
}

pub struct DedupLayer {
    dht_requester: DhtRequester,
}

impl DedupLayer {
    pub fn new(dht_requester: DhtRequester) -> Self {
        Self { dht_requester }
    }
}

impl<S> Layer<S> for DedupLayer {
    type Service = DedupMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        DedupMiddleware::new(service, self.dht_requester.clone())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{create_dht_actor_mock, make_dht_inbound_message, make_node_identity, service_spy},
    };
    use tari_test_utils::panic_context;
    use tokio::runtime::Runtime;

    #[test]
    fn process_message() {
        let mut rt = Runtime::new().unwrap();
        let spy = service_spy();

        let (dht_requester, mock) = create_dht_actor_mock(1);
        let mock_state = mock.get_shared_state();
        mock_state.set_signature_cache_insert(false);
        rt.spawn(mock.run());

        let mut dedup = DedupLayer::new(dht_requester).layer(spy.to_service::<PipelineError>());

        panic_context!(cx);

        assert!(dedup.poll_ready(&mut cx).is_ready());
        let node_identity = make_node_identity();
        let msg = make_dht_inbound_message(&node_identity, Vec::new(), DhtMessageFlags::empty(), false);

        rt.block_on(dedup.call(msg.clone())).unwrap();
        assert_eq!(spy.call_count(), 1);

        mock_state.set_signature_cache_insert(true);
        rt.block_on(dedup.call(msg)).unwrap();
        assert_eq!(spy.call_count(), 1);
        // Drop dedup so that the DhtMock will stop running
        drop(dedup);
    }

    #[test]
    fn deterministic_hash() {
        const TEST_MSG: &[u8] = b"test123";
        const EXPECTED_HASH: &str = "90cccd774db0ac8c6ea2deff0e26fc52768a827c91c737a2e050668d8c39c224";
        let node_identity = make_node_identity();
        let msg = make_dht_inbound_message(&node_identity, TEST_MSG.to_vec(), DhtMessageFlags::empty(), false);
        let hash1 = hash_inbound_message(&msg);

        let node_identity = make_node_identity();
        let msg = make_dht_inbound_message(&node_identity, TEST_MSG.to_vec(), DhtMessageFlags::empty(), false);
        let hash2 = hash_inbound_message(&msg);

        assert_eq!(hash1, hash2);
        let subjects = &[hash1, hash2];
        assert!(subjects.into_iter().all(|h| h.to_hex() == EXPECTED_HASH));
    }
}

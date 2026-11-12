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

//! # Liveness Service
//!
//! This service is responsible for sending pings to any peer as well as maintaining
//! some very basic counters for the number of ping/pongs sent and received.
//!
//! It is responsible for:
//! - handling requests to the Liveness backend. Types of requests can be found in the [LivenessRequest] enum, and
//! - reading incoming [PingPong] messages and processing them.
//!
//! In future, this service may be expanded to included periodic pings to maintain
//! latency and availability statistics for peers.
//!
//! [LivenessRequest]: ./messages/enum.LivenessRequets.html
//! [PingPong]: ./messages/enum.PingPong.html

pub mod config;
pub use self::config::{LivenessConfig, MAX_INFLIGHT_TTL};

pub mod error;

mod handle;
pub use handle::{
    LivenessEvent,
    LivenessEventSender,
    LivenessHandle,
    LivenessRequest,
    LivenessResponse,
    PingPongEvent,
};

mod message;
mod service;

mod state;
pub use state::Metadata;

#[cfg(feature = "test-mocks")]
pub mod mock;

use std::sync::Arc;

use futures::{Stream, StreamExt};
use log::*;
use tari_comms::{PeerManager, connectivity::ConnectivityRequester};
use tari_comms_dht::Dht;
use tari_service_framework::{
    ServiceInitializationError,
    ServiceInitializer,
    ServiceInitializerContext,
    async_trait,
    reply_channel,
};
use tokio::sync::broadcast;

use self::{message::PingPongMessage, service::LivenessService};
pub use crate::proto::liveness::MetadataKey;
use crate::{
    comms_connector::{PeerMessage, TopicSubscriptionFactory},
    domain_message::DomainMessage,
    services::liveness::state::LivenessState,
    tari_message::TariMessageType,
};

const LOG_TARGET: &str = "p2p::services::liveness";

/// The decode budget (see `tari_comms::decode_budget`) for an inbound `PingPongMessage`. Its only list is the
/// `metadata` map, which is charged one item per entry and legitimately holds one entry per `MetadataKey` (two today).
/// Without it, one 8 MiB messaging frame decodes into a map of ~1M entries.
pub const PING_PONG_MAX_DECODE_ITEMS: usize = 64;

/// Upper bound on the serialized byte length of an inbound `PingPongMessage`.
///
/// `PING_PONG_MAX_DECODE_ITEMS` bounds the number of `metadata` map entries, not the size of each `bytes` value: a
/// single multi-megabyte entry carries only one item yet allocates that buffer and clones it onto the liveness event
/// broadcast channel. An honest `PingPongMessage` with all defined metadata keys is under 256 bytes, so 4 KiB leaves
/// ample headroom while rejecting oversized frames before prost allocates anything.
pub const PING_PONG_MAX_PAYLOAD_BYTES: usize = 4096;

/// True if the message was encrypted, or signed by an identity other than the peer that delivered it. Honest nodes
/// send Ping and Pong messages directly in clear text and only on their own behalf, so such a message was relayed on
/// someone else's behalf. Decoding or handling it would blame or credit the delivering neighbour for someone else's
/// payload or `ChainMetadata`.
fn is_relayed_or_encrypted(msg: &PeerMessage) -> bool {
    msg.dht_header.flags.is_encrypted() ||
        msg.authenticated_origin
            .as_ref()
            .is_some_and(|origin| *origin != msg.source_peer.public_key)
}

fn decode_inbound_ping_pong(msg: Arc<PeerMessage>) -> DomainMessage<Result<PingPongMessage, prost::DecodeError>> {
    let inner = if msg.body.len() > PING_PONG_MAX_PAYLOAD_BYTES {
        Err(prost::DecodeError::new("PingPong payload exceeds maximum byte size"))
    } else {
        msg.decode_message_with_max_items(PING_PONG_MAX_DECODE_ITEMS)
    };
    DomainMessage {
        source_peer: msg.source_peer.clone(),
        dht_header: msg.dht_header.clone(),
        authenticated_origin: msg.authenticated_origin.clone(),
        inner,
    }
}

/// Initializer for the Liveness service handle and service future.
pub struct LivenessInitializer {
    pub(crate) config: Option<LivenessConfig>,
    inbound_message_subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
}

impl LivenessInitializer {
    /// Create a new LivenessInitializer from the inbound message subscriber
    pub fn new(
        config: LivenessConfig,
        inbound_message_subscription_factory: Arc<TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>>,
    ) -> Self {
        Self {
            config: Some(config),
            inbound_message_subscription_factory,
        }
    }

    /// Get a stream of inbound PingPong messages
    fn ping_stream(&self) -> impl Stream<Item = DomainMessage<Result<PingPongMessage, prost::DecodeError>>> + use<> {
        self.inbound_message_subscription_factory
            .get_subscription(TariMessageType::PingPong, "Liveness")
            .filter(|msg| {
                let relayed = is_relayed_or_encrypted(msg);
                if relayed {
                    debug!(
                        target: LOG_TARGET,
                        "Dropping encrypted or relayed PingPong message {} from peer '{}' (flags: {:?}, origin: {:?})",
                        msg.dht_header.message_tag,
                        msg.source_peer.node_id.short_str(),
                        msg.dht_header.flags,
                        msg.authenticated_origin,
                    );
                }
                futures::future::ready(!relayed)
            })
            .map(decode_inbound_ping_pong)
    }
}

#[async_trait]
impl ServiceInitializer for LivenessInitializer {
    async fn initialize(&mut self, context: ServiceInitializerContext) -> Result<(), ServiceInitializationError> {
        debug!(target: LOG_TARGET, "Initializing Liveness Service");
        let (sender, receiver) = reply_channel::unbounded();

        let (publisher, _) = broadcast::channel(200);

        // Register handle before waiting for handles to be ready
        context.register_handle(LivenessHandle::new(sender, publisher.clone()));

        // Saving a clone
        let config = self
            .config
            .take()
            .expect("Liveness service initialized more than once.");

        // Create a stream which receives PingPong messages from comms
        let ping_stream = self.ping_stream();

        // Spawn the Liveness service on the executor
        context.spawn_when_ready(|handles| async move {
            let dht = handles.expect_handle::<Dht>();
            let connectivity = handles.expect_handle::<ConnectivityRequester>();
            let outbound_messages = dht.outbound_requester();
            let peer_manager = handles.expect_handle::<Arc<PeerManager>>();

            let service = LivenessService::new(
                config,
                receiver,
                ping_stream,
                LivenessState::new(),
                connectivity,
                outbound_messages,
                publisher,
                handles.get_shutdown_signal(),
                peer_manager,
            );
            service.run().await;
            debug!(target: LOG_TARGET, "Liveness service has shut down");
        });

        debug!(target: LOG_TARGET, "Liveness service initialized");
        Ok(())
    }
}

#[cfg(test)]
mod decode_budget_test {
    use std::collections::HashMap;

    use prost::Message;
    use tari_comms::decode_budget::decode_with_max_items;

    use super::*;
    use crate::proto::liveness::PingPong;

    fn ping_with_metadata(metadata: HashMap<i32, Vec<u8>>) -> Vec<u8> {
        PingPongMessage {
            ping_pong: PingPong::Ping as i32,
            nonce: 1,
            metadata,
        }
        .encode_to_vec()
    }

    #[test]
    fn a_ping_with_too_many_metadata_entries_is_rejected_before_decoding_it() {
        let flood = ping_with_metadata(
            (0..=i32::try_from(PING_PONG_MAX_DECODE_ITEMS).unwrap())
                .map(|k| (k, Vec::new()))
                .collect(),
        );
        // Prost alone accepts it
        assert!(PingPongMessage::decode(flood.as_slice()).is_ok());
        let err = decode_with_max_items::<PingPongMessage>(&flood, PING_PONG_MAX_DECODE_ITEMS).unwrap_err();
        assert!(err.to_string().contains("decode budget"), "unexpected error: {err}");

        // A ping carrying every metadata key decodes
        let ping = ping_with_metadata(
            [
                (MetadataKey::ChainMetadata as i32, vec![1u8; 128]),
                (MetadataKey::ContactsLiveness as i32, Vec::new()),
            ]
            .into(),
        );
        let decoded = decode_with_max_items::<PingPongMessage>(&ping, PING_PONG_MAX_DECODE_ITEMS).unwrap();
        assert_eq!(decoded.metadata.len(), 2);
    }

    #[tokio::test]
    async fn ping_stream_drops_encrypted_or_relayed_messages_and_rejects_oversized_direct_payloads() {
        use std::time::Duration;

        use tari_comms::{message::MessageExt, wrap_in_envelope_body};
        use tari_comms_dht::{domain_message::MessageHeader, envelope::DhtMessageFlags, inbound::DecryptedDhtMessage};
        use tower::Service;

        use crate::{
            comms_connector::pubsub_connector,
            test_utils::{make_dht_inbound_message, make_node_identity},
        };

        let (mut connector, subscription_factory) = pubsub_connector(16);
        let initializer = LivenessInitializer::new(LivenessConfig::default(), Arc::new(subscription_factory));
        let mut stream = Box::pin(initializer.ping_stream());

        let relay_identity = make_node_identity();
        let other_origin = make_node_identity().public_key().clone();

        let make_decrypted = |flags: DhtMessageFlags, origin, body: Vec<u8>| {
            let header = MessageHeader::new(TariMessageType::PingPong as i32);
            let mut envelope = wrap_in_envelope_body!(header);
            envelope.push_part(body);
            let mut inbound = make_dht_inbound_message(&relay_identity, envelope.to_encoded_bytes());
            inbound.dht_header.flags = flags;
            DecryptedDhtMessage::succeeded(envelope, origin, inbound)
        };

        // Encrypted or foreign-signed frames (even with malformed/oversized bodies) are dropped before decoding so the
        // delivering neighbour is never blamed or answered.
        for (flags, origin) in [
            (DhtMessageFlags::ENCRYPTED, Some(other_origin.clone())),
            (DhtMessageFlags::ENCRYPTED, None),
            (DhtMessageFlags::ENCRYPTED, Some(relay_identity.public_key().clone())),
            (DhtMessageFlags::NONE, Some(other_origin)),
        ] {
            connector
                .call(make_decrypted(flags, origin, vec![
                    0xff;
                    PING_PONG_MAX_PAYLOAD_BYTES
                        .saturating_add(1)
                ]))
                .await
                .unwrap();
        }

        // An oversized clear-text direct frame is yielded as a DecodeError without allocating a multi-KB metadata map,
        // so LivenessService bans the direct sender.
        let oversized_direct = ping_with_metadata([(MetadataKey::ChainMetadata as i32, vec![7u8; 8192])].into());
        assert!(oversized_direct.len() > PING_PONG_MAX_PAYLOAD_BYTES);
        connector
            .call(make_decrypted(DhtMessageFlags::NONE, None, oversized_direct))
            .await
            .unwrap();

        let first = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .expect("direct message did not arrive")
            .expect("stream ended");
        assert!(first.inner.is_err(), "oversized direct PingPong should fail decode");

        // Honest direct clear-text messages (unsigned or signed by `source_peer`) pass through cleanly.
        let valid_body = ping_with_metadata([(MetadataKey::ChainMetadata as i32, vec![1u8; 64])].into());
        for origin in [None, Some(relay_identity.public_key().clone())] {
            connector
                .call(make_decrypted(DhtMessageFlags::NONE, origin, valid_body.clone()))
                .await
                .unwrap();
            let msg = tokio::time::timeout(Duration::from_secs(2), stream.next())
                .await
                .expect("valid direct message did not arrive")
                .expect("stream ended");
            let decoded = msg.inner.expect("valid direct PingPong should decode");
            assert_eq!(decoded.nonce, 1);
        }
    }
}

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
use tari_comms::{PeerManager, message::InboundMessage, pipeline::PipelineError};
use tower::{Service, ServiceExt, layer::Layer};

use crate::{
    inbound::DhtInboundMessage,
    proto::{envelope::DhtEnvelope, is_canonical},
};

const LOG_TARGET: &str = "comms::dht::deserialize";

#[cfg(feature = "metrics")]
fn inbound_non_canonical() -> tari_metrics::IntCounter {
    static METER: std::sync::LazyLock<tari_metrics::IntCounter> = std::sync::LazyLock::new(|| {
        tari_metrics::register_int_counter(
            "comms::dht::inbound_non_canonical_total",
            "Inbound DHT envelopes dropped because their bytes are not the canonical encoding",
        )
        .unwrap()
    });

    METER.clone()
}

/// # DHT Deserialization middleware
///
/// Takes in an `InboundMessage` and deserializes the body into a [DhtEnvelope].
/// The `next_service` is called with a constructed [DhtInboundMessage] which contains
/// the relevant comms-level and dht-level information.
#[derive(Clone)]
pub struct DhtDeserializeMiddleware<S> {
    next_service: S,
    peer_manager: Arc<PeerManager>,
}

impl<S> DhtDeserializeMiddleware<S> {
    pub fn new(peer_manager: Arc<PeerManager>, service: S) -> Self {
        Self {
            peer_manager,
            next_service: service,
        }
    }
}

impl<S> Service<InboundMessage> for DhtDeserializeMiddleware<S>
where
    S: Service<DhtInboundMessage, Response = (), Error = PipelineError> + Clone + Send + 'static,
    S::Future: Send,
{
    type Error = PipelineError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: InboundMessage) -> Self::Future {
        let next_service = self.next_service.clone();
        let peer_manager = self.peer_manager.clone();
        Box::pin(async move {
            trace!(target: LOG_TARGET, "Deserializing InboundMessage {}", message.tag);

            let InboundMessage {
                source_peer,
                mut body,
                tag,
                ..
            } = message;

            if body.is_empty() {
                return Err(anyhow::anyhow!("Received empty message from peer '{source_peer}'"));
            }

            let raw = body.clone();
            match DhtEnvelope::decode(&mut body) {
                Ok(dht_envelope) => {
                    // The same envelope can be encoded in many ways. Only the canonical encoding is accepted. Relays
                    // rebuild the header, so what is preserved hop to hop is the signed bytes (body and signature).
                    if !is_canonical(&raw, &dht_envelope) {
                        debug!(
                            target: LOG_TARGET,
                            "Dropping non-canonically encoded DHT envelope from peer '{source_peer}' ({tag})"
                        );
                        #[cfg(feature = "metrics")]
                        inbound_non_canonical().inc();
                        return Ok(());
                    }
                    let res;
                    if let Some(source_peer) = peer_manager.find_by_node_id(&source_peer).await? {
                        // .or_not_found(&source_peer)
                        // .map(Arc::new)?;

                        let inbound_msg = DhtInboundMessage::new(
                            tag,
                            dht_envelope.header.try_into()?,
                            Arc::new(source_peer),
                            dht_envelope.body,
                        );
                        trace!(
                            target: LOG_TARGET,
                            "Deserialization succeeded. Passing message {} onto next service (Trace: {})",
                            tag,
                            inbound_msg.dht_header.message_tag
                        );

                        let next_service = next_service.ready_oneshot().await?;
                        res = next_service.oneshot(inbound_msg).await;
                    } else {
                        warn!(
                            target: LOG_TARGET,
                            "Received message from unknown peer '{source_peer}'. Message tag: {tag}"
                        );
                        peer_manager
                            .ban_peer_by_node_id(
                                &source_peer,
                                Duration::from_secs(60 * 5),
                                "Received message from unknown peer".to_string(),
                            )
                            .await?;
                        res = Err(anyhow::anyhow!("Received message from unknown peer '{}'", source_peer));
                    }
                    res
                },
                Err(err) => {
                    error!(target: LOG_TARGET, "DHT deserialization failed: {err}");
                    Err(err.into())
                },
            }
        })
    }
}

pub struct DeserializeLayer {
    peer_manager: Arc<PeerManager>,
}

impl DeserializeLayer {
    pub fn new(peer_manager: Arc<PeerManager>) -> Self {
        Self { peer_manager }
    }
}

impl<S> Layer<S> for DeserializeLayer {
    type Service = DhtDeserializeMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        DhtDeserializeMiddleware::new(self.peer_manager.clone(), service)
    }
}

#[cfg(test)]
mod test {
    use tari_comms::message::{MessageExt, MessageTag};

    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{
            assert_send_static_service,
            build_peer_manager,
            make_comms_inbound_message,
            make_dht_envelope,
            make_node_identity,
            service_spy,
        },
    };

    #[tokio::test]
    async fn deserialize() {
        let spy = service_spy();
        let peer_manager = build_peer_manager();
        let node_identity = make_node_identity();
        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();

        let mut deserialize = DeserializeLayer::new(peer_manager).layer(spy.to_service::<PipelineError>());
        assert_send_static_service(&deserialize);

        let dht_envelope = make_dht_envelope(
            &node_identity,
            &b"A".to_vec(),
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
            false,
        )
        .unwrap();

        deserialize
            .ready()
            .await
            .unwrap()
            .call(make_comms_inbound_message(
                &node_identity,
                dht_envelope.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();

        let msg = spy.pop_request().unwrap();
        assert_eq!(msg.body, b"A".to_vec().to_encoded_bytes());
        assert_eq!(msg.dht_header, dht_envelope.header.unwrap().try_into().unwrap());
    }

    #[tokio::test]
    async fn a_non_canonical_envelope_is_dropped_without_a_ban() {
        let spy = service_spy();
        let peer_manager = build_peer_manager();
        let node_identity = make_node_identity();
        peer_manager.add_or_update_peer(node_identity.to_peer()).await.unwrap();
        let mut deserialize = DeserializeLayer::new(peer_manager.clone()).layer(spy.to_service::<PipelineError>());

        let dht_envelope = make_dht_envelope(
            &node_identity,
            &b"A".to_vec(),
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
            false,
        )
        .unwrap();
        let mut bytes = dht_envelope.to_encoded_bytes();
        // Unknown field 15 (varint) = 1: decodes to the same envelope
        bytes.extend_from_slice(&[0x78, 0x01]);
        assert_eq!(DhtEnvelope::decode(bytes.as_slice()).unwrap(), dht_envelope);

        #[cfg(feature = "metrics")]
        let dropped_before = inbound_non_canonical().get();
        deserialize
            .ready()
            .await
            .unwrap()
            .call(make_comms_inbound_message(&node_identity, bytes.into()))
            .await
            .unwrap();

        assert_eq!(spy.call_count(), 0);
        assert!(!peer_manager.is_peer_banned(node_identity.node_id()).await.unwrap());
        #[cfg(feature = "metrics")]
        assert!(inbound_non_canonical().get() > dropped_before);
    }
}

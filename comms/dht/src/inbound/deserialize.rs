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

use crate::{inbound::DhtInboundMessage, proto::envelope::DhtEnvelope};
use futures::{task::Context, Future};
use log::*;
use prost::Message;
use std::{convert::TryInto, sync::Arc, task::Poll};
use tari_comms::{message::InboundMessage, pipeline::PipelineError, PeerManager};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::deserialize";

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
where S: Service<DhtInboundMessage, Response = (), Error = PipelineError> + Clone + 'static
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: InboundMessage) -> Self::Future {
        let next_service = self.next_service.clone();
        let peer_manager = self.peer_manager.clone();
        async move {
            trace!(target: LOG_TARGET, "Deserializing InboundMessage {}", message.tag);

            let InboundMessage {
                source_peer,
                mut body,
                tag,
                ..
            } = message;

            if body.is_empty() {
                return Err(format!("Received empty message from peer '{}'", source_peer)
                    .as_str()
                    .into());
            }

            match DhtEnvelope::decode(&mut body) {
                Ok(dht_envelope) => {
                    let source_peer = peer_manager
                        .find_by_node_id(&source_peer)
                        .await
                        .map(Arc::new)
                        .map_err(PipelineError::from_debug)?;

                    let inbound_msg = DhtInboundMessage::new(
                        tag,
                        dht_envelope.header.try_into().map_err(PipelineError::from_debug)?,
                        source_peer,
                        dht_envelope.body,
                    );
                    trace!(
                        target: LOG_TARGET,
                        "Deserialization succeeded. Passing message {} onto next service (Trace: {})",
                        tag,
                        inbound_msg.dht_header.message_tag
                    );

                    next_service.oneshot(inbound_msg).await
                },
                Err(err) => {
                    error!(target: LOG_TARGET, "DHT deserialization failed: {}", err);
                    Err(PipelineError::from_debug(err))
                },
            }
        }
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
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{
            make_comms_inbound_message,
            make_dht_envelope,
            make_node_identity,
            make_peer_manager,
            service_spy,
        },
    };
    use tari_comms::message::{MessageExt, MessageTag};

    #[tokio_macros::test_basic]
    async fn deserialize() {
        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        peer_manager.add_peer(node_identity.to_peer()).await.unwrap();

        let mut deserialize = DeserializeLayer::new(peer_manager).layer(spy.to_service::<PipelineError>());

        let dht_envelope = make_dht_envelope(
            &node_identity,
            b"A".to_vec(),
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
        );

        deserialize
            .ready_and()
            .await
            .unwrap()
            .call(make_comms_inbound_message(
                &node_identity,
                dht_envelope.to_encoded_bytes().into(),
            ))
            .await
            .unwrap();

        let msg = spy.pop_request().unwrap();
        assert_eq!(msg.body, b"A".to_vec());
        assert_eq!(msg.dht_header, dht_envelope.header.unwrap().try_into().unwrap());
    }
}

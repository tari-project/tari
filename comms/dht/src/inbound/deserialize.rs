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
use std::{convert::TryInto, task::Poll};
use tari_comms::{message::InboundMessage, pipeline::PipelineError};
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
}

impl<S> DhtDeserializeMiddleware<S> {
    pub fn new(service: S) -> Self {
        Self { next_service: service }
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
                    let inbound_msg = DhtInboundMessage::new(
                        tag,
                        dht_envelope.header.try_into().map_err(PipelineError::from_debug)?,
                        source_peer,
                        dht_envelope.body,
                    );
                    debug!(
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

#[derive(Default)]
pub struct DeserializeLayer;

impl DeserializeLayer {
    pub fn new() -> Self {
        DeserializeLayer
    }
}

impl<S> Layer<S> for DeserializeLayer {
    type Service = DhtDeserializeMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        DhtDeserializeMiddleware::new(service)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{make_comms_inbound_message, make_dht_envelope, make_node_identity, service_spy},
    };
    use futures::executor::block_on;
    use tari_comms::message::{MessageExt, MessageTag};
    use tari_test_utils::panic_context;

    #[test]
    fn deserialize() {
        let spy = service_spy();
        let mut deserialize = DeserializeLayer::new().layer(spy.to_service::<PipelineError>());

        panic_context!(cx);

        assert!(deserialize.poll_ready(&mut cx).is_ready());
        let node_identity = make_node_identity();
        let dht_envelope = make_dht_envelope(
            &node_identity,
            b"A".to_vec(),
            DhtMessageFlags::empty(),
            false,
            MessageTag::new(),
        );
        block_on(deserialize.call(make_comms_inbound_message(
            &node_identity,
            dht_envelope.to_encoded_bytes().into(),
        )))
        .unwrap();

        let msg = spy.pop_request().unwrap();
        assert_eq!(msg.body, b"A".to_vec());
        assert_eq!(msg.dht_header, dht_envelope.header.unwrap().try_into().unwrap());
    }
}

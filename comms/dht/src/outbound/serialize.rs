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

use crate::{
    consts::DHT_ENVELOPE_HEADER_VERSION,
    outbound::message::DhtOutboundMessage,
    proto::envelope::{DhtEnvelope, DhtHeader},
};
use futures::{task::Context, Future};
use log::*;
use std::task::Poll;
use tari_comms::{
    message::{MessageExt, OutboundMessage},
    pipeline::PipelineError,
    Bytes,
};
use tari_utilities::ByteArray;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::serialize";

#[derive(Clone)]
pub struct SerializeMiddleware<S> {
    inner: S,
}

impl<S> SerializeMiddleware<S> {
    pub fn new(service: S) -> Self {
        Self { inner: service }
    }
}

impl<S> Service<DhtOutboundMessage> for SerializeMiddleware<S>
where S: Service<OutboundMessage, Response = (), Error = PipelineError> + Clone + 'static
{
    type Error = PipelineError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: DhtOutboundMessage) -> Self::Future {
        let next_service = self.inner.clone();
        async move {
            trace!(target: LOG_TARGET, "Serializing outbound message {:?}", message.tag);

            let DhtOutboundMessage {
                tag,
                destination_node_id,
                custom_header,
                body,
                ephemeral_public_key,
                destination,
                dht_message_type,
                network,
                dht_flags,
                origin_mac,
                reply,
                ..
            } = message;

            let dht_header = custom_header.map(DhtHeader::from).unwrap_or_else(|| DhtHeader {
                version: DHT_ENVELOPE_HEADER_VERSION,
                origin_mac: origin_mac.map(|b| b.to_vec()).unwrap_or_else(Vec::new),
                ephemeral_public_key: ephemeral_public_key.map(|e| e.to_vec()).unwrap_or_else(Vec::new),
                message_type: dht_message_type as i32,
                network: network as i32,
                flags: dht_flags.bits(),
                destination: Some(destination.into()),
                message_tag: tag.as_value(),
            });
            let envelope = DhtEnvelope::new(dht_header, body);

            let body = Bytes::from(envelope.to_encoded_bytes());

            next_service
                .oneshot(OutboundMessage {
                    tag,
                    peer_node_id: destination_node_id,
                    reply,
                    body,
                })
                .await
        }
    }
}

#[derive(Default)]
pub struct SerializeLayer;

impl SerializeLayer {
    pub fn new() -> Self {
        Self
    }
}

impl<S> Layer<S> for SerializeLayer {
    type Service = SerializeMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        SerializeMiddleware::new(service)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_utils::{create_outbound_message, service_spy};
    use futures::executor::block_on;
    use prost::Message;
    use tari_comms::peer_manager::NodeId;
    use tari_test_utils::panic_context;

    #[test]
    fn serialize() {
        let spy = service_spy();
        let mut serialize = SerializeLayer.layer(spy.to_service::<PipelineError>());

        panic_context!(cx);

        assert!(serialize.poll_ready(&mut cx).is_ready());
        let body = b"A";
        let msg = create_outbound_message(body);
        block_on(serialize.call(msg)).unwrap();

        let mut msg = spy.pop_request().unwrap();
        let dht_envelope = DhtEnvelope::decode(&mut msg.body).unwrap();
        assert_eq!(dht_envelope.body, b"A".to_vec());
        assert_eq!(msg.peer_node_id, NodeId::default());
    }
}

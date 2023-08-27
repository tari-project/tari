// Copyright 2019, The Taiji Project
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

use std::task::Poll;

use futures::task::Context;
use log::*;
use taiji_comms::{
    message::{MessageExt, OutboundMessage},
    pipeline::PipelineError,
    Bytes,
};
use tari_utilities::ByteArray;
use tower::{layer::Layer, util::Oneshot, Service, ServiceExt};

use crate::{
    outbound::message::DhtOutboundMessage,
    proto::envelope::{DhtEnvelope, DhtHeader},
};

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
where
    S: Service<OutboundMessage, Response = (), Error = PipelineError> + Clone + Send,
    S::Future: Send,
{
    type Error = PipelineError;
    type Future = Oneshot<S, OutboundMessage>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, message: DhtOutboundMessage) -> Self::Future {
        let next_service = self.inner.clone();

        let DhtOutboundMessage {
            protocol_version,
            tag,
            destination_node_id,
            custom_header,
            body,
            ephemeral_public_key,
            destination,
            dht_message_type,
            dht_flags,
            message_signature,
            reply,
            expires,
            ..
        } = message;
        trace!(
            target: LOG_TARGET,
            "Serializing outbound message {:?} for peer `{}`",
            message.tag,
            destination_node_id.short_str()
        );
        let dht_header = custom_header.map(DhtHeader::from).unwrap_or_else(|| DhtHeader {
            major: protocol_version.as_major(),
            message_signature: message_signature.map(|b| b.to_vec()).unwrap_or_else(Vec::new),
            ephemeral_public_key: ephemeral_public_key.map(|e| e.to_vec()).unwrap_or_else(Vec::new),
            message_type: dht_message_type.into(),
            flags: dht_flags.bits(),
            destination: Some(destination.into()),
            message_tag: tag.as_value(),
            expires,
        });
        let envelope = DhtEnvelope::new(dht_header, body.into());

        let body = Bytes::from(envelope.to_encoded_bytes());

        trace!(
            target: LOG_TARGET,
            "Serialized outbound message {} for peer `{}`. Passing onto next service",
            tag,
            destination_node_id.short_str()
        );
        next_service.oneshot(OutboundMessage {
            tag,
            peer_node_id: destination_node_id,
            reply,
            body,
        })
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
    use prost::Message;
    use taiji_comms::peer_manager::NodeId;

    use super::*;
    use crate::test_utils::{assert_send_static_service, create_outbound_message, service_spy};

    #[tokio::test]
    async fn serialize() {
        let spy = service_spy();
        let mut serialize = SerializeLayer.layer(spy.to_service::<PipelineError>());

        let body = b"A";
        let msg = create_outbound_message(body);
        assert_send_static_service(&serialize);

        let service = serialize.ready().await.unwrap();
        service.call(msg).await.unwrap();
        let mut msg = spy.pop_request().unwrap();
        let dht_envelope = DhtEnvelope::decode(&mut msg.body).unwrap();
        assert_eq!(dht_envelope.body, b"A".to_vec());
        assert_eq!(msg.peer_node_id, NodeId::default());
    }
}

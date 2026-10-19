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

use std::task::Poll;

use futures::task::Context;
use log::*;
use tari_comms::{
    Bytes,
    message::{MessageExt, OutboundMessage},
    pipeline::PipelineError,
};
use tari_utilities::ByteArray;
use tower::{Service, ServiceExt, layer::Layer, util::Oneshot};

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
            expires: expires.unwrap_or_default(),
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
    use std::sync::Arc;

    use prost::Message;
    use tari_comms::{peer_manager::NodeId, wrap_in_envelope_body};

    use super::*;
    use crate::{
        envelope::{DhtMessageFlags, DhtMessageType, NodeDestination},
        proto::{dht::JoinMessage, is_canonical},
        test_utils::{
            assert_send_static_service,
            create_outbound_message,
            make_keypair,
            make_node_identity,
            make_valid_message_signature,
            service_spy,
        },
    };

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

    /// Serializes `msg` and asserts the envelope it produces decodes and re-encodes to exactly the same bytes, so the
    /// inbound canonical-encoding check accepts it
    async fn assert_round_trips(msg: DhtOutboundMessage) {
        let spy = service_spy();
        let mut serialize = SerializeLayer.layer(spy.to_service::<PipelineError>());
        serialize.ready().await.unwrap().call(msg).await.unwrap();
        let sent = spy.pop_request().unwrap().body;
        let envelope = DhtEnvelope::decode(sent.clone()).unwrap();
        assert!(is_canonical(&sent, &envelope));
    }

    #[tokio::test]
    async fn honest_envelopes_are_canonically_encoded() {
        let node_identity = make_node_identity();
        let signature = Bytes::from(make_valid_message_signature(
            node_identity.secret_key(),
            b"binding hash",
        ));

        // A clear text Join addressed to the sender's own key, with an expiry
        let mut join =
            create_outbound_message(&wrap_in_envelope_body!(JoinMessage::from(&node_identity)).to_encoded_bytes());
        join.destination = node_identity.public_key().clone().into();
        join.dht_message_type = DhtMessageType::Join;
        join.message_signature = Some(signature.clone());
        join.expires = Some(1_900_000_000);
        assert_round_trips(join).await;

        // An encrypted message for a public key
        let (_, ephemeral_public_key) = make_keypair();
        let mut encrypted = create_outbound_message(&[7u8; 120]);
        encrypted.destination = make_node_identity().public_key().clone().into();
        encrypted.dht_flags = DhtMessageFlags::ENCRYPTED;
        encrypted.ephemeral_public_key = Some(Arc::new(ephemeral_public_key));
        encrypted.message_signature = Some(signature);
        assert_round_trips(encrypted).await;

        // Unknown destination, no expiry and no signature
        let mut unsigned = create_outbound_message(b"A");
        unsigned.destination = NodeDestination::Unknown;
        assert_round_trips(unsigned).await;
    }
}

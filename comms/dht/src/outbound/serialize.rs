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

use crate::{consts::DHT_RNG, outbound::message::DhtOutboundMessage, proto::envelope::DhtEnvelope};
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::{
    message::{MessageExt, MessageFlags},
    outbound_message_service::OutboundMessage,
    peer_manager::NodeIdentity,
    utils::signature,
};
use tari_comms_middleware::MiddlewareError;
use tari_utilities::{hex::Hex, message_format::MessageFormat};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::dht::serialize";

#[derive(Clone)]
pub struct SerializeMiddleware<S> {
    inner: S,
    node_identity: Arc<NodeIdentity>,
}

impl<S> SerializeMiddleware<S> {
    pub fn new(service: S, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            inner: service,
            node_identity,
        }
    }
}

impl<S> Service<DhtOutboundMessage> for SerializeMiddleware<S>
where
    S: Service<OutboundMessage, Response = ()> + Clone + 'static,
    S::Error: Into<MiddlewareError>,
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtOutboundMessage) -> Self::Future {
        Self::serialize(self.inner.clone(), Arc::clone(&self.node_identity), msg)
    }
}

impl<S> SerializeMiddleware<S>
where
    S: Service<OutboundMessage, Response = ()>,
    S::Error: Into<MiddlewareError>,
{
    pub async fn serialize(
        next_service: S,
        node_identity: Arc<NodeIdentity>,
        message: DhtOutboundMessage,
    ) -> Result<(), MiddlewareError>
    {
        debug!(target: LOG_TARGET, "Serializing outbound message");

        let DhtOutboundMessage {
            mut dht_header,
            body,
            destination_peer,
            comms_flags,
            ..
        } = message;

        // If forwarding the message, the DhtHeader already has a signature that should not change
        if comms_flags.contains(MessageFlags::FORWARDED) {
            trace!(target: LOG_TARGET, "Forwarded message. Message will not be signed");
        } else {
            // Sign the body
            let signature = DHT_RNG
                .with(|rng| signature::sign(&mut *rng.borrow_mut(), node_identity.secret_key().clone(), &body))?;
            dht_header.origin_signature = signature.to_binary()?;
            trace!(
                target: LOG_TARGET,
                "Signed message: {}",
                dht_header.origin_signature.to_hex()
            );
        }

        let envelope = DhtEnvelope::new(dht_header.into(), body);

        let body = envelope.to_encoded_bytes()?;

        next_service
            .oneshot(OutboundMessage::new(destination_peer.node_id, comms_flags, body))
            .await
            .map_err(Into::into)
    }
}

pub struct SerializeLayer {
    node_identity: Arc<NodeIdentity>,
}

impl SerializeLayer {
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        Self { node_identity }
    }
}

impl<S> Layer<S> for SerializeLayer {
    type Service = SerializeMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        SerializeMiddleware::new(service, Arc::clone(&self.node_identity))
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        outbound::OutboundEncryption,
        test_utils::{make_dht_header, make_node_identity, service_spy},
    };
    use futures::executor::block_on;
    use prost::Message;
    use tari_comms::{
        connection::NetAddressesWithStats,
        message::MessageFlags,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
        types::CommsPublicKey,
    };
    use tari_test_utils::panic_context;

    #[test]
    fn serialize() {
        let spy = service_spy();
        let node_identity = make_node_identity();
        let mut serialize = SerializeLayer::new(Arc::clone(&node_identity)).layer(spy.to_service::<MiddlewareError>());

        panic_context!(cx);

        assert!(serialize.poll_ready(&mut cx).is_ready());
        let body = b"A".to_vec();
        let msg = DhtOutboundMessage::new(
            Peer::new(
                CommsPublicKey::default(),
                NodeId::default(),
                NetAddressesWithStats::new(vec![]),
                PeerFlags::empty(),
                PeerFeatures::COMMUNICATION_NODE,
            ),
            make_dht_header(&node_identity, &body, DhtMessageFlags::empty()),
            OutboundEncryption::None,
            MessageFlags::empty(),
            body,
        );
        block_on(serialize.call(msg)).unwrap();

        let msg = spy.pop_request().unwrap();
        let dht_envelope = DhtEnvelope::decode(&msg.body).unwrap();
        assert_eq!(dht_envelope.body, b"A".to_vec());
        assert_eq!(msg.peer_node_id, NodeId::default());
    }
}

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

use crate::{consts::DHT_RNG, message::DhtEnvelope, outbound::message::DhtOutboundMessage};
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::{outbound_message_service::OutboundMessage, peer_manager::NodeIdentity, utils::signature};
use tari_comms_middleware::{error::box_as_middleware_error, MiddlewareError};
use tari_utilities::message_format::MessageFormat;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::dht::deserialize";

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
        mut next_service: S,
        node_identity: Arc<NodeIdentity>,
        message: DhtOutboundMessage,
    ) -> Result<(), MiddlewareError>
    {
        trace!(target: LOG_TARGET, "Serializing DhtOutboundMessage");
        next_service.ready().await.map_err(Into::into)?;

        let mut rng = DHT_RNG.with(|rng| rng.clone());

        let DhtOutboundMessage {
            mut dht_header,
            body,
            peer_node_identity,
            comms_flags,
            ..
        } = message;

        // Sign the body
        let signature =
            signature::sign(&mut rng, node_identity.secret_key.clone(), &*body).map_err(box_as_middleware_error)?;
        dht_header.origin_signature = signature.to_binary().map_err(box_as_middleware_error)?;

        let envelope = DhtEnvelope::new(dht_header, body);

        let body = envelope.to_binary().map_err(box_as_middleware_error)?;

        next_service
            .call(OutboundMessage::new(peer_node_identity.node_id, comms_flags, body))
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
        message::DhtMessageFlags,
        test_utils::{make_dht_header, make_node_identity, service_spy},
    };
    use futures::executor::block_on;
    use tari_comms::{
        message::MessageFlags,
        peer_manager::{NodeId, PeerNodeIdentity},
        types::CommsPublicKey,
    };
    use tari_test_utils::panic_context;

    #[test]
    fn serialize() {
        let spy = service_spy();
        let node_identity = make_node_identity();
        let mut serialize = SerializeLayer::new(Arc::clone(&node_identity)).layer(spy.service::<MiddlewareError>());

        panic_context!(cx);

        assert!(serialize.poll_ready(&mut cx).is_ready());
        let body = b"A".to_vec();
        let msg = DhtOutboundMessage::new(
            PeerNodeIdentity::new(NodeId::default(), CommsPublicKey::default()),
            make_dht_header(&node_identity, &body, DhtMessageFlags::empty()),
            CommsPublicKey::default(),
            MessageFlags::empty(),
            body,
        );
        block_on(serialize.call(msg)).unwrap();

        let msg = spy.pop_request().unwrap();
        let dht_envelope = DhtEnvelope::from_binary(&msg.body).unwrap();
        assert_eq!(dht_envelope.body, b"A".to_vec());
        assert_eq!(msg.peer_node_id, NodeId::default());
    }
}

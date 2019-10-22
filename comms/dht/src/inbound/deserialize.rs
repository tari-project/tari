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

use crate::{envelope::DhtEnvelope, inbound::DhtInboundMessage};
use futures::{task::Context, Future, Poll};
use log::*;
use tari_comms::message::InboundMessage;
use tari_comms_middleware::MiddlewareError;
use tari_utilities::message_format::MessageFormat;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::dht::deserialize";

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
where
    S: Service<DhtInboundMessage, Response = ()> + Clone + 'static,
    S::Error: Into<MiddlewareError>,
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: InboundMessage) -> Self::Future {
        Self::deserialize(self.next_service.clone(), msg)
    }
}

impl<S> DhtDeserializeMiddleware<S>
where
    S: Service<DhtInboundMessage, Response = ()>,
    S::Error: Into<MiddlewareError>,
{
    pub async fn deserialize(mut next_service: S, message: InboundMessage) -> Result<(), MiddlewareError> {
        trace!(target: LOG_TARGET, "Deserializing InboundMessage");
        next_service.ready().await.map_err(Into::into)?;

        let InboundMessage {
            source_peer,
            envelope_header,
            body,
            ..
        } = message;

        match DhtEnvelope::from_binary(&body) {
            Ok(dht_envelope) => {
                trace!(target: LOG_TARGET, "Deserialization succeeded. Checking signatures");
                if !dht_envelope.is_signature_valid() {
                    // The origin signature is not valid, this message should never have been sent
                    warn!(
                        target: LOG_TARGET,
                        "SECURITY: Origin signature verification failed. Discarding message from NodeId {}",
                        source_peer.node_id
                    );
                    return Ok(());
                }

                trace!(target: LOG_TARGET, "Origin signature validation passed.");

                let inbound_msg =
                    DhtInboundMessage::new(dht_envelope.header, source_peer, envelope_header, dht_envelope.body);
                next_service.call(inbound_msg).await.map_err(Into::into)
            },
            Err(err) => {
                error!(target: LOG_TARGET, "DHT deserialization failed: {}", err);
                Err(err.into())
            },
        }
    }
}

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
    use tari_comms::message::MessageFlags;
    use tari_test_utils::panic_context;

    #[test]
    fn deserialize() {
        let spy = service_spy();
        let mut deserialize = DeserializeLayer::new().layer(spy.service::<MiddlewareError>());

        panic_context!(cx);

        assert!(deserialize.poll_ready(&mut cx).is_ready());
        let node_identity = make_node_identity();
        let dht_envelope = make_dht_envelope(&node_identity, b"A".to_vec(), DhtMessageFlags::empty());
        block_on(deserialize.call(make_comms_inbound_message(
            &node_identity,
            dht_envelope.to_binary().unwrap(),
            MessageFlags::empty(),
        )))
        .unwrap();

        let msg = spy.pop_request().unwrap();
        assert_eq!(msg.body, b"A".to_vec());
        assert_eq!(msg.dht_header, dht_envelope.header);
    }
}

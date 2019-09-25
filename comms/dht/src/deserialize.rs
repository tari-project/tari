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

use crate::messages::{DhtEnvelope, DhtInboundMessage};
use futures::{task::Context, Future, Poll};
use std::pin::Pin;
use tari_comms::message::InboundMessage;
use tari_utilities::message_format::{MessageFormat, MessageFormatError};
use tower::{layer::Layer, Service};
use tracing::{error, metadata::Level, span, trace, warn, Span};

pub struct DhtDeserialize<S> {
    inner: S,
    span: tracing::Span,
}

impl<S> DhtDeserialize<S> {
    pub fn new(service: S) -> Self {
        Self {
            inner: service,
            span: span!(Level::TRACE, "comms::dht::deserialize"),
        }
    }
}

impl<S> Service<InboundMessage> for DhtDeserialize<S>
where
    S: Service<DhtInboundMessage, Response = ()> + Clone + Unpin + 'static,
    S::Error: From<MessageFormatError>,
{
    type Error = S::Error;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_ready(cx)
    }

    fn call(&mut self, msg: InboundMessage) -> Self::Future {
        let span = self.span.clone();
        Self::deserialize(self.inner.clone(), msg, span)
    }
}

impl<S> DhtDeserialize<S>
where
    S: Service<DhtInboundMessage, Response = ()>,
    S::Error: From<MessageFormatError>,
{
    pub async fn deserialize(mut service: S, message: InboundMessage, span: Span) -> Result<(), S::Error> {
        let _enter = span.enter();
        trace!("Deserializing InboundMessage");
        match DhtEnvelope::from_binary(&message.body) {
            Ok(dht_envelope) => {
                trace!("Deserialization succeeded. Checking signatures");
                if !dht_envelope.is_signature_valid() {
                    // The origin signature is not valid, this message should never have been sent
                    warn!(
                        "SECURITY: Origin signature verification failed. Discarding message from NodeId {}",
                        message.source_peer.node_id
                    );
                    return Ok(());
                }

                trace!("Origin signature validation passed.");
                service
                    .call(DhtInboundMessage::new(
                        dht_envelope.header,
                        message.source_peer,
                        message.envelope_header,
                        dht_envelope.body,
                    ))
                    .await
            },
            Err(err) => {
                error!("DHT deserialization failed: {}", err);
                Err(err.into())
            },
        }
    }
}

pub struct DhtDeserializeLayer;

impl DhtDeserializeLayer {
    pub fn new() -> Self {
        DhtDeserializeLayer
    }
}

impl<S> Layer<S> for DhtDeserializeLayer {
    type Service = DhtDeserialize<S>;

    fn layer(&self, service: S) -> Self::Service {
        DhtDeserialize::new(service)
    }
}

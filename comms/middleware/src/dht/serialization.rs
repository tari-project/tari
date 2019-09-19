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

use crate::dht::messages::DhtEnvelope;
use futures::{Future, Poll};
use futures_test::futures_core_reexport::task::Context;
use std::pin::Pin;
use tari_comms::{message::InboundMessage, peer_manager::error::PeerManagerError::DeserializationError};
use tari_utilities::message_format::{MessageFormat, MessageFormatError};
use tower_layer::Layer;
use tower_service::Service;

pub struct DhtDeserializedMessage {
    dht_envelope: DhtEnvelope,
    inbound_message: InboundMessage,
}

impl DhtDeserializedMessage {
    pub fn new(inbound_message: InboundMessage, dht_envelope: DhtEnvelope) -> Self {
        Self {
            inbound_message,
            dht_envelope,
        }
    }
}

pub struct DhtDeserialize<S> {
    inner: S,
}

impl<S> DhtDeserialize<S> {
    pub fn new(service: S) -> Self {
        Self { inner: service }
    }
}

impl<S> Service<InboundMessage> for DhtDeserialize<S>
where
    S: Service<DhtDeserializedMessage, Response = ()> + Clone + Unpin + 'static,
    S::Error: From<MessageFormatError>,
{
    type Error = S::Error;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_ready(cx)
    }

    fn call(&mut self, msg: InboundMessage) -> Self::Future {
        Deserialize::new(self.inner.clone()).deserialize(msg)
    }
}

pub struct DhtDeserializeLayer;

impl<S> Layer<S> for DhtDeserializeLayer {
    type Service = DhtDeserialize<S>;

    fn layer(&self, service: S) -> Self::Service {
        DhtDeserialize::new(service)
    }
}

struct Deserialize<S> {
    inner: S,
}

impl<S> Deserialize<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S> Deserialize<S>
where
    S: Service<DhtDeserializedMessage, Response = ()>,
    S::Error: From<MessageFormatError>,
{
    pub async fn deserialize(mut self, message: InboundMessage) -> Result<(), S::Error> {
        match DhtEnvelope::from_binary(&message.body) {
            Ok(dht_envelope) => {
                self.inner
                    .call(DhtDeserializedMessage::new(message, dht_envelope))
                    .await
            },
            Err(err) => Err(err.into()),
        }
    }
}

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
    inbound::message::{DecryptedDhtMessage, DhtInboundMessage},
    message::DhtMessageFlags,
};
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::{message::Message, peer_manager::NodeIdentity, utils::crypt};
use tari_comms_middleware::MiddlewareError;
use tari_utilities::message_format::MessageFormat;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::middleware::encryption";

/// This layer is responsible for attempting to decrypt inbound messages.
pub struct DecryptionLayer {
    node_identity: Arc<NodeIdentity>,
}

impl DecryptionLayer {
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        Self { node_identity }
    }
}

impl<S> Layer<S> for DecryptionLayer {
    type Service = DecryptionService<S>;

    fn layer(&self, service: S) -> Self::Service {
        DecryptionService::new(service, Arc::clone(&self.node_identity))
    }
}

/// Responsible for decrypting InboundMessages and passing a DecryptedInboundMessage to the given service
#[derive(Clone)]
pub struct DecryptionService<S> {
    node_identity: Arc<NodeIdentity>,
    inner: S,
}

impl<S> DecryptionService<S> {
    pub fn new(service: S, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            inner: service,
            node_identity,
        }
    }
}

impl<S> Service<DhtInboundMessage> for DecryptionService<S>
where
    S: Service<DecryptedDhtMessage, Response = ()> + Clone,
    S::Error: Into<MiddlewareError>,
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtInboundMessage) -> Self::Future {
        Self::handle_message(self.inner.clone(), Arc::clone(&self.node_identity), msg)
    }
}

impl<S> DecryptionService<S>
where
    S: Service<DecryptedDhtMessage, Response = ()>,
    S::Error: Into<MiddlewareError>,
{
    async fn handle_message(
        next_service: S,
        node_identity: Arc<NodeIdentity>,
        message: DhtInboundMessage,
    ) -> Result<(), MiddlewareError>
    {
        let dht_header = &message.dht_header;
        if !dht_header.flags.contains(DhtMessageFlags::ENCRYPTED) {
            return Self::success_not_encrypted(next_service, message).await;
        }

        debug!(target: LOG_TARGET, "Attempting to decrypt message");
        let shared_secret = crypt::generate_ecdh_secret(&node_identity.secret_key, &dht_header.origin_public_key);
        match crypt::decrypt(&shared_secret, &message.body) {
            Ok(decrypted) => Self::decryption_succeeded(next_service, message, decrypted).await,
            Err(err) => {
                debug!(target: LOG_TARGET, "Unable to decrypt message: {}", err);
                Self::decryption_failed(next_service, message).await
            },
        }
    }

    async fn decryption_succeeded(
        mut next_service: S,
        message: DhtInboundMessage,
        decrypted: Vec<u8>,
    ) -> Result<(), MiddlewareError>
    {
        next_service.ready().await.map_err(Into::into)?;
        // This `Message` was created in the OutboundMessageRequester. Deserialization is done here
        // to determine if the decryption produced valid bytes or not.
        match Message::from_binary(&decrypted) {
            Ok(deserialized) => {
                debug!(target: LOG_TARGET, "Message successfully decrypted");
                let msg = DecryptedDhtMessage::succeed(deserialized, message);
                next_service.call(msg).await.map_err(Into::into)
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Unable to deserialize message: {}", err);
                Self::decryption_failed(next_service, message).await
            },
        }
    }

    async fn success_not_encrypted(mut next_service: S, message: DhtInboundMessage) -> Result<(), MiddlewareError> {
        match Message::from_binary(&message.body) {
            Ok(deserialized) => {
                debug!(target: LOG_TARGET, "Message successfully decrypted");
                let msg = DecryptedDhtMessage::succeed(deserialized, message);
                next_service.ready().await.map_err(Into::into)?;
                next_service.call(msg).await.map_err(Into::into)
            },
            Err(err) => {
                debug!(target: LOG_TARGET, "Unable to deserialize message: {}", err);
                Self::decryption_failed(next_service, message).await
            },
        }
    }

    async fn decryption_failed(mut next_service: S, message: DhtInboundMessage) -> Result<(), MiddlewareError> {
        let msg = DecryptedDhtMessage::fail(message);
        next_service.ready().await.map_err(Into::into)?;
        next_service.call(msg).await.map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        message::DhtMessageFlags,
        test_utils::{make_dht_inbound_message, make_node_identity, service_fn},
    };
    use futures::{executor::block_on, future};
    use std::sync::Mutex;
    use tari_test_utils::counter_context;

    #[test]
    fn poll_ready() {
        let inner = service_fn(|_: DecryptedDhtMessage| future::ready(Result::<(), MiddlewareError>::Ok(())));
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(inner, node_identity);

        counter_context!(cx, counter);

        assert!(service.poll_ready(&mut cx).is_ready());

        assert_eq!(counter.get(), 0);
    }

    #[test]
    fn decrypt_inbound_success() {
        let result = Mutex::new(None);
        let inner = service_fn(|msg: DecryptedDhtMessage| {
            *result.lock().unwrap() = Some(msg);
            future::ready(Result::<(), MiddlewareError>::Ok(()))
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(inner, Arc::clone(&node_identity));

        let plain_text_msg = Message::from_message_format((), ()).unwrap();
        let secret_key = crypt::generate_ecdh_secret(&node_identity.secret_key, &node_identity.identity.public_key);
        let encrypted = crypt::encrypt(&secret_key, &plain_text_msg.to_binary().unwrap()).unwrap();
        let inbound_msg = make_dht_inbound_message(&node_identity, encrypted, DhtMessageFlags::ENCRYPTED);

        block_on(service.call(inbound_msg)).unwrap();
        let decrypted = result.lock().unwrap().take().unwrap();
        assert_eq!(decrypted.decryption_succeeded(), true);
        assert_eq!(decrypted.decryption_result.unwrap(), plain_text_msg);
    }

    #[test]
    fn decrypt_inbound_fail() {
        let result = Mutex::new(None);
        let inner = service_fn(|msg: DecryptedDhtMessage| {
            *result.lock().unwrap() = Some(msg);
            future::ready(Result::<(), MiddlewareError>::Ok(()))
        });
        let node_identity = make_node_identity();
        let mut service = DecryptionService::new(inner, Arc::clone(&node_identity));

        let nonsense = "Cannot Decrypt this".as_bytes().to_vec();
        let inbound_msg = make_dht_inbound_message(&node_identity, nonsense.clone(), DhtMessageFlags::ENCRYPTED);

        block_on(service.call(inbound_msg)).unwrap();
        let decrypted = result.lock().unwrap().take().unwrap();
        assert_eq!(decrypted.decryption_succeeded(), false);
        assert_eq!(decrypted.decryption_result.unwrap_err(), nonsense);
    }
}

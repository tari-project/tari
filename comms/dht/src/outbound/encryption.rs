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

use crate::outbound::message::{DhtOutboundMessage, OutboundEncryption};
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::{peer_manager::NodeIdentity, utils::crypt};
use tari_comms_middleware::MiddlewareError;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::middleware::encryption";

/// This layer is responsible for attempting to decrypt inbound messages.
pub struct EncryptionLayer {
    node_identity: Arc<NodeIdentity>,
}

impl EncryptionLayer {
    pub fn new(node_identity: Arc<NodeIdentity>) -> Self {
        Self { node_identity }
    }
}

impl<S> Layer<S> for EncryptionLayer {
    type Service = EncryptionService<S>;

    fn layer(&self, service: S) -> Self::Service {
        EncryptionService::new(service, Arc::clone(&self.node_identity))
    }
}

/// Responsible for decrypting InboundMessages and passing a DecryptedInboundMessage to the given service
#[derive(Clone)]
pub struct EncryptionService<S> {
    node_identity: Arc<NodeIdentity>,
    inner: S,
}

impl<S> EncryptionService<S> {
    pub fn new(service: S, node_identity: Arc<NodeIdentity>) -> Self {
        Self {
            inner: service,
            node_identity,
        }
    }
}

impl<S> Service<DhtOutboundMessage> for EncryptionService<S>
where
    S: Service<DhtOutboundMessage, Response = ()> + Clone,
    S::Error: Into<MiddlewareError>,
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DhtOutboundMessage) -> Self::Future {
        Self::handle_message(self.inner.clone(), Arc::clone(&self.node_identity), msg)
    }
}

impl<S> EncryptionService<S>
where
    S: Service<DhtOutboundMessage, Response = ()>,
    S::Error: Into<MiddlewareError>,
{
    async fn handle_message(
        mut next_service: S,
        node_identity: Arc<NodeIdentity>,
        mut message: DhtOutboundMessage,
    ) -> Result<(), MiddlewareError>
    {
        trace!(target: LOG_TARGET, "DHT Message flags: {:?}", message.dht_header.flags);
        match &message.encryption {
            OutboundEncryption::EncryptFor(public_key) => {
                debug!(target: LOG_TARGET, "Encrypting message for {}", public_key);
                let shared_secret = crypt::generate_ecdh_secret(node_identity.secret_key(), public_key);
                message.body = crypt::encrypt(&shared_secret, &message.body)?;
            },
            OutboundEncryption::EncryptForDestination => {
                debug!(
                    target: LOG_TARGET,
                    "Encrypting message for peer with public key {}", message.destination_peer.public_key
                );
                let shared_secret =
                    crypt::generate_ecdh_secret(node_identity.secret_key(), &message.destination_peer.public_key);
                message.body = crypt::encrypt(&shared_secret, &message.body)?
            },
            OutboundEncryption::None => {
                debug!(target: LOG_TARGET, "Encryption not set for message",);
            },
        };

        next_service.ready().await.map_err(Into::into)?;
        next_service.call(message).await.map_err(Into::into)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{make_dht_header, make_node_identity, service_spy},
    };
    use futures::executor::block_on;
    use tari_comms::{
        connection::NetAddressesWithStats,
        message::MessageFlags,
        peer_manager::{NodeId, Peer, PeerFeatures, PeerFlags},
        types::CommsPublicKey,
    };
    use tari_test_utils::panic_context;

    #[test]
    fn no_encryption() {
        let spy = service_spy();
        let node_identity = make_node_identity();
        let mut encryption = EncryptionLayer::new(Arc::clone(&node_identity)).layer(spy.service::<MiddlewareError>());

        panic_context!(cx);
        assert!(encryption.poll_ready(&mut cx).is_ready());

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
            body.clone(),
        );
        block_on(encryption.call(msg)).unwrap();

        let msg = spy.pop_request().unwrap();
        assert_eq!(msg.body, body);
        assert_eq!(msg.destination_peer.node_id, NodeId::default());
    }

    #[test]
    fn encryption() {
        let spy = service_spy();
        let node_identity = make_node_identity();
        let mut encryption = EncryptionLayer::new(Arc::clone(&node_identity)).layer(spy.service::<MiddlewareError>());

        panic_context!(cx);
        assert!(encryption.poll_ready(&mut cx).is_ready());

        let body = b"A".to_vec();
        let msg = DhtOutboundMessage::new(
            Peer::new(
                CommsPublicKey::default(),
                NodeId::default(),
                NetAddressesWithStats::new(vec![]),
                PeerFlags::empty(),
                PeerFeatures::COMMUNICATION_NODE,
            ),
            make_dht_header(&node_identity, &body, DhtMessageFlags::ENCRYPTED),
            OutboundEncryption::EncryptForDestination,
            MessageFlags::empty(),
            body.clone(),
        );
        block_on(encryption.call(msg)).unwrap();

        let msg = spy.pop_request().unwrap();
        assert_ne!(msg.body, body);
        assert_eq!(msg.destination_peer.node_id, NodeId::default());
    }
}

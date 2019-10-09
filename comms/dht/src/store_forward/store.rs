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
    envelope::NodeDestination,
    inbound::DecryptedDhtMessage,
    store_forward::{error::StoreAndForwardError, message::StoredMessage, state::SAFStorage},
    DhtConfig,
};
use futures::{task::Context, Future, Poll};
use log::*;
use std::sync::Arc;
use tari_comms::peer_manager::{NodeIdentity, PeerManager};
use tari_comms_middleware::MiddlewareError;
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &'static str = "comms::middleware::forward";

/// This layer is responsible for storing messages which have failed to decrypt
pub struct StoreLayer {
    peer_manager: Arc<PeerManager>,
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    storage: Arc<SAFStorage>,
}

impl StoreLayer {
    pub fn new(
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        storage: Arc<SAFStorage>,
    ) -> Self
    {
        Self {
            peer_manager,
            config,
            node_identity,
            storage,
        }
    }
}

impl<S> Layer<S> for StoreLayer {
    type Service = StoreMiddleware<S>;

    fn layer(&self, service: S) -> Self::Service {
        StoreMiddleware::new(
            service,
            self.config.clone(),
            Arc::clone(&self.peer_manager),
            Arc::clone(&self.node_identity),
            Arc::clone(&self.storage),
        )
    }
}

#[derive(Clone)]
pub struct StoreMiddleware<S> {
    next_service: S,
    config: DhtConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,

    storage: Arc<SAFStorage>,
}

impl<S> StoreMiddleware<S> {
    pub fn new(
        next_service: S,
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        storage: Arc<SAFStorage>,
    ) -> Self
    {
        Self {
            next_service,
            config,
            peer_manager,
            node_identity,
            storage,
        }
    }
}

impl<S> Service<DecryptedDhtMessage> for StoreMiddleware<S>
where
    S: Service<DecryptedDhtMessage, Response = ()> + Clone + 'static,
    S::Error: Into<MiddlewareError>,
{
    type Error = MiddlewareError;
    type Response = ();

    type Future = impl Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DecryptedDhtMessage) -> Self::Future {
        StoreTask::new(
            self.next_service.clone(),
            self.config.clone(),
            Arc::clone(&self.peer_manager),
            Arc::clone(&self.node_identity),
            Arc::clone(&self.storage),
        )
        .handle(msg)
    }
}

/// Responsible for processing a single DecryptedDhtMessage, storing if necessary or passing the message
/// to the next service.
struct StoreTask<S> {
    peer_manager: Arc<PeerManager>,
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    next_service: S,
    storage: Arc<SAFStorage>,
}

impl<S> StoreTask<S> {
    pub fn new(
        next_service: S,
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        storage: Arc<SAFStorage>,
    ) -> Self
    {
        Self {
            config,
            peer_manager,
            node_identity,
            next_service,
            storage,
        }
    }
}

impl<S> StoreTask<S>
where
    S: Service<DecryptedDhtMessage, Response = ()>,
    S::Error: Into<MiddlewareError>,
{
    async fn handle(mut self, message: DecryptedDhtMessage) -> Result<(), MiddlewareError> {
        match message.success() {
            Some(_) => {
                trace!(target: LOG_TARGET, "Passing message to next service");
                self.next_service.oneshot(message).await.map_err(Into::into)?;
            },
            None => {
                debug!(target: LOG_TARGET, "Decryption failed for message. Storing.");
                self.store(message)?;
            },
        }

        Ok(())
    }

    fn store(&mut self, message: DecryptedDhtMessage) -> Result<(), StoreAndForwardError> {
        let DecryptedDhtMessage {
            version,
            comms_header,
            decryption_result,
            dht_header,
            ..
        } = message;

        let encrypted_body = decryption_result
            .err()
            .expect("checked previously that decryption failed");

        let peer_manager = &self.peer_manager;
        let node_identity = &self.node_identity;

        match &dht_header.destination {
            NodeDestination::Undisclosed => {
                self.storage.insert(
                    dht_header.origin_signature.clone(),
                    StoredMessage::new(version, comms_header, dht_header, encrypted_body),
                    self.config.saf_low_priority_msg_storage_ttl,
                );
            },
            NodeDestination::PublicKey(dest_public_key) => {
                if peer_manager.exists(&dest_public_key)? {
                    self.storage.insert(
                        dht_header.origin_signature.clone(),
                        StoredMessage::new(version, comms_header, dht_header, encrypted_body),
                        self.config.saf_high_priority_msg_storage_ttl,
                    );
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                if (peer_manager.exists_node_id(&dest_node_id)?) |
                    (peer_manager.in_network_region(
                        &dest_node_id,
                        &node_identity.identity.node_id,
                        self.config.num_regional_nodes,
                    )?)
                {
                    self.storage.insert(
                        dht_header.origin_signature.clone(),
                        StoredMessage::new(version, comms_header, dht_header, encrypted_body),
                        self.config.saf_high_priority_msg_storage_ttl,
                    );
                }
            },
        };

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        test_utils::{make_dht_inbound_message, make_node_identity, make_peer_manager, service_spy},
    };
    use chrono::Utc;
    use futures::executor::block_on;
    use tari_comms::message::Message;

    #[test]
    fn decryption_succeeded() {
        let storage = Arc::new(SAFStorage::new(1));

        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, storage)
            .layer(spy.service::<MiddlewareError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        let msg = DecryptedDhtMessage::succeeded(Message::from_message_format((), ()).unwrap(), inbound_msg);
        block_on(service.call(msg)).unwrap();
        assert!(spy.is_called());
    }

    #[test]
    fn decryption_failed() {
        let storage = Arc::new(SAFStorage::new(1));
        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, Arc::clone(&storage))
            .layer(spy.service::<MiddlewareError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        let msg = DecryptedDhtMessage::failed(inbound_msg.clone());
        block_on(service.call(msg)).unwrap();
        assert_eq!(spy.is_called(), false);
        let msg = storage.remove(&inbound_msg.dht_header.origin_signature).unwrap();
        assert!((Utc::now() - msg.stored_at).num_seconds() <= 5);
    }
}

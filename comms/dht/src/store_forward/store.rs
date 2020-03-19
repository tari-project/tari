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
    envelope::{DhtMessageFlags, NodeDestination},
    inbound::DecryptedDhtMessage,
    proto::store_forward::StoredMessage,
    store_forward::{error::StoreAndForwardError, state::SafStorage},
    DhtConfig,
};
use futures::{task::Context, Future};
use log::*;
use std::{sync::Arc, task::Poll};
use tari_comms::{
    message::MessageExt,
    peer_manager::{NodeIdentity, PeerManager},
    pipeline::PipelineError,
};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::middleware::forward";

/// This layer is responsible for storing messages which have failed to decrypt
pub struct StoreLayer {
    peer_manager: Arc<PeerManager>,
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    storage: Arc<SafStorage>,
}

impl StoreLayer {
    pub fn new(
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        storage: Arc<SafStorage>,
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

    storage: Arc<SafStorage>,
}

impl<S> StoreMiddleware<S> {
    pub fn new(
        next_service: S,
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        storage: Arc<SafStorage>,
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
    S::Error: std::error::Error + Send + Sync + 'static,
{
    type Error = PipelineError;
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
    next_service: S,
    storage: Option<InnerStorage>,
}

impl<S> StoreTask<S> {
    pub fn new(
        next_service: S,
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        storage: Arc<SafStorage>,
    ) -> Self
    {
        Self {
            storage: Some(InnerStorage {
                config,
                peer_manager,
                node_identity,
                storage,
            }),
            next_service,
        }
    }
}

impl<S> StoreTask<S>
where
    S: Service<DecryptedDhtMessage, Response = ()>,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    async fn handle(mut self, message: DecryptedDhtMessage) -> Result<(), PipelineError> {
        match message.success() {
            Some(_) => {
                // If message was not originally encrypted and has an origin we want to store a copy for others
                if message.dht_header.origin.is_some() && !message.dht_header.flags.contains(DhtMessageFlags::ENCRYPTED)
                {
                    debug!(
                        target: LOG_TARGET,
                        "Cleartext message sent from origin {}. Adding to SAF storage.",
                        message.origin_public_key()
                    );
                    let mut storage = self.storage.take().expect("StoreTask intialized without storage");
                    let msg_clone = message.clone();
                    storage.store(msg_clone).await.map_err(PipelineError::from_debug)?;
                }

                trace!(target: LOG_TARGET, "Passing message to next service");
                self.next_service
                    .oneshot(message)
                    .await
                    .map_err(PipelineError::from_debug)?;
            },
            None => {
                if message.dht_header.origin.is_none() {
                    // TODO: #banheuristic
                    warn!(
                        target: LOG_TARGET,
                        "Store task received an encrypted message with no source. This message is invalid and should \
                         not be stored or propagated. Dropping message. Sent by node '{}'",
                        message.source_peer.node_id.short_str()
                    );
                    return Ok(());
                }
                debug!(
                    target: LOG_TARGET,
                    "Decryption failed for message. Adding to SAF storage."
                );
                let mut storage = self.storage.take().expect("StoreTask intialized without storage");
                storage.store(message).await.map_err(PipelineError::from_debug)?;
            },
        }

        Ok(())
    }
}

struct InnerStorage {
    peer_manager: Arc<PeerManager>,
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    storage: Arc<SafStorage>,
}

impl InnerStorage {
    async fn store(&mut self, message: DecryptedDhtMessage) -> Result<(), StoreAndForwardError> {
        let DecryptedDhtMessage {
            version,
            decryption_result,
            dht_header,
            ..
        } = message;

        let origin = dht_header.origin.as_ref().expect("already checked");

        let body = match decryption_result {
            Ok(body) => body.to_encoded_bytes()?,
            Err(encrypted_body) => encrypted_body,
        };

        let peer_manager = &self.peer_manager;
        let node_identity = &self.node_identity;

        match &dht_header.destination {
            NodeDestination::Unknown => {
                self.storage.insert(
                    origin.signature.clone(),
                    StoredMessage::new(version, dht_header, body),
                    self.config.saf_low_priority_msg_storage_ttl,
                );
            },
            NodeDestination::PublicKey(dest_public_key) => {
                if peer_manager.exists(&dest_public_key).await {
                    self.storage.insert(
                        origin.signature.clone(),
                        StoredMessage::new(version, dht_header, body),
                        self.config.saf_high_priority_msg_storage_ttl,
                    );
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                if peer_manager.exists_node_id(&dest_node_id).await ||
                    peer_manager
                        .in_network_region(
                            &dest_node_id,
                            node_identity.node_id(),
                            self.config.num_neighbouring_nodes,
                        )
                        .await?
                {
                    self.storage.insert(
                        origin.signature.clone(),
                        StoredMessage::new(version, dht_header, body),
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
    use chrono::{DateTime, Utc};
    use std::time::{Duration, UNIX_EPOCH};
    use tari_comms::wrap_in_envelope_body;

    #[tokio_macros::test_basic]
    async fn cleartext_message_no_origin() {
        let storage = Arc::new(SafStorage::new(1));

        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, storage.clone())
            .layer(spy.to_service::<PipelineError>());

        let mut inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        inbound_msg.dht_header.origin = None;
        let msg = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(Vec::new()).unwrap(), inbound_msg);
        service.call(msg).await.unwrap();
        assert!(spy.is_called());
        storage.with_lock(|mut lock| {
            assert_eq!(lock.iter().count(), 0);
        });
    }

    #[tokio_macros::test_basic]
    async fn cleartext_message_with_origin() {
        let storage = Arc::new(SafStorage::new(1));

        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, storage.clone())
            .layer(spy.to_service::<PipelineError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        let msg = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(Vec::new()).unwrap(), inbound_msg);
        service.call(msg).await.unwrap();
        assert!(spy.is_called());
        storage.with_lock(|mut lock| {
            assert_eq!(lock.iter().count(), 1);
        });
    }

    #[tokio_macros::test_basic]
    async fn decryption_succeeded_no_store() {
        let storage = Arc::new(SafStorage::new(1));

        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, storage.clone())
            .layer(spy.to_service::<PipelineError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::ENCRYPTED);
        let msg = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(b"secret".to_vec()).unwrap(), inbound_msg);
        service.call(msg).await.unwrap();
        assert!(spy.is_called());
        storage.with_lock(|mut lock| {
            assert_eq!(lock.iter().count(), 0);
        });
    }

    #[tokio_macros::test_basic]
    async fn decryption_failed_should_store() {
        let storage = Arc::new(SafStorage::new(1));
        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, Arc::clone(&storage))
            .layer(spy.to_service::<PipelineError>());

        let inbound_msg = make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty());
        let msg = DecryptedDhtMessage::failed(inbound_msg.clone());
        service.call(msg).await.unwrap();
        assert_eq!(spy.is_called(), false);
        let msg = storage
            .remove(&inbound_msg.dht_header.origin.unwrap().signature)
            .unwrap();
        let timestamp: DateTime<Utc> = (UNIX_EPOCH + Duration::from_secs(msg.stored_at.unwrap().seconds as u64)).into();
        assert!((Utc::now() - timestamp).num_seconds() <= 5);
    }
}

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

use std::{sync::Arc, task::Poll};

use futures::{future::BoxFuture, task::Context};
use log::*;
use tari_comms::{
    peer_manager::{NodeIdentity, PeerFeatures, PeerManager},
    pipeline::PipelineError,
};
use tari_utilities::epoch_time::EpochTime;
use tower::{layer::Layer, Service, ServiceExt};

use super::StoreAndForwardRequester;
use crate::{
    inbound::DecryptedDhtMessage,
    store_forward::{database::NewStoredMessage, message::StoredMessagePriority, SafConfig, SafResult},
};

const LOG_TARGET: &str = "comms::dht::storeforward::store";

/// This layer is responsible for storing messages which have failed to decrypt
pub struct StoreLayer {
    peer_manager: Arc<PeerManager>,
    config: SafConfig,
    node_identity: Arc<NodeIdentity>,
    saf_requester: StoreAndForwardRequester,
}

impl StoreLayer {
    /// New store layer.
    pub fn new(
        config: SafConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        saf_requester: StoreAndForwardRequester,
    ) -> Self {
        Self {
            peer_manager,
            config,
            node_identity,
            saf_requester,
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
            self.saf_requester.clone(),
        )
    }
}

#[derive(Clone)]
pub struct StoreMiddleware<S> {
    next_service: S,
    config: SafConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    saf_requester: StoreAndForwardRequester,
}

impl<S> StoreMiddleware<S> {
    pub fn new(
        next_service: S,
        config: SafConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        saf_requester: StoreAndForwardRequester,
    ) -> Self {
        Self {
            next_service,
            config,
            peer_manager,
            node_identity,
            saf_requester,
        }
    }
}

impl<S> Service<DecryptedDhtMessage> for StoreMiddleware<S>
where
    S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + Send + Sync + 'static,
    S::Future: Send,
{
    type Error = PipelineError;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    type Response = ();

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, msg: DecryptedDhtMessage) -> Self::Future {
        if msg.is_duplicate() {
            trace!(
                target: LOG_TARGET,
                "Passing duplicate message {} to next service (Trace: {})",
                msg.tag,
                msg.dht_header.message_tag
            );

            let service = self.next_service.clone();
            Box::pin(async move {
                let service = service.ready_oneshot().await?;
                service.oneshot(msg).await
            })
        } else {
            Box::pin(
                StoreTask::new(
                    self.next_service.clone(),
                    self.config.clone(),
                    Arc::clone(&self.peer_manager),
                    Arc::clone(&self.node_identity),
                    self.saf_requester.clone(),
                )
                .handle(msg),
            )
        }
    }
}

/// Responsible for processing a single DecryptedDhtMessage, storing if necessary or passing the message
/// to the next service.
struct StoreTask<S> {
    next_service: S,
    peer_manager: Arc<PeerManager>,
    config: SafConfig,
    node_identity: Arc<NodeIdentity>,
    saf_requester: StoreAndForwardRequester,
}

impl<S> StoreTask<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Send + Sync
{
    pub fn new(
        next_service: S,
        config: SafConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        saf_requester: StoreAndForwardRequester,
    ) -> Self {
        Self {
            next_service,

            peer_manager,
            config,
            node_identity,
            saf_requester,
        }
    }

    /// Determine if this is a message we should store for our peers and, if so, store it.
    ///
    /// The criteria for storing a message is:
    /// 1. Messages MUST have a message origin set and be encrypted (Join messages are the exception)
    /// 1. Unencrypted Join messages - this increases the knowledge the network has of peers (Low priority)
    /// 1. Encrypted Discovery messages - so that nodes are aware of other nodes that are looking for them (High
    /// priority) 1. Encrypted messages addressed to the neighbourhood - some node in the neighbourhood may be
    /// interested in this message (High priority) 1. Encrypted messages addressed to a particular public key or
    /// node id that this node knows about
    async fn handle(mut self, mut message: DecryptedDhtMessage) -> Result<(), PipelineError> {
        if !self.node_identity.features().contains(PeerFeatures::DHT_STORE_FORWARD) {
            trace!(
                target: LOG_TARGET,
                "Passing message {} to next service (Not a SAF node) (Trace: {})",
                message.tag,
                message.dht_header.message_tag
            );
            let service = self.next_service.ready_oneshot().await?;
            return service.oneshot(message).await;
        }

        message.set_saf_stored(false);
        if self.is_valid_for_storage(&message) {
            if let Some(priority) = self.get_storage_priority(&message).await? {
                message.set_saf_stored(true);
                let existing = self.store(priority, message.clone()).await?;
                message.set_already_forwarded(existing);
            }
        }

        trace!(
            target: LOG_TARGET,
            "Passing message {} to next service (Trace: {})",
            message.tag,
            message.dht_header.message_tag
        );

        let service = self.next_service.ready_oneshot().await?;
        service.oneshot(message).await
    }

    fn is_valid_for_storage(&self, message: &DecryptedDhtMessage) -> bool {
        if message.body_len() == 0 {
            debug!(
                target: LOG_TARGET,
                "Message {} from peer '{}' not eligible for SAF storage because it has no body (Trace: {})",
                message.tag,
                message.source_peer.node_id.short_str(),
                message.dht_header.message_tag
            );
            return false;
        }

        if let Some(expires) = message.dht_header.expires {
            let now = EpochTime::now();
            if expires < now {
                debug!(
                    target: LOG_TARGET,
                    "Message {} from peer '{}' not eligible for SAF storage because it has expired (Trace: {})",
                    message.tag,
                    message.source_peer.node_id.short_str(),
                    message.dht_header.message_tag
                );
                return false;
            }
        }

        true
    }

    async fn get_storage_priority(&self, message: &DecryptedDhtMessage) -> SafResult<Option<StoredMessagePriority>> {
        let log_not_eligible = |reason: &str| {
            debug!(
                target: LOG_TARGET,
                "Message {} from peer '{}' not eligible for SAF storage because {} (Trace: {})",
                message.tag,
                message.source_peer.node_id.short_str(),
                reason,
                message.dht_header.message_tag
            );
        };

        if message.body_len() > self.config.max_message_size {
            log_not_eligible(&format!(
                "the message body exceeded the maximum storage size (body size={}, max={})",
                message.body_len(),
                self.config.max_message_size
            ));
            return Ok(None);
        }

        if message.dht_header.message_type.is_saf_message() {
            log_not_eligible("it is a SAF protocol message");
            return Ok(None);
        }

        if message.dht_header.message_type.is_dht_message() {
            log_not_eligible(&format!("it is a DHT {} message", message.dht_header.message_type));
            return Ok(None);
        }

        if message
            .authenticated_origin()
            .map(|pk| pk == self.node_identity.public_key())
            .unwrap_or(false)
        {
            log_not_eligible("this message originates from this node");
            return Ok(None);
        }

        match message.success() {
            // The message decryption was successful, or the message was not encrypted
            Some(_) => {
                // If the message doesnt have an origin we wont store it
                if !message.has_message_signature() {
                    log_not_eligible("it is a cleartext message and does not have an message signature");
                    return Ok(None);
                }

                // If this node decrypted the message (message.success() above), no need to store it
                if message.is_encrypted() {
                    log_not_eligible("the message was encrypted for this node");
                    return Ok(None);
                }

                log_not_eligible("it is not an eligible DhtMessageType");
                // Otherwise, don't store
                Ok(None)
            },
            // This node could not decrypt the message
            None => {
                if !message.has_message_signature() {
                    // #banheuristic - the source peer should not have propagated this message
                    debug!(
                        target: LOG_TARGET,
                        "Store task received an encrypted message with no message signature. This message {} is \
                         invalid and should not be stored or propagated. Dropping message. Sent by node '{}' (Trace: \
                         {})",
                        message.tag,
                        message.source_peer.node_id.short_str(),
                        message.dht_header.message_tag
                    );
                    return Ok(None);
                }

                // The destination of the message will determine if we store it
                self.get_priority_by_destination(message).await
            },
        }
    }

    async fn get_priority_by_destination(
        &self,
        message: &DecryptedDhtMessage,
    ) -> SafResult<Option<StoredMessagePriority>> {
        let log_not_eligible = |reason: &str| {
            debug!(
                target: LOG_TARGET,
                "Message {} from peer '{}' not eligible for SAF storage because {} (Trace: {})",
                message.tag,
                message.source_peer.node_id.short_str(),
                message.dht_header.message_tag,
                reason
            );
        };

        let peer_manager = &self.peer_manager;
        let node_identity = &self.node_identity;

        if message.dht_header.destination == node_identity.public_key() {
            log_not_eligible("the message is destined for this node");
            return Ok(None);
        }

        if let Some(origin_pk) = message.authenticated_origin() {
            if let Ok(Some(peer)) = self.peer_manager.find_by_public_key(origin_pk).await {
                if peer.is_banned() {
                    log_not_eligible("source peer is banned by this node");
                    return Ok(None);
                }
            }
        }

        match message.dht_header.destination.to_derived_node_id() {
            // No destination provided,
            None => {
                if message.dht_header.message_type.is_dht_discovery() {
                    log_not_eligible("it is an anonymous discovery message");
                    Ok(None)
                } else {
                    Ok(Some(StoredMessagePriority::Low))
                }
            },
            Some(dest_node_id) => {
                if !peer_manager
                    .in_network_region(
                        &dest_node_id,
                        node_identity.node_id(),
                        self.config.num_neighbouring_nodes,
                    )
                    .await?
                {
                    return Ok(None);
                }

                match peer_manager.find_by_node_id(&dest_node_id).await {
                    Ok(Some(peer)) if peer.is_banned() => {
                        log_not_eligible("destination peer is banned.");
                        Ok(None)
                    },
                    // We know the peer, they aren't banned and they are in our network region, keep the message for
                    // them
                    Ok(_) => Ok(Some(StoredMessagePriority::High)),
                    // We don't know this peer, let's keep the message for a short while (default: 6 hours) because they
                    // are in our neighbourhood.
                    Err(err) if err.is_peer_not_found() => Ok(Some(StoredMessagePriority::Low)),
                    Err(err) => Err(err.into()),
                }
            },
        }
    }

    async fn store(&mut self, priority: StoredMessagePriority, message: DecryptedDhtMessage) -> SafResult<bool> {
        debug!(
            target: LOG_TARGET,
            "Storing message {} from peer '{}' ({} bytes) (Trace: {})",
            message.tag,
            message.source_peer.node_id.short_str(),
            message.body_len(),
            message.dht_header.message_tag,
        );

        let stored_message = NewStoredMessage::new(message, priority);
        self.saf_requester.insert_message(stored_message).await
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use chrono::Utc;
    use tari_comms::wrap_in_envelope_body;
    use tari_test_utils::async_assert_eventually;
    use tari_utilities::hex::Hex;

    use super::*;
    use crate::{
        envelope::{DhtMessageFlags, NodeDestination},
        test_utils::{
            assert_send_static_service,
            build_peer_manager,
            create_store_and_forward_mock,
            make_dht_inbound_message,
            make_node_identity,
            service_spy,
        },
    };

    #[tokio::test]
    async fn cleartext_message_no_origin() {
        let (requester, mock_state) = create_store_and_forward_mock();

        let spy = service_spy();
        let peer_manager = build_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, requester)
            .layer(spy.to_service::<PipelineError>());
        assert_send_static_service(&service);

        let inbound_msg = make_dht_inbound_message(
            &make_node_identity(),
            &b"".to_vec(),
            DhtMessageFlags::empty(),
            false,
            false,
        )
        .unwrap();
        let msg = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(Vec::new()), None, inbound_msg);
        service.call(msg).await.unwrap();
        assert!(spy.is_called());
        let messages = mock_state.get_messages().await;
        assert_eq!(messages.len(), 0);
    }

    #[tokio::test]
    async fn decryption_succeeded_no_store() {
        let (requester, mock_state) = create_store_and_forward_mock();

        let spy = service_spy();
        let peer_manager = build_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, requester)
            .layer(spy.to_service::<PipelineError>());

        let msg_node_identity = make_node_identity();
        let inbound_msg = make_dht_inbound_message(
            &msg_node_identity,
            &b"This shouldnt be stored".to_vec(),
            DhtMessageFlags::ENCRYPTED,
            true,
            false,
        )
        .unwrap();
        let msg = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(b"secret".to_vec()),
            Some(msg_node_identity.public_key().clone()),
            inbound_msg,
        );
        service.call(msg).await.unwrap();
        assert!(spy.is_called());

        assert_eq!(mock_state.call_count(), 0);
    }

    #[tokio::test]
    async fn decryption_failed_should_store() {
        let (requester, mock_state) = create_store_and_forward_mock();
        let spy = service_spy();
        let peer_manager = build_peer_manager();
        let origin_node_identity = make_node_identity();
        peer_manager.add_peer(origin_node_identity.to_peer()).await.unwrap();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, requester)
            .layer(spy.to_service::<PipelineError>());

        let mut inbound_msg = make_dht_inbound_message(
            &origin_node_identity,
            &b"Will you keep this for me?".to_vec(),
            DhtMessageFlags::ENCRYPTED,
            true,
            false,
        )
        .unwrap();
        inbound_msg.dht_header.destination =
            NodeDestination::PublicKey(Box::new(origin_node_identity.public_key().clone()));
        let msg = DecryptedDhtMessage::failed(inbound_msg.clone());
        service.call(msg).await.unwrap();
        assert!(spy.is_called());

        async_assert_eventually!(
            mock_state.call_count(),
            expect = 1,
            max_attempts = 10,
            interval = Duration::from_millis(10),
        );

        let message = mock_state.get_messages().await.remove(0);
        assert_eq!(
            message.destination_pubkey.unwrap(),
            origin_node_identity.public_key().to_hex()
        );
        let duration = Utc::now().naive_utc().signed_duration_since(message.stored_at);
        assert!(duration.num_seconds() <= 5);
    }

    #[tokio::test]
    async fn decryption_failed_banned_peer() {
        let (requester, mock_state) = create_store_and_forward_mock();
        let spy = service_spy();
        let peer_manager = build_peer_manager();
        let origin_node_identity = make_node_identity();
        let mut peer = origin_node_identity.to_peer();
        peer.ban_for(Duration::from_secs(1_000_000 /* 🧏 */), "for being evil".to_string());
        peer_manager.add_peer(peer).await.unwrap();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, requester)
            .layer(spy.to_service::<PipelineError>());

        let mut inbound_msg = make_dht_inbound_message(
            &origin_node_identity,
            &b"Will you keep this for me?".to_vec(),
            DhtMessageFlags::ENCRYPTED,
            true,
            false,
        )
        .unwrap();
        inbound_msg.dht_header.destination =
            NodeDestination::PublicKey(Box::new(origin_node_identity.public_key().clone()));
        let msg_banned = DecryptedDhtMessage::failed(inbound_msg.clone());
        service.call(msg_banned).await.unwrap();
        assert!(spy.is_called());

        assert_eq!(mock_state.call_count(), 0);
        let messages = mock_state.get_messages().await;
        assert!(messages.is_empty());
    }
}

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

use super::StoreAndForwardRequester;
use crate::{
    envelope::NodeDestination,
    inbound::DecryptedDhtMessage,
    store_forward::{
        database::NewStoredMessage,
        error::StoreAndForwardError,
        message::StoredMessagePriority,
        SafResult,
    },
    DhtConfig,
};
use futures::{task::Context, Future};
use log::*;
use std::{sync::Arc, task::Poll};
use tari_comms::{
    peer_manager::{NodeIdentity, PeerFeatures, PeerManager},
    pipeline::PipelineError,
};
use tower::{layer::Layer, Service, ServiceExt};

const LOG_TARGET: &str = "comms::dht::storeforward::store";

/// This layer is responsible for storing messages which have failed to decrypt
pub struct StoreLayer {
    peer_manager: Arc<PeerManager>,
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    saf_requester: StoreAndForwardRequester,
}

impl StoreLayer {
    pub fn new(
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        saf_requester: StoreAndForwardRequester,
    ) -> Self
    {
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
    config: DhtConfig,
    peer_manager: Arc<PeerManager>,
    node_identity: Arc<NodeIdentity>,
    saf_requester: StoreAndForwardRequester,
}

impl<S> StoreMiddleware<S> {
    pub fn new(
        next_service: S,
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        saf_requester: StoreAndForwardRequester,
    ) -> Self
    {
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
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError> + Clone + 'static
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
            self.saf_requester.clone(),
        )
        .handle(msg)
    }
}

/// Responsible for processing a single DecryptedDhtMessage, storing if necessary or passing the message
/// to the next service.
struct StoreTask<S> {
    next_service: S,
    peer_manager: Arc<PeerManager>,
    config: DhtConfig,
    node_identity: Arc<NodeIdentity>,
    saf_requester: StoreAndForwardRequester,
}

impl<S> StoreTask<S> {
    pub fn new(
        next_service: S,
        config: DhtConfig,
        peer_manager: Arc<PeerManager>,
        node_identity: Arc<NodeIdentity>,
        saf_requester: StoreAndForwardRequester,
    ) -> Self
    {
        Self {
            config,
            peer_manager,
            node_identity,
            saf_requester,
            next_service,
        }
    }
}

impl<S> StoreTask<S>
where S: Service<DecryptedDhtMessage, Response = (), Error = PipelineError>
{
    /// Determine if this is a message we should store for our peers and, if so, store it.
    ///
    /// The criteria for storing a message is:
    /// 1. Messages MUST have a message origin set and be encrypted (Join messages are the exception)
    /// 1. Unencrypted Join messages - this increases the knowledge the network has of peers (Low priority)
    /// 1. Encrypted Discovery messages - so that nodes are aware of other nodes that are looking for them (High
    /// priority) 1. Encrypted messages addressed to the neighbourhood - some node in the neighbourhood may be
    /// interested in this message (High priority) 1. Encrypted messages addressed to a particular public key or
    /// node id that this node knows about
    async fn handle(mut self, message: DecryptedDhtMessage) -> Result<(), PipelineError> {
        if !self.node_identity.features().contains(PeerFeatures::DHT_STORE_FORWARD) {
            trace!(target: LOG_TARGET, "Passing message to next service (Not a SAF node)");
            self.next_service.oneshot(message).await?;
            return Ok(());
        }

        if let Some(priority) = self
            .get_storage_priority(&message)
            .await
            .map_err(PipelineError::from_debug)?
        {
            self.store(priority, message.clone())
                .await
                .map_err(PipelineError::from_debug)?;
        }

        trace!(target: LOG_TARGET, "Passing message to next service");
        self.next_service.oneshot(message).await?;

        Ok(())
    }

    async fn get_storage_priority(&self, message: &DecryptedDhtMessage) -> SafResult<Option<StoredMessagePriority>> {
        let log_not_eligible = |reason: &str| {
            debug!(
                target: LOG_TARGET,
                "Message from peer '{}' not eligible for SAF storage because {}",
                message.source_peer.node_id.short_str(),
                reason
            );
        };

        if message.body_len() > self.config.saf_max_message_size {
            log_not_eligible(&format!(
                "the message body exceeded the maximum storage size (body size={}, max={})",
                message.body_len(),
                self.config.saf_max_message_size
            ));
            return Ok(None);
        }

        if message.dht_header.message_type.is_saf_message() {
            log_not_eligible("it is a SAF message");
            return Ok(None);
        }

        if message.dht_header.message_type.is_dht_join() {
            log_not_eligible("it is a join message");
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
                if !message.has_origin_mac() {
                    log_not_eligible("it is encrypted and does not have an origin MAC");
                    return Ok(None);
                }

                // If this node decrypted the message (message.success() above), no need to store it
                if message.is_encrypted() {
                    log_not_eligible("the message was encrypted for this node");
                    return Ok(None);
                }

                // If this is a join message, we may want to store it if it's for our neighbourhood
                // if message.dht_header.message_type.is_dht_join() {
                // return match self.get_priority_for_dht_join(message).await? {
                //     Some(priority) => Ok(Some(priority)),
                //     None => {
                //         log_not_eligible("the join message was not considered in this node's neighbourhood");
                //         Ok(None)
                //     },
                // };
                // }

                log_not_eligible("it is not an eligible DhtMessageType");
                // Otherwise, don't store
                Ok(None)
            },
            // This node could not decrypt the message
            None => {
                if !message.has_origin_mac() {
                    // TODO: #banheuristic - the source peer should not have propagated this message
                    warn!(
                        target: LOG_TARGET,
                        "Store task received an encrypted message with no origin MAC. This message is invalid and \
                         should not be stored or propagated. Dropping message. Sent by node '{}'",
                        message.source_peer.node_id.short_str()
                    );
                    return Ok(None);
                }

                // The destination of the message will determine if we store it
                self.get_priority_by_destination(message).await
            },
        }
    }

    // async fn get_priority_for_dht_join(
    //     &self,
    //     message: &DecryptedDhtMessage,
    // ) -> SafResult<Option<StoredMessagePriority>>
    // {
    //     debug_assert!(message.dht_header.message_type.is_dht_join() && !message.is_encrypted());
    //
    //     let body = message
    //         .decryption_result
    //         .as_ref()
    //         .expect("already checked that this message is not encrypted");
    //     let join_msg = body
    //         .decode_part::<JoinMessage>(0)?
    //         .ok_or_else(|| StoreAndForwardError::InvalidEnvelopeBody)?;
    //     let node_id = NodeId::from_bytes(&join_msg.node_id).map_err(StoreAndForwardError::MalformedNodeId)?;
    //
    //     // If this join request is for a peer that we'd consider to be a neighbour, store it for other neighbours
    //     if self
    //         .peer_manager
    //         .in_network_region(
    //             &node_id,
    //             self.node_identity.node_id(),
    //             self.config.num_neighbouring_nodes,
    //         )
    //         .await?
    //     {
    //         if self.saf_requester.query_messages(
    //             DhtMessageType::Join,
    //         )
    //         return Ok(Some(StoredMessagePriority::Low));
    //     }
    //
    //     Ok(None)
    // }

    async fn get_priority_by_destination(
        &self,
        message: &DecryptedDhtMessage,
    ) -> SafResult<Option<StoredMessagePriority>>
    {
        let log_not_eligible = |reason: &str| {
            debug!(
                target: LOG_TARGET,
                "Message from peer '{}' not eligible for SAF storage because {}",
                message.source_peer.node_id.short_str(),
                reason
            );
        };

        let peer_manager = &self.peer_manager;
        let node_identity = &self.node_identity;

        if message.dht_header.destination == node_identity.public_key() ||
            message.dht_header.destination == node_identity.node_id()
        {
            log_not_eligible("the message is destined for this node");
            return Ok(None);
        }

        use NodeDestination::*;
        match &message.dht_header.destination {
            Unknown => {
                // No destination provided,
                if message.dht_header.message_type.is_dht_discovery() {
                    log_not_eligible("it is an anonymous discovery message");
                    Ok(None)
                } else {
                    Ok(Some(StoredMessagePriority::Low))
                }
            },
            PublicKey(dest_public_key) => {
                // If we know the destination peer, keep the message for them
                match peer_manager.find_by_public_key(&dest_public_key).await {
                    Ok(peer) => {
                        if peer.is_banned() {
                            log_not_eligible(
                                "origin peer is banned. ** This should not happen because it should have been checked \
                                 earlier in the pipeline **",
                            );
                            Ok(None)
                        } else {
                            Ok(Some(StoredMessagePriority::High))
                        }
                    },
                    Err(err) if err.is_peer_not_found() => {
                        log_not_eligible(&format!(
                            "this node does not know the destination public key '{}'",
                            dest_public_key
                        ));
                        Ok(None)
                    },
                    Err(err) => Err(err.into()),
                }
            },
            NodeId(dest_node_id) => {
                if peer_manager.exists_node_id(&dest_node_id).await ||
                    peer_manager
                        .in_network_region(
                            &dest_node_id,
                            node_identity.node_id(),
                            self.config.num_neighbouring_nodes,
                        )
                        .await?
                {
                    Ok(Some(StoredMessagePriority::High))
                } else {
                    log_not_eligible(&format!(
                        "this node does not know the destination node id '{}' or does not consider it a neighbouring \
                         node id",
                        dest_node_id
                    ));
                    Ok(None)
                }
            },
        }
    }

    async fn store(&mut self, priority: StoredMessagePriority, message: DecryptedDhtMessage) -> SafResult<()> {
        debug!(
            target: LOG_TARGET,
            "Storing message from peer '{}' ({} bytes)",
            message.source_peer.node_id.short_str(),
            message.body_len(),
        );

        let stored_message = NewStoredMessage::try_construct(message, priority)
            .ok_or_else(|| StoreAndForwardError::InvalidStoreMessage)?;
        self.saf_requester.insert_message(stored_message).await?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        envelope::DhtMessageFlags,
        proto::{dht::JoinMessage, envelope::DhtMessageType},
        test_utils::{
            create_store_and_forward_mock,
            make_dht_inbound_message,
            make_node_identity,
            make_peer_manager,
            service_spy,
        },
    };
    use chrono::Utc;
    use std::time::Duration;
    use tari_comms::{message::MessageExt, wrap_in_envelope_body};
    use tari_crypto::tari_utilities::{hex::Hex, ByteArray};
    use tari_test_utils::async_assert_eventually;

    #[tokio_macros::test_basic]
    async fn cleartext_message_no_origin() {
        let (requester, mock_state) = create_store_and_forward_mock();

        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, requester)
            .layer(spy.to_service::<PipelineError>());

        let inbound_msg =
            make_dht_inbound_message(&make_node_identity(), b"".to_vec(), DhtMessageFlags::empty(), false);
        let msg = DecryptedDhtMessage::succeeded(wrap_in_envelope_body!(Vec::new()), None, inbound_msg);
        service.call(msg).await.unwrap();
        assert!(spy.is_called());
        let messages = mock_state.get_messages().await;
        assert_eq!(messages.len(), 0);
    }

    #[ignore]
    #[tokio_macros::test_basic]
    async fn cleartext_join_message() {
        let (requester, mock_state) = create_store_and_forward_mock();

        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let join_msg_bytes = JoinMessage {
            node_id: node_identity.node_id().to_vec(),
            addresses: vec![],
            peer_features: 0,
        }
        .to_encoded_bytes();

        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, requester)
            .layer(spy.to_service::<PipelineError>());
        let sender_identity = make_node_identity();
        let inbound_msg = make_dht_inbound_message(&sender_identity, b"".to_vec(), DhtMessageFlags::empty(), true);

        let mut msg = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(join_msg_bytes),
            Some(sender_identity.public_key().clone()),
            inbound_msg,
        );
        msg.dht_header.message_type = DhtMessageType::Join;
        service.call(msg).await.unwrap();
        assert!(spy.is_called());

        // Because we dont wait for the message to reach the mock/service before continuing (for efficiency and it's not
        // necessary) we need to wait for the call to happen eventually - it should be almost instant
        async_assert_eventually!(
            mock_state.call_count(),
            expect = 1,
            max_attempts = 10,
            interval = Duration::from_millis(10),
        );
        let messages = mock_state.get_messages().await;
        assert_eq!(messages[0].message_type, DhtMessageType::Join as i32);
    }

    #[tokio_macros::test_basic]
    async fn decryption_succeeded_no_store() {
        let (requester, mock_state) = create_store_and_forward_mock();

        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, requester)
            .layer(spy.to_service::<PipelineError>());

        let msg_node_identity = make_node_identity();
        let inbound_msg = make_dht_inbound_message(
            &msg_node_identity,
            b"This shouldnt be stored".to_vec(),
            DhtMessageFlags::ENCRYPTED,
            true,
        );
        let msg = DecryptedDhtMessage::succeeded(
            wrap_in_envelope_body!(b"secret".to_vec()),
            Some(msg_node_identity.public_key().clone()),
            inbound_msg,
        );
        service.call(msg).await.unwrap();
        assert!(spy.is_called());

        assert_eq!(mock_state.call_count(), 0);
    }

    #[tokio_macros::test_basic]
    async fn decryption_failed_should_store() {
        let (requester, mock_state) = create_store_and_forward_mock();
        let spy = service_spy();
        let peer_manager = make_peer_manager();
        let origin_node_identity = make_node_identity();
        peer_manager.add_peer(origin_node_identity.to_peer()).await.unwrap();
        let node_identity = make_node_identity();
        let mut service = StoreLayer::new(Default::default(), peer_manager, node_identity, requester)
            .layer(spy.to_service::<PipelineError>());

        let mut inbound_msg = make_dht_inbound_message(
            &origin_node_identity,
            b"Will you keep this for me?".to_vec(),
            DhtMessageFlags::ENCRYPTED,
            true,
        );
        inbound_msg.dht_header.destination =
            NodeDestination::PublicKey(Box::new(origin_node_identity.public_key().clone()));
        let msg = DecryptedDhtMessage::failed(inbound_msg.clone());
        service.call(msg).await.unwrap();
        assert_eq!(spy.is_called(), true);

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
}

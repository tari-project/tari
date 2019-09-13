// Copyright 2019. The Tari Project
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
    consts::DHT_FORWARD_NODE_COUNT,
    inbound_message_pipeline::{
        error::InboundMessagePipelineError,
        InboundTopicSubscriptionFactory,
        MessageCache,
        MessageCacheConfig,
    },
    message::{
        Frame,
        FrameSet,
        InboundMessage,
        Message,
        MessageData,
        MessageEnvelopeHeader,
        MessageFlags,
        MessageHeader,
        NodeDestination,
    },
    outbound_message_service::{broadcast_strategy::BroadcastStrategy, OutboundServiceRequester},
    peer_manager::{NodeId, NodeIdentity, Peer, PeerManager},
    pub_sub_channel::{pubsub_channel, TopicPayload, TopicPublisher},
    types::CommsPublicKey,
};
use futures::{channel::mpsc::Receiver, SinkExt, StreamExt};
use log::*;
use serde::{de::DeserializeOwned, Serialize};
use std::{convert::TryFrom, fmt::Debug, sync::Arc};

const LOG_TARGET: &str = "comms::inbound_message_pipeline";

/// This struct hold the subscription factories for the output pub-sub channels for a pipeline. This contained
/// separately to the main Pipeline structure so that external modules can request subscriptions once the pipeline is
/// running.
pub struct InboundMessageSubscriptionFactories<MType> {
    pub handle_message_subscription_factory: Arc<InboundTopicSubscriptionFactory<MType>>,
    // TODO Add the forward message subscription factory
}

/// This enum encodes the possible routes a message can be sent along at the culmination of the Inbound Message Pipeline
pub enum InboundMessageRoute {
    Handle,
    Forward,
}

/// The InboundMessagePipeline contains the logic for processing a raw MessageEnvelope received via a PeerConnection and
/// routing it via one of the InboundMessageRoutes. This pipeline contains the logic for the various validations and
/// check done to decide whether this message should be passed further along the pipeline and, eventually, be sent along
/// out of the output routes which are Publisher-Subscriber channels which other services will subscribe to.
pub struct InboundMessagePipeline<MType>
where MType: Send + Sync + Debug
{
    node_identity: Arc<NodeIdentity>,
    message_sink_receiver: Receiver<FrameSet>,
    message_cache: MessageCache<Frame>,
    peer_manager: Arc<PeerManager>,
    // TODO Remove this and replace it with a forward_message_publisher that will contain the logic for how and when to
    // forward messages
    outbound_service: OutboundServiceRequester,
    handle_message_publisher: TopicPublisher<MType, InboundMessage>,
}

impl<MType> InboundMessagePipeline<MType>
where MType: Eq + Send + Sync + Debug + Serialize + DeserializeOwned + 'static
{
    pub fn new(
        node_identity: Arc<NodeIdentity>,
        message_sink_receiver: Receiver<FrameSet>,
        peer_manager: Arc<PeerManager>,
        outbound_message_service: OutboundServiceRequester,
        pub_sub_buffer_size: usize,
    ) -> (
        InboundMessagePipeline<MType>,
        InboundMessageSubscriptionFactories<MType>,
    )
    {
        let message_cache = MessageCache::<Frame>::new(MessageCacheConfig::default());

        let (handle_message_publisher, handle_message_subscription_factory) = pubsub_channel(pub_sub_buffer_size);

        (
            InboundMessagePipeline {
                node_identity,
                message_sink_receiver,
                message_cache,
                peer_manager,
                outbound_service: outbound_message_service,
                handle_message_publisher,
            },
            InboundMessageSubscriptionFactories {
                handle_message_subscription_factory: Arc::new(handle_message_subscription_factory),
            },
        )
    }

    /// Run the Inbound Message Pipeline on the set `message_sink_receiver` processing each message in that stream. If
    /// an error occurs while processing a message it is logged and the pipeline will move onto the next message. Most
    /// errors represent a reason why a message didn't make it through the pipeline.
    pub async fn run(mut self) -> () {
        info!(target: LOG_TARGET, "Starting Inbound Message Pipeline");
        while let Some(frame_set) = self.message_sink_receiver.next().await {
            if let Err(e) = self.process_message(frame_set).await {
                info!(target: LOG_TARGET, "Inbound Message Pipeline Error: {:?}", e);
            }
        }
        info!(target: LOG_TARGET, "Closing Inbound Message Pipeline");
    }

    /// Process a single received message from its raw serialized form i.e. a FrameSet
    pub async fn process_message(&mut self, frame_set: FrameSet) -> Result<(), InboundMessagePipelineError> {
        let message_data =
            MessageData::try_from(frame_set).map_err(|_| InboundMessagePipelineError::DeserializationError)?;

        self.message_cache_check(&message_data)?;

        let peer = self.check_source_peer(&message_data.source_node_id)?;

        let message_envelope_header = message_data
            .message_envelope
            .deserialize_header()
            .map_err(|_| InboundMessagePipelineError::DeserializationError)?;

        if !message_envelope_header.verify_signatures(message_data.message_envelope.body_frame().clone())? {
            return Err(InboundMessagePipelineError::InvalidMessageSignature);
        }

        match self.resolve_message_route(&message_envelope_header.dest, message_data.forwardable)? {
            InboundMessageRoute::Handle => {
                self.handle_message_route(message_envelope_header, message_data, &peer)
                    .await?
            },
            InboundMessageRoute::Forward => {
                self.forward_message_route(message_envelope_header, message_data)
                    .await?
            },
        }

        Ok(())
    }

    /// This function contains the logic of sending a message along the `Handle` route.
    /// # Arguments:
    /// `message_envelope_header`: The serialized MessageEnvelope Header
    /// `message_data`: The deserialized complete MessageEnvelope and Metadata
    /// `source_peer`: The Peer that this MessageEnvelope was received from.
    async fn handle_message_route(
        &mut self,
        message_envelope_header: MessageEnvelopeHeader,
        message_data: MessageData,
        source_peer: &Peer,
    ) -> Result<(), InboundMessagePipelineError>
    {
        let message: Message;
        if message_envelope_header.flags.contains(MessageFlags::ENCRYPTED) {
            debug!(target: LOG_TARGET, "Attempting to decrypt message");
            match message_data
                .message_envelope
                .deserialize_encrypted_body(&self.node_identity.secret_key, &message_envelope_header.origin_source)
            {
                Ok(decrypted_message_body) => {
                    debug!(target: LOG_TARGET, "Message successfully decrypted");
                    message = decrypted_message_body;
                },
                Err(_) => {
                    // Message might have been for this node if it was able to decrypt it but because it could nto be
                    // decrypted it will be forwarded.
                    if message_envelope_header.dest == NodeDestination::Unknown {
                        debug!(
                            target: LOG_TARGET,
                            "Unable to decrypt message with unknown recipient, forwarding..."
                        );

                        if message_data.forwardable {
                            return self.forward_message_route(message_envelope_header, message_data).await;
                        } else {
                            return Err(InboundMessagePipelineError::InvalidDestination);
                        }
                    } else {
                        warn!(target: LOG_TARGET, "Unable to decrypt message addressed to this node");
                        // Message was for this node but could not be decrypted
                        return Err(InboundMessagePipelineError::DecryptionFailure);
                    }
                },
            }
        } else {
            debug!(target: LOG_TARGET, "Message not encrypted");
            message = message_data
                .message_envelope
                .deserialize_body()
                .map_err(|_| InboundMessagePipelineError::DeserializationError)?
        };

        // Construct InboundMessage to be published via the Handle PubSub channel
        let header: MessageHeader<MType> = message
            .deserialize_header()
            .map_err(|_| InboundMessagePipelineError::DeserializationError)?;

        debug!(target: LOG_TARGET, "Received message type: {:?}", header.message_type);
        let inbound_message = InboundMessage::new(
            source_peer.clone().into(),
            message_envelope_header.origin_source.clone(),
            message,
        );

        debug!(target: LOG_TARGET, "Publishing message type: {:?}", header.message_type);

        self.handle_message_publisher
            .send(TopicPayload::new(header.message_type, inbound_message))
            .await
            .map_err(|_| InboundMessagePipelineError::PublisherError)?;

        Ok(())
    }

    /// This function defines the logic to handle sending messages along the `Message Forward` route.
    async fn forward_message_route(
        &mut self,
        message_envelope_header: MessageEnvelopeHeader,
        message_data: MessageData,
    ) -> Result<(), InboundMessagePipelineError>
    {
        let broadcast_strategy = BroadcastStrategy::forward(
            self.node_identity.identity.node_id.clone(),
            &self.peer_manager,
            message_envelope_header.dest,
            vec![
                message_envelope_header.origin_source,
                message_envelope_header.peer_source,
            ],
        )?;

        debug!(target: LOG_TARGET, "Forwarding message");
        self.outbound_service
            .forward_message(broadcast_strategy, message_data.message_envelope)
            .await?;

        Ok(())
    }

    // Utility Functions that require the Pipeline context resources
    /// Check whether this message body has been received before (within the cache TTL period). If it has then reject
    /// the message, else add it to the cache.
    fn message_cache_check(&mut self, message_data: &MessageData) -> Result<(), InboundMessagePipelineError> {
        if !self.message_cache.contains(message_data.message_envelope.body_frame()) {
            if let Err(_e) = self
                .message_cache
                .insert(message_data.message_envelope.body_frame().clone())
            {
                error!(
                    target: LOG_TARGET,
                    "Duplicate message found in Message Cache AFTER checking the cache for the message"
                );
                return Err(InboundMessagePipelineError::DuplicateMessageDiscarded);
            }
            return Ok(());
        } else {
            return Err(InboundMessagePipelineError::DuplicateMessageDiscarded);
        }
    }

    /// Resolve which route a message will be sent along based on its Destination.
    /// TODO Remove the `forwardable` flag when the DHT service is plumbed into output of this pipeline. It is an
    /// artifact of the previous architecture
    fn resolve_message_route(
        &mut self,
        message_destination: &NodeDestination<CommsPublicKey>,
        forwardable: bool,
    ) -> Result<InboundMessageRoute, InboundMessagePipelineError>
    {
        match message_destination {
            NodeDestination::Unknown => Ok(InboundMessageRoute::Handle),
            NodeDestination::PublicKey(dest_public_key) => {
                if &self.node_identity.identity.public_key == dest_public_key {
                    Ok(InboundMessageRoute::Handle)
                } else if forwardable {
                    Ok(InboundMessageRoute::Forward)
                } else {
                    Err(InboundMessagePipelineError::InvalidDestination)
                }
            },
            NodeDestination::NodeId(dest_node_id) => {
                if self.peer_manager.in_network_region(
                    &dest_node_id,
                    &self.node_identity.identity.node_id,
                    DHT_FORWARD_NODE_COUNT,
                )? {
                    Ok(InboundMessageRoute::Handle)
                } else if forwardable {
                    Ok(InboundMessageRoute::Forward)
                } else {
                    Err(InboundMessagePipelineError::InvalidDestination)
                }
            },
        }
    }

    /// Check whether the the source of the message is known to our Peer Manager, if it is return the peer but otherwise
    /// we discard the message as it should be in our Peer Manager
    fn check_source_peer(&self, source_node_id: &NodeId) -> Result<Peer, InboundMessagePipelineError> {
        match self.peer_manager.find_with_node_id(&source_node_id).ok() {
            Some(peer) => Ok(peer),
            None => {
                warn!(
                    target: LOG_TARGET,
                    "Received unknown node id from peer connection. Discarding message from NodeId={:?}",
                    source_node_id
                );
                Err(InboundMessagePipelineError::CannotFindSourcePeer)
            },
        }
    }
}

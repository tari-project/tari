// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::io;

use tari_swarm::{
    libp2p::{
        gossipsub,
        gossipsub::{IdentTopic, MessageId},
    },
    messaging::{prost::ProstCodec, Codec},
};
use tokio::sync::mpsc;

use crate::identity::PeerId;

pub struct RawGossipMessage {}

#[derive(Debug, Clone)]
pub struct GossipPublisher<T> {
    topic: IdentTopic,
    sender: mpsc::Sender<(IdentTopic, Vec<u8>)>,
    codec: ProstCodec<T>,
}

impl<T: prost::Message + Default> GossipPublisher<T> {
    pub(super) fn new(topic: IdentTopic, sender: mpsc::Sender<(IdentTopic, Vec<u8>)>) -> Self {
        Self {
            topic,
            sender,
            codec: ProstCodec::default(),
        }
    }

    pub async fn publish(&self, msg: T) -> Result<(), GossipError> {
        let len = msg.encoded_len();

        let mut buf = Vec::with_capacity(len);
        self.codec
            .encode_to(&mut buf, msg)
            .await
            .map_err(GossipError::EncodeError)?;
        self.sender
            .send((self.topic.clone(), buf))
            .await
            .map_err(|_| GossipError::CannotPublishNetworkShutdown)?;
        Ok(())
    }
}

#[derive(Debug)]
pub struct GossipSubscription<T> {
    receiver: mpsc::UnboundedReceiver<GossipMessage<gossipsub::Message>>,
    codec: ProstCodec<T>,
}

impl<T: prost::Message + Default> GossipSubscription<T> {
    pub(super) fn new(receiver: mpsc::UnboundedReceiver<GossipMessage<gossipsub::Message>>) -> Self {
        Self {
            receiver,
            codec: ProstCodec::default(),
        }
    }

    pub async fn next_message(&mut self) -> Option<Result<GossipMessage<T>, InboundGossipError>> {
        let raw_msg = self.receiver.recv().await?;

        match self.codec.decode_from(&mut raw_msg.message.data.as_slice()).await {
            Ok((len, msg)) => Some(Ok(GossipMessage {
                message_id: raw_msg.message_id,
                propagation_source: raw_msg.propagation_source,
                origin: raw_msg.origin,
                message_size: len,
                message: msg,
            })),
            Err(err) => Some(Err(InboundGossipError {
                message_id: raw_msg.message_id,
                propagation_source: raw_msg.propagation_source,
                error: GossipError::DecodeError(err),
            })),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GossipMessage<T> {
    /// Message ID. Use this to report back the validation result.
    pub message_id: MessageId,
    /// The peer ID of the node that sent this message
    pub propagation_source: PeerId,
    /// The peer ID of the node that originally published this message, if available
    pub origin: Option<PeerId>,
    /// The size of the message in bytes
    pub message_size: usize,
    /// The decoded message payload
    pub message: T,
}

impl<T> GossipMessage<T> {
    pub fn origin_or_source(&self) -> PeerId {
        self.origin.unwrap_or(self.propagation_source)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GossipError {
    #[error("Cannot publish the message because the network has shutdown")]
    CannotPublishNetworkShutdown,
    #[error("Decode error: {0}")]
    DecodeError(io::Error),
    #[error("Encode error: {0}")]
    EncodeError(io::Error),
}

#[derive(Debug, thiserror::Error)]
#[error("Inbound gossip error for id={message_id},peer_id={propagation_source}: {error}")]
pub struct InboundGossipError {
    pub message_id: MessageId,
    pub propagation_source: PeerId,
    pub error: GossipError,
}

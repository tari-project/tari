// Copyright 2024 The Tari Project
// SPDX-License-Identifier: BSD-3-Clause

use std::io;

use libp2p::{gossipsub, gossipsub::IdentTopic};
use tari_swarm::messaging::{prost::ProstCodec, Codec};
use tokio::sync::mpsc;

use crate::identity::PeerId;

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
    receiver: mpsc::UnboundedReceiver<(PeerId, gossipsub::Message)>,
    codec: ProstCodec<T>,
}

impl<T: prost::Message + Default> GossipSubscription<T> {
    pub(super) fn new(receiver: mpsc::UnboundedReceiver<(PeerId, gossipsub::Message)>) -> Self {
        Self {
            receiver,
            codec: ProstCodec::default(),
        }
    }

    pub async fn next_message(&mut self) -> Option<Result<GossipMessage<T>, io::Error>> {
        let Some((source, raw_msg)) = self.receiver.recv().await else {
            return None;
        };

        match self.codec.decode_from(&mut raw_msg.data.as_slice()).await {
            Ok((len, msg)) => Some(Ok(GossipMessage {
                source,
                origin: raw_msg.source,
                decoded_len: len,
                message: msg,
            })),
            Err(err) => Some(Err(err)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct GossipMessage<T> {
    /// The peer ID of the node that sent this message
    pub source: PeerId,
    /// The peer ID of the node that originally published this message, if available
    pub origin: Option<PeerId>,
    /// The decoded size of the message excl the length bytes
    pub decoded_len: usize,
    /// The decoded message payload
    pub message: T,
}

impl<T> GossipMessage<T> {
    pub fn origin_or_source(&self) -> PeerId {
        self.origin.unwrap_or(self.source)
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

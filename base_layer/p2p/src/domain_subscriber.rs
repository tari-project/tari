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
use crate::{comms_connector::PeerMessage, domain_message::DomainMessage};
use derive_error::Error;
use futures::{executor::block_on, stream::FusedStream, Stream, StreamExt};
use std::{fmt::Debug, sync::Arc};
use tari_utilities::message_format::MessageFormat;

#[derive(Debug, Error, PartialEq)]
pub enum DomainSubscriberError {
    /// Subscription stream ended
    SubscriptionStreamEnded,
    /// Error reading from the stream
    StreamError,
    /// Message deserialization error
    MessageError,
    /// Subscription Reader is not initialized
    SubscriptionReaderNotInitialized,
}

pub struct SyncDomainSubscription<S> {
    subscription: Option<S>,
}
impl<S, MType> SyncDomainSubscription<S>
where S: Stream<Item = Arc<PeerMessage<MType>>> + Unpin + FusedStream
{
    pub fn new(stream: S) -> Self {
        SyncDomainSubscription {
            subscription: Some(stream),
        }
    }

    pub fn receive_messages<T>(&mut self) -> Result<Vec<DomainMessage<T>>, DomainSubscriberError>
    where T: MessageFormat {
        let subscription = self.subscription.take();

        match subscription {
            Some(mut s) => {
                let (stream_messages, stream_complete): (Vec<Arc<PeerMessage<MType>>>, bool) = block_on(async {
                    let mut result = Vec::new();
                    let mut complete = false;
                    loop {
                        futures::select!(
                            item = s.next() => {
                                if let Some(item) = item {
                                    result.push(item)
                                }
                            },
                            complete => {
                                complete = true;
                                break
                            },
                            default => break,
                        );
                    }
                    (result, complete)
                });

                let mut messages = Vec::new();

                for message in stream_messages {
                    let msg = message
                        .deserialize_message::<T>()
                        .map_err(|_| DomainSubscriberError::MessageError)?;
                    messages.push(DomainMessage {
                        source_peer: message.source_peer.clone(),
                        origin_pubkey: message.dht_header.origin_public_key.clone(),
                        inner: msg,
                    });
                }

                if !stream_complete {
                    self.subscription = Some(s);
                }

                return Ok(messages);
            },
            None => return Err(DomainSubscriberError::SubscriptionStreamEnded),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::{executor::block_on, SinkExt};
    use rand::rngs::EntropyRng;
    use serde::{Deserialize, Serialize};
    use tari_comms::{
        connection::NetAddress,
        message::{MessageEnvelopeHeader, MessageFlags, MessageHeader, NodeDestination},
        peer_manager::{NodeIdentity, Peer, PeerFlags},
    };
    use tari_comms_dht::message::{DhtHeader, DhtMessageFlags, DhtMessageType};
    use tari_pubsub::{pubsub_channel, TopicPayload};

    #[test]
    fn topic_pub_sub() {
        let (mut publisher, subscriber_factory) = pubsub_channel(10);

        #[derive(Serialize, Deserialize, Debug, Clone)]
        struct Dummy {
            a: u32,
            b: String,
        }

        let node_identity = NodeIdentity::random(&mut EntropyRng::new(), "127.0.0.1:9000".parse().unwrap()).unwrap();

        let messages = vec![
            ("Topic1".to_string(), Dummy {
                a: 1u32,
                b: "one".to_string(),
            }),
            ("Topic2".to_string(), Dummy {
                a: 2u32,
                b: "two".to_string(),
            }),
            ("Topic1".to_string(), Dummy {
                a: 3u32,
                b: "three".to_string(),
            }),
            ("Topic2".to_string(), Dummy {
                a: 4u32,
                b: "four".to_string(),
            }),
            ("Topic1".to_string(), Dummy {
                a: 5u32,
                b: "five".to_string(),
            }),
            ("Topic2".to_string(), Dummy {
                a: 6u32,
                b: "size".to_string(),
            }),
            ("Topic1".to_string(), Dummy {
                a: 7u32,
                b: "seven".to_string(),
            }),
        ];

        let serialized_messages = messages.iter().map(|m| {
            TopicPayload::new(
                m.0.clone(),
                Arc::new(PeerMessage::new(
                    m.1.to_binary().unwrap(),
                    MessageHeader::new(()).unwrap(),
                    MessageEnvelopeHeader {
                        version: 0,
                        message_public_key: node_identity.identity.public_key.clone(),
                        message_signature: Vec::new(),
                        flags: MessageFlags::empty(),
                    },
                    DhtHeader::new(
                        NodeDestination::Undisclosed,
                        node_identity.identity.public_key.clone(),
                        Vec::new(),
                        DhtMessageType::None,
                        DhtMessageFlags::empty(),
                    ),
                    Peer::new(
                        node_identity.identity.public_key.clone(),
                        node_identity.identity.node_id.clone(),
                        Vec::<NetAddress>::new().into(),
                        PeerFlags::empty(),
                    ),
                )),
            )
        });

        block_on(async {
            for m in serialized_messages {
                publisher.send(m).await.unwrap();
            }
        });
        drop(publisher);

        let mut domain_sub =
            SyncDomainSubscription::new(subscriber_factory.get_subscription("Topic1".to_string()).fuse());

        let messages = domain_sub.receive_messages::<Dummy>().unwrap();

        assert_eq!(
            domain_sub.receive_messages::<Dummy>().unwrap_err(),
            DomainSubscriberError::SubscriptionStreamEnded
        );

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].inner().a, 1);
        assert_eq!(messages[1].inner().a, 3);
        assert_eq!(messages[2].inner().a, 5);
        assert_eq!(messages[3].inner().a, 7);
    }
}

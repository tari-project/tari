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

use super::peer_message::PeerMessage;
use crate::{comms_connector::InboundDomainConnector, tari_message::TariMessageType};
use futures::{channel::mpsc, future, stream::Fuse, Stream, StreamExt};
use log::*;
use std::{fmt::Debug, sync::Arc};
use tokio::{runtime::Handle, sync::broadcast};

const LOG_TARGET: &str = "comms::middleware::pubsub";

/// Alias for a pubsub-type domain connector
pub type PubsubDomainConnector = InboundDomainConnector<mpsc::Sender<Arc<PeerMessage>>>;
pub type SubscriptionFactory = TopicSubscriptionFactory<TariMessageType, Arc<PeerMessage>>;

/// Connects `InboundDomainConnector` to a `tari_pubsub::TopicPublisher` through a buffered broadcast channel
pub fn pubsub_connector(executor: Handle, buf_size: usize) -> (PubsubDomainConnector, SubscriptionFactory) {
    let (publisher, subscription_factory) = pubsub_channel(buf_size);
    let (sender, receiver) = mpsc::channel(buf_size);

    // Spawn a task which forwards messages from the pubsub service to the TopicPublisher
    let forwarder = receiver
        // Map DomainMessage into a TopicPayload
        .filter_map(|msg: Arc<PeerMessage>| {
            let opt = match TariMessageType::from_i32(msg.message_header.message_type) {
                Some(msg_type) => {
                    let message_tag_trace = msg.dht_header.message_tag;
                    let payload = TopicPayload::new(msg_type, msg);
                    trace!(
                        target: LOG_TARGET,
                        "Created topic payload message {:?}, Trace: {}",
                        &payload.topic(), message_tag_trace
                    );
                    Some(payload)
                }
                None => {
                    warn!(target: LOG_TARGET, "Invalid or unrecognised Tari message type '{}'", msg.message_header.message_type);
                    None
                }
            };
            future::ready(opt)
        })
        // Forward TopicPayloads to the publisher
        .for_each(move |item| {
            if let Err(err) = publisher.send(item).map_err(|_| "No subscribers when sending message".to_string())
            {
                warn!(
                    target: LOG_TARGET,
                    "Error forwarding pubsub messages to publisher: {}", err
                );
            }
            future::ready(())
        });

    executor.spawn(forwarder);

    (InboundDomainConnector::new(sender), subscription_factory)
}

/// Create a topic-based pub-sub channel
fn pubsub_channel<T, M>(size: usize) -> (TopicPublisher<T, M>, TopicSubscriptionFactory<T, M>)
where
    T: Clone + Debug + Send + Eq,
    M: Send + Clone,
{
    let (publisher, _) = broadcast::channel(size);
    (publisher.clone(), TopicSubscriptionFactory::new(publisher))
}

/// The container for a message that is passed along the pub-sub channel that contains a Topic to define the type of
/// message and the message itself.
#[derive(Debug, Clone)]
pub struct TopicPayload<T, M> {
    topic: T,
    message: M,
}

impl<T, M> TopicPayload<T, M> {
    pub fn new(topic: T, message: M) -> Self {
        Self { topic, message }
    }

    pub fn topic(&self) -> &T {
        &self.topic
    }

    pub fn message(&self) -> &M {
        &self.message
    }
}

pub type TopicPublisher<T, M> = broadcast::Sender<TopicPayload<T, M>>;

/// This structure is used to create subscriptions to particular topics.
/// Note that subscriptions obtained after messages are published will miss messages.
#[derive(Clone)]
pub struct TopicSubscriptionFactory<T, M> {
    sender: broadcast::Sender<TopicPayload<T, M>>,
}

impl<T, M> TopicSubscriptionFactory<T, M>
where
    T: Clone + Eq + Debug + Send,
    M: Clone + Send,
{
    pub fn new(sender: broadcast::Sender<TopicPayload<T, M>>) -> Self {
        TopicSubscriptionFactory { sender }
    }

    /// Create a subscription stream to a particular topic. The provided label is used to identify which consumer is
    /// lagging.
    pub fn get_subscription(&self, topic: T, label: &'static str) -> impl Stream<Item = M> {
        self.sender
            .subscribe()
            .filter_map({
                let topic = topic.clone();
                move |result| {
                    let opt = match result {
                        Ok(payload) => Some(payload),
                        Err(broadcast::RecvError::Closed) => None,
                        Err(broadcast::RecvError::Lagged(n)) => {
                            warn!(
                                target: LOG_TARGET,
                                "Subscription '{}' for topic '{:?}' lagged. {} message(s) dropped.", label, topic, n
                            );
                            None
                        },
                    };
                    future::ready(opt)
                }
            })
            .filter_map(move |item| {
                let opt = if item.topic() == &topic {
                    Some(item.message)
                } else {
                    None
                };
                future::ready(opt)
            })
    }

    /// Convenience function that returns a fused (`stream::Fuse`) version of the subscription stream.
    pub fn get_subscription_fused(&self, topic: T, label: &'static str) -> Fuse<impl Stream<Item = M>> {
        self.get_subscription(topic, label).fuse()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::stream;
    use std::time::Duration;
    use tari_test_utils::collect_stream;

    #[tokio_macros::test_basic]
    async fn topic_pub_sub() {
        let (publisher, subscriber_factory) = pubsub_channel(10);

        #[derive(Debug, Clone)]
        struct Dummy {
            a: u32,
            b: String,
        }

        let messages = vec![
            TopicPayload::new("Topic1", Dummy {
                a: 1u32,
                b: "one".to_string(),
            }),
            TopicPayload::new("Topic2", Dummy {
                a: 2u32,
                b: "two".to_string(),
            }),
            TopicPayload::new("Topic1", Dummy {
                a: 3u32,
                b: "three".to_string(),
            }),
            TopicPayload::new("Topic2", Dummy {
                a: 4u32,
                b: "four".to_string(),
            }),
            TopicPayload::new("Topic1", Dummy {
                a: 5u32,
                b: "five".to_string(),
            }),
            TopicPayload::new("Topic2", Dummy {
                a: 6u32,
                b: "size".to_string(),
            }),
            TopicPayload::new("Topic1", Dummy {
                a: 7u32,
                b: "seven".to_string(),
            }),
        ];

        let mut sub1 = subscriber_factory.get_subscription("Topic1", "Test").fuse();
        let mut sub2 = subscriber_factory.get_subscription("Topic2", "Test");
        drop(subscriber_factory);

        for m in messages {
            publisher.send(m).unwrap();
        }

        let topic1a = collect_stream!(sub1, take = 4, timeout = Duration::from_secs(10));

        assert_eq!(topic1a[0].a, 1);
        assert_eq!(topic1a[1].a, 3);
        assert_eq!(topic1a[2].a, 5);
        assert_eq!(topic1a[3].a, 7);

        let messages2 = vec![
            TopicPayload::new("Topic1", Dummy {
                a: 11u32,
                b: "one one".to_string(),
            }),
            TopicPayload::new("Topic2", Dummy {
                a: 22u32,
                b: "two two".to_string(),
            }),
            TopicPayload::new("Topic1", Dummy {
                a: 33u32,
                b: "three three".to_string(),
            }),
        ];

        stream::iter(messages2)
            .for_each(|msg| {
                publisher.send(msg).unwrap();
                future::ready(())
            })
            .await;

        let topic1b = collect_stream!(sub1, take = 2, timeout = Duration::from_secs(10));

        assert_eq!(topic1b[0].a, 11);
        assert_eq!(topic1b[1].a, 33);

        let topic2 = collect_stream!(sub2, take = 4, timeout = Duration::from_secs(10));

        assert_eq!(topic2[0].a, 2);
        assert_eq!(topic2[1].a, 4);
        assert_eq!(topic2[2].a, 6);
        assert_eq!(topic2[3].a, 22);
    }
}
